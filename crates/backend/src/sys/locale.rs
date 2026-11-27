//! The user's language settings.

use windows_sys::Win32::Globalization::GetUserDefaultLocaleName;

/// The user's locale name, such as `ru-RU`; empty when Windows gives none.
pub fn user_locale() -> String {
    let mut buf = [0u16; 85];
    // SAFETY: the buffer is writable and its length is passed with it.
    let n = unsafe { GetUserDefaultLocaleName(buf.as_mut_ptr(), buf.len() as i32) };
    String::from_utf16_lossy(&buf[..(n.max(1) - 1) as usize])
}
