//! Win32 desktop injection: puts our video windows behind the desktop icons.
//!
//! Classic (Win10 / Win11 < 24H2):  Progman --0x052C--> spawns a top-level WorkerW behind
//!   the WorkerW that hosts SHELLDLL_DefView; we SetParent into it.
//! Raised desktop (Win11 24H2+):    Progman has WS_EX_NOREDIRECTIONBITMAP and owns both
//!   SHELLDLL_DefView and WorkerW as children; we parent to Progman as a layered child,
//!   z-ordered just below SHELLDLL_DefView and above WorkerW.

use windows::core::{w, PCWSTR};
use windows::Win32::Foundation::{BOOL, HWND, LPARAM, POINT, RECT, WPARAM};
use windows::Win32::Graphics::Gdi::{EnumDisplayMonitors, GetMonitorInfoW, HDC, HMONITOR, MONITORINFO, MONITORINFOEXW};
use windows::Win32::UI::WindowsAndMessaging::*;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Monitor {
    pub device: String,
    pub rect: RECT,
    pub primary: bool,
}

impl Monitor {
    pub fn width(&self) -> i32 { self.rect.right - self.rect.left }
    pub fn height(&self) -> i32 { self.rect.bottom - self.rect.top }
    /// "Màn hình 1 (chính)" style label.
    pub fn label(&self) -> String {
        let n = self.device.trim_start_matches(r"\\.\DISPLAY");
        format!("Màn hình {}{}", n, if self.primary { " (chính)" } else { "" })
    }
}

pub fn monitors() -> Vec<Monitor> {
    unsafe extern "system" fn cb(hmon: HMONITOR, _: HDC, _: *mut RECT, data: LPARAM) -> BOOL {
        let list = &mut *(data.0 as *mut Vec<Monitor>);
        let mut info = MONITORINFOEXW::default();
        info.monitorInfo.cbSize = std::mem::size_of::<MONITORINFOEXW>() as u32;
        if GetMonitorInfoW(hmon, &mut info as *mut _ as *mut MONITORINFO).as_bool() {
            let len = info.szDevice.iter().position(|&c| c == 0).unwrap_or(info.szDevice.len());
            list.push(Monitor {
                device: String::from_utf16_lossy(&info.szDevice[..len]),
                rect: info.monitorInfo.rcMonitor,
                primary: info.monitorInfo.dwFlags & 1 != 0,
            });
        }
        BOOL(1)
    }
    let mut list: Vec<Monitor> = Vec::new();
    unsafe {
        let _ = EnumDisplayMonitors(HDC::default(), None, Some(cb), LPARAM(&mut list as *mut _ as isize));
    }
    list.sort_by(|a, b| a.device.cmp(&b.device));
    list
}

pub struct DesktopTarget {
    pub parent: HWND,
    /// Set on raised desktops: insert our window right after (below) this one.
    pub insert_after: Option<HWND>,
    pub raised_worker: Option<HWND>,
}

fn find_window_ex(parent: HWND, after: HWND, class: PCWSTR) -> HWND {
    unsafe { FindWindowExW(parent, after, class, PCWSTR::null()).unwrap_or_default() }
}

pub fn locate_target() -> Option<DesktopTarget> {
    unsafe {
        let progman = FindWindowW(w!("Progman"), PCWSTR::null()).ok()?;
        let mut res = 0usize;
        // Ask Progman to spawn the WorkerW (both variants of the undocumented message).
        let _ = SendMessageTimeoutW(progman, 0x052C, WPARAM(0xD), LPARAM(0x1), SMTO_NORMAL, 1000, Some(&mut res));
        let _ = SendMessageTimeoutW(progman, 0x052C, WPARAM(0), LPARAM(0), SMTO_NORMAL, 1000, Some(&mut res));

        let ex = GetWindowLongPtrW(progman, GWL_EXSTYLE) as u32;
        let defview_in_progman = find_window_ex(progman, HWND::default(), w!("SHELLDLL_DefView"));
        if ex & WS_EX_NOREDIRECTIONBITMAP.0 != 0 && !defview_in_progman.is_invalid() {
            let worker = find_window_ex(progman, HWND::default(), w!("WorkerW"));
            return Some(DesktopTarget {
                parent: progman,
                insert_after: Some(defview_in_progman),
                raised_worker: if worker.is_invalid() { None } else { Some(worker) },
            });
        }

        // Classic: find the top-level window hosting SHELLDLL_DefView, take the WorkerW after it.
        unsafe extern "system" fn enum_cb(top: HWND, data: LPARAM) -> BOOL {
            let out = &mut *(data.0 as *mut HWND);
            let defview = FindWindowExW(top, HWND::default(), w!("SHELLDLL_DefView"), PCWSTR::null()).unwrap_or_default();
            if !defview.is_invalid() {
                *out = FindWindowExW(HWND::default(), top, w!("WorkerW"), PCWSTR::null()).unwrap_or_default();
            }
            BOOL(1)
        }
        let mut worker = HWND::default();
        let _ = EnumWindows(Some(enum_cb), LPARAM(&mut worker as *mut _ as isize));
        if worker.is_invalid() {
            // Some builds keep the empty WorkerW as a child of Progman.
            worker = find_window_ex(progman, HWND::default(), w!("WorkerW"));
        }
        let parent = if worker.is_invalid() { progman } else { worker };
        Some(DesktopTarget { parent, insert_after: None, raised_worker: None })
    }
}

