//! The Keys tab: press points and Rapid Trigger, bindings, macros and the gamepad.

pub mod macros;
pub mod model;

use easyrazer_backend::api::KeyView;
use egui::Ui;
use razer_core::binding::System;

use crate::app::{App, Section};
use crate::widgets;

const SECTIONS: [(Section, &str); 4] =
    [(Section::Actuation, "keys.actuation"), (Section::Bindings, "keys.bindings"), (Section::Macros, "keys.macros"), (Section::Gamepad, "keys.gamepad")];

pub fn open_section(app: &mut App, s: Section) {
    // Bindings are edited one key at a time.
    if s == Section::Bindings && app.keys.selection.len() > 1 {
        app.keys.selection.clear();
    }
    app.section = s;
}

pub fn ui(app: &mut App, ui: &mut Ui) {
    ui.horizontal(|ui| {
        let items: Vec<(Section, String)> = SECTIONS.iter().map(|(s, k)| (*s, app.t(k))).collect();
        if let Some(s) = widgets::segments(ui, &items, app.section, true) {
            open_section(app, s);
        }
        if matches!(app.section, Section::Actuation | Section::Bindings) {
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| selection_bar(app, ui));
        }
    });
    match app.section {
        Section::Macros => macros::ui(app, ui),
        Section::Gamepad => crate::gamepad::tab::ui(app, ui),
        Section::Actuation | Section::Bindings => {}
    }
}

/// The hint goes last in a right-to-left row: its width changes, the buttons stay put.
fn selection_bar(app: &mut App, ui: &mut Ui) {
    let single = app.section == Section::Bindings;
    let count = app.keys.selection.len();
    if widgets::button(ui, app.t("common.clearSelection"), count > 0).clicked() {
        app.keys.selection.clear();
    }
    if !single && ui.button(app.t("common.selectAll")).clicked() {
        app.keys.selection = app.layout.iter().filter(|k| k.key.editable).map(|k| k.key.key).collect();
    }
    let text = if count > 0 { app.tf("common.selected", &[("n", &count.to_string())]) } else { app.t(if single { "bindings.selectHint" } else { "common.selectHint" }) };
    let r = ui.label(egui::RichText::new(text).color(crate::theme::MUTED));
    if !single {
        r.on_hover_text(app.t("common.selectHow"));
    }
}

/// Key name that tells apart the keys sharing a cap label.
pub fn key_name(app: &App, k: &KeyView) -> String {
    let label = k.key.label;
    match k.name {
        "SPACEBAR" => app.t("bindings.space"),
        "NUMPAD_NUM_LOCK" => "Num Lock".into(),
        n if n.starts_with("NUMPAD_") => format!("Num {label}"),
        n => {
            let side = ["LEFT", "RIGHT"].into_iter().find(|s| n.strip_prefix(s).and_then(|r| r.strip_prefix('_')).is_some_and(|m| ["SHIFT", "CTRL", "ALT", "GUI"].contains(&m)));
            match side {
                Some(s) => app.tf(&format!("bindings.{}", s.to_lowercase()), &[("k", label)]),
                None => label.to_string(),
            }
        }
    }
}

/// `Fn+` and the key's name: how a Hypershift binding is pressed.
pub fn combo(app: &App, k: Option<u8>) -> Option<String> {
    let k = k?;
    let view = app.layout.iter().find(|v| v.key.key == k)?;
    Some(format!("Fn+{}", key_name(app, view)))
}

/// The key holding a unique system action in the Hypershift layer, unapplied edits included.
pub fn holder(app: &App, s: System) -> Option<u8> {
    app.keys.holder(app.layout.iter().map(|k| k.key.key), s)
}
