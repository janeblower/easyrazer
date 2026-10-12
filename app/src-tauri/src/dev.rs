//! Developer mode: the keyboard traffic log, the command console and the bug-report text.

use std::collections::BTreeMap;
use std::fmt::Write;

use razer_core::logged::{self, Entry, Listener};
use razer_core::packet::{ARGS_LEN, Command};
use razer_core::profiles::RawKey;
use razer_core::registry::{self, CommandInfo};
use razer_core::{control, keymap, macros};
use serde::Serialize;
use tauri::{AppHandle, Emitter, State};

use crate::commands::AppState;
use crate::device::Device;
use crate::settings::{DEV_LOG_SIZES, DevPanel, DevWindow, Settings};

/// Log lines a bug report carries.
const REPORT_ENTRIES: usize = 200;

#[derive(Serialize)]
pub struct DeviceInfo {
    model: String,
    vid: u16,
    pid: u16,
    interface: i32,
    tid: u8,
    lighting_protocol: String,
    led: u8,
    lamp_array: bool,
    mode: Option<u8>,
    startup_slot: Option<u8>,
    loaded_profile: Option<u32>,
    firmware: Option<String>,
    serial: Option<String>,
}

#[derive(Serialize)]
pub struct DevKey {
    name: &'static str,
    normal: Option<RawKey>,
    hypershift: Option<RawKey>,
}

/// Streams the log to every window and keeps the larger log while the mode is on.
pub fn sync_listener(app: &AppHandle, device: &Device) {
    let s = device.settings();
    device.log().set_capacity(if s.dev_mode { s.dev_log_size } else { logged::CAPACITY });
    let listener = device.settings().dev_mode.then(|| {
        let app = app.clone();
        Box::new(move |e: &Entry| {
            let _ = app.emit("hid-log", e);
        }) as Listener
    });
    device.log().listen(listener);
}

#[tauri::command]
pub async fn set_dev_mode(app: AppHandle, state: State<'_, AppState>, on: bool) -> Result<(), String> {
    let mut device = state.device();
    device.update_settings(|s| {
        s.dev_mode = on;
        if on && s.dev_panel.is_none() {
            s.dev_panel = Some(DevPanel::default());
        }
    })?;
    sync_listener(&app, &device);
    let _ = app.emit("settings-changed", ());
    Ok(())
}

#[tauri::command]
pub async fn set_dev_log_size(app: AppHandle, state: State<'_, AppState>, size: usize) -> Result<(), String> {
    let mut device = state.device();
    device.update_settings(|s| s.dev_log_size = size.clamp(*DEV_LOG_SIZES.start(), *DEV_LOG_SIZES.end()))?;
    sync_listener(&app, &device);
    let _ = app.emit("settings-changed", ());
    Ok(())
}

#[tauri::command]
pub async fn set_dev_panel(app: AppHandle, state: State<'_, AppState>, panel: DevPanel) -> Result<(), String> {
    state.device().update_settings(|s| s.dev_panel = Some(panel))?;
    let _ = app.emit("settings-changed", ());
    Ok(())
}

#[tauri::command]
pub async fn set_dev_window(state: State<'_, AppState>, geom: DevWindow) -> Result<(), String> {
    state.device().update_settings(|s| s.dev_window = Some(geom))
}

#[tauri::command]
pub async fn dev_log(state: State<'_, AppState>) -> Result<Vec<Entry>, String> {
    Ok(state.device().log().entries())
}

#[tauri::command]
pub fn dev_commands() -> BTreeMap<String, CommandInfo> {
    registry::all().clone()
}

#[tauri::command]
pub async fn dev_device(state: State<'_, AppState>) -> Result<Option<DeviceInfo>, String> {
    Ok(device_info(&mut state.device()))
}

fn device_info(device: &mut Device) -> Option<DeviceInfo> {
    let loaded_profile = device.loaded();
    let (t, d) = device.connect()?;
    Some(DeviceInfo {
        model: d.name.clone(),
        vid: d.vid,
        pid: d.pid,
        interface: d.control_interface,
        tid: d.tid,
        lighting_protocol: d.lighting.protocol.clone(),
        led: d.lighting.led,
        lamp_array: d.lamp_array.is_some(),
        mode: control::mode(t).ok(),
        startup_slot: control::active_profile(t).ok(),
        loaded_profile,
        firmware: control::firmware(t).ok().map(|(major, minor)| format!("{major}.{minor:02}")),
        serial: control::serial(t).ok(),
    })
}

#[tauri::command]
pub async fn dev_exchange(state: State<'_, AppState>, class: u8, id: u8, size: u8, args: Vec<u8>) -> Result<Entry, String> {
    // `packet::request` asserts on this; the console is a trust boundary.
    if args.len() > ARGS_LEN {
        return Err(format!("more than {ARGS_LEN} bytes"));
    }
    state.device().dev_exchange(Command::new(class, id), size, &args)
}

#[tauri::command]
pub async fn dev_keys(state: State<'_, AppState>, keys: Vec<u8>) -> Result<BTreeMap<u8, DevKey>, String> {
    let device = state.device();
    let Some(p) = device.settings().loaded_profile() else { return Ok(BTreeMap::new()) };
    Ok(keys
        .into_iter()
        .map(|k| {
            let key = DevKey { name: keymap::name(k).unwrap_or(""), normal: p.data.normal.get(&k).cloned(), hypershift: p.data.hypershift.get(&k).cloned() };
            (k, key)
        })
        .collect())
}

