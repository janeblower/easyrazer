//! Start with Windows: the per-user `Run` value.

use std::os::windows::process::CommandExt;
use std::path::Path;
use std::process::{Command, Output};

use crate::device::CREATE_NO_WINDOW;

const KEY: &str = r"HKCU\Software\Microsoft\Windows\CurrentVersion\Run";
const VALUE: &str = "EasyRazer";
pub const TRAY_ARG: &str = "--tray";

fn reg(args: &[&str]) -> std::io::Result<Output> {
    Command::new("reg").args(args).creation_flags(CREATE_NO_WINDOW).output()
}

fn query() -> Option<Output> {
    reg(&["query", KEY, "/v", VALUE]).ok()
}

/// The value itself is the source of truth: the user may remove it in Task Manager.
pub fn enabled() -> bool {
    parse(query()).is_some()
}

fn parse(out: Option<Output>) -> Option<String> {
    let o = out.filter(|o| o.status.success())?;
    let text = String::from_utf8_lossy(&o.stdout);
    let line = text.lines().find(|l| l.trim_start().starts_with(VALUE))?;
    Some(line.split_once("REG_SZ")?.1.trim().to_string())
}

fn command_line(exe: &Path) -> String {
    format!("\"{}\" {TRAY_ARG}", exe.display())
}

pub fn set(on: bool) -> Result<(), String> {
    if on == enabled() {
        return Ok(());
    }
    let out = if on {
        let exe = std::env::current_exe().map_err(|e| e.to_string())?;
        reg(&["add", KEY, "/v", VALUE, "/t", "REG_SZ", "/d", &command_line(&exe), "/f"])
    } else {
        reg(&["delete", KEY, "/v", VALUE, "/f"])
    }
    .map_err(|e| e.to_string())?;
    if out.status.success() {
        Ok(())
    } else {
        Err(format!("reg: {}", String::from_utf8_lossy(&out.stderr).trim()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::windows::process::ExitStatusExt;
    use std::process::ExitStatus;

    fn reg(code: u32, stdout: &str) -> Option<Output> {
        Some(Output { status: ExitStatus::from_raw(code), stdout: stdout.as_bytes().to_vec(), stderr: Vec::new() })
    }

    #[test]
    fn command_line_quotes_the_path() {
        assert_eq!(command_line(Path::new(r"C:\Program Files\Изи\easyrazer.exe")), r#""C:\Program Files\Изи\easyrazer.exe" --tray"#);
    }

    #[test]
    fn reads_the_value() {
        let out = "\r\nHKEY_CURRENT_USER\\Software\\Microsoft\\Windows\\CurrentVersion\\Run\r\n    EasyRazer    REG_SZ    \"C:\\a b\\easyrazer.exe\" --tray\r\n\r\n";
        assert_eq!(parse(reg(0, out)).as_deref(), Some("\"C:\\a b\\easyrazer.exe\" --tray"));
    }

    #[test]
    fn missing_value_is_off() {
        assert_eq!(parse(reg(1, "")), None);
        assert_eq!(parse(None), None);
    }

    #[test]
    #[ignore = "writes HKCU Run"]
    fn round_trip_in_registry() {
        let was = enabled();
        set(true).unwrap();
        let exe = std::env::current_exe().unwrap();
        assert_eq!(parse(query()), Some(command_line(&exe)));
        set(false).unwrap();
        assert!(!enabled());
        set(false).unwrap();
        if was {
            set(true).unwrap();
        }
    }
}
