//! The desktop music widget: a small floating card that sits on the desktop (above the wallpaper
//! and icons, under every normal window) and shows what is playing with basic controls. It is a
//! layered window painted with the CPU rasterizer — no GPU surface, nothing running while idle
//! except a 4 Hz check — and tinted from the current wallpaper's colours.

use crate::model::AppState;
use crate::nowplaying::{LocalSource, MediaSource, NowPlaying, Transport};
use crate::raster::{fill_rrect, stroke_rrect, text_mask, draw_mask, draw_img, rgba, Canvas, Img, Rgba, R};
use crate::vdock::Surface;
use std::cell::RefCell;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use windows::core::{w, PCWSTR};
use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, POINT, RECT, WPARAM};
use windows::Win32::Graphics::Gdi::ScreenToClient;
use windows::Win32::System::Com::{CoInitializeEx, COINIT_APARTMENTTHREADED};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::Input::KeyboardAndMouse::{ReleaseCapture, SetCapture, TrackMouseEvent, TME_LEAVE, TRACKMOUSEEVENT};
use windows::Win32::UI::WindowsAndMessaging::*;

const T_TICK: usize = 1;
const BASE_W: f32 = 360.0;
const BASE_H: f32 = 124.0;

#[derive(Clone, Copy, PartialEq)]
struct Look { scale: f32, opacity: f32 }

#[derive(Clone, Copy)]
enum Drag { Move(i32, i32), Seek, Vol }

#[derive(PartialEq)]
struct DrawKey { np: Option<NowPlaying>, look: Look, accent: (u8, u8, u8), seek: Option<u32>, vol: Option<u32>, sec: u64, hover: u8 }

struct Wd {
    hwnd: HWND,
    surf: Surface,
    sources: Vec<Box<dyn MediaSource>>,
    /// Which source produced the current `np` (controls go back to it).
    active: usize,
    state: Arc<Mutex<AppState>>,
    np: Option<NowPlaying>,
    look: Look,
    accent: (u8, u8, u8),
    accent_for: Option<PathBuf>,
    art: Option<(PathBuf, Img)>,
    drawn: Option<DrawKey>,
    shown: bool,
    drag: Option<Drag>,
    seek_preview: Option<f32>,
    vol_preview: Option<f32>,
    placed: Option<((i32, i32), u8)>,
    last_z: std::time::Instant,
    vol_before_mute: f32,
    /// What the mouse is over: 0 nothing, 1 progress bar, 2 volume zone (the slider only shows then).
    hover: u8,
    /// The experimental desktop-lyrics window.
    lyr: Option<Lyr>,
}

struct Lyr {
    hwnd: HWND,
    surf: Surface,
    path: Option<PathBuf>,
    lyrics: crate::lyrics::Lyrics,
    drawn: Option<(String, String)>,
    visible: bool,
}

/// Which control zone (1 = progress bar, 2 = volume) the point is in.
fn zone_at(g: &Geo, x: f32, y: f32) -> u8 {
    let s = g.s;
    if x >= g.spk.0 - 14.0 * s && x <= g.vol.x + g.vol.w + 10.0 * s && y >= 50.0 * s && y <= 82.0 * s { return 2; }
    if x >= g.bar.x - 6.0 && x <= g.bar.x + g.bar.w + 6.0 && (y - g.bar.y).abs() < 11.0 * s { return 1; }
    0
}

thread_local! { static W: RefCell<Option<Wd>> = const { RefCell::new(None) }; }

fn with_w<T>(f: impl FnOnce(&mut Wd) -> T) -> Option<T> {
    W.with(|w| w.try_borrow_mut().ok().and_then(|mut b| b.as_mut().map(f)))
}

pub fn start(state: Arc<Mutex<AppState>>, engine: crate::engine::Engine) {
    let _ = std::thread::Builder::new().name("vire-widget".into())
        .spawn(move || unsafe { thread_main(state, engine) });
}

// ------------------------------------------------------------------ geometry

struct Geo { w: i32, h: i32, s: f32, prev: (f32, f32), play: (f32, f32), next: (f32, f32), bar: R, vol: R, spk: (f32, f32), art: R }

fn geo(scale: f32) -> Geo {
    let s = scale;
    Geo {
        w: (BASE_W * s).round() as i32, h: (BASE_H * s).round() as i32, s,
        art: R { x: 14.0 * s, y: 14.0 * s, w: 92.0 * s, h: 92.0 * s },
        prev: (150.0 * s, 66.0 * s), play: (192.0 * s, 66.0 * s), next: (234.0 * s, 66.0 * s),
        spk: (276.0 * s, 66.0 * s),
        vol: R { x: 290.0 * s, y: 62.0 * s, w: 54.0 * s, h: 8.0 * s },
        bar: R { x: 120.0 * s, y: 96.0 * s, w: 226.0 * s, h: 4.0 * s },
    }
}

fn near(p: (f32, f32), x: f32, y: f32, r: f32) -> bool { (p.0 - x).powi(2) + (p.1 - y).powi(2) <= r * r }

// ------------------------------------------------------------------ drawing

