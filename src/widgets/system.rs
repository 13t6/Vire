//! System widget: CPU, memory, disk and battery as four calm bars on a glass card. Everything is
//! read from Windows' own counters (GetSystemTimes, GlobalMemoryStatusEx, GetDiskFreeSpaceEx,
//! GetSystemPowerStatus) once a second.

use super::*;
use crate::raster::{draw_mask, fill_rrect, rgba, text_mask_ex, stroke_rrect, R};
use eframe::egui::Ui;
use serde_json::json;
use windows::core::w;
use windows::Win32::Foundation::FILETIME;
use windows::Win32::Storage::FileSystem::GetDiskFreeSpaceExW;
use windows::Win32::System::Power::{GetSystemPowerStatus, SYSTEM_POWER_STATUS};
use windows::Win32::System::SystemInformation::{GlobalMemoryStatusEx, MEMORYSTATUSEX};
use windows::Win32::System::Threading::GetSystemTimes;

pub const INFO: KindInfo = KindInfo {
    id: "system",
    name: "Hệ thống",
    desc: "CPU, RAM, ổ đĩa và pin trên một thẻ kính, cập nhật mỗi giây.",
    category: Category::System,
    default_size: (300, 190),
    min_size: (200, 120),
    defaults: || json!({ "opacity": 1.0, "blur": true, "accent": [120, 200, 255] }),
    create: || Box::new(SysWidget { prev: None }),
    customize,
};

struct SysWidget { prev: Option<(u64, u64, u64)> }

fn ft(f: FILETIME) -> u64 { (f.dwHighDateTime as u64) << 32 | f.dwLowDateTime as u64 }

impl SysWidget {
    /// CPU load 0..1 since the previous call.
    fn cpu(&mut self) -> f32 {
        let (mut i, mut k, mut u) = (FILETIME::default(), FILETIME::default(), FILETIME::default());
        if unsafe { GetSystemTimes(Some(&mut i), Some(&mut k), Some(&mut u)) }.is_err() { return 0.0; }
        let now = (ft(i), ft(k), ft(u));
        let r = match self.prev {
            Some(p) => {
                let (di, dk, du) = (now.0.saturating_sub(p.0), now.1.saturating_sub(p.1), now.2.saturating_sub(p.2));
                let total = (dk + du).max(1) as f32; // kernel time already includes idle time
                (1.0 - di as f32 / total).clamp(0.0, 1.0)
            }
            None => 0.0,
        };
        self.prev = Some(now);
        r
    }
}

fn ram() -> (f32, f32, f32) {
    let mut m = MEMORYSTATUSEX { dwLength: std::mem::size_of::<MEMORYSTATUSEX>() as u32, ..Default::default() };
    if unsafe { GlobalMemoryStatusEx(&mut m) }.is_err() { return (0.0, 0.0, 0.0); }
    let gb = |b: u64| b as f32 / 1073741824.0;
    (m.dwMemoryLoad as f32 / 100.0, gb(m.ullTotalPhys - m.ullAvailPhys), gb(m.ullTotalPhys))
}

fn disk() -> (f32, f32, f32) {
    let (mut free, mut total, mut _f) = (0u64, 0u64, 0u64);
    if unsafe { GetDiskFreeSpaceExW(w!("C:\\"), Some(&mut free), Some(&mut total), Some(&mut _f)) }.is_err() || total == 0 { return (0.0, 0.0, 0.0); }
    let gb = |b: u64| b as f32 / 1073741824.0;
    (1.0 - free as f32 / total as f32, gb(total - free), gb(total))
}

/// (charge 0..1, charging) — None on a PC without a battery.
fn battery() -> Option<(f32, bool)> {
    let mut s = SYSTEM_POWER_STATUS::default();
    if unsafe { GetSystemPowerStatus(&mut s) }.is_err() || s.BatteryFlag == 128 || s.BatteryLifePercent == 255 { return None; }
    Some((s.BatteryLifePercent as f32 / 100.0, s.ACLineStatus == 1))
}

impl Widget for SysWidget {
    fn render(&mut self, ctx: &Ctx, cfg: &Value, c: &mut Canvas) -> bool {
        let op = cfg.get("opacity").and_then(|v| v.as_f64()).unwrap_or(1.0) as f32;
        let ac = cfg.get("accent").and_then(|v| serde_json::from_value::<[u8; 3]>(v.clone()).ok()).unwrap_or([120, 200, 255]);
        let s = ctx.scale;
        let (w, h) = (c.w as f32, c.h as f32);
        let card = R { x: 4.0 * s, y: 4.0 * s, w: w - 8.0 * s, h: h - 8.0 * s };
        let rad = 22.0 * s;
        fill_rrect(c, card, rad, |fy| rgba(255, 255, 255, (0.16 - 0.07 * fy) * op));
        stroke_rrect(c, card, rad, 1.2 * s, |fy| rgba(255, 255, 255, (0.50 - 0.32 * fy) * op));

        let cpu = self.cpu();
        let (rp, ru, rt) = ram();
        let (dp, du, dt) = disk();
        let mut rows: Vec<(&str, f32, String)> = vec![
            ("CPU", cpu, format!("{:.0}%", cpu * 100.0)),
            ("RAM", rp, format!("{ru:.1} / {rt:.0} GB")),
            ("Ổ C", dp, format!("{du:.0} / {dt:.0} GB")),
        ];
        if let Some((b, chg)) = battery() { rows.push(("Pin", b, format!("{:.0}%{}", b * 100.0, if chg { " ⚡" } else { "" }))); }

        let pad = 20.0 * s;
        let n = rows.len() as f32;
        let row_h = (card.h - 2.0 * pad * 0.8) / n;
        let font = ((row_h * 0.30).clamp(11.0, 17.0 * s)) as i32;
        for (i, (label, v, val)) in rows.iter().enumerate() {
            let y = card.y + pad * 0.8 + i as f32 * row_h;
            let lm = text_mask_ex(label, font, 600, "Segoe UI", 0);
            let vm = text_mask_ex(val, font, 400, "Segoe UI", 0);
            draw_mask(c, &lm, card.x + pad, y, rgba(255, 255, 255, 0.92 * op));
            draw_mask(c, &vm, card.x + card.w - pad - vm.w as f32, y, rgba(255, 255, 255, 0.70 * op));
            let by = y + lm.h as f32 + 5.0 * s;
            let bh = (6.0 * s).max(4.0);
            let bw = card.w - 2.0 * pad;
            fill_rrect(c, R { x: card.x + pad, y: by, w: bw, h: bh }, bh / 2.0, |_| rgba(255, 255, 255, 0.16 * op));
            // Green → amber → red as the load grows (battery: the other way round).
            let load = if *label == "Pin" { 1.0 - v } else { *v };
            let col = if load < 0.6 { lerp_rgb(ac, [255, 214, 90], load / 0.6) } else { lerp_rgb([255, 214, 90], [255, 90, 80], (load - 0.6) / 0.4) };
            let fw = (bw * v.clamp(0.0, 1.0)).max(bh);
            fill_rrect(c, R { x: card.x + pad, y: by, w: fw, h: bh }, bh / 2.0, |_| rgba(col[0], col[1], col[2], 0.95 * op));
        }
        true
    }

    fn interval_ms(&self, _cfg: &Value, _now: &Now, _audio: bool) -> u32 { 1000 }

    fn blur_radius(&self, cfg: &Value) -> Option<f32> {
        cfg.get("blur").and_then(|v| v.as_bool()).unwrap_or(true).then_some(22.0)
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