/// Re-parents `hwnd` into the desktop and moves it over `mon`.
pub fn attach(hwnd: HWND, target: &DesktopTarget, mon: &Monitor) {
    unsafe {
        if target.insert_after.is_some() {
            let ex = GetWindowLongPtrW(hwnd, GWL_EXSTYLE);
            SetWindowLongPtrW(hwnd, GWL_EXSTYLE, ex | WS_EX_LAYERED.0 as isize);
            let _ = SetLayeredWindowAttributes(hwnd, windows::Win32::Foundation::COLORREF(0), 255, LWA_ALPHA);
        }
        let style = GetWindowLongPtrW(hwnd, GWL_STYLE);
        let style = (style & !(WS_POPUP.0 as isize)) | WS_CHILD.0 as isize | WS_VISIBLE.0 as isize;
        SetWindowLongPtrW(hwnd, GWL_STYLE, style);
        let _ = SetParent(hwnd, target.parent);
        place(hwnd, target, mon);
    }
}

pub fn place(hwnd: HWND, target: &DesktopTarget, mon: &Monitor) {
    unsafe {
        let mut pt = POINT { x: mon.rect.left, y: mon.rect.top };
        let _ = windows::Win32::Graphics::Gdi::ScreenToClient(target.parent, &mut pt);
        let after = target.insert_after.unwrap_or(HWND_BOTTOM);
        let _ = SetWindowPos(hwnd, after, pt.x, pt.y, mon.width(), mon.height(),
            SWP_NOACTIVATE | SWP_SHOWWINDOW | SWP_FRAMECHANGED);
        // The static-wallpaper layer goes to the very bottom, under *all* our windows. (Putting it
        // right under the newest one let it cover the video of the monitors placed before it.)
        if let Some(worker) = target.raised_worker {
            let _ = SetWindowPos(worker, HWND_BOTTOM, 0, 0, 0, 0, SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE);
        }
    }
}

/// Repaints the static Windows wallpaper (clears leftovers after we detach).
pub fn refresh_static_wallpaper() {
    use windows::Win32::UI::WindowsAndMessaging::{SystemParametersInfoW, SPI_GETDESKWALLPAPER, SPI_SETDESKWALLPAPER, SPIF_UPDATEINIFILE, SYSTEM_PARAMETERS_INFO_UPDATE_FLAGS};
    unsafe {
        let mut buf = [0u16; 1024];
        if SystemParametersInfoW(SPI_GETDESKWALLPAPER, buf.len() as u32, Some(buf.as_mut_ptr() as *mut _), SYSTEM_PARAMETERS_INFO_UPDATE_FLAGS(0)).is_ok() {
            let _ = SystemParametersInfoW(SPI_SETDESKWALLPAPER, 0, Some(buf.as_mut_ptr() as *mut _), SPIF_UPDATEINIFILE);
        }
    }
}

