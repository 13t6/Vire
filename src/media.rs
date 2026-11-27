//! Video import, metadata and thumbnails via the Windows Shell (no ffmpeg needed at runtime).

use crate::model::{data_dir, new_id, now_secs, WallpaperItem};
use std::path::{Path, PathBuf};

use windows::core::{Interface, HSTRING};
use windows::Win32::Foundation::SIZE;
use windows::Win32::Graphics::Gdi::*;
use windows::Win32::Storage::EnhancedStorage::{PKEY_Media_Duration, PKEY_Video_FrameHeight, PKEY_Video_FrameWidth};
use windows::Win32::System::Com::{CoInitializeEx, COINIT_APARTMENTTHREADED};
use windows::Win32::UI::Shell::*;

pub fn com_init_sta() {
    unsafe { let _ = CoInitializeEx(None, COINIT_APARTMENTTHREADED); }
}

fn shell_item(path: &Path) -> Option<IShellItem2> {
    unsafe { SHCreateItemFromParsingName(&HSTRING::from(path.as_os_str()), None).ok() }
}

/// (duration seconds, width, height)
pub fn probe(path: &Path) -> (f64, u32, u32) {
    let Some(item) = shell_item(path) else { return (0.0, 0, 0) };
    unsafe {
        let dur = item.GetUInt64(&PKEY_Media_Duration).map(|d| d as f64 / 10_000_000.0).unwrap_or(0.0);
        let w = item.GetUInt32(&PKEY_Video_FrameWidth).unwrap_or(0);
        let h = item.GetUInt32(&PKEY_Video_FrameHeight).unwrap_or(0);
        (dur, w, h)
    }
}

pub struct SongMeta { pub title: Option<String>, pub artist: String, pub album: String, pub duration: f64 }

/// Tags and length of an audio file, read with the shell's property system (MP3/FLAC/M4A/WMA/…
/// — whatever Windows has a handler for).
pub fn song_meta(path: &Path) -> SongMeta {
    use windows::Win32::Storage::EnhancedStorage::{PKEY_Music_AlbumTitle, PKEY_Music_Artist, PKEY_Title};
    let mut m = SongMeta { title: None, artist: String::new(), album: String::new(), duration: 0.0 };
    let Some(item) = shell_item(path) else { return m };
    let get = |k: &windows::Win32::UI::Shell::PropertiesSystem::PROPERTYKEY| -> Option<String> {
        unsafe { item.GetString(k).ok().and_then(|p| { let s = p.to_string().ok(); windows::Win32::System::Com::CoTaskMemFree(Some(p.0 as *const _)); s }) }
    };
    m.title = get(&PKEY_Title);
    m.artist = get(&PKEY_Music_Artist).unwrap_or_default();
    m.album = get(&PKEY_Music_AlbumTitle).unwrap_or_default();
    m.duration = unsafe { item.GetUInt64(&PKEY_Media_Duration) }.map(|d| d as f64 / 10_000_000.0).unwrap_or(0.0);
    m
}

/// Where a song's cover PNG is cached.
pub fn art_path(song: &Path) -> PathBuf {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for b in song.to_string_lossy().bytes() { h ^= b as u64; h = h.wrapping_mul(0x100_0000_01b3); }
    data_dir().join("Thumbs").join(format!("art_{h:x}.png"))
}

/// Cover art of a song (embedded picture, via the shell thumbnail) saved as a small PNG. Returns
/// None when the file has no art.
pub fn song_art(path: &Path, out: &Path) -> bool {
    if out.exists() { return true; }
    let Some(item) = shell_item(path) else { return false };
    unsafe {
        let Ok(factory) = item.cast::<IShellItemImageFactory>() else { return false };
        let Ok(hbmp) = factory.GetImage(SIZE { cx: 256, cy: 256 }, SIIGBF_BIGGERSIZEOK | SIIGBF_THUMBNAILONLY) else { return false };
        let ok = crate::img::save_thumbnail(hbmp, out, 256, 256);
        let _ = DeleteObject(hbmp);
        ok
    }
}

/// Lowers the calling thread's CPU and disk priority (for imports / thumbnails).
pub fn background_priority() {
    use windows::Win32::System::Threading::{GetCurrentThread, SetThreadPriority, THREAD_MODE_BACKGROUND_BEGIN};
    unsafe { let _ = SetThreadPriority(GetCurrentThread(), THREAD_MODE_BACKGROUND_BEGIN); }
}

