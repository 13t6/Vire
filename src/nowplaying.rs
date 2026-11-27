//! "What is playing" as the desktop widget sees it, independent of where the sound comes from.
//!
//! Today there is one source — Vire's own local player. A later source (Windows media transport
//! controls: Spotify, YouTube Music in a browser, any Windows player) only has to implement
//! [`MediaSource`]; the widget asks every source in turn and shows the first one that has
//! something playing, and sends play/pause/next/seek back to that same source.

use crate::engine::Engine;
use crate::model::AppState;
use crate::player::PCmd;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

#[derive(Clone, PartialEq, Default)]
pub struct NowPlaying {
    /// The song file (local source only) — lets the lyrics find its `.lrc`.
    pub path: Option<PathBuf>,
    pub title: String,
    pub artist: String,
    /// Cover picture (PNG) when the source has one.
    pub art: Option<PathBuf>,
    pub pos: f64,
    pub dur: f64,
    pub playing: bool,
    /// None = this source has no volume control of its own.
    pub volume: Option<f32>,
}

pub enum Transport { PlayPause, Next, Prev, Seek(f64), Volume(f32) }

pub trait MediaSource {
    fn now_playing(&mut self) -> Option<NowPlaying>;
    fn control(&mut self, t: Transport);
}

/// Vire's own player (the songs in `Documents\Vire\Music`).
pub struct LocalSource {
    engine: Engine,
    state: Arc<Mutex<AppState>>,
    art_for: Option<PathBuf>,
    saved_for: Option<PathBuf>,
    last_save: std::time::Instant,
}

impl LocalSource {
    pub fn new(engine: Engine, state: Arc<Mutex<AppState>>) -> Self { LocalSource { engine, state, art_for: None, saved_for: None, last_save: std::time::Instant::now() } }
}

impl MediaSource for LocalSource {
    fn now_playing(&mut self) -> Option<NowPlaying> {
        let v = self.engine.player_view();
        let cur = v.current.clone()?;
        // Remember what you were listening to (and where), so it comes back after a restart.
        if self.saved_for.as_ref() != Some(&cur) || (v.playing && self.last_save.elapsed().as_secs() >= 15) {
            self.saved_for = Some(cur.clone());
            self.last_save = std::time::Instant::now();
            let mut st = self.state.lock().unwrap();
            // (Right after a restore the position is still catching up — don't overwrite it then.)
            if st.settings.pl_last.as_ref() != Some(&cur) || v.playing {
                st.settings.pl_last = Some(cur.clone());
                st.settings.pl_last_pos = if v.playing || st.settings.pl_last.as_ref() != Some(&cur) { v.pos } else { st.settings.pl_last_pos };
                st.save();
            }
        }
        // Tags of the playing song (the page that used to read them in the background is gone).
        { let mut st = self.state.lock().unwrap(); if st.song(&cur).map_or(false, |s| !s.meta_done) { st.backfill_meta(1); } }
        let (title, artist) = self.state.lock().unwrap().song(&cur)
            .map(|s| (s.title.clone(), s.artist.clone()))
            .unwrap_or_else(|| (cur.file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_default(), String::new()));
        // Cover art is cut out of the file once, off the widget thread.
        let png = crate::media::art_path(&cur);
        if !png.exists() && self.art_for.as_ref() != Some(&cur) {
            self.art_for = Some(cur.clone());
            let song = cur.clone();
            let out = png.clone();
            std::thread::spawn(move || { crate::media::com_init_sta(); let _ = crate::media::song_art(&song, &out); });
        }
        Some(NowPlaying { path: Some(cur.clone()), title, artist, art: png.exists().then_some(png), pos: v.pos, dur: v.dur, playing: v.playing, volume: Some(v.volume) })
    }

    fn control(&mut self, t: Transport) {
        match t {
            Transport::PlayPause => self.engine.player(PCmd::Toggle),
            Transport::Next => self.engine.player(PCmd::Next),
            Transport::Prev => self.engine.player(PCmd::Prev),
            Transport::Seek(s) => self.engine.player(PCmd::Seek(s)),
            Transport::Volume(v) => {
                self.engine.player(PCmd::Volume(v));
                let mut st = self.state.lock().unwrap();
                st.settings.pl_volume = v;
                st.save();
            }
        }
    }
}
