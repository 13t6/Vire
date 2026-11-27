//! Dock geometry, hit-testing and the list of items (running apps + pins + system buttons).

use super::*;

/// Panic-free clamp (f32::clamp aborts when lo > hi, e.g. a tooltip wider than its window).
pub(super) fn clampf(v: f32, lo: f32, hi: f32) -> f32 { if hi < lo { lo } else { v.max(lo).min(hi) } }

pub(super) fn weekday_vi(d: u16) -> &'static str { ["CN", "T2", "T3", "T4", "T5", "T6", "T7"][(d % 7) as usize] }

pub(super) unsafe fn now_clock() -> (String, String) {
    let t = windows::Win32::System::SystemInformation::GetLocalTime();
    (format!("{:02}:{:02}", t.wHour, t.wMinute), format!("{} {}/{}", weekday_vi(t.wDayOfWeek), t.wDay, t.wMonth))
}

impl Dock {
    pub(super) fn vertical(&self) -> bool { self.cfg.edge != Edge::Bottom }

    pub(super) fn item_len(&self, it: &Item, s: f32) -> f32 {
        match it.kind {
            Kind::Sep => 14.0 * s,
            Kind::Clock => if self.vertical() { 56.0 * s } else { 66.0 * s },
            _ => 56.0 * s,
        }
    }

    pub(super) unsafe fn layout(&mut self) {
        let s = GetDpiForSystem() as f32 / 96.0;
        let mon = self.mon_rect.unwrap_or_else(|| primary_monitor());
        let lens: Vec<f32> = self.items.iter().map(|it| self.item_len(it, s)).collect();
        let len = lens.iter().sum::<f32>() + 16.0 * s;
        let max_icon = if self.cfg.magnify { 68.0 * s } else { 44.0 * s };
        // Vertical docks show tooltips beside the icons, so they need a wider transparent margin.
        // Fully transparent pixels of a layered window are click-through, so this costs nothing.
        let vertical = self.vertical();
        let over = (max_icon - 44.0 * s).max(0.0) + if vertical { 260.0 * s } else { 44.0 * s };
        let pad = if vertical { 24.0 * s } else { 160.0 * s };
        // Reflection style: a taller strip — a little air above the icons, the icons, a small gap,
        // then the mirrored copy fading out. No plate behind any of it.
        let (thick, refl_gap, refl_len) = if self.cfg.style == 1 { ((8.0 + 44.0 + 6.0 + 30.0) * s, 6.0 * s, 30.0 * s) } else { (62.0 * s, 0.0, 0.0) };
        self.geo = Geo { s, slot: 56.0 * s, icon: 44.0 * s, max_icon, thick, refl_gap, refl_len, over, pad, margin: 8.0 * s, len, lens, mon };
        if self.scales.len() != self.items.len() { self.scales = vec![1.0; self.items.len()]; }
    }

    pub(super) fn is_icon(&self, i: usize) -> bool { matches!(self.items[i].kind, Kind::App | Kind::Start | Kind::Control | Kind::Tray) }

    /// Current (magnified) length of item i: neighbours get pushed apart like on macOS.
    pub(super) fn eff_len(&self, i: usize) -> f32 {
        let sc = self.scales.get(i).copied().unwrap_or(1.0);
        if self.is_icon(i) { self.geo.lens[i] * (1.0 + (sc - 1.0) * 0.9) } else { self.geo.lens[i] }
    }

    pub(super) fn cur_len(&self) -> f32 { 16.0 * self.geo.s + (0..self.items.len()).map(|i| self.eff_len(i)).sum::<f32>() }

    /// Centre of item i in screen coordinates along the axis, in the *un-magnified* layout
    /// (stable reference for the magnification falloff — no feedback wobble).
    pub(super) fn base_center_screen(&self, i: usize) -> f32 {
        let m = self.geo.mon;
        let origin = if self.vertical() { m.top as f32 + ((m.bottom - m.top) as f32 - self.geo.len) / 2.0 } else { m.left as f32 + ((m.right - m.left) as f32 - self.geo.len) / 2.0 };
        origin + 8.0 * self.geo.s + self.geo.lens[..i].iter().sum::<f32>() + self.geo.lens[i] / 2.0
    }

    /// Hover position in content-window local coordinates (stored in screen coordinates).
    pub(super) fn hover_local(&self) -> Option<(f32, f32)> {
        let (x, y, _, _) = self.content_rect(self.reveal);
        self.hover.map(|(hx, hy)| (hx - x as f32, hy - y as f32))
    }

