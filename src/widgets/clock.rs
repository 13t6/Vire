//! Clock widget: five typographic styles on the system clock, in the user's own time zone (or a
//! fixed UTC offset). It repaints once a second (once a minute without seconds) — only when the
//! text really changed — and sleeps otherwise.

use super::*;
use crate::raster::{blur_mask, draw_mask, draw_mask_grad, fill_rrect, rgba, stroke_rrect, text_mask_ex, Mask, R};
use eframe::egui::Ui;
use serde_json::json;

pub const INFO: KindInfo = KindInfo {
    id: "clock",
    name: "Đồng hồ",
    desc: "Đồng hồ số lớn kiểu màn hình khoá macOS / iOS: giờ, ngày, giây, múi giờ tuỳ chọn.",
    category: Category::Clock,
    default_size: (440, 190),
    min_size: (180, 80),
    defaults: || serde_json::to_value(ClockCfg::preset(4)).unwrap(),
    create: || Box::new(Clock { last: None }),
    customize,
};

#[derive(Serialize, Deserialize, Clone, PartialEq)]
#[serde(default)]
pub struct ClockCfg {
    /// 0 Minimal Digital · 1 Elegant Typography · 2 Glass Clock · 3 Modern Bold · 4 Cinematic Clock.
    pub style: u8,
    /// Font family ("" = the style's own).
    pub font: String,
    /// Text size in percent of the style's size.
    pub size: f32,
    pub color: [u8; 3],
    /// 0 left · 1 centre · 2 right.
    pub align: u8,
    pub opacity: f32,
    /// Frosted-glass blur behind the clock.
    pub blur: bool,
    pub shadow: f32,
    pub glow: f32,
    pub show_date: bool,
    pub show_seconds: bool,
    pub hour24: bool,
    /// Fixed UTC offset in minutes; None = this PC's own time zone.
    pub tz: Option<i32>,
    pub show_tz: bool,
}

impl Default for ClockCfg { fn default() -> Self { ClockCfg::preset(4) } }

impl ClockCfg {
    /// Settings that make a style look the way it was designed (the user can change any of them after).
    pub fn preset(style: u8) -> ClockCfg {
        let base = ClockCfg { style, font: String::new(), size: 100.0, color: [255, 255, 255], align: 1, opacity: 1.0, blur: false, shadow: 0.5, glow: 0.0, show_date: true, show_seconds: false, hour24: true, tz: None, show_tz: false };
        match style {
            0 => ClockCfg { align: 0, shadow: 0.35, ..base },
            1 => ClockCfg { align: 1, shadow: 0.4, ..base },
            2 => ClockCfg { align: 1, blur: true, shadow: 0.2, ..base },
            3 => ClockCfg { align: 0, color: [255, 214, 90], shadow: 0.6, ..base },
            _ => ClockCfg { align: 1, glow: 0.55, shadow: 0.3, ..base },
        }
    }
}

const STYLES: [&str; 5] = ["Minimal Digital", "Elegant Typography", "Glass Clock", "Modern Bold", "Cinematic Clock"];
const FONTS: [&str; 11] = ["", "Segoe UI", "Segoe UI Light", "Segoe UI Black", "Bahnschrift", "Georgia", "Cambria", "Consolas", "Cascadia Mono", "Trebuchet MS", "Arial"];

