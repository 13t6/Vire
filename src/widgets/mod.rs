//! Desktop widgets: small floating panels that live on the desktop wallpaper (above the wallpaper
//! and icons, under every normal window), managed from the dashboard's *Widgets* tab.
//!
//! Layout of this module:
//! * `mod.rs`   — the data model (instances, profiles), the [`Hub`] shared with the dashboard, and
//!                the **registry** of widget kinds. Adding a widget = write one file with a
//!                [`Widget`] implementation and add one line to [`kinds`]; nothing else changes.
//! * `host.rs`  — the desktop side: one layered window per visible instance, drag / resize /
//!                context menu, timers, audio leases, multi-monitor placement.
//! * `clock.rs`, `viz.rs` — the first two widgets (render + their settings).
//!
//! Rendering is the CPU rasteriser of the dock (`raster.rs`): no GPU surface, and a widget only
//! draws when its content really changes (the clock once a second / minute, the visualizer only
//! while sound is playing).

pub mod clock;
mod host;
pub mod viz;

pub use host::start;

use crate::dsp::Frame;
use crate::raster::Canvas;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU32, AtomicU64, Ordering};
use std::sync::{Arc, Mutex, MutexGuard};

// ------------------------------------------------------------------ registry

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Category { Clock, Music, System, Productivity, Custom }

impl Category {
    pub const ALL: [Category; 5] = [Category::Clock, Category::Music, Category::System, Category::Productivity, Category::Custom];
    pub fn label(self) -> &'static str {
        match self { Category::Clock => "Đồng hồ", Category::Music => "Nhạc", Category::System => "Hệ thống", Category::Productivity => "Năng suất", Category::Custom => "Tuỳ biến" }
    }
}

/// What a widget gets when it draws.
pub struct Ctx<'a> {
    /// Local time (and the UTC offset of this machine, in minutes).
    pub now: Now,
    /// Seconds since this widget last drew (clamped), for animation.
    pub dt: f32,
    /// DPI scale of the system (1.0 = 96 dpi).
    pub scale: f32,
    /// Latest analysed audio frame (only when the widget asked for audio and capture is on).
    pub audio: Option<&'a Frame>,
    /// Sound is really playing right now.
    pub signal: bool,
    /// The built-in music player is playing (the source some widgets can be limited to).
    pub builtin_playing: bool,
    /// Drawing a gallery preview, not the desktop widget.
    pub preview: bool,
}

#[derive(Clone, Copy, Debug)]
pub struct Now { pub year: i32, pub month: u32, pub day: u32, pub weekday: u32, pub hour: u32, pub minute: u32, pub second: u32, pub ms: u32, pub utc_offset_min: i32 }

/// A kind of desktop widget. One object per *instance* (so it can keep animation state).
pub trait Widget {
    /// Draws into `c` (cleared, `c.w × c.h` physical pixels). Returns false when nothing changed
    /// since the last call (the previous picture stays on screen — no redraw, no update).
    fn render(&mut self, ctx: &Ctx, cfg: &Value, c: &mut Canvas) -> bool;
    /// Milliseconds until it needs to draw again (0 = never until its settings change).
    fn interval_ms(&self, cfg: &Value, now: &Now, audio_live: bool) -> u32;
    /// Whether it reads the audio analysis (the capture only runs while such a widget is visible).
    fn uses_audio(&self, _cfg: &Value) -> bool { false }
    /// Frosted-glass backdrop behind the widget: corner radius in logical px, or None.
    fn blur_radius(&self, _cfg: &Value) -> Option<f32> { None }
}

/// Static description + factory of a widget kind.
pub struct KindInfo {
    pub id: &'static str,
    pub name: &'static str,
    pub desc: &'static str,
    pub category: Category,
    /// Logical size in px (new instance) and the smallest allowed.
    pub default_size: (i32, i32),
    pub min_size: (i32, i32),
    pub defaults: fn() -> Value,
    pub create: fn() -> Box<dyn Widget>,
    /// Draws the kind-specific settings; returns true when something changed.
    pub customize: fn(&mut eframe::egui::Ui, &mut Value) -> bool,
}

