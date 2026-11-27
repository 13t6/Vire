//! Small Windows helpers: taskbar windows / auto-hide, key presses, launching, window activation,
//! app identity and icons, taskbar pin import.

use super::*;

pub(super) unsafe fn taskbars() -> Vec<HWND> {
    unsafe extern "system" fn cb(h: HWND, data: LPARAM) -> BOOL {
        let mut buf = [0u16; 32];
        let n = GetClassNameW(h, &mut buf) as usize;
        if String::from_utf16_lossy(&buf[..n]) == "Shell_SecondaryTrayWnd" { (*(data.0 as *mut Vec<HWND>)).push(h); }
        BOOL(1)
    }
    let mut v = Vec::new();
    if let Ok(main) = FindWindowW(w!("Shell_TrayWnd"), PCWSTR::null()) { v.push(main); }
    let _ = EnumWindows(Some(cb), LPARAM(&mut v as *mut _ as isize));
    v
}

pub(super) fn appbar_data(hwnd: HWND) -> APPBARDATA { APPBARDATA { cbSize: std::mem::size_of::<APPBARDATA>() as u32, hWnd: hwnd, ..Default::default() } }

pub(super) unsafe fn taskbar_autohide() -> bool {
    let mut d = appbar_data(FindWindowW(w!("Shell_TrayWnd"), PCWSTR::null()).unwrap_or_default());
    SHAppBarMessage(ABM_GETSTATE, &mut d) & 1 != 0
}

pub(super) unsafe fn set_taskbar_autohide(on: bool) {
    let mut d = appbar_data(FindWindowW(w!("Shell_TrayWnd"), PCWSTR::null()).unwrap_or_default());
    d.lParam = LPARAM(if on { 1 } else { 0 });
    SHAppBarMessage(ABM_SETSTATE, &mut d);
}

/// Vire Dock has a dock on every monitor, so every Windows taskbar is hidden while it is on.
pub(super) unsafe fn show_taskbars(show: bool) {
    for h in taskbars() {
        let visible = IsWindowVisible(h).as_bool();
        // Explorer owns these windows: the *asynchronous* form never makes us wait for it (a plain
        // ShowWindow sends a message and blocks until Explorer answers — if Explorer is busy, so are we).
        let flags = SWP_NOMOVE | SWP_NOSIZE | SWP_NOZORDER | SWP_NOACTIVATE | SWP_ASYNCWINDOWPOS;
        let clipped = { let r = CreateRectRgn(0, 0, 0, 0); let t = GetWindowRgn(h, r); let _ = DeleteObject(r); t.0 != 0 };
        if show {
            if clipped { SetWindowRgn(h, HRGN::default(), true); }
            if !visible { let _ = SetWindowPos(h, None, 0, 0, 0, 0, flags | SWP_SHOWWINDOW); }
        } else {
            // Hidden = still running but clipped to a single pixel below the screen edge. A window
            // that is really hidden has no accessible buttons, and the dock reads them (tbmirror).
            if !visible { let _ = SetWindowPos(h, None, 0, 0, 0, 0, flags | SWP_SHOWWINDOW); continue; }
            if !clipped {
                dbg("taskbar visible -> clipping it away");
                let mut rc = RECT::default();
                let _ = GetWindowRect(h, &mut rc);
                let hh = (rc.bottom - rc.top).max(2);
                SetWindowRgn(h, CreateRectRgn(0, hh - 1, 1, hh), true);
            }
        }
    }
}

pub(super) unsafe fn primary_monitor() -> RECT {
    let mon = MonitorFromPoint(POINT { x: 0, y: 0 }, MONITOR_DEFAULTTOPRIMARY);
    let mut mi = MONITORINFO { cbSize: std::mem::size_of::<MONITORINFO>() as u32, ..Default::default() };
    let _ = GetMonitorInfoW(mon, &mut mi);
    mi.rcMonitor
}

