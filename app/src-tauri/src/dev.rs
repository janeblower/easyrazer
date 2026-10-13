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

use crate::commands::{AppState, lock};
use crate::device::Device;
use crate::settings::{DEV_LOG_SIZES, DevPanel, DevWindow, DeviceData, Settings};

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

#[derive(Clone, Serialize)]
struct LogEvent<'a> {
    id: &'a str,
    entry: &'a Entry,
}

/// Streams the log to every window and keeps the larger log while the mode is on.
pub fn sync_listener(app: &AppHandle, dev_mode: bool, log_size: usize, id: &str, device: &Device) {
    device.log().set_capacity(if dev_mode { log_size } else { logged::CAPACITY });
    let listener = dev_mode.then(|| {
        let (app, id) = (app.clone(), id.to_string());
        Box::new(move |e: &Entry| {
            let _ = app.emit("hid-log", LogEvent { id: &id, entry: e });
        }) as Listener
    });
    device.log().listen(listener);
}

#[tauri::command]
pub async fn set_dev_mode(app: AppHandle, state: State<'_, AppState>, on: bool) -> Result<(), String> {
    state.update_settings(|s| {
        s.dev_mode = on;
        if on && s.dev_panel.is_none() {
            s.dev_panel = Some(DevPanel::default());
        }
    })?;
    let (mode, size) = { let s = state.settings(); (s.dev_mode, s.dev_log_size) };
    for (id, d) in state.boards.all() {
        sync_listener(&app, mode, size, &id, &lock(&d));
    }
    let _ = app.emit("settings-changed", ());
    Ok(())
}

#[tauri::command]
pub async fn set_dev_log_size(app: AppHandle, state: State<'_, AppState>, size: usize) -> Result<(), String> {
    state.update_settings(|s| s.dev_log_size = size.clamp(*DEV_LOG_SIZES.start(), *DEV_LOG_SIZES.end()))?;
    let (mode, size) = { let s = state.settings(); (s.dev_mode, s.dev_log_size) };
    for (id, d) in state.boards.all() {
        sync_listener(&app, mode, size, &id, &lock(&d));
    }
    let _ = app.emit("settings-changed", ());
    Ok(())
}

#[tauri::command]
pub async fn set_dev_panel(app: AppHandle, state: State<'_, AppState>, panel: DevPanel) -> Result<(), String> {
    state.update_settings(|s| s.dev_panel = Some(panel))?;
    let _ = app.emit("settings-changed", ());
    Ok(())
}

#[tauri::command]
pub async fn set_dev_window(state: State<'_, AppState>, geom: DevWindow) -> Result<(), String> {
    state.update_settings(|s| s.dev_window = Some(geom))
}

#[tauri::command]
pub async fn dev_log(state: State<'_, AppState>, id: String) -> Result<Vec<Entry>, String> {
    let d = state.device(&id)?;
    Ok(lock(&d).log().entries())
}

#[tauri::command]
pub fn dev_commands() -> BTreeMap<String, CommandInfo> {
    registry::all().clone()
}

#[tauri::command]
pub async fn dev_device(state: State<'_, AppState>, id: String) -> Result<Option<DeviceInfo>, String> {
    let d = state.device(&id)?;
    Ok(device_info(&mut lock(&d)))
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
pub async fn dev_exchange(state: State<'_, AppState>, id: String, class: u8, cmd: u8, size: u8, args: Vec<u8>) -> Result<Entry, String> {
    // `packet::request` asserts on this; the console is a trust boundary.
    if args.len() > ARGS_LEN {
        return Err(format!("more than {ARGS_LEN} bytes"));
    }
    let d = state.device(&id)?;
    lock(&d).dev_exchange(Command::new(class, cmd), size, &args)
}

#[tauri::command]
pub async fn dev_keys(state: State<'_, AppState>, id: String, keys: Vec<u8>) -> Result<BTreeMap<u8, DevKey>, String> {
    let d = state.device(&id)?;
    let device = lock(&d);
    let Some(p) = device.data().loaded_profile() else { return Ok(BTreeMap::new()) };
    Ok(keys
        .into_iter()
        .map(|k| {
            let key = DevKey { name: keymap::name(k).unwrap_or(""), normal: p.data.normal.get(&k).cloned(), hypershift: p.data.hypershift.get(&k).cloned() };
            (k, key)
        })
        .collect())
}

#[tauri::command]
pub async fn dev_macro(state: State<'_, AppState>, id: String, macro_id: u16) -> Result<Option<Vec<u8>>, String> {
    let d = state.device(&id)?;
    let r = lock(&d).data().macros.get(&macro_id).and_then(|m| macros::encode(&m.events));
    Ok(r)
}

#[tauri::command]
pub async fn dev_diagnostics(state: State<'_, AppState>, id: String, uncensored: bool) -> Result<String, String> {
    let s = state.settings().clone();
    let d = state.device(&id)?;
    let mut device = lock(&d);
    let info = device_info(&mut device);
    Ok(report(info.as_ref(), &s, device.data(), &device.log().entries(), uncensored))
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
pub fn report(info: Option<&DeviceInfo>, s: &Settings, d: &DeviceData, entries: &[Entry], uncensored: bool) -> String {
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
        d.profiles.len(),
        d.loaded,
        d.slots.keys().collect::<Vec<_>>(),
        d.macros.len(),
        d.driver_mode,
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
        let censored = report(Some(&info()), &s, &DeviceData::default(), &entries, false);
        assert!(!censored.contains("AA BB CC") && !censored.contains("00 41 00 42") && !censored.contains("PM2149") && !censored.contains("50 4D 32 31"));
        assert!(censored.contains("<3 bytes hidden>") && censored.contains("<4 bytes hidden>"));
        let raw = report(Some(&info()), &s, &DeviceData::default(), &entries, true);
        assert!(raw.contains("AA BB CC") && raw.contains("00 41 00 42") && raw.contains("PM2149") && raw.contains("50 4D 32 31"));
    }

    #[test]
    fn a_report_carries_the_last_entries() {
        let entries: Vec<Entry> = (0..250).map(|i| entry(i, "00:84", vec![], vec![3, 0])).collect();
        let r = report(None, &Settings::default(), &DeviceData::default(), &entries, false);
        assert!(r.contains("\n#50 ") && !r.contains("\n#49 ") && r.contains("\n#249 "));
        assert!(r.contains("device: not connected"));
    }
}
