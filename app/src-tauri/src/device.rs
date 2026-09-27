//! Owns the keyboard connection: opening, reconnecting, Synapse detection, device mode.

use std::os::windows::process::CommandExt;
use std::process::Command;

use hidapi::HidApi;
use razer_core::control;
use razer_core::hid::{self, HidTransport};
use razer_core::transport::Error;
use serde::Serialize;

const CREATE_NO_WINDOW: u32 = 0x0800_0000;

#[derive(Clone, Debug, Serialize)]
pub struct Status {
    pub device: bool,
    pub synapse: bool,
    pub mode: Option<u8>,
    pub profile: Option<u8>,
}

pub struct Device {
    api: HidApi,
    control: Option<HidTransport>,
    /// Last known Synapse state; assumed running until checked.
    synapse: bool,
}

impl Device {
    pub fn new() -> Result<Self, Error> {
        let api = HidApi::new().map_err(|e| Error::Io(e.to_string()))?;
        Ok(Self { api, control: None, synapse: true })
    }

    /// Connected control interface, reopened if the keyboard was replugged.
    pub fn connect(&mut self) -> Option<&HidTransport> {
        let alive = self.control.as_ref().is_some_and(|t| control::mode(t).is_ok());
        if !alive {
            self.control = None;
            let _ = self.api.refresh_devices();
            self.control = hid::open_control(&self.api).ok().flatten();
        }
        self.control.as_ref()
    }

    pub fn status(&mut self, check_synapse: bool) -> Status {
        if check_synapse {
            self.synapse = synapse_running();
        }
        let synapse = self.synapse;
        let Some(t) = self.connect() else {
            return Status { device: false, synapse, mode: None, profile: None };
        };
        let mut mode = control::mode(t).ok();
        if should_release_driver_mode(check_synapse, synapse, mode) && control::set_hardware_mode(t).is_ok() {
            mode = control::mode(t).ok();
        }
        Status { device: true, synapse, mode, profile: control::active_profile(t).ok() }
    }
}

/// Synapse leaves the keyboard in driver mode, where it types nothing on its own.
/// Take it back only right after confirming Synapse is gone, never on a stale answer:
/// otherwise we would fight a Synapse that has just started.
fn should_release_driver_mode(fresh_check: bool, synapse: bool, mode: Option<u8>) -> bool {
    fresh_check && !synapse && mode == Some(control::MODE_DRIVER)
}

pub fn synapse_running() -> bool {
    let out = Command::new("tasklist")
        .args(["/FI", "IMAGENAME eq RazerAppEngine.exe", "/NH"])
        .creation_flags(CREATE_NO_WINDOW)
        .output();
    synapse_in(out.ok().map(|o| o.stdout))
}

/// A failed check counts as "running": writing while Synapse is alive gets overwritten.
fn synapse_in(tasklist: Option<Vec<u8>>) -> bool {
    tasklist.is_none_or(|out| String::from_utf8_lossy(&out).contains("RazerAppEngine"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tasklist_failure_counts_as_synapse_running() {
        assert!(synapse_in(None));
    }

    #[test]
    fn detects_synapse_in_tasklist_output() {
        assert!(synapse_in(Some(b"RazerAppEngine.exe  1234 Console  1  80 000 K".to_vec())));
        assert!(!synapse_in(Some(b"INFO: No tasks are running which match the specified criteria.".to_vec())));
    }

    #[test]
    fn driver_mode_is_released_only_right_after_a_fresh_check() {
        assert!(should_release_driver_mode(true, false, Some(control::MODE_DRIVER)));
        assert!(!should_release_driver_mode(false, false, Some(control::MODE_DRIVER)));
        assert!(!should_release_driver_mode(true, true, Some(control::MODE_DRIVER)));
        assert!(!should_release_driver_mode(true, false, Some(control::MODE_HARDWARE)));
    }
}