pub(super) unsafe fn send_keys(keys: &[VIRTUAL_KEY]) {
    let mut inputs = Vec::new();
    let mk = |vk: VIRTUAL_KEY, up: bool| INPUT {
        r#type: INPUT_KEYBOARD,
        Anonymous: INPUT_0 { ki: KEYBDINPUT { wVk: vk, dwFlags: if up { KEYEVENTF_KEYUP } else { KEYBD_EVENT_FLAGS(0) }, ..Default::default() } },
    };
    for k in keys { inputs.push(mk(*k, false)); }
    for k in keys.iter().rev() { inputs.push(mk(*k, true)); }
    SendInput(&inputs, std::mem::size_of::<INPUT>() as i32);
}

pub(super) unsafe fn launch(target: &str, args: Option<&str>) {
    // The dock is never the foreground app, so a window opened from it (Task Manager, Settings…)
    // could start BEHIND the app you are using. Hand our right to take the foreground on to
    // whatever starts now (ASFW_ANY), the way the Windows taskbar does.
    let _ = AllowSetForegroundWindow(u32::MAX);
    let a = args.map(HSTRING::from);
    let _ = ShellExecuteW(None, w!("open"), &HSTRING::from(target), a.as_ref().map_or(PCWSTR::null(), |h| PCWSTR(h.as_ptr())), None, SW_SHOWNORMAL);
}

pub(super) unsafe fn prop_string(store: &IPropertyStore, key: &windows::Win32::UI::Shell::PropertiesSystem::PROPERTYKEY) -> Option<String> {
    let pv = store.GetValue(key).ok()?;
    let p = PropVariantToStringAlloc(&pv).ok()?;
    let s = p.to_string().ok();
    CoTaskMemFree(Some(p.0 as *const _));
    s.filter(|s| !s.is_empty())
}

pub(super) unsafe fn exe_path_of(hwnd: HWND) -> Option<String> {
    let mut pid = 0u32;
    GetWindowThreadProcessId(hwnd, Some(&mut pid));
    let h = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid).ok()?;
    let mut buf = [0u16; 1024];
    let mut len = buf.len() as u32;
    let ok = QueryFullProcessImageNameW(h, PROCESS_NAME_WIN32, PWSTR(buf.as_mut_ptr()), &mut len).is_ok();
    let _ = CloseHandle(h);
    ok.then(|| String::from_utf16_lossy(&buf[..len as usize]))
}

/// (grouping key, exe path) for a top-level window.
pub(super) unsafe fn key_of(hwnd: HWND) -> (String, String) {
    let exe = exe_path_of(hwnd).unwrap_or_default();
    // Asking a window for its properties makes it answer a message: skip windows that are not
    // answering at the moment instead of freezing the dock together with them.
    let aumid = if IsHungAppWindow(hwnd).as_bool() { None } else {
        SHGetPropertyStoreForWindow::<_, IPropertyStore>(hwnd).ok().and_then(|s| prop_string(&s, &PKEY_AppUserModel_ID))
    };
    // Packaged (MSIX / Store) apps such as Claude or Terminal don't expose it on the window, but
    // their process knows it — and it is the id the Windows taskbar groups them by.
    let aumid = aumid.or_else(|| package_aumid(hwnd));
    (aumid.unwrap_or_else(|| exe.to_lowercase()), exe)
}

unsafe fn package_aumid(hwnd: HWND) -> Option<String> {
    use windows::Win32::Storage::Packaging::Appx::GetApplicationUserModelId;
    let mut pid = 0u32;
    GetWindowThreadProcessId(hwnd, Some(&mut pid));
    let h = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid).ok()?;
    let mut buf = [0u16; 256];
    let mut len = buf.len() as u32;
    let r = GetApplicationUserModelId(h, &mut len, PWSTR(buf.as_mut_ptr()));
    let _ = CloseHandle(h);
    (r.0 == 0 && len > 1).then(|| String::from_utf16_lossy(&buf[..(len as usize - 1)]))
}

