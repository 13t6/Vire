//! Shared widgets of both looks: buttons, switch, segmented control, chips, titles, the speaker
//! volume control, and the grouped-rows card.

use super::*;

pub(super) fn semi(size: f32) -> FontId { FontId::new(size, FontFamily::Name("semi".into())) }
pub(super) fn bold(size: f32) -> FontId { FontId::new(size, FontFamily::Name("bold".into())) }

pub(super) fn load_png_texture(ctx: &egui::Context, name: &str, bytes: &[u8]) -> Option<TextureHandle> {
    let img = crate::img::decode(bytes)?;
    let ci = egui::ColorImage::from_rgba_unmultiplied([img.w as usize, img.h as usize], &img.px);
    Some(ctx.load_texture(name, ci, egui::TextureOptions::LINEAR))
}

/// Vertical black ramp used as a legibility shade over images (keeps rounded corners via Image).
pub(super) fn shade_texture(ctx: &egui::Context) -> TextureHandle {
    let n = 64;
    let mut px = Vec::with_capacity(n * 4);
    for i in 0..n {
        let t = i as f32 / (n - 1) as f32;
        let a = (t * t * 235.0) as u8;
        px.extend_from_slice(&[6, 7, 12, a]);
    }
    ctx.load_texture("shade", egui::ColorImage::from_rgba_unmultiplied([1, n], &px), egui::TextureOptions::LINEAR)
}

/// Chamfered neon button. Default: dark with a cyan edge; hover: solid neon with inverted text and a
/// glitch shift. `primary` = solid cyan from the start (and cyber yellow on hover).
/// Uppercase in the HUD look, sentence case in the Cinematic one.
pub(super) fn up(s: &str) -> String { if hud::soft() { s.to_string() } else { s.to_uppercase() } }

/// Cinematic: soft rounded button (violet when primary, frosted glass otherwise).
pub(super) fn soft_button(ui: &mut Ui, icon: Option<Icon>, text: &str, primary: bool) -> Response {
    // PLACEHOLDER colour: the colour is chosen below (it depends on the fill) and passed to `galley()`
    // — a colour baked in here would win over it, white text on a light accent was unreadable.
    let galley = ui.painter().layout_no_wrap(text.to_string(), semi(13.0), Color32::PLACEHOLDER);
    let icon_w = if icon.is_some() { 22.0 } else { 0.0 };
    let (rect, resp) = ui.allocate_exact_size(vec2(galley.size().x + icon_w + 34.0, 36.0), Sense::click());
    let t = ui.ctx().animate_bool_with_time(resp.id, resp.hovered(), 0.12);
    let p = ui.painter();
    let fill = if primary { hud::accent().lerp_to_gamma(Color32::WHITE, 0.15 * t) } else { Color32::from_white_alpha((14.0 + 16.0 * t) as u8) };
    if primary { p.rect_filled(rect.expand(2.0 + 2.0 * t), 20.0, hud::with_alpha(hud::accent(), (30.0 + 30.0 * t) as u8)); }
    p.rect_filled(rect, 18.0, fill);
    if !primary { p.rect_stroke(rect, 18.0, Stroke::new(1.0_f32, Color32::from_white_alpha((30.0 + 30.0 * t) as u8))); }
    let fg = if primary { hud::on_color(fill) } else { tx() };
    let mut x = rect.left() + 17.0;
    if let Some(ic) = icon { paint_icon(p, pos2(x + 7.0, rect.center().y), ic, fg); x += icon_w; }
    p.galley(pos2(x, rect.center().y - galley.size().y / 2.0), galley, fg);
    resp.on_hover_cursor(egui::CursorIcon::PointingHand)
}

