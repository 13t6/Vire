//! Live previews of an app's windows above its dock icon, like the taskbar's thumbnails. The
//! pictures are drawn by DWM itself (each window is registered as a thumbnail source): Vire copies
//! no pixels and keeps nothing in memory. Click a preview to switch to that window, middle-click
//! to close it.

use super::*;
use windows::Win32::Graphics::Dwm::{
    DwmQueryThumbnailSourceSize, DwmRegisterThumbnail, DwmSetWindowAttribute, DwmUnregisterThumbnail, DwmUpdateThumbnailProperties,
    DWMWA_USE_IMMERSIVE_DARK_MODE, DWMWA_WINDOW_CORNER_PREFERENCE, DWMWCP_ROUND, DWM_THUMBNAIL_PROPERTIES, DWM_TNP_OPACITY,
    DWM_TNP_RECTDESTINATION, DWM_TNP_SOURCECLIENTAREAONLY, DWM_TNP_VISIBLE,
};

/// What the cursor rests on: an app icon with open windows.
pub(super) struct Hover { pub key: String, pub wins: Vec<isize>, pub anchor: Anchor }

struct Cell { src: HWND, thumb: isize, rect: RECT, title: String, minimized: bool }

struct Pv {
    hwnd: HWND,
    key: String,
    wins: Vec<isize>,
    cells: Vec<Cell>,
    hot: Option<usize>,
    /// The pointer is on the close button of this cell.
    hot_x: Option<usize>,
    hide_at: Option<Instant>,
    dwell: Option<(String, Instant)>,
    /// Item just clicked: no preview for it until the cursor moves to another icon.
    suppress: Option<String>,
    font: HFONT,
    icon_font: HFONT,
    s: f32,
}

thread_local! { static PV: RefCell<Option<Pv>> = const { RefCell::new(None) }; }

const BG: (u8, u8, u8) = (32, 32, 32);
const HOT: (u8, u8, u8) = (55, 55, 55);
fn rgb(c: (u8, u8, u8)) -> COLORREF { COLORREF(c.0 as u32 | (c.1 as u32) << 8 | (c.2 as u32) << 16) }

unsafe fn ensure() {
    PV.with(|p| {
        if p.borrow().is_some() { return; }
        let hinst = GetModuleHandleW(None).unwrap_or_default();
        let wc = WNDCLASSW { lpfnWndProc: Some(proc_), hInstance: hinst.into(), lpszClassName: w!("VireWinPreview"), hbrBackground: CreateSolidBrush(rgb(BG)), hCursor: LoadCursorW(None, IDC_HAND).unwrap_or_default(), ..Default::default() };
        RegisterClassW(&wc);
        let owner = HWND(DOCK_ENV.with(|e| e.get()).0 as *mut _);
        let Ok(hwnd) = CreateWindowExW(WS_EX_TOOLWINDOW | WS_EX_NOACTIVATE | WS_EX_TOPMOST, w!("VireWinPreview"), w!(""), WS_POPUP, 0, 0, 10, 10, owner, None, hinst, None) else { return };
        let dark: u32 = 1;
        let _ = DwmSetWindowAttribute(hwnd, DWMWA_USE_IMMERSIVE_DARK_MODE, &dark as *const _ as *const _, 4);
        let round = DWMWCP_ROUND;
        let _ = DwmSetWindowAttribute(hwnd, DWMWA_WINDOW_CORNER_PREFERENCE, &round as *const _ as *const _, 4);
        let s = GetDpiForSystem() as f32 / 96.0;
        let font = CreateFontW(-(13.0 * s) as i32, 0, 0, 0, 600, 0, 0, 0, DEFAULT_CHARSET.0 as u32, OUT_TT_PRECIS.0 as u32,
            CLIP_DEFAULT_PRECIS.0 as u32, CLEARTYPE_QUALITY.0 as u32, (DEFAULT_PITCH.0 | FF_SWISS.0) as u32, &HSTRING::from("Segoe UI"));
        // Windows' own icon font (Segoe Fluent Icons on 11; Segoe MDL2 Assets is the same glyph set).
        let icon_font = CreateFontW(-(11.0 * s) as i32, 0, 0, 0, 400, 0, 0, 0, DEFAULT_CHARSET.0 as u32, OUT_TT_PRECIS.0 as u32,
            CLIP_DEFAULT_PRECIS.0 as u32, CLEARTYPE_QUALITY.0 as u32, (DEFAULT_PITCH.0 | FF_SWISS.0) as u32, &HSTRING::from("Segoe Fluent Icons"));
        *p.borrow_mut() = Some(Pv { hwnd, key: String::new(), wins: vec![], cells: vec![], hot: None, hot_x: None, hide_at: None, dwell: None, suppress: None, font, icon_font, s });
    });
}

