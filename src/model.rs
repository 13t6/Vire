//! Settings + wallpaper library, persisted as JSON in %APPDATA%\DynamicWallpaper.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

pub const VIDEO_EXTS: &[&str] = &["mp4", "m4v", "mov", "wmv", "avi", "mkv", "webm"];

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum FitMode {
    /// Fill the screen, cropping edges (like AVLayerVideoGravity.resizeAspectFill).
    Fill,
    /// Show the whole video, black bars if needed.
    Fit,
}

/// Glass material used by the Vire window, Vire Dock and (where possible) Explorer / Settings.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Material { Acrylic, Mica, Glass }

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Edge { Bottom, Left, Right }

/// An app pinned to Vire Dock: a .lnk, an .exe, or a packaged app id (`shell:AppsFolder\…`).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Pin {
    /// Grouping key: AppUserModelID if known, else the lower-cased exe path.
    pub key: String,
    /// What to launch (lnk / exe path or `shell:AppsFolder\AUMID`).
    pub launch: String,
    pub name: String,
    /// Target exe (lower-cased) — lets a running window find its pin even when the window's
    /// AppUserModelID differs from the shortcut's.
    #[serde(default)]
    pub exe: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    pub is_muted: bool,
    pub volume: f32,
    pub playback_rate: f32,
    pub fit: FitMode,
    pub pause_on_battery: bool,
    pub pause_on_fullscreen: bool,
    /// Pause a monitor's video while a maximized window hides its desktop.
    pub pause_on_covered: bool,
    /// Also pause the video when a single window fills at least 85% of a monitor.
    pub pause_cover_85: bool,
    pub launch_at_login: bool,
    pub user_paused: bool,
    pub selected_id: Option<String>,
    /// true = same wallpaper on every monitor.
    pub sync_all: bool,
    /// monitor device name (\\.\DISPLAY1) -> wallpaper id.
    pub screen_map: HashMap<String, String>,
    /// System liquid glass (Windows Transparency effects + Acrylic on Explorer / Settings).
    pub sys_glass: bool,
    /// The user's Transparency value before Vire turned it on (restored when switched off).
    pub sys_glass_prev: Option<u32>,
    pub material: Material,
    /// Vire's own dock (replaces the Windows taskbar while on).
    pub vd_enabled: bool,
    pub vd_edge: Edge,
    pub vd_autohide: bool,
    pub vd_magnify: bool,
    pub vd_clock: bool,
    /// Dock look: 0 = glass slab, 1 = reflection (no plate, taller, icons mirrored under their feet).
    pub vd_style: u8,
    pub vd_pins: Vec<Pin>,
    pub vd_pins_imported: bool,
    /// Apps pinned on the Windows taskbar that the user unpinned *on the Dock*: hidden here only
    /// (Windows does not let another program change its taskbar pins).
    pub vd_unpinned: Vec<String>,
    /// Windows taskbar auto-hide state before Vire Dock took over.
    pub vd_prev_autohide: Option<bool>,
    /// Music page player: volume, shuffle, repeat (0 off, 1 all, 2 one), what was playing and where.
    pub pl_volume: f32,
    pub pl_shuffle: bool,
    pub pl_repeat: u8,
    pub pl_last: Option<PathBuf>,
    pub pl_last_pos: f64,
    /// Floating music widget on the desktop: shown, position (None = bottom-right), size step
    /// (0 small … 2 large), opacity, and whether it stays even when nothing is loaded.
    pub w_enabled: bool,
    pub w_pos: Option<(i32, i32)>,
    pub w_size: u8,
    pub w_opacity: f32,
    pub w_always: bool,
    /// EXPERIMENTAL: synced lyrics (.lrc next to the song) shown on the desktop.
    pub lyrics_on: bool,
    /// Dashboard look: 0 = Cyberpunk HUD, 1 = Cinematic Dark.
    pub ui_theme: u8,
    /// Cinematic look: sidebar folded down to icons.
    pub ui_compact: bool,
    /// false = no live wallpaper at all: the normal Windows wallpaper stays (for people who only
    /// want the Dock / widgets).
    pub live_wallpaper: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            is_muted: true,
            volume: 0.5,
            playback_rate: 1.0,
            fit: FitMode::Fill,
            pause_on_battery: false,
            pause_on_fullscreen: true,
            pause_on_covered: true,
            pause_cover_85: false,
            launch_at_login: false,
            user_paused: false,
            selected_id: None,
            sync_all: true,
            screen_map: HashMap::new(),
            sys_glass: false,
            sys_glass_prev: None,
            material: Material::Acrylic,
            vd_enabled: false,
            vd_edge: Edge::Bottom,
            vd_autohide: false,
            vd_magnify: true,
            vd_clock: true,
            vd_style: 0,
            vd_pins: Vec::new(),
            vd_pins_imported: false,
            vd_unpinned: Vec::new(),
            vd_prev_autohide: None,
            pl_volume: 0.8,
            pl_shuffle: false,
            pl_repeat: 1,
            pl_last: None,
            pl_last_pos: 0.0,
            w_enabled: true,
            w_pos: None,
            w_size: 1,
            w_opacity: 0.78,
            w_always: true,
            lyrics_on: false,
            ui_theme: 1, // Cinematic Dark for new installs
            ui_compact: false,
            live_wallpaper: true,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct WallpaperItem {
    pub id: String,
    pub title: String,
    pub path: PathBuf,
    pub thumb: Option<PathBuf>,
    pub duration_secs: f64,
    pub width: u32,
    pub height: u32,
    pub built_in: bool,
    pub date_added: u64,
    /// Optional background music played with this wallpaper (a copy inside Vire's Music folder).
    #[serde(default)]
    pub music: Option<PathBuf>,
    #[serde(default)]
    pub music_title: Option<String>,
    /// More songs of this wallpaper's own playlist (after music, which is the first).
    #[serde(default)]
    pub music_more: Vec<PathBuf>,
}

