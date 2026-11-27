//! The "Nhạc" page: just the music sources. Today that is your own music (played by Vire and shown
//! in the desktop widget); streaming services will be connected here later.

use super::*;
use super::hud;
use crate::model::AppState;

pub struct MusicPage {
    msg: Option<String>,
}

impl MusicPage {
    pub fn new() -> Self { MusicPage { msg: None } }
    pub fn note(&mut self, m: String) { self.msg = Some(m); }
}

/// A streaming service that will be connectable here.
struct Service { name: &'static str, tag: &'static str, color: Color32, glyph: &'static str }

const SERVICES: [Service; 3] = [
    Service { name: "Spotify", tag: "Sắp có", color: Color32::from_rgb(30, 215, 96), glyph: "S" },
    Service { name: "YouTube Music", tag: "Sắp có", color: Color32::from_rgb(255, 64, 64), glyph: "▶" },
    Service { name: "Trình phát khác", tag: "Sắp có · mọi app có điều khiển media của Windows", color: Color32::from_rgb(64, 160, 255), glyph: "◉" },
];

impl App {
    /// Returns true when something the wallpaper engine cares about changed (never, here).
    pub(super) fn music_tab(&mut self, ui: &mut Ui, ctx: &egui::Context, st: &mut AppState) -> bool {
        page_title(ui, "Nhạc", "Chọn nơi nhạc đến. Nhạc hiện trên widget nổi ở desktop.");
        ui.add_space(16.0);
        let n = st.library.songs.len();
        let mut changed = false;
        egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
            let gap = 14.0;
            let full = ui.available_width();

            // ---- your own music (works today)
            let (rect, _) = ui.allocate_exact_size(vec2(full, 156.0), Sense::hover());
            {
                let p = ui.painter();
                hud::neon_panel(p, rect, 20.0, self.pal.card, hud::tone(CYAN), 0.9);
                let c = rect.min + vec2(44.0, 46.0);
                hud::neon_panel(p, Rect::from_center_size(c, vec2(52.0, 52.0)), 14.0, hud::with_alpha(hud::tone(MAGENTA), 50), hud::tone(MAGENTA), 1.0);
                p.text(c, Align2::CENTER_CENTER, "♪", bold(24.0), hud::tone(MAGENTA));
                hud::glow_text(p, rect.min + vec2(88.0, 36.0), Align2::LEFT_CENTER, if hud::soft() { "Nhạc của bạn" } else { "NHẠC CỦA BẠN" }, bold(17.0), hud::tone(CYAN));
                p.text(rect.min + vec2(88.0, 58.0), Align2::LEFT_CENTER, if hud::soft() { format!("Đang dùng · {n} bài trong thư viện") } else { format!("> ĐANG DÙNG · {n} BÀI TRONG THƯ VIỆN") }, FontId::monospace(11.5), hud::tone(GREEN));
                if !hud::soft() { p.text(rect.right_top() + vec2(-22.0, 18.0), Align2::RIGHT_CENTER, format!("SRC//LOCAL {}", hud::hex(n as u32, 4)), FontId::monospace(10.0), hud::with_alpha(hud::tone(CYAN), 130)); }
                hud::barcode(p, Rect::from_min_size(rect.right_top() + vec2(-120.0, 32.0), vec2(98.0, 7.0)), 5, hud::with_alpha(hud::tone(MAGENTA), 150));
            }
            let mut inner = ui.new_child(egui::UiBuilder::new().max_rect(Rect::from_min_size(rect.min + vec2(24.0, 88.0), vec2(full - 48.0, 56.0))).layout(Layout::top_down(Align::Min)));
            inner.horizontal(|ui| {
                if pill_button(ui, Some(Icon::Plus), "Thêm nhạc", true).on_hover_text("Hoặc kéo thả file âm thanh vào cửa sổ").clicked() { self.pick_dialog_songs(ctx); }
                if pill_button(ui, Some(Icon::Folder), "Thư mục", false).on_hover_text(format!("Bỏ nhạc vào đây, Vire tự nạp:\n{}", crate::model::music_dir().display())).clicked() {
                    open_url(&crate::model::music_dir().to_string_lossy());
                }
                ui.add_space(10.0);
                let mut on = st.settings.w_enabled;
                // Switching it on shows it right away (even before any song plays); the widget's own
                // right-click menu can still make it appear only while music is playing.
                if toggle(ui, &mut on).changed() { st.settings.w_enabled = on; if on { st.settings.w_always = true; } st.save(); }
                ui.label(RichText::new("Hiện widget nhạc trên desktop").size(13.0).color(subtle()));
            });

            // ---- speaker: point at it for the volume bar, click it to mute (same control as on the Wallpaper page)
            {
                let btn = Rect::from_center_size(pos2(rect.right() - 44.0, rect.top() + 116.0), vec2(38.0, 38.0));
                let muted = st.settings.is_muted;
                let mut vd = self.vol_drag;
                let (toggled, set_vol) = speaker_control(ui, btn, muted, st.settings.volume, &mut vd, "music_speaker", 0);
                self.vol_drag = vd;
                if toggled { st.settings.is_muted = !muted; st.save(); changed = true; }
                if let Some(v) = set_vol {
                    st.settings.volume = v; st.settings.pl_volume = v;
                    if v > 0.0 { st.settings.is_muted = false; }
                    st.save();
                    self.engine.player(crate::player::PCmd::Volume(v));
                    changed = true;
                }
            }            ui.add_space(gap);

            // ---- services to connect later
            let w = (full - gap * 2.0) / 3.0;
            ui.horizontal_top(|ui| {
                ui.spacing_mut().item_spacing.x = gap;
                for s in &SERVICES {
                    let (rect, _) = ui.allocate_exact_size(vec2(w, 150.0), Sense::hover());
                    let p = ui.painter();
                    hud::neon_panel(p, rect, 16.0, self.pal.card, hud::with_alpha(s.color, 150), 0.4);
                    let c = rect.min + vec2(40.0, 42.0);
                    hud::neon_panel(p, Rect::from_center_size(c, vec2(46.0, 46.0)), 12.0, hud::with_alpha(s.color, 40), hud::with_alpha(s.color, 200), 0.5);
                    p.text(c, Align2::CENTER_CENTER, s.glyph, bold(20.0), hud::with_alpha(s.color, 230));
                    p.text(rect.min + vec2(78.0, 34.0), Align2::LEFT_CENTER, elide(p, &up(s.name), bold(14.0), w - 96.0), bold(14.0), subtle());
                    let tag = elide(p, s.tag, FontId::monospace(10.5), w - 36.0);
                    p.text(rect.min + vec2(20.0, 82.0), Align2::LEFT_CENTER, if hud::soft() { tag.to_string() } else { format!("// {tag}") }, FontId::monospace(10.5), muted());
                    let b = Rect::from_min_size(pos2(rect.left() + 20.0, rect.bottom() - 46.0), vec2(112.0, 30.0));
                    hud::neon_panel(p, b, 8.0, Color32::TRANSPARENT, hud::with_alpha(hud::tone(CYAN), 70), 0.0);
                    p.text(b.center(), Align2::CENTER_CENTER, if hud::soft() { "Kết nối" } else { "KẾT NỐI [ LOCKED ]" }, FontId::monospace(9.5), hud::with_alpha(hud::tone(CYAN), 110));
                }
            });
            if let Some(m) = self.mp.msg.clone() { ui.add_space(8.0); ui.label(RichText::new(format!("✓ {m}")).size(12.5).color(hud::tone(GREEN))); }
        });
        changed
    }
}
