//! Calendar widget: the current month on a glass card, today highlighted, weeks start on Monday.
//! Draws once a minute (the day can only change at midnight).

use super::*;
use crate::raster::{draw_mask, fill_rrect, rgba, stroke_rrect, text_mask_ex, R};
use eframe::egui::Ui;
use serde_json::json;

pub const INFO: KindInfo = KindInfo {
    id: "calendar",
    name: "Lịch tháng",
    desc: "Lịch của tháng hiện tại, ngày hôm nay được tô sáng.",
    category: Category::Productivity,
    default_size: (300, 280),
    min_size: (220, 200),
    defaults: || json!({ "opacity": 1.0, "blur": true, "accent": [255, 120, 100] }),
    create: || Box::new(Calendar),
    customize,
};

struct Calendar;

const MONTH_EN: [&str; 12] = ["January", "February", "March", "April", "May", "June", "July", "August", "September", "October", "November", "December"];

fn days_in_month(y: i32, m: u32) -> u32 {
    let (ny, nm) = if m == 12 { (y + 1, 1) } else { (y, m + 1) };
    (days_from_civil(ny, nm, 1) - days_from_civil(y, m, 1)) as u32
}

impl Widget for Calendar {
    fn render(&mut self, ctx: &Ctx, cfg: &Value, c: &mut Canvas) -> bool {
        let op = cfg.get("opacity").and_then(|v| v.as_f64()).unwrap_or(1.0) as f32;
        let ac = cfg.get("accent").and_then(|v| serde_json::from_value::<[u8; 3]>(v.clone()).ok()).unwrap_or([255, 120, 100]);
        let s = ctx.scale;
        let n = ctx.now;
        let (w, h) = (c.w as f32, c.h as f32);
        let card = R { x: 4.0 * s, y: 4.0 * s, w: w - 8.0 * s, h: h - 8.0 * s };
        let rad = 24.0 * s;
        fill_rrect(c, card, rad, |fy| rgba(255, 255, 255, (0.16 - 0.07 * fy) * op));
        stroke_rrect(c, card, rad, 1.2 * s, |fy| rgba(255, 255, 255, (0.50 - 0.32 * fy) * op));

        let vi = ui_vietnamese();
        let title = if vi { format!("Tháng {} · {}", n.month, n.year) } else { format!("{} {}", MONTH_EN[(n.month as usize - 1) % 12], n.year) };
        let pad = 18.0 * s;
        let tpx = ((h * 0.075).clamp(14.0, 24.0 * s)) as i32;
        let tm = text_mask_ex(&title, tpx, 700, "Segoe UI", 0);
        draw_mask(c, &tm, card.x + pad, card.y + pad * 0.7, rgba(255, 255, 255, 0.95 * op));

        let heads: [&str; 7] = if vi { ["T2", "T3", "T4", "T5", "T6", "T7", "CN"] } else { ["Mo", "Tu", "We", "Th", "Fr", "Sa", "Su"] };
        let gx = card.x + pad;
        let gw = card.w - 2.0 * pad;
        let top = card.y + pad * 0.7 + tm.h as f32 + 10.0 * s;
        let cell_w = gw / 7.0;
        let cell_h = ((card.y + card.h - pad * 0.7 - top) / 7.0).max(10.0);
        let px = ((cell_h * 0.46).clamp(10.0, 16.0 * s)) as i32;
        let centered = |c: &mut Canvas, text: &str, cx: f32, cy: f32, weight: i32, col| {
            let m = text_mask_ex(text, px, weight, "Segoe UI", 0);
            draw_mask(c, &m, cx - m.w as f32 / 2.0, cy - m.h as f32 / 2.0, col);
        };
        for (i, hd) in heads.iter().enumerate() {
            centered(c, hd, gx + cell_w * (i as f32 + 0.5), top + cell_h * 0.5, 600, rgba(255, 255, 255, 0.5 * op));
        }
        // Weekday of the 1st: days since 1970-01-01 (a Thursday) → Monday = 0.
        let first = days_from_civil(n.year, n.month, 1);
        let offset = (first + 3).rem_euclid(7) as u32;
        for d in 1..=days_in_month(n.year, n.month) {
            let idx = offset + d - 1;
            let (col, row) = (idx % 7, idx / 7);
            let cx = gx + cell_w * (col as f32 + 0.5);
            let cy = top + cell_h * (row as f32 + 1.5);
            let today = d == n.day;
            if today {
                let r = (cell_h.min(cell_w) * 0.5 - 1.0 * s).max(6.0);
                fill_rrect(c, R { x: cx - r, y: cy - r, w: 2.0 * r, h: 2.0 * r }, r, |_| rgba(ac[0], ac[1], ac[2], 0.95 * op));
            }
            let weekend = col == 6;
            let a = if today { 1.0 } else if weekend { 0.62 } else { 0.9 };
            centered(c, &d.to_string(), cx, cy, if today { 700 } else { 400 }, rgba(255, 255, 255, a * op));
        }
        true
    }

    fn interval_ms(&self, _cfg: &Value, now: &Now, _audio: bool) -> u32 {
        ((60 - now.second.min(59)) * 1000).saturating_sub(now.ms) + 8
    }

    fn blur_radius(&self, cfg: &Value) -> Option<f32> {
        cfg.get("blur").and_then(|v| v.as_bool()).unwrap_or(true).then_some(24.0)
    }
}

fn customize(ui: &mut Ui, v: &mut Value) -> bool {
    let mut changed = false;
    let mut blur = v.get("blur").and_then(|x| x.as_bool()).unwrap_or(true);
    if ui.checkbox(&mut blur, "Nền kính mờ").changed() { v["blur"] = json!(blur); changed = true; }
    let mut op = v.get("opacity").and_then(|x| x.as_f64()).unwrap_or(1.0) as f32;
    if ui.add(eframe::egui::Slider::new(&mut op, 0.3..=1.0).text("Độ đậm")).changed() { v["opacity"] = json!(op); changed = true; }
    changed
}
