//! Wallpaper engine: a dedicated thread owning one desktop-embedded window per monitor,
//! each rendered by a Media Foundation IMFMediaEngine (hardware decode, native seamless loop).

use crate::desktop::{self, DesktopTarget, Monitor};
use crate::model::{AppState, FitMode};
use crate::player::{Jukebox, PCmd, PlayerView};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::mpsc::{channel, Receiver, Sender};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use windows::core::{implement, w, Interface, Result, BSTR};
use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, RECT, WPARAM};
use windows::Win32::Graphics::Gdi::{GetStockObject, BLACK_BRUSH, HBRUSH};
use windows::Win32::Media::MediaFoundation::*;
use windows::Win32::System::Com::{CoCreateInstance, CoInitializeEx, CLSCTX_INPROC_SERVER, COINIT_MULTITHREADED};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::System::Power::{GetSystemPowerStatus, SYSTEM_POWER_STATUS};
use windows::Win32::System::Threading::GetCurrentProcessId;
use windows::Win32::UI::WindowsAndMessaging::*;

/// Everything the engine needs to know, snapshotted from AppState.
#[derive(Clone, PartialEq)]
pub struct Plan {
    pub default_path: Option<PathBuf>,
    pub per_monitor: HashMap<String, PathBuf>,
    pub muted: bool,
    pub volume: f32,
    pub rate: f32,
    pub fit: FitMode,
    pub user_paused: bool,
    pub pause_on_battery: bool,
    pub pause_on_fullscreen: bool,
    pub pause_on_covered: bool,
    /// Live wallpaper switched on (off = nothing is created; the Windows wallpaper shows).
    pub enabled: bool,
    /// The songs of the wallpaper on the primary monitor. When they change (a different
    /// wallpaper is picked, or the match is changed) the player switches to it.
    pub match_songs: Vec<PathBuf>,
}

impl Plan {
    pub fn from_state(st: &AppState) -> Self {
        let mut per_monitor = HashMap::new();
        let mut music: Vec<PathBuf> = Vec::new();
        let mons = desktop::monitors();
        for m in &mons {
            if let Some(p) = st.path_for_monitor(&m.device) { per_monitor.insert(m.device.clone(), p); }
            if m.primary { music = st.item_for_monitor(&m.device).map(|i| i.songs()).unwrap_or_default(); }
        }
        if mons.iter().all(|m| !m.primary) { music = st.selected().map(|i| i.songs()).unwrap_or_default(); }
        music.retain(|p| p.exists());
        if !st.settings.live_wallpaper { music.clear(); } // no wallpaper, no matched song
        let s = &st.settings;
        Plan {
            match_songs: music,
            default_path: st.selected().map(|i| i.path.clone()),
            per_monitor,
            muted: s.is_muted,
            volume: s.volume,
            rate: s.playback_rate,
            fit: s.fit,
            user_paused: s.user_paused,
            pause_on_battery: s.pause_on_battery,
            pause_on_fullscreen: s.pause_on_fullscreen,
            pause_on_covered: s.pause_on_covered,
            enabled: s.live_wallpaper,
        }
    }
    fn path_for(&self, device: &str) -> Option<PathBuf> {
        self.per_monitor.get(device).cloned().or_else(|| self.default_path.clone())
    }
}

enum Cmd {
    Apply(Plan),
    Player(PCmd),
    Shutdown(Sender<()>),
}

#[derive(Default)]
pub struct Status {
    /// Music-page player snapshot (written by the engine thread, read by the window).
    pub player: Mutex<PlayerView>,
    /// Video actually running (false when paused by user, battery or fullscreen app).
    pub playing: AtomicBool,
    pub auto_paused: AtomicBool,
    /// Monitors currently paused because a maximized window hides their desktop.
    pub covered: AtomicUsize,
    pub monitors: Mutex<Vec<Monitor>>,
}

#[derive(Clone)]
pub struct Engine {
    tx: Sender<Cmd>,
    pub status: Arc<Status>,
}

