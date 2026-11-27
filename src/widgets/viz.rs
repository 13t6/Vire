//! Music visualizer widget. Pure rendering of the *real* spectrum / waveform published by
//! `audio.rs` + `dsp.rs` — there is no random or simulated motion anywhere: with no sound the
//! picture eases down to a calm idle state and stops redrawing.
//!
//! Four looks: Spectrum Bars, Waveform, Circular, Ambient (soft glows + flowing waves).

use super::*;
use crate::dsp::{BANDS, WAVE};
use crate::raster::{draw_img, fill_rrect, glow_disc, line_aa, rgba, stroke_rrect, Canvas, Img, Rgba, R};
use eframe::egui::Ui;
use serde_json::json;

pub const INFO: KindInfo = KindInfo {
    id: "visualizer",
    name: "Music Visualizer",
    desc: "Hiển thị âm thanh đang phát trên Windows theo thời gian thực: cột phổ, sóng, vòng tròn, ánh sáng nền.",
    category: Category::Music,
    default_size: (480, 200),
    min_size: (160, 80),
    defaults: || serde_json::to_value(VizCfg::default()).unwrap(),
    create: || Box::new(Viz::new()),
    customize,
};

#[derive(Serialize, Deserialize, Clone, PartialEq)]
#[serde(default)]
pub struct VizCfg {
    /// 0 Spectrum Bars · 1 Waveform · 2 Circular · 3 Ambient.
    pub style: u8,
    pub color1: [u8; 3],
    pub color2: [u8; 3],
    pub bars: u32,
    pub sensitivity: f32,
    /// Animation speed: how fast bars react and rotate.
    pub speed: f32,
    pub glow: f32,
    pub opacity: f32,
    pub background: bool,
    pub blur: bool,
    /// Extra response of the lowest frequencies.
    pub bass: f32,
    pub intensity: f32,
    pub symmetric: bool,
    /// 0 = everything Windows plays · 1 = only the built-in music player.
    pub source: u8,
}

impl Default for VizCfg {
    fn default() -> Self {
        VizCfg { style: 0, color1: [120, 90, 255], color2: [90, 220, 255], bars: 48, sensitivity: 1.2, speed: 1.0, glow: 0.45, opacity: 1.0, background: false, blur: false, bass: 1.0, intensity: 1.0, symmetric: false, source: 0 }
    }
}

const STYLES: [&str; 4] = ["Spectrum Bars", "Waveform", "Circular", "Ambient"];
const PRESETS: [(&str, [u8; 3], [u8; 3]); 5] = [
    ("Aurora", [120, 90, 255], [90, 220, 255]),
    ("Sunset", [255, 94, 98], [255, 195, 113]),
    ("Ocean", [20, 120, 255], [60, 235, 200]),
    ("Neon", [255, 60, 200], [60, 255, 170]),
    ("Mono", [180, 185, 200], [255, 255, 255]),
];

struct Viz {
    sm: [f32; BANDS],
    pk: [f32; BANDS],
    wave: [f32; WAVE],
    agc: f32,
    bass_e: f32,
    mid_e: f32,
    treb_e: f32,
    energy: f32,
    phase: f32,
    rot: f32,
    drawn_idle: bool,
}

impl Viz {
    fn new() -> Self { Viz { sm: [0.0; BANDS], pk: [0.0; BANDS], wave: [0.0; WAVE], agc: 0.1, bass_e: 0.0, mid_e: 0.0, treb_e: 0.0, energy: 0.0, phase: 0.0, rot: 0.0, drawn_idle: false } }

