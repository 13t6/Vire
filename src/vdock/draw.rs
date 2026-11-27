//! Painting the glass slab and the icons layer, show / hide, AppBar reservation, animation.

use super::*;

impl Dock {
    /// The blur slab is a plain (non-layered) window: DWM clips its blur to the window region,
    /// so the rounded corners are real — no square halo. Its client is black = transparent glass.
    pub(super) unsafe fn draw_body(&mut self) {
        // Reflection style has no plate at all: the blur window stays hidden.
        if self.cfg.style == 1 {
            if IsWindowVisible(self.body).as_bool() { let _ = ShowWindow(self.body, SW_HIDE); }
            self.body_drawn = None;
            return;
        }
        if self.shown && !IsWindowVisible(self.body).as_bool() { let _ = ShowWindow(self.body, SW_SHOWNOACTIVATE); }
        let (x, y, w, h) = self.body_rect(self.reveal);
        let key = (w, h, self.cfg.material);
        // Always slot the slab directly *below* the icon layer (never "to the top", which would
        // cover the icons while it slides in).
        if self.body_drawn != Some(key) {
            let _ = SetWindowPos(self.body, self.content, x, y, w, h, SWP_NOACTIVATE | SWP_NOOWNERZORDER);
            // Blur-behind with an explicit rounded region (documented DWM API): the blur follows
            // our own, softer corner radius. The tint comes from the content layer.
            {
                use windows::Win32::Graphics::Dwm::{DwmEnableBlurBehindWindow, DWM_BLURBEHIND};
                use windows::Win32::Graphics::Gdi::{CreateRoundRectRgn, DeleteObject};
                let d = (CORNER * self.geo.s * 2.0).round() as i32;
                let rgn = CreateRoundRectRgn(0, 0, w + 1, h + 1, d, d);
                let bb = DWM_BLURBEHIND { dwFlags: 3, fEnable: BOOL(1), hRgnBlur: rgn, fTransitionOnMaximized: BOOL(0) };
                let _ = DwmEnableBlurBehindWindow(self.body, &bb);
                let _ = DeleteObject(rgn);
            }
            self.body_drawn = Some(key);
        } else {
            let _ = SetWindowPos(self.body, self.content, x, y, 0, 0, SWP_NOACTIVATE | SWP_NOOWNERZORDER | SWP_NOSIZE);
        }
    }

    /// Tint, sheen and liquid rim of the glass slab, drawn (anti-aliased) on the content layer.
    /// Radius matches Windows 11's rounded window corners (8 px × DPI) that clip the blur.
    pub(super) fn paint_glass(&self, c: &mut Canvas, r: R) {
        let s = self.geo.s;
        let rad = CORNER * s;
        let (tint, rim_top, rim_bot, sheen) = match self.cfg.material {
            Material::Glass => (rgba(255, 255, 255, 0.08), 0.60, 0.24, 0.13),
            Material::Acrylic => (rgba(16, 18, 26, 0.18), 0.40, 0.14, 0.06),
            Material::Mica => (rgba(22, 24, 34, 0.30), 0.26, 0.10, 0.03),
        };
        fill_rrect(c, r, rad, |_| tint);
        if !self.vertical() {
            fill_rrect(c, r, rad, |fy| rgba(255, 255, 255, if fy < 0.45 { sheen * (1.0 - fy / 0.45) } else { 0.0 }));
        }
        let outer = R { x: r.x - 0.6, y: r.y - 0.6, w: r.w + 1.2, h: r.h + 1.2 };
        stroke_rrect(c, outer, rad + 0.6, 1.6 * s, |fy| rgba(255, 255, 255, rim_top + (rim_bot - rim_top) * fy));
        stroke_rrect(c, R { x: r.x + 1.8 * s, y: r.y + 1.8 * s, w: r.w - 3.6 * s, h: r.h - 3.6 * s }, rad - 1.8 * s, 1.0, |_| rgba(255, 255, 255, 0.05));
    }

    pub(super) unsafe fn draw_content(&mut self) {
        let (x, y, _, _) = self.content_rect(self.reveal);
        let c = self.render_content();
        self.content_surf.present(self.content, &c, x, y);
    }

