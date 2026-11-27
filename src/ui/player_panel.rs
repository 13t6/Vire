//! The compact Spotify-style player beside the wallpaper preview (Wallpaper tab): the wallpaper's
//! own picture as cover art, seek bar, play-order / prev / play / next, the collection list
//! (all songs + favourites) and the experimental desktop-lyrics switch. Playback is the player in
//! `player.rs`; this file only draws and sends commands.

use super::*;
use super::hud;
use crate::model::AppState;
use crate::player::PCmd;
use std::path::PathBuf;

fn tcy() -> Color32 { hud::tone(hud::CYAN) }
fn tye() -> Color32 { hud::tone(hud::YELLOW) }
fn tmg() -> Color32 { hud::tone(hud::MAGENTA) }

fn fmt_time(secs: f64) -> String { let s = secs.max(0.0) as u64; format!("{}:{:02}", s / 60, s % 60) }

// ---- little vector icons (45° HUD style, no rounding)

fn tri(p: &egui::Painter, a: Pos2, b: Pos2, c: Pos2, col: Color32) { p.add(Shape::convex_polygon(vec![a, b, c], col, Stroke::NONE)); }

fn ic_play(p: &egui::Painter, c: Pos2, s: f32, col: Color32) { tri(p, c + vec2(-0.35 * s, -0.5 * s), c + vec2(-0.35 * s, 0.5 * s), c + vec2(0.55 * s, 0.0), col); }
fn ic_pause(p: &egui::Painter, c: Pos2, s: f32, col: Color32) {
    p.rect_filled(Rect::from_center_size(c + vec2(-0.25 * s, 0.0), vec2(0.28 * s, s)), 0.0, col);
    p.rect_filled(Rect::from_center_size(c + vec2(0.25 * s, 0.0), vec2(0.28 * s, s)), 0.0, col);
}
fn ic_skip(p: &egui::Painter, c: Pos2, s: f32, col: Color32, fwd: bool) {
    let d = if fwd { 1.0 } else { -1.0 };
    tri(p, c + vec2(-0.5 * s * d, -0.45 * s), c + vec2(-0.5 * s * d, 0.45 * s), c + vec2(0.35 * s * d, 0.0), col);
    p.rect_filled(Rect::from_center_size(c + vec2(0.5 * s * d, 0.0), vec2(0.14 * s, 0.9 * s)), 0.0, col);
}
fn ic_shuffle(p: &egui::Painter, c: Pos2, col: Color32) {
    let st = Stroke::new(1.6_f32, col);
    p.line_segment([c + vec2(-8.0, 4.0), c + vec2(-3.0, 4.0)], st);
    p.line_segment([c + vec2(-3.0, 4.0), c + vec2(2.0, -4.0)], st);
    p.line_segment([c + vec2(2.0, -4.0), c + vec2(6.0, -4.0)], st);
    p.line_segment([c + vec2(-8.0, -4.0), c + vec2(-3.0, -4.0)], st);
    p.line_segment([c + vec2(-3.0, -4.0), c + vec2(2.0, 4.0)], st);
    p.line_segment([c + vec2(2.0, 4.0), c + vec2(6.0, 4.0)], st);
    tri(p, c + vec2(5.0, -7.0), c + vec2(5.0, -1.0), c + vec2(9.0, -4.0), col);
    tri(p, c + vec2(5.0, 1.0), c + vec2(5.0, 7.0), c + vec2(9.0, 4.0), col);
}
fn ic_repeat(p: &egui::Painter, c: Pos2, col: Color32, one: bool) {
    let st = Stroke::new(1.6_f32, col);
    let r = Rect::from_center_size(c, vec2(16.0, 10.0));
    p.line_segment([r.left_top() + vec2(3.0, 0.0), r.right_top()], st);
    p.line_segment([r.right_top(), r.right_bottom()], st);
    p.line_segment([r.right_bottom(), r.left_bottom() + vec2(3.0, 0.0)], st);
    p.line_segment([r.left_bottom(), r.left_top() + vec2(0.0, 3.0)], st);
    tri(p, r.left_top() + vec2(4.0, -3.0), r.left_top() + vec2(4.0, 3.0), r.left_top() + vec2(8.0, 0.0), col);
    if one { p.text(c, Align2::CENTER_CENTER, "1", FontId::monospace(8.5), col); }
}
fn ic_heart(p: &egui::Painter, c: Pos2, r: f32, col: Color32, filled: bool) {
    let pts: Vec<Pos2> = (0..36).map(|i| {
        let t = i as f32 / 36.0 * std::f32::consts::TAU;
        let (x, y) = (16.0 * t.sin().powi(3), -(13.0 * t.cos() - 5.0 * (2.0 * t).cos() - 2.0 * (3.0 * t).cos() - (4.0 * t).cos()));
        c + vec2(x, y + 1.0) * (r / 17.0)
    }).collect();
    if filled {
        let mut m = egui::Mesh::default();
        m.colored_vertex(c, col);
        for q in &pts { m.colored_vertex(*q, col); }
        let n = pts.len() as u32;
        for i in 0..n { m.add_triangle(0, 1 + i, 1 + (i + 1) % n); }
        p.add(Shape::mesh(m));
    } else { p.add(Shape::closed_line(pts, Stroke::new(1.4_f32, col))); }
}
fn ic_list(p: &egui::Painter, c: Pos2, col: Color32) {
    let st = Stroke::new(1.7_f32, col);
    for dy in [-5.0, 0.0, 5.0] { p.line_segment([c + vec2(-3.0, dy), c + vec2(8.0, dy)], st); p.rect_filled(Rect::from_center_size(c + vec2(-7.0, dy), vec2(2.5, 2.5)), 0.0, col); }
}