    /// Feeds one real audio frame through sensitivity / bass / smoothing.
    fn update(&mut self, ctx: &Ctx, cfg: &VizCfg) {
        let dt = ctx.dt.clamp(0.001, 0.1);
        // Source Built-in player: only react while Vire's own player plays; otherwise stay idle.
        let frame = if cfg.source == 1 && !ctx.builtin_playing { None } else { ctx.audio };
        // Bars rise fast and fall slower; when Windows goes silent they settle a bit faster.
        let calm = if ctx.signal || ctx.preview { 1.0 } else { 1.7 };
        let (att, rel) = (1.0 - (-dt * 30.0 * cfg.speed).exp(), 1.0 - (-dt * 7.5 * cfg.speed * calm).exp());
        let gain = cfg.sensitivity * cfg.intensity;
        for i in 0..BANDS {
            let raw = frame.map_or(0.0, |f| f.bands[i]);
            let mut v = ((raw - 0.10) / 0.90).max(0.0).powf(1.25) * gain;
            if (i as f32) < BANDS as f32 * 0.25 { v *= cfg.bass; }
            let v = v.clamp(0.0, 1.0);
            let k = if v > self.sm[i] { att } else { rel };
            self.sm[i] += (v - self.sm[i]) * k;
            self.pk[i] = self.sm[i].max(self.pk[i] - dt * 0.35 * cfg.speed);
        }
        let mean = |a: usize, b: usize| self.sm[a..b].iter().sum::<f32>() / (b - a) as f32;
        let (be, me, te) = (mean(0, 20), mean(20, 60), mean(60, BANDS));
        self.bass_e += (be - self.bass_e) * att.max(rel);
        self.mid_e += (me - self.mid_e) * att.max(rel);
        self.treb_e += (te - self.treb_e) * att.max(rel);
        self.energy = (self.bass_e + self.mid_e + self.treb_e) / 3.0;
        // Waveform with a slow automatic gain so quiet music is still readable.
        let peak = frame.map_or(0.0, |f| f.wave.iter().fold(0.0f32, |a, &x| a.max(x.abs())));
        self.agc = peak.max(self.agc * (1.0 - dt * 0.4)).max(0.05);
        let wk = 1.0 - (-dt * 28.0 * cfg.speed).exp();
        for i in 0..WAVE {
            let target = frame.map_or(0.0, |f| (f.wave[i] / self.agc * 0.8 * gain.min(2.0)).clamp(-1.0, 1.0));
            self.wave[i] += (target - self.wave[i]) * wk;
        }
        self.phase += dt * cfg.speed * (0.6 + self.energy * 2.4);
        self.rot += dt * cfg.speed * (0.25 + self.bass_e * 1.6);
    }

    /// Band value for bar `j` of `n` (max over the bands it covers, interpolated when n > BANDS).
    fn bar(&self, j: usize, n: usize, smoothed: &[f32; BANDS]) -> f32 {
        if n >= BANDS {
            let f = j as f32 / (n - 1).max(1) as f32 * (BANDS - 1) as f32;
            let (a, t) = (f.floor() as usize, f.fract());
            return smoothed[a] * (1.0 - t) + smoothed[(a + 1).min(BANDS - 1)] * t;
        }
        let (a, b) = (j * BANDS / n, ((j + 1) * BANDS / n).max(j * BANDS / n + 1).min(BANDS));
        smoothed[a..b].iter().cloned().fold(0.0, f32::max)
    }
}

fn col(c: [u8; 3], a: f32) -> Rgba { rgba(c[0], c[1], c[2], a.clamp(0.0, 1.0)) }

impl Widget for Viz {
    fn render(&mut self, ctx: &Ctx, cfg: &Value, c: &mut Canvas) -> bool {
        let cfg: VizCfg = serde_json::from_value(cfg.clone()).unwrap_or_default();
        self.update(ctx, &cfg);
        let idle = self.energy < 0.004 && self.pk.iter().all(|&p| p < 0.01) && self.wave.iter().all(|&w| w.abs() < 0.01);
        if idle && self.drawn_idle && !ctx.preview { return false; }
        self.drawn_idle = idle;

        let s = ctx.scale;
        let (w, h) = (c.w as f32, c.h as f32);
        let op = cfg.opacity;
        if cfg.background {
            let rad = 26.0 * s;
            let card = R { x: 3.0 * s, y: 3.0 * s, w: w - 6.0 * s, h: h - 6.0 * s };
            fill_rrect(c, R { x: card.x, y: card.y + 4.0 * s, w: card.w, h: card.h }, rad, |_| rgba(0, 0, 0, 0.16 * op));
            fill_rrect(c, card, rad, |fy| rgba(14, 16, 24, (0.42 - 0.12 * fy) * op));
            fill_rrect(c, R { x: card.x, y: card.y, w: card.w, h: card.h * 0.45 }, rad, |fy| rgba(255, 255, 255, 0.07 * (1.0 - fy) * op));
            stroke_rrect(c, card, rad, 1.1 * s, |fy| rgba(255, 255, 255, (0.34 - 0.22 * fy) * op));
        }
        let pad = if cfg.background { 22.0 * s } else { 8.0 * s };
        let area = R { x: pad, y: pad, w: w - 2.0 * pad, h: h - 2.0 * pad };
        match cfg.style {
            1 => self.draw_wave(c, &cfg, area, s, op),
            2 => self.draw_circle(c, &cfg, area, s, op),
            3 => self.draw_ambient(c, &cfg, R { x: 0.0, y: 0.0, w, h }, s, op),
            _ => self.draw_bars(c, &cfg, area, s, op),
        }
        true
    }