/// Saves a 640x360 PNG thumbnail: Explorer's thumbnail handler first, and if Windows has none
/// for this file (common for 2K/4K or unusual encodes), a frame decoded with Media Foundation.
pub fn make_thumbnail(video: &Path, out: &Path) -> bool {
    shell_thumbnail(video, out) || unsafe { mf_thumbnail(video, out) }
}

/// Decodes the frame at ~1 s with the Source Reader (video processing on → RGB32 output).
unsafe fn mf_thumbnail(video: &Path, out: &Path) -> bool {
    use windows::Win32::Media::MediaFoundation::*;
    let run = || -> windows::core::Result<bool> {
        MFStartup(MF_VERSION, MFSTARTUP_LITE)?;
        let mut attrs: Option<IMFAttributes> = None;
        MFCreateAttributes(&mut attrs, 1)?;
        let attrs = attrs.unwrap();
        attrs.SetUINT32(&MF_SOURCE_READER_ENABLE_VIDEO_PROCESSING, 1)?;
        let reader = MFCreateSourceReaderFromURL(&HSTRING::from(video.as_os_str()), &attrs)?;
        let stream = MF_SOURCE_READER_FIRST_VIDEO_STREAM.0 as u32;
        let mt = MFCreateMediaType()?;
        mt.SetGUID(&MF_MT_MAJOR_TYPE, &MFMediaType_Video)?;
        mt.SetGUID(&MF_MT_SUBTYPE, &MFVideoFormat_RGB32)?;
        reader.SetCurrentMediaType(stream, None, &mt)?;
        let cur = reader.GetCurrentMediaType(stream)?;
        let size = cur.GetUINT64(&MF_MT_FRAME_SIZE)?;
        let (fw, fh) = ((size >> 32) as u32, (size & 0xFFFF_FFFF) as u32);
        let pos = windows::core::PROPVARIANT::from(10_000_000i64); // 1 s
        let _ = reader.SetCurrentPosition(&windows::core::GUID::zeroed(), &pos);
        for _ in 0..40 {
            let mut flags = 0u32;
            let mut sample: Option<IMFSample> = None;
            reader.ReadSample(stream, 0, None, Some(&mut flags), None, Some(&mut sample))?;
            if flags & MF_SOURCE_READERF_ENDOFSTREAM.0 as u32 != 0 { break; }
            let Some(sample) = sample else { continue };
            let buf = sample.ConvertToContiguousBuffer()?;
            let mut ptr: *mut u8 = std::ptr::null_mut();
            let mut len = 0u32;
            buf.Lock(&mut ptr, None, Some(&mut len))?;
            let stride = fw * 4;
            let ok = len >= stride * fh && crate::img::save_frame_thumbnail(std::slice::from_raw_parts(ptr, (stride * fh) as usize), fw, fh, stride, out, 640, 360);
            let _ = buf.Unlock();
            return Ok(ok);
        }
        Ok(false)
    };
    let ok = run().unwrap_or(false);
    let _ = windows::Win32::Media::MediaFoundation::MFShutdown();
    ok
}

fn shell_thumbnail(video: &Path, out: &Path) -> bool {
    let Some(item) = shell_item(video) else { return false };
    unsafe {
        let Ok(factory) = item.cast::<IShellItemImageFactory>() else { return false };
        let Ok(hbmp) = factory.GetImage(SIZE { cx: 640, cy: 360 }, SIIGBF_BIGGERSIZEOK | SIIGBF_THUMBNAILONLY) else { return false };
        // Centre-crop to 16:9 so cards line up, encode PNG with WIC.
        let ok = crate::img::save_thumbnail(hbmp, out, 640, 360);
        let _ = DeleteObject(hbmp);
        ok
    }
}

pub fn thumb_path_for(id: &str) -> PathBuf { data_dir().join("Thumbs").join(format!("{id}_640.png")) }

/// Cheap content fingerprint: size + FNV-1a of the first and last MiB.
pub fn fingerprint(p: &Path) -> Option<(u64, u64)> {
    use std::io::{Read, Seek, SeekFrom};
    let mut f = std::fs::File::open(p).ok()?;
    let size = f.metadata().ok()?.len();
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    let mut buf = vec![0u8; 1 << 20];
    let mut eat = |f: &mut std::fs::File, h: &mut u64| -> Option<()> {
        let n = f.read(&mut buf).ok()?;
        for b in &buf[..n] { *h ^= *b as u64; *h = h.wrapping_mul(0x100_0000_01b3); }
        Some(())
    };
    eat(&mut f, &mut h)?;
    if size > (2 << 20) { f.seek(SeekFrom::End(-(1 << 20))).ok()?; eat(&mut f, &mut h)?; }
    Some((size, h))
}

