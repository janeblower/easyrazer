//! The Keys tab: press points and Rapid Trigger, bindings, macros and the gamepad.

mod actuation;
mod binding;
mod labels;
pub mod macros;
pub mod map;
pub mod model;
mod nav;
mod slider;
pub mod snap;

use easyrazer_backend::api::KeyView;
use egui::Ui;
use razer_core::binding::System;

use crate::app::{App, Section};
use crate::widgets;

/// What the Keys tab remembers between frames besides the edits.
#[derive(Default)]
pub struct View {
    pub snap_picked: Option<usize>,
    /// Text typed into the binding's key search.
    pub query: String,
}

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
    if ui.input(|i| i.key_pressed(egui::Key::Escape)) && !ui.ctx().egui_wants_keyboard_input() && app.dialogs.is_empty() {
        app.keys.selection.clear();
        app.gamepad.selected = None;
    }
    match app.section {
        Section::Macros => macros::ui(app, ui),
        Section::Gamepad => crate::gamepad::tab::ui(app, ui),
        Section::Actuation | Section::Bindings => key_map(app, ui),
    }
}

fn key_map(app: &mut App, ui: &mut Ui) {
    let props = map_props(app);
    let mut selection = std::mem::take(&mut app.keys.selection);
    let mut state = std::mem::take(&mut app.map);
    map::show(app, ui, &props, &mut selection, &mut state);
    app.keys.selection = selection;
    app.map = state;
    let bind_key = (app.keys.selection.len() == 1).then(|| app.keys.selection.first().copied()).flatten();
    if app.section == Section::Bindings && bind_key.is_none() {
        widgets::card(ui, |ui| widgets::empty_state(ui, crate::icons::Icon::keyboard, &app.t("bindings.pickTitle"), &app.t("bindings.pickText")));
        return;
    }
    let online = app.online();
    ui.add_enabled_ui(online, |ui| {
        if !online {
            ui.multiply_opacity(0.6);
        }
        ui.columns(2, |cols| {
            let [left, right] = cols else { return };
            widgets::card(left, |ui| match app.section {
                Section::Actuation => actuation::card(app, ui),
                _ => {
                    let a = bind_key.and_then(|k| app.keys.binding_of(k));
                    let title = app.t("bindings.main");
                    if let Some(next) = binding::card(app, ui, &title, bind_key, a, bind_key.map(model::factory), false)
                        && let Some(k) = bind_key
                    {
                        app.keys.set_binding(k, next);
                    }
                }
            });
            widgets::card(right, |ui| match app.section {
                Section::Actuation => snap::card(app, ui),
                _ => {
                    let a = bind_key.and_then(|k| app.keys.hs_binding_of(k));
                    let title = match combo(app, bind_key) {
                        Some(c) => format!("Hypershift · {c}"),
                        None => "Hypershift".into(),
                    };
                    if let Some(next) = binding::card(app, ui, &title, bind_key, a, bind_key.map(model::hs_factory), true)
                        && let Some(k) = bind_key
                    {
                        let keys: Vec<u8> = app.layout.iter().map(|v| v.key.key).collect();
                        if let Some(m) = app.keys.set_hs_binding(keys, k, next) {
                            app.dialogs.push(crate::dialogs::Dialog::Move { key: k, m });
                        }
                    }
                }
            });
        });
    });
    crate::dev::keys(app, ui);
}

/// What the key caps show in the open section.
fn map_props(app: &App) -> map::Props {
    let k = &app.keys;
    let actuation = app.section == Section::Actuation;
    let mut p = map::Props { inline: !actuation, single: !actuation, dim: !app.online(), ..Default::default() };
    for v in &app.layout {
        let key = v.key.key;
        if actuation {
            if let Some(mm) = k.value_of(key) {
                p.values.insert(key, format!("{mm:.1}"));
            }
            if let Some(hs) = k.hs_of(key).filter(|hs| Some(*hs) != k.value_of(key)) {
                p.hs_values.insert(key, format!("{hs:.1}"));
            }
        } else {
            if k.bind_base.contains_key(&key) || k.bind_edits.contains_key(&key) {
                let a = k.binding_of(key);
                if a != Some(model::factory(key)) {
                    p.values.insert(key, labels::short_label(app, a.as_ref(), &app.layout));
                }
            }
            if let Some(a) = k.hs_binding_of(key)
                && Some(a) != Some(k.binding_of(key).unwrap_or(model::factory(key)))
            {
                let icon = match a {
                    razer_core::binding::Action::System { action } => Some(action),
                    _ => None,
                };
                p.hs_binds.insert(key, map::HsBind { text: labels::short_label(app, Some(&a), &app.layout), icon });
            }
        }
    }
    if actuation {
        p.edited = k.edits.keys().copied().collect();
        p.hs_edited = k.hs_edits.keys().copied().collect();
        p.errors = k.errors.clone();
        p.unsaved = k.unsaved.clone();
        if app.rapid_here() {
            let fw_hw = app.fw_hw();
            for v in &app.layout {
                let r = k.rapid_of(v.key.key);
                if r.enabled {
                    let crt = model::KeyEdits::mode_of(&r, fw_hw) == model::RapidMode::Crt || (fw_hw && k.crt());
                    p.rapid.insert(v.key.key, if crt { "CRT" } else { "RT" });
                }
            }
            p.snap = snap::colors(k.snap());
        }
        p.rapid_edited = k.rapid_edits.keys().copied().collect();
    } else {
        p.edited = k.bind_edits.keys().copied().collect();
        p.hs_edited = k.hs_bind_edits.keys().copied().collect();
        p.errors = k.bind_errors.clone();
        p.unsaved = k.bind_unsaved.clone();
    }
    let editable = app.layout.iter().filter(|v| v.key.editable).count();
    p.loaded = app.progress.map(|(done, total)| (done * editable).checked_div(total).unwrap_or(0));
    p
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