unsafe fn hide(pv: &mut Pv) {
    for c in pv.cells.drain(..) { if c.thumb != 0 { let _ = DwmUnregisterThumbnail(c.thumb); } }
    if IsWindowVisible(pv.hwnd).as_bool() { let _ = ShowWindow(pv.hwnd, SW_HIDE); }
    pv.key.clear();
    pv.wins.clear();
    pv.hot = None;
    pv.hot_x = None;
    pv.hide_at = None;
}

unsafe fn show(pv: &mut Pv, h: &Hover) {
    for c in pv.cells.drain(..) { if c.thumb != 0 { let _ = DwmUnregisterThumbnail(c.thumb); } }
    let wins: Vec<HWND> = h.wins.iter().map(|w| HWND(*w as *mut _)).filter(|w| IsWindow(*w).as_bool()).take(6).collect();
    if wins.is_empty() { hide(pv); return; }
    let s = pv.s;
    let (cw, th, tt, pad) = ((232.0 * s) as i32, (132.0 * s) as i32, (30.0 * s) as i32, (8.0 * s) as i32);
    let n = wins.len() as i32;
    let (w, hh) = (pad + n * (cw + pad), pad + tt + th + pad);
    let a = h.anchor;
    let gap = (12.0 * s) as i32;
    let (mut x, mut y) = match a.edge {
        Edge::Bottom => (a.x - w / 2, a.y - hh - gap),
        Edge::Left => (a.x + gap, a.y - hh / 2),
        Edge::Right => (a.x - w - gap, a.y - hh / 2),
    };
    x = x.max(a.mon.left + 4).min(a.mon.right - w - 4);
    y = y.max(a.mon.top + 4).min(a.mon.bottom - hh - 4);
    let _ = SetWindowPos(pv.hwnd, HWND_TOPMOST, x, y, w, hh, SWP_NOACTIVATE | SWP_SHOWWINDOW);
    for (i, src) in wins.into_iter().enumerate() {
        let left = pad + i as i32 * (cw + pad);
        let rect = RECT { left, top: pad, right: left + cw, bottom: pad + tt + th };
        let minimized = IsIconic(src).as_bool();
        let thumb = DwmRegisterThumbnail(pv.hwnd, src).unwrap_or(0);
        if thumb != 0 {
            let inner = RECT { left: rect.left + (6.0 * s) as i32, top: rect.top + tt, right: rect.right - (6.0 * s) as i32, bottom: rect.bottom - (6.0 * s) as i32 };
            let (bw, bh) = ((inner.right - inner.left) as f32, (inner.bottom - inner.top) as f32);
            let dst = match DwmQueryThumbnailSourceSize(thumb) {
                Ok(sz) if sz.cx > 0 && sz.cy > 0 => {
                    let k = (bw / sz.cx as f32).min(bh / sz.cy as f32);
                    let (dw, dh) = ((sz.cx as f32 * k) as i32, (sz.cy as f32 * k) as i32);
                    let (dx, dy) = (inner.left + (bw as i32 - dw) / 2, inner.top + (bh as i32 - dh) / 2);
                    RECT { left: dx, top: dy, right: dx + dw, bottom: dy + dh }
                }
                _ => inner,
            };
            let props = DWM_THUMBNAIL_PROPERTIES {
                dwFlags: DWM_TNP_RECTDESTINATION | DWM_TNP_VISIBLE | DWM_TNP_SOURCECLIENTAREAONLY | DWM_TNP_OPACITY,
                rcDestination: dst, opacity: 255, fVisible: BOOL(1), fSourceClientAreaOnly: BOOL(0), ..Default::default()
            };
            let _ = DwmUpdateThumbnailProperties(thumb, &props);
        }
        pv.cells.push(Cell { src, thumb, rect, title: window_title(src), minimized });
    }
    pv.key = h.key.clone();
    pv.wins = h.wins.clone();
    pv.hot = None;
    pv.hide_at = None;
    let _ = InvalidateRect(pv.hwnd, None, true);
}