fn fill_tri(c: &mut Canvas, p: [(f32, f32); 3], col: Rgba) {
    let (minx, maxx) = (p.iter().map(|q| q.0).fold(f32::MAX, f32::min).floor() as i32, p.iter().map(|q| q.0).fold(f32::MIN, f32::max).ceil() as i32);
    let (miny, maxy) = (p.iter().map(|q| q.1).fold(f32::MAX, f32::min).floor() as i32, p.iter().map(|q| q.1).fold(f32::MIN, f32::max).ceil() as i32);
    let edge = |a: (f32, f32), b: (f32, f32), x: f32, y: f32| (b.0 - a.0) * (y - a.1) - (b.1 - a.1) * (x - a.0);
    let area = edge(p[0], p[1], p[2].0, p[2].1);
    if area.abs() < 1e-3 { return; }
    for y in miny..=maxy {
        for x in minx..=maxx {
            let mut hit = 0;
            for sy in 0..3 { for sx in 0..3 {
                let (fx, fy) = (x as f32 + (sx as f32 + 0.5) / 3.0, y as f32 + (sy as f32 + 0.5) / 3.0);
                let (e0, e1, e2) = (edge(p[0], p[1], fx, fy), edge(p[1], p[2], fx, fy), edge(p[2], p[0], fx, fy));
                if (area > 0.0 && e0 >= 0.0 && e1 >= 0.0 && e2 >= 0.0) || (area < 0.0 && e0 <= 0.0 && e1 <= 0.0 && e2 <= 0.0) { hit += 1; }
            } }
            if hit > 0 { c.blend(x, y, col, hit as f32 / 9.0); }
        }
    }
}

fn disc(c: &mut Canvas, cx: f32, cy: f32, r: f32, col: Rgba) { fill_rrect(c, R { x: cx - r, y: cy - r, w: 2.0 * r, h: 2.0 * r }, r, |_| col); }

fn fmt_time(s: f64) -> String { let s = s.max(0.0) as u64; format!("{}:{:02}", s / 60, s % 60) }

/// Text clipped with "…" to `max_w` pixels.
fn fitted(text: &str, px: i32, weight: i32, max_w: f32) -> crate::raster::Mask {
    let m = text_mask(text, px, weight);
    if m.w as f32 <= max_w || text.chars().count() < 2 { return m; }
    let chars: Vec<char> = text.chars().collect();
    let (mut lo, mut hi) = (1usize, chars.len());
    while lo < hi {
        let mid = (lo + hi + 1) / 2;
        let s: String = chars[..mid].iter().collect::<String>() + "…";
        if text_mask(&s, px, weight).w as f32 <= max_w { lo = mid; } else { hi = mid - 1; }
    }
    text_mask(&(chars[..lo].iter().collect::<String>().trim_end().to_string() + "…"), px, weight)
}