    fn interval_ms(&self, _cfg: &Value, _now: &Now, audio_live: bool) -> u32 {
        // ~30 fps while there is sound or the picture is still settling; otherwise a cheap 4 Hz
        // look at whether sound came back (nothing is drawn while idle).
        if audio_live || !self.drawn_idle { 33 } else { 250 }
    }

    fn uses_audio(&self, _cfg: &Value) -> bool { true }

    fn blur_radius(&self, cfg: &Value) -> Option<f32> {
        let c: VizCfg = serde_json::from_value(cfg.clone()).unwrap_or_default();
        (c.background && c.blur).then_some(26.0)
    }
}

impl Viz {
    fn draw_bars(&self, c: &mut Canvas, cfg: &VizCfg, a: R, s: f32, op: f32) {
        let n = cfg.bars.clamp(4, 128) as usize;
        let slot = a.w / n as f32;
        let bw = (slot * 0.68).max(1.5);
        let minh = 3.0 * s;
        let mid = a.y + a.h / 2.0;
        for j in 0..n {
            // Symmetric layout mirrors the spectrum around the centre of the widget.
            let jj = if cfg.symmetric { let m = (j as f32 - (n as f32 - 1.0) / 2.0).abs() * 2.0 / (n as f32 - 1.0).max(1.0); ((m * (n as f32 - 1.0)).round() as usize).min(n - 1) } else { j };
            let v = self.bar(jj, n, &self.sm);
            let x = a.x + slot * j as f32 + (slot - bw) / 2.0;
            let (top, hh) = if cfg.symmetric {
                let half = (v * a.h / 2.0).max(minh / 2.0);
                (mid - half, half * 2.0)
            } else {
                let hh = (v * a.h).max(minh);
                (a.y + a.h - hh, hh)
            };
            let r = R { x, y: top, w: bw, h: hh };
            if cfg.glow > 0.02 {
                for (g, al) in [(5.0, 0.10), (2.5, 0.16)] {
                    let gg = g * s * cfg.glow * 2.0;
                    let rr = R { x: r.x - gg, y: r.y - gg, w: r.w + 2.0 * gg, h: r.h + 2.0 * gg };
                    fill_rrect(c, rr, bw / 2.0 + gg, |fy| { let k = lerp_rgb(cfg.color2, cfg.color1, fy); col(k, al * cfg.glow * 2.0 * op) });
                }
            }
            fill_rrect(c, r, bw / 2.0, |fy| { let k = lerp_rgb(cfg.color2, cfg.color1, fy); col(k, op) });
        }
    }

    fn draw_wave(&self, c: &mut Canvas, cfg: &VizCfg, a: R, s: f32, op: f32) {
        let p = 120usize;
        let mid = a.y + a.h / 2.0;
        let pts: Vec<(f32, f32)> = (0..p).map(|i| {
            let f = i as f32 / (p - 1) as f32;
            let wi = (f * (WAVE - 1) as f32).round() as usize;
            // Taper to zero at the ends so the line starts and ends on the centre line.
            let taper = (std::f32::consts::PI * f).sin().powf(0.6);
            (a.x + a.w * f, mid - self.wave[wi] * taper * a.h * 0.46)
        }).collect();
        let passes: [(f32, f32); 3] = [(9.0 * cfg.glow, 0.10 * cfg.glow), (4.5 * cfg.glow, 0.18 * cfg.glow), (0.0, 0.0)];
        for (extra, al) in passes {
            let (wd, a_) = if extra == 0.0 { (2.4 * s, 1.0) } else { ((2.4 + extra) * s, al * 2.0) };
            if a_ <= 0.01 { continue; }
            for i in 0..p - 1 {
                let k = lerp_rgb(cfg.color1, cfg.color2, i as f32 / (p - 1) as f32);
                line_aa(c, pts[i], pts[i + 1], wd, col(k, a_ * op));
            }
        }
    }

