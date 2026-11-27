//! The Lighting tab: effects, colors and the custom per-key layout.

use crate::app::App;
use crate::jobs::Done;

pub fn pending(_app: &App) -> usize {
    0
}

pub fn can_apply(_app: &App) -> bool {
    false
}

pub fn can_write(_app: &App) -> bool {
    false
}

/// A custom layout cannot go into the flash and stays in the app.
pub fn skips_write(_app: &App) -> bool {
    false
}

pub fn apply(app: &mut App, then: Done) {
    then(app);
}

pub fn write(app: &mut App, then: Done) {
    then(app);
}

pub fn revert(_app: &mut App) {}

pub fn ui(_app: &mut App, _ui: &mut egui::Ui) {}