    pub(super) unsafe fn render_content(&mut self) -> Canvas {
        let (_, _, w, h) = self.content_rect(self.reveal);
        let mut c = Canvas::new(w, h);
        let g_s = self.geo.s;
        let b = self.body_local();
        let reflect = self.cfg.style == 1;
        let refl_total = self.geo.refl_gap + self.geo.refl_len;
        // Reflection style: where the icons stand (the line the mirror image starts from), per edge.
        let base = match self.cfg.edge { Edge::Bottom => b.y + b.h - refl_total, Edge::Left => b.x + refl_total, Edge::Right => b.x + b.w - refl_total };
        // Cross-axis centre of the icon zone (clock text, separators).
        let zone_c = if !reflect { if self.vertical() { b.cx() } else { b.cy() } } else { match self.cfg.edge { Edge::Bottom => base - self.geo.icon / 2.0, Edge::Left => base + self.geo.icon / 2.0, Edge::Right => base - self.geo.icon / 2.0 } };
        if reflect {
            // No plate. A nearly invisible film keeps the strip clickable / hoverable.
            fill_rrect(&mut c, b, 6.0 * g_s, |_| rgba(0, 0, 0, 0.008));
        } else {
            // Glass skin (also makes the whole slab receive clicks/hover).
            self.paint_glass(&mut c, b);
        }
        let items = self.items.clone();
        let mut tooltip: Option<(String, f32, f32)> = None;
        for (i, it) in items.iter().enumerate() {
            let along = self.item_center(i);
            let sc = self.scales.get(i).copied().unwrap_or(1.0);
            let size = self.geo.icon * sc;
            let pad = (self.geo.thick - self.geo.icon) / 2.0;
            // Icon rect anchored at the screen edge, growing toward the centre.
            let ir = if reflect { match self.cfg.edge {
                Edge::Bottom => R { x: along - size / 2.0, y: base - size, w: size, h: size },
                Edge::Left => R { x: base, y: along - size / 2.0, w: size, h: size },
                Edge::Right => R { x: base - size, y: along - size / 2.0, w: size, h: size },
            } } else { match self.cfg.edge {
                Edge::Bottom => R { x: along - size / 2.0, y: b.y + b.h - pad - size, w: size, h: size },
                Edge::Left => R { x: b.x + pad, y: along - size / 2.0, w: size, h: size },
                Edge::Right => R { x: b.x + b.w - pad - size, y: along - size / 2.0, w: size, h: size },
            } };
            match it.kind {
                Kind::App => {
                    if let Some(img) = self.icon_for(it) {
                        let img_ptr: *const Img = img;
                        draw_img(&mut c, &*img_ptr, ir, 1.0);
                    } else {
                        fill_rrect(&mut c, ir, size * 0.22, |_| rgba(120, 130, 170, 0.6));
                    }
                    if !it.wins.is_empty() {
                        // Soft white running mark; brighter + longer for the active app.
                        let (len, a) = if it.active { (18.0 * g_s, 0.95) } else { (7.0 * g_s, 0.6) };
                        let t = if reflect { 2.4 * g_s } else { 3.0 * g_s };
                        let gp = (self.geo.refl_gap - t) / 2.0; // the mark sits in the gap under the icons
                        let mr = if reflect { match self.cfg.edge {
                            Edge::Bottom => R { x: along - len / 2.0, y: base + gp, w: len, h: t },
                            Edge::Left => R { x: base - self.geo.refl_gap + gp, y: along - len / 2.0, w: t, h: len },
                            Edge::Right => R { x: base + gp, y: along - len / 2.0, w: t, h: len },
                        } } else { match self.cfg.edge {
                            Edge::Bottom => R { x: along - len / 2.0, y: b.y + b.h - 4.5 * g_s, w: len, h: t },
                            Edge::Left => R { x: b.x + 1.5 * g_s, y: along - len / 2.0, w: t, h: len },
                            Edge::Right => R { x: b.x + b.w - 4.5 * g_s, y: along - len / 2.0, w: t, h: len },
                        } };
                        fill_rrect(&mut c, mr, t / 2.0, |_| rgba(255, 255, 255, a));
                    }
                }
                Kind::Start => {
                    // Four-pane logo in the Vire palette.
                    let q = size * 0.30;
                    let gap = size * 0.06;
                    let (cx, cy) = (ir.cx(), ir.cy());
                    for (dx, dy, col) in [(-1.0, -1.0, rgba(150, 230, 244, 1.0)), (1.0, -1.0, rgba(196, 184, 255, 1.0)), (-1.0, 1.0, rgba(196, 184, 255, 1.0)), (1.0, 1.0, rgba(150, 230, 244, 1.0))] {
                        let rr = R { x: cx + if dx < 0.0 { -q - gap / 2.0 } else { gap / 2.0 }, y: cy + if dy < 0.0 { -q - gap / 2.0 } else { gap / 2.0 }, w: q, h: q };
                        fill_rrect(&mut c, rr, q * 0.22, |_| col);
                    }
                }
                Kind::Tray => {
                    // Chevron "^" (points toward the screen centre).
                    let (cx, cy) = (ir.cx(), ir.cy());
                    let k = size * 0.16;
                    let th = 2.6 * g_s;
                    let pts: [(f32, f32); 3] = match self.cfg.edge {
                        Edge::Bottom => [(-k, k * 0.5), (0.0, -k * 0.5), (k, k * 0.5)],
                        Edge::Left => [(-k * 0.5, -k), (k * 0.5, 0.0), (-k * 0.5, k)],
                        Edge::Right => [(k * 0.5, -k), (-k * 0.5, 0.0), (k * 0.5, k)],
                    };
                    for seg in [(pts[0], pts[1]), (pts[1], pts[2])] {
                        // Anti-aliased thick line via small round dots along the segment.
                        let steps = 24;
                        for s in 0..=steps {
                            let t = s as f32 / steps as f32;
                            let (x, y) = (cx + seg.0 .0 + (seg.1 .0 - seg.0 .0) * t, cy + seg.0 .1 + (seg.1 .1 - seg.0 .1) * t);
                            fill_rrect(&mut c, R { x: x - th / 2.0, y: y - th / 2.0, w: th, h: th }, th / 2.0, |_| rgba(240, 242, 250, 0.9));
                        }
                    }
                }
                Kind::Control => {
                    // Two "sliders" glyph.
                    let (cx, cy) = (ir.cx(), ir.cy());
                    let wl = size * 0.46;
                    for (k, off) in [(0.3f32, -0.12f32), (0.7, 0.12)] {
                        let yy = cy + off * size;
                        fill_rrect(&mut c, R { x: cx - wl / 2.0, y: yy - 1.2 * g_s, w: wl, h: 2.4 * g_s }, 1.2 * g_s, |_| rgba(235, 238, 250, 0.85));
                        let kx = cx - wl / 2.0 + wl * k;
                        fill_rrect(&mut c, R { x: kx - 4.0 * g_s, y: yy - 4.0 * g_s, w: 8.0 * g_s, h: 8.0 * g_s }, 4.0 * g_s, |_| rgba(255, 255, 255, 1.0));
                    }
                }
                Kind::Sep => {
                    let l = if reflect { self.geo.icon * 0.75 } else { self.geo.thick * 0.5 };
                    let sr = if self.vertical() { R { x: zone_c - l / 2.0, y: along - 0.5, w: l, h: 1.0 } } else { R { x: along - 0.5, y: zone_c - l / 2.0, w: 1.0, h: l } };
                    fill_rrect(&mut c, sr, 0.5, |_| rgba(255, 255, 255, 0.28));
                }
                Kind::Clock => {
                    let tm = text_mask(&self.clock.0, (15.0 * g_s) as i32, 600);
                    if self.vertical() {
                        draw_mask(&mut c, &tm, zone_c - tm.w as f32 / 2.0, along - tm.h as f32 / 2.0, rgba(255, 255, 255, 1.0));
                    } else {
                        let dm = text_mask(&self.clock.1, (11.5 * g_s) as i32, 400);
                        let total = tm.h as f32 + dm.h as f32 - 2.0 * g_s;
                        let y0 = zone_c - total / 2.0;
                        draw_mask(&mut c, &tm, along - tm.w as f32 / 2.0, y0, rgba(255, 255, 255, 1.0));
                        draw_mask(&mut c, &dm, along - dm.w as f32 / 2.0, y0 + tm.h as f32 - 2.0 * g_s, rgba(225, 230, 245, 0.75));
                    }
                }
            }
            // Tooltip for the hovered item.
            if let Some((mx, my)) = self.hover_local() {
                if self.hit(mx, my) == Some(i) && it.kind != Kind::Sep && !self.menu_open {
                    tooltip = Some((it.name.clone(), ir.cx(), match self.cfg.edge { Edge::Bottom => ir.y, Edge::Left => ir.x + ir.w, Edge::Right => ir.x }));
                }
            }
        }
        if reflect {
            let edge = match self.cfg.edge { Edge::Bottom => 0, Edge::Left => 1, Edge::Right => 2 };
            c.reflect(edge, base.round() as i32, self.geo.refl_gap.round() as i32, self.geo.refl_len.round() as i32, 0.42);
        }
        if let Some((name, cx, edge_pos)) = tooltip {
            let m = text_mask(&name, (13.0 * g_s) as i32, 500);
            let (pw, ph) = (m.w as f32 + 22.0 * g_s, 28.0 * g_s);
            let along = cx; // for vertical docks this is the icon centre x; use y below
            let pr = match self.cfg.edge {
                Edge::Bottom => R { x: clampf(along - pw / 2.0, 0.0, w as f32 - pw), y: edge_pos - 10.0 * g_s - ph, w: pw, h: ph },
                Edge::Left | Edge::Right => {
                    let i = self.hover_local().and_then(|(mx, my)| self.hit(mx, my)).unwrap_or(0);
                    let cy = self.item_center(i);
                    let px = if self.cfg.edge == Edge::Left { edge_pos + 10.0 * g_s } else { edge_pos - 10.0 * g_s - pw };
                    R { x: clampf(px, 0.0, w as f32 - pw), y: clampf(cy - ph / 2.0, 0.0, h as f32 - ph), w: pw, h: ph }
                }
            };
            fill_rrect(&mut c, pr, ph / 2.0, |_| rgba(18, 20, 28, 0.88));
            stroke_rrect(&mut c, pr, ph / 2.0, 1.0, |_| rgba(255, 255, 255, 0.14));
            draw_mask(&mut c, &m, pr.x + 11.0 * g_s, pr.cy() - m.h as f32 / 2.0, rgba(255, 255, 255, 1.0));
        }
        c
    }

