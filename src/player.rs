//! Music-page player: a play queue and one audio-only Media Foundation engine. It lives on the
//! engine thread, so music keeps playing (and moves on to the next track) with the window closed.
//! The UI only sends [`PCmd`]s and reads the [`PlayerView`] snapshot.

use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use windows::core::{Result, BSTR};
use windows::Win32::Media::MediaFoundation::*;

/// What the UI sees (published every tick).
#[derive(Clone, Default)]
pub struct PlayerView {
    pub current: Option<PathBuf>,
    pub pos: f64,
    pub dur: f64,
    pub playing: bool,
    /// Shared, rebuilt only when the queue changes (a 10 000-track queue isn't cloned every tick).
    pub queue: Arc<Vec<PathBuf>>,
    pub shuffle: bool,
    /// 0 = off, 1 = all, 2 = one.
    pub repeat: u8,
    pub volume: f32,
    pub error: Option<String>,
}

pub enum PCmd {
    SetQueue(Vec<PathBuf>, usize),
    /// A new playlist (the new wallpaper's songs): when the song playing now is in it, that song
    /// simply carries on (same position) and the list continues after it; otherwise start the list.
    Requeue(Vec<PathBuf>),
    /// Load a track paused at a position (restoring the last session).
    Restore(PathBuf, f64),
    Toggle,
    Pause,
    Next,
    Prev,
    Seek(f64),
    Volume(f32),
    Shuffle(bool),
    Repeat(u8),
}

pub struct Jukebox {
    media: Option<IMFMediaEngine>,
    queue: Vec<PathBuf>,
    index: usize,
    shuffle: bool,
    repeat: u8,
    vol: f32,
    wants_play: bool,
    pending_seek: Option<f64>,
    history: Vec<usize>,
    rng: u64,
    started: u64,
    error: Option<String>,
    pos: f64,
    dur: f64,
    last_tick: Instant,
    dirty: bool,
    queue_arc: Arc<Vec<PathBuf>>,
    queue_changed: bool,
    logged: u64,
    logged_pos: u64,
}

