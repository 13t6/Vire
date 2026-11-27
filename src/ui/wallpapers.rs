//! The Cyberpunk look's sidebar and Wallpaper page (gallery + hero preview).

use super::*;

impl App {
    // ---------- Sidebar ----------
    pub(super) fn sidebar(&mut self, ui: &mut Ui, st: &AppState) {
        ui.add_space(14.0);
        // Logo block with brackets and a system tag.
        let (head, _) = ui.allocate_exact_size(vec2(ui.available_width(), 62.0), Sense::hover());
        if let Some(logo) = &self.logo { egui::Image::new(logo).fit_to_exact_size(vec2(44.0, 44.0)).paint_at(ui, Rect::from_min_size(head.min + vec2(6.0, 8.0), vec2(44.0, 44.0))); }
        hud::brackets(ui.painter(), Rect::from_min_size(head.min + vec2(2.0, 4.0), vec2(52.0, 52.0)), 9.0, CYAN);
        hud::glow_text(ui.painter(), head.min + vec2(66.0, 22.0), Align2::LEFT_CENTER, "VIRE", bold(22.0), CYAN);
        ui.painter().text(head.min + vec2(66.0, 42.0), Align2::LEFT_CENTER, "LIVE//WALLPAPER", FontId::monospace(9.5), muted());
        ui.add_space(14.0);
        ui.painter().rect_filled(Rect::from_min_size(pos2(ui.min_rect().left(), ui.cursor().top()), vec2(ui.available_width(), 1.0)), 0.0, hud::with_alpha(CYAN, 50));
        ui.add_space(14.0);
        for (i, tab) in Tab::ALL.into_iter().enumerate() {
            let selected = self.tab == tab;
            let (rect, resp) = ui.allocate_exact_size(vec2(ui.available_width(), 38.0), Sense::click());
            let t = ui.ctx().animate_bool_with_time(resp.id, resp.hovered(), 0.07);
            let solid = selected || t > 0.5;
            let neon = if selected { CYAN } else { MAGENTA };
            if solid { hud::neon_panel(ui.painter(), rect, 9.0, neon, neon, 0.8); }
            else { hud::neon_panel(ui.painter(), rect, 9.0, hud::with_alpha(BG0, 200), hud::with_alpha(CYAN, 40), 0.0); }
            let fg = if solid { BG0 } else { subtle() };
            paint_icon(ui.painter(), rect.left_center() + vec2(22.0, 0.0), tab.into(), fg);
            hud::glitch_text(ui.painter(), rect.left_center() + vec2(44.0, 0.0), Align2::LEFT_CENTER, &tab.label().to_uppercase(), semi(12.5), fg, if selected { 0.0 } else { t });
            ui.painter().text(rect.right_center() - vec2(10.0, 0.0), Align2::RIGHT_CENTER, format!("{:02}", i + 1), FontId::monospace(10.0), if solid { hud::with_alpha(BG0, 170) } else { hud::with_alpha(CYAN, 70) });
            if resp.on_hover_cursor(egui::CursorIcon::PointingHand).clicked() { self.tab = tab; }
            ui.add_space(4.0);
        }

        ui.with_layout(Layout::bottom_up(Align::Min), |ui| {
            ui.add_space(14.0);
            let playing = self.engine.status.playing.load(Ordering::Relaxed);
            let auto = self.engine.status.auto_paused.load(Ordering::Relaxed);
            let covered = self.engine.status.covered.load(Ordering::Relaxed);
            let mons = self.engine.status.monitors.lock().unwrap().len();
            let (dot, title) = if playing { (GREEN, "Đang phát") }
                else if st.settings.user_paused { (AMBER, "Đã tạm dừng") }
                else if auto { (AMBER, "Tự dừng · pin / toàn màn hình") }
                else if covered > 0 { (ICE, "Nghỉ · desktop bị che") }
                else { (muted(), "Chưa có hình nền") };
            let sub = format!("{mons} MÀN HÌNH{}", if st.settings.vd_enabled { " · VIRE DOCK" } else { "" });
            let (rect, _) = ui.allocate_exact_size(vec2(ui.available_width(), 118.0), Sense::hover());
            let p = ui.painter();
            hud::neon_panel(p, rect, 12.0, hud::with_alpha(self.pal.card, 240), hud::with_alpha(CYAN, 130), 0.5);
            p.text(rect.min + vec2(12.0, 14.0), Align2::LEFT_CENTER, "STATUS//", FontId::monospace(10.0), hud::with_alpha(CYAN, 150));
            p.text(rect.right_top() + vec2(-12.0, 14.0), Align2::RIGHT_CENTER, hud::hex(mons as u32 * 31 + playing as u32, 4), FontId::monospace(10.0), hud::with_alpha(MAGENTA, 170));
            p.rect_filled(Rect::from_center_size(rect.min + vec2(17.0, 38.0), vec2(8.0, 8.0)), 0.0, dot);
            p.text(rect.min + vec2(30.0, 38.0), Align2::LEFT_CENTER, elide(p, &title.to_uppercase(), semi(11.5), rect.width() - 44.0), semi(11.5), tx());
            p.text(rect.min + vec2(12.0, 58.0), Align2::LEFT_CENTER, sub, FontId::monospace(10.5), muted());
            hud::barcode(p, Rect::from_min_size(rect.min + vec2(12.0, 76.0), vec2(rect.width() - 24.0, 8.0)), 4242 + mons as u32, hud::with_alpha(CYAN, 110));
            let blink = (ui.input(|i| i.time) * 1.6) as i64 % 2 == 0;
            ui.painter().text(rect.min + vec2(12.0, 100.0), Align2::LEFT_CENTER, format!("> READY{}", if blink { "_" } else { " " }), FontId::monospace(10.5), GREEN);
            hud::crosshair(ui.painter(), rect.right_bottom() - vec2(18.0, 18.0), 7.0, hud::with_alpha(YELLOW, 160));
        });
    }

