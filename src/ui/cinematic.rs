//! "Cinematic Dark – Wallpaper Showcase": the second look of the dashboard.
//!
//! Only the frame (blurred-wallpaper backdrop, collapsible glass sidebar) and the wallpaper
//! showcase page live here. Every other page, every action and all the data are the very same
//! ones the HUD look uses — the shared widgets switch to their soft variants through
//! `hud::soft()` — so the two looks can never drift apart in behaviour.

use super::*;
use super::hud;
use crate::model::AppState;

/// UV rect that makes a texture of `size` cover `rect` (centre crop, never stretched).
pub(super) fn cover_uv(size: [usize; 2], rect: Rect) -> Rect {
    let (ta, ra) = (size[0] as f32 / size[1].max(1) as f32, rect.width() / rect.height().max(1.0));
    if ta > ra {
        let k = ra / ta; Rect::from_min_max(pos2((1.0 - k) / 2.0, 0.0), pos2((1.0 + k) / 2.0, 1.0))
    } else {
        let k = ta / ra; Rect::from_min_max(pos2(0.0, (1.0 - k) / 2.0), pos2(1.0, (1.0 + k) / 2.0))
    }
}

/// Box-downscales an RGBA image to `w`×`h` and softens it: drawn stretched with linear filtering
/// it reads as a heavy blur, for a few KB of texture.
fn tiny_blur(img: &crate::img::Rgba, w: usize, h: usize) -> Vec<u8> {
    let (sw, sh) = (img.w as usize, img.h as usize);
    let mut out = vec![0f32; w * h * 3];
    for y in 0..h {
        for x in 0..w {
            let (x0, x1) = (x * sw / w, ((x + 1) * sw / w).max(x * sw / w + 1));
            let (y0, y1) = (y * sh / h, ((y + 1) * sh / h).max(y * sh / h + 1));
            let (mut acc, mut n) = ([0f32; 3], 0f32);
            for yy in (y0..y1).step_by(2) {
                for xx in (x0..x1).step_by(2) {
                    let i = (yy * sw + xx) * 4;
                    for c in 0..3 { acc[c] += img.px[i + c] as f32; }
                    n += 1.0;
                }
            }
            for c in 0..3 { out[(y * w + x) * 3 + c] = acc[c] / n.max(1.0); }
        }
    }
    for _ in 0..2 {
        let src = out.clone();
        for y in 0..h {
            for x in 0..w {
                let mut acc = [0f32; 3];
                let mut n = 0f32;
                for (dx, dy) in [(-1i32, 0i32), (1, 0), (0, -1), (0, 1), (0, 0)] {
                    let (xx, yy) = ((x as i32 + dx).clamp(0, w as i32 - 1) as usize, (y as i32 + dy).clamp(0, h as i32 - 1) as usize);
                    for c in 0..3 { acc[c] += src[(yy * w + xx) * 3 + c]; }
                    n += 1.0;
                }
                for c in 0..3 { out[(y * w + x) * 3 + c] = acc[c] / n; }
            }
        }
    }
    let mut px = Vec::with_capacity(w * h * 4);
    for i in 0..w * h { px.extend_from_slice(&[out[i * 3] as u8, out[i * 3 + 1] as u8, out[i * 3 + 2] as u8, 255]); }
    px
}

impl App {
    /// Blurred copy of a wallpaper's thumbnail (one cached at a time). A new one also starts the
    /// cross-fade of the backdrop and eases the accent colour toward the new wallpaper's.
    pub(super) fn blur_for(&mut self, ctx: &egui::Context, item: &WallpaperItem) -> Option<TextureHandle> {
        if let Some((id, t)) = &self.blur { if *id == item.id { return Some(t.clone()); } }
        let img = crate::img::decode(&std::fs::read(item.thumb.as_ref()?).ok()?)?;
        let (w, h) = (48, 27);
        let px = tiny_blur(&img, w, h);
        let now = ctx.input(|i| i.time);
        let target = hud::accent_from(&px).unwrap_or(hud::C_VIOLET);
        self.accent_anim = (hud::accent(), target, now);
        let tex = ctx.load_texture(format!("cine_blur_{}", item.id), egui::ColorImage::from_rgba_unmultiplied([w, h], &px), egui::TextureOptions::LINEAR);
        if let Some((_, old)) = self.blur.take() { self.blur_prev = Some((old, now)); }
        self.blur = Some((item.id.clone(), tex.clone()));
        Some(tex)
    }

