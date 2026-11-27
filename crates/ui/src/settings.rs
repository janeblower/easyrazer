//! The Settings tab: the app, the keyboard, the gamepad driver and the developer switches.

use easyrazer_backend::api::AppSettings;
use easyrazer_backend::i18n;
use easyrazer_backend::settings::CloseAction;
use egui::{RichText, Stroke, Ui};

use crate::app::App;
use crate::dialogs::Dialog;
use crate::gamepad::{self, DriverOp};
use crate::icons::{self, Icon};
use crate::text::VERSION;
use crate::{theme, widgets};

/// The settings as last read; read again after every change and on `SettingsChanged`.
pub fn reload(app: &mut App) {
    app.settings_view = Some(app.jobs.backend().app_settings());
}

/// Runs a setting change and reads the settings back, whatever came of it.
fn set(app: &mut App, change: impl FnOnce(&easyrazer_backend::Backend) -> Result<(), String>) {
    app.settings_message = change(app.jobs.backend()).err().unwrap_or_default();
    reload(app);
}

/// Room for a switch on the right of a row, and for a drop-down or a button.
const SWITCH: f32 = 50.0;
const WIDE: f32 = 190.0;

/// A card with its title; `outline` marks the developer switches apart.
fn block(ui: &mut Ui, title: &str, color: egui::Color32, outline: Option<egui::Color32>, add: impl FnOnce(&mut Ui)) {
    let frame = egui::Frame::new().fill(theme::PANEL).corner_radius(theme::RADIUS).inner_margin(egui::Margin::symmetric(16, 12));
    let frame = match outline {
        Some(c) => frame.stroke(Stroke::new(1.0, c)),
        None => frame,
    };
    frame.show(ui, |ui| {
        ui.set_width(ui.available_width());
        ui.spacing_mut().item_spacing.y = 6.0;
        ui.label(RichText::new(title.to_uppercase()).size(11.0).strong().color(color));
        add(ui);
    });
}

/// A row with its title and a muted line under it, the control on the right in `control_width`.
fn row(ui: &mut Ui, title: &str, desc: Option<&str>, control_width: f32, control: impl FnOnce(&mut Ui)) {
    ui.separator();
    ui.horizontal(|ui| {
        ui.vertical(|ui| {
            ui.set_width(ui.available_width() - control_width);
            ui.label(title);
            if let Some(d) = desc {
                ui.add(egui::Label::new(RichText::new(d).small().color(theme::MUTED)).wrap());
            }
        });
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), control);
    });
}

fn switch(ui: &mut Ui, on: bool, enabled: bool) -> Option<bool> {
    let mut v = on;
    widgets::toggle(ui, &mut v, "", enabled).changed().then_some(v)
}

pub fn ui(app: &mut App, ui: &mut Ui) {
    if app.settings_view.is_none() {
        reload(app);
    }
    let Some(s) = app.settings_view.clone() else { return };
    if !app.settings_message.is_empty() {
        ui.label(RichText::new(&app.settings_message).color(theme::ERROR));
    }
    // Two columns of blocks, so the tab fits the window the Actuation tab sets.
    let gap = ui.spacing().item_spacing.y;
    let width = (ui.available_width() - gap) / 2.0;
    ui.horizontal_top(|ui| {
        ui.spacing_mut().item_spacing.x = gap;
        let column = |ui: &mut Ui, add: &mut dyn FnMut(&mut Ui)| {
            ui.allocate_ui_with_layout(egui::vec2(width, 0.0), egui::Layout::top_down(egui::Align::Min), |ui| {
                ui.set_width(width);
                ui.spacing_mut().item_spacing.y = gap;
                add(ui);
            });
        };
        column(ui, &mut |ui| {
            block(ui, &app.t("settings.groups.app"), theme::MUTED, None, |ui| app_group(app, ui, &s));
            block(ui, &app.t("settings.groups.updates"), theme::MUTED, None, |ui| updates_group(app, ui, &s));
            block(ui, &app.t("settings.groups.dev"), theme::WARN, Some(theme::WRITE_LINE), |ui| dev_group(app, ui, &s));
        });
        column(ui, &mut |ui| {
            block(ui, &app.t("settings.groups.keyboard"), theme::MUTED, None, |ui| keyboard_group(app, ui, &s));
            if easyrazer_backend::gamepad::AVAILABLE {
                block(ui, &app.t("settings.groups.gamepad"), theme::MUTED, None, |ui| driver_group(app, ui));
            }
        });
    });
}

