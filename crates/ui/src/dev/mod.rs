//! Developer mode: the keyboard traffic log, the command console and the bug-report text.

use razer_core::logged::Entry;

use crate::app::App;

pub fn on_log(_app: &mut App, _id: String, _entry: Entry) {}

pub fn on_settings(_app: &mut App) {}

pub fn reload_log(_app: &mut App) {}

pub fn panel(_app: &mut App, _ui: &mut egui::Ui) {}

pub fn mode(app: &App) -> bool {
    app.jobs.backend().settings().dev_mode
}

pub fn panel_open(app: &App) -> bool {
    app.jobs.backend().settings().dev_panel.as_ref().is_some_and(|p| p.open)
}

pub fn toggle_panel(_app: &mut App) {}