    /// Body rect in screen coordinates for a given reveal (0 hidden .. 1 shown).
    pub(super) fn body_rect(&self, reveal: f32) -> (i32, i32, i32, i32) {
        let g = &self.geo;
        let m = g.mon;
        let len = self.cur_len();
        let hide = (1.0 - reveal) * (g.thick + g.margin + 4.0);
        match self.cfg.edge {
            Edge::Bottom => {
                let x = m.left as f32 + ((m.right - m.left) as f32 - len) / 2.0;
                let y = m.bottom as f32 - g.margin - g.thick + hide;
                (x as i32, y as i32, len as i32, g.thick as i32)
            }
            Edge::Left => {
                let y = m.top as f32 + ((m.bottom - m.top) as f32 - len) / 2.0;
                (m.left + (g.margin - hide) as i32, y as i32, g.thick as i32, len as i32)
            }
            Edge::Right => {
                let y = m.top as f32 + ((m.bottom - m.top) as f32 - len) / 2.0;
                (m.right - (g.margin + g.thick - hide) as i32, y as i32, g.thick as i32, len as i32)
            }
        }
    }

    /// Content window rect = body + overflow toward the screen centre.
    pub(super) fn content_rect(&self, reveal: f32) -> (i32, i32, i32, i32) {
        let (x, y, w, h) = self.body_rect(reveal);
        let o = self.geo.over as i32;
        let p = self.geo.pad as i32;
        match self.cfg.edge {
            Edge::Bottom => (x - p, y - o, w + 2 * p, h + o),
            Edge::Left => (x, y - p, w + o, h + 2 * p),
            Edge::Right => (x - o, y - p, w + o, h + 2 * p),
        }
    }

    /// Body rect in content-window local coordinates.
    pub(super) fn body_local(&self) -> R {
        let g = &self.geo;
        let len = self.cur_len();
        match self.cfg.edge {
            Edge::Bottom => R { x: g.pad, y: g.over, w: len, h: g.thick },
            Edge::Left => R { x: 0.0, y: g.pad, w: g.thick, h: len },
            Edge::Right => R { x: g.over, y: g.pad, w: g.thick, h: len },
        }
    }

    /// Centre of item i along the dock axis (local coordinates, magnified layout).
    pub(super) fn item_center(&self, i: usize) -> f32 {
        self.geo.pad + 8.0 * self.geo.s + (0..i).map(|k| self.eff_len(k)).sum::<f32>() + self.eff_len(i) / 2.0
    }

    /// Where item i sits on screen: popups and flyouts line up with it.
    pub(super) fn anchor(&self, i: usize) -> Anchor {
        let (cx, cy, _, _) = self.content_rect(self.reveal);
        let (bx, by, bw, _bh) = self.body_rect(self.reveal);
        let along = self.item_center(i) as i32;
        match self.cfg.edge {
            Edge::Bottom => Anchor { edge: Edge::Bottom, x: cx + along, y: by, mon: self.geo.mon },
            Edge::Left => Anchor { edge: Edge::Left, x: bx + bw, y: cy + along, mon: self.geo.mon },
            Edge::Right => Anchor { edge: Edge::Right, x: bx, y: cy + along, mon: self.geo.mon },
        }
    }

    pub(super) fn hit(&self, x: f32, y: f32) -> Option<usize> {
        let b = self.body_local();
        let (along, across_ok) = match self.cfg.edge {
            Edge::Bottom => (x, y >= b.y - (self.geo.max_icon - self.geo.icon)),
            Edge::Left => (y, x <= b.x + b.w + (self.geo.max_icon - self.geo.icon)),
            Edge::Right => (y, x >= b.x - (self.geo.max_icon - self.geo.icon)),
        };
        if !across_ok { return None; }
        let mut pos = self.geo.pad + 8.0 * self.geo.s;
        for i in 0..self.items.len() {
            let l = self.eff_len(i);
            if along >= pos && along < pos + l { return Some(i); }
            pos += l;
        }
        None
    }