struct Look { face: &'static str, weight: i32, time_px: f32, date_px: f32, upper: bool, spacing: i32, gradient: bool, card: bool, pill: bool, rule: bool, date_above: bool }

fn look(style: u8) -> Look {
    match style {
        0 => Look { face: "Segoe UI Light", weight: 300, time_px: 96.0, date_px: 20.0, upper: false, spacing: 0, gradient: false, card: false, pill: false, rule: false, date_above: false },
        1 => Look { face: "Georgia", weight: 400, time_px: 84.0, date_px: 15.0, upper: true, spacing: 3, gradient: false, card: false, pill: false, rule: true, date_above: true },
        2 => Look { face: "Segoe UI", weight: 600, time_px: 76.0, date_px: 18.0, upper: false, spacing: 0, gradient: false, card: true, pill: false, rule: false, date_above: false },
        3 => Look { face: "Segoe UI Black", weight: 900, time_px: 100.0, date_px: 17.0, upper: true, spacing: 2, gradient: false, card: false, pill: true, rule: false, date_above: false },
        _ => Look { face: "Segoe UI Light", weight: 300, time_px: 124.0, date_px: 17.0, upper: true, spacing: 5, gradient: true, card: false, pill: false, rule: true, date_above: false },
    }
}

struct Clock { last: Option<(String, String)> }

const WEEKDAY_EN: [&str; 7] = ["Sunday", "Monday", "Tuesday", "Wednesday", "Thursday", "Friday", "Saturday"];
const WEEKDAY_VI: [&str; 7] = ["Chủ nhật", "Thứ hai", "Thứ ba", "Thứ tư", "Thứ năm", "Thứ sáu", "Thứ bảy"];
const MONTH_EN: [&str; 12] = ["January", "February", "March", "April", "May", "June", "July", "August", "September", "October", "November", "December"];

fn date_text(n: &Now, upper: bool) -> String {
    let s = if ui_vietnamese() {
        format!("{}, {} tháng {}", WEEKDAY_VI[(n.weekday % 7) as usize], n.day, n.month)
    } else if upper {
        format!("{} · {} {}", WEEKDAY_EN[(n.weekday % 7) as usize], n.day, MONTH_EN[(n.month as usize - 1) % 12])
    } else {
        format!("{}, {} {}", WEEKDAY_EN[(n.weekday % 7) as usize], n.day, MONTH_EN[(n.month as usize - 1) % 12])
    };
    if upper { s.to_uppercase() } else { s }
}

fn tz_text(off: i32) -> String {
    let (sign, a) = if off < 0 { ('-', -off) } else { ('+', off) };
    if a % 60 == 0 { format!("UTC{sign}{}", a / 60) } else { format!("UTC{sign}{}:{:02}", a / 60, a % 60) }
}

impl Widget for Clock {
    fn render(&mut self, ctx: &Ctx, cfg: &Value, c: &mut Canvas) -> bool {
        let cfg: ClockCfg = serde_json::from_value(cfg.clone()).unwrap_or_default();
        let lk = look(cfg.style);
        let now = match cfg.tz { Some(off) => shifted(&ctx.now, off), None => ctx.now };
        let (h12, suffix) = (if now.hour % 12 == 0 { 12 } else { now.hour % 12 }, if now.hour < 12 { "AM" } else { "PM" });
        let main = if cfg.hour24 { format!("{:02}:{:02}", now.hour, now.minute) } else { format!("{}:{:02}", h12, now.minute) };
        let secs = if cfg.show_seconds { format!(":{:02}", now.second) } else { String::new() };
        let mut date = if cfg.show_date { date_text(&now, lk.upper) } else { String::new() };
        if cfg.show_tz { let tz = tz_text(cfg.tz.unwrap_or(ctx.now.utc_offset_min)); date = if date.is_empty() { tz } else { format!("{date}  ·  {tz}") }; }
        let key = (format!("{main}{secs}{}", if cfg.hour24 { "" } else { suffix }), date.clone());
        self.last = Some(key);

        let s = ctx.scale;
        let (w, h) = (c.w as f32, c.h as f32);
        let face = if cfg.font.is_empty() { lk.face } else { cfg.font.as_str() };
        let weight = if cfg.font.is_empty() { lk.weight } else { 400 };
        let base = cfg.size / 100.0 * s;
        let margin = if lk.card { 26.0 * s } else { 12.0 * s };

        // Fit the time into the width: shrink the text, never clip it.
        let mut time_px = lk.time_px * base;
        let mut tm = text_mask_ex(&main, time_px as i32, weight, face, 0);
        let sec_w = if secs.is_empty() { 0 } else { (text_mask_ex(&secs, (time_px * 0.42) as i32, weight, face, 0).w as f32 * 1.02) as i32 };
        let total_w = |tm: &Mask| tm.w as f32 + sec_w as f32;
        if total_w(&tm) > w - 2.0 * margin {
            time_px *= ((w - 2.0 * margin) / total_w(&tm)).max(0.2);
            tm = text_mask_ex(&main, time_px as i32, weight, face, 0);
        }
        let sm = (!secs.is_empty()).then(|| text_mask_ex(&secs, (time_px * 0.42) as i32, weight, face, 0));
        let suf = (!cfg.hour24).then(|| text_mask_ex(suffix, (time_px * 0.28).max(10.0) as i32, 600, "Segoe UI", 1));
        let date_px = (lk.date_px * base).max(9.0);
        let dm = (!date.is_empty()).then(|| text_mask_ex(&date, date_px as i32, if lk.pill { 700 } else { 400 }, if cfg.font.is_empty() { if cfg.style == 1 { "Georgia" } else { "Segoe UI" } } else { face }, lk.spacing));

        // Stack the pieces.
        let gap = 8.0 * s * cfg.size / 100.0;
        let time_h = tm.h as f32;
        let pill_pad = if lk.pill { (8.0 * s, 4.0 * s) } else { (0.0, 0.0) };
        let date_h = dm.as_ref().map_or(0.0, |m| m.h as f32 + pill_pad.1 * 2.0);
        let rule_h = if lk.rule && dm.is_some() { 14.0 * s } else { 0.0 };
        let total_h = time_h * 0.86 + if dm.is_some() { gap + date_h + rule_h } else { 0.0 };
        let top = ((h - total_h) / 2.0).max(2.0);

        // Card (Glass Clock).
        if lk.card {
            let card = R { x: 6.0 * s, y: 6.0 * s, w: w - 12.0 * s, h: h - 12.0 * s };
            let rad = 30.0 * s;
            fill_rrect(c, R { x: card.x, y: card.y + 5.0 * s, w: card.w, h: card.h }, rad, |_| rgba(0, 0, 0, 0.18 * cfg.opacity));
            fill_rrect(c, card, rad, |fy| rgba(255, 255, 255, (0.16 - 0.08 * fy) * cfg.opacity));
            fill_rrect(c, R { x: card.x, y: card.y, w: card.w, h: card.h * 0.5 }, rad, |fy| rgba(255, 255, 255, 0.10 * (1.0 - fy) * cfg.opacity));
            stroke_rrect(c, card, rad, 1.2 * s, |fy| rgba(255, 255, 255, (0.55 - 0.35 * fy) * cfg.opacity));
        }

        let place = |mw: f32| -> f32 {
            match cfg.align { 0 => margin, 2 => (w - margin - mw).max(margin), _ => ((w - mw) / 2.0).max(0.0) }
        };
        let col = rgba(cfg.color[0], cfg.color[1], cfg.color[2], cfg.opacity);
        let op = cfg.opacity;

        // Date above (Elegant).
        let mut y = top;
        if lk.date_above {
            if let Some(dm) = &dm {
                let x = place(dm.w as f32);
                paint_text(c, dm, x, y, rgba(cfg.color[0], cfg.color[1], cfg.color[2], 0.78 * op), &cfg, s, 0.4);
                y += dm.h as f32 + gap * 0.6;
                if lk.rule { rule(c, place(dm.w as f32), y, dm.w as f32, s, op, cfg.color); y += 8.0 * s; }
            }
        }

        // The time. Modern Bold colours the minutes; Cinematic fades top → bottom.
        let block_w = total_w(&tm) + suf.as_ref().map_or(0.0, |m| m.w as f32 * 1.0);
        let tx = place(block_w);
        if cfg.style == 3 {
            // hours white, ":" + minutes in the chosen colour
            let colon = main.find(':').unwrap_or(2);
            let hm = text_mask_ex(&main[..colon], time_px as i32, weight, face, 0);
            let rest = text_mask_ex(&main[colon..], time_px as i32, weight, face, 0);
            paint_text(c, &hm, tx, y - time_px * 0.07, rgba(255, 255, 255, op), &cfg, s, 1.0);
            paint_text(c, &rest, tx + hm.w as f32 - 2.0 * s, y - time_px * 0.07, rgba(cfg.color[0], cfg.color[1], cfg.color[2], op), &cfg, s, 1.0);
        } else if lk.gradient {
            // glow first, then gradient text
            paint_glow_shadow(c, &tm, tx, y - time_px * 0.07, &cfg, s, col);
            draw_mask_grad(c, &tm, tx, y - time_px * 0.07, rgba(255, 255, 255, op), rgba(((cfg.color[0] as u16 / 2) + 110).min(255) as u8, ((cfg.color[1] as u16 / 2) + 110).min(255) as u8, ((cfg.color[2] as u16 / 2) + 130).min(255) as u8, 0.72 * op));
        } else {
            paint_text(c, &tm, tx, y - time_px * 0.07, col, &cfg, s, 1.0);
        }
        // Seconds + AM/PM beside the time (baseline-aligned, dimmer).
        let mut ex = tx + tm.w as f32;
        if let Some(sm) = &sm {
            let sy = y - time_px * 0.07 + tm.h as f32 - sm.h as f32 - time_px * 0.10;
            paint_text(c, sm, ex, sy, rgba(cfg.color[0], cfg.color[1], cfg.color[2], 0.55 * op), &cfg, s, 0.5);
            ex += sm.w as f32;
        }
        if let Some(sf) = &suf {
            let sy = y - time_px * 0.07 + tm.h as f32 * 0.22;
            paint_text(c, sf, ex + 4.0 * s, sy, rgba(cfg.color[0], cfg.color[1], cfg.color[2], 0.7 * op), &cfg, s, 0.5);
        }
        y += time_h * 0.86;

        // Date below.
        if !lk.date_above {
            if let Some(dm) = &dm {
                y += gap;
                if lk.rule { rule(c, place(dm.w as f32), y - gap * 0.4, dm.w as f32, s, op, cfg.color); y += rule_h * 0.5; }
                let x = place(dm.w as f32 + pill_pad.0 * 2.0);
                if lk.pill {
                    let pr = R { x, y, w: dm.w as f32 + pill_pad.0 * 2.0, h: dm.h as f32 + pill_pad.1 * 2.0 };
                    stroke_rrect(c, pr, pr.h / 2.0, 1.4 * s, |_| rgba(cfg.color[0], cfg.color[1], cfg.color[2], 0.85 * op));
                    draw_mask(c, dm, pr.x + pill_pad.0, pr.y + pill_pad.1, rgba(cfg.color[0], cfg.color[1], cfg.color[2], op));
                } else {
                    paint_text(c, dm, x, y, rgba(cfg.color[0], cfg.color[1], cfg.color[2], 0.72 * op), &cfg, s, 0.35);
                }
            }
        }
        true
    }