    // ---------- Gallery ----------
    pub(super) fn gallery(&mut self, ui: &mut Ui, ctx: &egui::Context, st: &mut AppState) -> bool {
        let mut changed = false;
        ui.horizontal(|ui| {
            ui.vertical(|ui| page_title(ui, "Hình nền", "Hình nền động nằm ngay dưới icon desktop của bạn."));
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                if pill_button(ui, Some(Icon::Plus), "Thêm video", true).on_hover_text("Hoặc kéo thả video (.mp4 .mov .webm…) vào cửa sổ").clicked() { self.pick_files(ctx); }
                let videos = crate::model::videos_dir();
                if pill_button(ui, Some(Icon::Folder), "Thư mục video", false).on_hover_text(format!("Bỏ video vào đây, Vire tự nạp mỗi lần mở:\n{}\n(nhạc: thư mục Music cùng cấp)", videos.display())).clicked() {
                    open_url(&videos.to_string_lossy());
                }
                if self.importing > 0 { ui.spinner(); ui.label(RichText::new("Đang nhập…").color(muted())); }
            });
        });
        if let Some(err) = self.error.clone() {
            ui.horizontal(|ui| {
                ui.colored_label(Color32::from_rgb(255, 128, 128), format!("⚠ {err}"));
                if ui.small_button("Đóng").clicked() { self.error = None; }
            });
        }
        if let Some(n) = self.notice.clone() {
            ui.horizontal(|ui| {
                ui.label(RichText::new(format!("✓ {n}")).color(GREEN));
                if ui.small_button("Đóng").clicked() { self.notice = None; }
            });
        }
        ui.add_space(14.0);

        egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
            changed |= self.hero(ui, ctx, st);
            let wq = self.wall_q.trim().to_lowercase();
            let items: Vec<WallpaperItem> = st.library.items.iter().filter(|i| wq.is_empty() || i.title.to_lowercase().contains(&wq)).cloned().collect();
            section_label(ui, &if wq.is_empty() { format!("Bộ sưu tập  ·  {}", items.len()) } else { format!("Kết quả  ·  {} / {}", items.len(), st.library.items.len()) });
            // Search wallpapers by name.
            ui.add_space(4.0);
            ui.add(egui::TextEdit::singleline(&mut self.wall_q).hint_text("> TÌM HÌNH NỀN THEO TÊN…").desired_width(ui.available_width().min(340.0)).font(FontId::monospace(12.5)).margin(vec2(10.0, 7.0)));
            if items.is_empty() { ui.label(RichText::new("> KHÔNG CÓ HÌNH NỀN KHỚP").font(semi(12.0)).color(AMBER)); }
            ui.add_space(10.0);

            let gap = 16.0;
            let avail = ui.available_width() - 2.0;
            let cols = ((avail + gap) / (220.0 + gap)).floor().max(1.0) as usize;
            let card_w = (avail - gap * (cols as f32 - 1.0)) / cols as f32;
            let card_h = card_w * 9.0 / 16.0;
            let selected = st.settings.selected_id.clone();
            let mut delete: Option<String> = None;
            let mut rename_req: Option<(String, String)> = None;
            let mut music_req: Option<String> = None;
            let mut music_act = MusicAct::None;
            let mut music_for = String::new();
            for row in items.chunks(cols) {
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = gap;
                    for item in row {
                        let is_sel = selected.as_deref() == Some(item.id.as_str());
                        let (rect, resp) = ui.allocate_exact_size(vec2(card_w, card_h), Sense::click());
                        // `contains_pointer` (not `hovered`): the little buttons drawn on the card would
                        // otherwise steal the hover, hide themselves, give it back… = flicker.
                        let t = ui.ctx().animate_bool_with_time(resp.id, resp.contains_pointer(), 0.15);
                        self.paint_media(ui, ctx, Some(item), rect, 16.0, 0.45);
                        let p = ui.painter();
                        if t > 0.0 && !is_sel { p.rect_filled(rect, Rounding::ZERO, hud::with_alpha(MAGENTA, (34.0 * t) as u8)); }
                        // Neon frame: cyan when picked, magenta on hover, dim cyan otherwise; 45° cut corners.
                        let edge = if is_sel { CYAN } else if t > 0.3 { MAGENTA } else { hud::with_alpha(CYAN, 90) };
                        p.rect_stroke(rect, Rounding::ZERO, Stroke::new(if is_sel { 2.0_f32 } else { 1.0_f32 }, edge));
                        if is_sel || t > 0.3 { p.rect_stroke(rect.expand(2.0), Rounding::ZERO, Stroke::new(3.0_f32, hud::with_alpha(edge, 30))); }
                        hud::cut_corners(p, rect.expand(2.0), 16.0, BG1, edge);
                        let title = elide(p, &item.title, semi(14.5), rect.width() - 28.0);
                        p.text(pos2(rect.left() + 14.0, rect.bottom() - 30.0), Align2::LEFT_BOTTOM, title, semi(14.5), Color32::WHITE);
                        p.text(pos2(rect.left() + 14.0, rect.bottom() - 12.0), Align2::LEFT_BOTTOM,
                            format!("{}  ·  {}", item.resolution_text(), item.duration_text()), FontId::proportional(11.5), Color32::from_white_alpha(170));
                        if is_sel { chip(p, rect.min + vec2(10.0, 10.0), "Đang phát", LAVENDER, Some(LAVENDER)); }
                        else if item.built_in && t < 0.5 { let g = p.layout_no_wrap("Mẫu".into(), semi(11.0), ICE); chip(p, pos2(rect.right() - g.size().x - 28.0, rect.top() + 10.0), "Mẫu", ICE, None); }
                        if !is_sel && t > 0.05 {
                            let label = "> ĐẶT LÀM HÌNH NỀN";
                            let g = p.layout_no_wrap(label.into(), semi(12.0), BG0);
                            let pill = Rect::from_center_size(rect.center() - vec2(0.0, 10.0), vec2(g.size().x + 28.0, 32.0));
                            hud::neon_panel(p, pill, 8.0, hud::with_alpha(YELLOW, (255.0 * t) as u8), hud::with_alpha(YELLOW, (255.0 * t) as u8), t);
                            p.text(pill.center(), Align2::CENTER_CENTER, label, semi(12.0), hud::with_alpha(BG0, (255.0 * t) as u8));
                        }
                        // A small note badge marks wallpapers that have music.
                        if item.music.is_some() && !(t > 0.05 && !item.built_in) {
                            let c = pos2(rect.right() - 22.0, rect.top() + 22.0);
                            hud::neon_panel(ui.painter(), Rect::from_center_size(c, vec2(26.0, 26.0)), 7.0, hud::with_alpha(BG0, 215), MAGENTA, 0.6);
                            paint_icon(ui.painter(), c, Icon::Music, MAGENTA);
                        }
                        let mut del_hit = false;
                        if t > 0.05 && !item.built_in {
                            // Hover tool, top-right: delete. (Rename & music live in the right-click menu.)
                            let del = Rect::from_center_size(pos2(rect.right() - 22.0, rect.top() + 22.0), vec2(30.0, 30.0));
                            if round_button(ui, del, Icon::Trash, &format!("del{}", item.id)).on_hover_text("Xoá khỏi thư viện").clicked() {
                                delete = Some(item.id.clone()); del_hit = true;
                            }
                        }
                        if resp.clicked() && !del_hit && !is_sel {
                            self.pick_wallpaper(st, &item.id);
                            changed = true;
                        }
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
                ui.add_space(gap - 10.0);
            }
            if rename_req.is_some() { self.rename = rename_req.take(); self.rename_focus = true; }
            if music_req.is_some() { self.music_req = music_req.take(); }
            if !matches!(music_act, MusicAct::None) { changed |= self.apply_music(st, &music_for, music_act); }
            if let Some(id) = delete {
                st.remove(&id);
                self.textures.remove(&id);
                changed = true;
            }
            ui.add_space(16.0);
        });
        changed
    }

    /// Big "now playing" card with floating controls.
    pub(super) fn hero(&mut self, ui: &mut Ui, ctx: &egui::Context, st: &mut AppState) -> bool {
        let mut changed = false;
        // The wallpaper preview on the left, the compact music player on the right.
        let full_w = ui.available_width() - 2.0;
        let panel_w = (full_w * 0.36).clamp(260.0, 340.0);
        let w = full_w - panel_w - 14.0;
        let h = 312.0;
        let (full, _) = ui.allocate_exact_size(vec2(full_w, h), Sense::hover());
        let rect = Rect::from_min_size(full.min, vec2(w, h));
        let panel = Rect::from_min_size(pos2(full.min.x + w + 14.0, full.min.y), vec2(panel_w, h));
        let cur = st.selected().cloned();
        self.paint_media(ui, ctx, cur.as_ref(), rect, 20.0, 0.25);
        {
            // HUD frame: cyan edge, big 45° cuts, targeting brackets and tiny telemetry in the corners.
            let p = ui.painter();
            p.rect_stroke(rect, Rounding::ZERO, Stroke::new(1.5_f32, CYAN));
            p.rect_stroke(rect.expand(2.0), Rounding::ZERO, Stroke::new(4.0_f32, hud::with_alpha(CYAN, 28)));
            hud::cut_corners(p, rect.expand(2.0), 30.0, BG1, CYAN);
            hud::brackets(p, rect.shrink(10.0), 14.0, hud::with_alpha(YELLOW, 190));
            hud::crosshair(p, rect.center() + vec2(0.0, -rect.height() * 0.12), 10.0, hud::with_alpha(CYAN, 90));
            p.text(rect.right_top() + vec2(-70.0, 16.0), Align2::RIGHT_CENTER, format!("CAM//{}", hud::hex(rect.width() as u32, 4)), FontId::monospace(9.5), hud::with_alpha(CYAN, 160));
            hud::barcode(p, Rect::from_min_size(rect.left_bottom() + vec2(rect.width() * 0.45, -26.0), vec2(70.0, 7.0)), 31, hud::with_alpha(MAGENTA, 160));
        }

        let playing = self.engine.status.playing.load(Ordering::Relaxed);
        let covered = self.engine.status.covered.load(Ordering::Relaxed) > 0;
        let (label, dot) = if st.settings.user_paused { ("TẠM DỪNG", AMBER) }
            else if playing { ("ĐANG PHÁT", GREEN) }
            else if covered { ("ĐANG NGHỈ · DESKTOP BỊ CHE", ICE) }
            else { ("TỰ DỪNG", AMBER) };
        let status_chip = chip(ui.painter(), rect.min + vec2(18.0, 18.0), label, tx(), Some(dot));
        if let Some(cur) = &cur {
            if let Some(t) = &cur.music_label() {
                let t = elide(ui.painter(), t, semi(11.0), (rect.width() - 360.0).max(80.0));
                chip(ui.painter(), pos2(status_chip.right() + 8.0, status_chip.top()), &format!("♪ {t}"), LAVENDER, None);
            }
            // Music button, top-right: opens the picker (choose this wallpaper's song).
            let mr = Rect::from_center_size(pos2(rect.right() - 36.0, rect.top() + 36.0), vec2(36.0, 36.0));
            let resp = round_button(ui, mr, Icon::Music, "hero_music");
            let tip = match &cur.music_label() { Some(t) => format!("Nhạc nền: {t}\nBấm để chọn nhạc"), None => "Chọn nhạc cho hình nền này".to_string() };
            let resp = resp.on_hover_text(tip);
            let pid = ui.make_persistent_id("hero_music_pop");
            if resp.clicked() { ui.memory_mut(|m| m.toggle_popup(pid)); }
            let mut act = MusicAct::None;
            egui::popup_below_widget(ui, pid, &resp, egui::PopupCloseBehavior::CloseOnClickOutside, |ui| {
                act = music_menu(ui, &st.library.songs, &cur.songs());
            });
            if !matches!(act, MusicAct::None) { changed |= self.apply_music(st, &cur.id, act); }
        }

        if let Some(cur) = &cur {
            let fit = if st.settings.fit == FitMode::Fill { "Lấp đầy" } else { "Vừa khung" };
            let title = elide(ui.painter(), &cur.title, bold(26.0), rect.width() - 300.0);
            ui.painter().text(pos2(rect.left() + 22.0, rect.bottom() - 44.0), Align2::LEFT_BOTTOM, title, bold(26.0), Color32::WHITE);
            ui.painter().text(pos2(rect.left() + 22.0, rect.bottom() - 20.0), Align2::LEFT_BOTTOM,
                format!("{}  ·  {}  ·  {}", cur.resolution_text(), cur.duration_text(), fit), FontId::proportional(13.0), Color32::from_white_alpha(190));
        }

        // Controls, right-bottom.
        let size = 44.0;
        let y = rect.bottom() - 22.0 - size / 2.0;
        let play_rect = Rect::from_center_size(pos2(rect.right() - 22.0 - size / 2.0, y), vec2(size, size));
        let paused = st.settings.user_paused;
        if round_button(ui, play_rect, if paused { Icon::Play } else { Icon::Pause }, "hero_play").on_hover_text(if paused { "Phát" } else { "Tạm dừng" }).clicked() {
            st.settings.user_paused = !paused; changed = true;
        }
        // (Volume, seek and mute live on the player card to the right; the preview keeps only the\r\n        // pause button and the music-match picker.)\r\n        // The Spotify-style player card on the right.
        changed |= self.music_panel(ui, ctx, st, panel, cur.as_ref());
        changed
    }
}