pub(super) fn pill_button(ui: &mut Ui, icon: Option<Icon>, text: &str, primary: bool) -> Response {
    if hud::soft() { return soft_button(ui, icon, text, primary); }
    let label = text.to_uppercase();
    let galley = ui.painter().layout_no_wrap(label.clone(), semi(12.5), tx());
    let icon_w = if icon.is_some() { 22.0 } else { 0.0 };
    let size = vec2(galley.size().x + icon_w + 32.0, 34.0);
    let (rect, resp) = ui.allocate_exact_size(size, Sense::click());
    let t = ui.ctx().animate_bool_with_time(resp.id, resp.hovered(), 0.08);
    let solid = primary || t > 0.5;
    let neon = if primary && t > 0.5 { YELLOW } else { CYAN };
    let fill = if solid { neon } else { hud::with_alpha(BG0, 235) };
    hud::neon_panel(ui.painter(), rect, 9.0, fill, neon, 0.6 + t);
    let fg = if solid { BG0 } else { CYAN };
    let mut x = rect.left() + 16.0;
    if let Some(ic) = icon { paint_icon(ui.painter(), pos2(x + 7.0, rect.center().y), ic, fg); x += icon_w; }
    hud::glitch_text(ui.painter(), pos2(x, rect.center().y), Align2::LEFT_CENTER, &label, semi(12.5), fg, if solid { 0.0 } else { t });
    resp.on_hover_cursor(egui::CursorIcon::PointingHand)
}

/// Square chamfered icon button (used on top of images).
pub(super) fn round_button(ui: &mut Ui, rect: Rect, icon: Icon, id: &str) -> Response {
    let resp = ui.interact(rect, ui.id().with(id), Sense::click());
    let t = ui.ctx().animate_bool_with_time(resp.id, resp.hovered(), 0.08);
    if hud::soft() {
        // Frosted glass disc; brightens on hover.
        let p = ui.painter();
        let r = rect.width() / 2.0;
        p.circle_filled(rect.center() + vec2(0.0, 2.0), r + 1.0, Color32::from_black_alpha(60));
        p.circle_filled(rect.center(), r, Color32::from_rgba_unmultiplied(22, 20, 34, (170.0 + 60.0 * t) as u8));
        p.circle_stroke(rect.center(), r, Stroke::new(1.0_f32, Color32::from_white_alpha((40.0 + 50.0 * t) as u8)));
        paint_icon(p, rect.center(), icon, Color32::WHITE.lerp_to_gamma(hud::accent().lerp_to_gamma(Color32::WHITE, 0.45), t));
        return resp.on_hover_cursor(egui::CursorIcon::PointingHand);
    }
    let hot = t > 0.5;
    hud::neon_panel(ui.painter(), rect, rect.width() * 0.28, if hot { CYAN } else { hud::with_alpha(BG0, 225) }, CYAN, 0.6 + t);
    paint_icon(ui.painter(), rect.center(), icon, if hot { BG0 } else { CYAN });
    resp.on_hover_cursor(egui::CursorIcon::PointingHand)
}

