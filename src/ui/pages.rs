//! Displays, Dock & Taskbar and Settings pages.

use super::*;

impl App {
    // ---------- Displays ----------
    pub(super) fn displays(&mut self, ui: &mut Ui, ctx: &egui::Context, st: &mut AppState) -> bool {
        let mut changed = false;
        let mons = self.engine.status.monitors.lock().unwrap().clone();
        page_title(ui, "Màn hình", "Bấm vào một màn hình, rồi bấm vào hình nền muốn đặt cho nó.");
        ui.add_space(16.0);
        let soft = hud::soft();
        let rad = if soft { 12.0 } else { 0.0 };
        // Which monitor the picker below is for (none / unknown = the first one).
        if self.disp_sel.as_ref().map_or(true, |d| !mons.iter().any(|m| &m.device == d)) {
            self.disp_sel = mons.iter().find(|m| m.primary).or(mons.first()).map(|m| m.device.clone());
        }
        egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
            self.group(ui, |ui| {
                changed |= self.row(ui, "Đồng bộ tất cả màn hình", "Dùng cùng một hình nền cho mọi màn hình đang kết nối", |ui| toggle(ui, &mut st.settings.sync_all).changed());
            });
            ui.add_space(16.0);
            let sync = st.settings.sync_all;
            let wall_of = |st: &AppState, dev: &str| -> Option<String> {
                if st.settings.sync_all { st.settings.selected_id.clone() } else { st.settings.screen_map.get(dev).cloned().or(st.settings.selected_id.clone()) }
            };
            if !mons.is_empty() {
                // Monitor map: real layout, each screen showing its wallpaper. Click = pick it.
                let (minx, miny) = (mons.iter().map(|m| m.rect.left).min().unwrap(), mons.iter().map(|m| m.rect.top).min().unwrap());
                let (maxx, maxy) = (mons.iter().map(|m| m.rect.right).max().unwrap(), mons.iter().map(|m| m.rect.bottom).max().unwrap());
                let (vw, vh) = ((maxx - minx) as f32, (maxy - miny) as f32);
                let area_w = ui.available_width() - 4.0;
                let scale = (area_w / vw).min(230.0 / vh);
                let (area, _) = ui.allocate_exact_size(vec2(area_w, vh * scale + 8.0), Sense::hover());
                let origin = pos2(area.center().x - vw * scale / 2.0, area.top() + 4.0);
                for m in &mons {
                    let r = Rect::from_min_size(origin + vec2((m.rect.left - minx) as f32 * scale, (m.rect.top - miny) as f32 * scale),
                        vec2(m.width() as f32 * scale, m.height() as f32 * scale)).shrink(6.0);
                    let resp = ui.interact(r, ui.id().with(("mon", &m.device)), Sense::click());
                    let t = ui.ctx().animate_bool_with_time(resp.id, resp.hovered(), 0.12);
                    let picked = sync || self.disp_sel.as_deref() == Some(m.device.as_str());
                    let item = wall_of(st, &m.device).and_then(|id| st.item(&id).cloned());
                    match item.as_ref().and_then(|i| self.texture_for(ctx, i)) {
                        Some(tex) => { egui::Image::new(&tex).uv(cinematic::cover_uv(tex.size(), r)).rounding(rad).paint_at(ui, r); }
                        None => { ui.painter().rect_filled(r, rad, Color32::from_rgb(30, 30, 44)); }
                    }
                    egui::Image::new(&self.shade).rounding(rad).paint_at(ui, Rect::from_min_max(pos2(r.left(), r.top() + r.height() * 0.45), r.max));
                    let accent = if soft { hud::accent() } else { CYAN };
                    let edge = if picked { accent } else { Color32::from_white_alpha((50.0 + 80.0 * t) as u8) };
                    let p = ui.painter();
                    if picked { p.rect_stroke(r.expand(3.0), rad + 3.0, Stroke::new(3.0_f32, hud::with_alpha(accent, 70))); }
                    p.rect_stroke(r, rad, Stroke::new(if picked { 2.0_f32 } else { 1.0 }, edge));
                    p.text(pos2(r.left() + 12.0, r.bottom() - 28.0), Align2::LEFT_BOTTOM, m.label(), semi(13.5), Color32::WHITE);
                    let sub = format!("{}×{}{}", m.width(), m.height(), if m.primary { " · chính" } else { "" });
                    p.text(pos2(r.left() + 12.0, r.bottom() - 10.0), Align2::LEFT_BOTTOM, sub, FontId::proportional(11.5), Color32::from_white_alpha(170));
                    if let Some(it) = &item {
                        let tt = elide(p, &it.title, FontId::proportional(12.0), r.width() * 0.5);
                        p.text(pos2(r.right() - 12.0, r.bottom() - 10.0), Align2::RIGHT_BOTTOM, tt, FontId::proportional(12.0), Color32::from_white_alpha(200));
                    }
                    if !sync && resp.on_hover_cursor(egui::CursorIcon::PointingHand).on_hover_text("Chọn hình nền cho màn hình này").clicked() {
                        self.disp_sel = Some(m.device.clone());
                    }
                }
            }

            // Wallpaper picker: thumbnails, not names. Click one = it plays on the picked screen.
            let target = if sync { None } else { self.disp_sel.clone() };
            let label = match &target {
                None => "Chọn hình nền cho mọi màn hình".to_string(),
                Some(d) => format!("Chọn hình nền cho {}", mons.iter().find(|m| &m.device == d).map(|m| m.label()).unwrap_or_default()),
            };
            section_label(ui, &label);
            ui.add_space(10.0);
            let current = match &target { None => st.settings.selected_id.clone(), Some(d) => wall_of(st, d) };
            let items = st.library.items.clone();
            let gap = 14.0;
            let avail = ui.available_width() - 4.0;
            let cols = ((avail + gap) / (190.0 + gap)).floor().max(1.0) as usize;
            let cw = (avail - gap * (cols as f32 - 1.0)) / cols as f32;
            let ch = cw * 9.0 / 16.0;
            for row in items.chunks(cols) {
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = gap;
                    for it in row {
                        let on = current.as_deref() == Some(it.id.as_str());
                        let (r, resp) = ui.allocate_exact_size(vec2(cw, ch), Sense::click());
                        let t = ui.ctx().animate_bool_with_time(resp.id, resp.hovered(), 0.12);
                        match self.texture_for(ctx, it) {
                            Some(tex) => { egui::Image::new(&tex).uv(cinematic::cover_uv(tex.size(), r)).rounding(rad).paint_at(ui, r); }
                            None => { ui.painter().rect_filled(r, rad, Color32::from_rgb(30, 30, 44)); }
                        }
                        egui::Image::new(&self.shade).rounding(rad).paint_at(ui, Rect::from_min_max(pos2(r.left(), r.top() + r.height() * 0.5), r.max));
                        let accent = if soft { hud::accent() } else { CYAN };
                        let p = ui.painter();
                        if t > 0.0 && !on { p.rect_filled(r, rad, Color32::from_white_alpha((18.0 * t) as u8)); }
                        p.rect_stroke(r, rad, Stroke::new(if on { 2.0_f32 } else { 1.0 }, if on { accent } else { Color32::from_white_alpha((30.0 + 70.0 * t) as u8) }));
                        let tt = elide(p, &it.title, semi(12.5), cw - 20.0);
                        p.text(pos2(r.left() + 10.0, r.bottom() - 9.0), Align2::LEFT_BOTTOM, tt, semi(12.5), Color32::WHITE);
                        if on { chip(p, r.min + vec2(8.0, 8.0), "Đang dùng", tx(), Some(accent)); }
                        if resp.on_hover_cursor(egui::CursorIcon::PointingHand).clicked() && !on {
                            match &target {
                                None => { st.settings.selected_id = Some(it.id.clone()); st.settings.live_wallpaper = true; }
                                Some(d) => { st.settings.screen_map.insert(d.clone(), it.id.clone()); st.settings.live_wallpaper = true; }
                            }
                            changed = true;
                        }
                    }
                });
                ui.add_space(gap - 10.0);
            }
            ui.add_space(16.0);
        });
        changed
    }

    // ---------- Dock & Taskbar ----------
    pub(super) fn dock_preview(&mut self, ui: &mut Ui, ctx: &egui::Context, st: &AppState) {
        let s = st.settings.clone();
        let w = ui.available_width() - 2.0;
        let (area, _) = ui.allocate_exact_size(vec2(w, 170.0), Sense::hover());
        let cur = st.selected().cloned();
        self.paint_media(ui, ctx, cur.as_ref(), area, 18.0, 0.7);
        let p = ui.painter();
        p.rect_stroke(area, if hud::soft() { Rounding::same(12.0) } else { Rounding::ZERO }, Stroke::new(1.0_f32, Color32::from_white_alpha(26)));
        let colors = [Color32::from_rgb(90, 160, 255), LAVENDER, ICE, Color32::from_rgb(255, 170, 90), GREEN, Color32::from_rgb(255, 110, 140)];
        if s.vd_enabled {
            // Vire Dock on the chosen edge, in the chosen material.
            let (fill, rim) = match s.material {
                Material::Glass => (Color32::from_white_alpha(26), Color32::from_white_alpha(120)),
                Material::Acrylic => (Color32::from_rgba_unmultiplied(26, 28, 40, 160), Color32::from_white_alpha(70)),
                Material::Mica => (Color32::from_rgba_unmultiplied(32, 34, 48, 220), Color32::from_white_alpha(40)),
            };
            let n = colors.len() + 1 + usize::from(s.vd_clock) * 2;
            let (slot, thick) = (26.0, 34.0);
            let len = n as f32 * slot + 12.0;
            let vertical = s.vd_edge != Edge::Bottom;
            let body = match s.vd_edge {
                Edge::Bottom => Rect::from_min_size(pos2(area.center().x - len / 2.0, area.bottom() - 8.0 - thick), vec2(len, thick)),
                Edge::Left => Rect::from_min_size(pos2(area.left() + 8.0, area.center().y - len.min(150.0) / 2.0), vec2(thick, len.min(150.0))),
                Edge::Right => Rect::from_min_size(pos2(area.right() - 8.0 - thick, area.center().y - len.min(150.0) / 2.0), vec2(thick, len.min(150.0))),
            };
            if s.vd_style == 1 {
                // Reflection style: icons float with no plate, a faded mirror image under each.
                let n = colors.len() + 1 + usize::from(s.vd_clock) * 2;
                let (slot, isz) = (26.0, 18.0);
                let base = if vertical { 0.0 } else { area.bottom() - 8.0 - 22.0 };
                for i in 0..n {
                    let col = if i == 0 { ICE } else if i <= colors.len() { colors[i - 1] } else { Color32::from_white_alpha(200) };
                    let c = if vertical { pos2(body.center().x, body.top() + 6.0 + (body.height() - 12.0) / n as f32 * (i as f32 + 0.5)) } else { pos2(area.center().x - (n as f32 * slot) / 2.0 + slot * (i as f32 + 0.5), base - isz / 2.0) };
                    if vertical && c.y > body.bottom() - 6.0 { break; }
                    if s.vd_clock && i == n - 1 && !vertical { p.text(c, Align2::CENTER_CENTER, "4:33", FontId::proportional(10.5), tx()); continue; }
                    let ir = Rect::from_center_size(c, vec2(isz, isz));
                    p.rect_filled(ir, if hud::soft() { Rounding::same(4.0) } else { Rounding::ZERO }, col);
                    if !vertical {
                        for k in 0..6 {
                            let a = (90.0 * (1.0 - k as f32 / 6.0).powf(1.6)) as u8;
                            p.rect_filled(Rect::from_min_size(pos2(ir.left(), ir.bottom() + 4.0 + k as f32 * 3.0), vec2(isz, 3.0)), Rounding::ZERO, Color32::from_rgba_unmultiplied(col.r(), col.g(), col.b(), a));
                        }
                    }
                }
                return;
            }
            p.rect_filled(body.translate(vec2(0.0, 2.0)).expand(1.0), Rounding::ZERO, Color32::from_black_alpha(45));
            p.rect_filled(body, Rounding::ZERO, fill);
            if s.material == Material::Glass {
                let sheen = if vertical { Rect::from_min_max(body.min, pos2(body.right(), body.top() + 20.0)) } else { Rect::from_min_max(body.min, pos2(body.right(), body.top() + body.height() * 0.42)) };
                p.rect_filled(sheen, Rounding::ZERO, Color32::from_white_alpha(20));
            }
            p.rect_stroke(body, Rounding::ZERO, Stroke::new(1.0_f32, rim));
            let step = if vertical { (body.height() - 12.0) / n as f32 } else { slot };
            for i in 0..n {
                let c = if vertical { pos2(body.center().x, body.top() + 6.0 + step * (i as f32 + 0.5)) } else { pos2(body.left() + 6.0 + slot * (i as f32 + 0.5), body.center().y) };
                if vertical && c.y > body.bottom() - 6.0 { break; }
                let col = if i == 0 { ICE } else if i <= colors.len() { colors[i - 1] } else { Color32::from_white_alpha(200) };
                let sz = if vertical { (step - 6.0).clamp(8.0, 18.0) } else { 18.0 };
                if s.vd_clock && i == n - 1 && !vertical {
                    p.text(c, Align2::CENTER_CENTER, "4:33", FontId::proportional(10.5), tx());
                } else {
                    p.rect_filled(Rect::from_center_size(c, vec2(sz, sz)), Rounding::ZERO, col);
                }
                if (1..4).contains(&i) {
                    let m = if vertical { Rect::from_center_size(pos2(body.left() + 3.0, c.y), vec2(2.0, if i == 1 { 10.0 } else { 5.0 })) } else { Rect::from_center_size(pos2(c.x, body.bottom() - 3.5), vec2(if i == 1 { 10.0 } else { 5.0 }, 2.0)) };
                    p.rect_filled(m, Rounding::ZERO, Color32::from_white_alpha(if i == 1 { 240 } else { 160 }));
                }
            }
            return;
        }
        let bar_h = 40.0;
        let bar = Rect::from_min_max(pos2(area.left(), area.bottom() - bar_h), area.right_bottom());
        // Glass (system Acrylic) shows the wallpaper through; solid otherwise.
        let fill = if s.sys_glass { Color32::from_rgba_unmultiplied(40, 42, 52, 150) } else { Color32::from_rgb(28, 28, 30) };
        let icons_w = colors.len() as f32 * 26.0;
        let il = bar.center().x - icons_w / 2.0;
        let tr = bar.right() - 8.0;
        let cy = bar.center().y - 1.0;
        p.rect_filled(bar, Rounding::ZERO, fill);
        for (i, c) in colors.iter().enumerate() {
            let x = il + 13.0 + i as f32 * 26.0;
            p.rect_filled(Rect::from_center_size(pos2(x, cy), vec2(18.0, 18.0)), Rounding::ZERO, *c);
            if i != 2 { p.rect_filled(Rect::from_center_size(pos2(x, cy + 13.0), vec2(if i == 0 { 12.0 } else { 6.0 }, 2.5)), Rounding::ZERO, if i == 0 { ICE } else { Color32::from_gray(160) }); }
        }
        p.text(pos2(tr - 10.0, cy), Align2::RIGHT_CENTER, "4:33 PM", FontId::proportional(11.5), tx());
    }

    pub(super) fn taskbar_tab(&mut self, ui: &mut Ui, ctx: &egui::Context, st: &mut AppState) -> bool {
        let mut dock_changed = false;
        page_title(ui, "Dock & Taskbar", "Chọn kiểu thanh tác vụ, chất liệu kính và vị trí.");
        ui.add_space(16.0);
        egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
            self.dock_preview(ui, ctx, st);
            ui.add_space(14.0);
            let s = &mut st.settings;

            // 1. Which bar.
            let mode = usize::from(s.vd_enabled);
            if let Some(m) = segmented(ui, &["Taskbar Windows", "Vire Dock"], mode) {
                s.vd_enabled = m == 1;
                if s.vd_enabled && !s.vd_pins_imported {
                    s.vd_pins = vdock::import_taskbar_pins();
                    s.vd_pins_imported = true;
                }
                dock_changed = true;
            }
            ui.add_space(10.0);

            // 2. Material + system glass.
            let mut glass_changed = false;
            self.group(ui, |ui| {
                let mi = match s.material { Material::Acrylic => 0, Material::Mica => 1, Material::Glass => 2 };
                if self.row(ui, "Chất liệu", "Cho Vire, Dock, Explorer và Settings", |ui| {
                    segmented(ui, &["Acrylic", "Mica", "Glass"], mi).map(|m| { s.material = [Material::Acrylic, Material::Mica, Material::Glass][m]; }).is_some()
                }) { dock_changed = true; }
                glass_changed = self.row(ui, "Kính cho hệ thống", "Start, thanh thông báo, Settings, File Explorer", |ui| toggle(ui, &mut s.sys_glass).changed());
            });

            // 3. Options of the chosen bar.
            if s.vd_enabled {
                section_label(ui, "Vire Dock");
                ui.add_space(8.0);
                self.group(ui, |ui| {
                    let ei = match s.vd_edge { Edge::Bottom => 0, Edge::Left => 1, Edge::Right => 2 };
                    if self.row(ui, "Vị trí", "", |ui| {
                        segmented(ui, &["Dưới", "Trái", "Phải"], ei).map(|m| { s.vd_edge = [Edge::Bottom, Edge::Left, Edge::Right][m]; }).is_some()
                    }) { dock_changed = true; }
                    let si = usize::from(s.vd_style == 1);
                    if self.row(ui, "Kiểu Dock", "Kính mờ, hoặc trong suốt có ảnh phản chiếu dưới icon", |ui| {
                        segmented(ui, &["Kính", "Phản chiếu"], si).map(|m| { s.vd_style = m as u8; }).is_some()
                    }) { dock_changed = true; }
                    dock_changed |= self.row(ui, "Tự ẩn", "Hiện khi chạm chuột vào mép màn hình", |ui| toggle(ui, &mut s.vd_autohide).changed());
                    dock_changed |= self.row(ui, "Phóng to khi rê chuột", "", |ui| toggle(ui, &mut s.vd_magnify).changed());
                    dock_changed |= self.row(ui, "Đồng hồ trong Dock", "Kèm nút Wi-Fi / âm lượng / pin", |ui| toggle(ui, &mut s.vd_clock).changed());
                });
                ui.add_space(8.0);
                ui.label(RichText::new("Chuột phải vào icon để ghim / bỏ ghim. Bấm đồng hồ để mở lịch và thông báo. Icon khay của app khác và xem trước cửa sổ khi rê chuột chỉ có trên taskbar Windows.")
                    .size(12.0).color(muted()));
            }
            if glass_changed {
                let mut prev = s.sys_glass_prev;
                sysglass::set_enabled(s.sys_glass, &mut prev);
                s.sys_glass_prev = prev;
                dock_changed = true; // persist
            }
            ui.add_space(16.0);
        });
        dock_changed
    }

    // ---------- Preferences ----------
    pub(super) fn preferences(&mut self, ui: &mut Ui, st: &mut AppState) -> bool {
        let mut changed = false;
        page_title(ui, "Cài đặt", "");
        ui.add_space(6.0);
        egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
            let s = &mut st.settings;
            ui.add_space(8.0);
            let mut login = false;
            self.group(ui, |ui| {
                changed |= self.row(ui, "Hình nền động", "Tắt để giữ hình nền Windows như cũ (chỉ dùng Dock, widget). Chọn một hình ở tab Hình nền sẽ tự bật lại.", |ui| toggle(ui, &mut s.live_wallpaper).changed());
                changed |= self.row(ui, "Kiểu hiển thị", "", |ui| {
                    let mut ch = ui.selectable_value(&mut s.fit, FitMode::Fit, "Vừa khung").changed();
                    ch |= ui.selectable_value(&mut s.fit, FitMode::Fill, "Lấp đầy").changed();
                    ch
                });
                // One switch for both "desktop covered" and "fullscreen app".
                let mut smart = s.pause_on_covered && s.pause_on_fullscreen;
                if self.row(ui, "Tự nghỉ khi không thấy desktop", "Cửa sổ phóng to, game, phim toàn màn hình", |ui| toggle(ui, &mut smart).changed()) {
                    s.pause_on_covered = smart; s.pause_on_fullscreen = smart; changed = true;
                }
                changed |= self.row(ui, "Dừng khi dùng pin", "", |ui| toggle(ui, &mut s.pause_on_battery).changed());
                login = self.row(ui, "Khởi động cùng Windows", "", |ui| toggle(ui, &mut s.launch_at_login).changed());
            });
            ui.add_space(14.0);
            if pill_button(ui, Some(Icon::Folder), "Mở thư mục Vire", false).on_hover_text("Hình nền, nhạc và dữ liệu của bạn").clicked() { open_url(&crate::model::lib_root().to_string_lossy()); }
            if login { autostart::set_enabled(s.launch_at_login); changed = true; }
            // About: one quiet line instead of a page of its own.
            ui.add_space(26.0);
            ui.horizontal(|ui| {
                if let Some(logo) = &self.logo { ui.add(egui::Image::new(logo).fit_to_exact_size(vec2(22.0, 22.0)).rounding(if hud::soft() { 6.0 } else { 0.0 })); }
                ui.label(RichText::new(format!("Vire {}  ·  Windows  ·  Tác giả: NSH", env!("CARGO_PKG_VERSION"))).size(12.5).color(muted()));
            });
            ui.add_space(16.0);
        });
        changed
    }
}
