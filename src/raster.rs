//! Tiny CPU rasterizer for Vire Dock: premultiplied BGRA (0xAARRGGBB) buffers, anti-aliased
//! rounded rects via signed distance, bilinear image scaling, and GDI-rendered text masks.
//! Everything here touches only a few hundred thousand pixels, and only when something changes.

use windows::core::{HSTRING, PCWSTR};
use windows::Win32::Foundation::{COLORREF, RECT, SIZE};
use windows::Win32::Graphics::Gdi::*;

pub struct Canvas { pub w: i32, pub h: i32, pub px: Vec<u32> }

#[derive(Clone, Copy)]
pub struct Rgba { pub r: f32, pub g: f32, pub b: f32, pub a: f32 }
pub const fn rgba(r: u8, g: u8, b: u8, a: f32) -> Rgba { Rgba { r: r as f32 / 255.0, g: g as f32 / 255.0, b: b as f32 / 255.0, a } }

impl Canvas {
    pub fn new(w: i32, h: i32) -> Self { Canvas { w, h, px: vec![0; (w.max(1) * h.max(1)) as usize] } }

    /// Source-over of a straight-alpha colour with coverage `a` (0..1).
    #[inline]
    pub fn blend(&mut self, x: i32, y: i32, c: Rgba, a: f32) {
        if x < 0 || y < 0 || x >= self.w || y >= self.h { return; }
        let a = (a * c.a).clamp(0.0, 1.0);
        if a <= 0.002 { return; }
        let p = &mut self.px[(y * self.w + x) as usize];
        let (oa, or, og, ob) = (((*p >> 24) & 255) as f32 / 255.0, ((*p >> 16) & 255) as f32 / 255.0, ((*p >> 8) & 255) as f32 / 255.0, (*p & 255) as f32 / 255.0);
        let k = 1.0 - a;
        let na = a + oa * k;
        let nr = c.r * a + or * k;
        let ng = c.g * a + og * k;
        let nb = c.b * a + ob * k;
        *p = ((na * 255.0).round() as u32) << 24 | ((nr * 255.0).round() as u32) << 16 | ((ng * 255.0).round() as u32) << 8 | (nb * 255.0).round() as u32;
    }

    /// Mirror image of the strip of icons, like a glossy floor: the `len` pixels on the icon side of
    /// `base` are copied (flipped) to start `gap` pixels past it, fading out. `edge`: 0 = bottom
    /// (icons above the line), 1 = left (icons right of it), 2 = right (icons left of it).
    pub fn reflect(&mut self, edge: u8, base: i32, gap: i32, len: i32, alpha0: f32) {
        let fade = |d: i32| alpha0 * (1.0 - d as f32 / len.max(1) as f32).max(0.0).powf(1.7);
        let scale = |p: u32, k: f32| -> [f32; 4] {
            [((p >> 16) & 255) as f32 / 255.0 * k, ((p >> 8) & 255) as f32 / 255.0 * k, (p & 255) as f32 / 255.0 * k, ((p >> 24) & 255) as f32 / 255.0 * k]
        };
        for d in 0..len {
            let k = fade(d);
            if k <= 0.003 { continue; }
            if edge == 0 {
                let (sy, dy) = (base - 1 - d, base + gap + d);
                if sy < 0 || sy >= self.h || dy < 0 || dy >= self.h { continue; }
                for x in 0..self.w { let p = self.px[(sy * self.w + x) as usize]; if p >> 24 != 0 { self.over_pm(x, dy, scale(p, k)); } }
            } else {
                let (sx, dx) = if edge == 1 { (base + d, base - gap - 1 - d) } else { (base - 1 - d, base + gap + d) };
                if sx < 0 || sx >= self.w || dx < 0 || dx >= self.w { continue; }
                for y in 0..self.h { let p = self.px[(y * self.w + sx) as usize]; if p >> 24 != 0 { self.over_pm(dx, y, scale(p, k)); } }
            }
        }
    }

    /// Adds a premultiplied pixel (source-over).
    #[inline]
    fn over_pm(&mut self, x: i32, y: i32, s: [f32; 4]) {
        if x < 0 || y < 0 || x >= self.w || y >= self.h || s[3] <= 0.002 { return; }
        let p = &mut self.px[(y * self.w + x) as usize];
        let k = 1.0 - s[3];
        let ch = |shift: u32, v: f32| ((((*p >> shift) & 255) as f32 / 255.0) * k + v).clamp(0.0, 1.0);
        let (na, nr, ng, nb) = (ch(24, s[3]), ch(16, s[0]), ch(8, s[1]), ch(0, s[2]));
        *p = ((na * 255.0).round() as u32) << 24 | ((nr * 255.0).round() as u32) << 16 | ((ng * 255.0).round() as u32) << 8 | (nb * 255.0).round() as u32;
    }
}