pub(super) unsafe fn is_app_window(h: HWND, own_pid: u32) -> bool {
    if !IsWindowVisible(h).as_bool() { return false; }
    if GetWindow(h, GW_OWNER).map_or(false, |o| !o.is_invalid()) { return false; }
    let ex = GetWindowLongPtrW(h, GWL_EXSTYLE) as u32;
    if ex & WS_EX_TOOLWINDOW.0 != 0 && ex & WS_EX_APPWINDOW.0 == 0 { return false; }
    if ex & WS_EX_NOACTIVATE.0 != 0 && ex & WS_EX_APPWINDOW.0 == 0 { return false; }
    let mut cloaked = 0u32;
    if windows::Win32::Graphics::Dwm::DwmGetWindowAttribute(h, windows::Win32::Graphics::Dwm::DWMWA_CLOAKED, &mut cloaked as *mut _ as *mut _, 4).is_ok() && cloaked != 0 { return false; }
    if GetWindowTextLengthW(h) == 0 { return false; }
    let _ = own_pid; // Vire's own window is listed like any other app (all its helper windows are tool/owned/hidden)
    let mut cls = [0u16; 64];
    let n = GetClassNameW(h, &mut cls) as usize;
    !matches!(String::from_utf16_lossy(&cls[..n]).as_str(), "Progman" | "WorkerW" | "Shell_TrayWnd" | "Shell_SecondaryTrayWnd" | "Windows.UI.Core.CoreWindow")
}

/// Brings a window to the front AND gives it the keyboard, even though the dock is not the
/// foreground app (a plain SetForegroundWindow is often refused, and then the window just
/// appears *behind* the one you were using). Attaches to the foreground thread's input for the
/// call, raises the window, and falls back to SwitchToThisWindow.
pub(super) unsafe fn activate(h: HWND) {
    use windows::Win32::System::Threading::AttachThreadInput;
    // A window of an elevated app (Task Manager, regedit…) ignores ShowWindow/SetWindowPos from
    // a normal-rights process (UIPI): SetWindowPos fails with "access denied". SwitchToThisWindow
    // is the one call that still restores and raises it — it is what the Windows taskbar relies on.
    if SetWindowPos(h, HWND_TOP, 0, 0, 0, 0, SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE).is_err() {
        SwitchToThisWindow(h, BOOL(1));
        return;
    }
    if IsIconic(h).as_bool() { let _ = ShowWindow(h, SW_RESTORE); }
    let fg = GetForegroundWindow();
    let fg_tid = if fg.is_invalid() { 0 } else { GetWindowThreadProcessId(fg, None) };
    let my_tid = GetCurrentThreadId();
    let attached = fg_tid != 0 && fg_tid != my_tid && AttachThreadInput(my_tid, fg_tid, true).as_bool();
    // Raise it above everything first (works even when focus is refused), then ask for focus.
    let _ = SetWindowPos(h, HWND_TOPMOST, 0, 0, 0, 0, SWP_NOMOVE | SWP_NOSIZE);
    let _ = SetWindowPos(h, HWND_NOTOPMOST, 0, 0, 0, 0, SWP_NOMOVE | SWP_NOSIZE);
    let _ = BringWindowToTop(h);
    let _ = SetForegroundWindow(h);
    if attached { let _ = AttachThreadInput(my_tid, fg_tid, false); }
    if GetForegroundWindow() != h { SwitchToThisWindow(h, BOOL(1)); }
}
pub(super) unsafe fn hide_from_windows_taskbar(h: HWND) {
    use windows::Win32::UI::Shell::{ITaskbarList, TaskbarList};
    if let Ok(tb) = CoCreateInstance::<_, ITaskbarList>(&TaskbarList, None, CLSCTX_INPROC_SERVER) {
        if tb.HrInit().is_ok() { let _ = tb.DeleteTab(h); }
    }
}

pub(super) fn window_title(h: HWND) -> String {
    let mut t = [0u16; 128];
    let n = unsafe { GetWindowTextW(h, &mut t) } as usize;
    let s = String::from_utf16_lossy(&t[..n]);
    if s.chars().count() > 42 { format!("{}…", s.chars().take(41).collect::<String>()) } else { s }
}