impl Wd {
    fn render(&self, g: &Geo) -> Canvas {
        let s = g.s;
        let mut c = Canvas::new(g.w, g.h);
        let (ar, ag, ab) = self.accent;
        let card = R { x: 0.0, y: 0.0, w: g.w as f32, h: g.h as f32 };
        let op = self.look.opacity;
        // Glass card tinted with the wallpaper's colour.
        let (br, bg, bb) = (14.0 + ar as f32 * 0.16, 16.0 + ag as f32 * 0.16, 24.0 + ab as f32 * 0.16);
        fill_rrect(&mut c, card, 24.0 * s, |fy| rgba(br as u8, bg as u8, bb as u8, op * (1.0 - 0.12 * fy)));
        fill_rrect(&mut c, R { x: 0.0, y: 0.0, w: card.w, h: card.h * 0.5 }, 24.0 * s, |fy| rgba(255, 255, 255, 0.07 * (1.0 - fy) * op));
        stroke_rrect(&mut c, card, 24.0 * s, 1.2, |fy| rgba(255, 255, 255, 0.30 - 0.18 * fy));

        let np = self.np.clone().unwrap_or_default();
        // A bright version of the wallpaper colour (same hue, strongest channel lifted to ~215).
        let top = ar.max(ag).max(ab).max(1) as f32;
        let k = 215.0 / top;
        let (vr, vg, vb) = ((ar as f32 * k).min(255.0) as u8, (ag as f32 * k).min(255.0) as u8, (ab as f32 * k).min(255.0) as u8);
        let vivid = rgba(vr, vg, vb, 1.0);

        // Cover.
        let art_r = g.art;
        let mut art_c = Canvas::new(art_r.w as i32, art_r.h as i32);
        match &self.art {
            Some((_, img)) => draw_img(&mut art_c, img, R { x: 0.0, y: 0.0, w: art_r.w, h: art_r.h }, 1.0),
            None => {
                fill_rrect(&mut art_c, R { x: 0.0, y: 0.0, w: art_r.w, h: art_r.h }, 0.0, |fy| rgba((vr as f32 * (0.85 - 0.45 * fy)) as u8, (vg as f32 * (0.85 - 0.45 * fy)) as u8, (vb as f32 * (0.85 - 0.45 * fy)) as u8, 1.0));
                let note = text_mask("♪", (44.0 * s) as i32, 400);
                draw_mask(&mut art_c, &note, art_r.w / 2.0 - note.w as f32 / 2.0, art_r.h / 2.0 - note.h as f32 / 2.0, rgba(255, 255, 255, 0.85));
            }
        }
        // Round the cover's corners (pixels are premultiplied, so scale every channel).
        let rad = 16.0 * s;
        for y in 0..art_c.h { for x in 0..art_c.w {
            let d = crate::raster::sd_rrect(x as f32 + 0.5, y as f32 + 0.5, art_c.w as f32 / 2.0, art_c.h as f32 / 2.0, art_c.w as f32 / 2.0, art_c.h as f32 / 2.0, rad);
            let k = (0.5 - d).clamp(0.0, 1.0);
            if k < 1.0 {
                let p = &mut art_c.px[(y * art_c.w + x) as usize];
                let ch = |sh: u32| (((*p >> sh) & 255) as f32 * k) as u32;
                *p = ch(24) << 24 | ch(16) << 16 | ch(8) << 8 | ch(0);
            }
        } }
        draw_img(&mut c, &Img { w: art_c.w, h: art_c.h, px: art_c.px }, art_r, 1.0);

        // Title + artist.
        let tx = 120.0 * s;
        let max_w = card.w - tx - 16.0 * s;
        let title = fitted(if np.title.is_empty() { "Chưa phát bài nào" } else { &np.title }, (17.0 * s) as i32, 700, max_w);
        draw_mask(&mut c, &title, tx, 14.0 * s, rgba(255, 255, 255, 0.97));
        if !np.artist.is_empty() {
            let artist = fitted(&np.artist, (12.5 * s) as i32, 400, max_w);
            draw_mask(&mut c, &artist, tx, 38.0 * s, rgba(255, 255, 255, 0.62));
        }

        // Transport.
        let white = rgba(255, 255, 255, 0.92);
        let (pcx, pcy) = g.prev;
        let k = 7.0 * s;
        fill_tri(&mut c, [(pcx + k * 0.6, pcy - k), (pcx + k * 0.6, pcy + k), (pcx - k * 0.9, pcy)], white);
        fill_rrect(&mut c, R { x: pcx - k * 1.15, y: pcy - k, w: 2.0 * s, h: 2.0 * k }, 1.0, |_| white);
        let (ncx, ncy) = g.next;
        fill_tri(&mut c, [(ncx - k * 0.6, ncy - k), (ncx - k * 0.6, ncy + k), (ncx + k * 0.9, ncy)], white);
        fill_rrect(&mut c, R { x: ncx + k * 1.15 - 2.0 * s, y: ncy - k, w: 2.0 * s, h: 2.0 * k }, 1.0, |_| white);
        let (cx, cy) = g.play;
        disc(&mut c, cx, cy, 19.0 * s, rgba(255, 255, 255, 0.96));
        let dark = rgba(14, 16, 24, 1.0);
        if np.playing {
            fill_rrect(&mut c, R { x: cx - 6.0 * s, y: cy - 7.5 * s, w: 4.2 * s, h: 15.0 * s }, 1.2 * s, |_| dark);
            fill_rrect(&mut c, R { x: cx + 1.8 * s, y: cy - 7.5 * s, w: 4.2 * s, h: 15.0 * s }, 1.2 * s, |_| dark);
        } else {
            fill_tri(&mut c, [(cx - 5.0 * s, cy - 8.5 * s), (cx - 5.0 * s, cy + 8.5 * s), (cx + 8.5 * s, cy)], dark);
        }

        // Volume (only when the source has one).
        if let Some(v) = np.volume {
            let v = self.vol_preview.unwrap_or(v).clamp(0.0, 1.0);
            let (sx, sy) = g.spk;
            let dim = rgba(255, 255, 255, 0.78);
            fill_rrect(&mut c, R { x: sx - 7.0 * s, y: sy - 2.5 * s, w: 4.0 * s, h: 5.0 * s }, 0.5, |_| dim);
            fill_tri(&mut c, [(sx - 3.5 * s, sy - 2.5 * s), (sx + 1.5 * s, sy - 6.5 * s), (sx + 1.5 * s, sy + 6.5 * s)], dim);
            fill_tri(&mut c, [(sx - 3.5 * s, sy + 2.5 * s), (sx - 3.5 * s, sy - 2.5 * s), (sx + 1.5 * s, sy)], dim);
            if v <= 0.001 { fill_rrect(&mut c, R { x: sx + 4.0 * s, y: sy - 0.8 * s, w: 7.0 * s, h: 1.6 * s }, 0.8, |_| dim); }
            // The slider itself only shows while the mouse is over the volume zone (or dragging it).
            if self.hover == 2 || self.vol_preview.is_some() {
                let t = R { x: g.vol.x, y: g.vol.y + g.vol.h / 2.0 - 1.8 * s, w: g.vol.w, h: 3.6 * s };
                fill_rrect(&mut c, t, 2.0 * s, |_| rgba(255, 255, 255, 0.22));
                if v > 0.0 { fill_rrect(&mut c, R { w: t.w * v, ..t }, 2.0 * s, |_| vivid); }
                disc(&mut c, t.x + t.w * v, t.y + t.h / 2.0, 4.5 * s, white);
            }
        }

        // Progress: slim until you point at it, then it thickens and shows the handle.
        let frac = self.seek_preview.map(|f| f as f64).unwrap_or(if np.dur > 0.0 { np.pos / np.dur } else { 0.0 }).clamp(0.0, 1.0) as f32;
        let active = self.hover == 1 || self.seek_preview.is_some();
        let bar = if active { R { y: g.bar.y - 1.0 * s, h: g.bar.h + 2.0 * s, ..g.bar } } else { g.bar };
        fill_rrect(&mut c, bar, bar.h / 2.0, |_| rgba(255, 255, 255, if active { 0.28 } else { 0.20 }));
        if frac > 0.0 { fill_rrect(&mut c, R { w: bar.w * frac, ..bar }, bar.h / 2.0, |_| vivid); }
        if active { disc(&mut c, bar.x + bar.w * frac, bar.y + bar.h / 2.0, 6.0 * s, white); }
        let shown_pos = self.seek_preview.map(|f| f as f64 * np.dur).unwrap_or(np.pos);
        let left = text_mask(&fmt_time(shown_pos), (10.5 * s) as i32, 400);
        draw_mask(&mut c, &left, g.bar.x, g.bar.y + 8.0 * s, rgba(255, 255, 255, 0.55));
        let right = text_mask(&fmt_time(np.dur), (10.5 * s) as i32, 400);
        draw_mask(&mut c, &right, g.bar.x + g.bar.w - right.w as f32 + 2.0, g.bar.y + 8.0 * s, rgba(255, 255, 255, 0.55));
        c
    }

