//! The dock thread: window procedures, the 500 ms poll, followers on extra monitors, take-over /
//! restore of the Windows taskbar, Explorer watchdog.

use super::*;

pub(super) unsafe extern "system" fn owner_proc(h: HWND, m: u32, wp: WPARAM, lp: LPARAM) -> LRESULT { DefWindowProcW(h, m, wp, lp) }

pub(super) unsafe extern "system" fn body_proc(h: HWND, m: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    match m {
        WM_NCHITTEST => LRESULT(-1), // let the content window take the input
        WM_PAINT => {
            // Black over an extended DWM frame = transparent: only the blur/acrylic shows.
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

/// The leader dock (main monitor).
pub(super) fn with_dock<T>(f: impl FnOnce(&mut Dock) -> T) -> Option<T> {
    DOCKS.with(|d| d.try_borrow_mut().ok().and_then(|mut g| g.first_mut().map(f)))
}

/// The dock that owns window `h` (its icons layer or its glass slab).
pub(super) fn with_dock_h<T>(h: HWND, f: impl FnOnce(&mut Dock) -> T) -> Option<T> {
    DOCKS.with(|d| d.try_borrow_mut().ok().and_then(|mut g| g.iter_mut().find(|k| k.content == h || k.body == h).map(f)))
}

pub(super) unsafe extern "system" fn content_proc(h: HWND, m: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    let (mx, my) = ((lp.0 & 0xFFFF) as i16 as f32, ((lp.0 >> 16) & 0xFFFF) as i16 as f32);
    match m {
        WM_MOUSEACTIVATE => return LRESULT(MA_NOACTIVATE as isize),
        WM_MOUSEMOVE => {
            with_dock_h(h, |d| {
                if d.hover.is_none() {
                    let mut tme = TRACKMOUSEEVENT { cbSize: std::mem::size_of::<TRACKMOUSEEVENT>() as u32, dwFlags: TME_LEAVE, hwndTrack: h, dwHoverTime: 0 };
                    let _ = TrackMouseEvent(&mut tme);
                }
                let prev = d.hover_local().and_then(|(x, y)| d.hit(x, y));
                let (ox, oy, _, _) = d.content_rect(d.reveal);
                d.hover = Some((mx + ox as f32, my + oy as f32));
                d.leave_at = None;
                if d.cfg.magnify { d.start_anim(); } else if d.hit(mx, my) != prev { d.draw_content(); }
            });
            return LRESULT(0);
        }
        WM_MOUSE_LEAVE => {
            with_dock_h(h, |d| { d.hover = None; d.start_anim(); if !d.cfg.magnify { d.draw_content(); } });
            return LRESULT(0);
        }
        WM_LBUTTONUP | WM_MBUTTONUP => {
            preview::dismiss();
            // Anything that opens a menu / waits runs outside the dock borrow.
            let target = with_dock_h(h, |d| d.hit(mx, my).map(|i| (i, d.items[i].kind, d.anchor(i)))).flatten();
            dbg(&format!("mouse up at ({mx},{my}) msg={m:#x} -> {:?}", target.map(|t| (t.0, t.1))));
            match target {
                Some((_, Kind::Tray, a)) => { dbg("click on ^"); open_tray_flyout(a) }
                Some((i, _, _)) => { with_dock_h(h, |d| d.click(i, m == WM_MBUTTONUP)); }
                None => {}
            }
            return LRESULT(0);
        }
        WM_RBUTTONUP => {
            preview::dismiss();
            let Some((it, a)) = with_dock_h(h, |d| d.hit(mx, my).map(|i| (d.items[i].clone(), d.anchor(i)))).flatten() else { return LRESULT(0) };
            item_context_menu(h, &it, a);
            return LRESULT(0);
        }
        WM_TIMER => {
            match wp.0 {
                T_ANIM => {
                    with_dock_h(h, |d| {
                        let moving = d.animate();
                        if d.shown {
                            // Length (magnification) and position (auto-hide) both move the slab.
                            d.draw_body();
                            d.draw_content();
                        }
                        if !moving { d.anim = false; let _ = KillTimer(d.content, T_ANIM); }
                    });
                }
                T_POLL => poll_all(),
                _ => {}
            }
            return LRESULT(0);
        }
        WM_APPBAR => {
            with_dock_h(h, |d| match wp.0 as u32 {
                ABN_POSCHANGED => d.update_appbar(),
                ABN_FULLSCREENAPP if !IGNORE_FULLSCREEN.load(Ordering::Relaxed) => { d.fullscreen = lp.0 != 0; let v = !d.fullscreen; d.set_visible(v); }
                _ => {}
            });
            return LRESULT(0);
        }
        WM_DISPLAYCHANGE | WM_DPICHANGED | WM_SETTINGCHANGE => {
            with_dock_h(h, |d| { d.layout(); d.update_appbar(); d.body_drawn = None; d.redraw_all(); });
        }
        _ => {}
    }
    DefWindowProcW(h, m, wp, lp)
}

/// Applies the latest requested config, then enforces the invariants:
/// enabled ⇒ we own the bar (taskbar hidden, dock shown unless a fullscreen app runs);
/// disabled ⇒ dock windows hidden, taskbar given back.
pub(super) unsafe fn reconcile(d: &mut Dock) {
    let want = d.shared.as_ref().and_then(|s| s.desired.lock().unwrap().take());
    if let Some(cfg) = want { apply_cfg(d, cfg); }
    if d.cfg.enabled && !d.active { takeover(d); }
    if !d.cfg.enabled && d.active { restore(d); }
    if !d.active {
        for h in [d.body, d.content] { if IsWindowVisible(h).as_bool() { let _ = ShowWindow(h, SW_HIDE); } }
        d.shown = false;
    }
}

pub(super) unsafe fn poll(d: &mut Dock) {
    reconcile(d);
    poll_common(d, true);
}

/// Every 100 ms (from the leader's timer): the leader, then each follower dock.
pub(super) unsafe fn poll_all() {
    crate::watchdog::phase(crate::watchdog::DOCK, "dock:poll");
    poll_all_inner();
    crate::watchdog::phase(crate::watchdog::DOCK, "idle");
}

unsafe fn poll_all_inner() {
    explorer_watchdog();
    DOCKS.with(|ds| {
        let Ok(mut v) = ds.try_borrow_mut() else { return };
        if v.is_empty() { return; }
        if check_quit(&mut v[0]) { teardown_followers(&mut v); return; }
        poll(&mut v[0]);
        let (active, cfg, tick) = (v[0].active, v[0].cfg.clone(), v[0].tick);
        if tick % 10 == 0 || (!active && v.len() > 1) { sync_followers(&mut v, active); }
        for f in v.iter_mut().skip(1) { follower_poll(f, active, &cfg); }
    });
    // Window previews: what app icon (with open windows) is the cursor resting on?
    let hover = DOCKS.with(|ds| {
        let v = ds.try_borrow().ok()?;
        for d in v.iter() {
            if !d.active || !d.shown || d.menu_open { continue; }
            let Some((x, y)) = d.hover_local() else { continue };
            let i = d.hit(x, y)?;
            let it = &d.items[i];
            return (it.kind == Kind::App && !it.wins.is_empty()).then(|| preview::Hover { key: it.key.clone(), wins: it.wins.clone(), anchor: d.anchor(i) });
        }
        None
    });
    preview::tick(hover);
}

/// Safety net: if File Explorer is gone (ended by mistake, crashed) there is no desktop and no
/// taskbar any more. After ~4 s without its windows, start it again (retried every 30 s).
pub(super) unsafe fn explorer_watchdog() {
    static MISSING: AtomicU32 = AtomicU32::new(0);
    let alive = FindWindowW(w!("Shell_TrayWnd"), PCWSTR::null()).is_ok() || FindWindowW(w!("Progman"), PCWSTR::null()).is_ok();
    if alive { MISSING.store(0, Ordering::Relaxed); return; }
    let n = MISSING.fetch_add(1, Ordering::Relaxed) + 1;
    if n == 8 {
        dbg("explorer is gone -> starting it again");
        launch("explorer.exe", None);
    }
    if n >= 60 { MISSING.store(0, Ordering::Relaxed); }
}

/// Keeps exactly one follower dock per extra monitor (and none while the dock is off).
pub(super) unsafe fn sync_followers(v: &mut Vec<Dock>, active: bool) {
    let want: Vec<RECT> = if active { crate::desktop::monitors().into_iter().filter(|m| !m.primary).map(|m| m.rect).collect() } else { Vec::new() };
    let have: Vec<RECT> = v.iter().skip(1).filter_map(|d| d.mon_rect).collect();
    if want == have { return; }
    dbg(&format!("followers: want {} have {}", want.len(), have.len()));
    teardown_followers(v);
    let (owner, hinst) = DOCK_ENV.with(|e| e.get());
    let app = v[0].app.clone();
    for rect in want {
        let (body, content) = create_dock_windows(HWND(owner as *mut _), windows::Win32::Foundation::HINSTANCE(hinst as *mut _));
        dbg(&format!("followers: created windows invalid={}", body.is_invalid() || content.is_invalid()));
        if body.is_invalid() || content.is_invalid() { continue; }
        let mut f = new_dock(app.clone(), None, body, content);
        f.mon_rect = Some(rect);
        v.push(f);
    }
}

pub(super) unsafe fn teardown_followers(v: &mut Vec<Dock>) {
    while v.len() > 1 {
        let mut f = v.pop().unwrap();
        let _ = KillTimer(f.content, T_ANIM);
        if f.appbar { let mut ab = appbar_data(f.content); SHAppBarMessage(ABM_REMOVE, &mut ab); f.appbar = false; }
        f.content_surf.free();
        let _ = DestroyWindow(f.content);
        let _ = DestroyWindow(f.body);
    }
}

/// A follower copies the leader's settings and runs the same per-tick work on its own monitor.
pub(super) unsafe fn follower_poll(f: &mut Dock, active: bool, cfg: &VdCfg) {
    if !active { return; }
    if !f.active {
        f.cfg = cfg.clone();
        f.active = true;
        f.reveal = if cfg.autohide { 0.0 } else { 1.0 };
        f.reveal_target = f.reveal;
        f.scales.clear();
        f.refresh_items(GetCurrentProcessId());
        f.layout();
        f.update_appbar();
        f.set_visible(!f.fullscreen);
    } else if f.cfg != *cfg {
        apply_cfg(f, cfg.clone());
    }
    poll_common(f, false);
}

pub(super) unsafe fn poll_common(d: &mut Dock, leader: bool) {
    if !d.active || d.menu_open { return; }
    d.tick = d.tick.wrapping_add(1);
    let own_pid = GetCurrentProcessId();
    // Every 500 ms: app list, clock, keep the Windows taskbar hidden, fullscreen apps.
    // Icons loaded off-thread by now? Show them.
    let mut got_icons = false;
    while let Ok((k, img)) = d.icon_rx.try_recv() {
        d.icon_wait.remove(&k);
        if img.is_some() { d.icons.insert(k, img); got_icons = true; }
        else { let e = d.icon_tries.entry(k).or_insert((0, Instant::now())); e.0 += 1; e.1 = Instant::now() + Duration::from_secs(2); } // try again soon
    }
    if got_icons { d.redraw_all(); }
    if d.tick % 5 == 0 {
        let changed = d.refresh_items(own_pid);
        crate::watchdog::phase(crate::watchdog::DOCK, "dock:taskbars");
        if leader { show_taskbars(false); }
        crate::watchdog::phase(crate::watchdog::DOCK, "dock:fullscreen-check");
        // Only the dock on the monitor that has the fullscreen app hides; the others stay.
        let fs = !IGNORE_FULLSCREEN.load(Ordering::Relaxed) && {
            let (m, cx, cy) = (d.geo.mon, (d.geo.mon.left + d.geo.mon.right) / 2, (d.geo.mon.top + d.geo.mon.bottom) / 2);
            let _ = m;
            crate::desktop::fullscreen_monitor_rects(own_pid).iter().any(|r| cx >= r.left && cx < r.right && cy >= r.top && cy < r.bottom)
        };
        if fs != d.fullscreen || d.shown == fs { d.fullscreen = fs; d.set_visible(!fs); }
        if changed { d.update_appbar(); d.body_drawn = None; d.redraw_all(); }
    }
    // Auto-hide: reveal when the cursor touches the dock edge, hide shortly after it leaves.
    if d.cfg.autohide && d.shown {
        let mut p = POINT::default();
        let _ = GetCursorPos(&mut p);
        let (cx, cy, cw, ch) = d.content_rect(1.0);
        let m = d.geo.mon;
        let at_edge = match d.cfg.edge {
            Edge::Bottom => p.y >= m.bottom - 2 && p.x >= cx && p.x < cx + cw,
            Edge::Left => p.x <= m.left + 1 && p.y >= cy && p.y < cy + ch,
            Edge::Right => p.x >= m.right - 2 && p.y >= cy && p.y < cy + ch,
        };
        // "Inside" = the slab plus room for magnified icons (not the wide transparent margin).
        let (bx, by, bw, bh) = d.body_rect(1.0);
        let grow = (d.geo.max_icon - d.geo.icon + 8.0 * d.geo.s) as i32;
        // …extended all the way to the screen edge, so resting the mouse in the gap below the
        // dock doesn't count as "left" (that made it hide/show in a loop).
        let (ix, iy, iw, ih) = match d.cfg.edge {
            Edge::Bottom => (bx, by - grow, bw, m.bottom - (by - grow)),
            Edge::Left => (m.left, by, bx + bw + grow - m.left, bh),
            Edge::Right => (bx - grow, by, m.right - (bx - grow), bh),
        };
        let inside = p.x >= ix && p.x < ix + iw && p.y >= iy && p.y < iy + ih;
        if at_edge && d.reveal_target < 1.0 { d.reveal_target = 1.0; d.start_anim(); }
        if !inside && d.reveal_target > 0.0 {
            match d.leave_at {
                None => d.leave_at = Some(Instant::now()),
                Some(t) if t.elapsed() > Duration::from_millis(450) => { d.reveal_target = 0.0; d.leave_at = None; d.start_anim(); }
                _ => {}
            }
        } else if inside { d.leave_at = None; }
    }
}

/// Config change while (possibly) running: re-layout, re-dock to the new edge, redraw.
pub(super) unsafe fn apply_cfg(d: &mut Dock, cfg: VdCfg) {
    let edge_changed = d.cfg.edge != cfg.edge;
    d.cfg = cfg;
    if !d.cfg.enabled || !d.active { return; }
    if edge_changed && d.appbar {
        // Re-register so Windows recomputes the work area for the new edge.
        let mut ab = appbar_data(d.content);
        SHAppBarMessage(ABM_REMOVE, &mut ab);
        d.appbar = false;
    }
    if !d.cfg.autohide { d.reveal = 1.0; d.reveal_target = 1.0; }
    d.scales.clear();
    d.hover = None;
    d.refresh_items(GetCurrentProcessId());
    d.layout();
    d.update_appbar();
    d.body_drawn = None;
    d.set_visible(!d.fullscreen);
}

/// Take over from the Windows taskbar (its auto-hide frees the work area; we also hide it).
pub(super) unsafe fn takeover(d: &mut Dock) {
    {
        let mut st = d.app.lock().unwrap();
        if st.settings.vd_prev_autohide.is_none() { st.settings.vd_prev_autohide = Some(taskbar_autohide()); }
        // First run: bring over the apps pinned on the Windows taskbar.
        if !st.settings.vd_pins_imported {
            st.settings.vd_pins = import_taskbar_pins();
            st.settings.vd_pins_imported = true;
            d.cfg.pins = st.settings.vd_pins.clone();
        }
        st.save();
    }
    set_taskbar_autohide(true);
    show_taskbars(false);
    tbmirror::start();
    d.active = true;
    d.reveal = if d.cfg.autohide { 0.0 } else { 1.0 };
    d.reveal_target = d.reveal;
    d.scales.clear();
    d.refresh_items(GetCurrentProcessId());
    d.layout();
    d.update_appbar();
    d.set_visible(true);
}

/// Give everything back. Idempotent.
pub(super) unsafe fn restore(d: &mut Dock) {
    let _ = KillTimer(d.content, T_ANIM);
    d.anim = false;
    d.set_visible(false);
    if d.appbar { let mut ab = appbar_data(d.content); SHAppBarMessage(ABM_REMOVE, &mut ab); d.appbar = false; }
    show_taskbars(true);
    let prev = { let mut st = d.app.lock().unwrap(); let p = st.settings.vd_prev_autohide.take(); st.save(); p };
    if let Some(p) = prev { set_taskbar_autohide(p); }
    d.active = false;
}

/// Last-resort: called from the panic hook so a crash never leaves the user without a taskbar.
pub fn emergency_restore() {
    unsafe { show_taskbars(true); }
}

/// Cleans up whatever an older Vire build (or a crash) may have left on the Windows taskbar.
pub(super) unsafe fn startup_cleanup(app: &Arc<Mutex<AppState>>) {
    for h in taskbars() {
        SetWindowRgn(h, HRGN::default(), true);
        let ex = GetWindowLongPtrW(h, GWL_EXSTYLE);
        if ex & WS_EX_LAYERED.0 as isize != 0 { SetWindowLongPtrW(h, GWL_EXSTYLE, ex & !(WS_EX_LAYERED.0 as isize)); }
    }
    let mut st = app.lock().unwrap();
    if !st.settings.vd_enabled {
        show_taskbars(true);
        if let Some(p) = st.settings.vd_prev_autohide.take() { set_taskbar_autohide(p); st.save(); }
    }
}

pub(super) unsafe fn new_dock(app: Arc<Mutex<AppState>>, shared: Option<Arc<Shared>>, body: HWND, content: HWND) -> Dock {
    let cfg = VdCfg { enabled: false, edge: Edge::Bottom, autohide: false, magnify: true, clock: true, style: 0, material: Material::Acrylic, pins: vec![], unpinned: vec![] };
    let (icon_tx, icon_rx) = channel();
    Dock {
        cfg, app, body, content, content_surf: Surface::new(), shared, active: false, items: vec![], icons: HashMap::new(), icon_tx, icon_rx, icon_wait: std::collections::HashSet::new(), icon_tries: HashMap::new(), win_keys: HashMap::new(), pin_exe: HashMap::new(), order: Vec::new(),
        hover: None, scales: vec![], reveal: 1.0, reveal_target: 1.0, leave_at: None, appbar: false, tick: 0, clock: (String::new(), String::new()),
        fullscreen: false, shown: false, geo: Geo { s: 1.0, slot: 56.0, icon: 44.0, max_icon: 68.0, thick: 62.0, refl_gap: 0.0, refl_len: 0.0, over: 64.0, pad: 0.0, margin: 8.0, len: 100.0, lens: vec![], mon: RECT::default() },
        anim: false, menu_open: false, body_drawn: None, mon_rect: None, light: false, luma: 0.0, luma_set: false,
    }
}

/// Handles a pending quit request: give the taskbar back, answer, end the thread's loop.
pub(super) unsafe fn check_quit(d: &mut Dock) -> bool {
    let reply = d.shared.as_ref().and_then(|s| s.quit.lock().unwrap().take());
    if let Some(r) = reply {
        if d.active { restore(d); }
        let _ = r.send(());
        PostQuitMessage(0);
        return true;
    }
    false
}

pub(super) unsafe fn thread_main(shared: Arc<Shared>, app: Arc<Mutex<AppState>>, tid: Arc<AtomicU32>) {
    let _ = CoInitializeEx(None, COINIT_APARTMENTTHREADED);
    tid.store(GetCurrentThreadId(), Ordering::SeqCst);
    startup_cleanup(&app);
    let hinst = GetModuleHandleW(None).unwrap_or_default();
    let classes: [(PCWSTR, WNDPROC, HBRUSH); 3] = [
        (w!("VireDockOwner"), Some(owner_proc), HBRUSH::default()),
        (w!("VireDockBody"), Some(body_proc), HBRUSH(GetStockObject(BLACK_BRUSH).0)),
        (w!("VireDockContent"), Some(content_proc), HBRUSH::default()),
    ];
    for (cls, proc_, brush) in classes {
        let wc = WNDCLASSW { lpfnWndProc: proc_, hInstance: hinst.into(), lpszClassName: cls, hbrBackground: brush, hCursor: LoadCursorW(None, IDC_ARROW).unwrap_or_default(), ..Default::default() };
        RegisterClassW(&wc);
    }
    // Never-shown owner: owned windows stay out of Alt+Tab and the taskbar without needing
    // WS_EX_TOOLWINDOW (which would switch the blur off).
    let owner = CreateWindowExW(WS_EX_TOOLWINDOW, w!("VireDockOwner"), w!(""), WS_POPUP, 0, 0, 0, 0, None, None, hinst, None).unwrap_or_default();
    DOCK_ENV.with(|e| e.set((owner.0 as isize, hinst.0 as isize)));
    let (body, content) = create_dock_windows(owner, hinst.into());

    let mut dock = new_dock(app, Some(shared), body, content);
    dock.layout();
    DOCKS.with(|d| d.borrow_mut().push(dock));
    with_dock(|d| reconcile(d));
    SetTimer(content, T_POLL, 100, None);

    let mut msg = MSG::default();
    while GetMessageW(&mut msg, None, 0, 0).as_bool() {
        if msg.hwnd.is_invalid() && msg.message == WM_WAKE {
            with_dock(|d| { if !check_quit(d) { reconcile(d); } });
            continue;
        }
        let _ = TranslateMessage(&msg);
        DispatchMessageW(&msg);
    }
    DOCKS.with(|d| for k in d.borrow_mut().iter_mut() { k.content_surf.free(); });
}

/// One dock's two windows: the glass slab and the icons layer above it.
pub(super) unsafe fn create_dock_windows(owner: HWND, hinst: windows::Win32::Foundation::HINSTANCE) -> (HWND, HWND) {
    // Glass slab: a plain, non-layered, non-tool window → DWM blurs behind it and clips the blur
    // to the rounded window region. Black client over an extended frame = transparent.
    let body = CreateWindowExW(WS_EX_TOPMOST | WS_EX_NOACTIVATE | WS_EX_TRANSPARENT, w!("VireDockBody"), w!(""), WS_POPUP, 0, 0, 1, 1, owner, None, hinst, None).unwrap_or_default();
    let m = windows::Win32::UI::Controls::MARGINS { cxLeftWidth: -1, cxRightWidth: -1, cyTopHeight: -1, cyBottomHeight: -1 };
    let _ = windows::Win32::Graphics::Dwm::DwmExtendFrameIntoClientArea(body, &m);
    {
        use windows::Win32::Graphics::Dwm::{DwmSetWindowAttribute, DWMWA_BORDER_COLOR, DWMWA_WINDOW_CORNER_PREFERENCE, DWMWCP_DONOTROUND};
        // Corners are ours (blur region + drawn rim); no grey system border either.
        let pref = DWMWCP_DONOTROUND;
        let _ = DwmSetWindowAttribute(body, DWMWA_WINDOW_CORNER_PREFERENCE, &pref as *const _ as *const _, 4);
        let none: u32 = 0xFFFF_FFFE; // DWMWA_COLOR_NONE
        let _ = DwmSetWindowAttribute(body, DWMWA_BORDER_COLOR, &none as *const _ as *const _, 4);
    }
    // Icons layer: owned by the slab, so it always stays above it.
    let content = CreateWindowExW(WS_EX_TOOLWINDOW | WS_EX_TOPMOST | WS_EX_NOACTIVATE | WS_EX_LAYERED, w!("VireDockContent"), w!("Vire Dock"), WS_POPUP, 0, 0, 1, 1, body, None, hinst, None).unwrap_or_default();
    (body, content)
}
