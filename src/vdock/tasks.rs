//! Background apps shown under the ^ button, and ending tasks safely (never Windows' own).

use super::*;

pub(super) struct BgApp { pub(super) name: String, pub(super) exe: String, pub(super) pids: Vec<u32> }

/// "FileDescription" from the exe's version resource (e.g. "Zalo", "Steam"), else the file name.
pub(super) unsafe fn app_name(exe: &str) -> String {
    use windows::Win32::Storage::FileSystem::{GetFileVersionInfoSizeW, GetFileVersionInfoW, VerQueryValueW};
    let stem = std::path::Path::new(exe).file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_default();
    let h = HSTRING::from(exe);
    let size = GetFileVersionInfoSizeW(&h, None);
    if size == 0 { return stem; }
    let mut buf = vec![0u8; size as usize];
    if GetFileVersionInfoW(&h, 0, size, buf.as_mut_ptr() as *mut _).is_err() { return stem; }
    let mut p: *mut std::ffi::c_void = std::ptr::null_mut();
    let mut len = 0u32;
    if !VerQueryValueW(buf.as_ptr() as *const _, w!("\\VarFileInfo\\Translation"), &mut p, &mut len).as_bool() || len < 4 { return stem; }
    let (lang, cp) = (*(p as *const u16), *(p as *const u16).add(1));
    let q = HSTRING::from(format!("\\StringFileInfo\\{lang:04x}{cp:04x}\\FileDescription"));
    if !VerQueryValueW(buf.as_ptr() as *const _, &q, &mut p, &mut len).as_bool() || len <= 1 { return stem; }
    let s = String::from_utf16_lossy(std::slice::from_raw_parts(p as *const u16, len as usize - 1));
    let s = s.trim().to_string();
    if s.is_empty() || s.len() > 40 { stem } else { s }
}

pub(super) fn exe_of_pid(pid: u32) -> Option<String> {
    unsafe {
        let h = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid).ok()?;
        let mut buf = [0u16; 1024];
        let mut len = buf.len() as u32;
        let ok = QueryFullProcessImageNameW(h, PROCESS_NAME_WIN32, PWSTR(buf.as_mut_ptr()), &mut len).is_ok();
        let _ = CloseHandle(h);
        ok.then(|| String::from_utf16_lossy(&buf[..len as usize]))
    }
}

/// All of this user's processes, split into apps that are open on screen and background tasks,
/// each grouped by exe. A process started by an open app (browser tabs, Electron helpers, the
/// Claude CLI…) belongs to that app, so it is not listed as a separate background task.
/// Windows' own components are left out — ending those is a job for Task Manager.
pub(super) unsafe fn task_groups() -> (Vec<BgApp>, Vec<BgApp>) {
    use windows::Win32::System::Diagnostics::ToolHelp::*;
    use windows::Win32::System::RemoteDesktop::ProcessIdToSessionId;
    let own = GetCurrentProcessId();
    let mut session = 0u32;
    let _ = ProcessIdToSessionId(own, &mut session);

    // pid -> parent pid, for this session only.
    let mut parent: HashMap<u32, u32> = HashMap::new();
    if let Ok(snap) = CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0) {
        let mut pe = PROCESSENTRY32W { dwSize: std::mem::size_of::<PROCESSENTRY32W>() as u32, ..Default::default() };
        let mut ok = Process32FirstW(snap, &mut pe).is_ok();
        while ok {
            let mut s = u32::MAX;
            if ProcessIdToSessionId(pe.th32ProcessID, &mut s).is_ok() && s == session { parent.insert(pe.th32ProcessID, pe.th32ParentProcessID); }
            ok = Process32NextW(snap, &mut pe).is_ok();
        }
        let _ = CloseHandle(snap);
    }
    // Processes owning a visible, titled top-level window = open apps.
    unsafe extern "system" fn cb(h: HWND, data: LPARAM) -> BOOL {
        let v = &mut *(data.0 as *mut Vec<u32>);
        if IsWindowVisible(h).as_bool() && GetWindowTextLengthW(h) > 0 && GetWindow(h, GW_OWNER).map_or(true, |o| o.is_invalid()) {
            let mut pid = 0u32;
            GetWindowThreadProcessId(h, Some(&mut pid));
            if !v.contains(&pid) { v.push(pid); }
        }
        BOOL(1)
    }
    let mut visible: Vec<u32> = Vec::new();
    let _ = EnumWindows(Some(cb), LPARAM(&mut visible as *mut _ as isize));

    let windir = std::env::var("WINDIR").unwrap_or_else(|_| "C:\\Windows".into()).to_lowercase();
    let is_system = |low: &str| low.starts_with(&windir) || low.contains("\\windowsapps\\microsoft.") || low.contains("\\systemapps\\")
        || low.contains("\\windowsapps\\microsoftwindows") || low.contains("\\programdata\\microsoft\\");
    let mut exe: HashMap<u32, String> = HashMap::new();
    for &pid in parent.keys() { if let Some(e) = exe_of_pid(pid) { exe.insert(pid, e); } }

    // Open apps: their processes plus every descendant, attributed to the app's exe.
    let mut owner_of: HashMap<u32, u32> = HashMap::new(); // pid -> open-app root pid
    for &v in &visible { if exe.get(&v).map_or(false, |e| !is_system(&e.to_lowercase())) { owner_of.insert(v, v); } }
    loop {
        let mut grew = false;
        for (&pid, &pp) in &parent {
            if owner_of.contains_key(&pid) { continue; }
            if let Some(&root) = owner_of.get(&pp) { owner_of.insert(pid, root); grew = true; }
        }
        if !grew { break; }
    }
    let mut open: Vec<BgApp> = Vec::new();
    let mut bg: Vec<BgApp> = Vec::new();
    let push = |list: &mut Vec<BgApp>, e: &str, pid: u32| match list.iter_mut().find(|g| g.exe.eq_ignore_ascii_case(e)) {
        Some(g) => g.pids.push(pid),
        None => list.push(BgApp { name: app_name(e), exe: e.to_string(), pids: vec![pid] }),
    };
    for (&pid, e) in &exe {
        if pid == own || owner_of.get(&pid).map_or(false, |r| *r == own) { continue; }
        let low = e.to_lowercase();
        match owner_of.get(&pid) {
            Some(&root) => {
                // Group helpers under the app that owns them (e.g. all zen.exe + plugin-container).
                let root_exe = exe.get(&root).cloned().unwrap_or_else(|| e.clone());
                if root_exe.eq_ignore_ascii_case(e) || !is_system(&low) { push(&mut open, &root_exe, pid); }
            }
            None if !is_system(&low) && !low.contains("msedgewebview2") => push(&mut bg, e, pid),
            None => {}
        }
    }
    for l in [&mut open, &mut bg] { l.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase())); }
    (open, bg)
}