    /// Polls the sources and repaints only when something visible changed.
    unsafe fn tick(&mut self) {
        let (enabled, pos, size, opacity, always, wall, lyrics_on) = {
            let st = self.state.lock().unwrap();
            let wall = st.selected().and_then(|i| i.thumb.clone());
            (st.settings.w_enabled, st.settings.w_pos, st.settings.w_size, st.settings.w_opacity, st.settings.w_always, wall, st.settings.lyrics_on)
        };
        // First source with something playing wins; the rest are only asked when it has nothing.
        self.np = None;
        for (i, src) in self.sources.iter_mut().enumerate() {
            if let Some(np) = src.now_playing() { self.np = Some(np); self.active = i; break; }
        }
        self.lyrics_tick(lyrics_on);
        let want_show = enabled && (self.np.is_some() || always);
        if !want_show {
            if self.shown { let _ = ShowWindow(self.hwnd, SW_HIDE); self.shown = false; self.drawn = None; }
            return;
        }
        let look = Look { scale: [0.82, 1.0, 1.28][size.min(2) as usize], opacity: opacity.clamp(0.25, 1.0) };
        self.look = look;
        if self.accent_for != wall {
            self.accent_for = wall.clone();
            self.accent = wall.as_ref().and_then(|p| average_colour(p)).unwrap_or((150, 130, 255));
        }
        // Cover picture of the current song.
        let art_path = self.np.as_ref().and_then(|n| n.art.clone());
        if self.art.as_ref().map(|a| &a.0) != art_path.as_ref() {
            self.art = art_path.and_then(|p| std::fs::read(&p).ok().and_then(|b| crate::img::decode(&b)).map(|im| {
                // straight RGBA → premultiplied BGRA
                let px = im.px.chunks_exact(4).map(|q| { let a = q[3] as u32; let m = |v: u8| v as u32 * a / 255; a << 24 | m(q[0]) << 16 | m(q[1]) << 8 | m(q[2]) }).collect();
                (self.np.as_ref().and_then(|n| n.art.clone()).unwrap_or_default(), Img { w: im.w as i32, h: im.h as i32, px })
            }));
        }
        let g = geo(look.scale);
        // Position / size.
        if self.placed != Some((pos.unwrap_or((i32::MIN, i32::MIN)), size)) && self.drag.is_none() {
            let (x, y) = pos.unwrap_or_else(|| default_pos(&g));
            let (x, y) = clamp_to_screen(x, y, &g);
            let _ = SetWindowPos(self.hwnd, None, x, y, g.w, g.h, SWP_NOZORDER | SWP_NOACTIVATE);
            self.placed = Some((pos.unwrap_or((i32::MIN, i32::MIN)), size));
            self.drawn = None;
        }
        let key = DrawKey {
            np: self.np.clone().map(|mut n| { n.pos = n.pos.floor(); n }),
            look, accent: self.accent, seek: self.seek_preview.map(f32::to_bits), vol: self.vol_preview.map(f32::to_bits),
            sec: self.np.as_ref().map_or(0, |n| n.pos as u64),
            hover: self.hover,
        };
        let first = !self.shown;
        if first || self.drawn.as_ref() != Some(&key) {
            let c = self.render(&g);
            let mut r = RECT::default();
            let _ = GetWindowRect(self.hwnd, &mut r);
            self.surf.present(self.hwnd, &c, r.left, r.top);
            self.drawn = Some(key);
        }
        if first {
            let _ = ShowWindow(self.hwnd, SW_SHOWNOACTIVATE);
            self.shown = true;
            place_above_desktop(self.hwnd);
        } else if self.last_z.elapsed().as_secs() >= 3 {
            self.last_z = std::time::Instant::now();
            place_above_desktop(self.hwnd);
        }
    }

