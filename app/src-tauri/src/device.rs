//! Owns the keyboard connection: opening, reconnecting, Synapse detection, device mode.

use std::os::windows::process::CommandExt;
use std::path::PathBuf;
use std::process::{Command, Output};
use std::time::{SystemTime, UNIX_EPOCH};

use hidapi::HidApi;
use razer_core::actuation::{self, Outcome};
use razer_core::analog::KeyAssignment;
use razer_core::devices::DeviceSpec;
use razer_core::hid::{self, HidTransport};
use razer_core::transport::Error;
use razer_core::{control, layout};
use serde::Serialize;

pub const CREATE_NO_WINDOW: u32 = 0x0800_0000;

pub const SYNAPSE_RUNNING: &str = "Запущен Synapse — закройте его, включая значок в трее";
pub const NO_KEYBOARD: &str = "Поддерживаемая клавиатура Razer не найдена";

#[derive(Clone, Debug, Serialize)]
pub struct Status {
    pub device: bool,
    pub synapse: bool,
    pub mode: Option<u8>,
    pub profile: Option<u8>,
    pub model: Option<String>,
    /// PID of a Razer keyboard without a description.
    pub unsupported: Option<u16>,
}

pub struct Device {
    api: HidApi,
    control: Option<(HidTransport, &'static DeviceSpec)>,
    /// Last known Synapse state; assumed running until checked.
    synapse: bool,
    /// A backup has been written this session.
    backed_up: bool,
}

impl Device {
    pub fn new() -> Result<Self, Error> {
        let api = HidApi::new().map_err(|e| Error::Io(e.to_string()))?;
        Ok(Self { api, control: None, synapse: true, backed_up: false })
    }

    /// Reopens the control interface if the keyboard was replugged; `true` when connected.
    pub fn ensure_connected(&mut self) -> bool {
        let alive = self.control.as_ref().is_some_and(|(t, _)| control::mode(t).is_ok());
        if !alive {
            self.control = None;
            let _ = self.api.refresh_devices();
            self.control = hid::open_control(&self.api).ok().flatten();
        }
        self.control.is_some()
    }

    pub fn connect(&mut self) -> Option<(&HidTransport, &'static DeviceSpec)> {
        self.ensure_connected();
        self.control.as_ref().map(|(t, d)| (t, *d))
    }

    pub fn status(&mut self, check_synapse: bool) -> Status {
        if check_synapse {
            self.synapse = synapse_running();
        }
        let synapse = self.synapse;
        self.ensure_connected();
        let Some((t, spec)) = &self.control else {
            let unsupported = hid::unsupported_keyboard(&self.api);
            return Status { device: false, synapse, mode: None, profile: None, model: None, unsupported };
        };
        let mut mode = control::mode(t).ok();
        if should_release_driver_mode(check_synapse, synapse, mode) && control::set_hardware_mode(t).is_ok() {
            mode = control::mode(t).ok();
        }
        let profile = control::active_profile(t).ok();
        Status { device: true, synapse, mode, profile, model: Some(spec.name.clone()), unsupported: None }
    }

    /// Press points of every editable key in the active profile.
    pub fn read_all(&mut self, mut progress: impl FnMut(usize, usize)) -> Result<Vec<KeyAssignment>, String> {
        if self.synapse {
            return Err(SYNAPSE_RUNNING.into());
        }
        let (t, _) = self.connect().ok_or(NO_KEYBOARD)?;
        let profile = control::active_profile(t).map_err(|e| e.to_string())?;
        let keys = editable_keys();
        actuation::read_all(t, profile, &keys, |n| progress(n, keys.len())).map_err(|e| e.to_string())
    }

