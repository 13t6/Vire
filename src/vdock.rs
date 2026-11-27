//! Vire Dock — Vire's own macOS-style dock, placeable on the bottom / left / right edge.
//!
//! Two light windows on one thread:
//! * `body`    — the glass slab: system blur (Acrylic / Mica-like / clear Glass) clipped to a
//!               rounded region, plus a CPU-drawn rim & sheen;
//! * `content` — icons, magnification, running dots, tooltip, clock. Per-pixel alpha.
//! Everything is CPU-rendered into small bitmaps and only redrawn on change or while animating.
//! While on, the Windows taskbar is hidden (and restored when Vire Dock is turned off / Vire quits).

use crate::model::{AppState, Edge, Material, Pin};
use crate::raster::*;
use std::cell::RefCell;
use std::collections::HashMap;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::mpsc::{channel, Sender};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use windows::core::{w, Interface, HSTRING, PCWSTR, PWSTR};
use windows::Win32::Foundation::*;
use windows::Win32::Graphics::Gdi::*;
use windows::Win32::Storage::EnhancedStorage::PKEY_AppUserModel_ID;
use windows::Win32::System::Com::StructuredStorage::PropVariantToStringAlloc;
use windows::Win32::System::Com::*;
use windows::Win32::System::LibraryLoader::{GetModuleHandleW, GetProcAddress};
use windows::Win32::System::Threading::*;
use windows::Win32::UI::HiDpi::GetDpiForSystem;
use windows::Win32::UI::Input::KeyboardAndMouse::*;
use windows::Win32::UI::Shell::PropertiesSystem::{IPropertyStore, SHGetPropertyStoreForWindow};
use windows::Win32::UI::Shell::*;
use windows::Win32::UI::WindowsAndMessaging::*;

// ------------------------------------------------------------------ public API

#[derive(Clone, PartialEq)]
pub struct VdCfg {
    pub enabled: bool,
    pub edge: Edge,
    pub autohide: bool,
    pub magnify: bool,
    pub clock: bool,
    pub style: u8,
    pub material: Material,
    pub pins: Vec<Pin>,
    pub unpinned: Vec<String>,
}

impl VdCfg {
    pub fn from(s: &crate::model::Settings) -> Self {
        VdCfg { enabled: s.vd_enabled, edge: s.vd_edge, autohide: s.vd_autohide, magnify: s.vd_magnify, clock: s.vd_clock, style: s.vd_style, material: s.material, pins: s.vd_pins.clone(), unpinned: s.vd_unpinned.clone() }
    }
}

/// Desired state shared with the UI. The dock thread *reconciles* toward it every 100 ms (and
/// immediately when woken), so a request can never be lost — even if a wake-up message is
/// swallowed by a modal loop such as a context menu.
struct Shared {
    desired: Mutex<Option<VdCfg>>,
    quit: Mutex<Option<Sender<()>>>,
}

#[derive(Clone)]
pub struct VDock { shared: Arc<Shared>, tid: Arc<AtomicU32> }

/// Dev aid (`--dev-ignore-fullscreen`): keep the dock visible over fullscreen apps for testing.
pub static IGNORE_FULLSCREEN: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// Dock corner radius at 96 DPI (a little softer than Windows 11's own 8 px).
const CORNER: f32 = 14.0;

const WM_MOUSE_LEAVE: u32 = 0x02A3;
const WM_WAKE: u32 = WM_APP + 21;
const WM_APPBAR: u32 = WM_APP + 22;
const T_POLL: usize = 1;
const T_ANIM: usize = 2;

impl VDock {
    pub fn start(state: Arc<Mutex<AppState>>) -> Self {
        let initial = VdCfg::from(&state.lock().unwrap().settings);
        let shared = Arc::new(Shared { desired: Mutex::new(Some(initial)), quit: Mutex::new(None) });
        let tid = Arc::new(AtomicU32::new(0));
        let (s2, t2) = (shared.clone(), tid.clone());
        std::thread::Builder::new().name("vire-dock".into())
            .spawn(move || unsafe { thread_main(s2, state, t2) }).expect("vire-dock thread");
        VDock { shared, tid }
    }
    fn wake(&self) {
        let t = self.tid.load(Ordering::SeqCst);
        if t != 0 { unsafe { let _ = PostThreadMessageW(t, WM_WAKE, WPARAM(0), LPARAM(0)); } }
    }
    pub fn apply(&self, s: &crate::model::Settings) {
        *self.shared.desired.lock().unwrap() = Some(VdCfg::from(s));
        self.wake();
    }
    /// Removes the dock and gives the Windows taskbar back (blocks briefly).
    pub fn shutdown(&self) {
        let (tx, rx) = channel();
        *self.shared.quit.lock().unwrap() = Some(tx);
        self.wake();
        let _ = rx.recv_timeout(Duration::from_secs(2));
    }
}

