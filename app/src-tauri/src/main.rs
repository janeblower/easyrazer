#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod autostart;
mod commands;
mod device;
mod dynamic_lighting;
mod settings;

use std::sync::Mutex;

fn main() {
    let device = device::Device::new().expect("hidapi init");
    tauri::Builder::default()
        .manage(commands::AppState { device: Mutex::new(device) })
        .invoke_handler(tauri::generate_handler![
            commands::status,
            commands::layout,
            commands::read_all,
            commands::apply,
            commands::lighting_state,
            commands::lighting_preview,
            commands::lighting_apply,
            commands::lighting_write,
            commands::set_confirm_write,
            commands::set_dynamic_lighting
        ])
        .run(tauri::generate_context!())
        .expect("tauri run");
}