/// Flat square icon button; returns the response and whether it is hot.
fn ibtn(ui: &mut Ui, id: &str, center: Pos2, size: f32, tip: &str) -> (Response, bool) {
    let r = Rect::from_center_size(center, vec2(size, size));
    let resp = ui.interact(r, ui.id().with(id), Sense::click()).on_hover_cursor(egui::CursorIcon::PointingHand);
    let hot = resp.hovered();
    (if tip.is_empty() { resp } else { resp.on_hover_text(tip) }, hot)
}

impl App {
    /// The player card. Returns true if the wallpaper engine plan must be re-applied.
    pub(super) fn music_panel(&mut self, ui: &mut Ui, ctx: &egui::Context, st: &mut AppState, r: Rect, item: Option<&WallpaperItem>) -> bool {
        let mut changed = false;
        let status = self.engine.status.clone();
        let (cur, pos, dur, playing, shuffle, repeat, qlen, err) = {
            let v = status.player.lock().unwrap();
            (v.current.clone(), v.pos, v.dur, v.playing, v.shuffle, v.repeat, v.queue.len(), v.error.clone())
        };
        let song = cur.as_ref().and_then(|c| st.song(c)).cloned();
        let has_lrc = cur.as_ref().map_or(false, |c| crate::lyrics::lrc_for(c).is_some());

        hud::neon_panel(ui.painter(), r, 18.0, if hud::soft() { Color32::from_rgba_unmultiplied(20, 19, 30, 200) } else { self.pal.card }, tcy(), 0.8);
        let inner_w = r.width() - 28.0;
        // Header.
        ui.painter().text(r.min + vec2(16.0, 15.0), Align2::LEFT_CENTER, if hud::soft() { "Đang phát" } else { "NOW//PLAYING" }, if hud::soft() { semi(12.0) } else { FontId::monospace(10.0) }, hud::with_alpha(tcy(), 170));
        // Header tag: this song is the one matched to the wallpaper (it follows the wallpaper).
        let matched = item.map_or(false, |i| cur.as_ref().map_or(false, |c| i.songs().contains(c)));
        if matched { ui.painter().text(r.right_top() + vec2(-16.0, 15.0), Align2::RIGHT_CENTER, if hud::soft() { "♪ Nhạc của hình nền" } else { "◆ MATCH//HÌNH NỀN" }, if hud::soft() { semi(11.5) } else { FontId::monospace(9.5) }, tye()); }
        else { ui.painter().text(r.right_top() + vec2(-16.0, 15.0), Align2::RIGHT_CENTER, if hud::soft() { format!("{qlen} bài") } else { format!("SRC//LOCAL  {}", hud::hex(qlen as u32 + 7, 3)) }, FontId::monospace(9.5), hud::with_alpha(tmg(), 170)); }

        // Cover (the wallpaper's own picture) or the collection list.
        let art = Rect::from_min_size(r.min + vec2(14.0, 30.0), vec2(inner_w, 102.0));
        if !self.pl_list {
            self.paint_media(ui, ctx, item, art, 0.0, 0.55);
            let p = ui.painter();
            if !hud::soft() { p.rect_stroke(art, Rounding::ZERO, Stroke::new(1.0_f32, hud::with_alpha(tcy(), 120))); }
            hud::cut_corners(p, art.expand(1.0), 12.0, self.pal.card, tcy());
            if let Some(it) = item { p.text(art.left_bottom() + vec2(10.0, -8.0), Align2::LEFT_BOTTOM, elide(p, &up(&it.title), semi(11.5), inner_w - 20.0), semi(11.5), hud::with_alpha(Color32::WHITE, 210)); }
            if playing { // little equaliser
                let t = ui.input(|i| i.time) as f32;
                for k in 0..4 {
                    let h = 4.0 + 12.0 * (((t * (5.0 + k as f32 * 1.3) + k as f32 * 1.7).sin() * 0.5) + 0.5);
                    p.rect_filled(Rect::from_min_size(pos2(art.right() - 30.0 + k as f32 * 6.0, art.top() + 24.0 - h), vec2(3.5, h)), 0.0, tcy());
                }
            }
        } else {
            changed |= self.collection_list(ui, st, Rect::from_min_max(art.min, pos2(art.max.x, r.top() + 184.0)), cur.as_ref());
        }

        // Title / artist + favourite.
        let ty = r.top() + 150.0;
        let title = song.as_ref().map(|s| s.title.clone()).unwrap_or_else(|| "Chưa phát bài nào".into());
        let artist = song.as_ref().map(|s| if s.artist.is_empty() { "Nghệ sĩ không rõ".to_string() } else { s.artist.clone() }).unwrap_or_else(|| "Bấm ▶ để phát nhạc của bạn".into());
        if !self.pl_list {
            let p = ui.painter();
            p.text(pos2(r.left() + 16.0, ty), Align2::LEFT_CENTER, elide(p, &title, bold(15.0), inner_w - 40.0), bold(15.0), Color32::WHITE);
            p.text(pos2(r.left() + 16.0, ty + 19.0), Align2::LEFT_CENTER, elide(p, &artist, FontId::monospace(11.0), inner_w - 40.0), FontId::monospace(11.0), muted());
            if let Some(s) = &song {
                let (resp, hot) = ibtn(ui, "pl_like", pos2(r.right() - 26.0, ty + 8.0), 26.0, "Thêm / bỏ yêu thích");
                ic_heart(ui.painter(), pos2(r.right() - 26.0, ty + 8.0), 8.5, if s.liked { tmg() } else if hot { tcy() } else { muted() }, s.liked);
                if resp.clicked() { let path = s.path.clone(); if let Some(m) = st.library.songs.iter_mut().find(|x| x.path == path) { m.liked = !m.liked; } st.save(); }
            }
        }

        // Seek bar.
        let sy = r.top() + 196.0;
        let bar = Rect::from_min_max(pos2(r.left() + 16.0, sy - 8.0), pos2(r.right() - 16.0, sy + 8.0));
        let resp = ui.interact(bar, ui.id().with("pl_seek"), Sense::click_and_drag()).on_hover_cursor(egui::CursorIcon::PointingHand);
        let at = |x: f32| ((x - bar.left()) / bar.width()).clamp(0.0, 1.0);
        let live = if resp.dragged() || resp.clicked() { resp.interact_pointer_pos().map(|q| at(q.x)) } else { None };
        if resp.drag_stopped() || resp.clicked() {
            if let (Some(f), true) = (live.or_else(|| resp.interact_pointer_pos().map(|q| at(q.x))), dur > 0.0) { self.engine.player(PCmd::Seek(f as f64 * dur)); }
        }
        self.pl_seek = live.filter(|_| resp.dragged());
        let frac = self.pl_seek.unwrap_or(if dur > 0.0 { (pos / dur) as f32 } else { 0.0 }).clamp(0.0, 1.0);
        let hot = resp.hovered() || resp.dragged();
        let th = if hot { 6.0 } else { 3.0 };
        let track = Rect::from_min_max(pos2(bar.left(), sy - th / 2.0), pos2(bar.right(), sy + th / 2.0));
        let p = ui.painter();
        if hud::soft() {
            // Apple Music style: a thin white rail that thickens under the pointer.
            p.rect_filled(track, th / 2.0, Color32::from_white_alpha(46));
            p.rect_filled(Rect::from_min_max(track.min, pos2(track.left() + track.width() * frac, track.bottom())), th / 2.0, Color32::from_white_alpha(if hot { 255 } else { 215 }));
            if hot { p.circle_filled(pos2(track.left() + track.width() * frac, sy), 6.5, Color32::WHITE); }
        } else {
        p.rect_filled(track, 0.0, hud::with_alpha(tcy(), 42));
        p.rect_filled(Rect::from_min_max(track.min, pos2(track.left() + track.width() * frac, track.bottom())), 0.0, if hot { tye() } else { tcy() });
        hud::neon_panel(p, Rect::from_center_size(pos2(track.left() + track.width() * frac, sy), vec2(7.0, if hot { 15.0 } else { 11.0 })), 2.0, if hot { tye() } else { tcy() }, if hot { tye() } else { tcy() }, 0.6);
        }
        let shown = self.pl_seek.map(|f| f as f64 * dur).unwrap_or(pos);
        p.text(pos2(bar.left(), sy + 15.0), Align2::LEFT_CENTER, fmt_time(shown), FontId::monospace(10.0), muted());
        p.text(pos2(bar.right(), sy + 15.0), Align2::RIGHT_CENTER, fmt_time(dur), FontId::monospace(10.0), muted());

        // Transport row: play mode · prev · PLAY · next · volume (speaker), evenly spaced.
        let cy = r.top() + 244.0;
        let step = (r.width() - 44.0) / 4.0;
        let xs: [f32; 5] = std::array::from_fn(|i| r.left() + 22.0 + step * i as f32);
        let col_of = |on: bool, hot: bool| if hud::soft() { if on { hud::accent() } else if hot { Color32::WHITE } else { Color32::from_white_alpha(200) } } else if on { tye() } else if hot { tcy() } else { subtle() };
        // One button for the play order: off → repeat all → repeat one → shuffle → off …
        let mode = if shuffle { 3 } else { repeat.min(2) };
        let (rm, hm) = ibtn(ui, "pl_mode", pos2(xs[0], cy), 32.0,
            ["Phát lần lượt (không lặp)", "Lặp tất cả", "Lặp một bài (mặc định cho bài khớp hình nền)", "Trộn bài"][mode as usize]);
        let mc = col_of(mode > 0, hm);
        if mode == 3 { ic_shuffle(ui.painter(), pos2(xs[0], cy), mc); } else { ic_repeat(ui.painter(), pos2(xs[0], cy), mc, mode == 2); }
        if rm.clicked() {
            let (sh, rp) = match mode { 0 => (false, 1), 1 => (false, 2), 2 => (true, 1), _ => (false, 0) };
            st.settings.pl_shuffle = sh; st.settings.pl_repeat = rp;
            self.engine.player(PCmd::Shuffle(sh)); self.engine.player(PCmd::Repeat(rp)); st.save();
        }
        let (rp, hp) = ibtn(ui, "pl_prev", pos2(xs[1], cy), 34.0, "Bài trước");
        ic_skip(ui.painter(), pos2(xs[1], cy), 15.0, col_of(false, hp), false);
        if rp.clicked() { self.engine.player(PCmd::Prev); }
        // Big play / pause (solid neon square with cut corners).
        let pr = Rect::from_center_size(pos2(xs[2], cy), vec2(46.0, 46.0));
        let presp = ui.interact(pr, ui.id().with("pl_play"), Sense::click()).on_hover_cursor(egui::CursorIcon::PointingHand).on_hover_text(if playing { "Tạm dừng" } else { "Phát" });
        let glyph = if hud::soft() {
            // White disc with a dark glyph, growing a touch under the pointer.
            let k = ui.ctx().animate_bool_with_time(presp.id, presp.hovered(), 0.15);
            ui.painter().circle_filled(pr.center() + vec2(0.0, 3.0), 24.0, Color32::from_black_alpha(60));
            ui.painter().circle_filled(pr.center(), 23.0 + 2.0 * k, Color32::from_rgb(245, 245, 247));
            Color32::from_rgb(17, 19, 24)
        } else {
            hud::neon_panel(ui.painter(), pr, 13.0, if presp.hovered() { tye() } else { tcy() }, if presp.hovered() { tye() } else { tcy() }, 1.0);
            BG0
        };
        if playing { ic_pause(ui.painter(), pr.center(), 18.0, glyph); } else { ic_play(ui.painter(), pr.center() + vec2(1.5, 0.0), 19.0, glyph); }
        if presp.clicked() {
            if cur.is_none() && qlen == 0 {
                let paths: Vec<PathBuf> = st.library.songs.iter().map(|s| s.path.clone()).collect();
                if paths.is_empty() { self.notice = Some("Chưa có bài nhạc nào — thêm ở tab Nhạc".into()); } else { self.engine.player(PCmd::SetQueue(paths, 0)); }
            } else { self.engine.player(PCmd::Toggle); }
        }
        let (rn, hn) = ibtn(ui, "pl_next", pos2(xs[3], cy), 34.0, "Bài tiếp");
        ic_skip(ui.painter(), pos2(xs[3], cy), 15.0, col_of(false, hn), true);
        if rn.clicked() { self.engine.player(PCmd::Next); }
        // Volume: point at the speaker for the bar (it slides out to the left), click to mute.
        // This is the volume of the player — and of the music matched to the wallpaper.
        {
            let btn = Rect::from_center_size(pos2(xs[4], cy), vec2(32.0, 32.0));
            let muted = st.settings.is_muted;
            let mut vd = self.vol_drag;
            let (toggled, set_vol) = speaker_control(ui, btn, muted, st.settings.volume, &mut vd, "pl_speaker", 2);
            self.vol_drag = vd;
            if toggled { st.settings.is_muted = !muted; st.save(); changed = true; }
            if let Some(v) = set_vol {
                st.settings.volume = v; st.settings.pl_volume = v;
                if v > 0.0 { st.settings.is_muted = false; }
                st.save();
                changed = true;
            }
        }
        // Bottom row: collection list · lyrics (experimental).
        let by = r.bottom() - 22.0;
        let (rl, hl) = ibtn(ui, "pl_list", pos2(r.left() + 30.0, by), 30.0, "Danh sách / bộ sưu tập");
        ic_list(ui.painter(), pos2(r.left() + 30.0, by), col_of(self.pl_list, hl));
        if rl.clicked() { self.pl_list = !self.pl_list; }
        // Lyrics switch with a BETA tag.
        let lw = 112.0;
        let lr = Rect::from_center_size(pos2(r.right() - 16.0 - lw / 2.0, by), vec2(lw, 26.0));
        let lresp = ui.interact(lr, ui.id().with("pl_lyr"), Sense::click()).on_hover_cursor(egui::CursorIcon::PointingHand)
            .on_hover_text("THỬ NGHIỆM: hiện lời bài hát trên màn hình chính.\nCần file .lrc cùng tên với bài (vd: bai.mp3 → bai.lrc) nằm cạnh bài hát.");
        let on = st.settings.lyrics_on;
        hud::neon_panel(ui.painter(), lr, 7.0, if on { hud::with_alpha(tmg(), 70) } else if lresp.hovered() { hud::with_alpha(tcy(), 60) } else { Color32::TRANSPARENT }, if on { tmg() } else { hud::with_alpha(tcy(), 110) }, if on { 0.8 } else { 0.0 });
        ui.painter().text(lr.left_center() + vec2(8.0, 0.0), Align2::LEFT_CENTER, if hud::soft() { "Lời bài hát" } else { "LỜI" }, semi(11.5), if on { tmg() } else { subtle() });
        ui.painter().text(lr.right_center() - vec2(8.0, 0.0), Align2::RIGHT_CENTER, "BETA", FontId::monospace(9.0), tye());
        if lresp.clicked() { st.settings.lyrics_on = !on; st.save(); }
        if st.settings.lyrics_on && cur.is_some() && !has_lrc {
            ui.painter().text(pos2(r.right() - 16.0, by - 20.0), Align2::RIGHT_CENTER, if hud::soft() { "Không thấy file .lrc" } else { "KHÔNG THẤY FILE .LRC" }, FontId::monospace(9.0), tye());
        }
        if let Some(e) = err { ui.painter().text(pos2(r.left() + 16.0, by - 20.0), Align2::LEFT_CENTER, elide(ui.painter(), &e, FontId::monospace(9.0), inner_w * 0.5), FontId::monospace(9.0), hud::with_alpha(Color32::from_rgb(255, 120, 100), 230)); }
        changed
    }