impl Engine {
    pub fn start() -> Self {
        let (tx, rx) = channel();
        let status = Arc::new(Status::default());
        *status.monitors.lock().unwrap() = desktop::monitors();
        let st = status.clone();
        std::thread::Builder::new().name("wallpaper-engine".into())
            .spawn(move || unsafe { engine_thread(rx, st) }).expect("engine thread");
        Engine { tx, status }
    }
    pub fn apply(&self, st: &AppState) { let _ = self.tx.send(Cmd::Apply(Plan::from_state(st))); }
    pub fn player(&self, cmd: PCmd) { let _ = self.tx.send(Cmd::Player(cmd)); }
    pub fn player_view(&self) -> PlayerView { self.status.player.lock().unwrap().clone() }
    /// Stops every video and restores the normal Windows wallpaper (blocks until done).
    pub fn shutdown(&self) {
        let (tx, rx) = channel();
        if self.tx.send(Cmd::Shutdown(tx)).is_ok() { let _ = rx.recv_timeout(Duration::from_secs(3)); }
    }
}

#[implement(IMFMediaEngineNotify)]
struct Notify { tag: &'static str }

impl IMFMediaEngineNotify_Impl for Notify_Impl {
    fn EventNotify(&self, event: u32, p1: usize, p2: u32) -> Result<()> {
        // Only traced with VIRE_DEBUG: every event of the music player, errors of the video one.
        if self.tag == "music" || event == 5 { crate::vdock::dbg(&format!("mf[{}] event {event} p1={p1} p2={p2:#x}", self.tag)); }
        Ok(())
    }
}

struct Player {
    hwnd: HWND,
    media: IMFMediaEngine,
    path: PathBuf,
    monitor: Monitor,
    fitted_for: Option<(u32, u32, FitMode)>,
    paused: bool,
    /// Created silent because the wallpaper has music (changing it means re-creating the player).
    music_muted: bool,
    /// Last (volume, muted, rate) given to the engine.
    applied: Option<(f32, bool, f32)>,
    /// Plays unconditionally until then (draws the first frame).
    warm_until: Instant,
}

impl Player {
    unsafe fn destroy(self) {
        let _ = self.media.Shutdown();
        let _ = DestroyWindow(self.hwnd);
    }
}

unsafe extern "system" fn wnd_proc(hwnd: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    match msg {
        WM_MOUSEACTIVATE => LRESULT(MA_NOACTIVATE as isize),
        _ => DefWindowProcW(hwnd, msg, wp, lp),
    }
}

struct Ctx {
    factory: IMFMediaEngineClassFactory,
    notify: IMFMediaEngineNotify,
    target: Option<DesktopTarget>,
    players: Vec<Player>,
    /// Players of monitors that went away, hidden and paused. Media Foundation never gives back
    /// the memory of a destroyed engine (~50 MB + threads each), so a screen that blinks on and
    /// off would make Vire grow without end; we reuse these instead of creating new ones.
    spare: Vec<Player>,
    /// Wallpaper changes in progress: (new player's window, old player, since). The old video stays
    /// on screen until the new one has its first frame, then it is parked in `spare` — loading a
    /// file in the visible player left the screen empty for a moment (the Windows wallpaper flashed).
    swaps: Vec<(HWND, Player, Instant)>,
    plan: Option<Plan>,
    /// Paused everywhere (user, battery, fullscreen game).
    global_paused: bool,
    /// Devices whose desktop is hidden by a maximized window.
    covered: Vec<String>,
}

impl Ctx {
    unsafe fn create_player(&mut self, mon: &Monitor, path: &PathBuf) -> Result<Player> {
        let hinst = GetModuleHandleW(None)?;
        let hwnd = CreateWindowExW(
            WS_EX_TOOLWINDOW | WS_EX_NOACTIVATE, w!("VireWallpaper"), w!("Vire Wallpaper"),
            WS_POPUP, mon.rect.left, mon.rect.top, mon.width(), mon.height(), None, None, hinst, None)?;
        if let Some(t) = &self.target { desktop::attach(hwnd, t, mon); }
        crate::vdock::dbg(&format!("player created on {} ({}x{})", mon.device, mon.width(), mon.height()));

        let mut attrs: Option<IMFAttributes> = None;
        MFCreateAttributes(&mut attrs, 3)?;
        let attrs = attrs.unwrap();
        attrs.SetUINT64(&MF_MEDIA_ENGINE_PLAYBACK_HWND, hwnd.0 as u64)?;
        attrs.SetUnknown(&MF_MEDIA_ENGINE_CALLBACK, &self.notify)?;
        let media = self.factory.CreateInstance(0, &attrs)?;
        media.SetLoop(true)?;
        media.SetAutoPlay(false)?;
        let music_muted = self.plan.as_ref().map_or(false, |p| !p.match_songs.is_empty());
        media.SetMuted(music_muted)?; // only the creation-time value is honoured by the engine
        media.SetSource(&BSTR::from(path.to_string_lossy().as_ref()))?;
        let mut p = Player { hwnd, media, path: path.clone(), monitor: mon.clone(), fitted_for: None, paused: false, music_muted, applied: None, warm_until: Instant::now() + Duration::from_secs(2) };
        if let Some(plan) = self.plan.clone() { self.configure(&mut p, &plan); }
        Ok(p)
    }