/// Called every 100 ms by the dock with what the cursor is on.
pub(super) unsafe fn tick(hover: Option<Hover>) {
    ensure();
    PV.with(|p| {
        let Ok(mut g) = p.try_borrow_mut() else { return };
        let Some(pv) = g.as_mut() else { return };
        let visible = IsWindowVisible(pv.hwnd).as_bool();
        match hover {
            Some(h) => {
                if pv.suppress.as_deref().map_or(false, |k| k != h.key) { pv.suppress = None; }
                if pv.suppress.as_deref() == Some(h.key.as_str()) { return; }
                if visible && pv.key == h.key {
                    pv.hide_at = None;
                    if pv.wins != h.wins { show(pv, &h); } // a window opened / closed meanwhile
                    return;
                }
                // Rest on an icon a moment (quicker when a preview is already open: sliding along).
                let wait = Duration::from_millis(if visible { 120 } else { 450 });
                match &pv.dwell {
                    Some((k, t)) if *k == h.key => if t.elapsed() >= wait { show(pv, &h); },
                    _ => pv.dwell = Some((h.key.clone(), Instant::now())),
                }
            }
            None => {
                pv.dwell = None;
                pv.suppress = None;
                if !visible { return; }
                let mut pt = POINT::default();
                let _ = GetCursorPos(&mut pt);
                let mut r = RECT::default();
                let _ = GetWindowRect(pv.hwnd, &mut r);
                let inside = pt.x >= r.left - 4 && pt.x < r.right + 4 && pt.y >= r.top - 4 && pt.y < r.bottom + 16;
                if inside { pv.hide_at = None; return; }
                match pv.hide_at {
                    None => pv.hide_at = Some(Instant::now() + Duration::from_millis(300)),
                    Some(t) if Instant::now() >= t => hide(pv),
                    _ => {}
                }
            }
        }
    });
}

/// The icon was clicked (or a menu opened): close the preview, don't reopen it for this icon.
pub(super) fn dismiss() {
    PV.with(|p| {
        let Ok(mut g) = p.try_borrow_mut() else { return };
        if let Some(pv) = g.as_mut() {
            let k = if pv.key.is_empty() { pv.dwell.as_ref().map(|d| d.0.clone()) } else { Some(pv.key.clone()) };
            unsafe { hide(pv); }
            pv.suppress = k;
            pv.dwell = None;
        }
    });
}

/// The round "x" button in the top-right corner of a preview.
fn close_rect(c: &Cell, s: f32) -> RECT {
    let (d, m) = ((26.0 * s) as i32, (2.0 * s) as i32);
    RECT { left: c.rect.right - m - d, top: c.rect.top + m, right: c.rect.right - m, bottom: c.rect.top + m + d }
}
fn in_rect(r: &RECT, x: i32, y: i32) -> bool { x >= r.left && x < r.right && y >= r.top && y < r.bottom }

fn cell_at(pv: &Pv, x: i32, y: i32) -> Option<usize> {
    pv.cells.iter().position(|c| x >= c.rect.left && x < c.rect.right && y >= c.rect.top && y < c.rect.bottom)
}

