//! Vire dashboard (egui). Windows 11 Acrylic backdrop (composited by DWM, free for the app),
//! content on a floating rounded sheet, image-first cards, grouped settings, vector icons.

use crate::engine::Engine;
use crate::model::{is_video, AppState, Edge, FitMode, Material, WallpaperItem};
use crate::vdock::{self, VDock};
use crate::{autostart, media, sysglass, tray, APP_TITLE};
use eframe::egui::{self, pos2, vec2, Align, Align2, Color32, FontData, FontDefinitions, FontFamily, FontId, Layout, Margin,
    Pos2, Rect, Response, RichText, Rounding, Sense, Shape, Stroke, TextureHandle, Ui};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::atomic::Ordering;
use std::sync::mpsc::{channel, Receiver, Sender};
use std::sync::{Arc, Mutex};
use tray_icon::menu::CheckMenuItem;
use windows::core::{w, HSTRING};
use windows::Win32::Foundation::{BOOL, HWND};
use windows::Win32::Graphics::Dwm::{DwmSetWindowAttribute, DWMWA_USE_IMMERSIVE_DARK_MODE};
use windows::Win32::UI::Shell::ShellExecuteW;
use windows::Win32::UI::WindowsAndMessaging::{FindWindowW, SW_SHOWNORMAL};

mod actions;
mod cinematic;
mod hud;
mod icons;
pub(crate) mod kit;
mod music_menu;
mod music_page;
mod pages;
mod player_panel;
mod theme;
mod wallpapers;
mod widgets;
mod widgets_page;
use hud::{BG0, BG1, CYAN, MAGENTA, YELLOW};
use icons::*;
use music_menu::*;
use theme::*;
use widgets::*;

#[derive(Clone, Copy, PartialEq, Eq)]
enum Tab { Gallery, Displays, Music, Widgets, Taskbar, Theme, Preferences }

impl Tab {
    const ALL: [Tab; 7] = [Tab::Gallery, Tab::Displays, Tab::Music, Tab::Widgets, Tab::Taskbar, Tab::Theme, Tab::Preferences];
    fn label(self) -> &'static str {
        match self {
            Tab::Gallery => "Hình nền", Tab::Music => "Nhạc", Tab::Widgets => "Widgets", Tab::Displays => "Màn hình", Tab::Taskbar => "Dock & Taskbar",
            Tab::Theme => "Giao diện", Tab::Preferences => "Cài đặt",
        }
    }
}

enum BgResult {
    Imported(Result<media::Imported, String>),
    /// (wallpaper id, result of copying the song into Vire's Music folder)
    Music(String, Result<(PathBuf, String), String>),
    Thumb(String, Option<PathBuf>),
    /// A library video re-encoded to the screens' resolution: (id, new file + size or None on failure, old file).
    Shrunk(String, Option<(PathBuf, u32, u32)>, PathBuf),
    /// Files chosen in the (separate-thread) open dialog; `Some(id)` = a song for that wallpaper.
    Picked(Option<String>, Vec<PathBuf>),
}