    fn draw_circle(&self, c: &mut Canvas, cfg: &VizCfg, a: R, s: f32, op: f32) {
        let (cx, cy) = (a.x + a.w / 2.0, a.y + a.h / 2.0);
        let half = a.w.min(a.h) / 2.0;
        let r0 = half * 0.47 * (1.0 + 0.08 * self.bass_e);
        let maxlen = (half - r0 - 3.0 * s).max(8.0);
        let n = (cfg.bars.clamp(12, 120) as usize / 2 * 2).max(12);
        let circ = std::f32::consts::TAU * r0;
        let width = (circ / n as f32 * 0.55).clamp(1.6 * s, 9.0 * s);
        // Thin ring under the bars.
        let rp = 90;
        for i in 0..rp {
            let (t0, t1) = (i as f32 / rp as f32 * std::f32::consts::TAU, (i + 1) as f32 / rp as f32 * std::f32::consts::TAU);
            let k = lerp_rgb(cfg.color1, cfg.color2, ((t0 * 2.0).sin() * 0.5 + 0.5).abs());
            line_aa(c, (cx + t0.cos() * (r0 - 4.0 * s), cy + t0.sin() * (r0 - 4.0 * s)), (cx + t1.cos() * (r0 - 4.0 * s), cy + t1.sin() * (r0 - 4.0 * s)), 1.2 * s, col(k, 0.35 * op));
        }
        for i in 0..n {
            let ang = self.rot + i as f32 / n as f32 * std::f32::consts::TAU;
            // Mirror the spectrum left/right so it is continuous around the circle.
            let m = if cfg.symmetric { (i as f32 / n as f32 * 2.0 - 1.0).abs() } else { i as f32 / n as f32 };
            let bi = (m * (BANDS - 1) as f32).round() as usize;
            let v = self.sm[bi.min(BANDS - 1)];
            let len = (v * maxlen).max(2.5 * s);
            let (cs, sn) = (ang.cos(), ang.sin());
            let (p0, p1) = ((cx + cs * r0, cy + sn * r0), (cx + cs * (r0 + len), cy + sn * (r0 + len)));
            let k = lerp_rgb(cfg.color1, cfg.color2, m);
            if cfg.glow > 0.02 { line_aa(c, p0, p1, width + 5.0 * s * cfg.glow, col(k, 0.16 * cfg.glow * 2.0 * op)); }
            line_aa(c, p0, p1, width, col(k, op));
        }
    }

    fn draw_ambient(&self, c: &mut Canvas, cfg: &VizCfg, a: R, s: f32, op: f32) {
        // Drawn at quarter resolution and scaled up: soft by nature, and 16× cheaper.
        let (lw, lh) = (((a.w / 4.0) as i32).max(8), ((a.h / 4.0) as i32).max(6));
        let mut lo = Canvas::new(lw, lh);
        let (fw, fh) = (lw as f32, lh as f32);
        let t = self.phase;
        let k = (self.energy * 1.6).min(1.0);
        let blobs = [
            (0.25 + 0.18 * (t * 0.7).sin(), 0.55 + 0.20 * (t * 0.9 + 1.0).cos(), 0.30 + 0.35 * self.bass_e, cfg.color1),
            (0.70 + 0.16 * (t * 0.6 + 2.0).cos(), 0.45 + 0.22 * (t * 0.8).sin(), 0.26 + 0.35 * self.mid_e, cfg.color2),
            (0.48 + 0.25 * (t * 0.5 + 4.0).sin(), 0.70 + 0.12 * (t * 1.1).cos(), 0.22 + 0.32 * self.treb_e, lerp_rgb(cfg.color1, cfg.color2, 0.5)),
        ];
        for (bx, by, r, color) in blobs {
            glow_disc(&mut lo, bx * fw, by * fh, r * fw, r * fw * 0.8 + fh * 0.2, col(color, 1.0), (0.22 + 0.60 * k) * op * cfg.intensity.min(1.6));
        }
        // Two flowing waves along the bottom, amplitude from the real mid / bass energy.
        for (layer, (amp, freq, speed, color, al)) in [(0.10 + 0.24 * self.bass_e, 2.0, 1.0, cfg.color1, 0.40), (0.07 + 0.20 * self.mid_e, 3.1, -1.4, cfg.color2, 0.32)].into_iter().enumerate() {
            for x in 0..lw {
                let f = x as f32 / fw;
                let y = fh * (0.72 - 0.06 * layer as f32) + (f * freq * std::f32::consts::TAU + t * speed * 1.5).sin() * fh * amp * 0.6
                    + (f * freq * 1.9 * std::f32::consts::TAU - t * speed).sin() * fh * amp * 0.25;
                let y0 = y.max(0.0) as i32;
                for yy in y0..lh {
                    let d = (yy as f32 - y) / (fh - y).max(1.0);
                    lo.blend(x, yy, col(color, 1.0), al * (1.0 - d).max(0.0) * op * (0.35 + 0.65 * k));
                }
            }
        }
        draw_img(c, &Img { w: lw, h: lh, px: lo.px }, a, 1.0);
        let _ = s;
    }
}