    /// Whole window in the Cinematic look. Returns (wallpaper settings changed, dock changed).
    pub(super) fn cine_frame(&mut self, ctx: &egui::Context, st: &mut AppState, hovering_files: bool) -> (bool, bool) {
        let screen = ctx.screen_rect();
        let now = ctx.input(|i| i.time);
        let ease = |x: f64| { let x = x.clamp(0.0, 1.0) as f32; 1.0 - (1.0 - x).powi(3) }; // ease-out cubic
        let mut animating = false;

        // Backdrop: the wallpaper in use, blurred, drifting a few px against the pointer
        // (parallax), cross-fading when the wallpaper changes, under a dark veil.
        let bg = ctx.layer_painter(egui::LayerId::background());
        bg.rect_filled(screen, 0.0, hud::C_BG);
        let ptr = ctx.input(|i| i.pointer.hover_pos()).unwrap_or(screen.center());
        let off = (ptr - screen.center()) / screen.size().max(vec2(1.0, 1.0));
        let px = ctx.animate_value_with_time(egui::Id::new("cine_px"), -off.x * 18.0, 0.6);
        let py = ctx.animate_value_with_time(egui::Id::new("cine_py"), -off.y * 12.0, 0.6);
        let room = screen.expand(26.0).translate(vec2(px, py));
        let cur = st.selected().cloned();
        let tex = cur.as_ref().and_then(|c| self.blur_for(ctx, c));
        if let Some(t) = &tex { bg.image(t.id(), room, cover_uv(t.size(), room), Color32::WHITE); }
        if let Some((old, t0)) = self.blur_prev.clone() {
            let k = ease((now - t0) / 0.7);
            if k < 1.0 {
                bg.image(old.id(), room, cover_uv(old.size(), room), Color32::from_white_alpha((255.0 * (1.0 - k)) as u8));
                animating = true;
            } else { self.blur_prev = None; }
        }
        // Accent eases to the wallpaper's colour.
        let (a0, a1, t0) = self.accent_anim;
        let k = ease((now - t0) / 0.7);
        hud::set_accent(a0.lerp_to_gamma(a1, k));
        if k < 1.0 { animating = true; }
        // egui's own selection colour (selectable labels, text selection) follows the accent too.
        let acc = hud::accent();
        if ctx.style().visuals.selection.stroke.color != acc {
            ctx.style_mut(|s| {
                s.visuals.selection.bg_fill = hud::with_alpha(acc, 110);
                s.visuals.selection.stroke = Stroke::new(1.0_f32, acc);
            });
        }

        bg.rect_filled(screen, 0.0, Color32::from_rgba_unmultiplied(8, 9, 12, 182));
        // Ambient light: one soft pool of the accent colour high up, very faint.
        let glow = |c: Pos2, col: Color32, r: f32, a: u8| {
            let mut m = egui::Mesh::default();
            m.colored_vertex(c, Color32::from_rgba_unmultiplied(col.r(), col.g(), col.b(), a));
            let n = 56;
            for i in 0..=n {
                let ang = i as f32 / n as f32 * std::f32::consts::TAU;
                m.colored_vertex(c + vec2(ang.cos(), ang.sin()) * r, Color32::TRANSPARENT);
            }
            for i in 1..=n as u32 { m.add_triangle(0, i, i + 1); }
            bg.add(Shape::mesh(m));
        };
        glow(screen.center_top() + vec2(px * 2.0, 20.0 + py), hud::accent(), screen.width() * 0.55, 26);
        let shade = Rect::from_min_max(pos2(screen.left(), screen.center().y), screen.max);
        bg.image(self.shade.id(), shade, Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0)), Color32::from_white_alpha(210));

        let compact = st.settings.ui_compact;
        let side_w = ctx.animate_value_with_time(egui::Id::new("cine_side_w"), if compact { 84.0 } else { 230.0 }, 0.22);
        egui::SidePanel::left("cine_side").exact_width(side_w).resizable(false).show_separator_line(false)
            .frame(egui::Frame::none().inner_margin(Margin { left: 14.0, right: 0.0, top: 14.0, bottom: 14.0 }))
            .show(ctx, |ui| self.cine_sidebar(ui, st, side_w < 150.0));

        // Page change: the new page fades in and rises a few pixels (iOS-like).
        if self.tab_seen != self.tab { self.tab_seen = self.tab; self.tab_t0 = now; }
        let k = ease((now - self.tab_t0) / 0.32);
        if k < 1.0 { animating = true; }
        let mut out = (false, false);
        egui::CentralPanel::default()
            .frame(egui::Frame::none().inner_margin(Margin { left: 28.0, right: 26.0, top: 24.0 + 10.0 * (1.0 - k), bottom: 14.0 }))
            .show(ctx, |ui| {
                ui.set_opacity(0.15 + 0.85 * k);
                ui.set_min_size(ui.available_size());
                out = self.tab_body(ui, ctx, st);
                if hovering_files {
                    let r = ui.max_rect();
                    let p = ui.painter();
                    p.rect_filled(r, 26.0, Color32::from_rgba_unmultiplied(8, 9, 12, 210));
                    p.rect_stroke(r.shrink(10.0), 22.0, Stroke::new(1.5_f32, hud::accent()));
                    p.text(r.center(), Align2::CENTER_CENTER, "Thả video để thêm — hoặc thả bài nhạc vào hình nền hiện tại", bold(19.0), hud::C_TEXT);
                }
            });
        if animating { ctx.request_repaint(); }
        out
    }

    /// macOS-style sidebar: a translucent panel, quiet rows, the selected one a soft pill.
    fn cine_sidebar(&mut self, ui: &mut Ui, st: &mut AppState, compact: bool) {
        let full = ui.max_rect();
        let p = ui.painter().clone();
        hud::glass(&p, full, 22.0, 150);

        let inner = full.shrink2(vec2(10.0, 16.0));
        let mut ui = ui.new_child(egui::UiBuilder::new().max_rect(inner).layout(Layout::top_down(Align::Min)));
        let ui = &mut ui;
        // Brand.
        let (head, _) = ui.allocate_exact_size(vec2(inner.width(), 44.0), Sense::hover());
        let lsz = 34.0;
        let lx = if compact { head.center().x - lsz / 2.0 } else { head.left() + 8.0 };
        let logo_r = Rect::from_min_size(pos2(lx, head.center().y - lsz / 2.0), vec2(lsz, lsz));
        if let Some(logo) = &self.logo { egui::Image::new(logo).rounding(9.0).paint_at(ui, logo_r); }
        if !compact {
            p.text(pos2(logo_r.right() + 11.0, head.center().y - 7.0), Align2::LEFT_CENTER, "Vire", bold(18.0), hud::C_TEXT);
            p.text(pos2(logo_r.right() + 11.0, head.center().y + 11.0), Align2::LEFT_CENTER, "Live Wallpaper", FontId::proportional(11.5), hud::C_TEXT2);
        }
        ui.add_space(24.0);
        if !compact {
            ui.label(RichText::new("Thư viện").size(11.5).color(hud::C_TEXT2));
            ui.add_space(-4.0);
        }

        for tab in Tab::ALL {
            if tab == Tab::Theme {
                ui.add_space(10.0);
                if !compact { ui.label(RichText::new("Tuỳ chỉnh").size(11.5).color(hud::C_TEXT2)); ui.add_space(-4.0); }
            }
            let selected = self.tab == tab;
            let (rect, resp) = ui.allocate_exact_size(vec2(inner.width(), 38.0), Sense::click());
            let t = ui.ctx().animate_bool_with_time(resp.id, resp.hovered(), 0.12);
            let s = ui.ctx().animate_bool_with_time(resp.id.with("sel"), selected, 0.2);
            if s > 0.0 { p.rect_filled(rect, 10.0, Color32::from_white_alpha((26.0 * s) as u8)); }
            if t > 0.0 && !selected { p.rect_filled(rect, 10.0, Color32::from_white_alpha((10.0 * t) as u8)); }
            let fg = if selected { hud::C_TEXT } else { Color32::from_rgb(200, 201, 208).lerp_to_gamma(hud::C_TEXT, t) };
            let ic = if compact { rect.center() } else { rect.left_center() + vec2(20.0, 0.0) };
            paint_icon(&p, ic, tab.into(), if selected { hud::accent() } else { hud::C_TEXT2.lerp_to_gamma(hud::C_TEXT, t) });
            if !compact { p.text(rect.left_center() + vec2(42.0, 0.0), Align2::LEFT_CENTER, tab.label(), FontId::proportional(14.0), fg); }
            let resp = if compact { resp.on_hover_text(tab.label()) } else { resp };
            if resp.on_hover_cursor(egui::CursorIcon::PointingHand).clicked() { self.tab = tab; }
            ui.add_space(2.0);
        }

        // Bottom: what the wallpaper is doing + the fold button.
        let fold = Rect::from_min_size(pos2(inner.left(), inner.bottom() - 36.0), vec2(inner.width(), 36.0));
        let fr = ui.interact(fold, ui.id().with("cine_fold"), Sense::click());
        let ft = ui.ctx().animate_bool_with_time(fr.id, fr.hovered(), 0.12);
        if ft > 0.0 { p.rect_filled(fold, 10.0, Color32::from_white_alpha((12.0 * ft) as u8)); }
        let fc = if compact { fold.center() } else { fold.left_center() + vec2(20.0, 0.0) };
        paint_icon(&p, fc, Icon::Fold(!compact), hud::C_TEXT2.lerp_to_gamma(hud::C_TEXT, ft));
        if !compact { p.text(fold.left_center() + vec2(42.0, 0.0), Align2::LEFT_CENTER, "Thu gọn", FontId::proportional(13.0), hud::C_TEXT2); }
        if fr.on_hover_text(if compact { "Mở rộng thanh bên" } else { "Thu gọn thanh bên" }).on_hover_cursor(egui::CursorIcon::PointingHand).clicked() {
            st.settings.ui_compact = !st.settings.ui_compact;
            st.save();
        }

        let playing = self.engine.status.playing.load(Ordering::Relaxed);
        let covered = self.engine.status.covered.load(Ordering::Relaxed) > 0;
        let (dot, label) = if st.settings.user_paused { (Color32::from_rgb(255, 204, 0), "Đã tạm dừng") }
            else if playing { (Color32::from_rgb(52, 199, 89), "Đang phát") }
            else if covered { (hud::C_TEXT2, "Nghỉ · desktop bị che") }
            else { (hud::C_TEXT2, "Đang chờ") };
        let sy = fold.top() - 22.0;
        if compact {
            p.circle_filled(pos2(inner.center().x, sy), 4.0, dot);
        } else {
            p.circle_filled(pos2(inner.left() + 20.0, sy), 4.0, dot);
            p.text(pos2(inner.left() + 34.0, sy), Align2::LEFT_CENTER, label, FontId::proportional(12.5), hud::C_TEXT2);
        }
    }

    /// The showcase page: the wallpaper in use on a big stage, the player beside it, then the
    /// library as a gallery of large thumbnails.
    pub(super) fn cine_gallery(&mut self, ui: &mut Ui, ctx: &egui::Context, st: &mut AppState) -> bool {
        let mut changed = false;
        egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
            let full_w = ui.available_width() - 4.0;
            let panel_w = (full_w * 0.27).clamp(270.0, 320.0);
            let stage_w = full_w - panel_w - 20.0;
            let stage_h = (stage_w * 9.0 / 16.0).clamp(300.0, 460.0);
            let (row, _) = ui.allocate_exact_size(vec2(full_w, stage_h), Sense::hover());
            let stage = Rect::from_min_size(row.min, vec2(stage_w, stage_h));
            let panel = Rect::from_min_size(pos2(stage.right() + 20.0, row.top()), vec2(panel_w, stage_h));
            let cur = st.selected().cloned();
            changed |= self.stage(ui, ctx, st, stage, cur.as_ref());
            changed |= self.music_panel(ui, ctx, st, panel, cur.as_ref());

            // ---- library header
            ui.add_space(30.0);
            ui.horizontal(|ui| {
                ui.label(RichText::new("Thư viện").font(bold(22.0)).color(Color32::WHITE));
                ui.label(RichText::new(format!("{} hình nền", st.library.items.len())).size(13.0).color(Color32::from_white_alpha(120)));
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    if pill_button(ui, Some(Icon::Plus), "Thêm video", true).on_hover_text("Hoặc kéo thả video (.mp4 .mov .webm…) vào cửa sổ").clicked() { self.pick_files(ctx); }
                    let videos = crate::model::videos_dir();
                    if pill_button(ui, Some(Icon::Folder), "Thư mục", false).on_hover_text(format!("Bỏ video vào đây, Vire tự nạp mỗi lần mở:\n{}", videos.display())).clicked() {
                        open_url(&videos.to_string_lossy());
                    }
                    self.find_more(ui);
                    ui.add(egui::TextEdit::singleline(&mut self.wall_q).hint_text("🔍  Tìm hình nền…").desired_width(220.0).margin(vec2(12.0, 8.0)));
                    if self.importing > 0 { ui.spinner(); }
                });
            });
            if let Some(err) = self.error.clone() {
                ui.horizontal(|ui| {
                    ui.colored_label(Color32::from_rgb(252, 165, 165), format!("⚠ {err}"));
                    if ui.small_button("Đóng").clicked() { self.error = None; }
                });
            }
            if let Some(n) = self.notice.clone() {
                ui.horizontal(|ui| {
                    ui.label(RichText::new(format!("✓ {n}")).color(Color32::from_rgb(134, 239, 172)));
                    if ui.small_button("Đóng").clicked() { self.notice = None; }
                });
            }
            ui.add_space(14.0);
            changed |= self.cine_grid(ui, ctx, st);
            ui.add_space(20.0);
        });
        changed
    }

    /// The big stage: the wallpaper in use, its name and state, and the two controls it needs
    /// (pause, pick the matching song). Volume / seek live on the player beside it.
    fn stage(&mut self, ui: &mut Ui, ctx: &egui::Context, st: &mut AppState, r: Rect, cur: Option<&WallpaperItem>) -> bool {
        let mut changed = false;
        let rad = 24.0;
        {
            let p = ui.painter();
            p.rect_filled(r.translate(vec2(0.0, 14.0)).expand(4.0), rad + 6.0, Color32::from_black_alpha(70));
            p.rect_filled(r.translate(vec2(0.0, 4.0)).expand(1.0), rad + 1.0, Color32::from_black_alpha(90));
        }
        match cur.and_then(|i| self.texture_for(ctx, i)) {
            Some(tex) => { egui::Image::new(&tex).uv(cover_uv(tex.size(), r)).rounding(rad).paint_at(ui, r); }
            None => {
                ui.painter().rect_filled(r, rad, Color32::from_rgb(24, 22, 36));
                ui.painter().text(r.center(), Align2::CENTER_CENTER, "Chưa có hình nền — thêm một video để bắt đầu", semi(16.0), Color32::from_white_alpha(150));
            }
        }
        let shade = Rect::from_min_max(pos2(r.left(), r.top() + r.height() * 0.38), r.max);
        egui::Image::new(&self.shade).rounding(rad).paint_at(ui, shade);
        ui.painter().rect_stroke(r, rad, Stroke::new(1.0_f32, Color32::from_white_alpha(26)));

        // "Dynamic Island": one black capsule, top-centre — state · screens · matched song.
        // It widens a little when you point at the stage.
        let playing = self.engine.status.playing.load(Ordering::Relaxed);
        let covered = self.engine.status.covered.load(Ordering::Relaxed) > 0;
        let (label, dot) = if st.settings.user_paused { ("Tạm dừng", Color32::from_rgb(255, 204, 0)) }
            else if playing { ("Đang phát", Color32::from_rgb(52, 199, 89)) }
            else if covered { ("Đang nghỉ", hud::C_TEXT2) }
            else { ("Tự dừng", Color32::from_rgb(255, 204, 0)) };
        let mons = self.engine.status.monitors.lock().unwrap().len();
        let mut parts = vec![label.to_string(), format!("{mons} màn hình")];
        if let Some(t) = cur.and_then(|c| c.music_label()) { parts.push(format!("♪  {t}")); }
        let hov = ui.ctx().animate_bool_with_time(ui.id().with("island"), ui.rect_contains_pointer(r), 0.25);
        {
            let p = ui.painter();
            let font = FontId::proportional(12.5);
            let gs: Vec<_> = parts.iter().map(|s| p.layout_no_wrap(elide(p, s, font.clone(), r.width() * 0.32), font.clone(), hud::C_TEXT)).collect();
            let sep = 22.0 + 6.0 * hov;
            let w = gs.iter().map(|g| g.size().x).sum::<f32>() + sep * (gs.len() as f32 - 1.0) + 46.0;
            let island = Rect::from_center_size(pos2(r.center().x, r.top() + 30.0), vec2(w, 32.0 + 2.0 * hov));
            p.rect_filled(island.translate(vec2(0.0, 4.0)), 18.0, Color32::from_black_alpha(60));
            p.rect_filled(island, 17.0, Color32::from_rgba_unmultiplied(0, 0, 0, 225));
            p.rect_stroke(island, 17.0, Stroke::new(1.0_f32, Color32::from_white_alpha(14)));
            let mut x = island.left() + 18.0;
            p.circle_filled(pos2(x + 3.0, island.center().y), 3.5, dot);
            x += 13.0;
            for (i, g) in gs.into_iter().enumerate() {
                if i > 0 { p.circle_filled(pos2(x + sep / 2.0 - 4.0, island.center().y), 1.5, hud::C_TEXT2); x += sep - 8.0; }
                let col = if i == 0 { hud::C_TEXT } else { Color32::from_rgb(200, 201, 208) };
                let gw = g.size().x;
                p.galley(pos2(x, island.center().y - g.size().y / 2.0), g, col);
                x += gw + 8.0;
            }
        }

        let Some(cur) = cur else { return changed };
        // Title block, bottom-left.
        let fit = if st.settings.fit == FitMode::Fill { "Lấp đầy" } else { "Vừa khung" };
        let p = ui.painter();
        p.text(pos2(r.left() + 30.0, r.bottom() - 92.0), Align2::LEFT_BOTTOM, "ĐANG TRÌNH CHIẾU", semi(11.5), hud::with_alpha(hud::accent().lerp_to_gamma(Color32::WHITE, 0.45), 220));
        let title = elide(p, &cur.title, bold(36.0), r.width() - 220.0);
        p.text(pos2(r.left() + 28.0, r.bottom() - 46.0), Align2::LEFT_BOTTOM, title, bold(36.0), Color32::WHITE);
        p.text(pos2(r.left() + 30.0, r.bottom() - 24.0), Align2::LEFT_BOTTOM,
            format!("{}   ·   {}   ·   {}", cur.resolution_text(), cur.duration_text(), fit), FontId::proportional(13.5), Color32::from_white_alpha(185));

        // Controls, bottom-right: matched song picker + a large play / pause.
        let play = Rect::from_center_size(pos2(r.right() - 58.0, r.bottom() - 56.0), vec2(60.0, 60.0));
        let pr = ui.interact(play, ui.id().with("stage_play"), Sense::click());
        let t = ui.ctx().animate_bool_with_time(pr.id, pr.hovered(), 0.12);
        let p = ui.painter();
        p.circle_filled(play.center() + vec2(0.0, 4.0), 30.0, Color32::from_black_alpha(70));
        p.circle_filled(play.center(), 28.0 + 2.0 * t, hud::C_TEXT);
        let paused = st.settings.user_paused;
        paint_icon(p, play.center() + vec2(if paused { 1.5 } else { 0.0 }, 0.0), if paused { Icon::Play } else { Icon::Pause }, hud::C_CHARCOAL);
        if pr.on_hover_text(if paused { "Phát hình nền" } else { "Tạm dừng hình nền" }).on_hover_cursor(egui::CursorIcon::PointingHand).clicked() {
            st.settings.user_paused = !paused; changed = true;
        }
        let mr = Rect::from_center_size(pos2(play.left() - 34.0, play.center().y), vec2(44.0, 44.0));
        let resp = round_button(ui, mr, Icon::Music, "stage_music");
        let tip = match &cur.music_label() { Some(t) => format!("Nhạc đi kèm: {t}\nBấm để đổi"), None => "Chọn nhạc đi kèm hình nền này".to_string() };
        let resp = resp.on_hover_text(tip);
        let pid = ui.make_persistent_id("stage_music_pop");
        if resp.clicked() { ui.memory_mut(|m| m.toggle_popup(pid)); }
        let mut act = MusicAct::None;
        egui::popup_above_or_below_widget(ui, pid, &resp, egui::AboveOrBelow::Above, egui::PopupCloseBehavior::CloseOnClickOutside, |ui| {
            ui.set_min_width(240.0);
            act = music_menu(ui, &st.library.songs, &cur.songs());
        });
        if !matches!(act, MusicAct::None) { changed |= self.apply_music(st, &cur.id, act); }
        changed
    }

    /// Library as a gallery of big thumbnails. Click = use it; right-click = rename / music / delete.
    fn cine_grid(&mut self, ui: &mut Ui, ctx: &egui::Context, st: &mut AppState) -> bool {
        let mut changed = false;
        let wq = self.wall_q.trim().to_lowercase();
        let items: Vec<WallpaperItem> = st.library.items.iter().filter(|i| wq.is_empty() || i.title.to_lowercase().contains(&wq)).cloned().collect();
        if items.is_empty() {
            ui.label(RichText::new(if wq.is_empty() { "Thư viện trống — bấm “Thêm video” hoặc kéo thả video vào đây." } else { "Không có hình nền nào khớp." }).size(14.0).color(Color32::from_white_alpha(140)));
            return false;
        }
        let gap = 22.0;
        let avail = ui.available_width() - 4.0;
        let cols = ((avail + gap) / (300.0 + gap)).floor().max(1.0) as usize;
        let card_w = (avail - gap * (cols as f32 - 1.0)) / cols as f32;
        let img_h = card_w * 9.0 / 16.0;
        let selected = st.settings.selected_id.clone();
        let (mut delete, mut rename_req, mut music_act, mut music_for) = (None::<String>, None::<(String, String)>, MusicAct::None, String::new());
        for row in items.chunks(cols) {
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = gap;
                for item in row {
                    let is_sel = selected.as_deref() == Some(item.id.as_str());
                    let (cell, resp) = ui.allocate_exact_size(vec2(card_w, img_h + 52.0), Sense::click());
                    let t = ui.ctx().animate_bool_with_time(resp.id, resp.contains_pointer(), 0.16);
                    let lift = 4.0 * t;
                    let img = Rect::from_min_size(cell.min - vec2(0.0, lift), vec2(card_w, img_h));
                    let rad = 18.0;
                    {
                        let p = ui.painter();
                        p.rect_filled(img.translate(vec2(0.0, 8.0 + 4.0 * t)), rad, Color32::from_black_alpha((60.0 + 50.0 * t) as u8));
                        if is_sel { p.rect_filled(img.expand(5.0), rad + 5.0, hud::with_alpha(hud::accent(), 60)); }
                    }
                    match self.texture_for(ctx, item) {
                        Some(tex) => { egui::Image::new(&tex).uv(cover_uv(tex.size(), img)).rounding(rad).paint_at(ui, img); }
                        None => { ui.painter().rect_filled(img, rad, Color32::from_rgb(30, 28, 44)); }
                    }
                    let p = ui.painter();
                    if t > 0.0 && !is_sel { p.rect_filled(img, rad, Color32::from_black_alpha((70.0 * t) as u8)); }
                    let edge = if is_sel { hud::accent() } else { Color32::from_white_alpha((24.0 + 50.0 * t) as u8) };
                    p.rect_stroke(img, rad, Stroke::new(if is_sel { 2.0_f32 } else { 1.0 }, edge));
                    if is_sel { chip(p, img.min + vec2(12.0, 12.0), "Đang dùng", tx(), Some(hud::accent())); }
                    else if item.built_in && t < 0.5 { let g = p.layout_no_wrap("Mẫu".into(), semi(11.5), tx()); chip(p, pos2(img.right() - g.size().x - 34.0, img.top() + 12.0), "Mẫu", tx(), None); }
                    if item.music.is_some() && !(t > 0.05 && !item.built_in) {
                        let c = pos2(img.right() - 24.0, img.top() + 24.0);
                        p.circle_filled(c, 15.0, Color32::from_rgba_unmultiplied(14, 12, 22, 190));
                        paint_icon(p, c, Icon::Music, hud::accent().lerp_to_gamma(Color32::WHITE, 0.45));
                    }
                    if !is_sel && t > 0.05 {
                        let label = "Đặt làm hình nền";
                        let g = p.layout_no_wrap(label.into(), semi(13.0), Color32::WHITE);
                        let pill = Rect::from_center_size(img.center(), vec2(g.size().x + 34.0, 38.0));
                        let a = (255.0 * t) as u8;
                        p.rect_filled(pill, 19.0, hud::with_alpha(hud::accent(), a));
                        p.text(pill.center(), Align2::CENTER_CENTER, label, semi(13.0), hud::with_alpha(hud::on_accent(), a));
                    }
                    // Caption under the picture.
                    let title = elide(p, &item.title, semi(15.0), card_w - 8.0);
                    p.text(pos2(cell.left() + 4.0, img.bottom() + 14.0), Align2::LEFT_TOP, title, semi(15.0), Color32::WHITE);
                    p.text(pos2(cell.left() + 4.0, img.bottom() + 35.0), Align2::LEFT_TOP,
                        format!("{}  ·  {}", item.resolution_text(), item.duration_text()), FontId::proportional(12.5), Color32::from_white_alpha(130));
                    let mut del_hit = false;
                    if t > 0.05 && !item.built_in {
                        let del = Rect::from_center_size(pos2(img.right() - 24.0, img.top() + 24.0), vec2(34.0, 34.0));
                        if round_button(ui, del, Icon::Trash, &format!("cdel{}", item.id)).on_hover_text("Xoá khỏi thư viện").clicked() {
                            delete = Some(item.id.clone()); del_hit = true;
                        }
                    }
                    if resp.clicked() && !del_hit && !is_sel { self.pick_wallpaper(st, &item.id); changed = true; }
                    let resp = resp.on_hover_cursor(egui::CursorIcon::PointingHand);
                    resp.context_menu(|ui| {
                        if ui.button("Đổi tên").clicked() { rename_req = Some((item.id.clone(), item.title.clone())); ui.close_menu(); }
                        ui.separator();
                        ui.menu_button("Nhạc nền", |ui| {
                            let a = music_menu(ui, &st.library.songs, &item.songs());
                            if !matches!(a, MusicAct::None) { music_act = a; music_for = item.id.clone(); }
                        });
                        if !item.built_in {
                            ui.separator();
                            if ui.button("Xoá khỏi thư viện").clicked() { delete = Some(item.id.clone()); ui.close_menu(); }
                        }
                    });
                }
            });
            ui.add_space(gap - 8.0);
        }
        if rename_req.is_some() { self.rename = rename_req; self.rename_focus = true; }
        if !matches!(music_act, MusicAct::None) { changed |= self.apply_music(st, &music_for, music_act); }
        if let Some(id) = delete { st.remove(&id); self.textures.remove(&id); changed = true; }
        changed
    }

    // ---------- "Giao diện" page (both looks) ----------

    pub(super) fn theme_tab(&mut self, ui: &mut Ui, st: &mut AppState) {
        let ctx = ui.ctx().clone();
        page_title(ui, "Giao diện", "Chọn phong cách cho bảng điều khiển. Mọi chức năng, dữ liệu và cài đặt giữ nguyên.");
        ui.add_space(18.0);
        let cur = st.selected().cloned();
        let thumb = cur.as_ref().and_then(|c| self.texture_for(&ctx, c));
        let blur = cur.as_ref().and_then(|c| self.blur_for(&ctx, c));
        egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
            let gap = 22.0;
            let avail = ui.available_width() - 4.0;
            let w = ((avail - gap * 2.0) / 3.0).max(240.0);
            let looks: [(u8, &str, &str); 2] = [
                (1, "Cinematic Dark", "Cảm hứng macOS / iOS: kính mờ, bo góc mềm, màu nhấn tự lấy theo hình nền."),
                (0, "Cyberpunk HUD", "Neon cyan / magenta, góc vát 45°, chữ mono, hiệu ứng glitch và scanline."),
            ];
            ui.horizontal_top(|ui| {
                ui.spacing_mut().item_spacing.x = gap;
                for (id, name, desc) in looks {
                    let on = st.settings.ui_theme == id;
                    let (card, resp) = ui.allocate_exact_size(vec2(w, w * 0.62 + 128.0), Sense::click());
                    let t = ui.ctx().animate_bool_with_time(resp.id, resp.hovered(), 0.14);
                    let p = ui.painter().clone();
                    let soft = hud::soft();
                    let rad = if soft { 20.0 } else { 0.0 };
                    let edge = if on { if soft { hud::accent() } else { CYAN } } else { Color32::from_white_alpha((26.0 + 40.0 * t) as u8) };
                    p.rect_filled(card, rad, if soft { Color32::from_white_alpha(10) } else { self.pal.card });
                    p.rect_stroke(card, rad, Stroke::new(if on { 2.0_f32 } else { 1.0 }, edge));
                    let pv = Rect::from_min_size(card.min + vec2(14.0, 14.0), vec2(w - 28.0, w * 0.62 - 14.0));
                    if id == 1 { preview_cinematic(&p, pv, blur.as_ref(), thumb.as_ref()); } else { preview_hud(&p, pv, thumb.as_ref()); }
                    p.text(pos2(card.left() + 18.0, pv.bottom() + 22.0), Align2::LEFT_CENTER, name, bold(18.0), Color32::WHITE);
                    let g = p.layout(desc.to_string(), FontId::proportional(12.5), Color32::from_white_alpha(150), w - 36.0);
                    p.galley(pos2(card.left() + 18.0, pv.bottom() + 38.0), g, Color32::from_white_alpha(150));
                    let b = Rect::from_min_size(pos2(card.left() + 18.0, card.bottom() - 46.0), vec2(w - 36.0, 32.0));
                    let (bf, bt, btxt) = if on { (Color32::from_white_alpha(14), Color32::from_white_alpha(170), "✓  Đang dùng") }
                        else { let f = if soft { hud::accent() } else { CYAN }; (f, if soft { hud::on_color(f) } else { Color32::from_rgb(14, 10, 26) }, "Dùng giao diện này") };
                    p.rect_filled(b, if soft { 16.0 } else { 0.0 }, bf);
                    p.text(b.center(), Align2::CENTER_CENTER, btxt, semi(13.0), bt);
                    if resp.on_hover_cursor(egui::CursorIcon::PointingHand).clicked() && !on {
                        st.settings.ui_theme = id;
                        st.save();
                    }
                }
                // Placeholder for the looks still to come.
                let (card, _) = ui.allocate_exact_size(vec2(w, w * 0.62 + 128.0), Sense::hover());
                let p = ui.painter();
                let rad = if hud::soft() { 20.0 } else { 0.0 };
                p.rect_stroke(card, rad, Stroke::new(1.0_f32, Color32::from_white_alpha(30)));
                p.text(card.center() - vec2(0.0, 14.0), Align2::CENTER_CENTER, "+", bold(34.0), Color32::from_white_alpha(90));
                p.text(card.center() + vec2(0.0, 24.0), Align2::CENTER_CENTER, "Bộ giao diện mới sắp có", semi(13.5), Color32::from_white_alpha(120));
            });
            if st.settings.ui_theme == 1 {
                section_label(ui, "Tuỳ chọn Cinematic");
                ui.add_space(8.0);
                let mut compact = st.settings.ui_compact;
                self.group(ui, |ui| {
                    if self.row(ui, "Thu gọn thanh bên", "Chỉ hiện biểu tượng, nhường chỗ cho hình nền", |ui| toggle(ui, &mut compact).changed()) {
                        st.settings.ui_compact = compact;
                        st.save();
                    }
                });
            }
            ui.add_space(16.0);
        });
    }
}

