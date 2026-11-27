//! The Windows taskbar is the source of truth for *which apps are on the Dock and in what order*.
//! Its buttons are read through UI Automation (a Windows facility: no extra rights needed), on a
//! small worker thread, once a second. Each app button carries the app's identity ("Appid: …"), its
//! name, whether it is pinned and how many windows run — exactly what the taskbar itself shows.
//! (The taskbar keeps running, only made invisible: a hidden window has no accessible buttons.)

use super::*;
use std::sync::atomic::AtomicBool;
use windows::Win32::UI::Accessibility::{CUIAutomation, IUIAutomation, TreeScope_Descendants};

#[derive(Clone, Debug, PartialEq)]
pub(super) struct TbButton {
    /// Application id as Windows knows it (AppUserModelID, or the exe path for classic apps).
    pub appid: String,
    /// Name without the "- 1 running window pinned" tail.
    pub name: String,
    pub pinned: bool,
}

/// One accessible element of the taskbar → an app button, if it is one.
pub(super) fn parse_button(id: &str, name: &str) -> Option<TbButton> {
    let appid = id.strip_prefix("Appid:")?.trim().to_string();
    if appid.is_empty() { return None; }
    let low = name.to_lowercase();
    let pinned = low.ends_with(" pinned") || low.ends_with(" đã ghim");
    let mut base = name.trim().to_string();
    if pinned { base.truncate(base.len() - if low.ends_with(" pinned") { 7 } else { " đã ghim".len() }); }
    // "Zen - 1 running window", "Zalo - 2 running windows": cut at the last " - <digit>".
    if let Some(i) = base.rfind(" - ") {
        if base[i + 3..].chars().next().map_or(false, |c| c.is_ascii_digit()) { base.truncate(i); }
    }
    Some(TbButton { appid, name: base.trim().to_string(), pinned })
}

static SNAP: Mutex<Option<(Instant, Vec<TbButton>)>> = Mutex::new(None);
static STARTED: AtomicBool = AtomicBool::new(false);

/// The latest taskbar contents (None until the first good read, or when Explorer stopped answering).
pub(super) fn snapshot() -> Option<Vec<TbButton>> {
    let g = SNAP.lock().ok()?;
    g.as_ref().filter(|(t, _)| t.elapsed() < Duration::from_secs(15)).map(|(_, v)| v.clone())
}

pub(super) fn start() {
    if STARTED.swap(true, Ordering::SeqCst) { return; }
    let _ = std::thread::Builder::new().name("vire-tbmirror".into()).spawn(|| unsafe {
        let _ = CoInitializeEx(None, COINIT_MULTITHREADED);
        let Ok(auto) = CoCreateInstance::<_, IUIAutomation>(&CUIAutomation, None, CLSCTX_INPROC_SERVER) else { STARTED.store(false, Ordering::SeqCst); return };
        let Ok(all) = auto.CreateTrueCondition() else { STARTED.store(false, Ordering::SeqCst); return };
        loop {
            if let Some(list) = read(&auto, &all) {
                if let Ok(mut g) = SNAP.lock() { *g = Some((Instant::now(), list)); }
            }
            std::thread::sleep(Duration::from_millis(1000));
        }
    });
}

unsafe fn read(auto: &IUIAutomation, all: &windows::Win32::UI::Accessibility::IUIAutomationCondition) -> Option<Vec<TbButton>> {
    let tray = FindWindowW(w!("Shell_TrayWnd"), PCWSTR::null()).ok()?;
    let root = auto.ElementFromHandle(tray).ok()?;
    let arr = root.FindAll(TreeScope_Descendants, all).ok()?;
    let n = arr.Length().ok()?;
    let mut out = Vec::new();
    let mut have_start = false;
    for i in 0..n {
        let Ok(e) = arr.GetElement(i) else { continue };
        let id = e.CurrentAutomationId().map(|b| b.to_string()).unwrap_or_default();
        if id == "StartButton" { have_start = true; continue; }
        let name = e.CurrentName().map(|b| b.to_string()).unwrap_or_default();
        if let Some(b) = parse_button(&id, &name) { out.push(b); }
    }
    // The tree fills in lazily after Explorer (re)starts: a taskbar with a Start button but no apps
    // is real, but an empty read is not — keep the last good list in that case.
    if !have_start && out.is_empty() { return None; }
    Some(out)
}

/// Pins the Windows taskbar keeps as shortcuts (classic apps), re-read every 15 s at most.
pub(super) fn windows_pins() -> Vec<Pin> {
    thread_local! { static CACHE: RefCell<Option<(Instant, Vec<Pin>)>> = const { RefCell::new(None) }; }
    CACHE.with(|c| {
        let mut c = c.borrow_mut();
        if c.as_ref().map_or(true, |(t, _)| t.elapsed() > Duration::from_secs(15)) { *c = Some((Instant::now(), import_taskbar_pins())); }
        c.as_ref().map(|(_, v)| v.clone()).unwrap_or_default()
    })
}

#[cfg(test)]
mod tests {
    use super::parse_button;
    #[test]
    fn parses_taskbar_buttons() {
        let b = parse_button("Appid: F0DC299D809B9700", "Zen - 1 running window pinned").unwrap();
        assert_eq!((b.appid.as_str(), b.name.as_str(), b.pinned), ("F0DC299D809B9700", "Zen", true));
        let b = parse_button("Appid: C:\\x\\scrcpy.exe", "scrcpy.exe - Shortcut - 2 running windows pinned").unwrap();
        assert_eq!((b.name.as_str(), b.pinned), ("scrcpy.exe - Shortcut", true));
        let b = parse_button("Appid: Zalo", "Zalo - 2 running windows").unwrap();
        assert_eq!((b.name.as_str(), b.pinned), ("Zalo", false));
        let b = parse_button("Appid: Microsoft.Windows.Explorer", "File Explorer pinned").unwrap();
        assert_eq!((b.name.as_str(), b.pinned), ("File Explorer", true));
        assert!(parse_button("SystemTrayIcon", "Volume").is_none());
    }
}
