//! Menus: the Windows-style right-click menu of an item, the ^ flyout and background-task list,
//! dark popup menus.

use super::*;

/// Shows `menu` next to the dock item at `a` (above a bottom dock, beside a side dock) and returns
/// the chosen command id (0 = dismissed). Runs the modal loop with the dock borrow released.
/// True when Windows itself is shown in Vietnamese (the menu then speaks Vietnamese, otherwise
/// English — whatever the Windows taskbar menu would say).
pub(super) fn ui_is_vietnamese() -> bool {
    #[link(name = "kernel32")] extern "system" { fn GetUserDefaultUILanguage() -> u16; }
    unsafe { GetUserDefaultUILanguage() & 0x3FF == 0x2A }
}

/// 32-bit premultiplied DIB of `w`×`h` that we can draw into.
pub(super) unsafe fn new_argb(w: i32, h: i32) -> Option<(HBITMAP, *mut u32)> {
    let mut bi = BITMAPINFO::default();
    bi.bmiHeader = BITMAPINFOHEADER { biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32, biWidth: w, biHeight: -h, biPlanes: 1, biBitCount: 32, ..Default::default() };
    let mut bits: *mut std::ffi::c_void = std::ptr::null_mut();
    let bmp = CreateDIBSection(None, &bi, DIB_RGB_COLORS, &mut bits, None, 0).ok()?;
    (!bits.is_null()).then(|| (bmp, bits as *mut u32))
}

/// The 16 px glyph the Windows taskbar menu shows (Segoe Fluent Icons), white on transparent.
pub(super) unsafe fn glyph_bitmap(ch: char) -> Option<HBITMAP> {
    let size = 16;
    let (bmp, bits) = new_argb(size, size)?;
    let dc = CreateCompatibleDC(None);
    let old = SelectObject(dc, bmp);
    let font = CreateFontW(-15, 0, 0, 0, 400, 0, 0, 0, DEFAULT_CHARSET.0 as u32, OUT_TT_PRECIS.0 as u32, CLIP_DEFAULT_PRECIS.0 as u32,
        ANTIALIASED_QUALITY.0 as u32, (DEFAULT_PITCH.0 | FF_DONTCARE.0) as u32, &HSTRING::from("Segoe Fluent Icons"));
    let oldf = SelectObject(dc, font);
    SetBkMode(dc, TRANSPARENT);
    SetTextColor(dc, COLORREF(0x00FF_FFFF));
    let mut w = [ch as u16];
    let mut rc = RECT { left: 0, top: 0, right: size, bottom: size };
    DrawTextW(dc, &mut w, &mut rc, DT_SINGLELINE | DT_CENTER | DT_VCENTER | DT_NOPREFIX);
    let _ = GdiFlush();
    // GDI wrote opaque white into rgb with alpha 0: derive alpha from coverage, premultiply.
    let px = std::slice::from_raw_parts_mut(bits, (size * size) as usize);
    for p in px.iter_mut() {
        let a = ((*p >> 8) & 255).max((*p >> 16) & 255).max(*p & 255);
        *p = (a << 24) | (a << 16) | (a << 8) | a;
    }
    SelectObject(dc, oldf); let _ = DeleteObject(font);
    SelectObject(dc, old); let _ = DeleteDC(dc);
    Some(bmp)
}

/// Menu-sized (16 px) bitmap of an app icon (box-downscaled from the 96 px one).
pub(super) unsafe fn img_bitmap(img: &Img) -> Option<HBITMAP> {
    let size = 16;
    let (bmp, bits) = new_argb(size, size)?;
    let px = std::slice::from_raw_parts_mut(bits, (size * size) as usize);
    for y in 0..size { for x in 0..size {
        let (x0, x1) = (x * img.w / size, ((x + 1) * img.w / size).max(x * img.w / size + 1));
        let (y0, y1) = (y * img.h / size, ((y + 1) * img.h / size).max(y * img.h / size + 1));
        let (mut acc, mut n) = ([0u32; 4], 0u32);
        for yy in y0..y1.min(img.h) { for xx in x0..x1.min(img.w) {
            let p = img.px[(yy * img.w + xx) as usize];
            acc[0] += p >> 24; acc[1] += (p >> 16) & 255; acc[2] += (p >> 8) & 255; acc[3] += p & 255; n += 1;
        } }
        let n = n.max(1);
        px[(y * size + x) as usize] = ((acc[0] / n) << 24) | ((acc[1] / n) << 16) | ((acc[2] / n) << 8) | (acc[3] / n);
    } }
    Some(bmp)
}

