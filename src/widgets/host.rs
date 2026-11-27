//! The desktop side of the widgets: one small layered window per visible instance, sitting just
//! above the desktop (under every normal window). Handles drawing on a schedule, dragging,
//! Ctrl+wheel resizing, the right-click menu, multi-monitor placement and the audio lease.
//!
//! Everything runs on one thread with one render timer that always sleeps until the next widget is
//! due (a clock: once a second / minute; a visualizer: ~30 fps only while there is sound).

use super::*;
use crate::raster::Canvas;
use crate::vdock::Surface;
use std::cell::RefCell;
use std::time::{Duration, Instant};

use windows::core::{w, PCWSTR};
use windows::Win32::Foundation::{COLORREF, HWND, LPARAM, LRESULT, POINT, RECT, WPARAM};
use windows::Win32::Graphics::Dwm::{DwmEnableBlurBehindWindow, DWM_BLURBEHIND};
use windows::Win32::Graphics::Gdi::{BeginPaint, CreateRoundRectRgn, DeleteObject, EndPaint, FillRect, GetStockObject, BLACK_BRUSH, HBRUSH, PAINTSTRUCT};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::HiDpi::GetDpiForSystem;
use windows::Win32::UI::Input::KeyboardAndMouse::{ReleaseCapture, SetCapture};
use windows::Win32::UI::WindowsAndMessaging::*;

/// Thread message: the dashboard changed something, re-read the layout.
pub(super) const WM_HUB: u32 = WM_APP + 7;
const T_RENDER: usize = 1;
const T_SLOW: usize = 2;
const T_Z: usize = 3;

struct Win {
    id: u32,
    hwnd: HWND,
    /// Frosted-glass window right below `hwnd` (only while the widget wants blur).
    body: HWND,
    surf: Surface,
    canvas: Canvas,
    widget: Box<dyn Widget>,
    cfg: Value,
    cfg_json: String,
    /// Physical position / size on screen.
    pos: (i32, i32),
    size: (i32, i32),
    monitor: String,
    locked: bool,
    next: Instant,
    last_draw: Instant,
    lease: Option<crate::audio::Lease>,
    drag: Option<(i32, i32)>,
    force: bool,
    blur_key: Option<(i32, i32, i32)>,
}

struct Host {
    hub: Arc<Hub>,
    engine: crate::engine::Engine,
    hwnd: HWND,
    wins: Vec<Win>,
    ver: u64,
    covered: Vec<String>,
    covered_at: Instant,
    builtin_playing: bool,
}

thread_local! { static HOST: RefCell<Option<Host>> = const { RefCell::new(None) }; }

fn with_host<T>(f: impl FnOnce(&mut Host) -> T) -> Option<T> {
    HOST.with(|h| h.try_borrow_mut().ok().and_then(|mut b| b.as_mut().map(f)))
}

pub fn start(hub: Arc<Hub>, engine: crate::engine::Engine) {
    let _ = std::thread::Builder::new().name("vire-widgets".into()).spawn(move || unsafe { thread_main(hub, engine) });
}

fn dpi_scale() -> f32 { (unsafe { GetDpiForSystem() } as f32 / 96.0).max(1.0) }