// ------------------------------------------------------------------ settings panel

use crate::ui::kit;

fn customize(ui: &mut Ui, v: &mut Value) -> bool {
    let mut cfg: VizCfg = serde_json::from_value(v.clone()).unwrap_or_default();
    let before = cfg.clone();
    kit::row(ui, "Kiểu hiển thị", |ui| { if let Some(i) = kit::combo(ui, "viz_style", cfg.style as usize, &STYLES) { cfg.style = i as u8; true } else { false } });
    kit::row(ui, "Nguồn âm thanh", |ui| { if let Some(i) = kit::segmented(ui, &["Âm thanh hệ thống", "Trình phát trong app"], cfg.source as usize) { cfg.source = i as u8; true } else { false } });
    kit::row(ui, "Màu / dải màu", |ui| {
        let mut ch = false;
        ui.horizontal(|ui| {
            ch |= kit::color(ui, &mut cfg.color1);
            ui.label("→");
            ch |= kit::color(ui, &mut cfg.color2);
            for (name, a, b) in PRESETS { if ui.small_button(name).clicked() { cfg.color1 = a; cfg.color2 = b; ch = true; } }
        });
        ch
    });
    if cfg.style != 1 && cfg.style != 3 {
        let mut b = cfg.bars as f32;
        if kit::slider_row(ui, "Số cột", &mut b, 8.0..=96.0, |v| format!("{v:.0}")) { cfg.bars = b.round() as u32; }
    }
    kit::slider_row(ui, "Độ nhạy", &mut cfg.sensitivity, 0.3..=3.0, |v| format!("{v:.1}×"));
    kit::slider_row(ui, "Cường độ", &mut cfg.intensity, 0.3..=2.0, |v| format!("{v:.1}×"));
    kit::slider_row(ui, "Đáp ứng âm trầm", &mut cfg.bass, 0.3..=2.0, |v| format!("{v:.1}×"));
    kit::slider_row(ui, "Tốc độ hiệu ứng", &mut cfg.speed, 0.3..=2.5, |v| format!("{v:.1}×"));
    kit::slider_row(ui, "Độ phát sáng", &mut cfg.glow, 0.0..=1.0, |v| format!("{:.0}%", v * 100.0));
    kit::slider_row(ui, "Độ trong suốt", &mut cfg.opacity, 0.15..=1.0, |v| format!("{:.0}%", v * 100.0));
    if cfg.style == 0 || cfg.style == 2 { kit::row(ui, "Bố cục đối xứng", |ui| kit::toggle(ui, &mut cfg.symmetric).changed()); }
    kit::row(ui, "Hiện nền kính", |ui| kit::toggle(ui, &mut cfg.background).changed());
    if cfg.background { kit::row(ui, "Làm mờ nền", |ui| kit::toggle(ui, &mut cfg.blur).changed()); }
    if cfg != before { *v = json!(cfg); true } else { false }
}