/// Speaker button with a volume bar that appears while you point at it (or drag it). Click the
/// speaker = mute / unmute. `mode`: 0 = horizontal bar sliding out to the left, 1 = to the right,
/// 2 = a short DIAGONAL bar rising toward the top-left. Returns (mute toggled, Some(volume) when
/// a new volume was set).
pub(super) fn speaker_control(ui: &mut Ui, btn: Rect, muted: bool, volume: f32, drag: &mut Option<f32>, id: &str, mode: u8) -> (bool, Option<f32>) {
    let resp = round_button(ui, btn, if muted || volume <= 0.0 { Icon::Mute } else { Icon::Speaker }, id);
    let toggled = resp.clicked();
    let ptr = ui.ctx().pointer_hover_pos();
    let mut commit = None;
    let p = ui.painter().clone();
    // The bar opens ONLY when the pointer is on the speaker itself; once open it stays while the
    // pointer travels along the bar (so you can reach it). A closed, invisible bar never reacts.
    let open_id = ui.id().with((id, "vopen"));
    let was_open = ui.data(|d| d.get_temp::<bool>(open_id)).unwrap_or(false);
    let on_btn = ptr.map_or(false, |q| btn.expand(5.0).contains(q));
    let dragging = drag.is_some();
    let gate = move |near_bar: bool| -> bool { dragging || on_btn || (was_open && near_bar) };
    let remember = |open: bool| ui.data_mut(|d| d.insert_temp(open_id, open));
    if hud::soft() {
        // Control Center style: a frosted vertical capsule rising above the speaker; it fills
        // with white from the bottom. Drag up / down, or click a height.
        let (w, h) = (38.0, 132.0);
        let cap = Rect::from_min_size(pos2(btn.center().x - w / 2.0, btn.top() - 10.0 - h), vec2(w, h));
        let bridge = Rect::from_min_max(pos2(cap.left(), cap.bottom()), pos2(cap.right(), btn.top() + 2.0));
        let near = |q: Pos2| cap.expand(10.0).contains(q) || bridge.contains(q) || btn.expand(5.0).contains(q);
        let show = gate(ptr.map_or(false, near));
        remember(show);
        let k =ui.ctx().animate_bool_with_time(ui.id().with((id, "vcap")), show, 0.16);
        if k > 0.0 {
            // Layer above the page so it is never hidden by a neighbouring card.
            let lp = ui.ctx().layer_painter(egui::LayerId::new(egui::Order::Foreground, ui.id().with((id, "vlayer"))));
            let grow = cap.height() * (0.35 + 0.65 * k);
            let shown = Rect::from_min_max(pos2(cap.left(), cap.bottom() - grow), cap.max);
            let a = (255.0 * k) as u8;
            let sr = ui.interact(cap, ui.id().with((id, "vol")), Sense::click_and_drag()).on_hover_cursor(egui::CursorIcon::PointingHand);
            let inner = cap.shrink(3.0);
            let at = |q: Pos2| ((inner.bottom() - q.y) / inner.height()).clamp(0.0, 1.0);
            let live = if sr.dragged() || sr.clicked() { sr.interact_pointer_pos().map(at) } else { None };
            *drag = live.or(if sr.dragged() { *drag } else { None });
            let v = drag.unwrap_or(if muted { 0.0 } else { volume }).clamp(0.0, 1.0);
            if sr.drag_stopped() || sr.clicked() { commit = Some(live.unwrap_or(v)); *drag = None; }
            let rad = w / 2.0;
            lp.rect_filled(shown.translate(vec2(0.0, 8.0)).expand(3.0), rad + 3.0, Color32::from_black_alpha((60.0 * k) as u8));
            lp.rect_filled(shown, rad, Color32::from_rgba_unmultiplied(28, 30, 38, (225.0 * k) as u8));
            lp.rect_stroke(shown, rad, Stroke::new(1.0_f32, Color32::from_white_alpha((22.0 * k) as u8)));
            // Fill from the bottom, clipped to the capsule's round ends.
            let fill_h = (inner.height() * v).max(if v > 0.0 { inner.width() } else { 0.0 });
            if fill_h > 0.0 && k > 0.6 {
                let f = Rect::from_min_max(pos2(inner.left(), inner.bottom() - fill_h), inner.max);
                lp.rect_filled(f, inner.width() / 2.0, Color32::from_rgba_unmultiplied(245, 245, 247, a));
            }
            if k > 0.6 {
                let pct = format!("{}", (v * 100.0).round() as i32);
                lp.text(pos2(cap.center().x, cap.top() + 16.0), Align2::CENTER_CENTER, pct, semi(11.5),
                    if v > 0.9 { Color32::from_rgba_unmultiplied(17, 19, 24, a) } else { Color32::from_rgba_unmultiplied(245, 245, 247, a) });
                paint_icon(&lp, pos2(cap.center().x - 1.0, inner.bottom() - 16.0), if v <= 0.0 { Icon::Mute } else { Icon::Speaker },
                    if v > 0.12 { Color32::from_rgba_unmultiplied(17, 19, 24, a) } else { Color32::from_rgba_unmultiplied(245, 245, 247, a) });
            }
            if k < 1.0 { ui.ctx().request_repaint(); }
        }
        return (toggled, commit);
    }
    if mode == 2 {
        // Diagonal bar: from just off the button's top-left corner, 45° up-left, 84 px long.
        let dir = vec2(-1.0, -1.0) / std::f32::consts::SQRT_2;
        let len = 84.0;
        let s0 = btn.left_top() + vec2(-2.0, -2.0);
        let s1 = s0 + dir * len;
        let near = |q: Pos2| { let v = q - s0; let t = v.dot(dir); let d = (v - dir * t).length(); t > -12.0 && t < len + 12.0 && d < 16.0 };
        let open = gate(ptr.map_or(false, near));
        remember(open);
        if open {
            let hit = Rect::from_two_pos(s0, s1).expand(16.0);
            let sr = ui.interact(hit, ui.id().with((id, "vol")), Sense::click_and_drag());
            let at = |q: Pos2| ((q - s0).dot(dir) / len).clamp(0.0, 1.0);
            let live = if sr.dragged() { sr.interact_pointer_pos().map(at) } else if sr.clicked() { sr.interact_pointer_pos().filter(|q| near(*q)).map(at) } else { None };
            *drag = live.or(if sr.dragged() { *drag } else { None });
            let v = drag.unwrap_or(volume).clamp(0.0, 1.0);
            if sr.drag_stopped() || (sr.clicked() && live.is_some()) {
                commit = Some(live.unwrap_or(v));
                *drag = None;
            }
            let hot = sr.hovered() || sr.dragged();
            let col = if hot { YELLOW } else { CYAN };
            let n = vec2(-dir.y, dir.x); // perpendicular
            // Backdrop plate (a thin rotated strip) + track + fill + ticks + diamond knob + percent.
            let plate = vec![s0 + n * 9.0 + dir * 8.0, s1 + n * 9.0 - dir * 8.0, s1 - n * 9.0 - dir * 8.0, s0 - n * 9.0 + dir * 8.0];
            p.add(Shape::convex_polygon(plate.clone(), hud::with_alpha(BG0, 238), Stroke::NONE));
            p.add(Shape::closed_line(plate, Stroke::new(1.0_f32, hud::with_alpha(col, 200))));
            p.line_segment([s0, s1], Stroke::new(4.0_f32, hud::with_alpha(CYAN, 45)));
            p.line_segment([s0, s0 + dir * len * v], Stroke::new(4.0_f32, col));
            for k in 1..8 { let c = s0 + dir * len * (k as f32 / 8.0); p.line_segment([c + n * 4.0, c - n * 4.0], Stroke::new(1.0_f32, hud::with_alpha(BG0, 200))); }
            let kp = s0 + dir * len * v;
            p.add(Shape::convex_polygon(vec![kp + dir * 6.0, kp + n * 6.0, kp - dir * 6.0, kp - n * 6.0], col, Stroke::NONE));
            p.text(s1 + dir * 10.0 + vec2(-2.0, -2.0), Align2::RIGHT_BOTTOM, format!("{}%", (v * 100.0).round() as i32), semi(11.0), tx());
        }
        return (toggled, commit);
    }
    let sx = if mode == 1 { btn.right() + 8.0 } else { btn.left() - 148.0 };
    let slot = Rect::from_min_size(pos2(sx, btn.center().y - 15.0), vec2(140.0, 30.0));
    let open = gate(ptr.map_or(false, |q| slot.expand2(vec2(6.0, 8.0)).contains(q)));
    remember(open);
    if open {
        hud::neon_panel(&p, slot, 8.0, hud::with_alpha(BG0, 240), CYAN, 1.0);
        let track = Rect::from_min_max(pos2(slot.left() + 12.0, slot.center().y - 3.0), pos2(slot.right() - 40.0, slot.center().y + 3.0));
        let sr = ui.interact(slot, ui.id().with((id, "vol")), Sense::click_and_drag()).on_hover_cursor(egui::CursorIcon::PointingHand);
        let at = |x: f32| ((x - track.left()) / track.width()).clamp(0.0, 1.0);
        let live = if sr.dragged() || sr.clicked() { sr.interact_pointer_pos().map(|q| at(q.x)) } else { None };
        *drag = live.or(if sr.dragged() { *drag } else { None });
        let v = drag.unwrap_or(volume).clamp(0.0, 1.0);
        if sr.drag_stopped() || sr.clicked() { commit = Some(live.unwrap_or(v)); *drag = None; }
        let hot = sr.hovered() || sr.dragged();
        p.rect_filled(track, 0.0, hud::with_alpha(CYAN, 40));
        p.rect_filled(Rect::from_min_max(track.min, pos2(track.left() + track.width() * v, track.bottom())), 0.0, if hot { YELLOW } else { CYAN });
        for k in 1..10 { let x = track.left() + track.width() * k as f32 / 10.0; p.line_segment([pos2(x, track.top() - 2.0), pos2(x, track.bottom() + 2.0)], Stroke::new(1.0_f32, hud::with_alpha(BG0, 200))); }
        hud::neon_panel(&p, Rect::from_center_size(pos2(track.left() + track.width() * v, slot.center().y), vec2(7.0, 14.0)), 2.0, if hot { YELLOW } else { CYAN }, if hot { YELLOW } else { CYAN }, 0.6);
        p.text(pos2(slot.right() - 7.0, slot.center().y), Align2::RIGHT_CENTER, format!("{}%", (v * 100.0).round() as i32), semi(11.0), tx());
    }
    (toggled, commit)
}
/// Shortens `text` with "…" so it fits in `max_w` pixels.
pub(super) fn elide(p: &egui::Painter, text: &str, font: FontId, max_w: f32) -> String {
    if p.layout_no_wrap(text.to_string(), font.clone(), tx()).size().x <= max_w { return text.to_string(); }
    let chars: Vec<char> = text.chars().collect();
    let (mut lo, mut hi) = (0usize, chars.len());
    while lo < hi {
        let mid = (lo + hi + 1) / 2;
        let s: String = chars[..mid].iter().collect::<String>() + "…";
        if p.layout_no_wrap(s, font.clone(), tx()).size().x <= max_w { lo = mid; } else { hi = mid - 1; }
    }
    chars[..lo].iter().collect::<String>().trim_end().to_string() + "…"
}