fn app_group(app: &mut App, ui: &mut Ui, s: &AppSettings) {
    let current = s.language.clone().unwrap_or_else(|| app.lang.clone());
    row(ui, &app.t("settings.language"), None, WIDE, |ui| {
        let languages = i18n::languages();
        let name = languages.iter().find(|(c, _)| *c == current).map_or(current.clone(), |(_, n)| n.clone());
        egui::ComboBox::from_id_salt("language").selected_text(name).width(180.0).show_ui(ui, |ui| {
            for (code, name) in &languages {
                if ui.selectable_label(*code == current, name).clicked() {
                    let code = code.to_string();
                    set(app, move |b| b.set_language(code));
                }
            }
        });
    });
    let actions = [(CloseAction::Ask, app.t("settings.ask")), (CloseAction::Tray, app.t("settings.tray")), (CloseAction::Exit, app.t("settings.exit"))];
    row(ui, &app.t("settings.onClose"), Some(&app.t("settings.hints.onClose")), WIDE, |ui| {
        let shown = actions.iter().find(|(a, _)| *a == s.close_action).map_or(String::new(), |(_, t)| t.clone());
        egui::ComboBox::from_id_salt("close").selected_text(shown).width(180.0).show_ui(ui, |ui| {
            for (a, text) in &actions {
                if ui.selectable_label(*a == s.close_action, text).clicked() {
                    let a = *a;
                    set(app, move |b| b.set_close_action(a));
                }
            }
        });
    });
    row(ui, &app.t("settings.autostart"), Some(&app.t("settings.hints.autostart")), SWITCH, |ui| {
        if let Some(on) = switch(ui, s.autostart, true) {
            set(app, move |b| b.set_autostart(on));
        }
    });
}

fn updates_group(app: &mut App, ui: &mut Ui, s: &AppSettings) {
    row(ui, &app.t("updates.auto"), Some(&app.t("updates.autoHint")), SWITCH, |ui| {
        if let Some(on) = switch(ui, s.check_updates, true) {
            set(app, move |b| b.set_check_updates(on));
        }
    });
    let status = crate::updates::status(app);
    row(ui, &app.tf("updates.version", &[("version", VERSION)]), status.as_deref(), WIDE, |ui| {
        let checking = crate::updates::checking(app);
        if widgets::button(ui, app.t(if checking { "updates.checking" } else { "updates.check" }), !checking).clicked() {
            crate::updates::check(app);
        }
    });
}

fn keyboard_group(app: &mut App, ui: &mut Ui, s: &AppSettings) {
    row(ui, &app.t("settings.watchSynapse"), Some(&app.t("settings.hints.watchSynapse")), SWITCH, |ui| {
        if let Some(on) = switch(ui, s.watch_synapse, true) {
            set(app, move |b| b.set_watch_synapse(on));
        }
    });
    row(ui, &app.t("settings.confirmWrite"), Some(&app.t("settings.hints.confirmWrite")), SWITCH, |ui| {
        if let Some(on) = switch(ui, s.confirm_write, true) {
            set(app, move |b| b.set_confirm_write(on));
        }
    });
}