pub(super) unsafe fn set_menu_bitmap(menu: HMENU, id: usize, bmp: Option<HBITMAP>, keep: &mut Vec<HBITMAP>) {
    let Some(b) = bmp else { return };
    let mi = MENUITEMINFOW { cbSize: std::mem::size_of::<MENUITEMINFOW>() as u32, fMask: MIIM_BITMAP, hbmpItem: b, ..Default::default() };
    let _ = SetMenuItemInfoW(menu, id as u32, false, &mi);
    keep.push(b);
}

pub(super) unsafe fn popup(h: HWND, menu: HMENU, a: &Anchor) -> usize {
    let gap = 10;
    let (x, y, align) = match a.edge {
        Edge::Bottom => (a.x, a.y - gap, TPM_CENTERALIGN | TPM_BOTTOMALIGN),
        Edge::Left => (a.x + gap, a.y, TPM_LEFTALIGN | TPM_VCENTERALIGN),
        Edge::Right => (a.x - gap, a.y, TPM_RIGHTALIGN | TPM_VCENTERALIGN),
    };
    with_dock_h(h, |d| d.menu_open = true);
    let _ = SetForegroundWindow(h);
    crate::watchdog::phase(crate::watchdog::DOCK, "idle"); // the user may leave the menu open as long as they like
    let cmd = TrackPopupMenu(menu, TPM_RETURNCMD | TPM_NONOTIFY | TPM_RIGHTBUTTON | align, x, y, 0, h, None).0 as usize;
    crate::watchdog::phase(crate::watchdog::DOCK, "dock:menu-after");
    let _ = PostMessageW(h, WM_NULL, WPARAM(0), LPARAM(0));
    let _ = DestroyMenu(menu);
    with_dock_h(h, |d| { d.menu_open = false; d.draw_content(); });
    cmd
}

pub(super) const ID_OPEN: usize = 1;
pub(super) const ID_PIN: usize = 2;
pub(super) const ID_UNPIN: usize = 3;
pub(super) const ID_CLOSE: usize = 4;
pub(super) const ID_KILL: usize = 5;
pub(super) const ID_VIRE: usize = 9;
pub(super) const ID_TASKMGR: usize = 10;
pub(super) const ID_DATETIME: usize = 20;
pub(super) const ID_SOUND: usize = 21;
pub(super) const ID_NETWORK: usize = 22;
pub(super) const ID_POWER: usize = 23;

