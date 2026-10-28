//! The main window: what the backend last told about the keyboard, the edits on top of it,
//! and the screens that show both.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver};
use std::time::{Duration, Instant};

use easyrazer_backend::api::KeyView;
use easyrazer_backend::boards::DevicesView;
use easyrazer_backend::device::{MacroState, ProfilesView, Status};
use easyrazer_backend::settings::CloseAction;
use easyrazer_backend::{Backend, Event, api, i18n};
use egui::ViewportCommand;

use crate::dialogs::Dialog;
use crate::gamepad::GamepadState;
use crate::instance::ACTIVATE_REQUESTED;
use crate::jobs::{Done, Jobs};
use crate::keys::model::KeyEdits;
use crate::tray::{self, Tray};
use crate::{shell, theme};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Tab {
    Keys,
    Lighting,
    Settings,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Section {
    Actuation,
    Bindings,
    Macros,
    Gamepad,
}

/// Where an edit lives, to open it from the status bar.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Place {
    Actuation,
    Bindings,
    Gamepad,
    Macros,
    Lighting,
}

/// How long an error not tied to a tab stays on screen.
const ERROR_SHOWN: Duration = Duration::from_secs(10);

pub struct App {
    pub jobs: Jobs,
    done: Receiver<Done>,
    events: Receiver<Event>,
    pub tray: Tray,
    /// Read by the event pump: key depth is dropped while nobody sees it.
    visible: Arc<AtomicBool>,
    pub lang: String,
    pub tab: Tab,
    pub section: Section,
    /// The keyboard the window shows; every keyboard call is about one keyboard.
    pub keyboard: Option<String>,
    pub devices: Option<DevicesView>,
    pub status: Option<Status>,
    pub layout: Vec<KeyView>,
    /// Kept when the keyboard is unplugged, so the map does not jump to another board.
    pub layout_model: Option<String>,
    pub profiles: Option<ProfilesView>,
    pub macro_state: Option<MacroState>,
    pub busy: bool,
    /// The backend sends a status only when something changed, so one dropped while busy would be lost.
    pub missed: bool,
    /// Reads done, reads in all.
    pub progress: Option<(usize, usize)>,
    pub message: String,
    app_error: Option<(String, Instant)>,
    /// Profile the baseline was read from, and the connection and profile of that read.
    pub loaded_profile: Option<u32>,
    pub loaded_key: Option<(bool, Option<u32>)>,
    pub keys: KeyEdits,
    /// Live depth of the shown keyboard's keys, 0..255.
    pub depth: [u8; 256],
    pub gamepad: GamepadState,
    pub dialogs: Vec<Dialog>,
    pub shell: shell::State,
    pub map: crate::keys::map::MapState,
    pub keys_view: crate::keys::View,
    pub macros: crate::keys::macros::Editor,
    pub lighting: crate::lighting::Lighting,
    pub settings_view: Option<easyrazer_backend::api::AppSettings>,
    pub settings_message: String,
    /// The key last moved in driver mode: its depth and travel 0..255, whether it is down, and the key.
    pub lead: (u8, u8, bool, u8),
    quitting: bool,
}

impl App {
    pub fn new(backend: Arc<Backend>, events: Receiver<Event>, tray: Tray, ctx: &egui::Context, shown: bool) -> Self {
        crate::icons::install(ctx);
        let visible = Arc::new(AtomicBool::new(shown));
        let events = pump(events, ctx.clone(), visible.clone());
        let (jobs, done) = Jobs::new(backend.clone(), ctx.clone());
        let mut app = Self {
            lang: backend.lang(),
            jobs,
            done,
            events,
            tray,
            visible,
            tab: Tab::Keys,
            section: Section::Actuation,
            keyboard: None,
            devices: None,
            status: None,
            // Before anything that takes the device lock: a connect can hold it for the whole read.
            layout: api::layout(None),
            layout_model: None,
            profiles: None,
            macro_state: None,
            busy: false,
            missed: false,
            progress: None,
            message: String::new(),
            app_error: None,
            loaded_profile: None,
            loaded_key: None,
            keys: KeyEdits::default(),
            depth: [0; 256],
            gamepad: GamepadState::default(),
            dialogs: Vec::new(),
            shell: shell::State::default(),
            map: Default::default(),
            keys_view: Default::default(),
            macros: Default::default(),
            lighting: Default::default(),
            settings_view: None,
            settings_message: String::new(),
            lead: (0, 0, false, 0),
            quitting: false,
        };
        app.start();
        #[cfg(debug_assertions)]
        app.open_view(&std::env::var("EASYRAZER_VIEW").unwrap_or_default());
        app
    }

