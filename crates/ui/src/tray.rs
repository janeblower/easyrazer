//! Tray icon and its menu; clicks reach the window as [`Action`]s.

use std::sync::mpsc::{self, Receiver};

use easyrazer_backend::{autostart, i18n};
use tray_icon::menu::{CheckMenuItem, Menu, MenuEvent, MenuItem, PredefinedMenuItem};
use tray_icon::{Icon, MouseButton, MouseButtonState, TrayIcon, TrayIconBuilder, TrayIconEvent};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Action {
    Show,
    /// The item has already flipped itself; [`Tray::autostart_checked`] tells to what.
    Autostart,
    Quit,
}

pub struct Tray {
    _icon: TrayIcon,
    open: MenuItem,
    autostart: CheckMenuItem,
    quit: MenuItem,
    pub actions: Receiver<Action>,
}

impl Tray {
    pub fn new(icon: &egui::IconData, lang: &str, ctx: &egui::Context) -> Result<Self, Box<dyn std::error::Error>> {
        let open = MenuItem::new(i18n::t(lang, "tray.open"), true, None);
        let autostart = CheckMenuItem::new(i18n::t(lang, "tray.autostart"), true, autostart::enabled(), None);
        let quit = MenuItem::new(i18n::t(lang, "tray.quit"), true, None);
        let menu = Menu::with_items(&[&open, &autostart, &PredefinedMenuItem::separator(), &quit])?;
        let tray = TrayIconBuilder::new()
            .with_icon(Icon::from_rgba(icon.rgba.clone(), icon.width, icon.height)?)
            .with_tooltip("EasyRazer")
            .with_menu(Box::new(menu))
            .with_menu_on_left_click(false)
            .build()?;

        let (tx, actions) = mpsc::channel();
        // The window's loop sleeps while it is hidden; each action wakes it.
        let (menu_tx, menu_ctx) = (tx.clone(), ctx.clone());
        let ids = (open.id().clone(), autostart.id().clone(), quit.id().clone());
        MenuEvent::set_event_handler(Some(move |e: MenuEvent| {
            let action = match e.id {
                id if id == ids.0 => Action::Show,
                id if id == ids.1 => Action::Autostart,
                id if id == ids.2 => Action::Quit,
                _ => return,
            };
            let _ = menu_tx.send(action);
            menu_ctx.request_repaint();
        }));
        let ctx = ctx.clone();
        TrayIconEvent::set_event_handler(Some(move |e: TrayIconEvent| {
            if let TrayIconEvent::Click { button: MouseButton::Left, button_state: MouseButtonState::Up, .. } = e {
                let _ = tx.send(Action::Show);
                ctx.request_repaint();
            }
        }));
        Ok(Self { _icon: tray, open, autostart, quit, actions })
    }

    /// Brings the menu to the current interface language.
    pub fn retitle(&self, lang: &str) {
        self.open.set_text(i18n::t(lang, "tray.open"));
        self.autostart.set_text(i18n::t(lang, "tray.autostart"));
        self.quit.set_text(i18n::t(lang, "tray.quit"));
    }

    pub fn autostart_checked(&self) -> bool {
        self.autostart.is_checked()
    }

    /// Shows what the registry finally holds.
    pub fn sync_autostart(&self) {
        self.autostart.set_checked(autostart::enabled());
    }
}