/// "butterfly girl starlight gaze moewalls com" → "Butterfly girl starlight gaze".
pub fn clean_title(stem: &str) -> String {
    let mut words: Vec<String> = stem.replace(['_', '-', '.'], " ").split_whitespace().map(str::to_owned).collect();
    let junk = |w: &str| matches!(w.to_lowercase().as_str(), "4k" | "8k" | "2k" | "1080p" | "1440p" | "2160p" | "720p" | "60fps" | "30fps" | "hd" | "uhd" | "live" | "wallpaper" | "free");
    // A trailing "<site> com/net/org" pair is a download-site tag.
    if words.len() > 2 && matches!(words.last().map(|w| w.to_lowercase()).as_deref(), Some("com" | "net" | "org" | "io" | "vn")) {
        words.truncate(words.len() - 2);
    }
    while words.len() > 1 && words.last().map_or(false, |w| junk(w)) { words.pop(); }
    let s = words.join(" ");
    let mut c = s.chars();
    match c.next() { Some(f) => f.to_uppercase().collect::<String>() + c.as_str(), None => "Video".into() }
}

pub enum Imported { New(WallpaperItem), Existing(String) }

#[cfg(test)]
mod tests {
    use super::clean_title;
    #[test]
    fn titles() {
        assert_eq!(clean_title("butterfly girl starlight gaze moewalls com"), "Butterfly girl starlight gaze");
        assert_eq!(clean_title("dark-cyberpunk-2077-city-street-moewalls-com"), "Dark cyberpunk 2077 city street");
        assert_eq!(clean_title("ocean_sunset_4k"), "Ocean sunset");
        assert_eq!(clean_title("My Clip"), "My Clip");
        assert_eq!(clean_title("4k"), "4k");
    }
}

/// Copies the video into Vire's own library folder (once — a video that is already in the
/// library is recognised by content and simply re-selected). Call off the UI thread.
pub fn import_video(src: &Path, existing: &[(String, PathBuf)]) -> Result<Imported, String> {
    com_init_sta();
    if !src.is_file() { return Err("Không tìm thấy tệp".into()); }
    if let Some(fp) = fingerprint(src) {
        for (id, path) in existing {
            if path == src || fingerprint(path) == Some(fp) { return Ok(Imported::Existing(id.clone())); }
        }
    }
    let id = new_id();
    let dest = copy_into(src, &crate::model::videos_dir()).map_err(|e| format!("Không sao chép được video: {e}"))?;
    let (dur, w, h) = probe(&dest);
    let thumb = thumb_path_for(&id);
    let thumb = if make_thumbnail(&dest, &thumb) { Some(thumb) } else { None };
    let title = src.file_stem().map(|s| clean_title(&s.to_string_lossy())).unwrap_or_else(|| "Video".into());
    Ok(Imported::New(WallpaperItem { id, title, path: dest, thumb, duration_secs: dur, width: w, height: h, built_in: false, date_added: now_secs(), music: None, music_title: None, music_more: Vec::new() }))
}

/// Size a `w`x`h` video needs so it still covers every connected screen at 1:1 (never larger than
/// the original). `None` when it is already within ~10% of that, or no screen is known.
pub fn shrink_target(w: u32, h: u32) -> Option<(u32, u32)> {
    if w == 0 || h == 0 { return None; }
    let k = crate::desktop::monitors().iter()
        .map(|m| (m.width().max(1) as f32 / w as f32).max(m.height().max(1) as f32 / h as f32))
        .fold(0.0f32, f32::max);
    if k <= 0.0 || k >= 0.9 { return None; }
    Some((((w as f32 * k).ceil() as u32 + 1) & !1, ((h as f32 * k).ceil() as u32 + 1) & !1))
}

/// Re-encodes the video next to itself at the screens' resolution. Returns the new file; the caller
/// deletes the old one. Call off the UI thread.
pub fn shrink_video(src: &Path, w: u32, h: u32) -> Option<PathBuf> {
    let (nw, nh) = shrink_target(w, h)?;
    let stem = src.file_stem()?.to_string_lossy().to_string();
    let base = stem.rsplit_once('_').filter(|(_, t)| t.ends_with('p') && t[..t.len() - 1].chars().all(|c| c.is_ascii_digit())).map_or(stem.clone(), |(b, _)| b.to_string());
    let small = src.with_file_name(format!("{base}_{}p.mp4", nh.min(nw)));
    if small == src { return None; }
    if downscale(src, &small, nw, nh) { Some(small) } else { let _ = std::fs::remove_file(&small); None }
}