/// Every widget Vire knows. **Register new widgets here.**
pub fn kinds() -> &'static [KindInfo] {
    static KINDS: [KindInfo; 2] = [clock::INFO, viz::INFO];
    &KINDS
}

pub fn kind(id: &str) -> Option<&'static KindInfo> { kinds().iter().find(|k| k.id == id) }

// ------------------------------------------------------------------ data

/// One widget on the desktop.
#[derive(Clone, Serialize, Deserialize)]
pub struct Instance {
    pub id: u32,
    pub kind: String,
    pub name: String,
    pub visible: bool,
    /// Monitor device (`\\.\DISPLAY1`); empty or unknown = the primary one.
    pub monitor: String,
    /// Top-left corner relative to that monitor, in logical px.
    pub x: i32,
    pub y: i32,
    pub w: i32,
    pub h: i32,
    pub locked: bool,
    pub settings: Value,
}

/// A named set of widgets (e.g. "Work", "Gaming"). `saved` is the layout snapshot to go back to.
#[derive(Clone, Serialize, Deserialize)]
pub struct Profile {
    pub name: String,
    pub instances: Vec<Instance>,
    #[serde(default)]
    pub saved: Option<Vec<Instance>>,
}

#[derive(Serialize, Deserialize)]
pub struct State {
    pub profiles: Vec<Profile>,
    pub active: usize,
    pub next_id: u32,
    /// System audio capture allowed (the visualizer's loopback). Nothing is captured when off.
    pub capture: bool,
}

impl Default for State {
    fn default() -> Self {
        State { profiles: vec![Profile { name: "Mặc định".into(), instances: Vec::new(), saved: None }], active: 0, next_id: 1, capture: true }
    }
}

impl State {
    pub fn profile(&self) -> &Profile { &self.profiles[self.active.min(self.profiles.len() - 1)] }
    pub fn profile_mut(&mut self) -> &mut Profile { let i = self.active.min(self.profiles.len() - 1); &mut self.profiles[i] }
    pub fn instance(&self, id: u32) -> Option<&Instance> { self.profile().instances.iter().find(|i| i.id == id) }
    pub fn instance_mut(&mut self, id: u32) -> Option<&mut Instance> { self.profile_mut().instances.iter_mut().find(|i| i.id == id) }

    /// Adds a widget of `kind` to the active profile, in the middle of the primary monitor (nudged
    /// a little for every one already there). Returns its id.
    pub fn add(&mut self, kind_id: &str) -> Option<u32> {
        let k = kind(kind_id)?;
        let id = self.next_id;
        self.next_id += 1;
        let n = self.profile().instances.iter().filter(|i| i.kind == kind_id).count();
        let mon = crate::desktop::monitors().into_iter().find(|m| m.primary);
        let (mw, mh) = mon.as_ref().map_or((1920, 1080), |m| (m.width(), m.height()));
        let (w, h) = k.default_size;
        let name = if n == 0 { k.name.to_string() } else { format!("{} {}", k.name, n + 1) };
        self.profile_mut().instances.push(Instance {
            id, kind: kind_id.to_string(), name, visible: true, monitor: mon.map(|m| m.device).unwrap_or_default(),
            x: ((mw - w) / 2 + 36 * n as i32).max(0), y: ((mh - h) / 3 + 36 * n as i32).max(0), w, h, locked: false, settings: (k.defaults)(),
        });
        Some(id)
    }

    pub fn duplicate(&mut self, id: u32) -> Option<u32> {
        let mut copy = self.instance(id)?.clone();
        copy.id = self.next_id;
        self.next_id += 1;
        copy.name = format!("{} (bản sao)", copy.name);
        copy.x += 40;
        copy.y += 40;
        let new_id = copy.id;
        self.profile_mut().instances.push(copy);
        Some(new_id)
    }

