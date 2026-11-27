#![windows_subsystem = "windows"]
//! Vire — live video wallpaper + macOS-style dock taskbar for Windows.
//! (Author: NSH.)

mod audio;
mod autostart;
mod desktop;
mod dsp;
mod engine;
mod gpu;
mod img;
mod lyrics;
mod media;
mod model;
mod nowplaying;
mod player;
mod raster;
mod sysglass;
mod tray;
mod ui;
mod vdock;
mod watchdog;
mod widget;
mod widgets;

use std::sync::{Arc, Mutex};
use windows::core::{w, PCWSTR};
use windows::Win32::Foundation::{GetLastError, ERROR_ALREADY_EXISTS, LPARAM, WPARAM};
use windows::Win32::System::Com::{CoInitializeEx, COINIT_APARTMENTTHREADED};
use windows::Win32::System::Threading::{CreateMutexW, GetCurrentProcess, SetProcessWorkingSetSize};
use windows::Win32::UI::WindowsAndMessaging::{FindWindowW, PostMessageW};

pub const APP_TITLE: &str = "Vire";

/// Hands cold pages (UI fonts, textures, start-up allocations) back to Windows.
pub fn trim_memory() {
    unsafe { let _ = SetProcessWorkingSetSize(GetCurrentProcess(), usize::MAX, usize::MAX); }
}

fn native_options(icon: Option<Arc<eframe::egui::IconData>>) -> eframe::NativeOptions {
    let mut viewport = eframe::egui::ViewportBuilder::default()
        .with_title(APP_TITLE)
        .with_inner_size([1120.0, 740.0])
        .with_min_inner_size([920.0, 620.0])
        .with_transparent(true)
        .with_drag_and_drop(true);
    if let Some(icon) = icon { viewport = viewport.with_icon(icon); }
    eframe::NativeOptions { viewport, centered: true, run_and_return: true, ..Default::default() }
}