/// Segmented control: a chamfered track; the picked segment is solid neon with dark text.
pub(crate) fn segmented(ui: &mut Ui, labels: &[&str], selected: usize) -> Option<usize> {
    let font = semi(12.5);
    if hud::soft() {
        let gs: Vec<_> = labels.iter().map(|l| ui.painter().layout_no_wrap(l.to_string(), semi(13.0), tx())).collect();
        let ws: Vec<f32> = gs.iter().map(|g| g.size().x + 28.0).collect();
        let (rect, _) = ui.allocate_exact_size(vec2(ws.iter().sum::<f32>() + 8.0, 36.0), Sense::hover());
        ui.painter().rect_filled(rect, 18.0, Color32::from_white_alpha(14));
        let mut x = rect.left() + 4.0;
        let mut picked = None;
        for (i, g) in gs.into_iter().enumerate() {
            let r = Rect::from_min_size(pos2(x, rect.top() + 4.0), vec2(ws[i], 28.0));
            let resp = ui.interact(r, ui.id().with(("seg", labels[0], i)), Sense::click());
            if i == selected { ui.painter().rect_filled(r, 14.0, hud::accent()); }
            else if resp.hovered() { ui.painter().rect_filled(r, 14.0, Color32::from_white_alpha(18)); }
            let col = if i == selected { hud::on_accent() } else { tx() };
            ui.painter().galley(r.center() - g.size() / 2.0, g, col);
            if resp.on_hover_cursor(egui::CursorIcon::PointingHand).clicked() && i != selected { picked = Some(i); }
            x += ws[i];
        }
        return picked;
    }
    let upper: Vec<String> = labels.iter().map(|l| l.to_uppercase()).collect();
    let galleys: Vec<_> = upper.iter().map(|l| ui.painter().layout_no_wrap(l.clone(), font.clone(), tx())).collect();
    let seg_w: Vec<f32> = galleys.iter().map(|g| g.size().x + 26.0).collect();
    let total = seg_w.iter().sum::<f32>() + 6.0;
    let (rect, _) = ui.allocate_exact_size(vec2(total, 34.0), Sense::hover());
    hud::neon_panel(ui.painter(), rect, 9.0, hud::with_alpha(BG0, 235), hud::with_alpha(CYAN, 150), 0.4);
    let mut x = rect.left() + 3.0;
    let mut picked = None;
    for (i, g) in galleys.into_iter().enumerate() {
        let r = Rect::from_min_size(pos2(x, rect.top() + 3.0), vec2(seg_w[i], 28.0));
        let resp = ui.interact(r, ui.id().with(("seg", labels[0], i)), Sense::click());
        let t = ui.ctx().animate_bool_with_time(resp.id, i == selected, 0.08);
        let hot = resp.hovered() && i != selected;
        if t > 0.5 { ui.painter().rect_filled(r, 0.0, CYAN); }
        else if hot { ui.painter().rect_filled(r, 0.0, hud::with_alpha(CYAN, 190)); }
        let col = if t > 0.5 || hot { BG0 } else { CYAN };
        ui.painter().text(r.center(), Align2::CENTER_CENTER, &upper[i], font.clone(), col);
        let _ = g;
        if resp.on_hover_cursor(egui::CursorIcon::PointingHand).clicked() && i != selected { picked = Some(i); }
        x += seg_w[i];
    }
    picked
}

