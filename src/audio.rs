//! System-wide audio capture for the music visualizer: WASAPI *loopback* on the default playback
//! device — everything Windows is playing (Spotify, a browser, VLC, a game…), nothing else.
//!
//! Privacy: this is the render endpoint only. The microphone / any capture device is never opened,
//! nothing is recorded, saved or sent anywhere — each buffer is analysed in place (see `dsp.rs`)
//! and discarded. The capture thread only exists while at least one [`Lease`] is held (a visible
//! visualizer); dropping the last lease stops it within ~10 ms. It follows a change of default
//! playback device and recovers from audio-service restarts by re-opening the device.

use crate::dsp::{Analyzer, Frame};
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicU8, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::{Duration, Instant};

use windows::core::PWSTR;
use windows::Win32::Media::Audio::*;
use windows::Win32::System::Com::{CoCreateInstance, CoInitializeEx, CoTaskMemFree, CLSCTX_ALL, COINIT_MULTITHREADED};

/// What the capture is doing right now (shown by the Widgets page).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum State { Stopped, Starting, Capturing, NoDevice }

struct Core {
    frame: Mutex<Frame>,
    /// Bumped for every published frame (renderers skip work when it didn't change).
    seq: AtomicU64,
    users: AtomicUsize,
    running: AtomicBool,
    state: AtomicU8,
    /// Real sound is arriving (not just silence).
    signal: AtomicBool,
}

fn core() -> &'static Arc<Core> {
    static CORE: OnceLock<Arc<Core>> = OnceLock::new();
    CORE.get_or_init(|| Arc::new(Core {
        frame: Mutex::new(Frame::default()), seq: AtomicU64::new(0), users: AtomicUsize::new(0),
        running: AtomicBool::new(false), state: AtomicU8::new(0), signal: AtomicBool::new(false),
    }))
}

/// Holding a lease keeps the capture running; dropping the last one stops it.
pub struct Lease(());

impl Drop for Lease {
    fn drop(&mut self) { core().users.fetch_sub(1, Ordering::SeqCst); }
}

/// Asks for the capture (starting it if needed). Cheap to call again; release by dropping.
pub fn acquire() -> Lease {
    let c = core();
    c.users.fetch_add(1, Ordering::SeqCst);
    if !c.running.swap(true, Ordering::SeqCst) {
        c.state.store(1, Ordering::Relaxed);
        let c2 = c.clone();
        let _ = std::thread::Builder::new().name("vire-audio".into()).spawn(move || unsafe { capture_thread(c2) });
    }
    Lease(())
}

/// Latest analysed frame and its sequence number.
pub fn snapshot() -> (Frame, u64) {
    let c = core();
    (c.frame.lock().unwrap().clone(), c.seq.load(Ordering::Relaxed))
}

pub fn state() -> State {
    match core().state.load(Ordering::Relaxed) { 1 => State::Starting, 2 => State::Capturing, 3 => State::NoDevice, _ => State::Stopped }
}

/// True while sound is actually playing (the visualizer idles smoothly otherwise).
pub fn has_signal() -> bool { core().signal.load(Ordering::Relaxed) }

// ------------------------------------------------------------------ capture thread

struct Session {
    client: IAudioClient,
    cap: IAudioCaptureClient,
    device_id: String,
    channels: usize,
    bits: usize,
    float: bool,
    rate: u32,
}

unsafe fn device_id(dev: &IMMDevice) -> String {
    match dev.GetId() {
        Ok(p) => { let s = p.to_string().unwrap_or_default(); CoTaskMemFree(Some(p.0 as *const _)); s }
        Err(_) => String::new(),
    }
}

unsafe fn default_render_id(en: &IMMDeviceEnumerator) -> Option<String> {
    en.GetDefaultAudioEndpoint(eRender, eConsole).ok().map(|d| device_id(&d))
}

/// Opens loopback capture on the default playback device.
unsafe fn open(en: &IMMDeviceEnumerator) -> windows::core::Result<Session> {
    let dev = en.GetDefaultAudioEndpoint(eRender, eConsole)?;
    let id = device_id(&dev);
    let client: IAudioClient = dev.Activate(CLSCTX_ALL, None)?;
    let fmt = client.GetMixFormat()?;
    let rd = |off: usize, n: usize| -> u32 {
        let p = (fmt as *const u8).add(off);
        match n { 2 => std::ptr::read_unaligned(p as *const u16) as u32, _ => std::ptr::read_unaligned(p as *const u32) }
    };
    let tag = rd(0, 2);
    let channels = (rd(2, 2) as usize).max(1);
    let rate = rd(4, 4);
    let bits = rd(14, 2) as usize;
    // WAVE_FORMAT_EXTENSIBLE keeps the real sample type in the first dword of SubFormat (offset 24).
    let float = tag == 3 || (tag == 0xFFFE && rd(24, 4) == 3);
    // 20 ms buffer; loopback needs no event, we poll the packet size.
    client.Initialize(AUDCLNT_SHAREMODE_SHARED, AUDCLNT_STREAMFLAGS_LOOPBACK, 2_000_000, 0, fmt, None)?;
    CoTaskMemFree(Some(fmt as *const _));
    let cap: IAudioCaptureClient = client.GetService()?;
    client.Start()?;
    Ok(Session { client, cap, device_id: id, channels, bits, float, rate })
}