/// Miniature of the Cinematic look (for the picker).
fn preview_cinematic(p: &egui::Painter, r: Rect, blur: Option<&TextureHandle>, thumb: Option<&TextureHandle>) {
    p.rect_filled(r, 14.0, hud::C_BG);
    if let Some(b) = blur { p.image(b.id(), r, cover_uv(b.size(), r), Color32::from_white_alpha(255)); }
    p.rect_filled(r, 14.0, Color32::from_rgba_unmultiplied(7, 7, 12, 170));
    let side = Rect::from_min_size(r.min + vec2(8.0, 8.0), vec2(r.width() * 0.16, r.height() - 16.0));
    p.rect_filled(side, 8.0, Color32::from_rgba_unmultiplied(30, 28, 44, 200));
    for i in 0..5 {
        let y = side.top() + 18.0 + i as f32 * 12.0;
        p.rect_filled(Rect::from_min_size(pos2(side.left() + 6.0, y), vec2(side.width() - 12.0, 7.0)), 3.5, if i == 0 { hud::with_alpha(hud::accent(), 150) } else { Color32::from_white_alpha(30) });
    }
    let stage = Rect::from_min_size(pos2(side.right() + 10.0, r.top() + 10.0), vec2(r.right() - side.right() - 18.0, r.height() * 0.56));
    match thumb {
        Some(t) => { p.image(t.id(), stage, cover_uv(t.size(), stage), Color32::WHITE); }
        None => { p.rect_filled(stage, 8.0, Color32::from_rgb(40, 36, 60)); }
    }
    p.rect_stroke(stage, 8.0, Stroke::new(1.0_f32, Color32::from_white_alpha(40)));
    p.circle_filled(stage.right_bottom() - vec2(14.0, 14.0), 7.0, hud::accent());
    let cw = (stage.width() - 12.0) / 3.0;
    for i in 0..3 {
        let c = Rect::from_min_size(pos2(stage.left() + i as f32 * (cw + 6.0), stage.bottom() + 10.0), vec2(cw, r.bottom() - stage.bottom() - 18.0));
        p.rect_filled(c, 6.0, Color32::from_white_alpha(if i == 0 { 46 } else { 24 }));
    }
}

