#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod autostart;
mod commands;
mod device;
mod dynamic_lighting;
mod engine;
mod i18n;
mod profiles;
mod settings;
mod tray;

use std::panic::AssertUnwindSafe;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use razer_core::rapid::Output;
use settings::CloseAction;
use tauri::{Emitter, Manager, WindowEvent};

fn main() {
    let default_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        // Only the main thread takes the process down; elsewhere panics are caught and the engine restarts.
        if std::thread::current().name() == Some("main") {
            device::release_keyboard();
        }
        default_hook(info);
    }));
    let device = device::Device::new().expect("hidapi init");
    tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(|app, _, _| tray::show(app)))
        .manage(commands::AppState { device: Mutex::new(device) })
        .setup(|app| {
            tray::build(app)?;
            if let Some(w) = app.get_webview_window("main") {
                no_alt_menu(w.hwnd()?.0);
            }
            if !std::env::args().any(|a| a == autostart::TRAY_ARG) {
                tray::show(app.handle());
            }
            let handle = app.handle().clone();
            let sink_handle = handle.clone();
            let progress_handle = handle.clone();
            // A connect reads the slots before the window asks for them.
            app.state::<commands::AppState>()
                .device()
                .set_progress(Arc::new(move |done, total, a| commands::emit_progress(&progress_handle, done, total, a)));
            app.state::<commands::AppState>().device().set_sink(Arc::new(move |o| {
                if let Output::Depth { depth, travel, down } = o {
                    // Up to 1 kHz while a key moves: skip it while nobody sees the window, but not the release.
                    if let Some(w) = sink_handle.get_webview_window("main")
                        && (depth == 0 || w.is_visible().unwrap_or(false))
                    {
                        let _ = w.emit("key-depth", (depth, travel, down));
                    }
                    return;
                }
                let h = sink_handle.clone();
                // Off the reader thread: the device lock can be held for seconds by a full read.
                std::thread::spawn(move || match o {
                    Output::Brightness(step) => h.state::<commands::AppState>().device().step_brightness(step > 0),
                    Output::Sleep => engine::sleep(),
                    Output::NextProfile => {
                        let state = h.state::<commands::AppState>();
                        state.device().next_profile();
                        let _ = h.emit("status", &commands::poll(&state));
                    }
                    _ => {}
                });
            }));
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
            commands::set_dynamic_lighting,
            commands::app_settings,
            commands::set_autostart,
            commands::autostart_answered,
            commands::set_close_action,
            commands::set_watch_synapse,
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
            commands::hide_window,
            commands::quit
        ])
        .build(tauri::generate_context!())
        .expect("tauri build")
        .run(|app, e| {
            if let tauri::RunEvent::Exit = e {
                app.state::<commands::AppState>().device().shutdown();
            }
        });
}

/// A lone Alt opens the (absent) window menu, and its modal loop stalls the event loop until the
/// next key: live depth freezes mid-release. Alt+Space still opens the system menu.
fn no_alt_menu(hwnd: *mut std::ffi::c_void) {
    use windows_sys::Win32::Foundation::{HWND, LPARAM, LRESULT, WPARAM};
    use windows_sys::Win32::UI::Shell::{DefSubclassProc, SetWindowSubclass};
    use windows_sys::Win32::UI::WindowsAndMessaging::{SC_KEYMENU, WM_SYSCOMMAND};

    unsafe extern "system" fn proc(hwnd: HWND, msg: u32, wp: WPARAM, lp: LPARAM, _: usize, _: usize) -> LRESULT {
        if msg == WM_SYSCOMMAND && (wp & 0xFFF0) == SC_KEYMENU as usize && lp == 0 {
            return 0;
        }
        unsafe { DefSubclassProc(hwnd, msg, wp, lp) }
    }
    unsafe { SetWindowSubclass(hwnd, Some(proc), 1, 0) };
}
