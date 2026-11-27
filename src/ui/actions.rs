//! Things the dashboard does: import videos / songs, music attach, background results, textures.

use super::*;
use std::path::PathBuf;

impl App {
    pub(super) fn generate_missing_thumbs(&self) {
        let todo: Vec<(String, PathBuf)> = self.state.lock().unwrap().library.items.iter()
            .filter(|i| i.thumb.as_ref().map_or(true, |t| !t.exists() || !t.to_string_lossy().ends_with("_640.png")))
            .map(|i| (i.id.clone(), i.path.clone())).collect();
        if todo.is_empty() { return; }
        let tx = self.bg_tx.clone();
        std::thread::spawn(move || {
            media::com_init_sta();
            media::background_priority();
            for (id, path) in todo {
                let out = media::thumb_path_for(&id);
                // Reuse a thumbnail that is already on disk; only build missing ones.
                let ok = out.exists() || media::make_thumbnail(&path, &out);
                let _ = tx.send(BgResult::Thumb(id, ok.then_some(out)));
            }
        });
    }

    pub(super) fn start_import(&mut self, paths: Vec<PathBuf>, ctx: &egui::Context) {
        let paths: Vec<PathBuf> = paths.into_iter().filter(|p| is_video(p)).collect();
        if paths.is_empty() { return; }
        self.importing += paths.len();
        let mut existing: Vec<(String, PathBuf)> = self.state.lock().unwrap().library.items.iter().map(|i| (i.id.clone(), i.path.clone())).collect();
        let (tx, c) = (self.bg_tx.clone(), ctx.clone());
        // One worker, one file at a time: the same video picked twice is copied only once.
        std::thread::spawn(move || {
            media::background_priority(); // copying a big file must not make playback stutter
            for p in paths {
                let r = media::import_video(&p, &existing);
                if let Ok(media::Imported::New(it)) = &r { existing.push((it.id.clone(), it.path.clone())); }
                let _ = tx.send(BgResult::Imported(r));
                c.request_repaint();
            }
        });
    }

    /// Applies a choice from the music list to wallpaper `id`. Returns true when settings changed.
    pub(super) fn apply_music(&mut self, st: &mut AppState, id: &str, act: MusicAct) -> bool {
        match act {
            MusicAct::None => return false,
            MusicAct::Add => { self.music_req = Some(id.to_string()); return false; }
            MusicAct::Toggle(p) => { st.toggle_music(id, &p); st.settings.is_muted = false; }
            MusicAct::Clear => st.clear_music(id),
        }
        true
    }

    /// The open dialog runs on its own thread: showing it inside the UI frame would nest a message
    /// loop in the window procedure and freeze the window ("not responding").
    pub(super) fn pick_files(&mut self, ctx: &egui::Context) { self.pick_dialog(ctx, None); }

    pub(super) fn pick_music(&mut self, ctx: &egui::Context, id: String) { self.pick_dialog(ctx, Some(id)); }

    /// "Thêm nhạc" on the Music page: any number of songs, into the library only.
    pub(super) fn pick_dialog_songs(&mut self, ctx: &egui::Context) { self.pick_dialog(ctx, Some(String::new())); }

    /// Copies songs into the Music folder (one worker, off the UI thread) and adds them to the list.
    pub(super) fn import_songs(&mut self, ctx: &egui::Context, files: Vec<PathBuf>) {
        let files: Vec<PathBuf> = files.into_iter().filter(|p| crate::model::is_audio(p)).collect();
        if files.is_empty() { return; }
        self.importing += files.len();
        let mut existing = self.state.lock().unwrap().music_paths();
        let (tx, c) = (self.bg_tx.clone(), ctx.clone());
        std::thread::spawn(move || {
            media::background_priority();
            for f in files {
                let r = media::import_music(&f, &existing);
                if let Ok((p, _)) = &r { existing.push(p.clone()); }
                let _ = tx.send(BgResult::Music(String::new(), r));
                c.request_repaint();
            }
        });
    }

    pub(super) fn pick_dialog(&mut self, ctx: &egui::Context, song_for: Option<String>) {
        let owner = unsafe { FindWindowW(None, &HSTRING::from(APP_TITLE)).unwrap_or_default() }.0 as isize;
        let (tx, c) = (self.bg_tx.clone(), ctx.clone());
        std::thread::spawn(move || {
            media::com_init_sta();
            let owner = HWND(owner as *mut _);
            let files = match &song_for {
                Some(id) if id.is_empty() => crate::img::pick_audios(owner),
                Some(_) => crate::img::pick_audios(owner),
                None => crate::img::pick_videos(owner),
            };
            let _ = tx.send(BgResult::Picked(song_for, files));
            c.request_repaint();
        });
    }

    /// Copies the song into Vire's Music folder (off the UI thread) and attaches it.
    pub(super) fn attach_music(&mut self, ctx: &egui::Context, id: String, file: PathBuf) {
        self.importing += 1;
        let existing = self.state.lock().unwrap().music_paths();
        let (tx, c) = (self.bg_tx.clone(), ctx.clone());
        std::thread::spawn(move || {
            let _ = tx.send(BgResult::Music(id, media::import_music(&file, &existing)));
            c.request_repaint();
        });
    }

    /// Re-encodes one library video to the connected screens' resolution, in the background, after
    /// the user said yes (a 4K decoder costs ~150 MB of RAM for pixels the screen cannot show).
    pub(super) fn shrink_one(&mut self, ctx: &egui::Context, id: String, w: u32, h: u32, path: PathBuf) {
        self.shrinking += 1;
        let (tx, c) = (self.bg_tx.clone(), ctx.clone());
        std::thread::spawn(move || {
            media::background_priority();
            media::com_init_sta();
            let r = match media::shrink_video(&path, w, h) {
                Some(new) => { let (_, nw, nh) = media::probe(&new); Some((new, nw, nh)) }
                None => None,
            };
            let _ = tx.send(BgResult::Shrunk(id, r.map(|(n, a, b)| (n, a, b)), path));
            c.request_repaint();
        });
    }