pub struct App {
    state: Arc<Mutex<AppState>>,
    engine: Engine,
    vdock: VDock,
    pal: Palette,
    glass: bool,
    material: Material,
    tab: Tab,
    /// (wallpaper id, text being edited) while the rename dialog is open.
    rename: Option<(String, String)>,
    rename_focus: bool,
    /// Short info line under the gallery header (import result).
    notice: Option<String>,
    /// Freshly imported videos bigger than the screens, waiting for the user's yes/no to shrink them.
    shrink_ask: Vec<String>,
    /// How many videos are being shrunk in the background right now.
    shrinking: usize,
    /// Wallpaper that should get music (file dialog opens on the next frame).
    music_req: Option<String>,
    textures: HashMap<String, TextureHandle>,
    logo: Option<TextureHandle>,
    shade: TextureHandle,
    bg_tx: Sender<BgResult>,
    bg_rx: Receiver<BgResult>,
    mp: music_page::MusicPage,
    /// Volume (0..1) while a speaker's volume bar is being dragged.
    vol_drag: Option<f32>,
    /// Player card: collection list open / favourites filter / seek position while dragging.
    pl_list: bool,
    pl_fav: bool,
    /// Search boxes: wallpapers (gallery) and songs (player collection list).
    wall_q: String,
    song_q: String,
    pl_seek: Option<f32>,
    importing: usize,
    /// Picked in the dialog thread, handled on the next frame.
    picked: Vec<(Option<String>, Vec<PathBuf>)>,
    error: Option<String>,
    tray_pause: CheckMenuItem,
    tray_mute: CheckMenuItem,
    /// Look currently applied to fonts / style (Settings::ui_theme).
    theme: u8,
    /// Cinematic backdrop: (wallpaper id, tiny blurred copy of its thumbnail).
    blur: Option<(String, TextureHandle)>,
    /// Displays page: monitor whose wallpaper is being picked.
    disp_sel: Option<String>,
    /// Cinematic: previous backdrop fading out (texture, start time).
    blur_prev: Option<(TextureHandle, f64)>,
    /// Cinematic: accent easing (from, to, start time).
    accent_anim: (Color32, Color32, f64),
    /// Cinematic: page shown + when it appeared (fade-in).
    tab_seen: Tab,
    tab_t0: f64,
    /// Desktop widgets: the shared hub, gallery filter, selected instance, live previews.
    hub: Arc<crate::widgets::Hub>,
    wd_q: String,
    wd_cat: usize,
    wd_sel: Option<u32>,
    wd_prev: HashMap<&'static str, widgets_page::Preview>,
    /// Held while the Widgets page is open (live preview of the visualizer); dropped otherwise.
    wd_lease: Option<crate::audio::Lease>,
}

fn open_url(url: &str) {
    unsafe { ShellExecuteW(None, w!("open"), &HSTRING::from(url), None, None, SW_SHOWNORMAL); }
}
impl App {
    pub fn new(cc: &eframe::CreationContext<'_>, state: Arc<Mutex<AppState>>, engine: Engine, vdock: VDock,
               tray_pause: CheckMenuItem, tray_mute: CheckMenuItem, hub: Arc<crate::widgets::Hub>) -> Self {
        let (material, theme) = { let s = &state.lock().unwrap().settings; (s.material, s.ui_theme) };
        hud::set_soft(theme == 1);
        let glass = apply_backdrop(material);
        let pal = Palette::new(glass, material);
        setup_fonts(&cc.egui_ctx, theme == 1);
        setup_style(&cc.egui_ctx, pal);
        tray::set_ui_ctx(Some(cc.egui_ctx.clone()));
        let (bg_tx, bg_rx) = channel();
        let logo = load_png_texture(&cc.egui_ctx, "logo", include_bytes!("../assets/icon_128.png"));
        let shade = shade_texture(&cc.egui_ctx);
        // `--tab=taskbar|displays|settings|about` opens that tab directly.
        let tab = match std::env::args().find_map(|a| a.strip_prefix("--tab=").map(str::to_ascii_lowercase)).as_deref() {
            Some("music") => Tab::Music,
            Some("displays") => Tab::Displays,
            Some("taskbar") => Tab::Taskbar,
            Some("settings") => Tab::Preferences,
            Some("theme") => Tab::Theme,
            Some("widgets") => Tab::Widgets,
            _ => Tab::Gallery,
        };
        // Files dropped into Documents\Vire since the last look join the library now.
        {
            let mut st = state.lock().unwrap();
            if st.rescan() { st.save(); engine.apply(&st); }
        }
        let dev_import = std::env::args().find_map(|a| a.strip_prefix("--dev-import-ui=").map(PathBuf::from));
        let mut app = App {
            state, engine, vdock, pal, glass, material, tab, textures: HashMap::new(), logo, shade, bg_tx, bg_rx,
            mp: music_page::MusicPage::new(), vol_drag: None, pl_list: false, pl_fav: false, wall_q: String::new(), song_q: String::new(), pl_seek: None, importing: 0, picked: Vec::new(), error: None, tray_pause, tray_mute, rename: None, rename_focus: false, notice: None, music_req: None, shrink_ask: Vec::new(), shrinking: 0,
            theme, blur: None, disp_sel: None, blur_prev: None,
            accent_anim: (hud::C_VIOLET, hud::C_VIOLET, -10.0), tab_seen: tab, tab_t0: -10.0,
            hub, wd_q: String::new(), wd_cat: 0, wd_sel: None, wd_prev: HashMap::new(), wd_lease: None,
        };
        app.generate_missing_thumbs();
        if let Some(p) = dev_import { app.start_import(vec![p], &cc.egui_ctx); } // hidden test flag
        app
    }
}

