//! Tauri commands: the only surface the UI can reach.

use std::collections::BTreeMap;
use std::sync::Mutex;

use razer_core::actuation::Outcome;
use razer_core::lighting::Look;
use razer_core::{analog, layout as kb_layout};
use serde::Serialize;
use tauri::{AppHandle, Emitter, State, WebviewWindow};

use crate::device::{self, Device, LightingState, Status};
use crate::settings::CloseAction;
use crate::{autostart, tray};

pub struct AppState {
    pub device: Mutex<Device>,
}

#[derive(Serialize)]
pub struct KeyView {
    key: u8,
    label: &'static str,
    x: f32,
    y: f32,
    w: f32,
    h: f32,
    editable: bool,
}

#[derive(Serialize)]
#[serde(tag = "status", rename_all = "lowercase")]
pub enum ApplyResult {
    Ok { key: u8, mm: f32 },
    Unconfirmed { key: u8, mm: f32 },
    Error { key: u8, message: String },
}

fn mm(threshold: u8) -> f32 {
    (analog::threshold_to_mm(threshold) * 10.0).round() / 10.0
}

/// One watcher step: the Synapse check runs outside the lock, `tasklist` takes a few hundred ms.
pub fn poll(state: &AppState) -> Status {
    let watch = state.device.lock().unwrap().settings().watch_synapse;
    let synapse = device::synapse_check(watch, device::synapse_running);
    state.device.lock().unwrap().status(Some(synapse))
}

#[tauri::command]
pub async fn status(state: State<'_, AppState>) -> Result<Status, String> {
    Ok(poll(&state))
}

#[tauri::command]
pub fn layout() -> Vec<KeyView> {
    kb_layout::keys()
        .into_iter()
        .map(|k| KeyView { key: k.key, label: k.label, x: k.x, y: k.y, w: k.w, h: k.h, editable: k.editable })
        .collect()
}

#[tauri::command]
pub async fn read_all(app: AppHandle, state: State<'_, AppState>) -> Result<BTreeMap<u8, f32>, String> {
    let all = state.device.lock().unwrap().read_all(|done, total| {
        let _ = app.emit("read-progress", (done, total));
    })?;
    Ok(all.iter().map(|a| (a.key, mm(a.threshold_low))).collect())
}

#[tauri::command]
pub async fn apply(state: State<'_, AppState>, changes: Vec<(u8, f32)>) -> Result<Vec<ApplyResult>, String> {
    let results = state.device.lock().unwrap().apply(&changes)?;
    Ok(results
        .into_iter()
        .map(|(key, o)| match o {
            Outcome::Ok(a) => ApplyResult::Ok { key, mm: mm(a.threshold_low) },
            Outcome::Unconfirmed(a) => ApplyResult::Unconfirmed { key, mm: mm(a.threshold_low) },
            Outcome::Failed(e) => ApplyResult::Error { key, message: e.to_string() },
        })
        .collect())
}

#[tauri::command]
pub async fn lighting_state(state: State<'_, AppState>) -> Result<LightingState, String> {
    state.device.lock().unwrap().lighting_state()
}

#[tauri::command]
pub async fn lighting_preview(state: State<'_, AppState>, look: Look) -> Result<(), String> {
    state.device.lock().unwrap().lighting_preview(&look)
}

#[tauri::command]
pub async fn lighting_apply(state: State<'_, AppState>, look: Look) -> Result<(), String> {
    state.device.lock().unwrap().lighting_apply(look)
}

#[tauri::command]
pub async fn lighting_write(state: State<'_, AppState>, look: Look) -> Result<(), String> {
    state.device.lock().unwrap().lighting_write(look)
}

#[tauri::command]
pub async fn set_confirm_write(state: State<'_, AppState>, on: bool) -> Result<(), String> {
    state.device.lock().unwrap().set_confirm_write(on)
}

#[tauri::command]
pub async fn set_dynamic_lighting(state: State<'_, AppState>, on: bool) -> Result<(), String> {
    state.device.lock().unwrap().set_dynamic_lighting(on)
}

#[derive(Serialize)]
pub struct AppSettings {
    autostart: bool,
    close_action: CloseAction,
    watch_synapse: bool,
    confirm_write: bool,
    autostart_offered: bool,
}

#[tauri::command]
pub async fn app_settings(state: State<'_, AppState>) -> Result<AppSettings, String> {
    let s = state.device.lock().unwrap().settings().clone();
    Ok(AppSettings {
        autostart: autostart::enabled(),
        close_action: s.close_action,
        watch_synapse: s.watch_synapse,
        confirm_write: s.confirm_write,
        autostart_offered: s.autostart_offered,
    })
}

#[tauri::command]
pub async fn set_autostart(app: AppHandle, on: bool) -> Result<(), String> {
    let r = autostart::set(on);
    tray::sync_autostart(&app);
    r
}

/// Any answer to the first-run question is final; a failed registry write leaves it unanswered.
#[tauri::command]
pub async fn autostart_answered(app: AppHandle, state: State<'_, AppState>, on: bool) -> Result<(), String> {
    if on {
        let r = autostart::set(true);
        tray::sync_autostart(&app);
        r?;
    }
    state.device.lock().unwrap().update_settings(|s| s.autostart_offered = true)
}

#[tauri::command]
pub async fn set_close_action(state: State<'_, AppState>, action: CloseAction) -> Result<(), String> {
    state.device.lock().unwrap().update_settings(|s| s.close_action = action)
}

#[tauri::command]
pub async fn set_watch_synapse(state: State<'_, AppState>, on: bool) -> Result<(), String> {
    state.device.lock().unwrap().update_settings(|s| s.watch_synapse = on)
}

#[tauri::command]
pub fn hide_window(window: WebviewWindow) -> Result<(), String> {
    window.hide().map_err(|e| e.to_string())
}

#[tauri::command]
pub fn quit(app: AppHandle) {
    app.exit(0);
}
