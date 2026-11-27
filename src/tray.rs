//! System tray + process-wide "open the dashboard" signalling.
//!
//! The dashboard window is fully destroyed when hidden (frees the GPU/OpenGL context, ~150 MB);
//! the tray and the wallpaper engine keep running. Opening it again re-runs eframe.

use crate::engine::Engine;
use crate::model::AppState;
use crate::APP_TITLE;
use eframe::egui;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::{Arc, Mutex};
use tray_icon::menu::{CheckMenuItem, Menu, MenuEvent, MenuItem, PredefinedMenuItem};
use tray_icon::{MouseButton, MouseButtonState, TrayIcon, TrayIconBuilder, TrayIconEvent};
use windows::core::w;
use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, WPARAM};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::System::Threading::GetCurrentThreadId;
use windows::Win32::UI::WindowsAndMessaging::*;

pub const WM_OPEN_DASHBOARD: u32 = WM_APP + 1;
pub const MSG_WINDOW_CLASS: windows::core::PCWSTR = w!("VireMessageWindow");

static MAIN_TID: AtomicU32 = AtomicU32::new(0);
static UI_RUNNING: AtomicBool = AtomicBool::new(false);
static UI_CTX: Mutex<Option<egui::Context>> = Mutex::new(None);

pub fn set_ui_ctx(ctx: Option<egui::Context>) {
    UI_RUNNING.store(ctx.is_some(), Ordering::SeqCst);
    *UI_CTX.lock().unwrap() = ctx;
}

/// Brings up the dashboard: focuses it if open, otherwise wakes the main loop to create it.
pub fn request_open() {
    if UI_RUNNING.load(Ordering::SeqCst) {
        if let Some(ctx) = UI_CTX.lock().unwrap().clone() {
            unsafe {
                if let Ok(hwnd) = FindWindowW(None, &windows::core::HSTRING::from(APP_TITLE)) {
                    let _ = ShowWindow(hwnd, SW_RESTORE);
                    let _ = SetForegroundWindow(hwnd);
                }
            }
            ctx.send_viewport_cmd(egui::ViewportCommand::Focus);
            ctx.request_repaint();
        }
    } else {
        unsafe { let _ = PostThreadMessageW(MAIN_TID.load(Ordering::SeqCst), WM_OPEN_DASHBOARD, WPARAM(0), LPARAM(0)); }
    }
}

unsafe extern "system" fn msg_wnd_proc(hwnd: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    if msg == WM_OPEN_DASHBOARD { request_open(); return LRESULT(0); }
    DefWindowProcW(hwnd, msg, wp, lp)
}

pub struct Tray {
    _icon: Option<TrayIcon>,
    pub pause: CheckMenuItem,
    pub mute: CheckMenuItem,
}

impl Tray {
    pub fn create(state: Arc<Mutex<AppState>>, engine: Engine, vdock: crate::vdock::VDock) -> Tray {
        unsafe {
            MAIN_TID.store(GetCurrentThreadId(), Ordering::SeqCst);
            // Hidden window a second launch of the exe can poke to open the dashboard.
            let hinst = GetModuleHandleW(None).unwrap_or_default();
            let wc = WNDCLASSW { lpfnWndProc: Some(msg_wnd_proc), hInstance: hinst.into(), lpszClassName: MSG_WINDOW_CLASS, ..Default::default() };
            RegisterClassW(&wc);
            let _ = CreateWindowExW(WINDOW_EX_STYLE(0), MSG_WINDOW_CLASS, w!(""), WINDOW_STYLE(0), 0, 0, 0, 0, None, None, hinst, None);
        }

        let (paused, muted) = { let st = state.lock().unwrap(); (st.settings.user_paused, st.settings.is_muted) };
        let open = MenuItem::with_id("open", "Mở Vire", true, None);
        let pause = CheckMenuItem::with_id("pause", "Tạm dừng hình nền", true, paused, None);
        let next = MenuItem::with_id("next", "Chọn hình nền tiếp theo", true, None);
        let mute = CheckMenuItem::with_id("mute", "Tắt tiếng", true, muted, None);
        let quit = MenuItem::with_id("quit", "Thoát", true, None);
        let menu = Menu::new();
        let _ = menu.append_items(&[&open, &PredefinedMenuItem::separator(), &pause, &next, &mute, &PredefinedMenuItem::separator(), &quit]);

        let icon = crate::img::decode(include_bytes!("../assets/icon_64.png"))
            .and_then(|i| tray_icon::Icon::from_rgba(i.px, i.w, i.h).ok());
        let mut b = TrayIconBuilder::new().with_menu(Box::new(menu)).with_tooltip(APP_TITLE).with_menu_on_left_click(false);
        if let Some(icon) = icon { b = b.with_icon(icon); }

        MenuEvent::set_event_handler(Some(move |e: MenuEvent| {
            match e.id.0.as_str() {
                "open" => request_open(),
                "quit" => { vdock.shutdown(); engine.shutdown(); std::process::exit(0); }
                id => {
                    let mut st = state.lock().unwrap();
                    match id {
                        "pause" => st.settings.user_paused = !st.settings.user_paused,
                        "next" => st.select_next(),
                        "mute" => st.settings.is_muted = !st.settings.is_muted,
                        _ => return,
                    }
                    engine.apply(&st);
                    st.save();
                    if let Some(ctx) = UI_CTX.lock().unwrap().as_ref() { ctx.request_repaint(); }
                }
            }
        }));
        TrayIconEvent::set_event_handler(Some(|e: TrayIconEvent| match e {
            TrayIconEvent::Click { button: MouseButton::Left, button_state: MouseButtonState::Up, .. }
            | TrayIconEvent::DoubleClick { button: MouseButton::Left, .. } => request_open(),
            _ => {}
        }));

        Tray { _icon: b.build().ok(), pause, mute }
    }

    /// Plain Win32 message loop used while the dashboard is closed. Returns when it should open.
    pub fn wait_for_open_request() {
        unsafe {
            let mut msg = MSG::default();
            while GetMessageW(&mut msg, None, 0, 0).as_bool() {
                if msg.hwnd.is_invalid() && msg.message == WM_OPEN_DASHBOARD { return; }
                let _ = TranslateMessage(&msg);
                DispatchMessageW(&msg);
            }
        }
    }
}