    fn interval_ms(&self, cfg: &Value, now: &Now, _audio: bool) -> u32 {
        // Wake right after the next second (or minute) boundary.
        let secs = cfg.get("show_seconds").and_then(|v| v.as_bool()).unwrap_or(false);
        if secs { 1000 - now.ms.min(999) + 8 } else { ((60 - now.second.min(59)) * 1000).saturating_sub(now.ms) + 8 }
    }

    fn blur_radius(&self, cfg: &Value) -> Option<f32> {
        let c: ClockCfg = serde_json::from_value(cfg.clone()).unwrap_or_default();
        c.blur.then(|| if c.style == 2 { 30.0 } else { 22.0 })
    }
}

fn rule(c: &mut Canvas, x: f32, y: f32, w: f32, s: f32, op: f32, col: [u8; 3]) {
    fill_rrect(c, R { x, y, w, h: (1.2 * s).max(1.0) }, 0.6, |_| rgba(col[0], col[1], col[2], 0.5 * op));
}

/// Shadow (soft, dark) and glow (soft, coloured) behind `m`.
fn paint_glow_shadow(c: &mut Canvas, m: &Mask, x: f32, y: f32, cfg: &ClockCfg, s: f32, col: crate::raster::Rgba) {
    if cfg.shadow > 0.02 {
        let r = ((4.0 + 8.0 * cfg.shadow) * s) as i32;
        let b = blur_mask(m, r.max(2));
        draw_mask(c, &b, x - r as f32, y - r as f32 + 3.0 * s, rgba(0, 0, 0, (0.75 * cfg.shadow * cfg.opacity).min(0.9)));
    }
    if cfg.glow > 0.02 {
        let r = ((6.0 + 16.0 * cfg.glow) * s) as i32;
        let b = blur_mask(m, r.max(3));
        draw_mask(c, &b, x - r as f32, y - r as f32, rgba((col.r * 255.0) as u8, (col.g * 255.0) as u8, (col.b * 255.0) as u8, (0.85 * cfg.glow * cfg.opacity).min(0.95)));
    }
}

/// Text with the configured shadow / glow. `k` scales both (small text gets a lighter touch).
fn paint_text(c: &mut Canvas, m: &Mask, x: f32, y: f32, col: crate::raster::Rgba, cfg: &ClockCfg, s: f32, k: f32) {
    let soft = ClockCfg { shadow: cfg.shadow * k.min(1.0), glow: cfg.glow * k.min(1.0), ..cfg.clone() };
    paint_glow_shadow(c, m, x, y, &soft, s, col);
    draw_mask(c, m, x, y, col);
}

// ------------------------------------------------------------------ settings panel

use crate::ui::kit;

const TZ_LABELS: [&str; 27] = ["Múi giờ của máy", "UTC−12", "UTC−11", "UTC−10", "UTC−9", "UTC−8 (Los Angeles)", "UTC−7", "UTC−6", "UTC−5 (New York)", "UTC−4", "UTC−3", "UTC−2", "UTC−1", "UTC+0 (London)", "UTC+1 (Paris)", "UTC+2", "UTC+3 (Moscow)", "UTC+4 (Dubai)", "UTC+5", "UTC+5:30 (Ấn Độ)", "UTC+6", "UTC+7 (Hà Nội)", "UTC+8 (Singapore)", "UTC+9 (Tokyo)", "UTC+10 (Sydney)", "UTC+11", "UTC+12"];
const TZ_MINUTES: [i32; 26] = [-720, -660, -600, -540, -480, -420, -360, -300, -240, -180, -120, -60, 0, 60, 120, 180, 240, 300, 330, 360, 420, 480, 540, 600, 660, 720];

fn customize(ui: &mut Ui, v: &mut Value) -> bool {
    let mut cfg: ClockCfg = serde_json::from_value(v.clone()).unwrap_or_default();
    let before = cfg.clone();
    // Style presets: choosing a style also applies the look it was designed with.
    let mut style_changed = false;
    kit::row(ui, "Kiểu đồng hồ", |ui| {
        let r = kit::combo(ui, "clock_style", cfg.style as usize, &STYLES);
        if let Some(i) = r { cfg = ClockCfg { size: cfg.size, hour24: cfg.hour24, tz: cfg.tz, show_tz: cfg.show_tz, show_seconds: cfg.show_seconds, ..ClockCfg::preset(i as u8) }; style_changed = true; }
        style_changed
    });
    kit::row(ui, "Phông chữ", |ui| {
        let labels: Vec<&str> = FONTS.iter().map(|f| if f.is_empty() { "Theo kiểu" } else { *f }).collect();
        let cur = FONTS.iter().position(|f| *f == cfg.font).unwrap_or(0);
        if let Some(i) = kit::combo(ui, "clock_font", cur, &labels) { cfg.font = FONTS[i].to_string(); true } else { false }
    });
    kit::slider_row(ui, "Cỡ chữ", &mut cfg.size, 40.0..=200.0, |v| format!("{v:.0}%"));
    kit::row(ui, "Màu chữ", |ui| kit::color(ui, &mut cfg.color));
    kit::row(ui, "Căn lề", |ui| { if let Some(i) = kit::segmented(ui, &["Trái", "Giữa", "Phải"], cfg.align as usize) { cfg.align = i as u8; true } else { false } });
    kit::slider_row(ui, "Độ trong suốt", &mut cfg.opacity, 0.15..=1.0, |v| format!("{:.0}%", v * 100.0));
    kit::row(ui, "Làm mờ nền (kính mờ)", |ui| kit::toggle(ui, &mut cfg.blur).changed());
    kit::slider_row(ui, "Đổ bóng", &mut cfg.shadow, 0.0..=1.0, |v| format!("{:.0}%", v * 100.0));
    kit::slider_row(ui, "Phát sáng", &mut cfg.glow, 0.0..=1.0, |v| format!("{:.0}%", v * 100.0));
    kit::row(ui, "Hiện ngày", |ui| kit::toggle(ui, &mut cfg.show_date).changed());
    kit::row(ui, "Hiện giây", |ui| kit::toggle(ui, &mut cfg.show_seconds).changed());
    kit::row(ui, "Định dạng giờ", |ui| { if let Some(i) = kit::segmented(ui, &["12 giờ", "24 giờ"], usize::from(cfg.hour24)) { cfg.hour24 = i == 1; true } else { false } });
    kit::row(ui, "Múi giờ", |ui| {
        let cur = cfg.tz.and_then(|m| TZ_MINUTES.iter().position(|x| *x == m)).map_or(0, |i| i + 1);
        if let Some(i) = kit::combo(ui, "clock_tz", cur, &TZ_LABELS) { cfg.tz = if i == 0 { None } else { Some(TZ_MINUTES[i - 1]) }; true } else { false }
    });
    kit::row(ui, "Hiện tên múi giờ", |ui| kit::toggle(ui, &mut cfg.show_tz).changed());
    if cfg != before { *v = json!(cfg); true } else { false }
}