impl WallpaperItem {
    /// Forgets songs whose file is gone (the next one becomes the first). True when something changed.
    pub fn prune_music(&mut self) -> bool {
        let before = self.songs();
        let mut alive = before.clone();
        alive.retain(|p| p.exists());
        if alive.len() == before.len() { return false; }
        let mut it = alive.into_iter();
        self.music = it.next();
        self.music_more = it.collect();
        if self.music.is_none() { self.music_title = None; }
        true
    }
    /// The wallpaper's own playlist, in order.
    pub fn songs(&self) -> Vec<PathBuf> { self.music.iter().chain(self.music_more.iter()).cloned().collect() }
    /// "Title" or "Title +2" for the chips.
    pub fn music_label(&self) -> Option<String> {
        let t = self.music_title.clone()?;
        Some(if self.music_more.is_empty() { t } else { format!("{t} +{}", self.music_more.len()) })
    }

    pub fn resolution_text(&self) -> String {
        match self.width {
            0 => "Video Loop".into(),
            w if w >= 3840 => format!("4K UHD ({}×{})", self.width, self.height),
            w if w >= 2560 => format!("2K QHD ({}×{})", self.width, self.height),
            w if w >= 1920 => format!("Full HD ({}×{})", self.width, self.height),
            _ => format!("HD ({}×{})", self.width, self.height),
        }
    }

    pub fn duration_text(&self) -> String {
        let s = self.duration_secs.round() as u64;
        if s >= 60 { format!("{}:{:02}", s / 60, s % 60) } else { format!("{}s loop", s) }
    }
}

/// A song in the music library (a file in the Music folder). The Music page plays these, and any
/// wallpaper can pick one as its background music.
#[derive(Clone, Serialize, Deserialize)]
pub struct Song {
    pub path: PathBuf,
    pub title: String,
    #[serde(default)]
    pub artist: String,
    #[serde(default)]
    pub album: String,
    #[serde(default)]
    pub duration: f64,
    /// Tags were read (or tried) — don't read them again.
    #[serde(default)]
    pub meta_done: bool,
    /// In the "favourites" collection.
    #[serde(default)]
    pub liked: bool,
}

