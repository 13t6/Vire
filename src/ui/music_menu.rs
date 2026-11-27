//! The song picker used by the wallpaper music button and the right-click menu.

use super::*;
use std::path::PathBuf;

/// A choice made in the music list.
pub(super) enum MusicAct { None, Add, Toggle(PathBuf), Clear }

#[derive(Clone, Copy, PartialEq)]
pub(super) enum RowKind { Song, Off, Add }

/// One row of the music picker, "cover tile" style: a coloured rounded tile, the title in bold, the
/// artist underneath, and — on the song that is on — a little animated equaliser. Returns true on click.
pub(super) fn music_row(ui: &mut Ui, kind: RowKind, selected: bool, title: &str, sub: &str, seed: &str) -> bool {
    let w = ui.available_width();
    let (rect, resp) = ui.allocate_exact_size(vec2(w, 52.0), Sense::click());
    let t = ui.ctx().animate_bool_with_time(resp.id, resp.hovered(), 0.12);
    let p = ui.painter();
    let rr = Rounding::ZERO;
    if selected {
        p.rect(rect, rr, Color32::from_rgba_unmultiplied(124, 104, 255, 52), Stroke::new(1.0_f32, Color32::from_rgba_unmultiplied(190, 176, 255, 90)));
    } else if t > 0.0 {
        p.rect_filled(rect, rr, Color32::from_white_alpha((18.0 * t) as u8));
    }
    // Tile.
    let tile = Rect::from_min_size(rect.min + vec2(8.0, 8.0), vec2(36.0, 36.0));
    match kind {
        RowKind::Song => {
            let mut h: u32 = 2166136261;
            for b in seed.bytes() { h ^= b as u32; h = h.wrapping_mul(16777619); }
            let hue = (h % 360) as f32 / 360.0;
            let top: Color32 = egui::ecolor::Hsva::new(hue, 0.55, 0.95, 1.0).into();
            let bot: Color32 = egui::ecolor::Hsva::new((hue + 0.12) % 1.0, 0.75, 0.62, 1.0).into();
            let mut mesh = egui::Mesh::default();
            // Rounded tile = gradient rect with a rounded mask on top: draw base rounded, then a lighter top half.
            p.rect_filled(tile, Rounding::ZERO, bot);
            p.rect_filled(Rect::from_min_max(tile.min, pos2(tile.max.x, tile.center().y)), Rounding::ZERO, top.gamma_multiply(0.85));
            let _ = &mut mesh;
            p.text(tile.center(), Align2::CENTER_CENTER, "♪", semi(17.0), Color32::from_white_alpha(235));
        }
        RowKind::Off => {
            p.rect_filled(tile, Rounding::ZERO, Color32::from_white_alpha(if selected { 40 } else { 22 }));
            p.text(tile.center(), Align2::CENTER_CENTER, "♪", semi(17.0), Color32::from_white_alpha(150));
            p.line_segment([tile.left_bottom() + vec2(9.0, -9.0), tile.right_top() + vec2(-9.0, 9.0)], Stroke::new(2.0_f32, Color32::from_white_alpha(190)));
        }
        RowKind::Add => {
            p.rect_stroke(tile, Rounding::ZERO, Stroke::new(1.4_f32, Color32::from_rgba_unmultiplied(196, 184, 255, 140)));
            p.text(tile.center(), Align2::CENTER_CENTER, "+", FontId::proportional(22.0), LAVENDER);
        }
    }
    let tx = tile.right() + 12.0;
    let right_pad = if selected && kind == RowKind::Song { 38.0 } else { 14.0 };
    let max_w = rect.right() - tx - right_pad;
    let col = if kind == RowKind::Add { LAVENDER } else { Color32::WHITE };
    let title = elide(p, title, bold(13.5), max_w);
    if sub.is_empty() {
        p.text(pos2(tx, rect.center().y), Align2::LEFT_CENTER, title, bold(13.5), col);
    } else {
        p.text(pos2(tx, rect.center().y - 8.0), Align2::LEFT_CENTER, title, bold(13.5), col);
        let sub = elide(p, sub, FontId::proportional(11.5), max_w);
        p.text(pos2(tx, rect.center().y + 9.0), Align2::LEFT_CENTER, sub, FontId::proportional(11.5), Color32::from_white_alpha(140));
    }
    // Equaliser on the song that is on.
    if selected && kind == RowKind::Song {
        let time = ui.input(|i| i.time) as f32;
        let base = pos2(rect.right() - 30.0, rect.center().y + 8.0);
        for k in 0..3 {
            let hgt = 5.0 + 11.0 * ((time * (5.0 + k as f32 * 1.7) + k as f32 * 1.9).sin() * 0.5 + 0.5);
            ui.painter().rect_filled(Rect::from_min_size(pos2(base.x + k as f32 * 6.0, base.y - hgt), vec2(3.5, hgt)), Rounding::ZERO, LAVENDER);
        }
        ui.ctx().request_repaint_after(std::time::Duration::from_millis(60));
    }
    resp.on_hover_cursor(egui::CursorIcon::PointingHand).clicked()
}

/// Music picker for a wallpaper (shared by the card's right-click menu and the hero's music
/// button): tick the songs of its own playlist, or none. Nothing is deleted from here.
pub(super) fn music_menu(ui: &mut Ui, songs: &[crate::model::Song], current: &[PathBuf]) -> MusicAct {
    let mut act = MusicAct::None;
    ui.set_width(300.0);
    ui.spacing_mut().item_spacing.y = 3.0;
    ui.add_space(6.0);
    ui.label(RichText::new("Nhạc cho hình nền").font(bold(15.0)).color(Color32::WHITE));
    ui.label(RichText::new("Chọn nhiều bài: chúng phát lần lượt và lặp lại").size(12.0).color(muted()));
    ui.add_space(4.0);
    if music_row(ui, RowKind::Off, current.is_empty(), "Không phát nhạc", "Chỉ xem hình nền", "off") && !current.is_empty() { act = MusicAct::Clear; }
    egui::ScrollArea::vertical().max_height(236.0).auto_shrink([true, true]).show(ui, |ui| {
        for s in songs {
            let on = current.contains(&s.path);
            let sub = if s.artist.is_empty() { s.album.clone() } else { s.artist.clone() };
            let sub = if sub.is_empty() { "Bài của bạn".to_string() } else { sub };
            if music_row(ui, RowKind::Song, on, &s.title, &sub, &s.title) { act = MusicAct::Toggle(s.path.clone()); }
        }
    });
    if music_row(ui, RowKind::Add, false, "Thêm bài từ máy", "MP3, FLAC, M4A, WAV…", "add") { act = MusicAct::Add; }
    ui.add_space(4.0);
    // Picking songs keeps the list open (tick several); adding a file closes it.
    if matches!(act, MusicAct::Add) { ui.close_menu(); }
    act
}