/// Angular switch: a chamfered slot with a square knob; on = solid cyan with a glow.
pub(crate) fn toggle(ui: &mut Ui, on: &mut bool) -> Response {
    let (rect, mut resp) = ui.allocate_exact_size(vec2(46.0, 24.0), Sense::click());
    if resp.clicked() { *on = !*on; resp.mark_changed(); }
    let t = ui.ctx().animate_bool_responsive(resp.id, *on);
    if hud::soft() {
        // iOS-like pill switch in violet.
        let p = ui.painter();
        let off = Color32::from_white_alpha(28);
        p.rect_filled(rect, 12.0, off.lerp_to_gamma(hud::accent(), t));
        let x = egui::lerp(rect.left() + 12.0..=rect.right() - 12.0, t);
        p.circle_filled(pos2(x, rect.center().y), 9.0, Color32::WHITE);
        return resp.on_hover_cursor(egui::CursorIcon::PointingHand);
    }
    let edge = if *on { CYAN } else { hud::with_alpha(CYAN, 110) };
    hud::neon_panel(ui.painter(), rect, 7.0, if *on { hud::with_alpha(CYAN, 60) } else { hud::with_alpha(BG0, 235) }, edge, if *on { 1.0 } else { 0.2 });
    let kw = 16.0;
    let x = egui::lerp(rect.left() + 4.0..=rect.right() - 4.0 - kw, t);
    ui.painter().rect_filled(Rect::from_min_size(pos2(x, rect.top() + 4.0), vec2(kw, rect.height() - 8.0)), 0.0, if *on { CYAN } else { muted() });
    resp.on_hover_cursor(egui::CursorIcon::PointingHand)
}

