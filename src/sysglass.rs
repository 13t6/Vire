//! System-wide liquid glass, the cheap way:
//! * turns on Windows' own Transparency effects → Acrylic on the taskbar, Start, notification
//!   center, Quick Settings, Mica on Settings / File Explorer (all drawn by DWM, zero cost here);
//! * upgrades File Explorer and Settings windows from Mica to the more see-through Acrylic
//!   backdrop via an out-of-context WinEvent hook (no polling, nothing injected).
//! The previous Transparency value is remembered and restored when switched off.

use std::sync::atomic::{AtomicBool, AtomicIsize, AtomicU8, Ordering};
use windows::core::{w, PCWSTR};
use windows::Win32::Foundation::{BOOL, HWND, LPARAM, WPARAM};
use windows::Win32::Graphics::Dwm::{DwmSetWindowAttribute, DWMSBT_AUTO, DWMSBT_TABBEDWINDOW, DWMSBT_TRANSIENTWINDOW, DWMWA_SYSTEMBACKDROP_TYPE, DWM_SYSTEMBACKDROP_TYPE};
use windows::Win32::System::Registry::*;
use windows::Win32::UI::Accessibility::{SetWinEventHook, UnhookWinEvent, HWINEVENTHOOK};
use windows::Win32::UI::WindowsAndMessaging::*;

const PERSONALIZE: PCWSTR = w!("Software\\Microsoft\\Windows\\CurrentVersion\\Themes\\Personalize");
static ACTIVE: AtomicBool = AtomicBool::new(false);
static HOOK: AtomicIsize = AtomicIsize::new(0);
/// 0 = Acrylic, 1 = Mica, 2 = Glass (see `Material`).
static MATERIAL: AtomicU8 = AtomicU8::new(0);

fn backdrop_kind() -> DWM_SYSTEMBACKDROP_TYPE {
    // Mica → "Mica Alt" (tabbed): more wallpaper colour than plain Mica. Acrylic / Glass → Acrylic.
    if MATERIAL.load(Ordering::Relaxed) == 1 { DWMSBT_TABBEDWINDOW } else { DWMSBT_TRANSIENTWINDOW }
}

/// Changes the material used for Explorer / Settings windows (re-applies to open ones).
pub fn set_material(m: crate::model::Material) {
    let v = match m { crate::model::Material::Acrylic => 0, crate::model::Material::Mica => 1, crate::model::Material::Glass => 2 };
    if MATERIAL.swap(v, Ordering::Relaxed) != v && ACTIVE.load(Ordering::Relaxed) {
        for_each_target(|h| set_backdrop(h, backdrop_kind()));
    }
}

fn read_transparency() -> Option<u32> {
    let mut v = 0u32;
    let mut size = 4u32;
    unsafe {
        RegGetValueW(HKEY_CURRENT_USER, PERSONALIZE, w!("EnableTransparency"), RRF_RT_REG_DWORD, None,
            Some(&mut v as *mut _ as *mut _), Some(&mut size)).ok().ok()?;
    }
    Some(v)
}

fn write_transparency(v: u32) {
    unsafe {
        let _ = RegSetKeyValueW(HKEY_CURRENT_USER, PERSONALIZE, w!("EnableTransparency"), REG_DWORD.0,
            Some(&v as *const _ as *const _), 4);
    }
    // Tell Explorer & apps to re-read colors (off the UI thread: a hung app could stall it).
    std::thread::spawn(|| unsafe {
        let mut res = 0usize;
        let _ = SendMessageTimeoutW(HWND_BROADCAST, WM_SETTINGCHANGE, WPARAM(0), LPARAM(w!("ImmersiveColorSet").as_ptr() as isize),
            SMTO_ABORTIFHUNG, 2000, Some(&mut res));
    });
}

fn is_glass_target(h: HWND) -> bool {
    unsafe {
        let mut cls = [0u16; 64];
        let n = GetClassNameW(h, &mut cls) as usize;
        match String::from_utf16_lossy(&cls[..n]).as_str() {
            "CabinetWClass" => true, // File Explorer
            "ApplicationFrameWindow" => {
                let mut t = [0u16; 64];
                let n = GetWindowTextW(h, &mut t) as usize;
                matches!(String::from_utf16_lossy(&t[..n]).as_str(), "Settings" | "Cài đặt")
            }
            _ => false,
        }
    }
}

fn set_backdrop(h: HWND, kind: DWM_SYSTEMBACKDROP_TYPE) {
    unsafe { let _ = DwmSetWindowAttribute(h, DWMWA_SYSTEMBACKDROP_TYPE, &kind as *const _ as *const _, 4); }
}

unsafe extern "system" fn on_event(_: HWINEVENTHOOK, _ev: u32, h: HWND, id_object: i32, id_child: i32, _: u32, _: u32) {
    if id_object != 0 || id_child != 0 || !ACTIVE.load(Ordering::Relaxed) { return; } // OBJID_WINDOW only
    if GetParent(h).map_or(false, |p| !p.is_invalid()) { return; }
    if is_glass_target(h) { set_backdrop(h, backdrop_kind()); }
}

fn for_each_target(f: impl Fn(HWND)) {
    unsafe extern "system" fn cb(h: HWND, data: LPARAM) -> BOOL {
        let f = &*(data.0 as *const &dyn Fn(HWND));
        if IsWindowVisible(h).as_bool() && is_glass_target(h) { f(h); }
        BOOL(1)
    }
    let r: &dyn Fn(HWND) = &f;
    unsafe { let _ = EnumWindows(Some(cb), LPARAM(&r as *const _ as isize)); }
}

/// Must be called from a thread that pumps messages (the UI / main thread).
/// `prev` stores the user's original Transparency value so it can be restored.
pub fn set_enabled(on: bool, prev: &mut Option<u32>) {
    if on {
        if prev.is_none() { *prev = Some(read_transparency().unwrap_or(1)); }
        if read_transparency() != Some(1) { write_transparency(1); }
        ACTIVE.store(true, Ordering::Relaxed);
        if HOOK.load(Ordering::Relaxed) == 0 {
            let h = unsafe { SetWinEventHook(EVENT_OBJECT_SHOW, EVENT_OBJECT_SHOW, None, Some(on_event), 0, 0, WINEVENT_OUTOFCONTEXT | WINEVENT_SKIPOWNPROCESS) };
            HOOK.store(h.0 as isize, Ordering::Relaxed);
        }
        for_each_target(|h| set_backdrop(h, backdrop_kind()));
    } else {
        ACTIVE.store(false, Ordering::Relaxed);
        let h = HOOK.swap(0, Ordering::Relaxed);
        if h != 0 { unsafe { let _ = UnhookWinEvent(HWINEVENTHOOK(h as *mut _)); } }
        for_each_target(|h| set_backdrop(h, DWMSBT_AUTO));
        if let Some(p) = prev.take() { if read_transparency() != Some(p) { write_transparency(p); } }
    }
}