    /// Brings a running player in line with the plan — touching only what actually changed, so an
    /// unrelated settings change never makes the video stutter.
    unsafe fn configure(&self, p: &mut Player, plan: &Plan) {
        // Volume/mute only take effect when set while paused (then resumed below).
        // A wallpaper with its own music replaces the video's soundtrack.
        let want = (plan.volume.clamp(0.0, 1.0), p.music_muted || plan.muted);
        if p.applied.map_or(true, |a| a != (want.0, want.1, plan.rate)) {
            let _ = p.media.Pause();
            let _ = p.media.SetVolume(want.0 as f64);
            let _ = p.media.SetMuted(want.1);
            let _ = p.media.SetDefaultPlaybackRate(plan.rate as f64);
            let _ = p.media.SetPlaybackRate(plan.rate as f64);
            p.applied = Some((want.0, want.1, plan.rate));
            p.paused = !self.should_pause(&p.monitor.device); // forces the resume below
        }
        let pause = self.should_pause(&p.monitor.device);
        if pause != p.paused {
            p.paused = pause;
            if pause { let _ = p.media.Pause(); } else { let _ = p.media.Play(); }
        }
    }

    fn should_pause(&self, device: &str) -> bool {
        self.global_paused || self.covered.iter().any(|d| d == device)
    }

    /// Applies play/pause per monitor; only touches players whose state changed.
    unsafe fn refresh_pause(&mut self) {
        for i in 0..self.players.len() {
            let want = self.should_pause(&self.players[i].monitor.device);
            let p = &mut self.players[i];
            // A new player always plays for its first ~2 s so a first frame reaches the screen. One
            // that starts out paused (desktop covered at start-up / after a wallpaper change) never
            // draws anything, and the desktop would be black when you uncover it.
            if Instant::now() < p.warm_until {
                if p.paused { p.paused = false; let _ = p.media.Play(); }
                continue;
            }
            if want != p.paused {
                p.paused = want;
                if want { let _ = p.media.Pause(); } else { let _ = p.media.Play(); }
            }
        }
    }