pub(super) fn chip(p: &egui::Painter, pos: Pos2, text: &str, fg: Color32, dot: Option<Color32>) -> Rect {
    let (fg, dot) = (hud::tone(fg), dot.map(hud::tone));
    if hud::soft() {
        let galley = p.layout_no_wrap(text.to_string(), semi(11.5), Color32::WHITE);
        let dot_w = if dot.is_some() { 14.0 } else { 0.0 };
        let rect = Rect::from_min_size(pos, vec2(galley.size().x + 22.0 + dot_w, 26.0));
        p.rect_filled(rect, 13.0, Color32::from_rgba_unmultiplied(14, 12, 22, 170));
        p.rect_stroke(rect, 13.0, Stroke::new(1.0_f32, Color32::from_white_alpha(36)));
        if let Some(c) = dot { p.circle_filled(pos2(rect.left() + 14.0, rect.center().y), 3.5, c); }
        p.galley(pos2(rect.left() + 11.0 + dot_w, rect.center().y - galley.size().y / 2.0), galley, Color32::WHITE);
        return rect;
    }
    let galley = p.layout_no_wrap(text.to_uppercase(), semi(10.5), fg);
    let dot_w = if dot.is_some() { 12.0 } else { 0.0 };
    let rect = Rect::from_min_size(pos, vec2(galley.size().x + 20.0 + dot_w, 22.0));
    let edge = dot.unwrap_or(fg);
    hud::neon_panel(p, rect, 6.0, hud::with_alpha(BG0, 215), edge, 0.5);
    if let Some(c) = dot { p.rect_filled(Rect::from_center_size(pos2(rect.left() + 12.0, rect.center().y), vec2(6.0, 6.0)), 0.0, c); }
    p.galley(pos2(rect.left() + 10.0 + dot_w, rect.center().y - galley.size().y / 2.0), galley, fg);
    rect
}

