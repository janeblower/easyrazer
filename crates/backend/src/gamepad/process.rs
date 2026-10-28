//! The elevated helper that installs and removes the driver: this program run again with
//! administrator rights, reporting back over a control channel.

use crate::sys::{self, Channel, ControlMessage};
use std::io;
use std::os::windows::io::AsHandle;
use windows_sys::Win32::System::Threading::{INFINITE, PROCESS_SYNCHRONIZE};

pub(super) fn driver_action(action: &str) -> Result<(), String> {
    let run = || -> io::Result<()> {
        let mut channel = Channel::create()?;
        let args = format!("{action} {} {}", std::process::id(), channel.name);
        let process = sys::run_elevated(&std::env::current_exe()?, &args)?;
        channel.connect(process.as_handle())?;
        if !sys::wait(process.as_handle(), INFINITE)? {
            return Err(io::Error::last_os_error());
        }
        channel.result(process.as_handle())?;
        match sys::exit_code(process.as_handle())? {
            0 => Ok(()),
            code => Err(io::Error::other(format!("helper exited with code {code}"))),
        }
    };
    run().map_err(|e| e.to_string())
}

pub fn driver_helper(action: &str, args: &[String]) -> Result<(), String> {
    let (pid, name) = endpoint(args).map_err(|e| e.to_string())?;
    let mut channel = Channel::open(name, pid).map_err(|e| e.to_string())?;
    let parent = sys::open_process(pid, PROCESS_SYNCHRONIZE).map_err(|e| e.to_string())?;
    channel.wait_connected(parent.as_handle()).map_err(|e| e.to_string())?;
    let result = match action {
        "--gamepad-install" => super::drivers::deploy(false),
        "--gamepad-replace" => super::drivers::deploy(true),
        "--gamepad-uninstall" => super::drivers::remove(),
        "--gamepad-boot-on" => super::drivers::boot(true),
        "--gamepad-boot-off" => super::drivers::boot(false),
        _ => Err(format!("unknown gamepad action {action}")),
    };
    channel.send(&ControlMessage::finished(result.as_ref().err().map(String::as_str)), 30_000).map_err(|e| e.to_string())?;
    result
}

fn endpoint(args: &[String]) -> io::Result<(u32, &str)> {
    let [pid, name] = args else {
        return Err(io::Error::new(io::ErrorKind::InvalidInput, "expected parent PID and control channel"));
    };
    let pid = pid
        .parse::<u32>()
        .ok()
        .filter(|p| *p != 0)
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "invalid parent PID"))?;
    let prefix = format!(r"\\.\pipe\EasyRazerGamepad-{pid}-");
    if !name
        .strip_prefix(&prefix)
        .is_some_and(|s| s.len() == 64 && s.bytes().all(|b| b.is_ascii_hexdigit()))
    {
        return Err(io::Error::new(io::ErrorKind::InvalidInput, "invalid control channel"));
    }
    Ok((pid, name))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn malformed_endpoint_is_rejected_before_opening_handles() {
        let name = format!(r"\\.\pipe\EasyRazerGamepad-1234-{}", "ab".repeat(32));
        for args in [
            vec![],
            vec!["1234".into()],
            vec!["0".into(), name.clone()],
            vec!["4294967296".into(), name.clone()],
            vec!["1234".into(), "Global\\other".into()],
            vec!["1234".into(), format!(r"\\.\pipe\EasyRazerGamepad-1234-{}", "gg".repeat(32))],
        ] {
            assert_eq!(endpoint(&args).unwrap_err().kind(), io::ErrorKind::InvalidInput);
        }
        assert_eq!(endpoint(&["1234".into(), name.clone()]).unwrap(), (1234, name.as_str()));
    }
}