    /// EXPERIMENTAL lyrics: one big line (and the next, dimmer) near the bottom of the desktop,
    /// click-through, under every normal window. Needs `song.lrc` next to the song.
    unsafe fn lyrics_tick(&mut self, on: bool) {
        let Some(l) = self.lyr.as_mut() else { return };
        let np = if on { self.np.clone() } else { None };
        let Some(np) = np else {
            if l.visible { let _ = ShowWindow(l.hwnd, SW_HIDE); l.visible = false; l.drawn = None; }
            return;
        };
        if np.path != l.path {
            l.path = np.path.clone();
            l.lyrics = np.path.as_ref().map(|p| crate::lyrics::load(p)).unwrap_or_default();
            l.drawn = None;
        }
        let idx = l.lyrics.index_at(np.pos);
        let line = idx.and_then(|i| l.lyrics.lines.get(i)).map(|x| x.1.clone()).unwrap_or_default();
        let next = idx.map(|i| i + 1).or(Some(0)).and_then(|i| l.lyrics.lines.get(i)).map(|x| x.1.clone()).unwrap_or_default();
        if l.lyrics.is_empty() || (line.is_empty() && next.is_empty()) {
            if l.visible { let _ = ShowWindow(l.hwnd, SW_HIDE); l.visible = false; l.drawn = None; }
            return;
        }
        let key = (line.clone(), next.clone());
        if l.drawn.as_ref() == Some(&key) && l.visible { return; }
        // Paint: soft shadow, then the neon-white line; the next line smaller and dim.
        let wa = work_area();
        let max_w = ((wa.right - wa.left) as f32 * 0.8).min(1500.0);
        let big = fitted(&line, 34, 700, max_w);
        let small = fitted(&next, 20, 400, max_w);
        let (w, h) = ((big.w.max(small.w) + 60).max(200), 34 + big.h + 10 + small.h + 24);
        let mut c = Canvas::new(w, h);
        let tag = text_mask("LYRICS // BETA", 11, 400);
        draw_mask(&mut c, &tag, (w - tag.w) as f32 / 2.0, 4.0, rgba(0, 229, 255, 0.55));
        let (bx, by) = ((w - big.w) as f32 / 2.0, 24.0);
        for (dx, dy, a) in [(-2.0, 2.0, 0.55), (2.0, 2.0, 0.55), (0.0, 3.0, 0.6), (-1.0, -1.0, 0.35), (1.0, -1.0, 0.35)] {
            draw_mask(&mut c, &big, bx + dx, by + dy, rgba(0, 0, 0, a));
        }
        for (dx, a) in [(-1.5, 0.25), (1.5, 0.25)] { draw_mask(&mut c, &big, bx + dx, by, rgba(0, 229, 255, a)); }
        draw_mask(&mut c, &big, bx, by, rgba(255, 255, 255, 0.98));
        let (sx, sy) = ((w - small.w) as f32 / 2.0, by + big.h as f32 + 10.0);
        for (dx, dy) in [(-1.0, 1.0), (1.0, 1.0)] { draw_mask(&mut c, &small, sx + dx, sy + dy, rgba(0, 0, 0, 0.55)); }
        draw_mask(&mut c, &small, sx, sy, rgba(255, 255, 255, 0.55));
        let x = (wa.left + wa.right - w) / 2;
        let y = wa.bottom - h - 110;
        let _ = SetWindowPos(l.hwnd, None, x, y, w, h, SWP_NOZORDER | SWP_NOACTIVATE);
        l.surf.present(l.hwnd, &c, x, y);
        if !l.visible { let _ = ShowWindow(l.hwnd, SW_SHOWNOACTIVATE); l.visible = true; place_above_desktop(l.hwnd); }
        l.drawn = Some(key);
    }

    fn control(&mut self, t: Transport) {
        let i = self.active.min(self.sources.len().saturating_sub(1));
        if let Some(s) = self.sources.get_mut(i) { s.control(t); }
    }
}

// ------------------------------------------------------------------ helpers

/// Mean colour of the wallpaper thumbnail, favouring saturated pixels (so a mostly-black wallpaper
/// with a neon sun gives the neon colour, not grey).
fn average_colour(path: &PathBuf) -> Option<(u8, u8, u8)> {
    let im = crate::img::decode(&std::fs::read(path).ok()?)?;
    let (mut r, mut g, mut b, mut wsum) = (0.0f64, 0.0f64, 0.0f64, 0.0f64);
    for q in im.px.chunks_exact(4).step_by(7) {
        let (rf, gf, bf) = (q[0] as f64, q[1] as f64, q[2] as f64);
        let (mx, mn) = (rf.max(gf).max(bf), rf.min(gf).min(bf));
        let wgt = 0.05 + (mx - mn) / 255.0 * (mx / 255.0);
        r += rf * wgt; g += gf * wgt; b += bf * wgt; wsum += wgt;
    }
    if wsum <= 0.0 { return None; }
    Some(((r / wsum) as u8, (g / wsum) as u8, (b / wsum) as u8))
}