    pub fn remove(&mut self, id: u32) { self.profile_mut().instances.retain(|i| i.id != id); }
}

fn file() -> PathBuf { crate::model::data_dir().join("widgets.json") }

/// Shared between the dashboard (edits) and the desktop host (draws, drags).
pub struct Hub {
    st: Mutex<State>,
    /// Bumped on every real change: the host re-reads the layout when it differs from its own.
    version: AtomicU64,
    dirty: Mutex<Option<std::time::Instant>>,
    host_tid: AtomicU32,
    /// A widget's "Customize…" asked the dashboard to open on this instance (0 = none).
    edit_req: AtomicU32,
}

impl Hub {
    pub fn load() -> Arc<Hub> {
        let st: State = std::fs::read_to_string(file()).ok().and_then(|s| serde_json::from_str(s.trim_start_matches('\u{feff}')).ok()).unwrap_or_default();
        let mut st = if st.profiles.is_empty() { State::default() } else { st };
        st.active = st.active.min(st.profiles.len() - 1);
        Arc::new(Hub { st: Mutex::new(st), version: AtomicU64::new(1), dirty: Mutex::new(None), host_tid: AtomicU32::new(0), edit_req: AtomicU32::new(0) })
    }

    pub fn lock(&self) -> MutexGuard<'_, State> { self.st.lock().unwrap() }

    pub fn version(&self) -> u64 { self.version.load(Ordering::Relaxed) }

    /// Runs `f` on the state; when it returns true the change is published (host wakes up) and
    /// scheduled for saving.
    pub fn with_mut(&self, f: impl FnOnce(&mut State) -> bool) -> bool {
        let changed = f(&mut self.st.lock().unwrap());
        if changed { self.touch(); }
        changed
    }

    /// Publishes a change made through [`lock`].
    pub fn touch(&self) {
        self.version.fetch_add(1, Ordering::Relaxed);
        *self.dirty.lock().unwrap() = Some(std::time::Instant::now());
        self.wake();
    }

    pub fn set_host_thread(&self, tid: u32) { self.host_tid.store(tid, Ordering::Relaxed); }

    pub fn wake(&self) {
        let tid = self.host_tid.load(Ordering::Relaxed);
        if tid != 0 { unsafe { let _ = windows::Win32::UI::WindowsAndMessaging::PostThreadMessageW(tid, host::WM_HUB, windows::Win32::Foundation::WPARAM(0), windows::Win32::Foundation::LPARAM(0)); } }
    }

    /// Writes the layout if it changed (called from both sides; at most every 400 ms).
    pub fn flush(&self, force: bool) {
        let mut d = self.dirty.lock().unwrap();
        let Some(t) = *d else { return };
        if !force && t.elapsed().as_millis() < 400 { return; }
        *d = None;
        drop(d);
        if let Ok(s) = serde_json::to_string_pretty(&*self.st.lock().unwrap()) {
            let p = file();
            let tmp = p.with_extension("json.tmp");
            if std::fs::write(&tmp, s).is_ok() { let _ = std::fs::rename(&tmp, &p); }
        }
    }

    pub fn request_edit(&self, id: u32) { self.edit_req.store(id, Ordering::Relaxed); }
    pub fn take_edit_request(&self) -> Option<u32> { match self.edit_req.swap(0, Ordering::Relaxed) { 0 => None, id => Some(id) } }
}

// ------------------------------------------------------------------ small shared helpers

/// Local wall-clock time and this machine's UTC offset.
pub fn now_local() -> Now {
    use windows::Win32::System::SystemInformation::{GetLocalTime, GetSystemTime};
    unsafe {
        let (l, u) = (GetLocalTime(), GetSystemTime());
        let secs = |t: &windows::Win32::Foundation::SYSTEMTIME| days_from_civil(t.wYear as i32, t.wMonth as u32, t.wDay as u32) * 86_400 + t.wHour as i64 * 3600 + t.wMinute as i64 * 60 + t.wSecond as i64;
        let off = ((secs(&l) - secs(&u)) as f64 / 60.0 / 15.0).round() as i32 * 15; // whole quarter hours
        Now { year: l.wYear as i32, month: l.wMonth as u32, day: l.wDay as u32, weekday: l.wDayOfWeek as u32, hour: l.wHour as u32, minute: l.wMinute as u32, second: l.wSecond as u32, ms: l.wMilliseconds as u32, utc_offset_min: off }
    }
}