/// `[ TITLE ]` with a neon glow, a `//` subtitle and a thin rule under it.
pub(super) fn page_title(ui: &mut Ui, title: &str, subtitle: &str) {
    if hud::soft() {
        ui.label(RichText::new(title).font(bold(30.0)).color(Color32::WHITE));
        if !subtitle.is_empty() { ui.label(RichText::new(subtitle).size(13.5).color(Color32::from_white_alpha(130))); }
        ui.add_space(4.0);
        return;
    }
    let text = format!("[ {} ]", title.to_uppercase());
    let g = ui.painter().layout_no_wrap(text.clone(), bold(26.0), CYAN);
    let (rect, _) = ui.allocate_exact_size(vec2(ui.available_width(), g.size().y + 4.0), Sense::hover());
    hud::glow_text(ui.painter(), rect.left_center(), Align2::LEFT_CENTER, &text, bold(26.0), CYAN);
    // Decoration on the right: a blinking cursor, a hex tag and a tiny barcode.
    let blink = (ui.input(|i| i.time) * 1.6) as i64 % 2 == 0;
    let tag = hud::hex(title.len() as u32 * 977 + 13, 6);
    let tx = rect.right() - 4.0;
    ui.painter().text(pos2(tx, rect.center().y - 6.0), Align2::RIGHT_CENTER, format!("SYS//{tag}{}", if blink { " ▮" } else { "  " }), FontId::monospace(10.5), hud::with_alpha(CYAN, 150));
    hud::barcode(ui.painter(), Rect::from_min_size(pos2(tx - 86.0, rect.center().y + 3.0), vec2(86.0, 6.0)), title.len() as u32 + 7, hud::with_alpha(MAGENTA, 170));
    if !subtitle.is_empty() { ui.label(RichText::new(format!("// {subtitle}")).size(12.5).color(muted())); }
    let (rule, _) = ui.allocate_exact_size(vec2(ui.available_width(), 5.0), Sense::hover());
    ui.painter().line_segment([rule.left_center(), rule.right_center()], Stroke::new(1.0_f32, hud::with_alpha(CYAN, 60)));
    ui.painter().rect_filled(Rect::from_min_size(rule.left_top() + vec2(0.0, 1.5), vec2(54.0, 2.0)), 0.0, YELLOW);
}