unsafe fn work_area() -> RECT {
    let mut r = RECT::default();
    let _ = SystemParametersInfoW(SPI_GETWORKAREA, 0, Some(&mut r as *mut _ as *mut _), SYSTEM_PARAMETERS_INFO_UPDATE_FLAGS(0));
    if r.right <= r.left { r = RECT { left: 0, top: 0, right: 1920, bottom: 1080 }; }
    r
}

unsafe fn default_pos(g: &Geo) -> (i32, i32) {
    let wa = work_area();
    (wa.right - g.w - 28, wa.bottom - g.h - 28)
}

unsafe fn clamp_to_screen(x: i32, y: i32, g: &Geo) -> (i32, i32) {
    let vx = GetSystemMetrics(SM_XVIRTUALSCREEN);
    let vy = GetSystemMetrics(SM_YVIRTUALSCREEN);
    let (vw, vh) = (GetSystemMetrics(SM_CXVIRTUALSCREEN), GetSystemMetrics(SM_CYVIRTUALSCREEN));
    (x.clamp(vx - g.w / 2, vx + vw - g.w / 2), y.clamp(vy, vy + vh - g.h / 2))
}

/// Puts the widget directly above the window that hosts the desktop icons: over the wallpaper and
/// icons, under every normal window — so it belongs to the desktop, not to the window stack.
unsafe fn place_above_desktop(hwnd: HWND) { place_above_desktop_n(hwnd, 4) }

/// Same, tolerating up to steps of our own windows between the desktop and this one (the desktop
/// widgets stack several).
pub(crate) unsafe fn place_above_desktop_n(hwnd: HWND, steps: usize) {
    let mut host = HWND::default();
    let progman = FindWindowW(w!("Progman"), PCWSTR::null()).unwrap_or_default();
    if !progman.is_invalid() && FindWindowExW(progman, HWND::default(), w!("SHELLDLL_DefView"), PCWSTR::null()).map_or(false, |h| !h.is_invalid()) {
        host = progman;
    } else {
        let mut top = HWND::default();
        loop {
            top = FindWindowExW(HWND::default(), top, w!("WorkerW"), PCWSTR::null()).unwrap_or_default();
            if top.is_invalid() { break; }
            if FindWindowExW(top, HWND::default(), w!("SHELLDLL_DefView"), PCWSTR::null()).map_or(false, |h| !h.is_invalid()) { host = top; break; }
        }
    }
    if host.is_invalid() { return; }
    // Already right above it? (GetWindow(host, GW_HWNDPREV) is the window just above `host`.)
    // (Two of our windows — card and lyrics — share this spot, so "within a few steps above the
    // desktop" counts as placed. Re-inserting each one every few seconds made them swap places
    // forever, which redraws the desktop = flicker.)
    let mut w = host;
    for _ in 0..steps {
        match GetWindow(w, GW_HWNDPREV) { Ok(p) if !p.is_invalid() => { if p == hwnd { return; } w = p; } _ => break }
    }
    let _ = SetWindowPos(hwnd, host, 0, 0, 0, 0, SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE);
}

// ------------------------------------------------------------------ window

unsafe fn thread_main(state: Arc<Mutex<AppState>>, engine: crate::engine::Engine) {
    let _ = CoInitializeEx(None, COINIT_APARTMENTTHREADED);
    let hinst = GetModuleHandleW(None).unwrap_or_default();
    let wc = WNDCLASSW { lpfnWndProc: Some(wnd_proc), hInstance: hinst.into(), lpszClassName: w!("VireMusicWidget"),
        hCursor: LoadCursorW(None, IDC_ARROW).unwrap_or_default(), ..Default::default() };
    RegisterClassW(&wc);
    let hwnd = CreateWindowExW(WS_EX_LAYERED | WS_EX_TOOLWINDOW | WS_EX_NOACTIVATE, w!("VireMusicWidget"), w!("Vire Music"), WS_POPUP, 0, 0, 10, 10, None, None, hinst, None).unwrap_or_default();
    if hwnd.is_invalid() { return; }
    let sources: Vec<Box<dyn MediaSource>> = vec![Box::new(LocalSource::new(engine, state.clone()))];
    let vol0 = state.lock().unwrap().settings.pl_volume;
    W.with(|w| *w.borrow_mut() = Some(Wd {
        hwnd, surf: Surface::new(), sources, active: 0, state, np: None, look: Look { scale: 1.0, opacity: 0.8 }, accent: (150, 130, 255),
        accent_for: None, art: None, drawn: None, shown: false, drag: None, seek_preview: None, vol_preview: None, placed: None,
        last_z: std::time::Instant::now(), vol_before_mute: if vol0 > 0.0 { vol0 } else { 0.8 }, hover: 0,
        lyr: {
            let wc2 = WNDCLASSW { lpfnWndProc: Some(lyr_proc), hInstance: hinst.into(), lpszClassName: w!("VireLyrics"), ..Default::default() };
            RegisterClassW(&wc2);
            CreateWindowExW(WS_EX_LAYERED | WS_EX_TOOLWINDOW | WS_EX_NOACTIVATE | WS_EX_TRANSPARENT, w!("VireLyrics"), w!("Vire Lyrics"), WS_POPUP, 0, 0, 10, 10, None, None, hinst, None)
                .ok().map(|h| Lyr { hwnd: h, surf: Surface::new(), path: None, lyrics: Default::default(), drawn: None, visible: false })
        },
    }));
    SetTimer(hwnd, T_TICK, 250, None);
    let mut msg = MSG::default();
    while GetMessageW(&mut msg, None, 0, 0).as_bool() {
        let _ = TranslateMessage(&msg);
        DispatchMessageW(&msg);
    }
}