#[tauri::command]
pub async fn dev_macro(state: State<'_, AppState>, id: u16) -> Result<Option<Vec<u8>>, String> {
    Ok(state.device().settings().macros.get(&id).and_then(|m| macros::encode(&m.events)))
}

#[tauri::command]
pub async fn dev_diagnostics(state: State<'_, AppState>, uncensored: bool) -> Result<String, String> {
    let mut device = state.device();
    let info = device_info(&mut device);
    Ok(report(info.as_ref(), device.settings(), &device.log().entries(), uncensored))
}

fn hex(b: &[u8]) -> String {
    b.iter().map(|x| format!("{x:02X}")).collect::<Vec<_>>().join(" ")
}

fn hex_cut(b: &[u8], keep: usize) -> String {
    let mut s = hex(&b[..keep]);
    if b.len() > keep {
        let _ = write!(s, " <{} bytes hidden>", b.len() - keep);
    }
    s
}

/// Plain text for a bug report; profile and macro names never go in, and censored hides what may be private.
pub fn report(info: Option<&DeviceInfo>, s: &Settings, entries: &[Entry], uncensored: bool) -> String {
    let mut out = format!("EasyRazer {} on {} {}\n", env!("CARGO_PKG_VERSION"), std::env::consts::OS, std::env::consts::ARCH);
    match info {
        Some(i) => {
            let serial = if uncensored { i.serial.as_deref().unwrap_or("?") } else { "hidden" };
            let _ = writeln!(
                out,
                "device: {} {:04X}:{:04X} interface {} tid {:02X}, lighting {} led {:02X}, LampArray {}, mode {:?}, startup slot {:?}, firmware {}, serial {serial}",
                i.model,
                i.vid,
                i.pid,
                i.interface,
                i.tid,
                i.lighting_protocol,
                i.led,
                i.lamp_array,
                i.mode,
                i.startup_slot,
                i.firmware.as_deref().unwrap_or("?"),
            );
        }
        None => out.push_str("device: not connected\n"),
    }
    let _ = writeln!(
        out,
        "settings: {} profiles, loaded {:?}, slots {:?}, {} macros, driver mode {}, confirm write {}, watch Synapse {}, on close {:?}, language {:?}",
        s.profiles.len(),
        s.loaded,
        s.slots.keys().collect::<Vec<_>>(),
        s.macros.len(),
        s.driver_mode,
        s.confirm_write,
        s.watch_synapse,
        s.close_action,
        s.language,
    );
    out.push_str("\nlog:\n");
    for e in &entries[entries.len().saturating_sub(REPORT_ENTRIES)..] {
        let (a, r) = if uncensored { (e.args.len(), e.reply.len()) } else { logged::public_len(e) };
        let _ = write!(out, "#{} {} {} [{}] {} <- {} {} {}ms", e.seq, e.time, e.cmd, e.size, hex_cut(&e.args, a), e.status.as_deref().unwrap_or("-"), hex_cut(&e.reply, r), e.ms);
        if e.polls > 0 {
            let _ = write!(out, " busy x{}", e.polls);
        }
        if e.background {
            out.push_str(" bg");
        }
        if let Some(err) = &e.error {
            let _ = write!(out, " error: {err}");
        }
        out.push('\n');
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(seq: u64, cmd: &str, args: Vec<u8>, reply: Vec<u8>) -> Entry {
        Entry { seq, cmd: cmd.into(), size: args.len() as u8, args, reply, ..Default::default() }
    }

    fn info() -> DeviceInfo {
        DeviceInfo {
            model: "Kb".into(),
            vid: 0x1532,
            pid: 0x0266,
            interface: 3,
            tid: 0x1F,
            lighting_protocol: "extended".into(),
            led: 5,
            lamp_array: true,
            mode: Some(3),
            startup_slot: Some(1),
            loaded_profile: Some(1),
            firmware: Some("1.02".into()),
            serial: Some("PM2149H00012345".into()),
        }
    }

    #[test]
    fn a_censored_report_hides_macro_bodies_names_and_the_serial() {
        let entries = [
            entry(0, "06:09", vec![0, 1, 0, 0, 0, 0, 3, 0xAA, 0xBB, 0xCC], vec![]),
            entry(1, "05:08", vec![1, 0, 0, 0, 4, 0, 0x41, 0, 0x42], vec![1, 0, 0, 0, 4, 0, 0x41, 0, 0x42]),
            entry(2, "05:88", vec![1, 0, 0, 0, 0x40], vec![1, 0, 0, 0, 4, 0, 0x41, 0, 0x42]),
            entry(3, "00:82", vec![], b"PM2149H00012345".to_vec()),
        ];
        let s = Settings::default();
        let censored = report(Some(&info()), &s, &entries, false);
        assert!(!censored.contains("AA BB CC") && !censored.contains("00 41 00 42") && !censored.contains("PM2149") && !censored.contains("50 4D 32 31"));
        assert!(censored.contains("<3 bytes hidden>") && censored.contains("<4 bytes hidden>"));
        let raw = report(Some(&info()), &s, &entries, true);
        assert!(raw.contains("AA BB CC") && raw.contains("00 41 00 42") && raw.contains("PM2149") && raw.contains("50 4D 32 31"));
    }

    #[test]
    fn a_report_carries_the_last_entries() {
        let entries: Vec<Entry> = (0..250).map(|i| entry(i, "00:84", vec![], vec![3, 0])).collect();
        let r = report(None, &Settings::default(), &entries, false);
        assert!(r.contains("\n#50 ") && !r.contains("\n#49 ") && r.contains("\n#249 "));
        assert!(r.contains("device: not connected"));
    }
}