impl Song {
    pub fn new(path: PathBuf, title: String) -> Self {
        Song { path, title, artist: String::new(), album: String::new(), duration: 0.0, meta_done: false, liked: false }
    }
}

#[derive(Default, Serialize, Deserialize)]
pub struct Library {
    pub items: Vec<WallpaperItem>,
    #[serde(default)]
    pub songs: Vec<Song>,
}

pub struct AppState {
    pub settings: Settings,
    pub library: Library,
}

/// Where everything of Vire lives. **Portable by default:** a `Data` folder right next to
/// `Vire.exe` (wallpapers, music, settings, widgets, thumbnails). If that folder can't be written
/// (e.g. the app sits in `Program Files`), the classic places are used instead:
/// `%APPDATA%\Vire` for settings and `Documents\Vire` for wallpapers / music.
/// The release package never contains `Data`, so nothing personal can be published by accident.
fn portable_root() -> Option<&'static PathBuf> {
    static ROOT: std::sync::OnceLock<Option<PathBuf>> = std::sync::OnceLock::new();
    ROOT.get_or_init(|| {
        let d = exe_dir().join("Data");
        fs::create_dir_all(&d).ok()?;
        let probe = d.join(".write-test");
        fs::write(&probe, b"x").ok()?;
        let _ = fs::remove_file(&probe);
        Some(d)
    }).as_ref()
}

fn legacy_data_dir() -> PathBuf {
    std::env::var_os("APPDATA").map(PathBuf::from).unwrap_or_else(|| PathBuf::from(".")).join("Vire")
}

fn legacy_lib_root() -> PathBuf {
    use windows::Win32::UI::Shell::{SHGetKnownFolderPath, FOLDERID_Documents, KF_FLAG_DEFAULT};
    let docs = unsafe { SHGetKnownFolderPath(&FOLDERID_Documents, KF_FLAG_DEFAULT, None) }
        .ok()
        .and_then(|p| unsafe { p.to_string().ok() })
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("USERPROFILE").map(|u| PathBuf::from(u).join("Documents")))
        .unwrap_or_else(|| PathBuf::from("."));
    docs.join("Vire")
}

/// Settings, library list, thumbnails.
pub fn data_dir() -> PathBuf {
    let dir = portable_root().cloned().unwrap_or_else(legacy_data_dir);
    let _ = fs::create_dir_all(dir.join("Thumbs"));
    dir
}

/// Vire's visible folder: whatever you drop into `Wallpapers` / `Music` is loaded the next time
/// Vire starts (and whenever its window opens).
pub fn lib_root() -> PathBuf { portable_root().cloned().unwrap_or_else(legacy_lib_root) }

/// One-time move of an existing installation's data (from `Documents\Vire` and `%APPDATA%\Vire`)
/// into the `Data` folder next to the exe, rewriting the paths stored in the JSON files.
/// Safe to call on every start: it does nothing once `Data\.migrated` exists, and nothing at all
/// when the portable folder isn't usable.
pub fn migrate_to_portable() {
    let Some(root) = portable_root() else { return };
    migrate_from(root, &legacy_lib_root(), &legacy_data_dir());
}

