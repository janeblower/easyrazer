//! The gamepad section of the Keys tab: the driver notice, the controller and the editor of
//! the picked control.

use easyrazer_backend::gamepad::Config;
use egui::{RichText, Stroke, Ui};

use super::model::{self, BINDINGS};
use super::{DriverOp, install, toggle};
use crate::app::App;
use crate::icons::{self, Icon};
use crate::{theme, widgets};

/// The controls a selection edits: a stick has four directions and a click.
fn rows(selected: &str) -> Vec<&'static str> {
    let side = ["left", "right"].into_iter().find(|s| selected == *s || selected.starts_with(&format!("{s}_")));
    match side {
        Some(s) => ["up", "down", "left", "right", "click"].iter().filter_map(|d| BINDINGS.iter().find(|b| **b == format!("{s}_{d}")).copied()).collect(),
        None => BINDINGS.iter().filter(|b| **b == selected).copied().collect(),
    }
}

pub fn ui(app: &mut App, ui: &mut Ui) {
    let driver = app.driver();
    let installed = app.gamepad.driver.as_ref().map(|d| d.ready);
    let op = app.gamepad.driver_op;
    let available = driver && installed == Some(true) && op.is_none();
    let can_edit = app.editable() && app.gamepad.ready && available;
    if !can_edit || ui.input(|i| !i.focused) {
        app.gamepad.capture = None;
    }
    if installed != Some(true) || !driver || op.is_some() {
        notice(app, ui, driver, installed);
    }
    let config = app.gamepad.config().clone();
    let input = app.gamepad.input.clone();
    capture(app, input.as_ref().map(|i| i.depth.as_slice()).unwrap_or(&[]));
    ui.add_enabled_ui(available, |ui| {
        if !available {
            ui.multiply_opacity(0.45);
        }
        ui.horizontal(|ui| {
            widgets::hint(ui, app.t("gamepad.hint"));
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                let mut on = config.enabled;
                if widgets::toggle(ui, &mut on, &app.t("gamepad.enabled"), can_edit).changed() {
                    toggle(app, on);
                }
            });
        });
        if let Some(e) = input.as_ref().and_then(|i| i.error.clone()) {
            ui.horizontal(|ui| {
                ui.label(RichText::new(e).color(theme::ERROR));
                if config.enabled && can_edit && ui.button(app.t("gamepad.retry")).clicked() {
                    toggle(app, true);
                }
            });
        }
        ui.horizontal_top(|ui| {
            let editor_width = 300.0;
            ui.vertical(|ui| {
                ui.set_width((ui.available_width() - editor_width - 16.0).max(300.0));
                let depth = if available { input.as_ref().map(|i| i.depth.clone()).unwrap_or_default() } else { Vec::new() };
                if let Some(id) = model::show(ui, &config, &depth, app.gamepad.selected.as_deref(), available) {
                    app.gamepad.selected = Some(id.to_string());
                }
            });
            ui.vertical(|ui| {
                ui.set_width(editor_width);
                widgets::card(ui, |ui| editor(app, ui, &config, can_edit, input.as_ref().is_some_and(|i| i.running)));
            });
        });
    });
}

fn notice(app: &mut App, ui: &mut Ui, driver: bool, installed: Option<bool>) {
    let op = app.gamepad.driver_op;
    let text = match (op, installed, app.gamepad.outdated()) {
        (Some(DriverOp::Uninstall), _, _) => app.t("gamepad.uninstalling"),
        (Some(DriverOp::Install), _, _) => app.t("gamepad.installing"),
        (None, None, _) => app.t("gamepad.checkingDriver"),
        (None, _, Some(v)) => app.tf("gamepad.driverOutdated", &[("version", v)]),
        (None, Some(true), None) => app.t("gamepad.driverRequired"),
        (None, Some(false), None) => app.t("gamepad.driverMissing"),
    };
    egui::Frame::new().fill(theme::WARN_BG).corner_radius(theme::RADIUS).inner_margin(egui::Margin::symmetric(14, 10)).show(ui, |ui| {
        ui.set_width(ui.available_width());
        ui.horizontal_wrapped(|ui| {
            ui.label(RichText::new("!").strong().color(theme::WARN));
            ui.label(RichText::new(text).color(theme::WARN));
            if installed == Some(false) || op == Some(DriverOp::Install) {
                let label = app.t(if app.gamepad.outdated().is_some() { "gamepad.update" } else { "gamepad.install" });
                let b = egui::Button::image_and_text(icons::image(Icon::shield, 16.0, theme::INK), RichText::new(label).color(theme::INK).strong()).fill(theme::ACCENT);
                if ui.add_enabled(op.is_none(), b).clicked() {
                    install(app);
                }
                if op.is_some() {
                    ui.spinner();
                }
            } else if installed == Some(true) && !driver && op.is_none() && widgets::primary(ui, app.t("gamepad.switchDriver"), true).clicked() {
                app.set_mode(true);
            }
        });
        if let Some(e) = &app.gamepad.install_error {
            ui.label(RichText::new(e).color(theme::ERROR));
        }
    });
}