    /// Writes press points; checks Synapse afresh and backs up the keyboard before the first write.
    pub fn apply(&mut self, changes: &[(u8, f32)]) -> Result<Vec<(u8, Outcome)>, String> {
        if changes.is_empty() {
            return Ok(Vec::new());
        }
        self.synapse = synapse_running();
        if self.synapse {
            return Err(SYNAPSE_RUNNING.into());
        }
        let backed_up = self.backed_up;
        let (t, _) = self.connect().ok_or(NO_KEYBOARD)?;
        let profile = control::active_profile(t).map_err(|e| e.to_string())?;
        if !backed_up {
            let all = actuation::read_all(t, profile, &editable_keys(), |_| {}).map_err(|e| e.to_string())?;
            write_backup(&actuation::format_backup(profile, &all))?;
        }
        let results = actuation::apply(t, profile, changes);
        self.backed_up = true;
        Ok(results)
    }
}

fn editable_keys() -> Vec<u8> {
    layout::keys().into_iter().filter(|k| k.editable).map(|k| k.key).collect()
}

fn write_backup(text: &str) -> Result<(), String> {
    let appdata = std::env::var("APPDATA").map_err(|e| e.to_string())?;
    let dir = PathBuf::from(appdata).join("EasyRazer");
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let secs = SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |d| d.as_secs());
    std::fs::write(dir.join(format!("backup-{secs}.txt")), text).map_err(|e| e.to_string())
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
    synapse_in(out.ok())
}

/// A failed check counts as "running": writing while Synapse is alive gets overwritten.
/// With no match tasklist still prints a (localized) notice and exits 0, so empty output is a failure too.
fn synapse_in(tasklist: Option<Output>) -> bool {
    tasklist.filter(|o| o.status.success() && !o.stdout.is_empty()).is_none_or(|o| {
        String::from_utf8_lossy(&o.stdout).contains("RazerAppEngine")
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    use std::os::windows::process::ExitStatusExt;
    use std::process::ExitStatus;

    fn tasklist(code: u32, stdout: &[u8]) -> Option<Output> {
        Some(Output { status: ExitStatus::from_raw(code), stdout: stdout.to_vec(), stderr: Vec::new() })
    }

    #[test]
    fn tasklist_failure_counts_as_synapse_running() {
        assert!(synapse_in(None));
        assert!(synapse_in(tasklist(1, b"")));
        assert!(synapse_in(tasklist(0, b"")));
    }

    #[test]
    fn detects_synapse_in_tasklist_output() {
        assert!(synapse_in(tasklist(0, b"RazerAppEngine.exe  1234 Console  1  80 000 K")));
        assert!(!synapse_in(tasklist(0, b"INFO: No tasks are running which match the specified criteria.")));
    }

    #[test]
    fn driver_mode_is_released_only_right_after_a_fresh_check() {
        assert!(should_release_driver_mode(true, false, Some(control::MODE_DRIVER)));
        assert!(!should_release_driver_mode(false, false, Some(control::MODE_DRIVER)));
        assert!(!should_release_driver_mode(true, true, Some(control::MODE_DRIVER)));
        assert!(!should_release_driver_mode(true, false, Some(control::MODE_HARDWARE)));
    }

    use razer_core::{analog, keymap};

    #[test]
    #[ignore = "needs the keyboard connected and Synapse closed"]
    fn apply_round_trip_on_hardware() {
        assert!(!synapse_running(), "close Synapse first");
        let mut dev = Device::new().unwrap();
        let (profile, a, original) = {
            let (t, _) = dev.connect().expect("keyboard not connected");
            let profile = control::active_profile(t).unwrap();
            let a = keymap::by_name("A").unwrap();
            (profile, a, actuation::read_key(t, profile, a).unwrap())
        };
        let result = dev.apply(&[(a, 2.4)]).unwrap();
        let (t, _) = dev.connect().unwrap();
        actuation::write_key(t, &original).unwrap();
        assert!(
            matches!(&result[0].1, Outcome::Ok(s) if s.threshold_low == analog::mm_to_threshold(2.4)),
            "{result:?}"
        );
        assert_eq!(actuation::read_key(t, profile, a).unwrap(), original);
    }
}