    pub(super) unsafe fn refresh_items(&mut self, own_pid: u32) -> bool {
        // Running windows in z-order, grouped by app key.
        struct Ctx { list: Vec<HWND>, pid: u32 }
        unsafe extern "system" fn cb(h: HWND, data: LPARAM) -> BOOL {
            let c = &mut *(data.0 as *mut Ctx);
            if is_app_window(h, c.pid) { c.list.push(h); }
            BOOL(1)
        }
        let mut ctx = Ctx { list: Vec::new(), pid: own_pid };
        let _ = EnumWindows(Some(cb), LPARAM(&mut ctx as *mut _ as isize));
        self.win_keys.retain(|h, _| ctx.list.iter().any(|w| w.0 as isize == *h));
        let fg = GetForegroundWindow().0 as isize;

        let mut items: Vec<Item> = vec![Item { kind: Kind::Start, key: "start".into(), name: "Start".into(), launch: None, pinned: true, wins: vec![], active: false, exe: String::new() }];
        // The real Windows taskbar decides which apps are there and in what order; the dock's own
        // pin list is only used until the first good read (and for apps pinned on the dock alone).
        let snap = tbmirror::snapshot().filter(|s| !s.is_empty());
        let mirror = snap.is_some();
        if let Some(snap) = &snap {
            let lnks = tbmirror::windows_pins();
            for b in snap {
                let hidden = b.pinned && self.cfg.unpinned.iter().any(|k| k.eq_ignore_ascii_case(&b.appid));
                let found = self.cfg.pins.iter().find(|p| p.key.eq_ignore_ascii_case(&b.appid)).cloned()
                    .or_else(|| lnks.iter().find(|p| p.key.eq_ignore_ascii_case(&b.appid)).cloned());
                let (launch, exe) = match found {
                    Some(p) => {
                        let exe = match &p.exe {
                            Some(e) => e.clone(),
                            None => self.pin_exe.entry(p.launch.clone()).or_insert_with(|| lnk_target(&p.launch).unwrap_or_default()).clone(),
                        };
                        (Some(p.launch), exe)
                    }
                    None if b.appid.contains('!') => (Some(format!("shell:AppsFolder\\{}", b.appid)), String::new()),
                    None if b.appid.to_lowercase().ends_with(".exe") && b.appid.contains(":\\") => (Some(b.appid.clone()), b.appid.to_lowercase()),
                    None => (None, String::new()),
                };
                items.push(Item { kind: Kind::App, key: b.appid.clone(), name: b.name.clone(), launch, pinned: b.pinned && !hidden, wins: vec![], active: false, exe });
            }
            // Apps pinned on the dock only follow the ones pinned on the taskbar.
            let mut at = items.iter().rposition(|it| it.kind == Kind::App && it.pinned).map_or(1, |i| i + 1);
            for p in &self.cfg.pins {
                if items.iter().any(|it| it.kind == Kind::App && it.key.eq_ignore_ascii_case(&p.key)) { continue; }
                let exe = match &p.exe {
                    Some(e) => e.clone(),
                    None => self.pin_exe.entry(p.launch.clone()).or_insert_with(|| lnk_target(&p.launch).unwrap_or_default()).clone(),
                };
                items.insert(at, Item { kind: Kind::App, key: p.key.clone(), name: p.name.clone(), launch: Some(p.launch.clone()), pinned: true, wins: vec![], active: false, exe });
                at += 1;
            }
        } else {
            for p in &self.cfg.pins {
                // Pins saved by older builds have no exe yet: resolve the shortcut once and cache it.
                let exe = match &p.exe {
                    Some(e) => e.clone(),
                    None => self.pin_exe.entry(p.launch.clone()).or_insert_with(|| lnk_target(&p.launch).unwrap_or_default()).clone(),
                };
                items.push(Item { kind: Kind::App, key: p.key.clone(), name: p.name.clone(), launch: Some(p.launch.clone()), pinned: true, wins: vec![], active: false, exe });
            }
        }
        let n_base = items.len();
        for h in ctx.list {
            let hk = h.0 as isize;
            if !self.win_keys.contains_key(&hk) {
                crate::watchdog::phase(crate::watchdog::DOCK, "dock:items:key_of");
                self.win_keys.insert(hk, key_of(h));
                crate::watchdog::phase(crate::watchdog::DOCK, "dock:poll");
                // Vire's own window: the Dock lists it, so the hidden Windows taskbar must not also
                // make a (blank, unusable) button for it.
                let mut pid = 0u32;
                GetWindowThreadProcessId(h, Some(&mut pid));
                if pid == own_pid { hide_from_windows_taskbar(h); }
            }
            let (key, exe) = self.win_keys[&hk].clone();
            let exe_low = exe.to_lowercase();
            let key_low = key.to_lowercase();
            let pos = items.iter().position(|it| it.kind == Kind::App && it.key.to_lowercase() == key_low)
                .or_else(|| items.iter().position(|it| it.kind == Kind::App && it.pinned && !exe_low.is_empty() && it.exe == exe_low))
                // File Explorer: its shortcut has no file target to compare, but Windows names it by id.
                .or_else(|| if exe_low.ends_with("\\explorer.exe") {
                    items.iter().position(|it| it.kind == Kind::App && it.pinned && it.key.eq_ignore_ascii_case("Microsoft.Windows.Explorer"))
                } else { None });
            let idx = match pos {
                Some(i) => i,
                None => {
                    let launch = if key.contains('!') { Some(format!("shell:AppsFolder\\{key}")) } else if !exe.is_empty() { Some(exe.clone()) } else { None };
                    items.push(Item { kind: Kind::App, key: key.clone(), name: window_title(h), launch, pinned: false, wins: vec![], active: false, exe: exe_low.clone() });
                    items.len() - 1
                }
            };
            if !mirror && items[idx].wins.is_empty() && !items[idx].pinned { items[idx].name = window_title(h); }
            items[idx].wins.push(hk);
            if hk == fg { items[idx].active = true; }
        }
        // Like the Windows taskbar: pinned apps keep their slots, running apps that aren't pinned
        // stay in the order they were opened. (EnumWindows lists by z-order, which changes every
        // time you focus a window — using it directly made the icons jump around.)
        let mut loose: Vec<Item> = items.split_off(n_base.min(items.len()));
        // Taskbar entries for apps the dock found no window for (hidden, other desktop…): not shown.
        if mirror { items.retain(|it| it.kind != Kind::App || it.pinned || !it.wins.is_empty()); }
        self.order.retain(|k| loose.iter().any(|it| &it.key == k));
        for it in &loose { if !self.order.contains(&it.key) { self.order.push(it.key.clone()); } }
        loose.sort_by_key(|it| self.order.iter().position(|k| *k == it.key).unwrap_or(usize::MAX));
        items.extend(loose);
        // Right-hand group: background apps (^), quick settings, clock.
        items.push(Item { kind: Kind::Sep, key: "sep".into(), name: String::new(), launch: None, pinned: true, wins: vec![], active: false, exe: String::new() });
        items.push(Item { kind: Kind::Tray, key: "tray".into(), name: "Ứng dụng chạy nền".into(), launch: None, pinned: true, wins: vec![], active: false, exe: String::new() });
        if self.cfg.clock {
            items.push(Item { kind: Kind::Control, key: "control".into(), name: "Wi-Fi, âm lượng, pin".into(), launch: None, pinned: true, wins: vec![], active: false, exe: String::new() });
            items.push(Item { kind: Kind::Clock, key: "clock".into(), name: "Lịch & thông báo".into(), launch: None, pinned: true, wins: vec![], active: false, exe: String::new() });
        }
        let clock = now_clock();
        let light_flipped = self.update_backdrop();
        let changed = items != self.items || clock != self.clock || light_flipped;
        if changed {
            dbg(&format!("items: {}", items.iter().map(|i| format!("[{:?} '{}' key={} pinned={} wins={}]", i.kind as u8, i.name, i.key, i.pinned, i.wins.len())).collect::<Vec<_>>().join(" ")));
            let shape_changed = items.len() != self.items.len() || items.iter().zip(self.items.iter()).any(|(a, b)| a.kind != b.kind);
            self.items = items;
            self.clock = clock;
            if shape_changed { self.layout(); }
        }
        changed
    }