/// The lyrics window never takes the mouse: clicks go through to the desktop.
unsafe extern "system" fn lyr_proc(h: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    match msg {
        WM_NCHITTEST => LRESULT(-1), // HTTRANSPARENT
        WM_MOUSEACTIVATE => LRESULT(MA_NOACTIVATE as isize),
        _ => DefWindowProcW(h, msg, wp, lp),
    }
}

fn lparam_xy(lp: LPARAM) -> (f32, f32) { ((lp.0 & 0xFFFF) as i16 as f32, ((lp.0 >> 16) & 0xFFFF) as i16 as f32) }

unsafe extern "system" fn wnd_proc(h: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    match msg {
        WM_MOUSEACTIVATE => return LRESULT(MA_NOACTIVATE as isize),
        WM_TIMER => { with_w(|w| w.tick()); return LRESULT(0); }
        WM_SETCURSOR => {
            let mut p = POINT::default();
            let _ = GetCursorPos(&mut p);
            let _ = ScreenToClient(h, &mut p);
            let over = with_w(|w| {
                let g = geo(w.look.scale);
                let (x, y) = (p.x as f32, p.y as f32);
                near(g.prev, x, y, 16.0 * g.s) || near(g.play, x, y, 20.0 * g.s) || near(g.next, x, y, 16.0 * g.s)
                    || (x >= g.bar.x - 6.0 && x <= g.bar.x + g.bar.w + 6.0 && (y - g.bar.y).abs() < 10.0 * g.s)
                    || (x >= g.vol.x - 6.0 && x <= g.vol.x + g.vol.w + 6.0 && (y - g.vol.y - g.vol.h / 2.0).abs() < 9.0 * g.s)
            }).unwrap_or(false);
            let _ = SetCursor(LoadCursorW(None, if over { IDC_HAND } else { IDC_SIZEALL }).unwrap_or_default());
            return LRESULT(1);
        }
        WM_LBUTTONDOWN => {
            let (x, y) = lparam_xy(lp);
            with_w(|w| {
                let g = geo(w.look.scale);
                let s = g.s;
                if near(g.prev, x, y, 16.0 * s) { w.control(Transport::Prev); return; }
                if near(g.next, x, y, 16.0 * s) { w.control(Transport::Next); return; }
                if near(g.play, x, y, 20.0 * s) { w.control(Transport::PlayPause); return; }
                if w.np.as_ref().map_or(false, |n| n.volume.is_some()) {
                    if near(g.spk, x, y, 10.0 * s) {
                        let v = w.np.as_ref().and_then(|n| n.volume).unwrap_or(0.0);
                        if v > 0.0 { w.vol_before_mute = v; w.control(Transport::Volume(0.0)); } else { let b = w.vol_before_mute.max(0.2); w.control(Transport::Volume(b)); }
                        return;
                    }
                    if zone_at(&g, x, y) == 2 && x >= g.vol.x - 6.0 && x <= g.vol.x + g.vol.w + 6.0 {
                        w.drag = Some(Drag::Vol);
                        w.vol_preview = Some(((x - g.vol.x) / g.vol.w).clamp(0.0, 1.0));
                        SetCapture(h); w.tick(); return;
                    }
                }
                if x >= g.bar.x - 6.0 && x <= g.bar.x + g.bar.w + 6.0 && (y - g.bar.y).abs() < 10.0 * s && w.np.as_ref().map_or(false, |n| n.dur > 0.0) {
                    w.drag = Some(Drag::Seek);
                    w.seek_preview = Some(((x - g.bar.x) / g.bar.w).clamp(0.0, 1.0));
                    SetCapture(h); w.tick(); return;
                }
                // Anywhere else on the card: drag the card.
                w.drag = Some(Drag::Move(x as i32, y as i32));
                SetCapture(h);
            });
            return LRESULT(0);
        }
        0x02A3 => { // WM_MOUSELEAVE
            with_w(|w| { if w.hover != 0 { w.hover = 0; w.tick(); } });
            return LRESULT(0);
        }
        WM_MOUSEMOVE => {
            let (x, y) = lparam_xy(lp);
            let mut tme = TRACKMOUSEEVENT { cbSize: std::mem::size_of::<TRACKMOUSEEVENT>() as u32, dwFlags: TME_LEAVE, hwndTrack: h, dwHoverTime: 0 };
            let _ = TrackMouseEvent(&mut tme);
            with_w(|w| {
                let g = geo(w.look.scale);
                if w.drag.is_none() {
                    let z = zone_at(&g, x, y);
                    if z != w.hover { w.hover = z; w.tick(); }
                }
                match w.drag {
                    Some(Drag::Move(dx, dy)) => {
                        let mut p = POINT::default();
                        let _ = GetCursorPos(&mut p);
                        let (nx, ny) = clamp_to_screen(p.x - dx, p.y - dy, &g);
                        let _ = SetWindowPos(w.hwnd, None, nx, ny, 0, 0, SWP_NOSIZE | SWP_NOZORDER | SWP_NOACTIVATE);
                    }
                    Some(Drag::Seek) => { w.seek_preview = Some(((x - g.bar.x) / g.bar.w).clamp(0.0, 1.0)); w.tick(); }
                    Some(Drag::Vol) => { w.vol_preview = Some(((x - g.vol.x) / g.vol.w).clamp(0.0, 1.0)); w.tick(); }
                    None => {}
                }
            });
            return LRESULT(0);
        }
        WM_LBUTTONUP => {
            let _ = ReleaseCapture();
            with_w(|w| {
                match w.drag.take() {
                    Some(Drag::Move(..)) => {
                        let mut r = RECT::default();
                        let _ = GetWindowRect(w.hwnd, &mut r);
                        let mut st = w.state.lock().unwrap();
                        st.settings.w_pos = Some((r.left, r.top));
                        st.save();
                        drop(st);
                        w.placed = Some(((r.left, r.top), w.placed.map_or(1, |p| p.1)));
                        // keep `placed` in step with the settings so tick() doesn't move it again
                        let sz = w.state.lock().unwrap().settings.w_size;
                        w.placed = Some(((r.left, r.top), sz));
                    }
                    Some(Drag::Seek) => {
                        if let (Some(f), Some(n)) = (w.seek_preview.take(), w.np.clone()) { w.control(Transport::Seek(f as f64 * n.dur)); }
                    }
                    Some(Drag::Vol) => { if let Some(v) = w.vol_preview.take() { w.control(Transport::Volume(v)); } }
                    None => {}
                }
                w.drawn = None;
            });
            return LRESULT(0);
        }
        WM_RBUTTONUP => { widget_menu(h); return LRESULT(0); }
        _ => {}
    }
    DefWindowProcW(h, msg, wp, lp)
}