    pub(super) unsafe fn redraw_all(&mut self) {
        if !self.shown { return; }
        self.draw_body();
        self.draw_content();
        // Content always above the glass slab.
        let _ = SetWindowPos(self.content, HWND_TOPMOST, 0, 0, 0, 0, SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE);
    }

    // -------------------------------------------------------------- behaviour

    /// Shows/hides both dock windows. Always checks the real window state (never trusts a flag).
    pub(super) unsafe fn set_visible(&mut self, v: bool) {
        dbg(&format!("dock[{:?}] set_visible({v})", self.mon_rect.map(|r| (r.left, r.top))));
        self.shown = v;
        for h in [self.body, self.content] {
            let vis = IsWindowVisible(h).as_bool();
            if v && !vis { let _ = ShowWindow(h, SW_SHOWNOACTIVATE); }
            if !v && vis { let _ = ShowWindow(h, SW_HIDE); }
        }
        if v { self.body_drawn = None; self.redraw_all(); }
    }

    pub(super) unsafe fn update_appbar(&mut self) {
        let want = self.cfg.enabled && !self.cfg.autohide;
        let mut d = appbar_data(self.content);
        if want && !self.appbar {
            d.uCallbackMessage = WM_APPBAR;
            SHAppBarMessage(ABM_NEW, &mut d);
            self.appbar = true;
        }
        if !want && self.appbar {
            SHAppBarMessage(ABM_REMOVE, &mut d);
            self.appbar = false;
            return;
        }
        if self.appbar {
            let m = self.geo.mon;
            let t = (self.geo.thick + self.geo.margin * 2.0) as i32;
            d.uEdge = match self.cfg.edge { Edge::Bottom => ABE_BOTTOM, Edge::Left => ABE_LEFT, Edge::Right => ABE_RIGHT };
            d.rc = m;
            SHAppBarMessage(ABM_QUERYPOS, &mut d);
            match self.cfg.edge {
                Edge::Bottom => d.rc.top = d.rc.bottom - t,
                Edge::Left => d.rc.right = d.rc.left + t,
                Edge::Right => d.rc.left = d.rc.right - t,
            }
            SHAppBarMessage(ABM_SETPOS, &mut d);
            dbg(&format!("appbar SETPOS dock[{:?}] -> {},{} {}x{} (changes the work area)", self.mon_rect.map(|r| (r.left, r.top)), d.rc.left, d.rc.top, d.rc.right - d.rc.left, d.rc.bottom - d.rc.top));
        }
    }