fn migrate_from(root: &Path, old_lib: &Path, old_data: &Path) {
    let marker = root.join(".migrated");
    if marker.exists() { return; }
    // Move a folder (rename; copy + delete across drives).
    fn move_dir(from: &Path, to: &Path) {
        if !from.exists() || to.exists() && to.read_dir().map_or(false, |mut d| d.next().is_some()) { return; }
        let _ = fs::remove_dir(to); // an empty folder created earlier
        if fs::rename(from, to).is_ok() { return; }
        fn copy_all(a: &Path, b: &Path) -> std::io::Result<()> {
            fs::create_dir_all(b)?;
            for e in fs::read_dir(a)?.flatten() {
                let (src, dst) = (e.path(), b.join(e.file_name()));
                if src.is_dir() { copy_all(&src, &dst)?; } else { fs::copy(&src, &dst)?; }
            }
            Ok(())
        }
        if copy_all(from, to).is_ok() { let _ = fs::remove_dir_all(from); }
    }
    move_dir(&old_lib.join("Wallpapers"), &root.join("Wallpapers"));
    move_dir(&old_lib.join("Music"), &root.join("Music"));
    move_dir(&old_data.join("Thumbs"), &root.join("Thumbs"));
    for name in ["settings.json", "settings.json.bak", "library.json", "library.json.bak", "widgets.json"] {
        let (from, to) = (old_data.join(name), root.join(name));
        if from.exists() && !to.exists() && fs::rename(&from, &to).is_err() { if fs::copy(&from, &to).is_ok() { let _ = fs::remove_file(&from); } }
    }
    // The JSON files store absolute paths: point them at the new places.
    let esc = |p: &Path| p.to_string_lossy().replace('\\', "\\\\");
    let pairs = [(esc(old_lib), esc(root)), (esc(old_data), esc(root))];
    for name in ["settings.json", "settings.json.bak", "library.json", "library.json.bak", "widgets.json"] {
        let f = root.join(name);
        let Ok(text) = fs::read_to_string(&f) else { continue };
        let text = text.trim_start_matches('\u{feff}');
        let mut out = text.to_string();
        for (from, to) in &pairs { out = out.replace(from.as_str(), to.as_str()); }
        if out != text { let _ = fs::write(&f, out); }
    }
    let _ = fs::remove_dir(old_lib); // only if now empty
    let _ = fs::write(&marker, b"data moved next to the exe");
}
pub fn videos_dir() -> PathBuf { let d = lib_root().join("Wallpapers"); let _ = fs::create_dir_all(&d); d }
pub fn music_dir() -> PathBuf { let d = lib_root().join("Music"); let _ = fs::create_dir_all(&d); d }

/// Moves files of the old hidden folders (`%APPDATA%\Vire\Videos|Music`) into the visible ones and
/// repoints the library at them. Returns true if anything moved.
fn migrate_legacy(lib: &mut Library) -> bool {
    let mut changed = false;
    for (old, new) in [(data_dir().join("Videos"), videos_dir()), (data_dir().join("Music"), music_dir())] {
        let Ok(rd) = fs::read_dir(&old) else { continue };
        for e in rd.flatten() {
            let from = e.path();
            let Some(name) = from.file_name() else { continue };
            let mut to = new.join(name);
            if to.exists() { continue; }
            if fs::rename(&from, &to).is_err() {
                if fs::copy(&from, &to).is_ok() { let _ = fs::remove_file(&from); } else { to = from.clone(); }
            }
            if to != from {
                changed = true;
                for it in lib.items.iter_mut() {
                    if it.path == from { it.path = to.clone(); }
                    if it.music.as_ref() == Some(&from) { it.music = Some(to.clone()); }
                    for m in it.music_more.iter_mut().filter(|m| **m == from) { *m = to.clone(); }
                }
                for s in lib.songs.iter_mut().filter(|s| s.path == from) { s.path = to.clone(); }
            }
        }
        let _ = fs::remove_dir(&old); // only succeeds when empty
    }
    changed
}

/// Writes `data` so a crash / kill / second process can never leave a half-written file:
/// write a temp file, keep the previous good copy as `.bak`, then atomically replace.
fn write_atomic(path: &Path, data: &str) {
    let tmp = path.with_extension("json.tmp");
    if fs::write(&tmp, data).is_ok() {
        if path.exists() { let _ = fs::copy(path, path.with_extension("json.bak")); }
        if fs::rename(&tmp, path).is_err() { let _ = fs::write(path, data); let _ = fs::remove_file(&tmp); }
    } else {
        let _ = fs::write(path, data);
    }
}

/// Reads JSON (tolerating a BOM); if the main file is missing/corrupt, falls back to `.bak`.
fn read_json<T: serde::de::DeserializeOwned>(path: &Path) -> Option<T> {
    let one = |p: &Path| fs::read_to_string(p).ok().and_then(|s| serde_json::from_str::<T>(s.trim_start_matches('\u{feff}')).ok());
    one(path).or_else(|| one(&path.with_extension("json.bak")))
}