/// Converts one packet to mono f32 samples appended to `out`.
unsafe fn to_mono(s: &Session, data: *const u8, frames: usize, out: &mut Vec<f32>) {
    let ch = s.channels;
    let bytes = s.bits / 8;
    for f in 0..frames {
        let mut acc = 0.0f32;
        for c in 0..ch {
            let p = data.add((f * ch + c) * bytes);
            acc += if s.float && s.bits == 32 { std::ptr::read_unaligned(p as *const f32) }
                else { match s.bits {
                    16 => std::ptr::read_unaligned(p as *const i16) as f32 / 32768.0,
                    24 => { let v = ((*p.add(2) as i32) << 24 | (*p.add(1) as i32) << 16 | (*p as i32) << 8) >> 8; v as f32 / 8_388_608.0 }
                    32 => std::ptr::read_unaligned(p as *const i32) as f32 / 2_147_483_648.0,
                    _ => 0.0,
                } };
        }
        out.push(acc / ch as f32);
    }
}

unsafe fn capture_thread(c: Arc<Core>) {
    let _ = CoInitializeEx(None, COINIT_MULTITHREADED);
    let mut analyzer = Analyzer::new();
    let mut mono: Vec<f32> = Vec::with_capacity(4096);
    let zeros = vec![0.0f32; 4096];
    let publish = |f: &Frame| { *c.frame.lock().unwrap() = f.clone(); c.seq.fetch_add(1, Ordering::Relaxed); };
    'outer: loop {
        if c.users.load(Ordering::SeqCst) == 0 { break; }
        let en: IMMDeviceEnumerator = match CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL) {
            Ok(e) => e,
            Err(_) => { c.state.store(3, Ordering::Relaxed); std::thread::sleep(Duration::from_millis(1500)); continue; }
        };
        let session = match open(&en) {
            Ok(s) => s,
            Err(_) => {
                // No playback device / audio service down: show an inactive state, retry quietly.
                c.state.store(3, Ordering::Relaxed);
                c.signal.store(false, Ordering::Relaxed);
                for _ in 0..30 { if c.users.load(Ordering::SeqCst) == 0 { break 'outer; } std::thread::sleep(Duration::from_millis(50)); }
                continue;
            }
        };
        analyzer.set_rate(session.rate);
        c.state.store(2, Ordering::Relaxed);
        let (mut last_data, mut last_check, mut last_zero) = (Instant::now(), Instant::now(), Instant::now());
        loop {
            if c.users.load(Ordering::SeqCst) == 0 { let _ = session.client.Stop(); break 'outer; }
            // Default device changed (headphones plugged in…)? Re-open on the new one.
            if last_check.elapsed() > Duration::from_millis(1500) {
                last_check = Instant::now();
                if default_render_id(&en).map_or(true, |id| id != session.device_id) { break; }
            }
            let mut got = false;
            loop {
                let n = match session.cap.GetNextPacketSize() { Ok(n) => n, Err(_) => { let _ = session.client.Stop(); c.state.store(3, Ordering::Relaxed); std::thread::sleep(Duration::from_millis(800)); continue 'outer; } };
                if n == 0 { break; }
                let (mut data, mut frames, mut flags) = (std::ptr::null_mut::<u8>(), 0u32, 0u32);
                if session.cap.GetBuffer(&mut data, &mut frames, &mut flags, None, None).is_err() { let _ = session.client.Stop(); c.state.store(3, Ordering::Relaxed); continue 'outer; }
                mono.clear();
                if flags & AUDCLNT_BUFFERFLAGS_SILENT.0 as u32 != 0 || data.is_null() { mono.resize(frames as usize, 0.0); }
                else { to_mono(&session, data, frames as usize, &mut mono); }
                let _ = session.cap.ReleaseBuffer(frames);
                // Analyse and drop: nothing of the audio is kept.
                analyzer.push(&mono, |f| { publish(f); c.signal.store(f.level > 0.0008, Ordering::Relaxed); });
                got = true;
            }
            if got { last_data = Instant::now(); }
            else if last_data.elapsed() > Duration::from_millis(250) {
                // Loopback delivers nothing while Windows plays nothing: feed silence so the
                // visualizer settles smoothly into its idle state (~30 frames per second).
                if last_zero.elapsed() > Duration::from_millis(33) {
                    last_zero = Instant::now();
                    analyzer.push(&zeros[..1600], |f| publish(f));
                    c.signal.store(false, Ordering::Relaxed);
                }
            }
            std::thread::sleep(Duration::from_millis(8));
        }
    }
    c.signal.store(false, Ordering::Relaxed);
    c.state.store(0, Ordering::Relaxed);
    *c.frame.lock().unwrap() = Frame::default();
    c.seq.fetch_add(1, Ordering::Relaxed);
    c.running.store(false, Ordering::SeqCst);
    // A lease may have arrived while we were shutting down: start again instead of missing it.
    if c.users.load(Ordering::SeqCst) > 0 && !c.running.swap(true, Ordering::SeqCst) {
        c.state.store(1, Ordering::Relaxed);
        capture_thread(c);
    }
    let _ = PWSTR::null();
}
