//! Launch at login via HKCU\Software\Microsoft\Windows\CurrentVersion\Run.

use windows::core::w;
use windows::Win32::System::Registry::{RegDeleteKeyValueW, RegSetKeyValueW, HKEY_CURRENT_USER, REG_SZ};

const RUN_KEY: windows::core::PCWSTR = w!("Software\\Microsoft\\Windows\\CurrentVersion\\Run");
const VALUE: windows::core::PCWSTR = w!("Vire");

/// "Vire" in the Windows search box / Start menu = a shortcut in the user's Start Menu\Programs
/// folder (Windows indexes that folder by itself). Rewritten at every start so it follows the exe
/// when the portable folder is moved. Needs COM (STA) on the calling thread.
pub fn set_start_shortcut(enabled: bool) {
    use windows::core::{Interface, HSTRING};
    use windows::Win32::System::Com::{CoCreateInstance, IPersistFile, CLSCTX_INPROC_SERVER};
    use windows::Win32::UI::Shell::{IShellLinkW, ShellLink};
    let Some(appdata) = std::env::var_os("APPDATA") else { return };
    let lnk = std::path::PathBuf::from(appdata).join(r"Microsoft\Windows\Start Menu\Programs\Vire.lnk");
    if !enabled { let _ = std::fs::remove_file(&lnk); return; }
    let Ok(exe) = std::env::current_exe() else { return };
    unsafe {
        let Ok(link) = CoCreateInstance::<_, IShellLinkW>(&ShellLink, None, CLSCTX_INPROC_SERVER) else { return };
        let _ = link.SetPath(&HSTRING::from(exe.as_os_str()));
        if let Some(dir) = exe.parent() { let _ = link.SetWorkingDirectory(&HSTRING::from(dir.as_os_str())); }
        let _ = link.SetDescription(&HSTRING::from("Vire - hình nền động, Dock, widget"));
        let _ = link.SetIconLocation(&HSTRING::from(exe.as_os_str()), 0);
        if let Ok(pf) = link.cast::<IPersistFile>() { let _ = pf.Save(&HSTRING::from(lnk.as_os_str()), true); }
    }
}

pub fn set_enabled(enabled: bool) {
    unsafe {
        // Drop the entry left by the pre-rename build, if any.
        let _ = RegDeleteKeyValueW(HKEY_CURRENT_USER, RUN_KEY, w!("DynamicWallpaperStudio"));
        if enabled {
            let Ok(exe) = std::env::current_exe() else { return };
            let cmd = format!("\"{}\" --tray\0", exe.display());
            let wide: Vec<u16> = cmd.encode_utf16().collect();
            let _ = RegSetKeyValueW(HKEY_CURRENT_USER, RUN_KEY, VALUE, REG_SZ.0,
                Some(wide.as_ptr() as *const _), (wide.len() * 2) as u32);
        } else {
            let _ = RegDeleteKeyValueW(HKEY_CURRENT_USER, RUN_KEY, VALUE);
        }
    }
}