pub const AUDIO_EXTS: &[&str] = &["mp3", "wav", "m4a", "aac", "flac", "wma", "ogg"];

pub fn is_audio(p: &Path) -> bool {
    p.extension().and_then(|e| e.to_str()).map(|e| AUDIO_EXTS.contains(&e.to_ascii_lowercase().as_str())).unwrap_or(false)
}

/// Loads whatever sits in the `Wallpapers` / `Music` folders but is not in the library yet
/// (files you dropped there, or lost by an old library.json). Files are used where they are —
/// nothing is copied, renamed or deleted. Returns true if the library changed.
fn scan_folders(lib: &mut Library) -> bool {
    let mut changed = false;
    let known: std::collections::HashSet<PathBuf> = lib.items.iter().map(|i| i.path.clone()).collect();
    if let Ok(rd) = fs::read_dir(videos_dir()) {
        let mut files: Vec<PathBuf> = rd.flatten().map(|e| e.path()).filter(|p| is_video(p) && !known.contains(p)).collect();
        files.sort();
        for p in files {
            let (dur, w, h) = crate::media::probe(&p);
            let id = new_id();
            let title = p.file_stem().map(|s| crate::media::clean_title(&s.to_string_lossy())).unwrap_or_else(|| "Video".into());
            lib.items.push(WallpaperItem { id, title, path: p, thumb: None, duration_secs: dur, width: w, height: h, built_in: false, date_added: now_secs(), music: None, music_title: None, music_more: Vec::new() });
            changed = true;
        }
    }
    if let Ok(rd) = fs::read_dir(music_dir()) {
        let mut files: Vec<PathBuf> = rd.flatten().map(|e| e.path()).filter(|p| is_audio(p) && !lib.songs.iter().any(|s| &s.path == p)).collect();
        files.sort();
        for p in files {
            let title = p.file_stem().map(|s| s.to_string_lossy().replace('_', " ").trim().to_string()).filter(|s| !s.is_empty()).unwrap_or_else(|| "Nhạc nền".into());
            lib.songs.push(Song::new(p, title));
            changed = true;
        }
    }
    changed
}

pub fn exe_dir() -> PathBuf {
    std::env::current_exe().ok().and_then(|p| p.parent().map(Path::to_path_buf)).unwrap_or_else(|| PathBuf::from("."))
}

pub fn now_secs() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0)
}

pub fn new_id() -> String {
    let n = SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_nanos()).unwrap_or(0);
    format!("{:x}", n)
}

pub fn is_video(p: &Path) -> bool {
    p.extension().and_then(|e| e.to_str()).map(|e| VIDEO_EXTS.contains(&e.to_ascii_lowercase().as_str())).unwrap_or(false)
}

/// Built-in presets shipped in `<exe>\samples`.
const BUILT_INS: &[(&str, &str)] = &[
    ("cyber_sunset_grid.mp4", "Cyber Sunset Grid"),
];