/// Devices (\\.\DISPLAYn) whose desktop is completely hidden by a maximized / fullscreen window.
/// Skips minimized, invisible, cloaked (suspended UWP / other virtual desktop) and tool windows.
pub fn covered_monitors(own_pid: u32, mons: &[Monitor]) -> Vec<String> {
    use windows::Win32::Graphics::Dwm::{DwmGetWindowAttribute, DWMWA_CLOAKED};
    struct Ctx<'a> { pid: u32, mons: &'a [Monitor], out: Vec<String> }
    unsafe extern "system" fn cb(h: HWND, data: LPARAM) -> BOOL {
        let ctx = &mut *(data.0 as *mut Ctx);
        if !IsWindowVisible(h).as_bool() || IsIconic(h).as_bool() { return BOOL(1); }
        let ex = GetWindowLongPtrW(h, GWL_EXSTYLE) as u32;
        if ex & WS_EX_TOOLWINDOW.0 != 0 || ex & WS_EX_NOACTIVATE.0 != 0 { return BOOL(1); }
        let mut cloaked = 0u32;
        if DwmGetWindowAttribute(h, DWMWA_CLOAKED, &mut cloaked as *mut _ as *mut _, 4).is_ok() && cloaked != 0 { return BOOL(1); }
        let mut pid = 0u32;
        GetWindowThreadProcessId(h, Some(&mut pid));
        if pid == ctx.pid { return BOOL(1); }
        let mut cls = [0u16; 64];
        let n = GetClassNameW(h, &mut cls) as usize;
        if matches!(String::from_utf16_lossy(&cls[..n]).as_str(), "Progman" | "WorkerW" | "Shell_TrayWnd" | "Shell_SecondaryTrayWnd") { return BOOL(1); }
        let mut r = RECT::default();
        if GetWindowRect(h, &mut r).is_err() { return BOOL(1); }
        let zoomed = IsZoomed(h).as_bool();
        for m in ctx.mons {
            let covers = r.left <= m.rect.left && r.top <= m.rect.top && r.right >= m.rect.right && r.bottom >= m.rect.bottom;
            // A maximized window fills the work area of the monitor its centre is on.
            let (cx, cy) = ((r.left + r.right) / 2, (r.top + r.bottom) / 2);
            let on_it = cx >= m.rect.left && cx < m.rect.right && cy >= m.rect.top && cy < m.rect.bottom;
            if (covers || (zoomed && on_it)) && !ctx.out.contains(&m.device) { ctx.out.push(m.device.clone()); }
        }
        BOOL(1)
    }
    let mut ctx = Ctx { pid: own_pid, mons, out: Vec::new() };
    unsafe { let _ = EnumWindows(Some(cb), LPARAM(&mut ctx as *mut _ as isize)); }
    ctx.out
}

/// Rectangles of the monitors that have a truly fullscreen app (game / video / presentation) on
/// them. Windows only says "some fullscreen app runs"; the monitor is the one the foreground
/// window is on. Empty when nothing is fullscreen.
pub fn fullscreen_monitor_rects(own_pid: u32) -> Vec<RECT> {
    if !fullscreen_app_active(own_pid) { return Vec::new(); }
    unsafe {
        let fg = GetForegroundWindow();
        let mut r = RECT::default();
        if fg.is_invalid() || GetWindowRect(fg, &mut r).is_err() { return monitors().iter().map(|m| m.rect).collect(); }
        let (cx, cy) = ((r.left + r.right) / 2, (r.top + r.bottom) / 2);
        let hit: Vec<RECT> = monitors().iter().map(|m| m.rect)
            .filter(|m| cx >= m.left && cx < m.right && cy >= m.top && cy < m.bottom).collect();
        if hit.is_empty() { monitors().iter().map(|m| m.rect).collect() } else { hit }
    }
}

/// True if a game / video / presentation runs truly fullscreen (a merely maximized window doesn't count).
pub fn fullscreen_app_active(own_pid: u32) -> bool {
    use windows::Win32::UI::Shell::{SHQueryUserNotificationState, QUNS_BUSY, QUNS_PRESENTATION_MODE, QUNS_RUNNING_D3D_FULL_SCREEN};
    unsafe {
        let fg = GetForegroundWindow();
        // Maximized or captioned windows are normal windows, not fullscreen. This is checked first:
        // with the Windows taskbar hidden and no work area reserved (auto-hide dock), ANY maximized
        // window exactly fills its monitor and Windows then reports "busy" (fullscreen) for it, which
        // used to hide the dock on that monitor for as long as a browser stayed maximized there.
        let normal = IsZoomed(fg).as_bool() || (GetWindowLongPtrW(fg, GWL_STYLE) as u32 & WS_CAPTION.0) == WS_CAPTION.0;
        if let Ok(s) = SHQueryUserNotificationState() {
            if s == QUNS_RUNNING_D3D_FULL_SCREEN || s == QUNS_PRESENTATION_MODE { return true; }
            if s == QUNS_BUSY && !normal { return true; }
        }
        if normal { return false; }
        if fg.is_invalid() { return false; }
        let mut pid = 0u32;
        GetWindowThreadProcessId(fg, Some(&mut pid));
        if pid == own_pid { return false; }
        let mut cls = [0u16; 64];
        let n = GetClassNameW(fg, &mut cls) as usize;
        let cls = String::from_utf16_lossy(&cls[..n]);
        if matches!(cls.as_str(), "Progman" | "WorkerW" | "Shell_TrayWnd" | "Shell_SecondaryTrayWnd") { return false; }
        let mut r = RECT::default();
        if GetWindowRect(fg, &mut r).is_err() { return false; }
        monitors().iter().any(|m| r.left <= m.rect.left && r.top <= m.rect.top && r.right >= m.rect.right && r.bottom >= m.rect.bottom)
    }
}