unsafe fn thread_main(hub: Arc<Hub>, engine: crate::engine::Engine) {
    let hinst = GetModuleHandleW(None).unwrap_or_default();
    for (name, proc) in [(w!("VireWidgetHost"), host_proc as unsafe extern "system" fn(HWND, u32, WPARAM, LPARAM) -> LRESULT), (w!("VireDeskWidget"), widget_proc), (w!("VireDeskBlur"), blur_proc)] {
        let wc = WNDCLASSW { lpfnWndProc: Some(proc), hInstance: hinst.into(), lpszClassName: name, hCursor: LoadCursorW(None, IDC_ARROW).unwrap_or_default(), ..Default::default() };
        RegisterClassW(&wc);
    }
    crate::vdock::dbg("widgets: host thread started");
    let hwnd = match CreateWindowExW(WINDOW_EX_STYLE(0), w!("VireWidgetHost"), w!("Vire Widgets"), WS_POPUP, 0, 0, 0, 0, HWND_MESSAGE, None, hinst, None) { Ok(h) => h, Err(e) => { crate::vdock::dbg(&format!("widgets: host window failed: {e}")); return; } };
    hub.set_host_thread(windows::Win32::System::Threading::GetCurrentThreadId());
    HOST.with(|h| *h.borrow_mut() = Some(Host { hub, engine, hwnd, wins: Vec::new(), ver: 0, covered: Vec::new(), covered_at: Instant::now() - Duration::from_secs(5), builtin_playing: false }));
    SetTimer(hwnd, T_SLOW, 1000, None);
    SetTimer(hwnd, T_Z, 250, None); // keep the widgets above the desktop (Explorer re-raises it when apps open or close)
    with_host(|h| reconcile(h));
    let mut msg = MSG::default();
    while GetMessageW(&mut msg, None, 0, 0).as_bool() {
        if msg.message == WM_HUB { with_host(|h| reconcile(h)); continue; }
        let _ = TranslateMessage(&msg);
        DispatchMessageW(&msg);
    }
}

// ------------------------------------------------------------------ layout → windows

unsafe fn monitor_rect(device: &str) -> RECT {
    let mons = crate::desktop::monitors();
    mons.iter().find(|m| m.device == device).or_else(|| mons.iter().find(|m| m.primary)).or(mons.first()).map(|m| m.rect)
        .unwrap_or(RECT { left: 0, top: 0, right: 1920, bottom: 1080 })
}

/// Creates / updates / destroys windows so they match the active profile.
unsafe fn reconcile(h: &mut Host) {
    crate::watchdog::phase(crate::watchdog::WIDGETS, "widgets:reconcile");
    reconcile_inner(h);
    crate::watchdog::phase(crate::watchdog::WIDGETS, "idle");
}