    /// Crops the source rect so the video covers the monitor (Fill) or letterboxes (Fit).
    unsafe fn fit(&self, p: &mut Player) {
        let Some(plan) = &self.plan else { return };
        if !p.media.HasVideo().as_bool() { return; }
        let (mut vw, mut vh) = (0u32, 0u32);
        if p.media.GetNativeVideoSize(Some(&mut vw), Some(&mut vh)).is_err() || vw == 0 || vh == 0 { return; }
        if p.fitted_for == Some((vw, vh, plan.fit)) { return; }
        let (ww, wh) = (p.monitor.width().max(1) as f32, p.monitor.height().max(1) as f32);
        let mut src = MFVideoNormalizedRect { left: 0.0, top: 0.0, right: 1.0, bottom: 1.0 };
        if plan.fit == FitMode::Fill {
            let (va, wa) = (vw as f32 / vh as f32, ww / wh);
            if va > wa {
                let m = (1.0 - wa / va) / 2.0; src.left = m; src.right = 1.0 - m;
            } else {
                let m = (1.0 - va / wa) / 2.0; src.top = m; src.bottom = 1.0 - m;
            }
        }
        let dst = RECT { left: 0, top: 0, right: ww as i32, bottom: wh as i32 };
        if let Ok(ex) = p.media.cast::<IMFMediaEngineEx>() {
            let border = MFARGB { rgbBlue: 0, rgbGreen: 0, rgbRed: 0, rgbAlpha: 255 };
            if ex.UpdateVideoStream(Some(&src), Some(&dst), Some(&border)).is_ok() {
                p.fitted_for = Some((vw, vh, plan.fit));
            }
        }
    }

    unsafe fn clear(&mut self) {
        // The desktop window they lived in is gone (Explorer restarted): the windows are dead,
        // so these really have to go.
        for p in self.players.drain(..).chain(self.spare.drain(..)) { p.destroy(); }
        for (_, p, _) in self.swaps.drain(..) { p.destroy(); }
    }

    /// A player for `path` on monitor `m`: a parked one if there is one (same file first), else new.
    unsafe fn take_or_create(&mut self, m: &Monitor, path: &PathBuf, music_muted: bool) -> Option<Player> {
        let pick = self.spare.iter().position(|q| q.path == *path).or_else(|| (!self.spare.is_empty()).then_some(0));
        if let Some(i) = pick {
            let mut q = self.spare.remove(i);
            if IsWindow(q.hwnd).as_bool() { self.retarget(&mut q, m, path, music_muted); return Some(q); }
            q.destroy();
        }
        self.create_player(m, path).ok()
    }

    /// Hides the old video of a wallpaper change once the new one shows a picture (or after 3 s).
    unsafe fn finish_swaps(&mut self) {
        let mut i = 0;
        while i < self.swaps.len() {
            let (newh, t0) = (self.swaps[i].0, self.swaps[i].2);
            let ready = match self.players.iter().find(|q| q.hwnd == newh) {
                // 2 = MF_MEDIA_ENGINE_READY_HAVE_CURRENT_DATA: a decoded frame is there to show.
                Some(q) => q.media.GetReadyState() >= 2 && q.media.GetCurrentTime() > 0.08,
                None => true,
            } || t0.elapsed() > Duration::from_secs(3);
            if ready {
                let (_, mut old, _) = self.swaps.remove(i);
                crate::vdock::dbg(&format!("swap done on {} after {} ms", old.monitor.device, t0.elapsed().as_millis()));
                let _ = old.media.Pause();
                old.paused = true;
                let _ = ShowWindow(old.hwnd, SW_HIDE);
                self.spare.push(old);
            } else {
                i += 1;
            }
        }
    }

    /// Points an existing player at another monitor / file / mute state instead of building a new
    /// engine (see `spare`).
    unsafe fn retarget(&mut self, p: &mut Player, m: &Monitor, path: &PathBuf, music_muted: bool) {
        if !IsWindow(p.hwnd).as_bool() { return; }
        p.monitor = m.clone();
        self.keep_placed_one(p);
        if p.path != *path {
            let _ = p.media.SetSource(&BSTR::from(path.to_string_lossy().as_ref()));
            p.path = path.clone();
            p.fitted_for = None;
            p.paused = false;
            p.warm_until = Instant::now() + Duration::from_secs(2);
            let _ = p.media.Play();
        }
        if p.music_muted != music_muted { p.music_muted = music_muted; p.applied = None; }
        let _ = ShowWindow(p.hwnd, SW_SHOWNOACTIVATE);
    }