    /// Debug builds open on the tab and section named like `keys/macros`, for screenshots.
    #[cfg(debug_assertions)]
    fn open_view(&mut self, view: &str) {
        let (tab, section) = view.split_once('/').unwrap_or((view, ""));
        self.tab = match tab {
            "lighting" => Tab::Lighting,
            "settings" => Tab::Settings,
            _ => Tab::Keys,
        };
        self.section = match section {
            "bindings" => Section::Bindings,
            "macros" => Section::Macros,
            "gamepad" => Section::Gamepad,
            _ => Section::Actuation,
        };
    }

    fn start(&mut self) {
        let b = self.jobs.backend();
        let settings = b.app_settings();
        if !settings.autostart_offered {
            self.dialogs.push(Dialog::Autostart);
        }
        // The tray and backend messages follow the stored language.
        if settings.language.is_none() {
            let _ = b.set_language(i18n::system_language().to_string());
        }
        self.lang = b.lang();
        self.tray.retitle(&self.lang);
        b.window_ready();
        self.load_devices();
        self.load_macros();
    }

    pub fn t(&self, key: &str) -> String {
        i18n::t(&self.lang, key)
    }

    pub fn tf(&self, key: &str, args: &[(&str, &str)]) -> String {
        i18n::tf(&self.lang, key, args)
    }

    pub fn ours(&self, id: Option<&str>) -> bool {
        id == self.keyboard.as_deref()
    }

    /// Errors not tied to a tab; shown over whatever tab is open.
    pub fn show_error(&mut self, e: impl Into<String>) {
        self.app_error = Some((e.into(), Instant::now()));
    }

    pub fn app_error(&self) -> &str {
        self.app_error.as_ref().map_or("", |(e, _)| e.as_str())
    }

    pub fn online(&self) -> bool {
        self.status.as_ref().is_some_and(|s| s.device)
    }

    pub fn writable(&self) -> bool {
        self.online() && !self.status.as_ref().is_some_and(|s| s.synapse) && !self.busy
    }

    /// An offline keyboard still has its profiles, which live in the app.
    pub fn editable(&self) -> bool {
        self.status.as_ref().is_some_and(|s| !s.synapse) && !self.busy
    }

    pub fn driver(&self) -> bool {
        self.status.as_ref().is_some_and(|s| s.driver_mode)
    }

    pub fn fw_rapid(&self) -> bool {
        self.status.as_ref().and_then(|s| s.analog).is_some_and(|a| a.firmware_rapid)
    }

    /// Rapid Trigger and Snap Tap work now: on the host in driver mode, or in the keyboard's firmware.
    pub fn rapid_here(&self) -> bool {
        self.driver() || self.fw_rapid()
    }

    /// The firmware runs Rapid Trigger, so continuous is one switch for the profile, not per key.
    pub fn fw_hw(&self) -> bool {
        self.fw_rapid() && !self.driver()
    }

    pub fn is_visible(&self) -> bool {
        self.visible.load(Ordering::Relaxed)
    }

    fn on_event(&mut self, e: Event) {
        match e {
            Event::Status(s) => self.on_status(s),
            Event::Devices => self.load_devices(),
            Event::KeyDepth { id, key, depth, travel, down } if self.ours(Some(&id)) => {
                self.depth[key as usize] = depth;
                self.lead = (depth, travel, down, key);
            }
            Event::ReadProgress { id, done, total, key } => {
                // The first keyboard attaches, and reads, before its `devices` event arrives.
                if self.keyboard.is_none() {
                    self.load_devices();
                }
                if self.ours(Some(&id)) {
                    self.progress = Some((done, total));
                    if let Some((k, mm)) = key {
                        self.keys.baseline.insert(k, mm);
                    }
                }
            }
            Event::SnapTap { id, on } if self.ours(Some(&id)) => {
                self.keys.snap_base.enabled = on;
                if let Some(mut s) = self.keys.snap_edit.clone() {
                    s.enabled = on;
                    self.keys.set_snap(s);
                }
            }
            Event::GamepadEnabled { id, profile, enabled } if self.ours(Some(&id)) && self.loaded_profile == Some(profile) => {
                self.gamepad.base.enabled = enabled;
                if let Some(mut c) = self.gamepad.edit.clone() {
                    c.enabled = enabled;
                    self.gamepad.set(c);
                }
            }
            Event::HidLog { id, entry } => crate::dev::on_log(self, id, entry),
            Event::SettingsChanged => {
                self.tray.sync_autostart();
                crate::settings::reload(self);
                crate::dev::on_settings(self);
            }
            Event::LanguageChanged => {
                self.lang = self.jobs.backend().lang();
                self.tray.retitle(&self.lang);
            }
            Event::Error(message) => self.show_error(message),
            _ => {}
        }
    }