unsafe fn reconcile_inner(h: &mut Host) {
    h.ver = h.hub.version();
    let (instances, capture) = { let st = h.hub.lock(); (st.profile().instances.clone(), st.capture) };
    let scale = dpi_scale();
    let hinst = GetModuleHandleW(None).unwrap_or_default();
    crate::vdock::dbg(&format!("widgets: reconcile {} instance(s), capture={}, windows={}", instances.len(), capture, h.wins.len()));

    // Gone or hidden → close.
    let keep: Vec<u32> = instances.iter().filter(|i| i.visible && kind(&i.kind).is_some()).map(|i| i.id).collect();
    h.wins.retain_mut(|w| {
        if keep.contains(&w.id) { return true; }
        let _ = DestroyWindow(w.hwnd);
        if !w.body.is_invalid() { let _ = DestroyWindow(w.body); }
        false
    });

    for inst in instances.iter().filter(|i| keep.contains(&i.id)) {
        let Some(k) = kind(&inst.kind) else { continue };
        let size = (((inst.w.max(k.min_size.0)) as f32 * scale).round() as i32, ((inst.h.max(k.min_size.1)) as f32 * scale).round() as i32);
        let mr = monitor_rect(&inst.monitor);
        let mut pos = (mr.left + (inst.x as f32 * scale) as i32, mr.top + (inst.y as f32 * scale) as i32);
        pos.0 = pos.0.clamp(mr.left - size.0 / 2, (mr.right - 40).max(mr.left));
        pos.1 = pos.1.clamp(mr.top, (mr.bottom - 40).max(mr.top));
        let cfg_json = inst.settings.to_string();

        if !h.wins.iter().any(|w| w.id == inst.id) {
            let hwnd = CreateWindowExW(WS_EX_LAYERED | WS_EX_TOOLWINDOW | WS_EX_NOACTIVATE, w!("VireDeskWidget"), PCWSTR::null(), WS_POPUP, pos.0, pos.1, size.0, size.1, None, None, hinst, None).unwrap_or_default();
            crate::vdock::dbg(&format!("widgets: create window id={} kind={} at {:?} size {:?} monitor={:?} (mon rect {},{},{},{})", inst.id, inst.kind, pos, size, inst.monitor, mr.left, mr.top, mr.right, mr.bottom));
            if hwnd.is_invalid() { continue; }
            SetWindowLongPtrW(hwnd, GWLP_USERDATA, inst.id as isize);
            h.wins.push(Win {
                id: inst.id, hwnd, body: HWND::default(), surf: Surface::new(), canvas: Canvas::new(size.0, size.1), widget: (k.create)(),
                cfg: inst.settings.clone(), cfg_json: cfg_json.clone(), pos, size, monitor: inst.monitor.clone(), locked: inst.locked,
                next: Instant::now(), last_draw: Instant::now(), lease: None, drag: None, force: true, blur_key: None,
            });
        }
        let w = h.wins.iter_mut().find(|w| w.id == inst.id).unwrap();
        w.locked = inst.locked;
        w.monitor = inst.monitor.clone();
        if w.cfg_json != cfg_json { w.cfg = inst.settings.clone(); w.cfg_json = cfg_json; w.force = true; }
        if w.size != size { w.size = size; w.canvas = Canvas::new(size.0, size.1); w.force = true; }
        if w.drag.is_none() && w.pos != pos { w.pos = pos; let _ = SetWindowPos(w.hwnd, None, pos.0, pos.1, 0, 0, SWP_NOSIZE | SWP_NOZORDER | SWP_NOACTIVATE); w.force = true; }
        // Audio only while a visible widget reads it and capture is allowed.
        let want = capture && w.widget.uses_audio(&w.cfg);
        if want && w.lease.is_none() { w.lease = Some(crate::audio::acquire()); }
        if !want && w.lease.is_some() { w.lease = None; }
        // Frosted-glass window behind.
        let blur = w.widget.blur_radius(&w.cfg);
        match (blur, w.body.is_invalid()) {
            (Some(_), true) => {
                let b = CreateWindowExW(WS_EX_TOOLWINDOW | WS_EX_NOACTIVATE, w!("VireDeskBlur"), PCWSTR::null(), WS_POPUP, w.pos.0, w.pos.1, w.size.0, w.size.1, None, None, hinst, None).unwrap_or_default();
                w.body = b; w.blur_key = None;
            }
            (None, false) => { let _ = DestroyWindow(w.body); w.body = HWND::default(); w.blur_key = None; }
            _ => {}
        }
        if let (Some(r), false) = (blur, w.body.is_invalid()) {
            let key = (w.size.0, w.size.1, (r * scale) as i32);
            if w.blur_key != Some(key) {
                let _ = SetWindowPos(w.body, None, w.pos.0, w.pos.1, w.size.0, w.size.1, SWP_NOZORDER | SWP_NOACTIVATE);
                let d = (r * scale * 2.0) as i32;
                let rgn = CreateRoundRectRgn(0, 0, w.size.0 + 1, w.size.1 + 1, d, d);
                let bb = DWM_BLURBEHIND { dwFlags: 3, fEnable: windows::Win32::Foundation::BOOL(1), hRgnBlur: rgn, fTransitionOnMaximized: windows::Win32::Foundation::BOOL(0) };
                let _ = DwmEnableBlurBehindWindow(w.body, &bb);
                let _ = DeleteObject(rgn);
                w.blur_key = Some(key);
            }
        }
        w.next = Instant::now();
    }
    // Hide unconfigured leftovers, keep order stable, re-render everything that changed.
    on_timer(h, true);
}

// ------------------------------------------------------------------ drawing schedule

unsafe fn on_timer(h: &mut Host, show_new: bool) {
    crate::watchdog::phase(crate::watchdog::WIDGETS, "widgets:render");
    on_timer_inner(h, show_new);
    crate::watchdog::phase(crate::watchdog::WIDGETS, "idle");
}

