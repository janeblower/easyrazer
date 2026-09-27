#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod commands;
mod device;

use std::sync::Mutex;

fn main() {
    let device = device::Device::new().expect("hidapi init");
    tauri::Builder::default()
        .manage(commands::AppState { device: Mutex::new(device) })
        .invoke_handler(tauri::generate_handler![
            commands::status,
            commands::layout,
            commands::read_all,
            commands::apply
        ])
        .run(tauri::generate_context!())
        .expect("tauri run");
}
