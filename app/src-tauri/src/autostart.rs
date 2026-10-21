//! Start with Windows: the per-user `Run` value.

use std::path::Path;

use crate::sys;

const KEY: &str = r"Software\Microsoft\Windows\CurrentVersion\Run";
const VALUE: &str = "EasyRazer";
pub const TRAY_ARG: &str = "--tray";

/// The value itself is the source of truth: the user may remove it in Task Manager.
pub fn enabled() -> bool {
    sys::has_string(KEY, VALUE)
}

fn command_line(exe: &Path) -> String {
    format!("\"{}\" {TRAY_ARG}", exe.display())
}

pub fn set(on: bool) -> Result<(), String> {
    if on == enabled() {
        return Ok(());
    }
    if on {
        let exe = std::env::current_exe().map_err(|e| e.to_string())?;
        sys::set_string(KEY, VALUE, &command_line(&exe))
    } else {
        sys::delete_value(KEY, VALUE)
    }
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
