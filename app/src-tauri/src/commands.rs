//! Tauri commands: the only surface the UI can reach.

use std::collections::BTreeMap;
use std::sync::{Mutex, MutexGuard};

use razer_core::actuation::Outcome;
use razer_core::lighting::Look;
use razer_core::layout as kb_layout;
use serde::Serialize;
use tauri::{AppHandle, Emitter, State, WebviewWindow};

use crate::device::{self, Device, LightingState, Status, Written, mm};
use crate::settings::CloseAction;
use crate::{autostart, tray};

pub struct AppState {
    pub device: Mutex<Device>,
}

impl AppState {
    /// A command that panicked mid-way leaves the device in a usable state, so poisoning is ignored.
    pub fn device(&self) -> MutexGuard<'_, Device> {
        self.device.lock().unwrap_or_else(|e| e.into_inner())
    }
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
    shape: kb_layout::Shape,
}

fn key_views(keys: Vec<kb_layout::LayoutKey>) -> Vec<KeyView> {
    keys.into_iter()
        .map(|k| KeyView { key: k.key, label: k.label, x: k.x, y: k.y, w: k.w, h: k.h, editable: k.editable, shape: k.shape })
        .collect()
}

#[derive(Serialize)]
#[serde(tag = "status", rename_all = "lowercase")]
pub enum ApplyResult {
    Ok { key: u8, mm: f32 },
    Unconfirmed { key: u8, mm: f32 },
    Error { key: u8, message: String },
}

#[derive(Serialize)]
pub struct Actuation {
    values: BTreeMap<u8, f32>,
    /// Keys whose press point a replug would reset.
    unsaved: Vec<u8>,
}

#[derive(Serialize)]
pub struct WriteResult {
    results: Vec<ApplyResult>,
    unsaved: Vec<u8>,
}

impl From<Written> for WriteResult {
    fn from(w: Written) -> Self {
        let results = w
            .results
            .into_iter()
            .map(|(key, o)| match o {
                Outcome::Ok(a) => ApplyResult::Ok { key, mm: mm(a.threshold_low) },
                Outcome::Unconfirmed(a) => ApplyResult::Unconfirmed { key, mm: mm(a.threshold_low) },
                Outcome::Failed(e) => ApplyResult::Error { key, message: e.to_string() },
            })
            .collect();
        Self { results, unsaved: w.unsaved }
    }
}

/// One watcher step: the Synapse check runs outside the lock, `tasklist` takes a few hundred ms.
pub fn poll(state: &AppState) -> Status {
    let watch = state.device().settings().watch_synapse;
    let synapse = device::synapse_check(watch, device::synapse_running);
    state.device().status(Some(synapse))
}

#[tauri::command]
pub async fn status(state: State<'_, AppState>) -> Result<Status, String> {
    Ok(poll(&state))
}

#[tauri::command]
pub fn layout() -> Vec<KeyView> {
    key_views(kb_layout::keys())
}

#[tauri::command]
pub fn lighting_layout() -> Vec<KeyView> {
    key_views(kb_layout::lighting_keys())
}

#[tauri::command]
pub async fn read_all(app: AppHandle, state: State<'_, AppState>) -> Result<Actuation, String> {
    let (all, unsaved) = state.device().read_all(|done, total| {
        let _ = app.emit("read-progress", (done, total));
    })?;
    Ok(Actuation { values: all.iter().map(|a| (a.key, mm(a.threshold_low))).collect(), unsaved })
}

#[tauri::command]
pub async fn apply(state: State<'_, AppState>, changes: Vec<(u8, f32)>) -> Result<WriteResult, String> {
    state.device().apply(&changes).map(Into::into)
}

#[tauri::command]
pub async fn save(state: State<'_, AppState>, changes: Vec<(u8, f32)>) -> Result<WriteResult, String> {
    state.device().save(&changes).map(Into::into)
}

#[tauri::command]
pub async fn lighting_state(state: State<'_, AppState>) -> Result<LightingState, String> {
    state.device().lighting_state()
}

#[tauri::command]
pub async fn lighting_preview(state: State<'_, AppState>, look: Look) -> Result<(), String> {
    state.device().lighting_preview(&look)
}

#[tauri::command]
pub async fn lighting_apply(state: State<'_, AppState>, look: Look) -> Result<(), String> {
    state.device().lighting_apply(look)
}

#[tauri::command]
pub async fn lighting_write(state: State<'_, AppState>, look: Look) -> Result<(), String> {
    state.device().lighting_write(look)
}

#[tauri::command]
pub async fn set_confirm_write(state: State<'_, AppState>, on: bool) -> Result<(), String> {
    state.device().set_confirm_write(on)
}

#[tauri::command]
pub async fn set_dynamic_lighting(state: State<'_, AppState>, on: bool) -> Result<(), String> {
    state.device().set_dynamic_lighting(on)
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
    let s = state.device().settings().clone();
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
    state.device().update_settings(|s| s.autostart_offered = true)
}

#[tauri::command]
pub async fn set_close_action(state: State<'_, AppState>, action: CloseAction) -> Result<(), String> {
    state.device().update_settings(|s| s.close_action = action)
}

#[tauri::command]
pub async fn set_watch_synapse(state: State<'_, AppState>, on: bool) -> Result<(), String> {
    state.device().update_settings(|s| s.watch_synapse = on)
}

#[tauri::command]
pub fn hide_window(window: WebviewWindow) -> Result<(), String> {
    window.hide().map_err(|e| e.to_string())
}

#[tauri::command]
pub fn quit(app: AppHandle) {
    app.exit(0);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_panic_under_the_lock_does_not_lock_everyone_out() {
        let state = AppState { device: Mutex::new(Device::new().unwrap()) };
        let _ = std::thread::scope(|s| {
            s.spawn(|| {
                let _guard = state.device();
                panic!("command failed");
            })
            .join()
        });
        assert!(state.device.is_poisoned());
        let _ = state.device().settings();
    }
}