    pub(super) fn icon_for(&mut self, it: &Item) -> Option<&Img> {
        if !self.icons.contains_key(&it.key) {
            if !self.icon_wait.contains(&it.key) {
                let src = it.launch.clone().or_else(|| it.wins.first().and_then(|h| self.win_keys.get(h).map(|k| k.1.clone())));
                let (tries, retry_at) = self.icon_tries.get(&it.key).copied().unwrap_or((0, Instant::now()));
                match src {
                    None => { self.icons.insert(it.key.clone(), None); }
                    // Third miss: the old direct way (known to work for every kind of app), once.
                    Some(path) if tries >= 3 => { let img = unsafe { load_icon(&path) }; self.icons.insert(it.key.clone(), img); }
                    Some(path) if Instant::now() >= retry_at => {
                        self.icon_wait.insert(it.key.clone());
                        let (tx, key) = (self.icon_tx.clone(), it.key.clone());
                        let _ = std::thread::Builder::new().name("vire-icon".into()).spawn(move || unsafe {
                            // The shell's icon / thumbnail handlers want an STA thread.
                            let _ = CoInitializeEx(None, COINIT_APARTMENTTHREADED);
                            let _ = tx.send((key, load_icon(&path)));
                        });
                    }
                    Some(_) => {}
                }
            }
            if !self.icons.contains_key(&it.key) { return None; } // a placeholder is drawn until an icon arrives
        }        self.icons.get(&it.key).and_then(|o| o.as_ref())
    }
}