#[inline]
pub fn sd_rrect(px: f32, py: f32, cx: f32, cy: f32, hw: f32, hh: f32, r: f32) -> f32 {
    let qx = (px - cx).abs() - (hw - r);
    let qy = (py - cy).abs() - (hh - r);
    let (ox, oy) = (qx.max(0.0), qy.max(0.0));
    (ox * ox + oy * oy).sqrt() + qx.max(qy).min(0.0) - r
}

#[derive(Clone, Copy, PartialEq, Debug)]
pub struct R { pub x: f32, pub y: f32, pub w: f32, pub h: f32 }
impl R {
    pub fn cx(&self) -> f32 { self.x + self.w / 2.0 }
    pub fn cy(&self) -> f32 { self.y + self.h / 2.0 }
}

/// Filled anti-aliased rounded rect. `shade(fy)` can vary colour from top (0) to bottom (1).
pub fn fill_rrect(c: &mut Canvas, r: R, rad: f32, col: impl Fn(f32) -> Rgba) {
    let rad = rad.min(r.w / 2.0).min(r.h / 2.0).max(0.0);
    let (x0, y0, x1, y1) = (r.x.floor() as i32 - 1, r.y.floor() as i32 - 1, (r.x + r.w).ceil() as i32 + 1, (r.y + r.h).ceil() as i32 + 1);
    for y in y0.max(0)..y1.min(c.h) {
        let fy = ((y as f32 + 0.5 - r.y) / r.h).clamp(0.0, 1.0);
        let k = col(fy);
        for x in x0.max(0)..x1.min(c.w) {
            let d = sd_rrect(x as f32 + 0.5, y as f32 + 0.5, r.cx(), r.cy(), r.w / 2.0, r.h / 2.0, rad);
            if d < 0.5 { c.blend(x, y, k, (0.5 - d).min(1.0)); }
        }
    }
}

/// ~1 px anti-aliased outline just inside a rounded rect; alpha varies with `fy`.
pub fn stroke_rrect(c: &mut Canvas, r: R, rad: f32, width: f32, col: impl Fn(f32) -> Rgba) {
    let rad = rad.min(r.w / 2.0).min(r.h / 2.0).max(0.0);
    let (x0, y0, x1, y1) = (r.x.floor() as i32 - 1, r.y.floor() as i32 - 1, (r.x + r.w).ceil() as i32 + 1, (r.y + r.h).ceil() as i32 + 1);
    for y in y0.max(0)..y1.min(c.h) {
        let fy = ((y as f32 + 0.5 - r.y) / r.h).clamp(0.0, 1.0);
        let k = col(fy);
        for x in x0.max(0)..x1.min(c.w) {
            let d = sd_rrect(x as f32 + 0.5, y as f32 + 0.5, r.cx(), r.cy(), r.w / 2.0, r.h / 2.0, rad);
            let band = (width / 2.0 + 0.5 - (d + width / 2.0).abs()).clamp(0.0, 1.0);
            if band > 0.0 { c.blend(x, y, k, band); }
        }
    }
}

/// Premultiplied BGRA image (e.g. an app icon).
pub struct Img { pub w: i32, pub h: i32, pub px: Vec<u32> }

/// Bilinear-scaled draw of `img` into `dst` with extra `opacity`.
pub fn draw_img(c: &mut Canvas, img: &Img, dst: R, opacity: f32) {
    if dst.w < 1.0 || dst.h < 1.0 { return; }
    let (x0, y0) = (dst.x.floor() as i32, dst.y.floor() as i32);
    let (x1, y1) = ((dst.x + dst.w).ceil() as i32, (dst.y + dst.h).ceil() as i32);
    let sx = img.w as f32 / dst.w;
    let sy = img.h as f32 / dst.h;
    let get = |x: i32, y: i32| -> [f32; 4] {
        let p = img.px[(y.clamp(0, img.h - 1) * img.w + x.clamp(0, img.w - 1)) as usize];
        [((p >> 16) & 255) as f32 / 255.0, ((p >> 8) & 255) as f32 / 255.0, (p & 255) as f32 / 255.0, ((p >> 24) & 255) as f32 / 255.0]
    };
    for y in y0.max(0)..y1.min(c.h) {
        let v = ((y as f32 + 0.5 - dst.y) * sy - 0.5).max(0.0);
        let (vy, fy) = (v.floor() as i32, v.fract());
        for x in x0.max(0)..x1.min(c.w) {
            // Edge coverage so fractional positions stay smooth.
            let cov = ((x as f32 + 1.0).min(dst.x + dst.w) - (x as f32).max(dst.x)).clamp(0.0, 1.0)
                * ((y as f32 + 1.0).min(dst.y + dst.h) - (y as f32).max(dst.y)).clamp(0.0, 1.0);
            if cov <= 0.0 { continue; }
            let u = ((x as f32 + 0.5 - dst.x) * sx - 0.5).max(0.0);
            let (ux, fx) = (u.floor() as i32, u.fract());
            let (a, b, cc, d) = (get(ux, vy), get(ux + 1, vy), get(ux, vy + 1), get(ux + 1, vy + 1));
            let mut s = [0.0f32; 4];
            for i in 0..4 {
                s[i] = ((a[i] * (1.0 - fx) + b[i] * fx) * (1.0 - fy) + (cc[i] * (1.0 - fx) + d[i] * fx) * fy) * opacity * cov;
            }
            c.over_pm(x, y, s);
        }
    }
}