/// Miniature of the HUD look.
fn preview_hud(p: &egui::Painter, r: Rect, thumb: Option<&TextureHandle>) {
    p.rect_filled(r, 0.0, Color32::from_rgb(5, 5, 5));
    let grid = Color32::from_rgba_unmultiplied(0, 229, 255, 16);
    let mut x = r.left();
    while x < r.right() { p.line_segment([pos2(x, r.top()), pos2(x, r.bottom())], Stroke::new(1.0_f32, grid)); x += 14.0; }
    let mut y = r.top();
    while y < r.bottom() { p.line_segment([pos2(r.left(), y), pos2(r.right(), y)], Stroke::new(1.0_f32, grid)); y += 14.0; }
    let cyan = Color32::from_rgb(0, 229, 255);
    let side = Rect::from_min_size(r.min + vec2(8.0, 8.0), vec2(r.width() * 0.18, r.height() - 16.0));
    p.rect_stroke(side, 0.0, Stroke::new(1.0_f32, hud::with_alpha(cyan, 120)));
    for i in 0..5 {
        let yy = side.top() + 18.0 + i as f32 * 12.0;
        let b = Rect::from_min_size(pos2(side.left() + 5.0, yy), vec2(side.width() - 10.0, 7.0));
        if i == 0 { p.rect_filled(b, 0.0, cyan); } else { p.rect_stroke(b, 0.0, Stroke::new(1.0_f32, hud::with_alpha(cyan, 60))); }
    }
    let stage = Rect::from_min_size(pos2(side.right() + 10.0, r.top() + 10.0), vec2(r.right() - side.right() - 18.0, r.height() * 0.52));
    match thumb {
        Some(t) => { p.image(t.id(), stage, cover_uv(t.size(), stage), Color32::WHITE); }
        None => { p.rect_filled(stage, 0.0, Color32::from_rgb(10, 14, 20)); }
    }
    p.rect_stroke(stage, 0.0, Stroke::new(1.0_f32, cyan));
    p.add(Shape::convex_polygon(vec![stage.left_top(), stage.left_top() + vec2(12.0, 0.0), stage.left_top() + vec2(0.0, 12.0)], Color32::from_rgb(5, 5, 5), Stroke::NONE));
    p.rect_filled(Rect::from_min_size(pos2(stage.left(), stage.bottom() + 8.0), vec2(40.0, 3.0)), 0.0, Color32::from_rgb(252, 238, 9));
    let cw = (stage.width() - 12.0) / 3.0;
    for i in 0..3 {
        let c = Rect::from_min_size(pos2(stage.left() + i as f32 * (cw + 6.0), stage.bottom() + 16.0), vec2(cw, r.bottom() - stage.bottom() - 24.0));
        p.rect_stroke(c, 0.0, Stroke::new(1.0_f32, if i == 0 { Color32::from_rgb(255, 0, 255) } else { hud::with_alpha(cyan, 90) }));
    }
}
