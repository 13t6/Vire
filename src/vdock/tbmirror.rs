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
        let mut round = 0u32;
        let mut revealed: Option<Instant> = None;
        let mut reveal_end: Option<Instant> = None;
        loop {
            // A press asked for by the dock is carried out within 100 ms (retried for ~3 s while the
            // taskbar's buttons are still loading); the contents are re-read once a second.
            if round == 80 {
                // Dev aid: VIRE_DEV_PRESS=start|clock|quick|hidden presses that button once.
                let a = match std::env::var("VIRE_DEV_PRESS").unwrap_or_default().as_str() {
                    "start" => Some(TbAct::Start), "clock" => Some(TbAct::Clock), "quick" => Some(TbAct::QuickSettings), "hidden" => Some(TbAct::HiddenIcons), _ => None };
                if let Some(a) = a { let _ = request(a); }
            }
            let pending = ACT.lock().ok().and_then(|g| *g);
            if let Some((act, t)) = pending {
                let ok = press(&auto, &all, act);
                if !ok && revealed.is_none() && round % 10 == 0 {
                    // An auto-hidden taskbar exposes no buttons. Let it show for a moment (it stays
                    // invisible: it is clipped to one pixel) so its buttons can be pressed.
                    dbg("taskbar press: buttons not available -> letting the taskbar show for a moment");
                    set_taskbar_autohide(false);
                    revealed = Some(Instant::now());
                }
                if ok || t.elapsed() > Duration::from_secs(5) {
                    if let Ok(mut g) = ACT.lock() { if g.map_or(false, |(a, _)| a == act) { *g = None; } }
                    if revealed.is_some() { reveal_end = Some(Instant::now() + Duration::from_millis(if ok { 900 } else { 0 })); }
                }
            }
            if let Some(t) = reveal_end {
                if Instant::now() >= t { set_taskbar_autohide(true); revealed = None; reveal_end = None; }
            }
            if round % 10 == 0 {
                if let Some(list) = read(&auto, &all) {
                    if let Ok(mut g) = SNAP.lock() { *g = Some((Instant::now(), list)); }
                }
            }
            round = round.wrapping_add(1);
            std::thread::sleep(Duration::from_millis(100));
        }
    });
}

/// What the dock wants pressed on the real Windows taskbar.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(super) enum TbAct { Start, Clock, QuickSettings, HiddenIcons }

static ACT: Mutex<Option<(TbAct, Instant)>> = Mutex::new(None);

/// Presses the matching button of the real taskbar (UI Automation "Invoke", exactly what a click
/// does) — no keystrokes are sent, so nothing leaks into a remote-desktop window (UltraViewer,
/// AnyDesk, RDP…) that happens to be in front. Returns false when the worker is not running.
pub(super) fn request(act: TbAct) -> bool {
    if !STARTED.load(Ordering::SeqCst) { return false; }
    if let Ok(mut g) = ACT.lock() { *g = Some((act, Instant::now())); }
    true
}

unsafe fn press(auto: &IUIAutomation, all: &windows::Win32::UI::Accessibility::IUIAutomationCondition, act: TbAct) -> bool {
    use windows::Win32::UI::Accessibility::{IUIAutomationInvokePattern, UIA_InvokePatternId};
    let Some(tray) = FindWindowW(w!("Shell_TrayWnd"), PCWSTR::null()).ok() else { return false };
    let Ok(root) = auto.ElementFromHandle(tray) else { return false };
    let Ok(arr) = root.FindAll(TreeScope_Descendants, all) else { return false };
    let Ok(n) = arr.Length() else { return false };
    let (mut start, mut icons) = (None, Vec::new());
    for i in 0..n {
        let Ok(e) = arr.GetElement(i) else { continue };
        match e.CurrentAutomationId().map(|b| b.to_string()).unwrap_or_default().as_str() {
            "StartButton" => start = Some(e),
            // Notification area, left to right: [^ hidden icons] … network, volume, battery, clock, [show desktop].
            // (Located by position, not by name: the names change with the language of Windows.)
            "SystemTrayIcon" => icons.push(e),
            _ => {}
        }
    }
    let k = icons.len();
    let target = match act {
        TbAct::Start => start,
        TbAct::HiddenIcons => icons.first().cloned(),
        TbAct::Clock => if k >= 2 { icons.get(k - 2).cloned() } else { None },
        TbAct::QuickSettings => if k >= 4 { icons.get(k - 3).cloned() } else { None },
    };
    let Some(t) = target else { dbg(&format!("taskbar press {act:?}: no target (start={} tray icons={k} of {n} elements)", start_found(&arr, n))); return false };
    use windows::Win32::UI::Accessibility::{IUIAutomationTogglePattern, IUIAutomationLegacyIAccessiblePattern, UIA_TogglePatternId, UIA_LegacyIAccessiblePatternId};
    let ok = if let Some(inv) = t.GetCurrentPattern(UIA_InvokePatternId).ok().and_then(|p| p.cast::<IUIAutomationInvokePattern>().ok()) {
        inv.Invoke().is_ok()
    } else if let Some(tg) = t.GetCurrentPattern(UIA_TogglePatternId).ok().and_then(|p| p.cast::<IUIAutomationTogglePattern>().ok()) {
        tg.Toggle().is_ok()
    } else if let Some(lg) = t.GetCurrentPattern(UIA_LegacyIAccessiblePatternId).ok().and_then(|p| p.cast::<IUIAutomationLegacyIAccessiblePattern>().ok()) {
        lg.DoDefaultAction().is_ok()
    } else {
        let r = t.CurrentBoundingRectangle().unwrap_or_default();
        dbg(&format!("taskbar press {act:?}: no usable pattern; rect={},{},{},{} class={}", r.left, r.top, r.right, r.bottom, t.CurrentClassName().map(|b| b.to_string()).unwrap_or_default()));
        return false;
    };
    dbg(&format!("taskbar press {act:?} -> {ok}"));
    ok
}

unsafe fn start_found(arr: &windows::Win32::UI::Accessibility::IUIAutomationElementArray, n: i32) -> String {
    // Debug summary: automation ids / names of the non-app elements.
    (0..n).filter_map(|i| arr.GetElement(i).ok()).map(|e| format!("{}|{}",
        e.CurrentAutomationId().map(|b| b.to_string()).unwrap_or_default(), e.CurrentName().map(|b| b.to_string()).unwrap_or_default()))
        .filter(|s| !s.starts_with("Appid")).collect::<Vec<_>>().join(" ; ")
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
