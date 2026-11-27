//! Windows Dynamic Lighting on/off: the per-user switch the Settings app writes.

use winreg::RegKey;
use winreg::enums::HKEY_CURRENT_USER;

const KEY: &str = r"Software\Microsoft\Lighting";
const VALUE: &str = "AmbientLightingEnabled";

/// Whether the global switch is on; it says nothing of the per-device ones, so the app only
/// warns on it. Anything but a clear `0` counts as on: a needless hint beats a hidden effect.
pub fn enabled() -> bool {
    let value = RegKey::predef(HKEY_CURRENT_USER).open_subkey(KEY).and_then(|key| key.get_value::<u32, _>(VALUE));
    value.ok() != Some(0)
}