// ------------------------------------------------------------------ model

#[derive(Clone, Copy, PartialEq, Debug)]
enum Kind { Start, App, Sep, Clock, Control, Tray }

#[derive(Clone, PartialEq, Debug)]
struct Item {
    kind: Kind,
    key: String,
    name: String,
    launch: Option<String>,
    pinned: bool,
    wins: Vec<isize>,
    active: bool,
    /// Lower-cased exe path (pins: shortcut target; running apps: the window's process).
    exe: String,
}

/// Screen position of a dock item, on the side facing the screen centre.
/// Bottom: (x = icon centre, y = top of the dock). Left/Right: (x = dock edge, y = icon centre).
#[derive(Clone, Copy)]
struct Anchor { edge: Edge, x: i32, y: i32, mon: RECT }

struct Geo {
    s: f32,
    slot: f32,
    icon: f32,
    max_icon: f32,
    thick: f32,  // body thickness
    /// Reflection style: gap under the icons and length of the mirrored image (0 = glass style).
    refl_gap: f32,
    refl_len: f32,
    over: f32,   // room for magnified icons + tooltip (toward the screen centre)
    pad: f32,    // transparent room at both ends along the edge (tooltips near the ends)
    margin: f32,
    len: f32,    // along the edge
    lens: Vec<f32>,
    mon: RECT,
}

pub(crate) struct Surface { mem: HDC, dib: HBITMAP, old: HGDIOBJ, bits: *mut u32, w: i32, h: i32 }

impl Surface {
    pub(crate) unsafe fn new() -> Self { Surface { mem: CreateCompatibleDC(None), dib: HBITMAP::default(), old: HGDIOBJ::default(), bits: std::ptr::null_mut(), w: 0, h: 0 } }
    pub(crate) unsafe fn present(&mut self, hwnd: HWND, c: &Canvas, x: i32, y: i32) {
        if c.w != self.w || c.h != self.h || self.bits.is_null() {
            if !self.dib.is_invalid() { SelectObject(self.mem, self.old); let _ = DeleteObject(self.dib); }
            let mut bi = BITMAPINFO::default();
            bi.bmiHeader = BITMAPINFOHEADER { biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32, biWidth: c.w, biHeight: -c.h, biPlanes: 1, biBitCount: 32, ..Default::default() };
            let mut bits: *mut std::ffi::c_void = std::ptr::null_mut();
            let Ok(dib) = CreateDIBSection(self.mem, &bi, DIB_RGB_COLORS, &mut bits, None, 0) else { return };
            self.old = SelectObject(self.mem, dib);
            self.dib = dib; self.bits = bits as *mut u32; self.w = c.w; self.h = c.h;
        }
        std::ptr::copy_nonoverlapping(c.px.as_ptr(), self.bits, c.px.len());
        let pos = POINT { x, y };
        let size = SIZE { cx: c.w, cy: c.h };
        let src = POINT { x: 0, y: 0 };
        let blend = BLENDFUNCTION { BlendOp: 0, BlendFlags: 0, SourceConstantAlpha: 255, AlphaFormat: 1 };
        let _ = UpdateLayeredWindow(hwnd, None, Some(&pos), Some(&size), self.mem, Some(&src), COLORREF(0), Some(&blend), ULW_ALPHA);
    }
    unsafe fn free(&mut self) {
        if !self.dib.is_invalid() { SelectObject(self.mem, self.old); let _ = DeleteObject(self.dib); }
        let _ = DeleteDC(self.mem);
    }
}

struct Dock {
    cfg: VdCfg,
    app: Arc<Mutex<AppState>>,
    body: HWND,
    content: HWND,
    content_surf: Surface,
    shared: Option<Arc<Shared>>,
    /// True while Vire Dock has taken over from the Windows taskbar.
    active: bool,
    items: Vec<Item>,
    icons: HashMap<String, Option<Img>>,
    /// App icons are fetched by the shell, which can take long (slow disk, network, antivirus):
    /// done on short-lived worker threads so the dock never freezes waiting for one.
    icon_tx: Sender<(String, Option<Img>)>,
    icon_rx: std::sync::mpsc::Receiver<(String, Option<Img>)>,
    icon_wait: std::collections::HashSet<String>,
    /// Failed icon loads: (attempts, when to try again). After 3 misses it is loaded directly once.
    icon_tries: HashMap<String, (u8, std::time::Instant)>,
    win_keys: HashMap<isize, (String, String)>, // hwnd -> (key, exe path)
    pin_exe: HashMap<String, String>,           // shortcut path -> lower-cased target exe
    order: Vec<String>,                         // keys of unpinned running apps, in opening order
    hover: Option<(f32, f32)>,
    scales: Vec<f32>,
    reveal: f32,
    reveal_target: f32,
    leave_at: Option<Instant>,
    appbar: bool,
    tick: u32,
    clock: (String, String),
    fullscreen: bool,
    shown: bool,
    geo: Geo,
    anim: bool,
    menu_open: bool,
    body_drawn: Option<(i32, i32, Material)>,
    /// None = the main monitor (the "leader" dock, which also owns the config and the taskbar
    /// takeover); Some = a follower dock on another monitor.
    mon_rect: Option<RECT>,
}