/// Right-click menu: the app's own Windows shell menu, with the dock's pin / close / end-task
/// entries on top.
pub(super) unsafe fn item_context_menu(h: HWND, it: &Item, a: Anchor) {
    let menu = CreatePopupMenu().unwrap_or_default();
    let mut bitmaps: Vec<HBITMAP> = Vec::new();
    let add = |id: usize, text: &str| { let _ = AppendMenuW(menu, MF_STRING, id, &HSTRING::from(text)); };
    match it.kind {
        Kind::App => {
            // Same entries, same order, same wording and little icons as the Windows 11 taskbar:
            // [app] · Pin / Unpin · End task · Close window.
            let name = if it.pinned || it.exe.is_empty() { it.name.clone() } else { app_name(&it.exe) };
            let l = |vi: &'static str, en: &'static str| if ui_is_vietnamese() { vi } else { en };
            let app_icon = it.launch.clone().or_else(|| (!it.exe.is_empty()).then(|| it.exe.clone())).and_then(|s| load_icon(&s));
            if it.launch.is_some() {
                add(ID_OPEN, &name);
                if let Some(img) = &app_icon { set_menu_bitmap(menu, ID_OPEN, img_bitmap(img), &mut bitmaps); }
            }
            if it.pinned { add(ID_UNPIN, l("Bỏ ghim khỏi thanh tác vụ", "Unpin from taskbar")); set_menu_bitmap(menu, ID_UNPIN, glyph_bitmap('\u{E77A}'), &mut bitmaps); }
            else if it.launch.is_some() { add(ID_PIN, l("Ghim vào thanh tác vụ", "Pin to taskbar")); set_menu_bitmap(menu, ID_PIN, glyph_bitmap('\u{E718}'), &mut bitmaps); }
            if !it.wins.is_empty() {
                // No "End task" for Windows' own programs: ending File Explorer takes the whole
                // desktop and taskbar with it.
                if !item_is_system(it) { add(ID_KILL, l("Kết thúc tác vụ", "End task")); set_menu_bitmap(menu, ID_KILL, glyph_bitmap('\u{F140}'), &mut bitmaps); }
                add(ID_CLOSE, l(if it.wins.len() > 1 { "Đóng tất cả cửa sổ" } else { "Đóng cửa sổ" }, if it.wins.len() > 1 { "Close all windows" } else { "Close window" }));
                set_menu_bitmap(menu, ID_CLOSE, glyph_bitmap('\u{E711}'), &mut bitmaps);
            }
        }
        Kind::Start => { send_keys(&[VK_LWIN, VIRTUAL_KEY(b'X' as u16)]); let _ = DestroyMenu(menu); return; } // Windows' own Win+X menu
        Kind::Tray => { let _ = DestroyMenu(menu); background_menu(h, a); return; }
        Kind::Clock => { add(ID_DATETIME, "Chỉnh ngày và giờ"); add(ID_VIRE, "Cài đặt Vire"); }
        Kind::Control => { add(ID_SOUND, "Cài đặt âm thanh"); add(ID_NETWORK, "Cài đặt mạng & Internet"); add(ID_POWER, "Nguồn & pin"); add(ID_VIRE, "Cài đặt Vire"); }
        Kind::Sep => { let _ = DestroyMenu(menu); return; }
    }
    let cmd = popup(h, menu, &a);
    for b in bitmaps { let _ = DeleteObject(b); }
    match cmd {
        ID_OPEN => { if let Some(l) = &it.launch { launch(l, None); } }
        ID_PIN => { with_dock_h(h, |d| d.set_pinned(it, true)); }
        ID_UNPIN => { with_dock_h(h, |d| d.set_pinned(it, false)); }
        ID_CLOSE => { for w in &it.wins { let _ = PostMessageW(HWND(*w as *mut _), WM_CLOSE, WPARAM(0), LPARAM(0)); } }
        ID_KILL if !item_is_system(it) => {
            // Every process of this app: the windows' processes and all their children.
            let mut roots: Vec<u32> = it.wins.iter().map(|w| { let mut p = 0u32; GetWindowThreadProcessId(HWND(*w as *mut _), Some(&mut p)); p }).collect();
            roots.sort(); roots.dedup();
            let (open, _) = task_groups();
            let pids: Vec<u32> = open.iter().filter(|g| g.pids.iter().any(|p| roots.contains(p))).flat_map(|g| g.pids.clone()).chain(roots.clone()).collect();
            force_kill(pids.into_iter().filter(|p| !pid_is_system(*p)).collect());
        }
        ID_VIRE => crate::tray::request_open(),
        ID_DATETIME => launch("ms-settings:dateandtime", None),
        ID_SOUND => launch("ms-settings:sound", None),
        ID_NETWORK => launch("ms-settings:network", None),
        ID_POWER => launch("ms-settings:powersleep", None),
        _ => {}
    }
}

/// Left-click on "^": Windows' own "hidden icons" flyout (the real tray icons — click, right-click,
/// Exit… all work). Win+B puts keyboard focus in the notification area even while the real
/// taskbar is hidden; Enter then opens the flyout. We only press Enter once focus really is there
/// (never into your own app), then slide the flyout next to the dock's "^" button.
pub(super) unsafe fn open_tray_flyout(a: Anchor) {
    let prev = GetForegroundWindow().0 as isize; // HWND isn't Send; carry it as a number
    // Preferred: press the real taskbar's "show hidden icons" button (no keystrokes — nothing can
    // leak into a remote-desktop window in front). Fallback: Win+B, then Enter once focus is there.
    let pressed = tbmirror::request(tbmirror::TbAct::HiddenIcons);
    dbg(&format!("flyout: click, pressed the taskbar button = {pressed}"));
    if !pressed { send_keys(&[VK_LWIN, VIRTUAL_KEY(b'B' as u16)]); }
    std::thread::spawn(move || unsafe {
        let prev = HWND(prev as *mut _);
        let cls = |h: HWND| { let mut b = [0u16; 64]; let n = GetClassNameW(h, &mut b) as usize; String::from_utf16_lossy(&b[..n]) };
        if !pressed {
            let mut focused = false;
            for _ in 0..30 { // up to ~750 ms
                std::thread::sleep(Duration::from_millis(25));
                if cls(GetForegroundWindow()) == "Shell_TrayWnd" { focused = true; break; }
            }
            dbg(&format!("flyout: tray focus={focused} (foreground class now '{}')", cls(GetForegroundWindow())));
            if !focused { return; }
            std::thread::sleep(Duration::from_millis(120));
            send_keys(&[VK_RETURN]);
        }
        // Wait for the flyout, then place it.
        let find = || FindWindowW(w!("TopLevelWindowForOverflowXamlIsland"), PCWSTR::null()).ok().filter(|f| IsWindowVisible(*f).as_bool());
        let mut fly = None;
        for _ in 0..40 { std::thread::sleep(Duration::from_millis(25)); if let Some(f) = find() { fly = Some(f); break; } }
        dbg(&format!("flyout: window found={}", fly.is_some()));
        let Some(f) = fly else { return };
        let mut r = RECT::default();
        if GetWindowRect(f, &mut r).is_ok() {
            let (w, hgt) = (r.right - r.left, r.bottom - r.top);
            let gap = 12;
            let (mut x, mut y) = match a.edge {
                Edge::Bottom => (a.x - w / 2, a.y - hgt - gap),
                Edge::Left => (a.x + gap, a.y - hgt / 2),
                Edge::Right => (a.x - w - gap, a.y - hgt / 2),
            };
            x = x.max(a.mon.left + 8).min(a.mon.right - w - 8);
            y = y.max(a.mon.top + 8).min(a.mon.bottom - hgt - 8);
            let _ = SetWindowPos(f, None, x, y, 0, 0, SWP_NOSIZE | SWP_NOZORDER | SWP_NOACTIVATE);
        }
        // When the flyout closes, hand the keyboard back to the app you were using.
        while find().is_some() { std::thread::sleep(Duration::from_millis(150)); }
        std::thread::sleep(Duration::from_millis(120));
        // (SetForegroundWindow is refused for a background process; SwitchToThisWindow isn't.)
        if cls(GetForegroundWindow()) == "Shell_TrayWnd" && IsWindow(prev).as_bool() && cls(prev) != "Shell_TrayWnd" {
            SwitchToThisWindow(prev, BOOL(1));
            dbg("flyout: gave the keyboard back to the previous app");
        }
    });
}