impl App {
    /// The page of the current tab (same pages in both looks; the Cinematic look has its own
    /// wallpaper showcase).
    fn tab_body(&mut self, ui: &mut Ui, ctx: &egui::Context, st: &mut AppState) -> (bool, bool) {
        let (mut c, mut d) = (false, false);
        match self.tab {
            Tab::Music => c |= self.music_tab(ui, ctx, st),
            Tab::Widgets => self.widgets_tab(ui, ctx, st),
            Tab::Gallery if !st.settings.live_wallpaper => {
                // No live wallpaper: say so, and offer the one-click way back.
                egui::Frame::none().fill(Color32::from_black_alpha(60)).rounding(if hud::soft() { Rounding::same(12.0) } else { Rounding::ZERO }).inner_margin(Margin::symmetric(14.0, 10.0)).show(ui, |ui| {
                    ui.horizontal(|ui| {
                        ui.label(RichText::new("Đang dùng hình nền Windows (hình nền động đang tắt). Chọn một hình bên dưới để bật lại.").size(13.0).color(subtle()));
                        if pill_button(ui, None, "Bật hình nền động", true).clicked() { st.settings.live_wallpaper = true; c = true; }
                    });
                });
                ui.add_space(8.0);
                c |= if hud::soft() { self.cine_gallery(ui, ctx, st) } else { self.gallery(ui, ctx, st) };
            }
            Tab::Gallery => c |= if hud::soft() { self.cine_gallery(ui, ctx, st) } else { self.gallery(ui, ctx, st) },
            Tab::Displays => c |= self.displays(ui, ctx, st),
            Tab::Taskbar => d |= self.taskbar_tab(ui, ctx, st),
            Tab::Theme => self.theme_tab(ui, st),
            Tab::Preferences => c |= self.preferences(ui, st),
        }
        (c, d)
    }

    /// A wallpaper picked on the Wallpaper page: it becomes the selection AND plays on the main
    /// screen even when each screen has its own wallpaper (otherwise the main screen kept its old
    /// per-screen pick, and with it the old matched song). Other screens keep theirs.
    fn pick_wallpaper(&self, st: &mut AppState, id: &str) {
        st.settings.selected_id = Some(id.to_string());
        st.settings.live_wallpaper = true; // choosing a wallpaper means you want it
        if !st.settings.sync_all {
            let mons = self.engine.status.monitors.lock().unwrap().clone();
            if let Some(m) = mons.iter().find(|m| m.primary).or(mons.first()) {
                st.settings.screen_map.insert(m.device.clone(), id.to_string());
            }
        }
    }
}

impl eframe::App for App {
    fn clear_color(&self, _: &egui::Visuals) -> [f32; 4] {
        if self.glass { [0.0; 4] } else { egui::Rgba::from(self.pal.sidebar).to_array() }
    }

    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        // Closing the window destroys only the dashboard; tray + wallpaper keep running.
        let mut changed = self.poll_background();
        let mut dock_changed = false;
        // A desktop widget's right-click "Tuỳ chỉnh" opens the dashboard on that widget.
        if let Some(id) = self.hub.take_edit_request() { self.tab = Tab::Widgets; self.wd_sel = Some(id); }
        self.hub.flush(false);
        if self.tab != Tab::Widgets { self.wd_lease = None; }

