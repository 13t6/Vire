//! The *Widgets* tab: a gallery of the available desktop widgets (with live previews), the list of
//! widgets on the desktop (show / hide / duplicate / remove), per-widget customization, desktop
//! profiles and saved layouts. Every control edits the shared [`widgets::Hub`]; the desktop host
//! picks the change up immediately.

use super::*;
use super::hud;
use crate::raster::Canvas;
use crate::widgets::{self, Category, Ctx, KindInfo, Profile, Widget};
use serde_json::Value;
use std::time::{Duration, Instant};

/// Live preview of one widget kind in the gallery (a real instance of the widget, drawn small).
pub(super) struct Preview { widget: Box<dyn Widget>, tex: Option<TextureHandle>, last: Instant, cfg: Value }

enum Act { Add(&'static str), Remove(u32), Duplicate(u32), Select(u32), Visible(u32, bool) }

const CATS: [&str; 6] = ["Tất cả", "Đồng hồ", "Nhạc", "Hệ thống", "Năng suất", "Tuỳ biến"];

impl App {
    pub(super) fn widgets_tab(&mut self, ui: &mut Ui, ctx: &egui::Context, st: &mut AppState) {
        let hub = self.hub.clone();
        page_title(ui, "Widgets", "Cá nhân hoá màn hình desktop của bạn.");
        ui.add_space(12.0);

        // The visualizer preview listens to the real audio while this page is open (and capture is on).
        let capture = hub.lock().capture;
        if capture && self.wd_lease.is_none() { self.wd_lease = Some(crate::audio::acquire()); }
        if !capture { self.wd_lease = None; }
        let live = self.wd_lease.is_some() && crate::audio::has_signal();
        let frame = self.wd_lease.as_ref().map(|_| crate::audio::snapshot().0);
        ctx.request_repaint_after(Duration::from_millis(if live { 45 } else { 900 }));

        let cur_wall = st.selected().cloned();
        let mut acts: Vec<Act> = Vec::new();
        egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
            // ---- toolbar: search + categories
            ui.horizontal(|ui| {
                ui.add(egui::TextEdit::singleline(&mut self.wd_q).hint_text(if hud::soft() { "🔍  Tìm widget…" } else { "> TÌM WIDGET…" }).desired_width(240.0).margin(vec2(10.0, 7.0)));
                ui.add_space(8.0);
                if let Some(i) = segmented(ui, &CATS, self.wd_cat) { self.wd_cat = i; }
            });
            ui.add_space(14.0);

            self.profile_bar(ui, &hub);
            ui.add_space(6.0);

            // ---- gallery
            let q = self.wd_q.trim().to_lowercase();
            let cat = (self.wd_cat > 0).then(|| Category::ALL[self.wd_cat - 1]);
            let shown: Vec<&'static KindInfo> = widgets::kinds().iter()
                .filter(|k| cat.map_or(true, |c| k.category == c))
                .filter(|k| q.is_empty() || k.name.to_lowercase().contains(&q) || k.desc.to_lowercase().contains(&q)).collect();
            section_label(ui, "Thư viện widget");
            ui.add_space(8.0);
            if shown.is_empty() {
                ui.label(RichText::new(if q.is_empty() { "Chưa có widget nào trong mục này — sắp có thêm." } else { "Không có widget nào khớp." }).size(13.5).color(muted()));
            }
            let gap = 18.0;
            let avail = ui.available_width() - 4.0;
            let cols = ((avail + gap) / (310.0 + gap)).floor().max(1.0) as usize;
            let card_w = (avail - gap * (cols as f32 - 1.0)) / cols as f32;
            let counts: Vec<(String, usize)> = { let s = hub.lock(); widgets::kinds().iter().map(|k| (k.id.to_string(), s.profile().instances.iter().filter(|i| i.kind == k.id).count())).collect() };
            let first_of = |kind: &str| hub.lock().profile().instances.iter().find(|i| i.kind == kind).map(|i| i.id);
            for row in shown.chunks(cols) {
                ui.horizontal_top(|ui| {
                    ui.spacing_mut().item_spacing.x = gap;
                    for k in row {
                        let n = counts.iter().find(|(id, _)| id == k.id).map_or(0, |c| c.1);
                        self.widget_card(ui, ctx, k, card_w, n, frame.as_ref(), live, &mut acts, first_of(k.id), cur_wall.as_ref());
                    }
                });
                ui.add_space(gap - 6.0);
            }

            // ---- on the desktop
            let instances = hub.lock().profile().instances.clone();
            ui.add_space(8.0);
            section_label(ui, &format!("Trên desktop  ·  {}", instances.len()));
            ui.add_space(8.0);
            if instances.is_empty() {
                ui.label(RichText::new("Chưa có widget nào trên desktop. Bấm “Thêm vào desktop” ở một thẻ phía trên.").size(13.5).color(muted()));
            } else {
                self.group(ui, |ui| {
                    for inst in &instances {
                        let k = widgets::kind(&inst.kind);
                        let sub = format!("{}{}{}", k.map_or("?", |k| k.name), if inst.locked { " · đã khoá vị trí" } else { "" }, if inst.visible { "" } else { " · đang ẩn" });
                        let id = inst.id;
                        self.row(ui, &inst.name, &sub, |ui| {
                            if pill_button(ui, Some(Icon::Trash), "Xoá", false).on_hover_text("Xoá widget khỏi desktop").clicked() { acts.push(Act::Remove(id)); }
                            if pill_button(ui, None, "Nhân đôi", false).clicked() { acts.push(Act::Duplicate(id)); }
                            if pill_button(ui, Some(Icon::Gear), "Tuỳ chỉnh", self.wd_sel == Some(id)).clicked() { acts.push(Act::Select(id)); }
                            let mut vis = inst.visible;
                            if toggle(ui, &mut vis).on_hover_text("Hiện / ẩn trên desktop").changed() { acts.push(Act::Visible(id, vis)); }
                            false
                        });
                    }
                });
            }

            // ---- customization of the selected widget
            if let Some(sel) = self.wd_sel.filter(|id| instances.iter().any(|i| i.id == *id)) {
                ui.add_space(14.0);
                let name = instances.iter().find(|i| i.id == sel).map(|i| i.name.clone()).unwrap_or_default();
                section_label(ui, &format!("Tuỳ chỉnh  ·  {name}"));
                ui.add_space(8.0);
                self.customize_panel(ui, &hub, sel);
            }
            ui.add_space(20.0);
        });

        // Apply what was clicked (after the frame, so the layout above never changes mid-draw).
        for a in acts {
            match a {
                Act::Add(kind) => { let mut sel = None; hub.with_mut(|s| { sel = s.add(kind); true }); if sel.is_some() { self.wd_sel = sel; } }
                Act::Remove(id) => { hub.with_mut(|s| { s.remove(id); true }); if self.wd_sel == Some(id) { self.wd_sel = None; } }
                Act::Duplicate(id) => { let mut sel = None; hub.with_mut(|s| { sel = s.duplicate(id); true }); if sel.is_some() { self.wd_sel = sel; } }
                Act::Select(id) => self.wd_sel = if self.wd_sel == Some(id) { None } else { Some(id) },
                Act::Visible(id, v) => { hub.with_mut(|s| { if let Some(i) = s.instance_mut(id) { i.visible = v; true } else { false } }); }
            }
        }
        let _ = st;
    }