/// The same instant shown in another fixed UTC offset (minutes).
pub fn shifted(now: &Now, offset_min: i32) -> Now {
    let local = days_from_civil(now.year, now.month, now.day) * 86_400 + now.hour as i64 * 3600 + now.minute as i64 * 60 + now.second as i64;
    let t = local - now.utc_offset_min as i64 * 60 + offset_min as i64 * 60;
    let days = t.div_euclid(86_400);
    let rem = t.rem_euclid(86_400);
    let (y, m, d) = civil_from_days(days);
    Now { year: y, month: m, day: d, weekday: (days + 4).rem_euclid(7) as u32, hour: (rem / 3600) as u32, minute: ((rem % 3600) / 60) as u32, second: (rem % 60) as u32, ms: now.ms, utc_offset_min: offset_min }
}

// Howard Hinnant's civil-date algorithms (days since 1970-01-01).
fn days_from_civil(y: i32, m: u32, d: u32) -> i64 {
    let y = if m <= 2 { y - 1 } else { y } as i64;
    let era = y.div_euclid(400);
    let yoe = y - era * 400;
    let doy = (153 * (if m > 2 { m - 3 } else { m + 9 }) as i64 + 2) / 5 + d as i64 - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe - 719_468
}

fn civil_from_days(z: i64) -> (i32, u32, u32) {
    let z = z + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    ((y + if m <= 2 { 1 } else { 0 }) as i32, m, d)
}

/// True when Windows' display language is Vietnamese (dates then read Vietnamese).
pub fn ui_vietnamese() -> bool {
    #[link(name = "kernel32")] extern "system" { fn GetUserDefaultUILanguage() -> u16; }
    unsafe { GetUserDefaultUILanguage() & 0x3FF == 0x2A }
}

/// Colour helper shared by the widgets' renderers.
pub fn lerp_rgb(a: [u8; 3], b: [u8; 3], t: f32) -> [u8; 3] {
    let l = |x: u8, y: u8| (x as f32 + (y as f32 - x as f32) * t.clamp(0.0, 1.0)).round() as u8;
    [l(a[0], b[0]), l(a[1], b[1]), l(a[2], b[2])]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn civil_round_trip_and_weekday() {
        // 2026-11-19 is a Thursday (weekday 4, Sunday = 0).
        let d = days_from_civil(2026, 11, 19);
        assert_eq!(civil_from_days(d), (2026, 11, 19));
        assert_eq!((d + 4).rem_euclid(7), 4);
    }

    #[test]
    fn timezone_shift_crosses_midnight() {
        let local = Now { year: 2026, month: 12, day: 31, hour: 23, minute: 30, second: 0, ms: 0, weekday: 4, utc_offset_min: 420 };
        let utc = shifted(&local, 0);
        assert_eq!((utc.year, utc.month, utc.day, utc.hour, utc.minute), (2026, 12, 31, 16, 30));
        let tokyo = shifted(&local, 540); // UTC+9, two hours ahead of +7
        assert_eq!((tokyo.year, tokyo.month, tokyo.day, tokyo.hour, tokyo.minute), (2027, 1, 1, 1, 30));
        assert_eq!(tokyo.weekday, 5);
    }

    #[test]
    fn profile_add_duplicate_remove() {
        let mut st = State::default();
        let a = st.add("clock").unwrap();
        let b = st.duplicate(a).unwrap();
        assert_ne!(a, b);
        assert_eq!(st.profile().instances.len(), 2);
        st.remove(a);
        assert_eq!(st.profile().instances.len(), 1);
        assert!(st.add("nope").is_none());
    }
}