    pub(super) fn poll_background(&mut self) -> bool {
        let mut changed = false;
        while let Ok(r) = self.bg_rx.try_recv() {
            let mut st = self.state.lock().unwrap();
            match r {
                BgResult::Shrunk(_, None, _) => {
                    self.shrinking = self.shrinking.saturating_sub(1);
                    self.error = Some("Không nén được video này — giữ nguyên bản gốc".into());
                }
                BgResult::Shrunk(id, Some((new, w, h)), old) => {
                    self.shrinking = self.shrinking.saturating_sub(1);
                    if let Some(it) = st.library.items.iter_mut().find(|i| i.id == id) { it.path = new; it.width = w; it.height = h; }
                    st.save();
                    self.notice = Some("Đã nén video theo độ phân giải màn hình".into());
                    changed = true;
                    // The old file may still be open for a moment: retry the delete for a while.
                    std::thread::spawn(move || for _ in 0..30 { if std::fs::remove_file(&old).is_ok() || !old.exists() { break; } std::thread::sleep(std::time::Duration::from_secs(2)); });
                }
                BgResult::Imported(Ok(media::Imported::New(item))) => {
                    self.importing = self.importing.saturating_sub(1);
                    st.settings.selected_id = Some(item.id.clone());
                    st.settings.live_wallpaper = true;
                    if media::shrink_target(item.width, item.height).is_some() { self.shrink_ask.push(item.id.clone()); }
                    st.library.items.push(item);
                    self.notice = Some("Đã lưu video vào thư viện của Vire".into());
                    changed = true;
                }
                BgResult::Imported(Ok(media::Imported::Existing(id))) => {
                    self.importing = self.importing.saturating_sub(1);
                    st.settings.selected_id = Some(id);
                    st.settings.live_wallpaper = true;
                    self.notice = Some("Video này đã có trong thư viện — đã chọn lại, không chép thêm".into());
                    changed = true;
                }
                BgResult::Imported(Err(e)) => { self.importing = self.importing.saturating_sub(1); self.error = Some(e); }
                BgResult::Music(id, Ok((path, title))) => {
                    self.importing = self.importing.saturating_sub(1);
                    let title = st.add_song(path.clone(), title);
                    if id.is_empty() {
                        // Added from the Music page: library only, no wallpaper involved.
                        self.mp.note(format!("Đã thêm “{title}”"));
                    } else {
                        st.add_music(&id, &path);
                        let was_muted = st.settings.is_muted;
                        st.settings.is_muted = false; // music you just added should be heard
                        self.notice = Some(format!("Đã thêm nhạc “{title}”{}", if was_muted { " và bật tiếng" } else { "" }));
                        changed = true;
                    }
                    st.save();
                }
                BgResult::Music(_, Err(e)) => { self.importing = self.importing.saturating_sub(1); self.error = Some(e); }
                BgResult::Picked(song_for, files) => self.picked.push((song_for, files)),                BgResult::Thumb(id, thumb) => {
                    if let Some(it) = st.library.items.iter_mut().find(|i| i.id == id) { it.thumb = thumb; }
                    self.textures.remove(&id);
                    st.save();
                }
            }
        }
        changed
    }

    pub(super) fn texture_for(&mut self, ctx: &egui::Context, item: &WallpaperItem) -> Option<TextureHandle> {
        if let Some(t) = self.textures.get(&item.id) { return Some(t.clone()); }
        let bytes = std::fs::read(item.thumb.as_ref()?).ok()?;
        let tex = load_png_texture(ctx, &item.id, &bytes)?;
        self.textures.insert(item.id.clone(), tex.clone());
        Some(tex)
    }

    /// Thumbnail (or a gradient placeholder) with rounded corners + bottom shade.
    pub(super) fn paint_media(&mut self, ui: &mut Ui, ctx: &egui::Context, item: Option<&WallpaperItem>, rect: Rect, _r: f32, shade_from: f32) {
        // Sharp HUD corners (the chamfer is cut by the caller); soft rounded ones in Cinematic.
        let rounding = if hud::soft() { Rounding::same(12.0) } else { Rounding::ZERO };
        match item.and_then(|i| self.texture_for(ctx, i)) {
            Some(tex) => {
                // "Cover" fit: crop the centre of the image to the frame's aspect — never stretch.
                let [tw, th] = tex.size();
                let (ta, ra) = (tw as f32 / th.max(1) as f32, rect.width() / rect.height().max(1.0));
                let uv = if ta > ra {
                    let k = ra / ta; Rect::from_min_max(pos2((1.0 - k) / 2.0, 0.0), pos2((1.0 + k) / 2.0, 1.0))
                } else {
                    let k = ta / ra; Rect::from_min_max(pos2(0.0, (1.0 - k) / 2.0), pos2(1.0, (1.0 + k) / 2.0))
                };
                egui::Image::new(&tex).uv(uv).rounding(rounding).paint_at(ui, rect);
            }
            None => { ui.painter().rect_filled(rect, rounding, Color32::from_rgb(40, 46, 66)); }
        }
        let shade = Rect::from_min_max(pos2(rect.left(), rect.top() + rect.height() * shade_from), rect.max);
        egui::Image::new(&self.shade).rounding(Rounding::ZERO).paint_at(ui, shade);
    }
}