    pub(super) unsafe fn start_anim(&mut self) {
        if !self.anim { self.anim = true; SetTimer(self.content, T_ANIM, 16, None); }
    }

    /// One animation step; returns true while still moving.
    pub(super) unsafe fn animate(&mut self) -> bool {
        let mut moving = false;
        let hover_along = self.hover.map(|(x, y)| if self.vertical() { y } else { x });
        let inside = self.hover_local().map_or(false, |(x, y)| self.hit(x, y).is_some());
        for i in 0..self.items.len() {
            let target = if self.cfg.magnify && inside && self.is_icon(i) {
                let d = (self.base_center_screen(i) - hover_along.unwrap_or(-1e6)) / self.geo.slot;
                1.0 + (self.geo.max_icon / self.geo.icon - 1.0) * (-d * d * 0.9).exp()
            } else { 1.0 };
            let cur = self.scales[i];
            let next = cur + (target - cur) * 0.32;
            if (next - target).abs() > 0.003 { moving = true; self.scales[i] = next; } else { self.scales[i] = target; }
        }
        let r = self.reveal + (self.reveal_target - self.reveal) * 0.3;
        if (r - self.reveal_target).abs() > 0.004 { moving = true; self.reveal = r; } else { self.reveal = self.reveal_target; }
        moving
    }
}
