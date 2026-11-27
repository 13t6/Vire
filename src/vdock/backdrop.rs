//! Is the picture behind the dock light? Then the dock inverts: dark icons / text (and a light
//! glass tint) instead of white ones that would vanish on a white backdrop.
//!
//! "Behind the dock" is whatever is really on the screen there — the wallpaper, or an app window
//! that sits at the bottom of the screen (a dark app over a white wallpaper keeps the dock white).
//! It is measured on a short row of screen pixels just beside the dock's plate (the transparent
//! margin of the dock shows what is underneath), twice a second, smoothed and with hysteresis so
//! a moving video does not make the dock flicker between the two looks.

use super::*;

impl Dock {
    /// Re-measures the backdrop of this dock. Returns true when light / dark flipped.
    pub(super) unsafe fn update_backdrop(&mut self) -> bool {
        let s = self.geo.s;
        let (bx, by, bw, bh) = self.body_rect(1.0);
        let n = 24;
        let dc = GetDC(None);
        if dc.is_invalid() { return false; }
        let (mut sum, mut cnt) = (0.0f32, 0u32);
        for k in 0..n {
            let t = 0.05 + 0.90 * (k as f32 + 0.5) / n as f32;
            let (x, y) = match self.cfg.edge {
                Edge::Bottom => ((bx as f32 + bw as f32 * t) as i32, (by as f32 - 5.0 * s) as i32),
                Edge::Left => ((bx as f32 + bw as f32 + 5.0 * s) as i32, (by as f32 + bh as f32 * t) as i32),
                Edge::Right => ((bx as f32 - 5.0 * s) as i32, (by as f32 + bh as f32 * t) as i32),
            };
            let c = GetPixel(dc, x, y).0;
            if c == 0xFFFF_FFFF { continue; } // outside every screen
            let lin = |v: u32| { let q = (v & 255) as f32 / 255.0; if q <= 0.04045 { q / 12.92 } else { ((q + 0.055) / 1.055).powf(2.4) } };
            sum += 0.2126 * lin(c) + 0.7152 * lin(c >> 8) + 0.0722 * lin(c >> 16);
            cnt += 1;
        }
        ReleaseDC(None, dc);
        if cnt < 4 { return false; }
        let now = sum / cnt as f32;
        self.luma = if self.luma_set { self.luma * 0.55 + now * 0.45 } else { now };
        self.luma_set = true;
        let want = if self.light { self.luma > 0.30 } else { self.luma > 0.42 };
        let flipped = want != self.light;
        if flipped { dbg(&format!("backdrop: {} (luma {:.2})", if want { "light -> dark ink" } else { "dark -> white ink" }, self.luma)); }
        self.light = want;
        flipped
    }
}