    pub fn show(&mut self, ctx: &egui::Context) {
        self.visible.store(true, Ordering::Relaxed);
        ctx.send_viewport_cmd(ViewportCommand::Visible(true));
        ctx.send_viewport_cmd(ViewportCommand::Minimized(false));
        ctx.send_viewport_cmd(ViewportCommand::Focus);
    }

    /// To the tray: the edits go, as they would with the window, and so does any lighting preview.
    pub fn hide(&mut self, ctx: &egui::Context) {
        self.visible.store(false, Ordering::Relaxed);
        self.depth = [0; 256];
        crate::changes::revert_all(self);
        ctx.send_viewport_cmd(ViewportCommand::Visible(false));
        if let Err(e) = self.jobs.backend().drop_previews() {
            self.show_error(e);
        }
    }

    pub fn quit(&mut self, ctx: &egui::Context) {
        self.quitting = true;
        ctx.send_viewport_cmd(ViewportCommand::Close);
    }

    fn on_tray(&mut self, ctx: &egui::Context, a: tray::Action) {
        match a {
            tray::Action::Show => self.show(ctx),
            tray::Action::Autostart => {
                if let Err(e) = self.jobs.backend().set_autostart(self.tray.autostart_checked()) {
                    self.show_error(self.tf("backend.autostart", &[("error", &e)]));
                }
            }
            tray::Action::Quit => self.quit(ctx),
        }
    }

    fn on_close(&mut self, ctx: &egui::Context) {
        if self.quitting || !ctx.input(|i| i.viewport().close_requested()) {
            return;
        }
        match self.jobs.backend().close_action() {
            CloseAction::Exit => self.quitting = true,
            CloseAction::Ask => {
                ctx.send_viewport_cmd(ViewportCommand::CancelClose);
                self.dialogs.push(Dialog::Close { remember: false });
            }
            CloseAction::Tray => {
                ctx.send_viewport_cmd(ViewportCommand::CancelClose);
                if crate::changes::has_unapplied(self) {
                    self.dialogs.push(Dialog::Hiding);
                } else {
                    self.hide(ctx);
                }
            }
        }
    }
}

impl eframe::App for App {
    fn logic(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        while let Ok(a) = self.tray.actions.try_recv() {
            self.on_tray(ctx, a);
        }
        if ACTIVATE_REQUESTED.swap(false, Ordering::Relaxed) {
            self.show(ctx);
        }
        while let Ok(e) = self.events.try_recv() {
            self.on_event(e);
        }
        while let Ok(done) = self.done.try_recv() {
            done(self);
        }
        if let Some((_, at)) = &self.app_error {
            match ERROR_SHOWN.checked_sub(at.elapsed()) {
                Some(left) => ctx.request_repaint_after(left),
                None => self.app_error = None,
            }
        }
        self.on_close(ctx);
        crate::gamepad::poll(self);
        crate::lighting::tick(self);
    }

    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        crate::dev::panel(self, ui);
        egui::CentralPanel::default().frame(egui::Frame::new().fill(theme::BG).inner_margin(16)).show(ui, |ui| {
            egui::ScrollArea::both().auto_shrink(false).show(ui, |ui| {
                ui.set_max_width(1121.0);
                shell::ui(self, ui);
            });
        });
        crate::dialogs::show(self, ui.ctx());
    }

    fn clear_color(&self, _visuals: &egui::Visuals) -> [f32; 4] {
        theme::BG.to_normalized_gamma_f32()
    }
}

/// Moves backend events to the window and wakes it; key depth only while it is seen, so a game
/// session in the tray costs nothing.
fn pump(events: Receiver<Event>, ctx: egui::Context, visible: Arc<AtomicBool>) -> Receiver<Event> {
    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || {
        for e in events {
            if matches!(e, Event::KeyDepth { .. }) && !visible.load(Ordering::Relaxed) {
                continue;
            }
            if tx.send(e).is_err() {
                return;
            }
            ctx.request_repaint();
        }
    });
    rx
}
