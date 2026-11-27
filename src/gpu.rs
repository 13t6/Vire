//! On laptops with two graphics chips Windows wakes the discrete NVIDIA/AMD GPU for any program that
//! draws with Direct3D or OpenGL, and its driver costs ~70 MB of RAM inside the process. A wallpaper
//! and a dock need nothing of that, so Vire asks Windows (Settings → Display → Graphics, the same
//! per-app switch you can flip by hand) to run it on the power-saving GPU. A choice the user already
//! made for Vire is never overwritten.

use windows::core::{w, HSTRING};
use windows::Win32::Foundation::ERROR_SUCCESS;
use windows::Win32::System::Registry::{RegGetValueW, RegSetKeyValueW, HKEY_CURRENT_USER, REG_SZ, RRF_RT_REG_SZ};

/// Returns true when the preference was just written — the caller must restart once so it applies.
pub fn ensure_power_saving() -> bool {
    let Ok(exe) = std::env::current_exe() else { return false };
    let name = HSTRING::from(exe.as_os_str());
    let key = w!("Software\\Microsoft\\DirectX\\UserGpuPreferences");
    unsafe {
        let mut len = 0u32;
        if RegGetValueW(HKEY_CURRENT_USER, key, &name, RRF_RT_REG_SZ, None, None, Some(&mut len)) == ERROR_SUCCESS {
            return false; // already has a setting (ours or the user's)
        }
        let value: Vec<u16> = "GpuPreference=1;".encode_utf16().chain(std::iter::once(0)).collect();
        RegSetKeyValueW(HKEY_CURRENT_USER, key, &name, REG_SZ.0, Some(value.as_ptr() as *const _), (value.len() * 2) as u32) == ERROR_SUCCESS
    }
}
