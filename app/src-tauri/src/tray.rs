//! Tray icon and window visibility.

use tauri::menu::{CheckMenuItem, Menu, MenuItem, PredefinedMenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{App, AppHandle, Emitter, Manager, Wry};

use crate::commands::AppState;
use crate::{autostart, i18n};

pub struct Items {
    open: MenuItem<Wry>,
    autostart: CheckMenuItem<Wry>,
    quit: MenuItem<Wry>,
}

fn lang(app: &AppHandle) -> String {
    app.state::<AppState>().device().lang().to_string()
}

/// Brings the menu to the current interface language.
pub fn retitle(app: &AppHandle) {
    let l = lang(app);
    let items = app.state::<Items>();
    let _ = items.open.set_text(i18n::t(&l, "tray.open"));
    let _ = items.autostart.set_text(i18n::t(&l, "tray.autostart"));
    let _ = items.quit.set_text(i18n::t(&l, "tray.quit"));
}

pub fn show(app: &AppHandle) {
    if let Some(w) = app.get_webview_window("main") {
        let _ = w.unminimize();
        let _ = w.show();
        let _ = w.set_focus();
    }
}

pub fn sync_autostart(app: &AppHandle) {
    let _ = app.state::<Items>().autostart.set_checked(autostart::enabled());
    let _ = app.emit("settings-changed", ());
}

pub fn build(app: &App) -> tauri::Result<()> {
    let l = lang(app.handle());
    let items = Items {
        open: MenuItem::with_id(app, "open", i18n::t(&l, "tray.open"), true, None::<&str>)?,
        autostart: CheckMenuItem::with_id(app, "autostart", i18n::t(&l, "tray.autostart"), true, autostart::enabled(), None::<&str>)?,
        quit: MenuItem::with_id(app, "quit", i18n::t(&l, "tray.quit"), true, None::<&str>)?,
    };
    let menu = Menu::with_items(app, &[&items.open, &items.autostart, &PredefinedMenuItem::separator(app)?, &items.quit])?;
    app.manage(items);
    TrayIconBuilder::with_id("main")
        .icon(app.default_window_icon().cloned().expect("bundle icon"))
        .tooltip("EasyRazer")
        .menu(&menu)
        .show_menu_on_left_click(false)
        .on_menu_event(|app, e| match e.id.as_ref() {
            "open" => show(app),
            "autostart" => {
                // The item has already flipped itself; the registry decides what it finally shows.
                let on = app.state::<Items>().autostart.is_checked().unwrap_or(false);
                if let Err(e) = autostart::set(on) {
                    let _ = app.emit("app-error", i18n::tf(&lang(app), "backend.autostart", &[("error", &e)]));
                }
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
