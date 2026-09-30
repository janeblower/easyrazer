//! Start with Windows: the per-user `Run` value.

use std::path::Path;
use std::ptr::null_mut;

use windows_sys::Win32::Foundation::ERROR_SUCCESS;
use windows_sys::Win32::System::Registry::{
    HKEY_CURRENT_USER, REG_SZ, RRF_RT_REG_SZ, RegDeleteKeyValueW, RegGetValueW, RegSetKeyValueW,
};

const KEY: &str = r"Software\Microsoft\Windows\CurrentVersion\Run";
const VALUE: &str = "EasyRazer";
pub const TRAY_ARG: &str = "--tray";

fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(Some(0)).collect()
}

fn check(status: u32) -> Result<(), String> {
    if status == ERROR_SUCCESS {
        Ok(())
    } else {
        Err(format!("registry: {}", std::io::Error::from_raw_os_error(status as i32)))
    }
}

/// The value itself is the source of truth: the user may remove it in Task Manager.
pub fn enabled() -> bool {
    let status = unsafe { RegGetValueW(HKEY_CURRENT_USER, wide(KEY).as_ptr(), wide(VALUE).as_ptr(), RRF_RT_REG_SZ, null_mut(), null_mut(), null_mut()) };
    status == ERROR_SUCCESS
}

fn command_line(exe: &Path) -> String {
    format!("\"{}\" {TRAY_ARG}", exe.display())
}

pub fn set(on: bool) -> Result<(), String> {
    if on == enabled() {
        return Ok(());
    }
    let (key, value) = (wide(KEY), wide(VALUE));
    let status = if on {
        let exe = std::env::current_exe().map_err(|e| e.to_string())?;
        let data = wide(&command_line(&exe));
        unsafe { RegSetKeyValueW(HKEY_CURRENT_USER, key.as_ptr(), value.as_ptr(), REG_SZ, data.as_ptr().cast(), (data.len() * 2) as u32) }
    } else {
        unsafe { RegDeleteKeyValueW(HKEY_CURRENT_USER, key.as_ptr(), value.as_ptr()) }
    };
    check(status)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn command_line_quotes_the_path() {
        assert_eq!(command_line(Path::new(r"C:\Program Files\Изи\easyrazer.exe")), r#""C:\Program Files\Изи\easyrazer.exe" --tray"#);
    }

    #[test]
    #[ignore = "writes HKCU Run"]
    fn round_trip_in_registry() {
        let was = enabled();
        set(true).unwrap();
        assert!(enabled());
        set(false).unwrap();
        assert!(!enabled());
        set(false).unwrap();
        if was {
            set(true).unwrap();
        }
    }
}