unsafe fn widget_menu(h: HWND) {
    let menu = CreatePopupMenu().unwrap_or_default();
    let cur = with_w(|w| { let st = w.state.lock().unwrap(); (st.settings.w_size, st.settings.w_opacity, st.settings.w_always) }).unwrap_or((1, 0.8, false));
    let add = |m: HMENU, id: usize, text: &str, checked: bool| {
        let _ = AppendMenuW(m, if checked { MF_STRING | MF_CHECKED } else { MF_STRING }, id, &windows::core::HSTRING::from(text));
    };
    add(menu, 1, "Mở trang Nhạc", false);
    let _ = AppendMenuW(menu, MF_SEPARATOR, 0, PCWSTR::null());
    let size = CreatePopupMenu().unwrap_or_default();
    for (i, t) in ["Nhỏ", "Vừa", "Lớn"].iter().enumerate() { add(size, 10 + i, t, cur.0 as usize == i); }
    let _ = AppendMenuW(menu, MF_POPUP, size.0 as usize, w!("Kích thước"));
    let op = CreatePopupMenu().unwrap_or_default();
    for (i, v) in [0.45f32, 0.62, 0.78, 0.92].iter().enumerate() { add(op, 20 + i, &format!("{}%", (v * 100.0) as i32), (cur.1 - v).abs() < 0.04); }
    let _ = AppendMenuW(menu, MF_POPUP, op.0 as usize, w!("Độ trong suốt"));
    add(menu, 30, "Luôn hiện (bỏ chọn: chỉ hiện khi đang phát nhạc)", cur.2);
    add(menu, 31, "Về góc phải dưới", false);
    let _ = AppendMenuW(menu, MF_SEPARATOR, 0, PCWSTR::null());
    add(menu, 40, "Ẩn widget", false);
    let mut p = POINT::default();
    let _ = GetCursorPos(&mut p);
    let _ = SetForegroundWindow(h);
    let cmd = TrackPopupMenu(menu, TPM_RETURNCMD | TPM_RIGHTBUTTON, p.x, p.y, 0, h, None).0 as usize;
    let _ = PostMessageW(h, WM_NULL, WPARAM(0), LPARAM(0));
    let _ = DestroyMenu(menu);
    if cmd == 0 { return; }
    if cmd == 1 { crate::tray::request_open(); return; }
    with_w(|w| {
        let mut st = w.state.lock().unwrap();
        match cmd {
            10..=12 => st.settings.w_size = (cmd - 10) as u8,
            20..=23 => st.settings.w_opacity = [0.45f32, 0.62, 0.78, 0.92][cmd - 20],
            30 => st.settings.w_always = !st.settings.w_always,
            31 => st.settings.w_pos = None,
            40 => st.settings.w_enabled = false,
            _ => {}
        }
        st.save();
        drop(st);
        w.drawn = None;
    });
}