impl Jukebox {
    pub fn new() -> Self {
        let seed = SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_nanos() as u64).unwrap_or(1) | 1;
        Jukebox { media: None, queue: Vec::new(), index: 0, shuffle: false, repeat: 1, vol: 0.8, wants_play: false, pending_seek: None,
            history: Vec::new(), rng: seed, started: 0, error: None, pos: 0.0, dur: 0.0, last_tick: Instant::now(), dirty: true,
            queue_arc: Arc::new(Vec::new()), queue_changed: false, logged: 0, logged_pos: u64::MAX }
    }

    pub fn playing(&self) -> bool { self.media.is_some() && self.wants_play }

    pub fn has_media(&self) -> bool { self.media.is_some() }

    fn rand(&mut self) -> u64 { self.rng ^= self.rng << 13; self.rng ^= self.rng >> 7; self.rng ^= self.rng << 17; self.rng }

    pub fn shutdown(&mut self) {
        if let Some(m) = self.media.take() { unsafe { let _ = m.Shutdown(); } }
    }

    pub fn snapshot(&mut self) -> PlayerView {
        if std::mem::replace(&mut self.queue_changed, false) { self.queue_arc = Arc::new(self.queue.clone()); }
        PlayerView {
            current: self.queue.get(self.index).filter(|_| self.media.is_some()).cloned(),
            pos: self.pos, dur: self.dur, playing: self.playing(), queue: self.queue_arc.clone(),
            shuffle: self.shuffle, repeat: self.repeat, volume: self.vol, error: self.error.clone(),
        }
    }

    pub fn take_dirty(&mut self) -> bool { std::mem::replace(&mut self.dirty, false) }

    pub unsafe fn handle(&mut self, cmd: PCmd, factory: &IMFMediaEngineClassFactory, notify: &IMFMediaEngineNotify) {
        self.dirty = true;
        if matches!(cmd, PCmd::SetQueue(..) | PCmd::Requeue(..) | PCmd::Restore(..)) { self.queue_changed = true; }
        match cmd {
            PCmd::Requeue(q) => {
                let now = self.queue.get(self.index).filter(|_| self.media.is_some()).cloned();
                let at = now.and_then(|c| q.iter().position(|p| *p == c));
                self.queue = q;
                self.history.clear();
                match at {
                    Some(i) => self.index = i, // same song: keep playing it, no restart
                    None => { self.index = 0; self.load(true, factory, notify); }
                }
            }
            PCmd::SetQueue(q, i) => { self.queue = q; self.index = i.min(self.queue.len().saturating_sub(1)); self.history.clear(); self.load(true, factory, notify); }
            PCmd::Restore(p, pos) => {
                if self.queue.is_empty() { self.queue = vec![p]; self.index = 0; self.load(false, factory, notify); self.pending_seek = (pos > 1.0).then_some(pos); }
            }
            PCmd::Toggle => {
                if self.media.is_none() { if !self.queue.is_empty() { self.load(true, factory, notify); } }
                else if self.wants_play { self.wants_play = false; if let Some(m) = &self.media { let _ = m.Pause(); } }
                else { self.wants_play = true; if let Some(m) = &self.media { let _ = m.Play(); } }
            }
            PCmd::Pause => { self.wants_play = false; if let Some(m) = &self.media { let _ = m.Pause(); } }
            PCmd::Next => self.advance(false, factory, notify),
            PCmd::Prev => {
                if self.pos > 3.0 { if let Some(m) = &self.media { let _ = m.SetCurrentTime(0.0); } self.pos = 0.0; }
                else if !self.queue.is_empty() {
                    self.index = match self.history.pop() { Some(i) if i < self.queue.len() => i, _ => (self.index + self.queue.len() - 1) % self.queue.len() };
                    self.load(true, factory, notify);
                }
            }
            PCmd::Seek(t) => { if let Some(m) = &self.media { let _ = m.SetCurrentTime(t.max(0.0)); self.pos = t; } }
            PCmd::Volume(v) => {
                self.vol = v.clamp(0.0, 1.0);
                // The engine only takes a new volume while paused.
                if let Some(m) = &self.media { let _ = m.Pause(); let _ = m.SetVolume(self.vol as f64); if self.wants_play { let _ = m.Play(); } }
            }
            PCmd::Shuffle(s) => self.shuffle = s,
            PCmd::Repeat(r) => self.repeat = r.min(2),
        }
    }

    /// Loads queue[index] into a fresh audio-only engine.
    unsafe fn load(&mut self, play: bool, factory: &IMFMediaEngineClassFactory, notify: &IMFMediaEngineNotify) {
        self.shutdown();
        self.pos = 0.0;
        self.dur = 0.0;
        self.pending_seek = None;
        self.error = None;
        let Some(path) = self.queue.get(self.index).cloned() else { self.wants_play = false; return };
        if !path.exists() { self.error = Some("Không tìm thấy tệp nhạc (đã bị xoá hoặc di chuyển)".into()); self.wants_play = false; return; }
        let make = || -> Result<IMFMediaEngine> {
            let mut attrs: Option<IMFAttributes> = None;
            MFCreateAttributes(&mut attrs, 2)?;
            let attrs = attrs.unwrap();
            attrs.SetUnknown(&MF_MEDIA_ENGINE_CALLBACK, notify)?;
            let media = factory.CreateInstance(MF_MEDIA_ENGINE_AUDIOONLY.0 as u32, &attrs)?;
            media.SetLoop(false)?;
            media.SetAutoPlay(false)?;
            media.SetVolume(self.vol as f64)?;
            media.SetSource(&BSTR::from(path.to_string_lossy().as_ref()))?;
            Ok(media)
        };
        match make() {
            Ok(m) => { if play { let _ = m.Play(); } self.media = Some(m); self.wants_play = play; self.started += 1; }
            Err(e) => { self.error = Some(format!("Không mở được bài này: {e}")); self.wants_play = false; }
        }
    }

    /// Moves to another track: `auto` = the current one just ended.
    unsafe fn advance(&mut self, auto: bool, factory: &IMFMediaEngineClassFactory, notify: &IMFMediaEngineNotify) {
        let n = self.queue.len();
        if n == 0 { return; }
        if auto && self.repeat == 2 {
            if let Some(m) = &self.media { let _ = m.SetCurrentTime(0.0); let _ = m.Play(); }
            self.wants_play = true;
            return;
        }
        let next = if self.shuffle && n > 1 {
            let mut i = (self.rand() % n as u64) as usize;
            if i == self.index { i = (i + 1) % n; }
            Some(i)
        } else if self.index + 1 < n { Some(self.index + 1) }
        else if self.repeat == 1 || !auto { Some(0) }
        else { None };
        self.history.push(self.index);
        if self.history.len() > 200 { self.history.remove(0); }
        match next {
            Some(i) => { self.index = i; self.load(true, factory, notify); }
            None => { // end of the queue with repeat off: stay on the first track, paused
                self.index = 0;
                self.load(false, factory, notify);
            }
        }
    }

    /// Called often; does real work ~4×/s while a track is loaded.
    pub unsafe fn tick(&mut self, factory: &IMFMediaEngineClassFactory, notify: &IMFMediaEngineNotify) {
        let Some(m) = self.media.clone() else { return };
        if self.last_tick.elapsed() < Duration::from_millis(250) { return; }
        self.last_tick = Instant::now();
        let dur = m.GetDuration();
        if dur.is_finite() && dur > 0.0 && dur != self.dur {
            self.dur = dur;
            if let Some(t) = self.pending_seek.take() { let _ = m.SetCurrentTime(t.min(dur - 1.0).max(0.0)); }
        }
        let pos = m.GetCurrentTime();
        if pos.is_finite() { self.pos = pos; }
        if self.started != self.logged || (self.pos as u64) % 5 == 0 && self.pos as u64 != self.logged_pos {
            self.logged = self.started; self.logged_pos = self.pos as u64;
            crate::vdock::dbg(&format!("player: #{} {:?} pos={:.1}/{:.1} playing={} idx={}/{} err={:?}", self.started, self.queue.get(self.index).map(|p| p.file_name()), self.pos, self.dur, self.wants_play, self.index, self.queue.len(), self.error));
        }
        if m.GetError().is_ok() {
            self.error = Some("Windows không phát được định dạng/codec này".into());
            let was = self.wants_play;
            self.wants_play = false;
            // Keep a queue moving past a bad file.
            if was && self.queue.len() > 1 && self.index + 1 < self.queue.len() { self.advance(true, factory, notify); }
            self.dirty = true;
            return;
        }
        if m.IsEnded().as_bool() && self.wants_play { self.advance(true, factory, notify); }
        self.dirty = true;
    }
}
