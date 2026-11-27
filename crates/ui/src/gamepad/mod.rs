//! The virtual gamepad: its config per profile, its live input and the hidshim driver.

pub mod model;
mod shape;
pub mod tab;

use std::time::{Duration, Instant};

use easyrazer_backend::api;
use easyrazer_backend::gamepad::drivers;
use easyrazer_backend::gamepad::{Config, Input};

use crate::app::{App, Section, Tab};
use crate::dialogs::Dialog;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DriverOp {
    Install,
    Uninstall,
}

/// How often the live input is asked for while the gamepad section is open.
const POLL: Duration = Duration::from_millis(33);
const POLL_AFTER_ERROR: Duration = Duration::from_millis(500);

#[derive(Default)]
pub struct GamepadState {
    pub base: Config,
    pub edit: Option<Config>,
    /// The base is the loaded profile's.
    pub ready: bool,
    pub selected: Option<String>,
    pub input: Option<Input>,
    pub driver: Option<drivers::Status>,
    pub driver_op: Option<DriverOp>,
    pub install_error: Option<String>,
    pub boot_changing: bool,
    /// The control waiting for a key press, and the depth it was armed with.
    pub capture: Option<&'static str>,
    pub previous: Vec<u8>,
    polling: bool,
    next_poll: Option<Instant>,
    /// What the last driver check was made for; a change of tab or section checks again.
    checked_for: Option<(Tab, Section, bool)>,
    input_for: Option<(Option<String>, Option<u32>)>,
}

impl GamepadState {
    pub fn config(&self) -> &Config {
        self.edit.as_ref().unwrap_or(&self.base)
    }

    pub fn set(&mut self, config: Config) {
        self.edit = (!model::same(&config, &self.base)).then_some(config);
    }

    pub fn take(&mut self, config: Config) {
        if self.edit.as_ref().is_some_and(|e| model::same(e, &config)) {
            self.edit = None;
        }
        self.base = config;
        self.ready = true;
    }

    /// Installed, but older than the app needs.
    pub fn outdated(&self) -> Option<&str> {
        self.driver.as_ref().filter(|d| !d.ready).and_then(|d| d.version.as_deref())
    }
}

/// Every frame: keeps the live input and the driver status fresh while they are on screen.
pub fn poll(app: &mut App) {
    let key = (app.keyboard.clone(), app.status.as_ref().and_then(|s| s.profile));
    if app.gamepad.input_for.as_ref() != Some(&key) {
        app.gamepad.input_for = Some(key);
        app.gamepad.input = None;
    }
    let view = (app.tab, app.section, app.is_visible());
    if app.gamepad.checked_for != Some(view) {
        app.gamepad.checked_for = Some(view);
        let shown = app.tab == Tab::Settings || (app.tab == Tab::Keys && app.section == Section::Gamepad);
        if shown && view.2 && app.gamepad.driver_op.is_none() {
            app.gamepad.install_error = None;
            check_driver(app);
        }
    }
    let Some(id) = app.keyboard.clone() else { return };
    let open = app.tab == Tab::Keys && app.section == Section::Gamepad && app.online() && app.is_visible() && !app.busy;
    if !open {
        app.gamepad.input = None;
        app.gamepad.next_poll = None;
        return;
    }
    if app.gamepad.polling || app.gamepad.next_poll.is_some_and(|t| t > Instant::now()) {
        return;
    }
    app.gamepad.polling = true;
    let worker_id = id.clone();
    app.jobs.run(move |b| b.gamepad_input(&worker_id), move |app, r| {
        app.gamepad.polling = false;
        let delay = if r.is_ok() { POLL } else { POLL_AFTER_ERROR };
        if app.ours(Some(&id)) {
            app.gamepad.input = Some(r.unwrap_or_else(|e| Input { depth: Vec::new(), running: false, error: Some(e) }));
        }
        app.gamepad.next_poll = Some(Instant::now() + delay);
        app.jobs.wake_after(delay);
    });
}

pub fn check_driver(app: &mut App) {
    app.jobs.run(|_| api::gamepad_driver(), |app, r| match r {
        Ok(s) => app.gamepad.driver = Some(s),
        Err(e) => {
            app.gamepad.driver = None;
            app.gamepad.install_error.get_or_insert(e);
        }
    });
}

pub fn install(app: &mut App) {
    if app.gamepad.driver.as_ref().is_some_and(|d| d.legacy) {
        app.dialogs.push(Dialog::LegacyDriver);
    } else {
        change_driver(app, DriverOp::Install, false);
    }
}

pub fn change_driver(app: &mut App, op: DriverOp, replace: bool) {
    if app.gamepad.driver_op.is_some() {
        return;
    }
    app.gamepad.driver_op = Some(op);
    app.gamepad.install_error = None;
    app.jobs.run(
        move |_| match op {
            DriverOp::Install => api::install_gamepad_driver(replace),
            DriverOp::Uninstall => api::uninstall_gamepad_driver(),
        },
        |app, r| {
            if let Err(e) = r {
                app.gamepad.install_error = Some(e);
            }
            app.gamepad.driver_op = None;
            check_driver(app);
        },
    );
}

pub fn set_boot(app: &mut App, on: bool) {
    if app.gamepad.boot_changing || app.gamepad.driver_op.is_some() {
        return;
    }
    app.gamepad.boot_changing = true;
    app.gamepad.install_error = None;
    app.jobs.run(move |_| api::set_gamepad_driver_boot(on), |app, r| {
        if let Err(e) = r {
            app.gamepad.install_error = Some(e);
        }
        app.gamepad.boot_changing = false;
        check_driver(app);
    });
}

/// The switch takes effect at once and leaves other edits waiting for Apply. Turning the
/// gamepad on may ask for elevation to bring the pad back, as after a reboot.
pub fn toggle(app: &mut App, enabled: bool) {
    let (Some(id), Some(profile)) = (app.keyboard.clone(), app.loaded_profile) else { return };
    let config = Config { enabled, ..app.gamepad.base.clone() };
    let worker_id = id.clone();
    let sent = config.clone();
    app.jobs.run(move |b| b.set_gamepad_config(&worker_id, profile, sent), move |app, r| match r {
        Ok(()) if app.ours(Some(&id)) && app.loaded_profile == Some(profile) => {
            app.gamepad.base = config;
            if let Some(mut e) = app.gamepad.edit.clone() {
                e.enabled = enabled;
                app.gamepad.set(e);
            }
        }
        Err(e) => app.show_error(e),
        _ => {}
    });
}
