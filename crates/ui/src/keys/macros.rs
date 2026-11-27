//! The macro library and its editor.

use crate::app::App;
use crate::jobs::Done;

pub fn pending(_app: &App) -> usize {
    0
}

pub fn can_apply(_app: &App) -> bool {
    false
}

pub fn apply(app: &mut App, then: Done) {
    then(app);
}

pub fn revert(_app: &mut App) {}

pub fn ui(_app: &mut App, _ui: &mut egui::Ui) {}