fn driver_group(app: &mut App, ui: &mut Ui) {
    let status = app.gamepad.driver.clone();
    let op = app.gamepad.driver_op;
    let outdated = app.gamepad.outdated().map(str::to_string);
    let state = match (op, &status, &outdated) {
        (Some(DriverOp::Install), _, _) => app.t("gamepad.installing"),
        (Some(DriverOp::Update), _, _) => app.t("gamepad.updating"),
        (Some(DriverOp::Uninstall), _, _) => app.t("gamepad.uninstalling"),
        (None, None, _) => app.t("gamepad.checkingDriver"),
        (None, _, Some(v)) => app.tf("gamepad.driverOutdatedShort", &[("version", v)]),
        (None, Some(d), None) => app.t(if d.installed { "gamepad.driverInstalled" } else { "gamepad.driverNotInstalled" }),
    };
    let installed = status.as_ref().is_some_and(|d| d.installed);
    let version = status.as_ref().and_then(|d| d.version.clone()).filter(|_| outdated.is_none()).map(|v| format!(" {v}")).unwrap_or_default();
    ui.separator();
    ui.horizontal(|ui| {
        ui.label(format!("hidshim{version}"));
        let (dot, _) = ui.allocate_exact_size(egui::vec2(8.0, 8.0), egui::Sense::hover());
        ui.painter().circle_filled(dot.center(), 4.0, if installed { theme::ACCENT } else { theme::MUTED });
        let color = if outdated.is_some() { theme::WARN } else if installed { theme::ACCENT } else { theme::MUTED };
        ui.label(RichText::new(state).small().color(color));
        if op.is_some() {
            ui.spinner();
        }
    });
    ui.add(egui::Label::new(RichText::new(app.t("settings.hints.driver")).small().color(theme::MUTED)).wrap());
    if status.as_ref().is_some_and(|d| d.legacy) {
        ui.label(RichText::new(app.t("gamepad.legacyInstalled")).small().color(theme::WARN));
    }
    let update = app.gamepad.update().map(str::to_string);
    if let Some(v) = &update {
        ui.label(RichText::new(app.tf("gamepad.updateAvailable", &[("version", v)])).small().color(theme::ACCENT));
    }
    // Under the text: half a window is too narrow for both buttons beside it.
    ui.horizontal(|ui| {
        if outdated.is_some() {
            let b = egui::Button::image_and_text(icons::image(Icon::shield, 16.0, theme::INK), RichText::new(app.t("gamepad.update")).color(theme::INK).strong()).fill(theme::ACCENT);
            if ui.add_enabled(op.is_none(), b).clicked() {
                gamepad::install(app);
            }
        }
        if update.is_some() && widgets::primary(ui, app.t("gamepad.updatePatch"), op.is_none()).clicked() {
            gamepad::change_driver(app, DriverOp::Update, false);
        }
        let label = app.t(if installed { "gamepad.uninstall" } else { "gamepad.install" });
        let b = egui::Button::image_and_text(icons::image(Icon::shield, 16.0, theme::TEXT), label);
        if ui.add_enabled(status.is_some() && op.is_none(), b).clicked() {
            if installed {
                app.dialogs.push(Dialog::UninstallDriver);
            } else {
                gamepad::install(app);
            }
        }
    });
    if let Some(boot) = status.as_ref().and_then(|d| d.boot) {
        let busy = app.gamepad.boot_changing || op.is_some();
        row(ui, &app.t("settings.driverBoot"), Some(&app.t("settings.hints.driverBoot")), SWITCH, |ui| {
            if let Some(on) = switch(ui, boot, !busy) {
                gamepad::set_boot(app, on);
            }
        });
    }
    if let Some(e) = &app.gamepad.install_error {
        ui.label(RichText::new(e).color(theme::ERROR));
    }
}

fn dev_group(app: &mut App, ui: &mut Ui, s: &AppSettings) {
    row(ui, &app.t("settings.devMode"), Some(&app.t("settings.hints.devMode")), SWITCH, |ui| {
        if let Some(on) = switch(ui, s.dev_mode, true) {
            set(app, move |b| b.set_dev_mode(on));
        }
    });
    row(ui, &app.t("settings.unverifiedWrites"), Some(&app.t("settings.hints.unverifiedWrites")), SWITCH, |ui| {
        if let Some(on) = switch(ui, s.unverified_writes, true) {
            set(app, move |b| b.set_unverified_writes(on));
        }
    });
    if s.dev_mode {
        row(ui, &app.t("settings.devLogSize"), None, WIDE, |ui| {
            let mut size = s.dev_log_size;
            let r = ui.add(egui::DragValue::new(&mut size).range(1000..=50_000).speed(100));
            if r.changed()
                && let Some(v) = &mut app.settings_view
            {
                v.dev_log_size = size;
            }
            // Sent once the value is settled: every step of a drag would resize the log.
            if r.drag_stopped() || r.lost_focus() {
                let size = app.settings_view.as_ref().map_or(size, |v| v.dev_log_size);
                set(app, move |b| b.set_dev_log_size(size));
            }
        });
    }
}