/// 96x96 premultiplied icon from any shell path (lnk, exe, `shell:AppsFolder\…`).
pub(super) unsafe fn load_icon(path: &str) -> Option<Img> {
    let item: IShellItemImageFactory = SHCreateItemFromParsingName(&HSTRING::from(path), None).ok()?;
    let hbmp = item.GetImage(SIZE { cx: 96, cy: 96 }, SIIGBF_ICONONLY | SIIGBF_BIGGERSIZEOK).ok()?;
    let mut bm = BITMAP::default();
    GetObjectW(hbmp, std::mem::size_of::<BITMAP>() as i32, Some(&mut bm as *mut _ as *mut _));
    let (w, h) = (bm.bmWidth, bm.bmHeight.abs());
    let mut bi = BITMAPINFO::default();
    bi.bmiHeader = BITMAPINFOHEADER { biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32, biWidth: w, biHeight: -h, biPlanes: 1, biBitCount: 32, ..Default::default() };
    let mut px = vec![0u32; (w * h).max(0) as usize];
    let dc = GetDC(None);
    let lines = GetDIBits(dc, hbmp, 0, h as u32, Some(px.as_mut_ptr() as *mut _), &mut bi, DIB_RGB_COLORS);
    ReleaseDC(None, dc);
    let _ = DeleteObject(hbmp);
    if lines == 0 || w <= 0 { return None; }
    // Shell icons come back premultiplied; if any channel exceeds alpha they're straight → premultiply.
    let straight = px.iter().any(|p| { let a = p >> 24; ((p >> 16) & 255) > a || ((p >> 8) & 255) > a || (p & 255) > a });
    if straight {
        for p in px.iter_mut() {
            let a = (*p >> 24) & 255;
            let m = |c: u32| c * a / 255;
            *p = (a << 24) | (m((*p >> 16) & 255) << 16) | (m((*p >> 8) & 255) << 8) | m(*p & 255);
        }
    }
    Some(Img { w, h, px })
}

/// Lower-cased target exe of a .lnk shortcut (None for non-shortcuts / packaged apps).
pub(super) fn lnk_target(path: &str) -> Option<String> {
    if !path.to_lowercase().ends_with(".lnk") { return Some(path.to_lowercase()).filter(|p| p.ends_with(".exe")); }
    unsafe {
        let link: IShellLinkW = CoCreateInstance(&ShellLink, None, CLSCTX_INPROC_SERVER).ok()?;
        link.cast::<IPersistFile>().ok()?.Load(&HSTRING::from(path), STGM_READ).ok()?;
        let mut buf = [0u16; 1024];
        link.GetPath(&mut buf, std::ptr::null_mut(), 0).ok()?;
        let n = buf.iter().position(|&c| c == 0).unwrap_or(buf.len());
        Some(String::from_utf16_lossy(&buf[..n]).to_lowercase()).filter(|s| !s.is_empty())
    }
}

/// Pins from the Windows taskbar's pinned-shortcut folder (first run of Vire Dock).
pub fn import_taskbar_pins() -> Vec<Pin> {
    let mut out = Vec::new();
    let Some(appdata) = std::env::var_os("APPDATA") else { return out };
    let dir = std::path::PathBuf::from(appdata).join(r"Microsoft\Internet Explorer\Quick Launch\User Pinned\TaskBar");
    let Ok(rd) = std::fs::read_dir(&dir) else { return out };
    let mut files: Vec<_> = rd.flatten().map(|e| e.path()).filter(|p| p.extension().map_or(false, |e| e.eq_ignore_ascii_case("lnk"))).collect();
    files.sort();
    unsafe {
        for lnk in files {
            let Ok(link) = CoCreateInstance::<_, IShellLinkW>(&ShellLink, None, CLSCTX_INPROC_SERVER) else { continue };
            let Ok(pf) = link.cast::<IPersistFile>() else { continue };
            if pf.Load(&HSTRING::from(lnk.as_os_str()), STGM_READ).is_err() { continue; }
            let aumid = link.cast::<IPropertyStore>().ok().and_then(|s| prop_string(&s, &PKEY_AppUserModel_ID));
            let mut buf = [0u16; 1024];
            let target = link.GetPath(&mut buf, std::ptr::null_mut(), 0).ok().map(|_| {
                let n = buf.iter().position(|&c| c == 0).unwrap_or(buf.len());
                String::from_utf16_lossy(&buf[..n])
            }).unwrap_or_default();
            let key = aumid.unwrap_or_else(|| target.to_lowercase());
            if key.is_empty() { continue; }
            let name = lnk.file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_default();
            let exe = (!target.is_empty()).then(|| target.to_lowercase());
            out.push(Pin { key, launch: lnk.to_string_lossy().to_string(), name, exe });
        }
    }
    out
}