    /// Profiles (separate widget layouts for different situations), saved layout, audio-capture switch.
    fn profile_bar(&mut self, ui: &mut Ui, hub: &Arc<widgets::Hub>) {
        let (names, active, has_saved, capture): (Vec<String>, usize, bool, bool) = {
            let s = hub.lock();
            (s.profiles.iter().map(|p| p.name.clone()).collect(), s.active, s.profile().saved.is_some(), s.capture)
        };
        self.group(ui, |ui| {
            self.row(ui, "Hồ sơ desktop", "Mỗi hồ sơ có bộ widget và bố cục riêng (ví dụ: Làm việc, Giải trí)", |ui| {
                let labels: Vec<&str> = names.iter().map(String::as_str).collect();
                if let Some(i) = kit::combo(ui, "wd_profile", active, &labels) { hub.with_mut(|s| { s.active = i; true }); }
                false
            });
            self.row(ui, "Quản lý hồ sơ", "", |ui| {
                if pill_button(ui, Some(Icon::Trash), "Xoá", false).on_hover_text("Xoá hồ sơ đang chọn").clicked() && names.len() > 1 {
                    hub.with_mut(|s| { let a = s.active; s.profiles.remove(a); s.active = a.saturating_sub(1).min(s.profiles.len() - 1); true });
                }
                if pill_button(ui, None, "Nhân đôi", false).clicked() {
                    hub.with_mut(|s| {
                        let mut p = s.profile().clone();
                        for i in p.instances.iter_mut() { i.id = s.next_id; s.next_id += 1; }
                        p.name = format!("{} (bản sao)", p.name);
                        p.saved = None;
                        s.profiles.push(p);
                        s.active = s.profiles.len() - 1;
                        true
                    });
                }
                if pill_button(ui, Some(Icon::Plus), "Hồ sơ mới", true).clicked() {
                    hub.with_mut(|s| { let n = s.profiles.len() + 1; s.profiles.push(Profile { name: format!("Hồ sơ {n}"), instances: Vec::new(), saved: None }); s.active = s.profiles.len() - 1; true });
                }
                let mut name = names.get(active).cloned().unwrap_or_default();
                if ui.add(egui::TextEdit::singleline(&mut name).desired_width(150.0).margin(vec2(8.0, 6.0))).changed() && !name.trim().is_empty() {
                    hub.with_mut(|s| { s.profile_mut().name = name.clone(); true });
                }
                false
            });
            self.row(ui, "Bố cục đã lưu", if has_saved { "Có một bản lưu — khôi phục để quay lại bố cục đó" } else { "Chưa lưu bố cục nào cho hồ sơ này" }, |ui| {
                if has_saved && pill_button(ui, None, "Khôi phục", false).on_hover_text("Quay lại bố cục đã lưu").clicked() {
                    hub.with_mut(|s| { let p = s.profile_mut(); if let Some(saved) = p.saved.clone() { p.instances = saved; true } else { false } });
                }
                if pill_button(ui, None, "Lưu bố cục", false).on_hover_text("Ghi nhớ vị trí và cài đặt hiện tại").clicked() {
                    hub.with_mut(|s| { let p = s.profile_mut(); p.saved = Some(p.instances.clone()); true });
                }
                false
            });
            let status = match crate::audio::state() {
                crate::audio::State::Stopped => "Chưa nghe (không có visualizer nào đang hiện)",
                crate::audio::State::Starting => "Đang khởi động…",
                crate::audio::State::Capturing => if crate::audio::has_signal() { "Đang nghe: có âm thanh" } else { "Đang nghe: im lặng" },
                crate::audio::State::NoDevice => "Không có thiết bị phát âm thanh",
            };
            self.row(ui, "Nghe âm thanh hệ thống", &format!("{status}. Chỉ phân tích tần số, không ghi âm / lưu / gửi đi; không dùng micro."), |ui| {
                let mut on = capture;
                if toggle(ui, &mut on).on_hover_text("Cho visualizer phản ứng với mọi âm thanh Windows đang phát (WASAPI loopback)").changed() { hub.with_mut(|s| { s.capture = on; true }); }
                false
            });
        });
    }

