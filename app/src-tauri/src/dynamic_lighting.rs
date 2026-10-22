//! Windows Dynamic Lighting on/off: the per-user switch the Settings app writes.

use crate::sys;

const KEY: &str = r"Software\Microsoft\Lighting";
const VALUE: &str = "AmbientLightingEnabled";

/// Anything but a clear `0` counts as enabled: a false warning beats an effect that silently stays hidden.
pub fn enabled() -> bool {
    sys::dword(KEY, VALUE) != Some(0)
}

/// The Lighting service watches this value and switches at once.
pub fn set(on: bool) -> Result<(), String> {
    sys::set_dword(KEY, VALUE, on as u32)
}