    /// Puts the player's window exactly over its monitor again if it drifted. The desktop window
    /// the videos live in changes its origin when a monitor leaves / returns or the layout changes,
    /// and the children keep their old relative position: one screen then shows the picture
    /// shifted, clipped or not at all. Only moves the window when it is really off (no flicker).
    unsafe fn keep_placed_one(&self, p: &mut Player) {
        let mut r = RECT::default();
        if GetWindowRect(p.hwnd, &mut r).is_err() { return; }
        let m = p.monitor.rect;
        if (r.left, r.top, r.right, r.bottom) == (m.left, m.top, m.right, m.bottom) && IsWindowVisible(p.hwnd).as_bool() { return; }
        crate::vdock::dbg(&format!("placement fixed on {}: was {},{},{},{} want {},{},{},{}", p.monitor.device, r.left, r.top, r.right, r.bottom, m.left, m.top, m.right, m.bottom));
        if let Some(t) = &self.target { desktop::place(p.hwnd, t, &p.monitor); }
        p.fitted_for = None;
    }

    unsafe fn keep_placed(&mut self) {
        let mut players = std::mem::take(&mut self.players);
        for p in players.iter_mut() { self.keep_placed_one(p); }
        self.players = players;
    }

    /// Makes the set of players match the plan + current monitors.
    unsafe fn sync(&mut self, status: &Status) {
        let Some(plan) = self.plan.clone() else { return };
        if !plan.enabled {
            // Live wallpaper off: free everything and let Windows show its own wallpaper again.
            if !self.players.is_empty() || !self.spare.is_empty() { self.clear(); desktop::refresh_static_wallpaper(); }
            return;
        }
        let mons = desktop::monitors();
        *status.monitors.lock().unwrap() = mons.clone();
        if self.target.is_none() { self.target = desktop::locate_target(); }

        let muted = !plan.match_songs.is_empty();
        let old = std::mem::take(&mut self.players);
        let mut keep = Vec::new();
        for mut p in old {
            let want = mons.iter().find(|m| m.device == p.monitor.device).and_then(|m| plan.path_for(&m.device).map(|path| (m.clone(), path)));
            match want {
                // Another file on the same, visible screen: load it in a second player *under* this
                // one and swap when it has a picture (no empty frame in between).
                Some((m, path)) if p.path != path && p.monitor.device == m.device && IsWindowVisible(p.hwnd).as_bool() => {
                    match self.take_or_create(&m, &path, muted) {
                        Some(q) => {
                            let _ = SetWindowPos(q.hwnd, p.hwnd, 0, 0, 0, 0, SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE);
                            crate::vdock::dbg(&format!("swap start on {}", m.device));
                            self.swaps.push((q.hwnd, p, Instant::now()));
                            keep.push(q);
                        }
                        None => { self.retarget(&mut p, &m, &path, muted); keep.push(p); }
                    }
                }
                Some((m, path)) => { self.retarget(&mut p, &m, &path, muted); keep.push(p); }
                None => {
                    // Monitor gone (or no wallpaper for it): park the player, don't destroy it.
                    crate::vdock::dbg(&format!("sync: park player {}", p.monitor.device));
                    let _ = p.media.Pause();
                    p.paused = true;
                    let _ = ShowWindow(p.hwnd, SW_HIDE);
                    self.spare.push(p);
                }
            }
        }
        self.players = keep;
        for m in &mons {
            if self.players.iter().any(|p| p.monitor.device == m.device) { continue; }
            let Some(path) = plan.path_for(&m.device) else { continue };
            // Prefer a parked player (same file first: it resumes without reloading).
            let pick = self.spare.iter().position(|p| p.path == path && p.monitor.device == m.device)
                .or_else(|| self.spare.iter().position(|p| p.path == path))
                .or_else(|| (!self.spare.is_empty()).then_some(0));
            if let Some(i) = pick {
                let mut p = self.spare.remove(i);
                if IsWindow(p.hwnd).as_bool() {
                    crate::vdock::dbg(&format!("sync: reuse parked player for {}", m.device));
                    self.retarget(&mut p, m, &path, muted);
                    self.players.push(p);
                    continue;
                }
                p.destroy();
            }
            match self.create_player(m, &path) {
                Ok(p) => self.players.push(p),
                Err(e) => eprintln!("player error on {}: {e}", m.device),
            }
        }
        for i in 0..self.players.len() {
            let pp: *mut Player = &mut self.players[i];
            self.configure(&mut *pp, &plan);
        }
    }

}

fn on_battery() -> bool {
    let mut s = SYSTEM_POWER_STATUS::default();
    unsafe { GetSystemPowerStatus(&mut s).is_ok() && (s.ACLineStatus == 0 || s.SystemStatusFlag == 1) }
}

unsafe fn engine_thread(rx: Receiver<Cmd>, status: Arc<Status>) {
    let _ = CoInitializeEx(None, COINIT_MULTITHREADED);
    if let Err(e) = MFStartup(MF_VERSION, MFSTARTUP_FULL) { eprintln!("MFStartup failed: {e}"); return; }
    let hinst = GetModuleHandleW(None).unwrap_or_default();
    let wc = WNDCLASSW {
        lpfnWndProc: Some(wnd_proc),
        hInstance: hinst.into(),
        lpszClassName: w!("VireWallpaper"),
        hbrBackground: HBRUSH(GetStockObject(BLACK_BRUSH).0),
        ..Default::default()
    };
    RegisterClassW(&wc);

    let factory: IMFMediaEngineClassFactory = match CoCreateInstance(&CLSID_MFMediaEngineClassFactory, None, CLSCTX_INPROC_SERVER) {
        Ok(f) => f,
        Err(e) => { eprintln!("Media engine unavailable: {e}"); return; }
    };
    let track_notify: IMFMediaEngineNotify = Notify { tag: "track" }.into();
    let mut jukebox = Jukebox::new();
    let mut ctx = Ctx { factory, notify: Notify { tag: "video" }.into(), target: None, players: Vec::new(), spare: Vec::new(), swaps: Vec::new(), plan: None, global_paused: false, covered: Vec::new() };
    let own_pid = GetCurrentProcessId();
    let mut last_check = Instant::now() - Duration::from_secs(10);
    let mut last_mons = desktop::monitors();
    let mut trim_at: Option<Instant> = None;
    let mut hotkeys_on = false;
    let mut trimmed_once = false;
    let mut last_stats = Instant::now();
    let mut last_eff: Option<f32> = None;
    let mut last_match: Vec<PathBuf> = Vec::new();
    let mut mons_cand: Option<(Vec<Monitor>, Instant)> = None;
    let mut stable_key: (bool, Vec<String>) = (false, Vec::new());
    let mut cand_key = stable_key.clone();
    let mut cand_since = Instant::now();
    let mut first_pause = true;

    loop {
        while let Ok(cmd) = rx.try_recv() {
            match cmd {
                Cmd::Apply(plan) => {
                    crate::vdock::dbg("cmd: Apply");
                    let t0 = Instant::now();
                    // One volume / mute for the music player (the card's speaker control).
                    let eff = if plan.muted { 0.0 } else { plan.volume.clamp(0.0, 1.0) };
                    if last_eff != Some(eff) { last_eff = Some(eff); jukebox.handle(PCmd::Volume(eff), &ctx.factory, &track_notify); }
                    // A different wallpaper (or a new match) → its matched song shows up in the player
                    // and plays; from there you can switch to any other song without touching the match.
                    if plan.match_songs != last_match {
                        last_match = plan.match_songs.clone();
                        if !last_match.is_empty() {
                            // The queue is just the wallpaper's own playlist, looping: left alone it
                            // keeps cycling through those songs (one song = repeat that song).
                            // If the song already playing is also in this wallpaper's list it carries on
                            // to its end (no restart) and the list continues from there.
                            jukebox.handle(PCmd::Requeue(last_match.clone()), &ctx.factory, &track_notify);
                            jukebox.handle(PCmd::Repeat(if last_match.len() > 1 { 1 } else { 2 }), &ctx.factory, &track_notify);
                            if plan.user_paused { jukebox.handle(PCmd::Pause, &ctx.factory, &track_notify); }
                            else if !jukebox.playing() { jukebox.handle(PCmd::Toggle, &ctx.factory, &track_notify); } // kept song was paused (e.g. restored at start)
                        }
                    }
                    ctx.plan = Some(plan);
                    ctx.sync(&status);
                    crate::vdock::dbg(&format!("engine: apply took {} ms", t0.elapsed().as_millis()));
                    trim_at = Some(Instant::now() + Duration::from_secs(4));
                }
                Cmd::Player(pc) => { jukebox.handle(pc, &ctx.factory, &track_notify); }
                Cmd::Shutdown(reply) => {
                    jukebox.shutdown();
                    ctx.clear();
                    desktop::refresh_static_wallpaper();
                    let _ = MFShutdown();
                    let _ = reply.send(());
                    return;
                }
            }
        }

        // Media keys belong to Vire only while a track is loaded (otherwise they'd be swallowed
        // from your browser / other players for nothing).
        if jukebox.has_media() != hotkeys_on {
            hotkeys_on = jukebox.has_media();
            use windows::Win32::UI::Input::KeyboardAndMouse::{RegisterHotKey, UnregisterHotKey, MOD_NOREPEAT, VK_MEDIA_NEXT_TRACK, VK_MEDIA_PLAY_PAUSE, VK_MEDIA_PREV_TRACK, VK_MEDIA_STOP};
            for (i, vk) in [VK_MEDIA_PLAY_PAUSE, VK_MEDIA_NEXT_TRACK, VK_MEDIA_PREV_TRACK, VK_MEDIA_STOP].into_iter().enumerate() {
                if hotkeys_on { let _ = RegisterHotKey(None, 1 + i as i32, MOD_NOREPEAT, vk.0 as u32); } else { let _ = UnregisterHotKey(None, 1 + i as i32); }
            }
        }
        let mut msg = MSG::default();
        while PeekMessageW(&mut msg, None, 0, 0, PM_REMOVE).as_bool() {
            if msg.message == WM_HOTKEY {
                let pc = match msg.wParam.0 { 1 => PCmd::Toggle, 2 => PCmd::Next, 3 => PCmd::Prev, _ => PCmd::Pause };
                jukebox.handle(pc, &ctx.factory, &track_notify);
                continue;
            }
            let _ = TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }

        // Twice a second: cheap Win32 queries, so a maximized/minimized window pauses/resumes quickly.
        if last_check.elapsed() >= Duration::from_millis(500) {
            last_check = Instant::now();
            // Explorer restarted or monitors changed -> rebuild everything.
            let parent_gone = ctx.target.as_ref().map_or(false, |t| !IsWindow(t.parent).as_bool());
            let mons = desktop::monitors();
            if parent_gone {
                crate::vdock::dbg("loop: desktop parent window gone -> rebuild everything");
                // Explorer restarted: rebuild everything.
                last_mons = mons;
                mons_cand = None;
                ctx.clear();
                ctx.target = None;
                ctx.sync(&status);
            } else if mons != last_mons {
                // A monitor change must hold for a second before we react (a screen that blinks or
                // an RDP session changing the list used to rebuild the wallpaper = a visible flash).
                // And only the monitors that really changed are rebuilt.
                match &mons_cand {
                    Some((m, t)) if *m == mons => { if t.elapsed() >= Duration::from_millis(1000) { crate::vdock::dbg(&format!("loop: monitors changed -> sync ({} -> {})", last_mons.len(), mons.len())); last_mons = mons; mons_cand = None;
                        // A layout change can also move the desktop window the videos are parented to.
                        if let Some(nt) = desktop::locate_target() { if ctx.target.as_ref().map_or(true, |t| t.parent != nt.parent) { ctx.clear(); } ctx.target = Some(nt); }
                        ctx.sync(&status); } }
                    _ => mons_cand = Some((mons, Instant::now())),
                }
            } else {
                mons_cand = None;
            }
            ctx.keep_placed();
            if let Some(plan) = ctx.plan.clone() {
                let auto_now = (plan.pause_on_battery && on_battery()) || (plan.pause_on_fullscreen && desktop::fullscreen_app_active(own_pid));
                let covered_now = if plan.pause_on_covered && !(plan.user_paused || auto_now) { desktop::covered_monitors(own_pid, &last_mons) } else { Vec::new() };
                // Pause / resume only after the situation has been the same for 1.2 s: Alt+Tab (the
                // task switcher counts as "fullscreen") and windows flicking between maximized
                // states used to pause and restart the video every time.
                let key = (auto_now, covered_now);
                if first_pause { stable_key = key.clone(); cand_key = key; first_pause = false; }
                else if key.0 == false && key.1.is_empty() { stable_key = key.clone(); cand_key = key; } // desktop uncovered: resume AT ONCE
                else if key != cand_key { cand_key = key; cand_since = Instant::now(); }
                else if cand_key != stable_key && cand_since.elapsed() >= Duration::from_millis(1200) { stable_key = cand_key.clone(); }
                let auto = stable_key.0;
                status.auto_paused.store(auto && !plan.user_paused, Ordering::Relaxed);
                ctx.global_paused = plan.user_paused || auto;
                ctx.covered = if ctx.global_paused { Vec::new() } else { stable_key.1.clone() };
                ctx.refresh_pause();
            }
            let covered = ctx.players.iter().filter(|p| p.paused && !ctx.global_paused).count();
            status.covered.store(covered, Ordering::Relaxed);
            status.playing.store(ctx.players.iter().any(|p| !p.paused), Ordering::Relaxed);
        }

        // Debug (VIRE_DEBUG): decoder statistics every 5 s — frames rendered / dropped / corrupted / jitter.
        if last_stats.elapsed() >= Duration::from_secs(5) {
            last_stats = Instant::now();
            if std::env::var_os("VIRE_DEBUG").is_some() {
                for p in &ctx.players {
                    if let Ok(ex) = p.media.cast::<IMFMediaEngineEx>() {
                        let get = |id: MF_MEDIA_ENGINE_STATISTIC| ex.GetStatistics(id).ok().and_then(|v| u64::try_from(&v).ok()).unwrap_or(0);
                        crate::vdock::dbg(&format!("video[{}] rendered={} dropped={} corrupted={} jitter={} fps={}", p.monitor.device, get(MF_MEDIA_ENGINE_STATISTIC_FRAMES_RENDERED), get(MF_MEDIA_ENGINE_STATISTIC_FRAMES_DROPPED), get(MF_MEDIA_ENGINE_STATISTIC_FRAMES_CORRUPTED), get(MF_MEDIA_ENGINE_STATISTIC_PLAYBACK_JITTER), get(MF_MEDIA_ENGINE_STATISTIC_FRAMES_PER_SECOND)));
                    }
                }
            }
        }
        if !ctx.swaps.is_empty() { ctx.finish_swaps(); }
        jukebox.tick(&ctx.factory, &track_notify);
        if jukebox.take_dirty() { *status.player.lock().unwrap() = jukebox.snapshot(); }

        for i in 0..ctx.players.len() {
            if ctx.players[i].fitted_for.is_none() {
                let p: *mut Player = &mut ctx.players[i];
                ctx.fit(&mut *p);
            }
        }


        // Once the decoder has warmed up, give its start-up pages back to Windows.
        // …and again every 30 s, so decoder scratch pages never pile up in the working set.
        // Only while NOTHING is playing: trimming the working set under a running 4K decoder makes it
        // fault its buffers back in = a hitch / flicker every time. (While playing, once after
        // start-up is enough.)
        if trim_at.map_or(false, |t| Instant::now() >= t) {
            let playing = status.playing.load(Ordering::Relaxed);
            if !playing || !trimmed_once { crate::trim_memory(); trimmed_once = true; }
            trim_at = if playing { None } else { Some(Instant::now() + Duration::from_secs(30)) };
        }

        MsgWaitForMultipleObjects(None, false, 100, QS_ALLINPUT);
    }
}
