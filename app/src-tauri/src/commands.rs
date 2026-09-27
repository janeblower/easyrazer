//! Tauri commands: the only surface the UI can reach.

use std::collections::BTreeMap;
use std::sync::Mutex;

use razer_core::actuation::Outcome;
use razer_core::{analog, layout as kb_layout};
use serde::Serialize;
use tauri::{AppHandle, Emitter, State};

use crate::device::{Device, Status};

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

#[tauri::command]
pub async fn status(state: State<'_, AppState>, check_synapse: bool) -> Result<Status, String> {
    Ok(state.device.lock().unwrap().status(check_synapse))
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