/// A process of Windows itself (File Explorer, Settings, dwm…): never ended from the dock.
pub(super) fn pid_is_system(pid: u32) -> bool {
    let windir = std::env::var("WINDIR").unwrap_or_else(|_| "C:\\Windows".into()).to_lowercase();
    match exe_of_pid(pid) {
        Some(e) => { let l = e.to_lowercase(); l.starts_with(&windir) || l.contains("\\systemapps\\") || l.contains("\\windowsapps\\microsoftwindows") }
        None => true, // can't even read it (protected / elevated): leave it alone
    }
}

/// True when any window of this dock item belongs to a Windows process.
pub(super) unsafe fn item_is_system(it: &Item) -> bool {
    it.wins.iter().any(|w| { let mut p = 0u32; GetWindowThreadProcessId(HWND(*w as *mut _), Some(&mut p)); pid_is_system(p) })
}

/// "Kết thúc <app>?" — Yes / No, No by default.
pub(super) unsafe fn confirm_end(owner: HWND, name: &str) -> bool {
    let text = HSTRING::from(format!("Kết thúc “{name}” ngay?\n\nCông việc chưa lưu trong ứng dụng này sẽ bị mất."));
    MessageBoxW(owner, &text, w!("Vire Dock"), MB_YESNO | MB_ICONWARNING | MB_DEFBUTTON2 | MB_TOPMOST | MB_SETFOREGROUND) == IDYES
}

/// "End task": terminate immediately, no questions (like Task Manager).
pub(super) fn force_kill(pids: Vec<u32>) {
    unsafe {
        for pid in pids {
            if let Ok(h) = OpenProcess(PROCESS_TERMINATE, false, pid) { let _ = TerminateProcess(h, 1); let _ = CloseHandle(h); }
        }
    }
}

/// Close politely (WM_CLOSE to its windows), then force-quit whatever is still alive.
pub(super) fn end_app(pids: Vec<u32>) {
    std::thread::spawn(move || unsafe {
        unsafe extern "system" fn cb(h: HWND, data: LPARAM) -> BOOL {
            let pids = &*(data.0 as *const Vec<u32>);
            let mut pid = 0u32;
            GetWindowThreadProcessId(h, Some(&mut pid));
            if pids.contains(&pid) { let _ = PostMessageW(h, WM_CLOSE, WPARAM(0), LPARAM(0)); }
            BOOL(1)
        }
        let _ = EnumWindows(Some(cb), LPARAM(&pids as *const _ as isize));
        std::thread::sleep(Duration::from_millis(1500));
        for pid in pids {
            if let Ok(h) = OpenProcess(PROCESS_TERMINATE | PROCESS_SYNCHRONIZE, false, pid) {
                if WaitForSingleObject(h, 0) == WAIT_TIMEOUT { let _ = TerminateProcess(h, 0); }
                let _ = CloseHandle(h);
            }
        }
    });
}

/// Dev aid (`--dev-bgapps=out.txt`): what the "^" menu would list.
pub fn dump_background_apps() -> String {
    unsafe {
        let (open, bg) = task_groups();
        let f = |l: &Vec<BgApp>| l.iter().map(|a| format!("  {}  [{} pid]  {}", a.name, a.pids.len(), a.exe)).collect::<Vec<_>>().join("\n");
        format!("OPEN:\n{}\nBACKGROUND:\n{}", f(&open), f(&bg))
    }
}