fn main() {
    // Hidden dev flag: `--preview-dock=out.png [--edge=left|right] [--material=glass|mica|acrylic]`.
    if let Some(out) = std::env::args().find_map(|a| a.strip_prefix("--preview-dock=").map(str::to_owned)) {
        unsafe { let _ = CoInitializeEx(None, COINIT_APARTMENTTHREADED); }
        let arg = |k: &str| std::env::args().find_map(|a| a.strip_prefix(k).map(str::to_ascii_lowercase));
        let edge = match arg("--edge=").as_deref() { Some("left") => model::Edge::Left, Some("right") => model::Edge::Right, _ => model::Edge::Bottom };
        let material = match arg("--material=").as_deref() { Some("glass") => Some(model::Material::Glass), Some("mica") => Some(model::Material::Mica), Some("acrylic") => Some(model::Material::Acrylic), _ => None };
        let state = Arc::new(Mutex::new(model::AppState::load()));
        let ok = vdock::preview(state, edge, material, std::path::Path::new(&out));
        std::process::exit(if ok { 0 } else { 1 });
    }
    if let Some(out) = std::env::args().find_map(|a| a.strip_prefix("--dev-bgapps=").map(str::to_owned)) {
        let _ = std::fs::write(out, vdock::dump_background_apps());
        std::process::exit(0);
    }
    // Hidden dev flag: `--dev-import=video` imports a video through the normal path and prints the result.
    if let Some(v) = std::env::args().find_map(|a| a.strip_prefix("--dev-import=").map(str::to_owned)) {
        let r = media::import_video(std::path::Path::new(&v), &[]);
        let msg = match r { Ok(media::Imported::New(i)) => format!("new {} {}x{} {}", i.path.display(), i.width, i.height, i.duration_secs), Ok(_) => "existing".into(), Err(e) => e };
        let _ = std::fs::write(std::env::temp_dir().join("vire_import.txt"), msg);
        std::process::exit(0);
    }
    // Hidden dev flag: `--dev-audio=out.txt` captures system audio for 8 s and logs what it hears.
    if let Some(out) = std::env::args().find_map(|a| a.strip_prefix("--dev-audio=").map(str::to_owned)) {
        let _lease = audio::acquire();
        let mut log = String::new();
        let t0 = std::time::Instant::now();
        while t0.elapsed().as_secs() < 8 {
            std::thread::sleep(std::time::Duration::from_millis(400));
            let (f, seq) = audio::snapshot();
            let (pk, pv) = f.bands.iter().enumerate().fold((0, 0.0f32), |a, (i, &v)| if v > a.1 { (i, v) } else { a });
            log += &format!("{:?} signal={} seq={} level={:.4} peak_band={} ({:.2}) bass={:.2} treble={:.2}\n", audio::state(), audio::has_signal(), seq, f.level, pk, pv,
                f.bands[..12].iter().sum::<f32>() / 12.0, f.bands[72..].iter().sum::<f32>() / 24.0);
        }
        drop(_lease);
        std::thread::sleep(std::time::Duration::from_millis(300));
        log += &format!("after release: {:?}\n", audio::state());
        let _ = std::fs::write(out, log);
        std::process::exit(0);
    }
    // Hidden dev flag: `--dev-widget-shots=<dir>` renders every clock / visualizer style to PNG
    // (visualizers listen to the real system audio for 2 s first), over a mid-grey backdrop.
    if let Some(dir) = std::env::args().find_map(|a| a.strip_prefix("--dev-widget-shots=").map(str::to_owned)) {
        unsafe { let _ = CoInitializeEx(None, COINIT_APARTMENTTHREADED); }
        let _lease = audio::acquire();
        let dir = std::path::PathBuf::from(dir);
        let _ = std::fs::create_dir_all(&dir);
        let shot = |name: &str, kind: &str, cfg: serde_json::Value, size: (i32, i32), frames: usize| {
            let Some(k) = widgets::kind(kind) else { return };
            let mut w = (k.create)();
            let mut c = raster::Canvas::new(size.0, size.1);
            for i in 0..frames {
                let (f, _) = audio::snapshot();
                c.px.fill(0);
                let ctx = widgets::Ctx { now: widgets::now_local(), dt: 0.033, scale: 1.0, audio: Some(&f), signal: audio::has_signal(), builtin_playing: false, preview: i + 1 == frames };
                w.render(&ctx, &cfg, &mut c);
                if i + 1 < frames { std::thread::sleep(std::time::Duration::from_millis(33)); }
            }
            // premultiplied source over a backdrop
            let bg = [62u32, 68, 86];
            let px: Vec<u32> = c.px.iter().map(|p| {
                let (a, r, g, b) = (p >> 24, (p >> 16) & 255, (p >> 8) & 255, p & 255);
                let k = 255 - a;
                0xFF00_0000 | ((r + bg[0] * k / 255).min(255) << 16) | ((g + bg[1] * k / 255).min(255) << 8) | (b + bg[2] * k / 255).min(255)
            }).collect();
            img::save_png(size.0 as u32, size.1 as u32, &px, &dir.join(format!("{name}.png")));
        };
        for s in 0..5u8 { shot(&format!("clock{s}"), "clock", serde_json::to_value(widgets::clock::ClockCfg::preset(s)).unwrap(), (440, 190), 1); }
        let mut sec = widgets::clock::ClockCfg::preset(2); sec.show_seconds = true; sec.hour24 = false;
        shot("clock_glass_12h_sec", "clock", serde_json::to_value(sec).unwrap(), (440, 190), 1);
        for s in 0..4u8 {
            let mut v = widgets::viz::VizCfg::default(); v.style = s; v.background = s == 2;
            shot(&format!("viz{s}"), "visualizer", serde_json::to_value(v).unwrap(), (520, 220), 90);
        }
        std::process::exit(0);
    }
    // Hidden dev flag: `--dev-thumb=<video>|<out.png>` builds one thumbnail and exits.
    if let Some(a) = std::env::args().find_map(|a| a.strip_prefix("--dev-thumb=").map(str::to_owned)) {
        unsafe { let _ = CoInitializeEx(None, COINIT_APARTMENTTHREADED); }
        let mut it = a.splitn(2, '|');
        let (v, o) = (it.next().unwrap_or_default(), it.next().unwrap_or_default());
        let ok = media::make_thumbnail(std::path::Path::new(v), std::path::Path::new(o));
        let _ = std::fs::write(format!("{o}.log.txt"), format!("video={v}\nout={o}\nok={ok}\nerr={}", img::LAST_ERROR.with(|l| l.borrow().clone())));
        std::process::exit(if ok { 0 } else { 1 });
    }
    // First run on this PC (or from a new folder): pick the power-saving GPU, then restart once so it applies.
    if gpu::ensure_power_saving() {
        if let Ok(exe) = std::env::current_exe() {
            if std::process::Command::new(exe).args(std::env::args().skip(1)).spawn().is_ok() { std::process::exit(0); }
        }
    }
    // Single instance: a second launch just asks the first one to open its dashboard.
    unsafe {
        let mutex = CreateMutexW(None, true, w!("Vire_SingleInstance"));
        if GetLastError() == ERROR_ALREADY_EXISTS {
            if let Ok(hwnd) = FindWindowW(tray::MSG_WINDOW_CLASS, PCWSTR::null()) {
                let _ = PostMessageW(hwnd, tray::WM_OPEN_DASHBOARD, WPARAM(0), LPARAM(0));
            }
            return;
        }
        std::mem::forget(mutex);
    }

    vdock::enable_dark_menus();
    // Any unexpected crash gives the Windows taskbar back before the process ends.
    std::panic::set_hook(Box::new(|_| vdock::emergency_restore()));
    // STA COM on the UI thread: WIC, the file dialog and winit's drag & drop all need it.
    unsafe { let _ = CoInitializeEx(None, COINIT_APARTMENTTHREADED); }
    let start_in_tray = std::env::args().any(|a| a == "--tray");
    let state = Arc::new(Mutex::new(model::AppState::load()));
    // Always listed in the Windows search box / Start menu (a shortcut that follows the exe).
    autostart::set_start_shortcut(true);
    let engine = engine::Engine::start();
    if std::env::args().any(|a| a == "--dev-ignore-fullscreen") { vdock::IGNORE_FULLSCREEN.store(true, std::sync::atomic::Ordering::Relaxed); }
    let vdock = vdock::VDock::start(state.clone());
    // Hidden dev flag: cycles Vire Dock through the edges at runtime (same path as the UI).
    if std::env::args().any(|a| a == "--dev-cycle-edges") {
        let (st, vd) = (state.clone(), vdock.clone());
        std::thread::spawn(move || {
            for edge in [model::Edge::Left, model::Edge::Right, model::Edge::Bottom] {
                std::thread::sleep(std::time::Duration::from_secs(5));
                let mut s = st.lock().unwrap();
                s.settings.vd_edge = edge;
                vd.apply(&s.settings);
            }
        });
    }
    // Saved shuffle / repeat first: the wallpaper's matched song (started by the apply below) then
    // overrides repeat with "repeat this song".
    {
        let st = state.lock().unwrap();
        engine.player(player::PCmd::Shuffle(st.settings.pl_shuffle));
        engine.player(player::PCmd::Repeat(st.settings.pl_repeat));
    }
    engine.apply(&state.lock().unwrap());
    // Restore the last song (paused) so the widget and the media keys work right after start-up,
    // with no window open.
    {
        let st = state.lock().unwrap();
        if let Some(list) = std::env::var_os("VIRE_DEV_PLAY") {
            engine.player(player::PCmd::SetQueue(std::env::split_paths(&list).collect(), 0));
        } else if let Some(p) = st.settings.pl_last.clone().filter(|p| p.exists()) {
            engine.player(player::PCmd::Restore(p, st.settings.pl_last_pos));
        }
    }
    widget::start(state.clone(), engine.clone());
    watchdog::start();
    let hub = widgets::Hub::load();
    widgets::start(hub.clone(), engine.clone());
    let tray = tray::Tray::create(state.clone(), engine.clone(), vdock.clone());
    {
        // Re-arm system liquid glass (hook lives on this thread, which always pumps messages).
        let mut st = state.lock().unwrap();
        sysglass::set_material(st.settings.material);
        if st.settings.sys_glass {
            let mut prev = st.settings.sys_glass_prev;
            sysglass::set_enabled(true, &mut prev);
            st.settings.sys_glass_prev = prev;
            st.save();
        }
    }

    let icon = img::decode(include_bytes!("../assets/icon_128.png"))
        .map(|i| Arc::new(eframe::egui::IconData { rgba: i.px, width: i.w, height: i.h }));

    let mut show = !start_in_tray;
    loop {
        if show {
            let (st, en, vd, hb) = (state.clone(), engine.clone(), vdock.clone(), hub.clone());
            let (pause, mute) = (tray.pause.clone(), tray.mute.clone());
            let _ = eframe::run_native(APP_TITLE, native_options(icon.clone()),
                Box::new(move |cc| Ok(Box::new(ui::App::new(cc, st, en, vd, pause, mute, hb)))));
            tray::set_ui_ctx(None);
        }
        // Dashboard closed: only the tray + wallpaper engine + dock stay alive.
        trim_memory();
        tray::Tray::wait_for_open_request();
        show = true;
    }
}
