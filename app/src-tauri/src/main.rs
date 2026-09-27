#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod autostart;
mod commands;
mod device;
mod dynamic_lighting;
mod i18n;
mod settings;
mod tray;

use std::panic::AssertUnwindSafe;
use std::sync::Mutex;
use std::time::Duration;

use settings::CloseAction;
use tauri::{Emitter, Manager, WindowEvent};

fn main() {
    let device = device::Device::new().expect("hidapi init");
    tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(|app, _, _| tray::show(app)))
        .manage(commands::AppState { device: Mutex::new(device) })
        .setup(|app| {
            tray::build(app)?;
            if !std::env::args().any(|a| a == autostart::TRAY_ARG) {
                tray::show(app.handle());
            }
            let handle = app.handle().clone();
            std::thread::spawn(move || loop {
                // One failed step must not end the watching for the rest of the session.
                let step = std::panic::catch_unwind(AssertUnwindSafe(|| commands::poll(&handle.state::<commands::AppState>())));
                if let Ok(status) = step {
                    let _ = handle.emit("status", &status);
                }
                std::thread::sleep(Duration::from_secs(2));
            });
            Ok(())
        })
        .on_window_event(|w, e| {
            if let WindowEvent::CloseRequested { api, .. } = e {
                let action = w.state::<commands::AppState>().device().settings().close_action;
                match action {
                    CloseAction::Ask => {
                        api.prevent_close();
                        let _ = w.emit("close-requested", ());
                    }
                    CloseAction::Tray => {
                        api.prevent_close();
                        let _ = w.hide();
                    }
                    CloseAction::Exit => w.app_handle().exit(0),
                }
            }
        })
        .invoke_handler(tauri::generate_handler![
            commands::status,
            commands::layout,
            commands::read_all,
            commands::apply,
            commands::save,
            commands::lighting_state,
            commands::lighting_layout,
            commands::lighting_preview,
            commands::lighting_apply,
            commands::lighting_write,
            commands::set_confirm_write,
            commands::set_dynamic_lighting,
            commands::app_settings,
            commands::set_autostart,
            commands::autostart_answered,
            commands::set_close_action,
            commands::set_watch_synapse,
            commands::set_language,
            commands::hide_window,
            commands::quit
        ])
        .run(tauri::generate_context!())
        .expect("tauri run");
}