unsafe fn on_timer_inner(h: &mut Host, show_new: bool) {
    let now_i = Instant::now();
    if h.covered_at.elapsed() > Duration::from_millis(700) {
        h.covered_at = Instant::now();
        let mons = crate::desktop::monitors();
        h.covered = crate::desktop::covered_monitors(std::process::id(), &mons);
    }
    let scale = dpi_scale();
    let wall = now_local();
    let any_audio = h.wins.iter().any(|w| w.lease.is_some());
    if any_audio { h.builtin_playing = h.engine.player_view().playing; }
    let (frame, _seq) = if any_audio { crate::audio::snapshot() } else { (crate::dsp::Frame::default(), 0) };
    let live = crate::audio::has_signal();
    let mons_primary = crate::desktop::monitors().into_iter().find(|m| m.primary).map(|m| m.device).unwrap_or_default();
    let covered = h.covered.clone();

    for w in h.wins.iter_mut() {
        if w.drag.is_some() { continue; }
        if !w.force && w.next > now_i { continue; }
        let device = if w.monitor.is_empty() { &mons_primary } else { &w.monitor };
        if !w.force && covered.iter().any(|d| d == device) { w.next = now_i + Duration::from_millis(500); continue; }
        let dt = w.last_draw.elapsed().as_secs_f32().min(0.2);
        w.last_draw = Instant::now();
        let audio_on = w.lease.is_some();
        let ctx = Ctx { now: wall, dt, scale, audio: audio_on.then_some(&frame), signal: live, builtin_playing: h.builtin_playing, preview: false };
        w.canvas.px.fill(0);
        let drew = w.widget.render(&ctx, &w.cfg, &mut w.canvas);
        if drew || w.force {
            if drew {
                // A near-invisible film keeps the whole widget clickable / draggable, not only its glyphs.
                if !w.locked { for p in w.canvas.px.iter_mut() { if *p >> 24 == 0 { *p = 0x01000000; } } }
                w.surf.present(w.hwnd, &w.canvas, w.pos.0, w.pos.1);
            }
            if !IsWindowVisible(w.hwnd).as_bool() {
                crate::vdock::dbg(&format!("widgets: show window id={} drew={} pos={:?}", w.id, drew, w.pos));
                if !w.body.is_invalid() { let _ = ShowWindow(w.body, SW_SHOWNOACTIVATE); }
                let _ = ShowWindow(w.hwnd, SW_SHOWNOACTIVATE);
                z_place(w);
            }
        }
        w.force = false;
        w.next = now_i + Duration::from_millis(w.widget.interval_ms(&w.cfg, &wall, live && audio_on).max(10) as u64);
    }
    let _ = show_new;
    reschedule(h);
}

unsafe fn reschedule(h: &mut Host) {
    let now = Instant::now();
    let ms = h.wins.iter().map(|w| w.next.saturating_duration_since(now).as_millis() as u32).min().unwrap_or(1000).clamp(5, 5000);
    SetTimer(h.hwnd, T_RENDER, ms, None);
}

/// Just above the desktop, under every normal window; the blur window right below its widget.
unsafe fn z_place(w: &Win) {
    crate::widget::place_above_desktop_n(w.hwnd, 40);
    if !w.body.is_invalid() { let _ = SetWindowPos(w.body, w.hwnd, 0, 0, 0, 0, SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE); }
}

unsafe extern "system" fn host_proc(hwnd: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    match msg {
        WM_TIMER => {
            match wp.0 {
                T_RENDER => { with_host(|h| on_timer(h, false)); }
                T_SLOW => with_host(|h| {
                    h.hub.flush(false);
                    if h.hub.version() != h.ver { reconcile(h); }
                }).unwrap_or(()),
                // Cheap (returns at once while a widget is already in place), so it can run often:
                // when an app opens or closes Explorer re-raises the desktop and a widget would
                // otherwise stay hidden behind it until the next pass.
                T_Z => with_host(|h| {
                    // Self-healing: the windows must always match the widgets that are switched on
                    // (a switch-on that was missed — a swallowed wake-up, a busy moment — used to
                    // need a restart of Vire), and a window that is not on screen gets shown again.
                    let mut want: Vec<u32> = { let st = h.hub.lock(); st.profile().instances.iter().filter(|i| i.visible && kind(&i.kind).is_some()).map(|i| i.id).collect() };
                    h.wins.retain(|w| IsWindow(w.hwnd).as_bool()); // a window that is gone is no window
                    let mut have: Vec<u32> = h.wins.iter().map(|w| w.id).collect();
                    want.sort(); have.sort();
                    if want != have || h.hub.version() != h.ver {
                        crate::vdock::dbg(&format!("widgets: out of step (want {want:?}, have {have:?}) -> reconcile"));
                        reconcile(h);
                    }
                    let mut redraw = false;
                    for w in h.wins.iter_mut() {
                        if !IsWindowVisible(w.hwnd).as_bool() { w.force = true; w.next = Instant::now(); redraw = true; }
                    }
                    if redraw { on_timer(h, false); }
                    for w in &h.wins { z_place(w); }
                }).unwrap_or(()),
                _ => {}
            }
            LRESULT(0)
        }
        _ => DefWindowProcW(hwnd, msg, wp, lp),
    }
}