/// Anti-aliased text rendered by GDI into an alpha mask (white-on-black → coverage).
pub struct Mask { pub w: i32, pub h: i32, pub a: Vec<u8> }

pub fn text_mask(text: &str, px_size: i32, weight: i32) -> Mask { text_mask_ex(text, px_size, weight, "Segoe UI", 0) }

/// Same with any installed font face and extra letter spacing (pixels).
pub fn text_mask_ex(text: &str, px_size: i32, weight: i32, face: &str, spacing: i32) -> Mask {
    // DrawTextW with an empty buffer can fault inside USER32: nothing to draw, so don't call it.
    if text.is_empty() { let h = px_size.max(1); return Mask { w: 1, h, a: vec![0; h as usize] }; }
    unsafe {
        let dc = CreateCompatibleDC(None);
        let font = CreateFontW(-px_size, 0, 0, 0, weight, 0, 0, 0, DEFAULT_CHARSET.0 as u32, OUT_TT_PRECIS.0 as u32,
            CLIP_DEFAULT_PRECIS.0 as u32, ANTIALIASED_QUALITY.0 as u32, (DEFAULT_PITCH.0 | FF_SWISS.0) as u32, &HSTRING::from(face));
        let old_font = SelectObject(dc, font);
        if spacing != 0 { SetTextCharacterExtra(dc, spacing); }
        let wide: Vec<u16> = text.encode_utf16().collect();
        let mut sz = SIZE::default();
        let _ = GetTextExtentPoint32W(dc, &wide, &mut sz);
        let (w, h) = (sz.cx.max(1) + 2, sz.cy.max(1));
        let mut bi = BITMAPINFO::default();
        bi.bmiHeader = BITMAPINFOHEADER { biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32, biWidth: w, biHeight: -h, biPlanes: 1, biBitCount: 32, ..Default::default() };
        let mut bits: *mut std::ffi::c_void = std::ptr::null_mut();
        let mut out = Mask { w, h, a: vec![0; (w * h) as usize] };
        if let Ok(dib) = CreateDIBSection(dc, &bi, DIB_RGB_COLORS, &mut bits, None, 0) {
            let old_bmp = SelectObject(dc, dib);
            SetBkMode(dc, TRANSPARENT);
            SetTextColor(dc, COLORREF(0x00FF_FFFF));
            let mut rc = RECT { left: 1, top: 0, right: w, bottom: h };
            let mut wide2 = wide.clone();
            DrawTextW(dc, &mut wide2, &mut rc, DT_SINGLELINE | DT_NOPREFIX);
            let _ = GdiFlush();
            let src = std::slice::from_raw_parts(bits as *const u32, (w * h) as usize);
            for (i, p) in src.iter().enumerate() {
                let (r, g, b) = ((p >> 16) & 255, (p >> 8) & 255, p & 255);
                out.a[i] = r.max(g).max(b) as u8;
            }
            SelectObject(dc, old_bmp);
            let _ = DeleteObject(dib);
        }
        SelectObject(dc, old_font);
        let _ = DeleteObject(font);
        let _ = DeleteDC(dc);
        let _ = PCWSTR::null();
        out
    }
}

pub fn draw_mask(c: &mut Canvas, m: &Mask, x: f32, y: f32, col: Rgba) {
    let (ox, oy) = (x.round() as i32, y.round() as i32);
    for j in 0..m.h {
        for i in 0..m.w {
            let a = m.a[(j * m.w + i) as usize];
            if a > 0 { c.blend(ox + i, oy + j, col, a as f32 / 255.0); }
        }
    }
}

