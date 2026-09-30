//! Windows Dynamic Lighting on/off: the per-user switch the Settings app writes.

use std::ptr::null_mut;

use windows_sys::Win32::Foundation::ERROR_SUCCESS;
use windows_sys::Win32::System::Registry::{HKEY_CURRENT_USER, REG_DWORD, RRF_RT_REG_DWORD, RegGetValueW, RegSetKeyValueW};

const KEY: &str = r"Software\Microsoft\Lighting";
const VALUE: &str = "AmbientLightingEnabled";

fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(Some(0)).collect()
}

/// Anything but a clear `0` counts as enabled: a false warning beats an effect that silently stays hidden.
pub fn enabled() -> bool {
    let mut data = 1u32;
    let mut size = 4u32;
    let status = unsafe { RegGetValueW(HKEY_CURRENT_USER, wide(KEY).as_ptr(), wide(VALUE).as_ptr(), RRF_RT_REG_DWORD, null_mut(), (&mut data as *mut u32).cast(), &mut size) };
    status != ERROR_SUCCESS || data != 0
}

/// The Lighting service watches this value and switches at once.
pub fn set(on: bool) -> Result<(), String> {
    let data = on as u32;
    let status = unsafe { RegSetKeyValueW(HKEY_CURRENT_USER, wide(KEY).as_ptr(), wide(VALUE).as_ptr(), REG_DWORD, (&data as *const u32).cast(), 4) };
    if status == ERROR_SUCCESS {
        Ok(())
    } else {
        Err(format!("registry: {}", std::io::Error::from_raw_os_error(status as i32)))
    }
}