impl AppState {
    pub fn load() -> Self {
        migrate_to_portable();
        let dir = data_dir();
        let settings: Settings = read_json(&dir.join("settings.json")).unwrap_or_default();
        let mut library: Library = read_json(&dir.join("library.json")).unwrap_or_default();
        // Only write back at start-up when something actually changed (a read-only start never
        // touches the files, so it can't race with another process).
        let mut dirty = migrate_legacy(&mut library);

        // Drop entries whose file vanished, then (re)register built-ins.
        let before = library.items.len();
        library.items.retain(|i| i.path.exists());
        dirty |= library.items.len() != before;
        // Older imports kept download-site tags in the name ("… moewalls com"): tidy those once.
        for it in library.items.iter_mut().filter(|i| !i.built_in) {
            let low = it.title.to_lowercase();
            if [" com", " net", " org"].iter().any(|t| low.ends_with(t)) { it.title = crate::media::clean_title(&it.title); dirty = true; }
            dirty |= it.prune_music();
        }
        let samples = exe_dir().join("samples");
        for (i, (file, title)) in BUILT_INS.iter().enumerate() {
            let path = samples.join(file);
            if !path.exists() { continue; }
            if let Some(existing) = library.items.iter_mut().find(|it| it.built_in && it.path.file_name() == path.file_name()) {
                if existing.path != path { existing.path = path; dirty = true; } // exe may have moved
                continue;
            }
            library.items.insert(i.min(library.items.len()), WallpaperItem {
                id: format!("builtin-{}", i),
                title: title.to_string(),
                path,
                thumb: None,
                duration_secs: 12.0,
                width: 1920,
                height: 1080,
                built_in: true,
                date_added: 0,
                music: None,
                music_title: None,
                music_more: Vec::new(),
            });
            dirty = true;
        }
        // Song list: forget missing files; songs attached by older versions join the list.
        let n = library.songs.len();
        library.songs.retain(|s| s.path.exists());
        dirty |= library.songs.len() != n;
        let attached: Vec<(PathBuf, String)> = library.items.iter().flat_map(|i| i.songs().into_iter().map(|m| (m, i.music_title.clone().unwrap_or_else(|| "Nhạc nền".into())))).collect();
        for (path, title) in attached {
            if !library.songs.iter().any(|s| s.path == path) { library.songs.push(Song::new(path, title)); dirty = true; }
        }
        dirty |= scan_folders(&mut library);

        let mut st = Self { settings, library };
        if st.settings.selected_id.as_ref().map_or(true, |id| st.item(id).is_none()) {
            st.settings.selected_id = st.library.items.first().map(|i| i.id.clone());
            dirty = true;
        }
        if dirty { st.save(); }
        st
    }

    /// Picks up files newly dropped into the Wallpapers / Music folders and forgets deleted ones.
    /// Returns true when the library changed (the caller then saves / refreshes).
    pub fn rescan(&mut self) -> bool {
        let (ni, ns) = (self.library.items.len(), self.library.songs.len());
        self.library.items.retain(|i| i.path.exists());
        self.library.songs.retain(|s| s.path.exists());
        for it in self.library.items.iter_mut() {
            it.prune_music();
        }
        let gone = ni != self.library.items.len() || ns != self.library.songs.len();
        let found = scan_folders(&mut self.library);
        if self.settings.selected_id.as_ref().map_or(true, |id| self.item(id).is_none()) {
            self.settings.selected_id = self.library.items.first().map(|i| i.id.clone());
        }
        gone || found
    }

    pub fn save(&self) {
        let dir = data_dir();
        if let Ok(s) = serde_json::to_string_pretty(&self.settings) { write_atomic(&dir.join("settings.json"), &s); }
        if let Ok(s) = serde_json::to_string_pretty(&self.library) { write_atomic(&dir.join("library.json"), &s); }
    }

    /// Paths of every song in the list (used to recognise a song that was already added).
    pub fn music_paths(&self) -> Vec<PathBuf> {
        self.library.songs.iter().map(|s| s.path.clone()).collect()
    }

    /// Adds a song to the list (once) and returns its list title.
    pub fn add_song(&mut self, path: PathBuf, title: String) -> String {
        if let Some(s) = self.library.songs.iter().find(|s| s.path == path) { return s.title.clone(); }
        self.library.songs.push(Song::new(path, title.clone()));
        title
    }

    /// Reads tags (title / artist / album / length) of up to `n` songs that haven't been read yet.
    /// Called a few at a time from the UI, so a big library fills in without freezing anything.
    pub fn backfill_meta(&mut self, n: usize) -> bool {
        let mut did = false;
        for s in self.library.songs.iter_mut().filter(|s| !s.meta_done).take(n) {
            let m = crate::media::song_meta(&s.path);
            if let Some(t) = m.title.filter(|t| !t.trim().is_empty()) { s.title = t; }
            s.artist = m.artist;
            s.album = m.album;
            s.duration = m.duration;
            s.meta_done = true;
            did = true;
        }
        // Wallpaper music chips show the song's title: keep them in step.
        if did {
            let titles: Vec<(PathBuf, String)> = self.library.songs.iter().map(|s| (s.path.clone(), s.title.clone())).collect();
            for it in self.library.items.iter_mut() {
                if let Some(m) = &it.music { if let Some((_, t)) = titles.iter().find(|(p, _)| p == m) { it.music_title = Some(t.clone()); } }
            }
        }
        did
    }