/// "^" right-click: every task of this user — open apps and background tasks — click to end one.
pub(super) unsafe fn background_menu(h: HWND, a: Anchor) {
    let (open, bg) = task_groups();
    let menu = CreatePopupMenu().unwrap_or_default();
    let label = |a: &BgApp| if a.pids.len() > 1 { format!("{}   ({} tiến trình)", a.name, a.pids.len()) } else { a.name.clone() };
    let _ = AppendMenuW(menu, MF_STRING | MF_GRAYED, 0, w!("ĐANG MỞ  —  bấm để kết thúc"));
    for (i, a) in open.iter().enumerate() { let _ = AppendMenuW(menu, MF_STRING, 100 + i, &HSTRING::from(label(a))); }
    let _ = AppendMenuW(menu, MF_SEPARATOR, 0, PCWSTR::null());
    let _ = AppendMenuW(menu, MF_STRING | MF_GRAYED, 0, w!("CHẠY NỀN  —  bấm để kết thúc"));
    if bg.is_empty() { let _ = AppendMenuW(menu, MF_STRING | MF_GRAYED, 0, w!("(không có)")); }
    for (i, a) in bg.iter().enumerate() { let _ = AppendMenuW(menu, MF_STRING, 1000 + i, &HSTRING::from(label(a))); }
    let _ = AppendMenuW(menu, MF_SEPARATOR, 0, PCWSTR::null());
    let _ = AppendMenuW(menu, MF_STRING, ID_TASKMGR, w!("Mở Trình quản lý tác vụ"));
    let cmd = popup(h, menu, &a);
    match cmd {
        ID_TASKMGR => launch("taskmgr.exe", None),
        n if n >= 1000 && n - 1000 < bg.len() => { if confirm_end(h, &bg[n - 1000].name) { end_app(bg[n - 1000].pids.clone()) } }
        n if n >= 100 && n - 100 < open.len() => { if confirm_end(h, &open[n - 100].name) { end_app(open[n - 100].pids.clone()) } }
        _ => {}
    }
}

/// Makes this process's popup menus use the dark theme like the rest of Windows 11
/// (uxtheme ordinals 135 = SetPreferredAppMode, 136 = FlushMenuThemes).
pub fn enable_dark_menus() {
    use windows::Win32::System::LibraryLoader::LoadLibraryW;
    unsafe {
        let Ok(ux) = LoadLibraryW(w!("uxtheme.dll")) else { return };
        if let Some(p) = GetProcAddress(ux, windows::core::PCSTR(135usize as *const u8)) {
            let f: unsafe extern "system" fn(i32) -> i32 = std::mem::transmute(p);
            f(2); // ForceDark
        }
        if let Some(p) = GetProcAddress(ux, windows::core::PCSTR(136usize as *const u8)) {
            let f: unsafe extern "system" fn() = std::mem::transmute(p);
            f();
        }
    }
}