/// Soft copy of a mask (two box-blur passes), grown by `r` px on every side — glow / shadow.
pub fn blur_mask(m: &Mask, r: i32) -> Mask {
    let r = r.max(1);
    let (w, h) = (m.w + 2 * r, m.h + 2 * r);
    let mut a: Vec<f32> = vec![0.0; (w * h) as usize];
    for y in 0..m.h { for x in 0..m.w { a[((y + r) * w + x + r) as usize] = m.a[(y * m.w + x) as usize] as f32; } }
    let mut tmp = vec![0.0f32; a.len()];
    for _ in 0..2 {
        // horizontal
        for y in 0..h {
            let row = (y * w) as usize;
            let mut acc = 0.0f32;
            let k = (2 * r + 1) as f32;
            for x in 0..(w.min(r)) { acc += a[row + x as usize]; }
            for x in 0..w {
                if x + r < w { acc += a[row + (x + r) as usize]; }
                if x - r - 1 >= 0 { acc -= a[row + (x - r - 1) as usize]; }
                tmp[row + x as usize] = acc / k;
            }
        }
        // vertical
        for x in 0..w {
            let mut acc = 0.0f32;
            let k = (2 * r + 1) as f32;
            for y in 0..(h.min(r)) { acc += tmp[(y * w + x) as usize]; }
            for y in 0..h {
                if y + r < h { acc += tmp[((y + r) * w + x) as usize]; }
                if y - r - 1 >= 0 { acc -= tmp[((y - r - 1) * w + x) as usize]; }
                a[(y * w + x) as usize] = acc / k;
            }
        }
    }
    Mask { w, h, a: a.into_iter().map(|v| v.round().clamp(0.0, 255.0) as u8).collect() }
}

/// A mask painted with a vertical colour gradient (top → bottom).
pub fn draw_mask_grad(c: &mut Canvas, m: &Mask, x: f32, y: f32, top: Rgba, bottom: Rgba) {
    let (ox, oy) = (x.round() as i32, y.round() as i32);
    for j in 0..m.h {
        let t = j as f32 / (m.h - 1).max(1) as f32;
        let col = Rgba { r: top.r + (bottom.r - top.r) * t, g: top.g + (bottom.g - top.g) * t, b: top.b + (bottom.b - top.b) * t, a: top.a + (bottom.a - top.a) * t };
        for i in 0..m.w {
            let a = m.a[(j * m.w + i) as usize];
            if a > 0 { c.blend(ox + i, oy + j, col, a as f32 / 255.0); }
        }
    }
}

/// Anti-aliased line with round caps (signed distance to the segment).
pub fn line_aa(c: &mut Canvas, a: (f32, f32), b: (f32, f32), width: f32, col: Rgba) {
    let hw = width / 2.0;
    let (x0, y0) = ((a.0.min(b.0) - hw - 1.0).floor() as i32, (a.1.min(b.1) - hw - 1.0).floor() as i32);
    let (x1, y1) = ((a.0.max(b.0) + hw + 1.0).ceil() as i32, (a.1.max(b.1) + hw + 1.0).ceil() as i32);
    let (dx, dy) = (b.0 - a.0, b.1 - a.1);
    let len2 = (dx * dx + dy * dy).max(1e-6);
    for y in y0.max(0)..=y1.min(c.h - 1) {
        for x in x0.max(0)..=x1.min(c.w - 1) {
            let (px, py) = (x as f32 + 0.5 - a.0, y as f32 + 0.5 - a.1);
            let t = ((px * dx + py * dy) / len2).clamp(0.0, 1.0);
            let d = ((px - dx * t).powi(2) + (py - dy * t).powi(2)).sqrt() - hw;
            if d < 0.5 { c.blend(x, y, col, (0.5 - d).min(1.0)); }
        }
    }
}

/// Soft radial glow (alpha falls off smoothly to the rim) — used at low resolution by the ambient look.
pub fn glow_disc(c: &mut Canvas, cx: f32, cy: f32, rx: f32, ry: f32, col: Rgba, strength: f32) {
    let (x0, y0, x1, y1) = ((cx - rx).floor() as i32, (cy - ry).floor() as i32, (cx + rx).ceil() as i32, (cy + ry).ceil() as i32);
    for y in y0.max(0)..y1.min(c.h) {
        let ny = (y as f32 + 0.5 - cy) / ry;
        for x in x0.max(0)..x1.min(c.w) {
            let nx = (x as f32 + 0.5 - cx) / rx;
            let d2 = nx * nx + ny * ny;
            if d2 < 1.0 { let k = (1.0 - d2).powf(1.6) * strength; c.blend(x, y, col, k); }
        }
    }
}