    pub fn pending_meta(&self) -> usize { self.library.songs.iter().filter(|s| !s.meta_done).count() }

    pub fn song(&self, path: &Path) -> Option<&Song> { self.library.songs.iter().find(|s| s.path == path) }


    /// Adds the song at `path` (which must be in the list) to a wallpaper's own playlist.
    pub fn add_music(&mut self, id: &str, path: &Path) {
        let Some(song) = self.library.songs.iter().find(|s| s.path == path).cloned() else { return };
        if let Some(it) = self.library.items.iter_mut().find(|i| i.id == id) {
            if it.songs().iter().any(|p| p == path) { return; }
            if it.music.is_none() { it.music = Some(song.path); it.music_title = Some(song.title); } else { it.music_more.push(song.path); }
        }
    }

    /// Adds the song to the wallpaper's playlist, or takes it out when it is already there.
    pub fn toggle_music(&mut self, id: &str, path: &Path) {
        let titles: Vec<(PathBuf, String)> = self.library.songs.iter().map(|s| (s.path.clone(), s.title.clone())).collect();
        let on = self.item(id).map_or(false, |i| i.songs().iter().any(|p| p == path));
        if !on { self.add_music(id, path); return; }
        if let Some(it) = self.library.items.iter_mut().find(|i| i.id == id) {
            let mut list = it.songs();
            list.retain(|p| p != path);
            let mut iter = list.into_iter();
            it.music = iter.next();
            it.music_more = iter.collect();
            it.music_title = it.music.as_ref().and_then(|m| titles.iter().find(|(p, _)| p == m).map(|(_, t)| t.clone()));
        }
    }

    /// Detaches all the music from a wallpaper (the songs stay in the list).
    pub fn clear_music(&mut self, id: &str) {
        if let Some(it) = self.library.items.iter_mut().find(|i| i.id == id) { it.music = None; it.music_title = None; it.music_more.clear(); }
    }

    pub fn item(&self, id: &str) -> Option<&WallpaperItem> {
        self.library.items.iter().find(|i| i.id == id)
    }

    pub fn selected(&self) -> Option<&WallpaperItem> {
        self.settings.selected_id.as_deref().and_then(|id| self.item(id))
    }

    /// Wallpaper shown on a given monitor.
    pub fn item_for_monitor(&self, device: &str) -> Option<&WallpaperItem> {
        if !self.settings.sync_all {
            if let Some(item) = self.settings.screen_map.get(device).and_then(|id| self.item(id)) { return Some(item); }
        }
        self.selected()
    }

    /// Wallpaper path to show on a given monitor.
    pub fn path_for_monitor(&self, device: &str) -> Option<PathBuf> {
        self.item_for_monitor(device).map(|i| i.path.clone())
    }

    pub fn select_next(&mut self) {
        let items = &self.library.items;
        if items.is_empty() { return; }
        let idx = self.settings.selected_id.as_deref()
            .and_then(|id| items.iter().position(|i| i.id == id))
            .map(|p| (p + 1) % items.len()).unwrap_or(0);
        self.settings.selected_id = Some(items[idx].id.clone());
    }