    /// One gallery card: live preview over the current wallpaper, name, category, description, buttons.
    #[allow(clippy::too_many_arguments)]
    fn widget_card(&mut self, ui: &mut Ui, ctx: &egui::Context, k: &'static KindInfo, w: f32, installed: usize,
                   frame: Option<&crate::dsp::Frame>, live: bool, acts: &mut Vec<Act>, first: Option<u32>, cur: Option<&crate::model::WallpaperItem>) {
        let (rect, _) = ui.allocate_exact_size(vec2(w, 268.0), Sense::hover());
        let soft = hud::soft();
        hud::neon_panel(ui.painter(), rect, 18.0, if soft { hud::C_GLASS } else { self.pal.card }, hud::tone(CYAN), 0.5);
        // Preview stage: the real wallpaper, with the live widget over it.
        let stage = Rect::from_min_size(rect.min + vec2(12.0, 12.0), vec2(w - 24.0, 132.0));
        self.paint_media(ui, ctx, cur, stage, 14.0, 0.99);
        let rad = if soft { Rounding::same(14.0) } else { Rounding::ZERO };
        ui.painter().rect_filled(stage, rad, Color32::from_black_alpha(70));
        if let Some((tex, (tw, th))) = self.preview_tex(ctx, k, frame, live) {
            let fit = (stage.width() / tw as f32).min(stage.height() / th as f32) * 0.94;
            let r = Rect::from_center_size(stage.center(), vec2(tw as f32 * fit, th as f32 * fit));
            egui::Image::new(&tex).fit_to_exact_size(r.size()).paint_at(ui, r);
        }
        ui.painter().rect_stroke(stage, rad, Stroke::new(1.0_f32, Color32::from_white_alpha(28)));
        if k.id == "visualizer" && !live {
            let tag = if self.hub.lock().capture { "Chưa có âm thanh" } else { "Đã tắt nghe âm thanh" };
            let p = ui.painter();
            let g = p.layout_no_wrap(tag.to_string(), FontId::proportional(11.5), Color32::from_white_alpha(210));
            let r = Rect::from_min_size(pos2(stage.right() - g.size().x - 20.0, stage.top() + 8.0), vec2(g.size().x + 12.0, 20.0));
            p.rect_filled(r, if soft { Rounding::same(10.0) } else { Rounding::ZERO }, Color32::from_black_alpha(150));
            p.galley(r.min + vec2(6.0, 3.0), g, Color32::from_white_alpha(210));
        }
        // Texts.
        let p = ui.painter().clone();
        p.text(pos2(rect.left() + 14.0, stage.bottom() + 22.0), Align2::LEFT_CENTER, k.name, bold(16.0), tx());
        let cat = chip(&p, pos2(rect.right() - 14.0 - 90.0, stage.bottom() + 11.0), k.category.label(), subtle(), None);
        let _ = cat;
        let g = p.layout(k.desc.to_string(), FontId::proportional(12.0), muted(), w - 28.0);
        p.galley(pos2(rect.left() + 14.0, stage.bottom() + 38.0), g, muted());
        // Buttons.
        let btn = Rect::from_min_size(pos2(rect.left() + 14.0, rect.bottom() - 46.0), vec2(w - 28.0, 34.0));
        let mut c = ui.new_child(egui::UiBuilder::new().max_rect(btn).layout(Layout::left_to_right(Align::Center)));
        if installed == 0 {
            if pill_button(&mut c, Some(Icon::Plus), "Thêm vào desktop", true).clicked() { acts.push(Act::Add(k.id)); }
        } else {
            if pill_button(&mut c, Some(Icon::Gear), "Tuỳ chỉnh", true).clicked() { if let Some(id) = first { acts.push(Act::Select(id)); } }
            if pill_button(&mut c, Some(Icon::Plus), "Thêm nữa", false).on_hover_text("Thêm một bản nữa (nhiều bản cùng loại, mỗi bản một cài đặt)").clicked() { acts.push(Act::Add(k.id)); }
            c.label(RichText::new(format!("{installed} đang dùng")).size(12.0).color(muted()));
        }
    }