// docks[0] is the leader (main monitor); the rest follow it, one per extra monitor.
thread_local! { static DOCKS: RefCell<Vec<Dock>> = const { RefCell::new(Vec::new()) }; }
thread_local! { static DOCK_ENV: std::cell::Cell<(isize, isize)> = const { std::cell::Cell::new((0, 0)) }; } // (owner hwnd, hinstance)


mod click;
mod draw;
mod items;
mod menus;
mod preview;
mod runtime;
mod tasks;
mod tbmirror;
mod winutil;
pub use menus::enable_dark_menus;
pub use runtime::emergency_restore;
pub use tasks::dump_background_apps;
pub use winutil::import_taskbar_pins;
use items::*;
use menus::*;
use runtime::*;
use tasks::*;
use winutil::*;

/// Debug trace, only when the VIRE_DEBUG environment variable is set.
pub(crate) fn dbg(msg: &str) {
    if std::env::var_os("VIRE_DEBUG").is_none() { return; }
    use std::io::Write;
    if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open(crate::model::data_dir().join("debug.log")) {
        let _ = writeln!(f, "{} {msg}", crate::model::now_secs());
    }
}

/// Dev aid: renders the dock (with the cursor resting on the 3rd app) over a sample backdrop
/// into a PNG, without creating windows or touching the real taskbar.
pub fn preview(app: Arc<Mutex<AppState>>, edge: Edge, material: Option<Material>, out: &std::path::Path) -> bool {
    unsafe {
        let mut d = new_dock(app.clone(), None, HWND::default(), HWND::default());
        let mut cfg = VdCfg::from(&app.lock().unwrap().settings);
        cfg.enabled = true;
        cfg.edge = edge;
        if let Some(m) = material { cfg.material = m; }
        if cfg.pins.is_empty() { cfg.pins = import_taskbar_pins(); }
        d.cfg = cfg;
        d.refresh_items(GetCurrentProcessId());
        d.layout();
        let idx = d.items.iter().enumerate().filter(|(_, it)| it.kind == Kind::App).nth(2).map(|(i, _)| i).unwrap_or(1);
        let along = d.base_center_screen(idx);
        let b = d.body_local();
        let (ox, oy, _, _) = d.content_rect(1.0);
        d.hover = Some(match edge { Edge::Bottom => (along, oy as f32 + b.cy()), _ => (ox as f32 + b.cx(), along) });
        for _ in 0..80 { d.animate(); }
        let (bx, by, bw, bh) = d.body_rect(1.0);
        let (cx, cy, cw, ch) = d.content_rect(1.0);
        let content = d.render_content();
        // Sample backdrop: the Cyber Sunset palette as a soft gradient.
        let pad = 40;
        let (w, h) = (cw + pad * 2, ch + pad * 2);
        let mut bg = Canvas::new(w, h);
        for y in 0..h { for x in 0..w {
            let t = y as f32 / h as f32;
            let u = x as f32 / w as f32;
            bg.blend(x, y, rgba((40.0 + 160.0 * t) as u8, (10.0 + 40.0 * u) as u8, (70.0 + 60.0 * (1.0 - t)) as u8, 1.0), 1.0);
        } }
        // Approximate the system blur/tint under the slab so the PNG resembles the real thing.
        let blur_tint = match d.cfg.material { Material::Glass => rgba(255, 255, 255, 0.10), Material::Acrylic => rgba(30, 30, 40, 0.45), Material::Mica => rgba(34, 30, 38, 0.80) };
        let (ox, oy) = (pad - cx, pad - cy);
        fill_rrect(&mut bg, R { x: (bx + ox) as f32, y: (by + oy) as f32, w: bw as f32, h: bh as f32 }, CORNER * d.geo.s, |_| blur_tint);
        for (src, sx, sy) in [(&content, pad, pad)] {
            for y in 0..src.h { for x in 0..src.w {
                let p = src.px[(y * src.w + x) as usize];
                let a = ((p >> 24) & 255) as f32 / 255.0;
                if a <= 0.0 { continue; }
                let col = rgba(((((p >> 16) & 255) as f32 / a).min(255.0)) as u8, ((((p >> 8) & 255) as f32 / a).min(255.0)) as u8, (((p & 255) as f32 / a).min(255.0)) as u8, 1.0);
                bg.blend(sx + x, sy + y, col, a);
            } }
        }
        crate::img::save_png(w as u32, h as u32, &bg.px, out)
    }
}