    pub fn remove(&mut self, id: &str) {
        if let Some(pos) = self.library.items.iter().position(|i| i.id == id && !i.built_in) {
            let item = self.library.items.remove(pos);
            // The file leaves the Wallpapers folder (Recycle Bin), or it would be re-loaded on the next scan.
            if item.path.starts_with(videos_dir()) { crate::media::recycle(&item.path); }
            if let Some(t) = item.thumb { let _ = fs::remove_file(t); }
            self.settings.screen_map.retain(|_, v| v != id);
            if self.settings.selected_id.as_deref() == Some(id) {
                self.settings.selected_id = self.library.items.first().map(|i| i.id.clone());
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn item() -> WallpaperItem {
        WallpaperItem { id: "w".into(), title: "w".into(), path: PathBuf::from("w.mp4"), thumb: None, duration_secs: 1.0, width: 1, height: 1, built_in: false, date_added: 0, music: None, music_title: None, music_more: Vec::new() }
    }
    #[test]
    fn playlist_toggle() {
        let mut st = AppState { settings: Settings::default(), library: Library::default() };
        st.library.items.push(item());
        for n in ["a", "b", "c"] { st.library.songs.push(Song::new(PathBuf::from(format!("{n}.mp3")), n.to_string())); }
        let pa = PathBuf::from("a.mp3"); let pb = PathBuf::from("b.mp3"); let pc = PathBuf::from("c.mp3");
        st.toggle_music("w", &pa); st.toggle_music("w", &pb); st.toggle_music("w", &pc);
        assert_eq!(st.item("w").unwrap().songs(), vec![pa.clone(), pb.clone(), pc.clone()]);
        assert_eq!(st.item("w").unwrap().music_label().as_deref(), Some("a +2"));
        st.toggle_music("w", &pa); // remove the first: b becomes the first
        assert_eq!(st.item("w").unwrap().songs(), vec![pb.clone(), pc.clone()]);
        assert_eq!(st.item("w").unwrap().music_title.as_deref(), Some("b"));
        st.toggle_music("w", &pb); st.toggle_music("w", &pc);
        assert!(st.item("w").unwrap().songs().is_empty() && st.item("w").unwrap().music_title.is_none());
    }
}
#[cfg(test)]
mod migrate_tests {
    use super::*;

    #[test]
    fn moves_files_and_rewrites_paths() {
        let base = std::env::temp_dir().join(format!("vire_mig_{}", std::process::id()));
        let _ = fs::remove_dir_all(&base);
        let (old_lib, old_data, root) = (base.join("Documents").join("Vire"), base.join("Roaming").join("Vire"), base.join("app").join("Data"));
        fs::create_dir_all(old_lib.join("Wallpapers")).unwrap();
        fs::create_dir_all(old_lib.join("Music")).unwrap();
        fs::create_dir_all(old_data.join("Thumbs")).unwrap();
        fs::create_dir_all(&root).unwrap();
        fs::write(old_lib.join("Wallpapers").join("a.mp4"), b"video").unwrap();
        fs::write(old_lib.join("Music").join("b.mp3"), b"song").unwrap();
        fs::write(old_data.join("Thumbs").join("t.png"), b"png").unwrap();
        let esc = |p: &Path| p.to_string_lossy().replace('\\', "\\\\");
        fs::write(old_data.join("library.json"), format!("{{\"path\":\"{}\\\\Wallpapers\\\\a.mp4\",\"thumb\":\"{}\\\\Thumbs\\\\t.png\"}}", esc(&old_lib), esc(&old_data))).unwrap();
        migrate_from(&root, &old_lib, &old_data);
        assert_eq!(fs::read(root.join("Wallpapers").join("a.mp4")).unwrap(), b"video");
        assert_eq!(fs::read(root.join("Music").join("b.mp3")).unwrap(), b"song");
        assert_eq!(fs::read(root.join("Thumbs").join("t.png")).unwrap(), b"png");
        let lib = fs::read_to_string(root.join("library.json")).unwrap();
        assert!(lib.contains(&esc(&root)) && !lib.contains(&esc(&old_lib)) && !lib.contains(&esc(&old_data)), "paths rewritten: {lib}");
        assert!(!old_lib.join("Wallpapers").exists(), "old folder moved, not copied");
        assert!(root.join(".migrated").exists());
        // second run changes nothing
        fs::write(old_lib.join("x.txt"), b"later").ok();
        migrate_from(&root, &old_lib, &old_data);
        let _ = fs::remove_dir_all(&base);
    }
}