pub(super) fn section_label(ui: &mut Ui, title: &str) {
    ui.add_space(14.0);
    if hud::soft() { ui.label(RichText::new(title).font(semi(15.0)).color(Color32::from_white_alpha(220))); ui.add_space(-2.0); return; }
    ui.label(RichText::new(format!("▌// {}", title.to_uppercase())).font(semi(12.5)).color(YELLOW));
    ui.add_space(-2.0);
}

impl App {
    // ---------- shared building blocks ----------

    /// Rounded card that groups rows, iOS-settings style.
    pub(super) fn group(&self, ui: &mut Ui, add: impl FnOnce(&mut Ui)) {
        if hud::soft() {
            egui::Frame::none().fill(Color32::from_rgba_unmultiplied(255, 255, 255, 7)).stroke(Stroke::new(1.0_f32, Color32::from_white_alpha(18)))
                .rounding(Rounding::same(16.0)).inner_margin(Margin::symmetric(20.0, 4.0))
                .show(ui, |ui| { ui.set_width(ui.available_width()); ui.spacing_mut().item_spacing.y = 0.0; add(ui); });
            return;
        }
        let out = egui::Frame::none().fill(self.pal.card).stroke(Stroke::new(1.0_f32, hud::with_alpha(CYAN, 110))).rounding(Rounding::ZERO)
            .inner_margin(Margin::symmetric(18.0, 4.0))
            .show(ui, |ui| { ui.set_width(ui.available_width()); ui.spacing_mut().item_spacing.y = 0.0; add(ui); });
        // 45° cut corners + a tick at the top-right.
        let r = out.response.rect;
        hud::cut_corners(ui.painter(), r.expand(1.0), 14.0, BG1, CYAN);
        hud::brackets(ui.painter(), Rect::from_min_max(r.right_top() + vec2(-26.0, 5.0), r.right_top() + vec2(-6.0, 16.0)), 5.0, hud::with_alpha(MAGENTA, 160));
    }

    /// One row inside a group; draws a hairline above itself unless it's the first.
    pub(super) fn row(&self, ui: &mut Ui, title: &str, desc: &str, add: impl FnOnce(&mut Ui) -> bool) -> bool {
        let first = ui.min_rect().height() < 1.0;
        let h = if desc.is_empty() { 52.0 } else { 62.0 };
        let (rect, _) = ui.allocate_exact_size(vec2(ui.available_width(), h), Sense::hover());
        let hair = if hud::soft() { Color32::from_white_alpha(16) } else { self.pal.hair };
        if !first { ui.painter().hline(rect.x_range(), rect.top(), Stroke::new(1.0_f32, hair)); }
        // Text block, vertically centred on the left.
        let wrap = rect.width() * 0.55;
        let tg = ui.painter().layout(title.to_string(), semi(14.0), tx(), wrap);
        let dcol = if hud::soft() { Color32::from_white_alpha(115) } else { muted() };
        let dg = (!desc.is_empty()).then(|| ui.painter().layout(desc.to_string(), FontId::proportional(12.0), dcol, wrap));
        let total = tg.size().y + dg.as_ref().map_or(0.0, |g| g.size().y + 3.0);
        let y0 = rect.center().y - total / 2.0;
        let th = tg.size().y;
        ui.painter().galley(pos2(rect.left(), y0), tg, tx());
        if let Some(g) = dg { ui.painter().galley(pos2(rect.left(), y0 + th + 3.0), g, dcol); }
        // Control, vertically centred on the right.
        let ctrl = Rect::from_min_max(pos2(rect.left() + wrap + 12.0, rect.top()), rect.max);
        let mut child = ui.new_child(egui::UiBuilder::new().max_rect(ctrl).layout(Layout::right_to_left(Align::Center)));
        child.spacing_mut().item_spacing = vec2(8.0, 8.0);
        add(&mut child)
    }
}