        let dropped: Vec<PathBuf> = ctx.input(|i| i.raw.dropped_files.iter().filter_map(|f| f.path.clone()).collect());
        if !dropped.is_empty() {
            // A song dropped on the window becomes the music of the current wallpaper.
            let sel = self.state.lock().unwrap().settings.selected_id.clone();
            let (audio, rest): (Vec<PathBuf>, Vec<PathBuf>) = dropped.into_iter().partition(|p| crate::model::is_audio(p));
            if self.tab == Tab::Music { self.import_songs(ctx, audio); }
            else if let Some(id) = sel { for song in audio { self.attach_music(ctx, id.clone(), song); } }
            self.start_import(rest, ctx);
        }
        for (song_for, files) in std::mem::take(&mut self.picked) {
            match song_for {
                Some(id) if id.is_empty() => self.import_songs(ctx, files),
                Some(id) => { for f in files { self.attach_music(ctx, id.clone(), f); } }
                None => self.start_import(files, ctx),
            }
        }
        let hovering_files = ctx.input(|i| !i.raw.hovered_files.is_empty());

        let state = self.state.clone();
        crate::watchdog::phase(crate::watchdog::UI, "dashboard:waiting-for-state-lock");
        let mut st = state.lock().unwrap();
        crate::watchdog::phase(crate::watchdog::UI, "dashboard:frame");
        let pal = self.pal;
        // Song tags (title / artist / length) are read a few per frame until the library is known.
        if st.pending_meta() > 0 {
            if st.backfill_meta(4) && st.pending_meta() == 0 { st.save(); }
            ctx.request_repaint();
        }

        // Look picked on the "Giao diện" page: re-skin fonts + widgets live (next frame).
        if st.settings.ui_theme != self.theme {
            self.theme = st.settings.ui_theme;
            hud::set_soft(self.theme == 1);
            apply_backdrop(self.material);
            setup_fonts(ctx, self.theme == 1);
            setup_style(ctx, self.pal);
            ctx.request_repaint();
        }
        if hud::soft() {
            let (c, d) = self.cine_frame(ctx, &mut st, hovering_files);
            changed |= c; dock_changed |= d;
        } else {
        // HUD backdrop: near-black with a faint grid, and scanlines over everything.
        hud::backdrop(&ctx.layer_painter(egui::LayerId::background()), ctx.screen_rect());
        hud::scanlines(ctx, ctx.screen_rect());
        ctx.request_repaint_after(std::time::Duration::from_millis(600)); // blinking cursors

        egui::SidePanel::left("sidebar").exact_width(236.0).resizable(false).show_separator_line(false)
            .frame(egui::Frame::none().fill(Color32::TRANSPARENT).inner_margin(Margin { left: 14.0, right: 12.0, top: 6.0, bottom: 6.0 }))
            .show(ctx, |ui| self.sidebar(ui, &st));

        egui::CentralPanel::default()
            .frame(egui::Frame::none().fill(Color32::TRANSPARENT).inner_margin(Margin { left: 0.0, right: 12.0, top: 12.0, bottom: 12.0 }))
            .show(ctx, |ui| {
                let sheet = ui.max_rect();
                let inner = egui::Frame::none().fill(pal.sheet).stroke(Stroke::new(1.0_f32, pal.hair)).rounding(Rounding::ZERO)
                    .inner_margin(Margin { left: 30.0, right: 30.0, top: 26.0, bottom: 10.0 })
                    .show(ui, |ui| {
                        ui.set_min_size(ui.available_size());
                        let (c, d) = self.tab_body(ui, ctx, &mut st);
                        changed |= c; dock_changed |= d;
                    });
                // Frame decoration: chamfered corners, brackets and tiny tags in the empty margins.
                let r = inner.response.rect;
                let p = ui.painter();
                hud::cut_corners(p, r, 24.0, BG0, CYAN);
                hud::brackets(p, r.shrink(7.0), 12.0, hud::with_alpha(CYAN, 90));
                p.text(pos2(r.left() + 40.0, r.bottom() - 7.0), Align2::LEFT_CENTER, format!("VIRE//{}  {}  {}", env!("CARGO_PKG_VERSION"), hud::hex(sheet.width() as u32, 8), if self.tab == Tab::Music { "AUDIO.BUS" } else { "VIDEO.BUS" }), FontId::monospace(9.0), hud::with_alpha(CYAN, 90));
                hud::barcode(p, Rect::from_min_size(pos2(r.right() - 150.0, r.bottom() - 12.0), vec2(112.0, 7.0)), 99 + self.tab as u32, hud::with_alpha(MAGENTA, 120));
                hud::crosshair(p, pos2(r.right() - 20.0, r.bottom() - 9.0), 6.0, hud::with_alpha(YELLOW, 150));
                if hovering_files {
                    let r = ui.max_rect();
                    ui.painter().rect_filled(r, 0.0, hud::with_alpha(MAGENTA, 40));
                    hud::neon_panel(ui.painter(), r.shrink(14.0), 22.0, Color32::TRANSPARENT, YELLOW, 1.0);
                    hud::glow_text(ui.painter(), r.center(), Align2::CENTER_CENTER, "> THẢ VIDEO ĐỂ THÊM — HOẶC THẢ BÀI NHẠC VÀO HÌNH NỀN HIỆN TẠI", bold(17.0), YELLOW);
                }
            });
        }

