//! Image decode/encode + file picker through components every Windows already ships
//! (WIC, IFileOpenDialog) instead of bundling crates — keeps Vire.exe small.

use std::path::{Path, PathBuf};
use windows::core::{w, Interface, HSTRING};
use windows::Win32::Foundation::{GENERIC_WRITE, HWND};
use windows::Win32::Graphics::Gdi::{HBITMAP, HPALETTE};
use windows::Win32::Graphics::Imaging::*;
use windows::Win32::System::Com::{CoCreateInstance, CoTaskMemFree, CLSCTX_INPROC_SERVER};
use windows::Win32::UI::Shell::Common::COMDLG_FILTERSPEC;
use windows::Win32::UI::Shell::*;

pub struct Rgba { pub w: u32, pub h: u32, pub px: Vec<u8> }

fn factory() -> Option<IWICImagingFactory> {
    unsafe { CoCreateInstance(&CLSID_WICImagingFactory, None, CLSCTX_INPROC_SERVER).ok() }
}

unsafe fn to_rgba(f: &IWICImagingFactory, src: &IWICBitmapSource) -> Option<Rgba> {
    let conv = f.CreateFormatConverter().ok()?;
    conv.Initialize(src, &GUID_WICPixelFormat32bppRGBA, WICBitmapDitherTypeNone, None, 0.0, WICBitmapPaletteTypeCustom).ok()?;
    let (mut w, mut h) = (0u32, 0u32);
    conv.GetSize(&mut w, &mut h).ok()?;
    let mut px = vec![0u8; (w * h * 4) as usize];
    conv.CopyPixels(std::ptr::null(), w * 4, &mut px).ok()?;
    Some(Rgba { w, h, px })
}

/// Decodes PNG/JPEG/... bytes (e.g. `include_bytes!`) to straight RGBA.
pub fn decode(bytes: &[u8]) -> Option<Rgba> {
    unsafe {
        let f = factory()?;
        let stream = f.CreateStream().ok()?;
        stream.InitializeFromMemory(bytes).ok()?;
        let dec = f.CreateDecoderFromStream(&stream, std::ptr::null(), WICDecodeMetadataCacheOnDemand).ok()?;
        let frame = dec.GetFrame(0).ok()?;
        to_rgba(&f, &frame.cast().ok()?)
    }
}

/// Centre-crops an HBITMAP to 16:9, scales it to `w`x`h` and writes a PNG.
pub fn save_thumbnail(hbmp: HBITMAP, out: &Path, w: u32, h: u32) -> bool {
    match unsafe { save_thumbnail_inner(hbmp, out, w, h) } {
        Ok(()) => true,
        Err(e) => { LAST_ERROR.with(|l| *l.borrow_mut() = e); let _ = std::fs::remove_file(out); false }
    }
}

thread_local! { pub static LAST_ERROR: std::cell::RefCell<String> = const { std::cell::RefCell::new(String::new()) }; }

unsafe fn save_thumbnail_inner(hbmp: HBITMAP, out: &Path, w: u32, h: u32) -> Result<(), String> {
    let f = factory().ok_or("factory")?;
    let bmp = f.CreateBitmapFromHBITMAP(hbmp, HPALETTE::default(), WICBitmapIgnoreAlpha).map_err(|e| format!("from_hbitmap: {e}"))?;
    save_scaled(&f, &bmp.cast().map_err(|e| format!("cast: {e}"))?, out, w, h)
}

/// Centre-crops a raw BGRX frame (e.g. decoded by Media Foundation) to 16:9 and saves a PNG.
pub fn save_frame_thumbnail(px: &[u8], fw: u32, fh: u32, stride: u32, out: &Path, w: u32, h: u32) -> bool {
    unsafe {
        let run = || -> Result<(), String> {
            let f = factory().ok_or("factory")?;
            let bmp = f.CreateBitmapFromMemory(fw, fh, &GUID_WICPixelFormat32bppBGR, stride, px).map_err(|e| format!("from_memory: {e}"))?;
            save_scaled(&f, &bmp.cast().map_err(|e| format!("cast: {e}"))?, out, w, h)
        };
        match run() { Ok(()) => true, Err(e) => { LAST_ERROR.with(|l| *l.borrow_mut() = e); false } }
    }
}