unsafe extern "system" fn blur_proc(h: HWND, m: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    match m {
        WM_NCHITTEST => LRESULT(-1),
        WM_PAINT => {
            let mut ps = PAINTSTRUCT::default();
            let dc = BeginPaint(h, &mut ps);
            FillRect(dc, &ps.rcPaint, HBRUSH(GetStockObject(BLACK_BRUSH).0));
            let _ = EndPaint(h, &ps);
            LRESULT(0)
        }
        WM_ERASEBKGND => LRESULT(1),
        _ => DefWindowProcW(h, m, wp, lp),
    }
}

// ------------------------------------------------------------------ the widget windows

unsafe fn id_of(h: HWND) -> u32 { GetWindowLongPtrW(h, GWLP_USERDATA) as u32 }

unsafe extern "system" fn widget_proc(h: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    match msg {
        WM_MOUSEACTIVATE => return LRESULT(MA_NOACTIVATE as isize),
        WM_NCHITTEST => {
            // Locked widgets don't take the mouse at all: clicks go to the desktop under them.
            let locked = with_host(|ho| ho.wins.iter().find(|w| w.hwnd == h).map_or(false, |w| w.locked)).unwrap_or(false);
            if locked { return LRESULT(-1); }
        }
        WM_SETCURSOR => { let _ = SetCursor(LoadCursorW(None, IDC_SIZEALL).unwrap_or_default()); return LRESULT(1); }
        WM_LBUTTONDOWN => {
            let (x, y) = ((lp.0 & 0xFFFF) as i16 as i32, ((lp.0 >> 16) & 0xFFFF) as i16 as i32);
            with_host(|ho| if let Some(w) = ho.wins.iter_mut().find(|w| w.hwnd == h) { w.drag = Some((x, y)); });
            SetCapture(h);
            return LRESULT(0);
        }
        WM_MOUSEMOVE => {
            let mut p = POINT::default();
            let _ = GetCursorPos(&mut p);
            with_host(|ho| if let Some(w) = ho.wins.iter_mut().find(|w| w.hwnd == h) {
                if let Some((dx, dy)) = w.drag {
                    w.pos = (p.x - dx, p.y - dy);
                    let _ = SetWindowPos(w.hwnd, None, w.pos.0, w.pos.1, 0, 0, SWP_NOSIZE | SWP_NOZORDER | SWP_NOACTIVATE);
                    if !w.body.is_invalid() { let _ = SetWindowPos(w.body, None, w.pos.0, w.pos.1, 0, 0, SWP_NOSIZE | SWP_NOZORDER | SWP_NOACTIVATE); }
                }
            });
            return LRESULT(0);
        }
        WM_LBUTTONUP => {
            let _ = ReleaseCapture();
            let saved = with_host(|ho| {
                let w = ho.wins.iter_mut().find(|w| w.hwnd == h)?;
                w.drag.take()?;
                Some((w.id, w.pos, w.size))
            }).flatten();
            if let Some((id, pos, size)) = saved { save_position(id, pos, size); }
            return LRESULT(0);
        }
        WM_MOUSEWHEEL => {
            // Ctrl + wheel resizes the widget by 6 % per notch.
            if (wp.0 & 0xFFFF) as u32 & 0x0008 != 0 {
                let delta = ((wp.0 >> 16) & 0xFFFF) as i16 as i32;
                let k = if delta > 0 { 1.06 } else { 1.0 / 1.06 };
                let id = id_of(h);
                if let Some(hub) = with_host(|ho| ho.hub.clone()) {
                    hub.with_mut(|st| {
                        let Some(inst) = st.instance_mut(id) else { return false };
                        let min = kind(&inst.kind).map_or((120, 60), |k| k.min_size);
                        inst.w = ((inst.w as f32 * k) as i32).clamp(min.0, 2400);
                        inst.h = ((inst.h as f32 * k) as i32).clamp(min.1, 1400);
                        true
                    });
                }
            }
            return LRESULT(0);
        }
        WM_RBUTTONUP => { widget_menu(h); return LRESULT(0); }
        WM_DISPLAYCHANGE => { with_host(|ho| reconcile(ho)); }
        _ => {}
    }
    DefWindowProcW(h, msg, wp, lp)
}