        // Rename dialog.
        if let Some((id, mut text)) = self.rename.take() {
            let (mut keep, mut save) = (true, false);
            ctx.layer_painter(egui::LayerId::new(egui::Order::Middle, egui::Id::new("rename_dim")))
                .rect_filled(ctx.screen_rect(), Rounding::ZERO, Color32::from_black_alpha(120));
            egui::Window::new("rename").title_bar(false).collapsible(false).resizable(false)
                .anchor(Align2::CENTER_CENTER, vec2(0.0, 0.0))
                .frame(egui::Frame::none().fill(Color32::from_rgb(8, 11, 16)).stroke(Stroke::new(1.0_f32, CYAN))
                    .rounding(Rounding::ZERO).inner_margin(Margin::same(22.0)).shadow(egui::epaint::Shadow { offset: vec2(0.0, 0.0), blur: 24.0, spread: 2.0, color: hud::with_alpha(CYAN, 60) }))
                .show(ctx, |ui| {
                    ui.label(RichText::new("> ĐỔI TÊN HÌNH NỀN").font(bold(17.0)).color(CYAN));
                    ui.add_space(10.0);
                    let r = ui.add(egui::TextEdit::singleline(&mut text).desired_width(360.0).font(FontId::proportional(15.5)).margin(vec2(10.0, 8.0)));
                    if self.rename_focus { r.request_focus(); self.rename_focus = false; }
                    if r.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) { save = true; }
                    ui.add_space(14.0);
                    ui.horizontal(|ui| {
                        if pill_button(ui, None, "Lưu", true).clicked() { save = true; }
                        if pill_button(ui, None, "Huỷ", false).clicked() { keep = false; }
                    });
                });
            if ctx.input(|i| i.key_pressed(egui::Key::Escape)) { keep = false; }
            if save {
                let t = text.trim();
                if !t.is_empty() {
                    if let Some(it) = st.library.items.iter_mut().find(|i| i.id == id) { it.title = t.to_string(); }
                    st.save();
                }
                keep = false;
            }
            if keep { self.rename = Some((id, text)); }
        }

        // "Shrink this video?" — asked once per imported video that is bigger than the screens.
        if let Some(id) = self.shrink_ask.first().cloned() {
            let found = st.library.items.iter().find(|i| i.id == id).map(|i| (i.title.clone(), i.width, i.height));
            if found.is_none() { self.shrink_ask.remove(0); }
            let (title, w, h) = found.unwrap_or_default();
            if w > 0 {
            let to = media::shrink_target(w, h);
            let screen = crate::desktop::monitors().iter().map(|m| (m.width(), m.height())).max_by_key(|s| s.0 * s.1).unwrap_or((0, 0));
            let mut answer: Option<bool> = None;
            ctx.layer_painter(egui::LayerId::new(egui::Order::Middle, egui::Id::new("shrink_dim")))
                .rect_filled(ctx.screen_rect(), Rounding::ZERO, Color32::from_black_alpha(120));
            egui::Window::new("shrink").title_bar(false).collapsible(false).resizable(false)
                .anchor(Align2::CENTER_CENTER, vec2(0.0, 0.0))
                .frame(egui::Frame::none().fill(Color32::from_rgb(8, 11, 16)).stroke(Stroke::new(1.0_f32, CYAN))
                    .rounding(Rounding::ZERO).inner_margin(Margin::same(22.0)).shadow(egui::epaint::Shadow { offset: vec2(0.0, 0.0), blur: 24.0, spread: 2.0, color: hud::with_alpha(CYAN, 60) }))
                .show(ctx, |ui| {
                    ui.set_max_width(420.0);
                    ui.label(RichText::new("> NÉN VIDEO CHO NHẸ MÁY?").font(bold(17.0)).color(CYAN));
                    ui.add_space(10.0);
                    ui.label(RichText::new(format!("“{title}” có độ phân giải {w}×{h}, trong khi màn hình của bạn chỉ {}×{}.", screen.0, screen.1)).font(FontId::proportional(15.0)));
                    ui.add_space(6.0);
                    let tail = to.map_or(String::new(), |(a, b)| format!(" ({a}×{b})"));
                    ui.label(RichText::new(format!("Nén xuống đúng độ phân giải màn hình{tail} sẽ giảm RAM và dung lượng chạy ngầm, hình nền nhìn vẫn như cũ. Bản gốc trong thư viện sẽ được thay bằng bản đã nén.")).font(FontId::proportional(14.0)).color(Color32::from_rgb(150, 160, 175)));
                    ui.add_space(14.0);
                    ui.horizontal(|ui| {
                        if pill_button(ui, None, "Nén", true).clicked() { answer = Some(true); }
                        if pill_button(ui, None, "Giữ nguyên", false).clicked() { answer = Some(false); }
                    });
                });
            if let Some(yes) = answer {
                self.shrink_ask.remove(0);
                if yes { self.shrink_one(ctx, id.clone(), w, h, st.library.items.iter().find(|i| i.id == id).map(|i| i.path.clone()).unwrap_or_default()); }
            }
            }
        }
        if self.shrinking > 0 { ctx.request_repaint_after(std::time::Duration::from_millis(500)); }

        if changed { self.engine.apply(&st); }
        if dock_changed {
            self.vdock.apply(&st.settings);
            if st.settings.material != self.material {
                // Re-skin live: window backdrop, palette, Explorer / Settings windows.
                self.material = st.settings.material;
                self.glass = apply_backdrop(self.material);
                self.pal = Palette::new(self.glass, self.material);
                setup_style(ctx, self.pal);
                sysglass::set_material(self.material);
            }
        }
        if changed || dock_changed { st.save(); }
        if self.tray_pause.is_checked() != st.settings.user_paused { self.tray_pause.set_checked(st.settings.user_paused); }
        if self.tray_mute.is_checked() != st.settings.is_muted { self.tray_mute.set_checked(st.settings.is_muted); }
        drop(st);

        // The file dialog is modal: open it only after the shared state is unlocked.
        if let Some(id) = self.music_req.take() { self.pick_music(ctx, id); }

        ctx.request_repaint_after(std::time::Duration::from_millis(1000));
        crate::watchdog::phase(crate::watchdog::UI, "idle");
    }
}
