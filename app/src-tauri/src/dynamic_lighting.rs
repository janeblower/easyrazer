//! Windows Dynamic Lighting on/off: the per-user switch the Settings app writes.

use std::os::windows::process::CommandExt;
use std::process::{Command, Output};

use crate::device::CREATE_NO_WINDOW;

const KEY: &str = r"HKCU\Software\Microsoft\Lighting";
const VALUE: &str = "AmbientLightingEnabled";

pub fn enabled() -> bool {
    parse(Command::new("reg").args(["query", KEY, "/v", VALUE]).creation_flags(CREATE_NO_WINDOW).output().ok())
}

/// Anything but a clear `0x0` counts as enabled: a false warning beats an effect that silently stays hidden.
fn parse(out: Option<Output>) -> bool {
    let Some(o) = out.filter(|o| o.status.success()) else {
        return true;
    };
    let text = String::from_utf8_lossy(&o.stdout);
    let value = text.lines().find(|l| l.contains(VALUE)).and_then(|l| l.split_whitespace().last());
    value != Some("0x0")
}

/// The Lighting service watches this value and switches at once.
pub fn set(on: bool) -> Result<(), String> {
    let data = if on { "1" } else { "0" };
    let out = Command::new("reg")
        .args(["add", KEY, "/v", VALUE, "/t", "REG_DWORD", "/d", data, "/f"])
        .creation_flags(CREATE_NO_WINDOW)
        .output()
        .map_err(|e| e.to_string())?;
    if out.status.success() {
        Ok(())
    } else {
        Err(format!("reg add: {}", String::from_utf8_lossy(&out.stderr).trim()))
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

    const LINE: &str = "\r\nHKEY_CURRENT_USER\\Software\\Microsoft\\Lighting\r\n    AmbientLightingEnabled    REG_DWORD    ";

    #[test]
    fn reads_the_switch() {
        assert!(!parse(reg(0, &format!("{LINE}0x0\r\n"))));
        assert!(parse(reg(0, &format!("{LINE}0x1\r\n"))));
    }

    #[test]
    fn anything_unclear_counts_as_enabled() {
        assert!(parse(None));
        assert!(parse(reg(1, "")));
        assert!(parse(reg(0, "\r\nHKEY_CURRENT_USER\\Software\\Microsoft\\Lighting\r\n")));
    }
}
