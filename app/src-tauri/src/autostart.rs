//! Start with Windows: the per-user `Run` value.

use std::path::Path;

use winreg::RegKey;
use winreg::enums::{HKEY_CURRENT_USER, KEY_SET_VALUE};

const KEY: &str = r"Software\Microsoft\Windows\CurrentVersion\Run";
const VALUE: &str = "EasyRazer";
pub const TRAY_ARG: &str = "--tray";

/// The value itself is the source of truth: the user may remove it in Task Manager.
pub fn enabled() -> bool {
    RegKey::predef(HKEY_CURRENT_USER).open_subkey(KEY).and_then(|key| key.get_value::<String, _>(VALUE)).is_ok()
}

fn command_line(exe: &Path) -> String {
    format!("\"{}\" {TRAY_ARG}", exe.display())
}

pub fn set(on: bool) -> Result<(), String> {
    if on == enabled() {
        return Ok(());
    }
    let registry = |e: std::io::Error| format!("registry: {e}");
    let (key, _) = RegKey::predef(HKEY_CURRENT_USER).create_subkey_with_flags(KEY, KEY_SET_VALUE).map_err(registry)?;
    if on {
        let exe = std::env::current_exe().map_err(|e| e.to_string())?;
        key.set_value(VALUE, &command_line(&exe)).map_err(registry)
    } else {
        key.delete_value(VALUE).map_err(registry)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn command_line_quotes_the_path() {
        assert_eq!(command_line(Path::new(r"C:\Program Files\Изи\easyrazer.exe")), r#""C:\Program Files\Изи\easyrazer.exe" --tray"#);
    }

    /// Puts the user's own value back byte for byte, even if the test panics: `set(true)`
    /// would write the test binary's path instead.
    struct Restore(Option<winreg::RegValue<'static>>);
    impl Drop for Restore {
        fn drop(&mut self) {
            let restored = RegKey::predef(HKEY_CURRENT_USER).create_subkey_with_flags(KEY, KEY_SET_VALUE).and_then(|(key, _)| match &self.0 {
                Some(value) => key.set_raw_value(VALUE, value),
                None => key.delete_value(VALUE).or_else(|e| if e.kind() == std::io::ErrorKind::NotFound { Ok(()) } else { Err(e) }),
            });
            if let Err(e) = restored {
                eprintln!("could not restore HKCU Run\\{VALUE}: {e}");
            }
        }
    }

    #[test]
    #[ignore = "writes HKCU Run"]
    fn round_trip_in_registry() {
        let _restore = Restore(RegKey::predef(HKEY_CURRENT_USER).open_subkey(KEY).and_then(|key| key.get_raw_value(VALUE)).ok());
        set(true).unwrap();
        assert!(enabled());
        set(false).unwrap();
        assert!(!enabled());
        set(false).unwrap();
    }
}
