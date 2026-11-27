//! The "Nhạc" page: what is playing now (cover, progress, controls), your song list, and the
//! sources. Today the source is your own music (played by Vire and shown in the desktop widget);
//! streaming services will be connected here later.

use super::player_panel::{fmt_time, ibtn, ic_pause, ic_play, ic_skip};
use super::*;
use super::hud;
use crate::model::AppState;
use crate::player::PCmd;

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
    /// Returns true when something the wallpaper engine cares about changed.
    pub(super) fn music_tab(&mut self, ui: &mut Ui, ctx: &egui::Context, st: &mut AppState) -> bool {
        page_title(ui, "Nhạc", "Nghe nhạc của bạn, chọn bài ngay tại đây. Nhạc cũng hiện trên widget nổi ở desktop.");
        ui.add_space(14.0);
        let mut changed = false;
        let soft = hud::soft();
        let acc = hud::tone(CYAN);
        let status = self.engine.status.clone();
        let (cur, pos, dur, playing, qlen) = { let v = status.player.lock().unwrap(); (v.current.clone(), v.pos, v.dur, v.playing, v.queue.len()) };
        let song = cur.as_ref().and_then(|c| st.song(c)).cloned();
        let sel = st.selected().cloned();
        let all: Vec<std::path::PathBuf> = st.library.songs.iter().map(|s| s.path.clone()).collect();

        egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
            let full = ui.available_width();
            let gap = 14.0;

            // ------------------------------------------------------------ now playing
            let (r, _) = ui.allocate_exact_size(vec2(full, 214.0), Sense::hover());
            // The wallpaper's own picture, darkened, is the backdrop — so the page takes its colours.
            self.paint_media(ui, ctx, sel.as_ref(), r, 0.0, 0.0);
            {
                let p = ui.painter();
                p.rect_filled(r, if soft { 14.0 } else { 0.0 }, Color32::from_black_alpha(if soft { 176 } else { 205 }));
                p.rect_filled(r, if soft { 14.0 } else { 0.0 }, hud::with_alpha(acc, 22));
                if soft { p.rect_stroke(r, 14.0, Stroke::new(1.0_f32, Color32::from_white_alpha(34))); } else { hud::neon_panel(p, r, 14.0, Color32::TRANSPARENT, acc, 0.9); }
            }
            let cover = Rect::from_min_size(r.min + vec2(24.0, 26.0), vec2(162.0, 162.0));
            self.paint_media(ui, ctx, sel.as_ref(), cover, 0.0, 0.6);
            {
                let p = ui.painter();
                p.rect_stroke(cover, if soft { 12.0 } else { 0.0 }, Stroke::new(1.0_f32, hud::with_alpha(Color32::WHITE, 70)));
                if cur.is_none() { p.text(cover.center(), Align2::CENTER_CENTER, "♪", bold(56.0), Color32::from_white_alpha(190)); }
            }
            let x0 = cover.right() + 28.0;
            let rw = r.right() - 28.0 - x0;
            {
                let p = ui.painter();
                p.text(pos2(x0, r.top() + 36.0), Align2::LEFT_CENTER, if soft { "Đang phát" } else { "NOW//PLAYING" }, if soft { semi(12.5) } else { FontId::monospace(10.5) }, hud::with_alpha(acc.lerp_to_gamma(Color32::WHITE, 0.4), 230));
                if playing {
                    let t = ui.input(|i| i.time) as f32;
                    for k in 0..5 {
                        let h = 4.0 + 13.0 * (((t * (4.5 + k as f32 * 1.1) + k as f32 * 1.7).sin() * 0.5) + 0.5);
                        p.rect_filled(Rect::from_min_size(pos2(x0 + 92.0 + k as f32 * 6.0, r.top() + 36.0 + 8.0 - h), vec2(3.5, h)), 1.5, acc);
                    }
                    ctx.request_repaint_after(std::time::Duration::from_millis(60));
                }
                let title = song.as_ref().map(|s| s.title.clone()).unwrap_or_else(|| "Chưa phát bài nào".into());
                let artist = song.as_ref().map(|s| if s.artist.is_empty() { "Nghệ sĩ không rõ".to_string() } else { s.artist.clone() }).unwrap_or_else(|| "Bấm ▶ để nghe nhạc của bạn".into());
                p.text(pos2(x0, r.top() + 70.0), Align2::LEFT_CENTER, elide(p, &title, bold(27.0), rw), bold(27.0), Color32::WHITE);
                p.text(pos2(x0, r.top() + 98.0), Align2::LEFT_CENTER, elide(p, &artist, semi(14.0), rw), semi(14.0), Color32::from_white_alpha(185));
            }
            // Progress.
            let sy = r.top() + 132.0;
            let bar = Rect::from_min_max(pos2(x0, sy - 9.0), pos2(x0 + rw, sy + 9.0));
            let resp = ui.interact(bar, ui.id().with("mp_seek"), Sense::click_and_drag()).on_hover_cursor(egui::CursorIcon::PointingHand);
            let at = |x: f32| ((x - bar.left()) / bar.width()).clamp(0.0, 1.0);
            if (resp.drag_stopped() || resp.clicked()) && dur > 0.0 {
                if let Some(q) = resp.interact_pointer_pos() { self.engine.player(PCmd::Seek(at(q.x) as f64 * dur)); }
            }
            let frac = if dur > 0.0 { (pos / dur) as f32 } else { 0.0 }.clamp(0.0, 1.0);
            let th = if resp.hovered() || resp.dragged() { 7.0 } else { 4.0 };
            {
                let p = ui.painter();
                let track = Rect::from_min_max(pos2(bar.left(), sy - th / 2.0), pos2(bar.right(), sy + th / 2.0));
                p.rect_filled(track, th / 2.0, Color32::from_white_alpha(46));
                p.rect_filled(Rect::from_min_max(track.min, pos2(track.left() + track.width() * frac, track.bottom())), th / 2.0, acc);
                if resp.hovered() || resp.dragged() { p.circle_filled(pos2(track.left() + track.width() * frac, sy), 7.0, Color32::WHITE); }
                p.text(pos2(bar.left(), sy + 17.0), Align2::LEFT_CENTER, fmt_time(pos), FontId::monospace(10.5), Color32::from_white_alpha(160));
                p.text(pos2(bar.right(), sy + 17.0), Align2::RIGHT_CENTER, fmt_time(dur), FontId::monospace(10.5), Color32::from_white_alpha(160));
            }
            // Transport.
            let cy = r.bottom() - 30.0;
            let (rp, hp) = ibtn(ui, "mp_prev", pos2(x0 + 22.0, cy), 36.0, "Bài trước");
            ic_skip(ui.painter(), pos2(x0 + 22.0, cy), 15.0, if hp { Color32::WHITE } else { Color32::from_white_alpha(215) }, false);
            if rp.clicked() { self.engine.player(PCmd::Prev); }
            let pr = Rect::from_center_size(pos2(x0 + 76.0, cy), vec2(46.0, 46.0));
            let presp = ui.interact(pr, ui.id().with("mp_play"), Sense::click()).on_hover_cursor(egui::CursorIcon::PointingHand).on_hover_text(if playing { "Tạm dừng" } else { "Phát" });
            let glyph = if soft {
                let k = ui.ctx().animate_bool_with_time(presp.id, presp.hovered(), 0.15);
                ui.painter().circle_filled(pr.center() + vec2(0.0, 3.0), 24.0, Color32::from_black_alpha(70));
                ui.painter().circle_filled(pr.center(), 23.0 + 2.0 * k, hud::accent().lerp_to_gamma(Color32::WHITE, 0.55));
                hud::C_CHARCOAL
            } else {
                hud::neon_panel(ui.painter(), pr, 13.0, acc, acc, 1.0);
                BG0
            };
            if playing { ic_pause(ui.painter(), pr.center(), 18.0, glyph); } else { ic_play(ui.painter(), pr.center() + vec2(1.5, 0.0), 19.0, glyph); }
            if presp.clicked() {
                if cur.is_none() && qlen == 0 {
                    if all.is_empty() { self.mp.note("Chưa có bài nhạc nào — bấm “Thêm nhạc” bên dưới".into()); } else { self.engine.player(PCmd::SetQueue(all.clone(), 0)); }
                } else { self.engine.player(PCmd::Toggle); }
            }
            let (rn, hn) = ibtn(ui, "mp_next", pos2(x0 + 130.0, cy), 36.0, "Bài tiếp");
            ic_skip(ui.painter(), pos2(x0 + 130.0, cy), 15.0, if hn { Color32::WHITE } else { Color32::from_white_alpha(215) }, true);
            if rn.clicked() { self.engine.player(PCmd::Next); }
            // Volume: point at the speaker for the bar, click to mute.
            {
                let btn = Rect::from_center_size(pos2(r.right() - 44.0, cy), vec2(38.0, 38.0));
                let muted_now = st.settings.is_muted;
                let mut vd = self.vol_drag;
                let (toggled, set_vol) = speaker_control(ui, btn, muted_now, st.settings.volume, &mut vd, "music_speaker", 0);
                self.vol_drag = vd;
                if toggled { st.settings.is_muted = !muted_now; st.save(); changed = true; }
                if let Some(v) = set_vol {
                    st.settings.volume = v; st.settings.pl_volume = v;
                    if v > 0.0 { st.settings.is_muted = false; }
                    st.save();
                    self.engine.player(PCmd::Volume(v));
                    changed = true;
                }
            }

            // ------------------------------------------------------------ actions
            ui.add_space(gap);
            ui.horizontal(|ui| {
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
            if let Some(m) = self.mp.msg.clone() { ui.add_space(6.0); ui.label(RichText::new(format!("✓ {m}")).size(12.5).color(hud::tone(GREEN))); }

            // ------------------------------------------------------------ song list
            ui.add_space(gap);
            section_label(ui, &format!("Thư viện nhạc  ·  {} bài", st.library.songs.len()));
            ui.add_space(6.0);
            if st.library.songs.is_empty() {
                ui.label(RichText::new("Chưa có bài nào — bấm “Thêm nhạc” hoặc kéo thả file âm thanh vào đây.").size(13.0).color(muted()));
            }
            let songs: Vec<(std::path::PathBuf, String, String, f64, bool)> = st.library.songs.iter().map(|s| (s.path.clone(), s.title.clone(), s.artist.clone(), s.duration, s.liked)).collect();
            for (i, (path, title, artist, d, liked)) in songs.iter().enumerate() {
                let (row, resp) = ui.allocate_exact_size(vec2(full, 48.0), Sense::click());
                let t = ui.ctx().animate_bool_with_time(resp.id, resp.hovered(), 0.1);
                let is_cur = cur.as_ref() == Some(path);
                let p = ui.painter();
                let rad = if soft { 12.0 } else { 0.0 };
                if is_cur { p.rect_filled(row, rad, hud::with_alpha(acc, 38)); p.rect_stroke(row, rad, Stroke::new(1.0_f32, hud::with_alpha(acc, 120))); }
                else if t > 0.0 { p.rect_filled(row, rad, Color32::from_white_alpha((16.0 * t) as u8)); }
                let c = pos2(row.left() + 30.0, row.center().y);
                if is_cur && playing {
                    let tm = ui.input(|i| i.time) as f32;
                    for k in 0..3 {
                        let h = 5.0 + 11.0 * (((tm * (5.0 + k as f32 * 1.3) + k as f32 * 1.7).sin() * 0.5) + 0.5);
                        p.rect_filled(Rect::from_min_size(pos2(c.x - 8.0 + k as f32 * 6.0, c.y + 8.0 - h), vec2(3.5, h)), 1.5, acc);
                    }
                } else if t > 0.3 {
                    ic_play(p, c + vec2(1.0, 0.0), 13.0, Color32::WHITE);
                } else {
                    p.text(c, Align2::CENTER_CENTER, format!("{}", i + 1), FontId::monospace(12.5), if is_cur { acc } else { muted() });
                }
                let tw = full - 190.0;
                p.text(pos2(row.left() + 62.0, row.center().y - 8.0), Align2::LEFT_CENTER, elide(p, title, bold(14.5), tw), bold(14.5), if is_cur { Color32::WHITE } else { tx() });
                p.text(pos2(row.left() + 62.0, row.center().y + 11.0), Align2::LEFT_CENTER, elide(p, if artist.is_empty() { "Nghệ sĩ không rõ" } else { artist }, semi(12.0), tw), semi(12.0), muted());
                if *liked { p.text(pos2(row.right() - 86.0, row.center().y), Align2::CENTER_CENTER, "♥", bold(15.0), hud::tone(MAGENTA)); }
                if *d > 0.0 { p.text(pos2(row.right() - 22.0, row.center().y), Align2::RIGHT_CENTER, fmt_time(*d), FontId::monospace(12.0), muted()); }
                if resp.on_hover_cursor(egui::CursorIcon::PointingHand).clicked() {
                    if is_cur { self.engine.player(PCmd::Toggle); } else { self.engine.player(PCmd::SetQueue(all.clone(), i)); }
                }
            }

            // ------------------------------------------------------------ services to connect later
            ui.add_space(gap + 6.0);
            section_label(ui, "Kết nối dịch vụ nhạc  ·  sắp có");
            ui.add_space(6.0);
            let w = (full - gap * 2.0) / 3.0;
            ui.horizontal_top(|ui| {
                ui.spacing_mut().item_spacing.x = gap;
                for s in &SERVICES {
                    let (rect, _) = ui.allocate_exact_size(vec2(w, 132.0), Sense::hover());
                    let p = ui.painter();
                    // Tinted with the service's own colour, so each card has a character of its own.
                    hud::neon_panel(p, rect, 16.0, hud::with_alpha(s.color, if soft { 30 } else { 18 }), hud::with_alpha(s.color, 170), 0.5);
                    let c = rect.min + vec2(40.0, 42.0);
                    hud::neon_panel(p, Rect::from_center_size(c, vec2(46.0, 46.0)), 12.0, hud::with_alpha(s.color, 70), hud::with_alpha(s.color, 220), 0.5);
                    p.text(c, Align2::CENTER_CENTER, s.glyph, bold(20.0), Color32::WHITE);
                    p.text(rect.min + vec2(78.0, 34.0), Align2::LEFT_CENTER, elide(p, &up(s.name), bold(14.5), w - 96.0), bold(14.5), tx());
                    let tag = elide(p, s.tag, FontId::monospace(10.5), w - 36.0);
                    p.text(rect.min + vec2(20.0, 84.0), Align2::LEFT_CENTER, if soft { tag.to_string() } else { format!("// {tag}") }, FontId::monospace(10.5), subtle());
                    let b = Rect::from_min_size(pos2(rect.left() + 20.0, rect.bottom() - 34.0), vec2(96.0, 24.0));
                    hud::neon_panel(p, b, 8.0, hud::with_alpha(s.color, 40), hud::with_alpha(s.color, 150), 0.0);
                    p.text(b.center(), Align2::CENTER_CENTER, if soft { "Kết nối" } else { "KẾT NỐI [ LOCKED ]" }, FontId::monospace(10.0), hud::with_alpha(Color32::WHITE, 190));
                }
            });
            ui.add_space(10.0);
        });
        changed
    }
}
