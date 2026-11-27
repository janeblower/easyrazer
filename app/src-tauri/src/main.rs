#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]
#![deny(unsafe_code)]

mod commands;
mod tray;

use std::sync::{Arc, mpsc};

use easyrazer_backend::api::Event;
use easyrazer_backend::settings::CloseAction;
use easyrazer_backend::{Backend, autostart, device, gamepad};
use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager, WindowEvent};

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.first().is_some_and(|a| matches!(a.as_str(), "--gamepad-install" | "--gamepad-replace" | "--gamepad-uninstall" | "--gamepad-boot-on" | "--gamepad-boot-off")) {
        if let Err(e) = gamepad::driver_helper(&args[0], &args[1..]) {
            eprintln!("hidshim: {e}");
            std::process::exit(1);
        }
        return;
    }
    let default_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        // Only the main thread takes the process down; elsewhere panics are caught and the engine restarts.
        if std::thread::current().name() == Some("main") {
            device::release_keyboard();
        }
        default_hook(info);
    }));
    let (backend, events) = Backend::new().expect("hidapi init");
    tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(|app, _, _| tray::show(app)))
        .plugin(tauri_plugin_dialog::init())
        .manage(backend.clone())
        .setup(move |app| {
            tray::build(app)?;
            let shown = !std::env::args().any(|a| a == autostart::TRAY_ARG);
            if shown {
                tray::show(app.handle());
            }
            forward(app.handle().clone(), events);
            backend.start(shown);
            Ok(())
        })
        .on_window_event(|w, e| {
            // The developer window closes like any other; only the main one asks or goes to the tray.
            if w.label() != "main" {
                return;
            }
            if let WindowEvent::CloseRequested { api, .. } = e {
                let action = w.state::<Arc<Backend>>().close_action();
                match action {
                    CloseAction::Ask => {
                        api.prevent_close();
                        let _ = w.emit("close-requested", ());
                    }
                    // The page knows whether it holds edits that the window would take with it.
                    CloseAction::Tray => {
                        api.prevent_close();
                        let _ = w.emit("hide-requested", ());
                    }
                    CloseAction::Exit => w.app_handle().exit(0),
                }
            }
        })
        .invoke_handler(tauri::generate_handler![
            commands::gamepad_config, commands::set_gamepad_config, commands::gamepad_input,
            commands::gamepad_driver, commands::install_gamepad_driver, commands::uninstall_gamepad_driver,
            commands::set_gamepad_driver_boot,
            commands::status,
            commands::devices,
            commands::rename_device,
            commands::forget_device,
            commands::set_last_device,
            commands::layout,
            commands::window_ready,
            commands::read_all,
            commands::apply,
            commands::save,
            commands::macros,
            commands::set_macro,
            commands::write_macro,
            commands::delete_macro,
            commands::lighting_state,
            commands::lighting_layout,
            commands::lighting_preview,
            commands::lighting_apply,
            commands::lighting_write,
            commands::set_confirm_write,
            commands::set_driver_mode,
            commands::set_lighting_delegated,
            commands::open_dynamic_lighting_settings,
            commands::app_settings,
            commands::set_autostart,
            commands::autostart_answered,
            commands::set_close_action,
            commands::set_watch_synapse,
            commands::set_unverified_writes,
            commands::set_language,
            commands::profiles,
            commands::load_profile,
            commands::create_profile,
            commands::duplicate_profile,
            commands::rename_profile,
            commands::delete_profile,
            commands::set_startup,
            commands::free_slot,
            commands::write_profile,
            commands::export_profile,
            commands::import_profile,
            commands::hide_window,
            commands::quit,
            commands::open_link,
            commands::set_dev_mode,
            commands::set_dev_panel,
            commands::set_dev_window,
            commands::dev_log,
            commands::set_dev_log_size,
            commands::dev_commands,
            commands::dev_device,
            commands::dev_exchange,
            commands::dev_keys,
            commands::dev_macro,
            commands::dev_diagnostics
        ])
        .build(tauri::generate_context!())
        .expect("tauri build")
        .run(move |app, e| match e {
            // The last window closing leaves the app in the tray; only an explicit exit carries a code.
            tauri::RunEvent::ExitRequested { code: None, api, .. } => api.prevent_exit(),
            tauri::RunEvent::Exit => app.state::<Arc<Backend>>().shutdown(),
            _ => {}
        });
}

#[derive(Clone, Serialize)]
struct LogEvent<'a> {
    id: &'a str,
    entry: &'a razer_core::logged::Entry,
}

/// Passes backend events to the WebView under the names and shapes it listens for.
fn forward(h: AppHandle, events: mpsc::Receiver<Event>) {
    // Up to 1 kHz while a key moves, and an emit can take a millisecond: the rest must not queue behind it.
    let (depth_tx, depth_rx) = mpsc::channel::<Event>();
    let dh = h.clone();
    std::thread::spawn(move || {
        while let Ok(Event::KeyDepth { id, key, depth, travel, down }) = depth_rx.recv() {
            // Skipped while nobody sees the window, but not the release.
            if let Some(w) = dh.get_webview_window("main")
                && (depth == 0 || w.is_visible().unwrap_or(false))
            {
                let _ = w.emit("key-depth", (&id, depth, travel, down, key));
            }
        }
    });
    std::thread::spawn(move || {
        for e in events {
            let _ = match &e {
                Event::KeyDepth { .. } => {
                    let _ = depth_tx.send(e);
                    Ok(())
                }
                Event::Status(s) => h.emit("status", s),
                Event::Devices => h.emit("devices", ()),
                Event::ReadProgress { id, done, total, key } => h.emit("read-progress", (id, done, total, key.map(|k| k.0), key.map(|k| k.1))),
                Event::HidLog { id, entry } => h.emit("hid-log", LogEvent { id, entry }),
                Event::SettingsChanged => {
                    tray::sync_autostart(&h);
                    h.emit("settings-changed", ())
                }
                Event::LanguageChanged => {
                    tray::retitle(&h);
                    Ok(())
                }
                Event::SnapTap { id, on } => h.emit("snap-tap", (id, on)),
                Event::GamepadEnabled { id, profile, enabled } => h.emit("gamepad-enabled", (id, profile, enabled)),
                Event::Error(message) => h.emit("app-error", message),
            };
        }
    });
}
