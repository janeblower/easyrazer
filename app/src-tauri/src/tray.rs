//! Tray icon and window visibility.

use tauri::menu::{CheckMenuItem, Menu, MenuItem, PredefinedMenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{App, AppHandle, Emitter, Manager, Wry};

use crate::autostart;

pub struct AutostartItem(pub CheckMenuItem<Wry>);

pub fn show(app: &AppHandle) {
    if let Some(w) = app.get_webview_window("main") {
        let _ = w.unminimize();
        let _ = w.show();
        let _ = w.set_focus();
    }
}

pub fn sync_autostart(app: &AppHandle) {
    let _ = app.state::<AutostartItem>().0.set_checked(autostart::enabled());
    let _ = app.emit("settings-changed", ());
}

pub fn build(app: &App) -> tauri::Result<()> {
    let autostart = CheckMenuItem::with_id(app, "autostart", "Запускать с Windows", true, autostart::enabled(), None::<&str>)?;
    let menu = Menu::with_items(
        app,
        &[
            &MenuItem::with_id(app, "open", "Открыть", true, None::<&str>)?,
            &autostart,
            &PredefinedMenuItem::separator(app)?,
            &MenuItem::with_id(app, "quit", "Выход", true, None::<&str>)?,
        ],
    )?;
    app.manage(AutostartItem(autostart));
    TrayIconBuilder::with_id("main")
        .icon(app.default_window_icon().cloned().expect("bundle icon"))
        .tooltip("EasyRazer")
        .menu(&menu)
        .show_menu_on_left_click(false)
        .on_menu_event(|app, e| match e.id.as_ref() {
            "open" => show(app),
            "autostart" => {
                // The item has already flipped itself; the registry decides what it finally shows.
                let on = app.state::<AutostartItem>().0.is_checked().unwrap_or(false);
                let _ = autostart::set(on);
                sync_autostart(app);
            }
            "quit" => app.exit(0),
            _ => {}
        })
        .on_tray_icon_event(|tray, e| {
            if let TrayIconEvent::Click { button: MouseButton::Left, button_state: MouseButtonState::Up, .. } = e {
                show(tray.app_handle());
            }
        })
        .build(app)?;
    Ok(())
}