/// A capture takes the first key pressed past half its travel.
fn capture(app: &mut App, depth: &[u8]) {
    let Some(binding) = app.gamepad.capture else { return };
    let keys: Vec<u8> = app.layout.iter().filter(|k| k.key.editable).map(|k| k.key.key).collect();
    let key = model::captured_key(&app.gamepad.previous, depth, &keys);
    app.gamepad.previous = depth.to_vec();
    if let Some(k) = key {
        assign(app, binding, Some(k));
    }
}

fn assign(app: &mut App, binding: &str, key: Option<u8>) {
    let mut c = app.gamepad.config().clone();
    match key {
        Some(k) => {
            c.bindings.insert(binding.to_string(), k);
        }
        None => {
            c.bindings.remove(binding);
            c.max_depth.remove(binding);
        }
    }
    app.gamepad.capture = None;
    app.gamepad.set(c);
}

fn editor(app: &mut App, ui: &mut Ui, config: &Config, can_edit: bool, running: bool) {
    match app.gamepad.selected.clone() {
        Some(sel) => {
            let title = if sel.starts_with("left") {
                app.t("gamepad.leftStick")
            } else if sel.starts_with("right") {
                app.t("gamepad.rightStick")
            } else {
                app.t(&format!("gamepad.{sel}"))
            };
            ui.label(RichText::new(title).heading());
            widgets::hint(ui, app.t("gamepad.assignHint"));
            let rows = rows(&sel);
            let single = rows.len() == 1;
            for binding in rows {
                let label = if single { app.t("gamepad.keyboardKey") } else { app.t(&format!("gamepad.{}", binding.split_once('_').map_or(binding, |(_, d)| d))) };
                ui.label(RichText::new(label).color(theme::MUTED));
                ui.horizontal(|ui| {
                    let capturing = app.gamepad.capture == Some(binding);
                    let name = config.bindings.get(binding).and_then(|k| app.layout.iter().find(|v| v.key.key == *k)).map(|v| crate::keys::key_name(app, v));
                    let text = if capturing { app.t("gamepad.capturing") } else { name.unwrap_or_else(|| app.t("gamepad.capture")) };
                    let b = egui::Button::new(text).stroke(Stroke::new(1.0, if capturing { theme::ACCENT } else { theme::LINE })).min_size(egui::vec2(200.0, 32.0));
                    let tip = app.t(if app.driver() { "gamepad.capture" } else { "gamepad.driverRequired" });
                    if ui.add_enabled(can_edit, b).on_hover_text(tip).clicked() {
                        app.gamepad.previous = app.gamepad.input.as_ref().map(|i| i.depth.clone()).unwrap_or_default();
                        app.gamepad.capture = if capturing { None } else { Some(binding) };
                    }
                    if widgets::icon_button(ui, Icon::remove, &app.t("gamepad.clear"), can_edit && config.bindings.contains_key(binding), theme::TEXT).clicked() {
                        assign(app, binding, None);
                    }
                });
                if model::analog(binding) {
                    let mut v = config.max_depth.get(binding).copied().unwrap_or(100);
                    ui.horizontal(|ui| {
                        ui.label(RichText::new(app.t("gamepad.maxDepth")).small().color(theme::MUTED));
                        ui.label(RichText::new(format!("{v}%")).small());
                    });
                    ui.spacing_mut().slider_width = 260.0;
                    if ui.add_enabled(can_edit, egui::Slider::new(&mut v, 1..=100).show_value(false)).changed() {
                        let mut c = config.clone();
                        c.max_depth.insert(binding.to_string(), v);
                        app.gamepad.set(c);
                    }
                }
            }
        }
        None => {
            widgets::hint(ui, app.t("gamepad.selectHint"));
        }
    }
    ui.separator();
    ui.label(RichText::new(app.t("gamepad.inputTitle")).heading());
    widgets::hint(ui, app.t("gamepad.inputHint"));
    ui.horizontal(|ui| {
        let (dot, _) = ui.allocate_exact_size(egui::vec2(8.0, 8.0), egui::Sense::hover());
        ui.painter().circle_filled(dot.center(), 4.0, if running { theme::ACCENT } else { theme::MUTED });
        ui.label(app.t(if running { "gamepad.running" } else { "gamepad.stopped" }));
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_stick_edits_its_four_directions_and_click() {
        assert_eq!(rows("left"), vec!["left_up", "left_down", "left_left", "left_right", "left_click"]);
        assert_eq!(rows("right_up").len(), 5);
        assert_eq!(rows("a"), vec!["a"]);
    }
}