unsafe extern "system" fn proc_(h: HWND, m: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    let (mx, my) = ((lp.0 & 0xFFFF) as i16 as i32, ((lp.0 >> 16) & 0xFFFF) as i16 as i32);
    match m {
        WM_MOUSEACTIVATE => LRESULT(MA_NOACTIVATE as isize),
        WM_MOUSEMOVE => {
            PV.with(|p| if let Ok(mut g) = p.try_borrow_mut() {
                if let Some(pv) = g.as_mut() {
                    let hot = cell_at(pv, mx, my);
                    let hot_x = hot.filter(|&i| in_rect(&close_rect(&pv.cells[i], pv.s), mx, my));
                    if hot != pv.hot || hot_x != pv.hot_x { pv.hot = hot; pv.hot_x = hot_x; let _ = InvalidateRect(h, None, true); }
                    pv.hide_at = None;
                }
            });
            LRESULT(0)
        }
        WM_LBUTTONUP | WM_MBUTTONUP => {
            let target = PV.with(|p| p.try_borrow().ok().and_then(|g| g.as_ref().and_then(|pv| cell_at(pv, mx, my).map(|i| (pv.cells[i].src, in_rect(&close_rect(&pv.cells[i], pv.s), mx, my))))));
            if let Some((src, on_x)) = target {
                // The round "x" (or a middle click) closes just that window; the preview stays open
                // for the others and refreshes by itself.
                if on_x && m == WM_LBUTTONUP { let _ = PostMessageW(src, WM_CLOSE, WPARAM(0), LPARAM(0)); return LRESULT(0); }
                if m == WM_MBUTTONUP { let _ = PostMessageW(src, WM_CLOSE, WPARAM(0), LPARAM(0)); return LRESULT(0); }
                dismiss();
                if IsIconic(src).as_bool() { let _ = ShowWindow(src, SW_RESTORE); }
                activate(src);
            }
            LRESULT(0)
        }
        WM_PAINT => {
            let mut ps = PAINTSTRUCT::default();
            let dc = BeginPaint(h, &mut ps);
            PV.with(|p| if let Ok(g) = p.try_borrow() {
                if let Some(pv) = g.as_ref() {
                    let old = SelectObject(dc, pv.font);
                    SetBkMode(dc, TRANSPARENT);
                    let r8 = (8.0 * pv.s) as i32;
                    for (i, c) in pv.cells.iter().enumerate() {
                        if pv.hot == Some(i) {
                            let b = CreateSolidBrush(rgb(HOT));
                            let ob = SelectObject(dc, b);
                            let op = SelectObject(dc, GetStockObject(NULL_PEN));
                            let _ = RoundRect(dc, c.rect.left, c.rect.top, c.rect.right, c.rect.bottom, r8 * 2, r8 * 2);
                            SelectObject(dc, op); SelectObject(dc, ob);
                            let _ = DeleteObject(b);
                        }
                        SetTextColor(dc, COLORREF(0x00F5F5F5));
                        // The window's own small icon, then its title (as in the Windows 11 flyout).
                        let isz = (16.0 * pv.s) as i32;
                        let mut ic = HICON(std::ptr::null_mut());
                        let mut res = 0usize;
                        let _ = SendMessageTimeoutW(c.src, WM_GETICON, WPARAM(2 /*ICON_SMALL2*/), LPARAM(0), SMTO_ABORTIFHUNG, 80, Some(&mut res));
                        if res == 0 { let _ = SendMessageTimeoutW(c.src, WM_GETICON, WPARAM(0), LPARAM(0), SMTO_ABORTIFHUNG, 80, Some(&mut res)); }
                        if res != 0 { ic = HICON(res as *mut _); } else {
                            let v = GetClassLongPtrW(c.src, GCLP_HICONSM); if v != 0 { ic = HICON(v as *mut _); }
                        }
                        let tx = if !ic.is_invalid() {
                            let _ = DrawIconEx(dc, c.rect.left + r8, c.rect.top + ((30.0 * pv.s) as i32 - isz) / 2, ic, isz, isz, 0, None, DI_NORMAL);
                            c.rect.left + r8 + isz + (8.0 * pv.s) as i32
                        } else { c.rect.left + r8 };
                        let mut tr = RECT { left: tx, top: c.rect.top, right: c.rect.right - r8 - (30.0 * pv.s) as i32, bottom: c.rect.top + (30.0 * pv.s) as i32 };
                        let mut t: Vec<u16> = c.title.encode_utf16().collect();
                        if !t.is_empty() { DrawTextW(dc, &mut t, &mut tr, DT_SINGLELINE | DT_VCENTER | DT_END_ELLIPSIS | DT_NOPREFIX); }
                        // Close button for this window: a faint circle, red under the pointer.
                        // Close button: Windows' own glyph (Segoe Fluent Icons "ChromeClose"), a rounded
                        // square that turns Windows red under the pointer.
                        if pv.hot == Some(i) {
                            let cr = close_rect(c, pv.s);
                            let on_x = pv.hot_x == Some(i);
                            if on_x {
                                let b = CreateSolidBrush(rgb((196, 43, 28)));
                                let ob = SelectObject(dc, b);
                                let op = SelectObject(dc, GetStockObject(NULL_PEN));
                                let _ = RoundRect(dc, cr.left, cr.top, cr.right, cr.bottom, (8.0 * pv.s) as i32, (8.0 * pv.s) as i32);
                                SelectObject(dc, op); SelectObject(dc, ob);
                                let _ = DeleteObject(b);
                            }
                            let of = SelectObject(dc, pv.icon_font);
                            SetTextColor(dc, COLORREF(0x00FFFFFF));
                            let mut xr = cr;
                            let mut xt: Vec<u16> = "\u{E711}".encode_utf16().collect();
                            DrawTextW(dc, &mut xt, &mut xr, DT_SINGLELINE | DT_VCENTER | DT_CENTER | DT_NOPREFIX);
                            SelectObject(dc, of);
                            SetTextColor(dc, COLORREF(0x00F5F5F5));
                        }
                        if c.minimized || c.thumb == 0 {
                            SetTextColor(dc, COLORREF(0x00A0A0A0));
                            let mut mr = RECT { left: c.rect.left, top: tr.bottom, right: c.rect.right, bottom: c.rect.bottom };
                            let mut t: Vec<u16> = "Đã thu nhỏ".encode_utf16().collect();
                            DrawTextW(dc, &mut t, &mut mr, DT_SINGLELINE | DT_VCENTER | DT_CENTER | DT_NOPREFIX);
                        }
                    }
                    SelectObject(dc, old);
                }
            });
            let _ = EndPaint(h, &ps);
            LRESULT(0)
        }
        _ => DefWindowProcW(h, m, wp, lp),
    }
}
