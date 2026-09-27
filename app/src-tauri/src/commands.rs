//! Tauri commands: the only surface the UI can reach.

use std::sync::Mutex;

use tauri::State;

use crate::device::{Device, Status};

pub struct AppState {
    pub device: Mutex<Device>,
}

#[tauri::command]
pub async fn status(state: State<'_, AppState>, check_synapse: bool) -> Result<Status, String> {
    Ok(state.device.lock().unwrap().status(check_synapse))
}