    /// Renders the kind's default look into a small texture (refreshed once a second; ~20 fps for the
    /// visualizer while sound plays).
    fn preview_tex(&mut self, ctx: &egui::Context, k: &'static KindInfo, frame: Option<&crate::dsp::Frame>, live: bool) -> Option<(TextureHandle, (i32, i32))> {
        let e = self.wd_prev.entry(k.id).or_insert_with(|| Preview { widget: (k.create)(), tex: None, last: Instant::now() - Duration::from_secs(5), cfg: (k.defaults)() });
        let every = if k.id == "visualizer" { if live { 50 } else { 900 } } else { 900 };
        let (w, h) = k.default_size;
        if e.tex.is_none() || e.last.elapsed() > Duration::from_millis(every) {
            let dt = e.last.elapsed().as_secs_f32().min(0.2);
            e.last = Instant::now();
            let mut c = Canvas::new(w, h);
            let cx = Ctx { now: widgets::now_local(), dt, scale: 1.0, audio: frame, signal: live, builtin_playing: false, preview: true };
            if e.widget.render(&cx, &e.cfg, &mut c) || e.tex.is_none() {
                let mut rgba = Vec::with_capacity(c.px.len() * 4);
                for p in &c.px { rgba.extend_from_slice(&[(p >> 16) as u8, (p >> 8) as u8, *p as u8, (p >> 24) as u8]); }
                let img = egui::ColorImage::from_rgba_premultiplied([w as usize, h as usize], &rgba);
                match &mut e.tex { Some(t) => t.set(img, egui::TextureOptions::LINEAR), None => e.tex = Some(ctx.load_texture(format!("wd_prev_{}", k.id), img, egui::TextureOptions::LINEAR)) }
            }
        }
        e.tex.clone().map(|t| (t, (w, h)))
    }

