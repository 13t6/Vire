//! Launch at login via HKCU\Software\Microsoft\Windows\CurrentVersion\Run.

use windows::core::w;
use windows::Win32::System::Registry::{RegDeleteKeyValueW, RegSetKeyValueW, HKEY_CURRENT_USER, REG_SZ};

const RUN_KEY: windows::core::PCWSTR = w!("Software\\Microsoft\\Windows\\CurrentVersion\\Run");
const VALUE: windows::core::PCWSTR = w!("Vire");

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
