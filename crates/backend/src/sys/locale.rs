//! The user's language settings.

use windows_sys::Win32::Foundation::{FILETIME, SYSTEMTIME};
use windows_sys::Win32::Globalization::GetUserDefaultLocaleName;
use windows_sys::Win32::System::Time::{FileTimeToSystemTime, SystemTimeToTzSpecificLocalTime};

/// The user's locale name, such as `ru-RU`; empty when Windows gives none.
pub fn user_locale() -> String {
    let mut buf = [0u16; 85];
    // SAFETY: the buffer is writable and its length is passed with it.
    let n = unsafe { GetUserDefaultLocaleName(buf.as_mut_ptr(), buf.len() as i32) };
    String::from_utf16_lossy(&buf[..(n.max(1) - 1) as usize])
}

/// Hours, minutes and seconds of a Unix time in milliseconds on the user's clock.
pub fn local_time(unix_ms: u64) -> Option<(u16, u16, u16)> {
    // FILETIME counts 100 ns steps from 1601.
    let ticks = (unix_ms + 11_644_473_600_000) * 10_000;
    let ft = FILETIME { dwLowDateTime: ticks as u32, dwHighDateTime: (ticks >> 32) as u32 };
    let mut utc = SYSTEMTIME::default();
    let mut local = SYSTEMTIME::default();
    // SAFETY: all three point to live locals of the right types.
    if unsafe { FileTimeToSystemTime(&ft, &mut utc) } == 0 {
        return None;
    }
    // SAFETY: as above; null asks for the current time zone.
    if unsafe { SystemTimeToTzSpecificLocalTime(std::ptr::null(), &utc, &mut local) } == 0 {
        return None;
    }
    Some((local.wHour, local.wMinute, local.wSecond))
}