fn downscale(src: &Path, dst: &Path, w: u32, h: u32) -> bool {
    // Blocking on a WinRT operation from an STA thread without a message pump never returns, so the
    // work runs on its own MTA thread.
    let (s, d) = (src.to_path_buf(), dst.to_path_buf());
    std::thread::spawn(move || {
        unsafe { let _ = CoInitializeEx(None, windows::Win32::System::Com::COINIT_MULTITHREADED); }
        downscale_mta(&s, &d, w, h)
    }).join().unwrap_or(false)
}

fn downscale_mta(src: &Path, dst: &Path, w: u32, h: u32) -> bool {
    use windows::{core::HSTRING, Media::MediaProperties::{MediaEncodingProfile, VideoEncodingQuality}, Media::Transcoding::MediaTranscoder, Storage::StorageFile};
    let run = || -> windows::core::Result<bool> {
        std::fs::File::create(dst).map_err(|_| windows::core::Error::from_win32())?;
        let s = StorageFile::GetFileFromPathAsync(&HSTRING::from(src.as_os_str()))?.get()?;
        let d = StorageFile::GetFileFromPathAsync(&HSTRING::from(dst.as_os_str()))?.get()?;
        let prof = MediaEncodingProfile::CreateMp4(VideoEncodingQuality::HD1080p)?;
        let v = prof.Video()?;
        v.SetWidth(w)?;
        v.SetHeight(h)?;
        v.SetBitrate(((10_000_000.0 * (w as f64 * h as f64) / (1920.0 * 1080.0)) as u32).max(2_000_000))?;
        let fr = v.FrameRate()?;
        fr.SetNumerator(60)?; fr.SetDenominator(1)?;
        let t = MediaTranscoder::new()?;
        let prep = t.PrepareFileTranscodeAsync(&s, &d, &prof)?.get()?;
        if !prep.CanTranscode()? { return Ok(false); }
        prep.TranscodeAsync()?.get()?;
        Ok(true)
    };
    run().unwrap_or(false)
}

/// Copies an audio file into Vire's Music folder (once — the same song is recognised by content
/// and reused) and returns (stored path, display title). Call off the UI thread.
pub fn import_music(src: &Path, existing: &[PathBuf]) -> Result<(PathBuf, String), String> {
    if !src.is_file() { return Err("Không tìm thấy tệp nhạc".into()); }
    let title = src.file_stem().map(|s| s.to_string_lossy().replace('_', " ").trim().to_string()).filter(|s| !s.is_empty()).unwrap_or_else(|| "Nhạc nền".into());
    if let Some(fp) = fingerprint(src) {
        for p in existing {
            if p == src || (p.exists() && fingerprint(p) == Some(fp)) { return Ok((p.clone(), title)); }
        }
    }
    let dest = copy_into(src, &crate::model::music_dir()).map_err(|e| format!("Không sao chép được nhạc: {e}"))?;
    Ok((dest, title))
}

/// Copies `src` into `dir` under its own name (" (2)" … if taken). Copies to a `.part` file first,
/// so a folder scan never sees a half-copied file.
fn copy_into(src: &Path, dir: &Path) -> std::io::Result<PathBuf> {
    let stem = src.file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_else(|| "file".into());
    let ext = src.extension().map(|e| e.to_string_lossy().to_ascii_lowercase()).unwrap_or_default();
    let mut dest = dir.join(format!("{stem}.{ext}"));
    let mut n = 2;
    while dest.exists() { dest = dir.join(format!("{stem} ({n}).{ext}")); n += 1; }
    let part = dest.with_extension(format!("{ext}.part"));
    std::fs::copy(src, &part)?;
    std::fs::rename(&part, &dest)?;
    Ok(dest)
}

/// Moves a file to the Recycle Bin (recoverable). Returns true on success.
pub fn recycle(path: &Path) -> bool {
    use std::os::windows::ffi::OsStrExt;
    let mut from: Vec<u16> = path.as_os_str().encode_wide().collect();
    from.extend([0, 0]); // double-NUL terminated list
    let mut op = SHFILEOPSTRUCTW {
        wFunc: FO_DELETE,
        pFrom: windows::core::PCWSTR(from.as_ptr()),
        fFlags: (FOF_ALLOWUNDO.0 | FOF_NOCONFIRMATION.0 | FOF_NOERRORUI.0 | FOF_SILENT.0) as u16,
        ..Default::default()
    };
    unsafe { SHFileOperationW(&mut op) == 0 && !op.fAnyOperationsAborted.as_bool() }
}
