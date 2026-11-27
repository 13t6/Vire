//! What a click on a dock item does, and pinning.

use super::*;

impl Dock {
    pub(super) unsafe fn click(&mut self, i: usize, middle: bool) {
        let it = self.items[i].clone();
        match it.kind {
            // The real taskbar's own buttons are pressed (see tbmirror); keystrokes are only the
            // fallback, and are never sent while a remote-desktop window is in front.
            Kind::Start => if !tbmirror::request(tbmirror::TbAct::Start) { send_keys(&[VK_LWIN]) },
            Kind::Clock => if !tbmirror::request(tbmirror::TbAct::Clock) { send_keys(&[VK_LWIN, VIRTUAL_KEY(b'N' as u16)]) },
            Kind::Control => if !tbmirror::request(tbmirror::TbAct::QuickSettings) { send_keys(&[VK_LWIN, VIRTUAL_KEY(b'A' as u16)]) },
            Kind::Sep | Kind::Tray => {}
            Kind::App => {
                dbg(&format!("click app '{}' wins={:?} middle={}", it.name, it.wins, middle));
                if middle || it.wins.is_empty() {
                    if let Some(l) = &it.launch { launch(l, None); }
                    return;
                }
                let fg = GetForegroundWindow().0 as isize;
                let wins: Vec<HWND> = it.wins.iter().map(|h| HWND(*h as *mut _)).collect();
                if let Some(pos) = it.wins.iter().position(|h| *h == fg) {
                    if wins.len() > 1 {
                        let next = wins[(pos + 1) % wins.len()];
                        if IsIconic(next).as_bool() { let _ = ShowWindow(next, SW_RESTORE); }
                        activate(next);
                    } else {
                        let _ = ShowWindow(wins[0], SW_MINIMIZE);
                    }
                } else {
                    let h = wins[0];
                    dbg(&format!("click app '{}' wins={:?} fg={} iconic={}", it.name, it.wins, fg, IsIconic(h).as_bool()));
                    if IsIconic(h).as_bool() { let _ = ShowWindow(h, SW_RESTORE); }
                    activate(h);
                    dbg(&format!("  after activate: fg={} target={}", GetForegroundWindow().0 as isize, h.0 as isize));
                }
            }
        }
    }

    pub(super) fn set_pinned(&mut self, it: &Item, pin: bool) {
        // Windows lets nobody but the user change the taskbar's own pins: an app pinned there is
        // only hidden from / shown on the dock; any other app is pinned on the dock alone.
        let on_taskbar = tbmirror::snapshot().map_or(false, |s| s.iter().any(|b| b.pinned && b.appid.eq_ignore_ascii_case(&it.key)));
        let mut st = self.app.lock().unwrap();
        if pin {
            st.settings.vd_unpinned.retain(|k| !k.eq_ignore_ascii_case(&it.key));
            let pins = &mut st.settings.vd_pins;
            if !on_taskbar && !pins.iter().any(|p| p.key.eq_ignore_ascii_case(&it.key)) {
                if let Some(l) = &it.launch { pins.push(Pin { key: it.key.clone(), launch: l.clone(), name: it.name.clone(), exe: (!it.exe.is_empty()).then(|| it.exe.clone()) }); }
            }
        } else {
            if on_taskbar && !st.settings.vd_unpinned.iter().any(|k| k.eq_ignore_ascii_case(&it.key)) { st.settings.vd_unpinned.push(it.key.clone()); }
            st.settings.vd_pins.retain(|p| !p.key.eq_ignore_ascii_case(&it.key));
        }
        self.cfg.pins = st.settings.vd_pins.clone();
        self.cfg.unpinned = st.settings.vd_unpinned.clone();
        st.save();
    }
}