    /// Collection list (all songs / favourites) drawn over the cover area.
    fn collection_list(&mut self, ui: &mut Ui, st: &mut AppState, area: Rect, cur: Option<&PathBuf>) -> bool {
        // Filter chips.
        let mut x = area.left();
        for (label, fav) in [(if hud::soft() { "Tất cả" } else { "TẤT CẢ" }, false), (if hud::soft() { "Yêu thích ♥" } else { "YÊU THÍCH ♥" }, true)] {
            let g = ui.painter().layout_no_wrap(label.into(), FontId::monospace(10.0), tx());
            let rr = Rect::from_min_size(pos2(x, area.top()), vec2(g.size().x + 18.0, 22.0));
            let resp = ui.interact(rr, ui.id().with(("pl_filter", fav)), Sense::click()).on_hover_cursor(egui::CursorIcon::PointingHand);
            let on = self.pl_fav == fav;
            hud::neon_panel(ui.painter(), rr, 6.0, if on { tcy() } else { hud::with_alpha(BG0, 220) }, hud::with_alpha(tcy(), if on { 255 } else { 110 }), 0.4);
            ui.painter().text(rr.center(), Align2::CENTER_CENTER, label, FontId::monospace(10.0), if on { BG0 } else { tcy() });
            if resp.clicked() { self.pl_fav = fav; }
            x = rr.right() + 6.0;
        }
        // Search box.
        let sb = Rect::from_min_size(pos2(area.left(), area.top() + 26.0), vec2(area.width(), 24.0));
        // (A child ui, not `ui.put`: put would move the page's layout cursor and push the cards up.)
        let mut sui = ui.new_child(egui::UiBuilder::new().max_rect(sb));
        sui.add(egui::TextEdit::singleline(&mut self.song_q).hint_text(if hud::soft() { "🔍  Tìm bài hát…" } else { "> TÌM NHẠC…" }).desired_width(sb.width()).font(FontId::monospace(11.5)).margin(vec2(8.0, 4.0)));
        let q = self.song_q.trim().to_lowercase();
        let paths: Vec<usize> = (0..st.library.songs.len()).filter(|&i| {
            let s = &st.library.songs[i];
            (!self.pl_fav || s.liked) && (q.is_empty() || s.title.to_lowercase().contains(&q) || s.artist.to_lowercase().contains(&q) || s.album.to_lowercase().contains(&q))
        }).collect();
        let list = Rect::from_min_max(pos2(area.left(), area.top() + 56.0), area.max);
        if paths.is_empty() {
            ui.painter().text(list.center(), Align2::CENTER_CENTER, up(if !q.is_empty() { "Không có bài khớp" } else if self.pl_fav { "Chưa có bài yêu thích" } else { "Chưa có bài nào" }).as_str(), FontId::monospace(11.0), muted());
            return false;
        }
        let mut play: Option<usize> = None;
        let mut like: Option<PathBuf> = None;
        let mut child = ui.new_child(egui::UiBuilder::new().max_rect(list));
        child.set_clip_rect(list);
        egui::ScrollArea::vertical().auto_shrink([false, false]).show_rows(&mut child, 30.0, paths.len(), |ui, range| {
            for k in range {
                let si = paths[k];
                let s = &st.library.songs[si];
                let on = cur == Some(&s.path);
                let (rect, resp) = ui.allocate_exact_size(vec2(list.width() - 8.0, 30.0), Sense::click());
                if on { ui.painter().rect_filled(rect, 0.0, hud::with_alpha(tcy(), 38)); }
                else if resp.hovered() { ui.painter().rect_filled(rect, 0.0, hud::with_alpha(tmg(), 40)); }
                let hr = Rect::from_center_size(pos2(rect.left() + 14.0, rect.center().y), vec2(22.0, 22.0));
                let hresp = ui.interact(hr, ui.id().with(("pl_row_heart", si)), Sense::click()).on_hover_cursor(egui::CursorIcon::PointingHand);
                ic_heart(ui.painter(), hr.center(), 6.5, if s.liked { tmg() } else if hresp.hovered() { tcy() } else { muted() }, s.liked);
                if hresp.clicked() { like = Some(s.path.clone()); }
                let t = elide(ui.painter(), &s.title, semi(12.0), rect.width() - 90.0);
                ui.painter().text(pos2(rect.left() + 32.0, rect.center().y), Align2::LEFT_CENTER, t, semi(12.0), if on { tcy() } else { tx() });
                ui.painter().text(pos2(rect.right() - 6.0, rect.center().y), Align2::RIGHT_CENTER, if s.duration > 0.0 { fmt_time(s.duration) } else { String::new() }, FontId::monospace(10.0), muted());
                if resp.clicked() && !hresp.clicked() { play = Some(k); }
            }
        });
        if let Some(p) = like { if let Some(m) = st.library.songs.iter_mut().find(|x| x.path == p) { m.liked = !m.liked; } st.save(); }
        if let Some(k) = play {
            let q: Vec<PathBuf> = paths.iter().map(|&i| st.library.songs[i].path.clone()).collect();
            self.engine.player(PCmd::SetQueue(q, k));
        }
        false
    }
}