    /// Common settings (name, monitor, position, size, lock) + the kind's own settings.
    fn customize_panel(&mut self, ui: &mut Ui, hub: &Arc<widgets::Hub>, id: u32) {
        let mons = crate::desktop::monitors();
        let labels: Vec<String> = mons.iter().map(|m| m.label()).collect();
        let scale = (unsafe { windows::Win32::UI::HiDpi::GetDpiForSystem() } as f32 / 96.0).max(1.0);
        self.group(ui, |ui| {
            hub.with_mut(|s| {
                let Some(inst) = s.instance_mut(id) else { return false };
                let Some(k) = widgets::kind(&inst.kind) else { return false };
                let mut ch = false;
                ch |= kit::row(ui, "Tên", |ui| ui.add(egui::TextEdit::singleline(&mut inst.name).desired_width(230.0).margin(vec2(8.0, 6.0))).changed());
                // Monitor + position.
                let mi = mons.iter().position(|m| m.device == inst.monitor).or_else(|| mons.iter().position(|m| m.primary)).unwrap_or(0);
                let lab: Vec<&str> = labels.iter().map(String::as_str).collect();
                if !lab.is_empty() {
                    ch |= kit::row(ui, "Màn hình", |ui| { if let Some(i) = kit::combo(ui, "wd_mon", mi, &lab) { inst.monitor = mons[i].device.clone(); true } else { false } });
                }
                let (mw, mh) = mons.get(mi).map_or((1920.0, 1080.0), |m| (m.width() as f32 / scale, m.height() as f32 / scale));
                ch |= kit::row(ui, "Vị trí nhanh", |ui| {
                    let mut moved = false;
                    ui.horizontal_wrapped(|ui| {
                        let (w, h) = (inst.w as f32, inst.h as f32);
                        for (name, fx, fy) in [("Trái trên", 0.0, 0.0), ("Giữa trên", 0.5, 0.0), ("Phải trên", 1.0, 0.0), ("Giữa", 0.5, 0.5), ("Trái dưới", 0.0, 1.0), ("Giữa dưới", 0.5, 1.0), ("Phải dưới", 1.0, 1.0)] {
                            if ui.small_button(name).clicked() {
                                let m = 28.0;
                                inst.x = (m + (mw - w - 2.0 * m) * fx).round() as i32;
                                inst.y = (m + (mh - h - 2.0 * m) * fy).round() as i32;
                                moved = true;
                            }
                        }
                    });
                    moved
                });
                let mut x = inst.x as f32; let mut y = inst.y as f32;
                if kit::slider_row(ui, "Vị trí ngang", &mut x, 0.0..=(mw - 40.0).max(1.0), |v| format!("{v:.0} px")) { inst.x = x.round() as i32; ch = true; }
                if kit::slider_row(ui, "Vị trí dọc", &mut y, 0.0..=(mh - 40.0).max(1.0), |v| format!("{v:.0} px")) { inst.y = y.round() as i32; ch = true; }
                let (mut w, mut h) = (inst.w as f32, inst.h as f32);
                if kit::slider_row(ui, "Chiều rộng", &mut w, k.min_size.0 as f32..=1600.0, |v| format!("{v:.0} px")) { inst.w = w.round() as i32; ch = true; }
                if kit::slider_row(ui, "Chiều cao", &mut h, k.min_size.1 as f32..=900.0, |v| format!("{v:.0} px")) { inst.h = h.round() as i32; ch = true; }
                ch |= kit::row(ui, "Khoá vị trí", |ui| kit::toggle(ui, &mut inst.locked).on_hover_text("Khoá: widget không bị kéo nhầm và chuột xuyên qua widget").changed());
                ui.add_space(4.0);
                ui.label(RichText::new("Mẹo: kéo widget trên desktop để di chuyển, Ctrl + lăn chuột để đổi cỡ, chuột phải để mở menu.").size(12.0).color(muted()));
                ui.add_space(6.0);
                ch |= (k.customize)(ui, &mut inst.settings);
                ch
            });
        });
    }
}