unsafe fn save_scaled(f: &IWICImagingFactory, bmp: &IWICBitmapSource, out: &Path, w: u32, h: u32) -> Result<(), String> {
    let step = |n: &str| { let n = n.to_string(); move |e: windows::core::Error| format!("{n}: {e}") };
    let (mut iw, mut ih) = (0u32, 0u32);
    bmp.GetSize(&mut iw, &mut ih).map_err(step("size"))?;
    if iw == 0 || ih == 0 { return Err("empty".into()); }
    let (cw, ch) = if iw * 9 > ih * 16 { (ih * 16 / 9, ih) } else { (iw, iw * 9 / 16) };
    let clip = f.CreateBitmapClipper().map_err(step("clipper"))?;
    let rect = WICRect { X: ((iw - cw) / 2) as i32, Y: ((ih - ch) / 2) as i32, Width: cw as i32, Height: ch as i32 };
    clip.Initialize(bmp, &rect).map_err(step("clip"))?;
    let scaler = f.CreateBitmapScaler().map_err(step("scaler"))?;
    scaler.Initialize(&clip, w, h, WICBitmapInterpolationModeFant).map_err(step("scale"))?;
    // Materialise the pixels first (some sources can't be pulled twice by the encoder).
    let conv = f.CreateFormatConverter().map_err(step("conv"))?;
    conv.Initialize(&scaler, &GUID_WICPixelFormat32bppBGRA, WICBitmapDitherTypeNone, None, 0.0, WICBitmapPaletteTypeCustom).map_err(step("convert"))?;
    let mut px = vec![0u8; (w * h * 4) as usize];
    conv.CopyPixels(std::ptr::null(), w * 4, &mut px).map_err(step("copy"))?;
    for p in px.chunks_exact_mut(4) { p[3] = 255; } // opaque thumbnail
    let px32: Vec<u32> = px.chunks_exact(4).map(|c| u32::from_le_bytes([c[0], c[1], c[2], c[3]])).collect();
    if save_png(w, h, &px32, out) { Ok(()) } else { Err("encode".into()) }
}

/// Writes a premultiplied BGRA buffer as PNG (used by the hidden `--preview-dock` dev flag).
pub fn save_png(w: u32, h: u32, px: &[u32], out: &Path) -> bool {
    unsafe {
        let run = || -> Option<()> {
            let f = factory()?;
            let bytes = std::slice::from_raw_parts(px.as_ptr() as *const u8, px.len() * 4);
            let bmp = f.CreateBitmapFromMemory(w, h, &GUID_WICPixelFormat32bppPBGRA, w * 4, bytes).ok()?;
            let stream = f.CreateStream().ok()?;
            stream.InitializeFromFilename(&HSTRING::from(out.as_os_str()), GENERIC_WRITE.0).ok()?;
            let enc = f.CreateEncoder(&GUID_ContainerFormatPng, std::ptr::null()).ok()?;
            enc.Initialize(&stream, WICBitmapEncoderNoCache).ok()?;
            let mut frame: Option<IWICBitmapFrameEncode> = None;
            enc.CreateNewFrame(&mut frame, std::ptr::null_mut()).ok()?;
            let frame = frame?;
            frame.Initialize(None).ok()?;
            frame.SetSize(w, h).ok()?;
            let mut fmt = GUID_WICPixelFormat32bppBGRA;
            frame.SetPixelFormat(&mut fmt).ok()?;
            frame.WriteSource(&bmp, std::ptr::null()).ok()?;
            frame.Commit().ok()?;
            enc.Commit().ok()
        };
        run().is_some()
    }
}

/// Native multi-select "open video" dialog. Empty when cancelled.
pub fn pick_videos(owner: HWND) -> Vec<PathBuf> {
    unsafe {
        pick_inner(owner, true, w!("Video"), w!("*.mp4;*.m4v;*.mov;*.wmv;*.avi;*.mkv;*.webm"), w!("Chọn video để làm hình nền động"))
            .unwrap_or_default()
    }
}

/// Native multi-select "open audio" dialog. Empty when cancelled.
pub fn pick_audios(owner: HWND) -> Vec<PathBuf> {
    unsafe {
        pick_inner(owner, true, w!("Âm thanh"), w!("*.mp3;*.wav;*.m4a;*.aac;*.flac;*.wma;*.ogg"), w!("Chọn nhạc"))
            .unwrap_or_default()
    }
}

unsafe fn pick_inner(owner: HWND, multi: bool, name: windows::core::PCWSTR, pattern: windows::core::PCWSTR, title: windows::core::PCWSTR) -> Option<Vec<PathBuf>> {
    let dlg: IFileOpenDialog = CoCreateInstance(&FileOpenDialog, None, CLSCTX_INPROC_SERVER).ok()?;
    let opts = dlg.GetOptions().ok()?;
    let multi_flag = if multi { FOS_ALLOWMULTISELECT } else { FILEOPENDIALOGOPTIONS(0) };
    dlg.SetOptions(opts | multi_flag | FOS_FILEMUSTEXIST | FOS_FORCEFILESYSTEM).ok()?;
    let spec = [COMDLG_FILTERSPEC { pszName: name, pszSpec: pattern }];
    dlg.SetFileTypes(&spec).ok()?;
    dlg.SetTitle(title).ok()?;
    dlg.Show(owner).ok()?; // Err = cancelled
    let items = dlg.GetResults().ok()?;
    let mut out = Vec::new();
    for i in 0..items.GetCount().ok()? {
        let Ok(it) = items.GetItemAt(i) else { continue };
        if let Ok(p) = it.GetDisplayName(SIGDN_FILESYSPATH) {
            if let Ok(s) = p.to_string() { out.push(PathBuf::from(s)); }
            CoTaskMemFree(Some(p.0 as *const _));
        }
    }
    Some(out)
}