/// Saves where a dragged widget ended up (monitor + offset in logical px).
unsafe fn save_position(id: u32, pos: (i32, i32), size: (i32, i32)) {
    let Some(hub) = with_host(|ho| ho.hub.clone()) else { return };
    let scale = dpi_scale();
    let (cx, cy) = (pos.0 + size.0 / 2, pos.1 + size.1 / 2);
    let mons = crate::desktop::monitors();
    let m = mons.iter().find(|m| cx >= m.rect.left && cx < m.rect.right && cy >= m.rect.top && cy < m.rect.bottom).or_else(|| mons.iter().find(|m| m.primary));
    let Some(m) = m else { return };
    hub.with_mut(|st| {
        let Some(inst) = st.instance_mut(id) else { return false };
        inst.monitor = m.device.clone();
        inst.x = ((pos.0 - m.rect.left) as f32 / scale).round() as i32;
        inst.y = ((pos.1 - m.rect.top) as f32 / scale).round() as i32;
        true
    });
    // The window is already where the user dropped it: sync our own state to the new version.
    with_host(|ho| ho.ver = ho.hub.version());
    hub.flush(true);
}

unsafe fn widget_menu(h: HWND) {
    let id = id_of(h);
    let Some((hub, locked)) = with_host(|ho| (ho.hub.clone(), ho.wins.iter().find(|w| w.id == id).map_or(false, |w| w.locked))) else { return };
    let vi = ui_vietnamese();
    let l = |v: &'static str, e: &'static str| if vi { v } else { e };
    let menu = CreatePopupMenu().unwrap_or_default();
    let add = |cmd: usize, text: &str| { let _ = AppendMenuW(menu, MF_STRING, cmd, &windows::core::HSTRING::from(text)); };
    add(1, l("Tuỳ chỉnh…", "Customize…"));
    add(2, l("Nhân đôi", "Duplicate"));
    add(3, if locked { l("Mở khoá vị trí", "Unlock position") } else { l("Khoá vị trí", "Lock position") });
    let _ = AppendMenuW(menu, MF_SEPARATOR, 0, PCWSTR::null());
    add(4, l("Ẩn widget", "Hide widget"));
    add(5, l("Xoá widget", "Remove widget"));
    let mut p = POINT::default();
    let _ = GetCursorPos(&mut p);
    let _ = SetForegroundWindow(h);
    let cmd = TrackPopupMenu(menu, TPM_RETURNCMD | TPM_NONOTIFY | TPM_RIGHTBUTTON, p.x, p.y, 0, h, None).0 as usize;
    let _ = PostMessageW(h, WM_NULL, WPARAM(0), LPARAM(0));
    let _ = DestroyMenu(menu);
    match cmd {
        1 => { hub.request_edit(id); crate::tray::request_open(); }
        2 => { hub.with_mut(|st| st.duplicate(id).is_some()); }
        3 => { hub.with_mut(|st| { if let Some(i) = st.instance_mut(id) { i.locked = !i.locked; true } else { false } }); }
        4 => { hub.with_mut(|st| { if let Some(i) = st.instance_mut(id) { i.visible = false; true } else { false } }); }
        5 => { hub.with_mut(|st| { st.remove(id); true }); }
        _ => {}
    }
    let _ = COLORREF(0);
}
