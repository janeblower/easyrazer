//! What every tab shares: the tab bar with the keyboard and profile menus, the status line
//! with Revert / Apply / Write, and the footer.

mod devices;
mod profiles;

use egui::{CornerRadius, RichText, Stroke, Ui};

use crate::app::{App, Place, Section, Tab};
use crate::icons::{self, Icon};
use crate::status::{self, Kind};
use crate::text::VERSION;
use crate::widgets;
use crate::{about, changes, dialogs::Dialog, theme};

pub use devices::truncate;

#[derive(Default)]
pub struct State {
    devices: devices::Menu,
    profiles: profiles::Menu,
}

const TABS: [(Tab, &str); 3] = [(Tab::Keys, "tabs.keys"), (Tab::Lighting, "tabs.lighting"), (Tab::Settings, "tabs.settings")];

pub fn ui(app: &mut App, ui: &mut Ui) {
    ui.spacing_mut().item_spacing.y = 16.0;
    nav(app, ui);
    status_line(app, ui);
    match app.tab {
        Tab::Keys => crate::keys::ui(app, ui),
        Tab::Lighting => crate::lighting::ui(app, ui),
        Tab::Settings => crate::settings::ui(app, ui),
    }
    footer(app, ui);
}

fn nav(app: &mut App, ui: &mut Ui) {
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 4.0;
        for (tab, key) in TABS {
            let on = app.tab == tab;
            let text = RichText::new(app.t(key)).color(if on { theme::TEXT } else { theme::MUTED });
            let button = egui::Button::new(text)
                .fill(if on { theme::BG } else { theme::PANEL })
                .stroke(Stroke::NONE)
                .corner_radius(CornerRadius { nw: theme::RADIUS, ne: theme::RADIUS, sw: 0, se: 0 });
            let r = ui.add(button);
            if on {
                ui.painter().hline(r.rect.x_range(), r.rect.bottom() - 1.0, Stroke::new(2.0, theme::ACCENT));
            }
            if r.clicked() {
                app.tab = tab;
            }
        }
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            ui.spacing_mut().item_spacing.x = 8.0;
            mode(app, ui);
            profiles::menu(app, ui);
            if app.devices.as_ref().is_some_and(|d| d.devices.len() > 1) {
                devices::menu(app, ui);
            }
        });
    });
}

/// The HW / Driver switch, and the developer panel's button in developer mode.
fn mode(app: &mut App, ui: &mut Ui) {
    ui.spacing_mut().item_spacing.x = 0.0;
    if crate::dev::mode(app) {
        let open = crate::dev::panel_open(app);
        let text = RichText::new("</>").monospace().strong().color(if open { theme::DEV } else { theme::MUTED });
        let b = egui::Button::new(text).stroke(Stroke::new(1.0, if open { theme::DEV } else { theme::LINE })).min_size(egui::vec2(48.0, 34.0));
        if ui.add(b).on_hover_text(app.t("dev.title")).clicked() {
            crate::dev::toggle_panel(app);
        }
        ui.add_space(8.0);
    }
    let driver = app.driver();
    let writable = app.writable();
    let hint = app.t("mode.hint");
    // Right to left: Driver is drawn first and lands on the right.
    for (on, icon, key) in [(true, Icon::rust, "mode.driver"), (false, Icon::chip, "mode.hw")] {
        let color = match (driver == on, on) {
            (false, _) => theme::MUTED,
            (true, true) => theme::DRIVER,
            (true, false) => theme::ACCENT,
        };
        let r = theme::RADIUS;
        let corners = if on { CornerRadius { ne: r, se: r, nw: 0, sw: 0 } } else { CornerRadius { nw: r, sw: r, ne: 0, se: 0 } };
        let b = egui::Button::image(icons::image(icon, 28.0, color))
            .corner_radius(corners)
            .stroke(Stroke::new(1.0, if driver == on { color } else { theme::LINE }))
            .min_size(egui::vec2(52.0, 34.0));
        let resp = ui.add_enabled(writable, b).on_hover_text(format!("{}\n\n{hint}", app.t(key)));
        if resp.clicked() {
            app.set_mode(on);
        }
    }
}

fn status_line(app: &mut App, ui: &mut Ui) {
    let name = app.devices.as_ref().and_then(|d| d.devices.iter().find(|e| app.ours(Some(&e.id)))).map(|d| d.name.clone());
    let message = if app.tab == Tab::Lighting { "" } else { app.message.as_str() };
    let input = status::Input {
        status: app.status.as_ref(),
        progress: app.progress,
        unsupported: app.devices.as_ref().and_then(|d| d.unsupported),
        name: name.as_deref(),
        message,
        error: app.app_error(),
    };
    let (kind, text) = status::line(&app.lang, &input);
    let (fill, color) = match kind {
        Kind::Info => (theme::PANEL, theme::TEXT),
        Kind::Warn => (theme::WARN_BG, theme::WARN),
        Kind::Ok => (theme::PANEL, theme::ACCENT),
    };
    egui::Frame::new().fill(fill).corner_radius(theme::RADIUS).inner_margin(egui::Margin::symmetric(16, 8)).show(ui, |ui| {
        ui.set_min_width(ui.available_width());
        ui.allocate_ui_with_layout(egui::vec2(ui.available_width(), 34.0), egui::Layout::left_to_right(egui::Align::Center), |ui| {
            ui.add(egui::Label::new(RichText::new(text).color(color)).wrap());
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| actions(app, ui));
        });
    });
}

/// Revert and Apply work on the drafts of every tab; Write puts the applied state into the flash.
fn actions(app: &mut App, ui: &mut Ui) {
    let s = changes::summarize(app);
    let write_tip = app.t(if s.skips_write { "common.writeSkips" } else { "common.write" });
    if widgets::write_button(ui, &write_tip, s.can_write).clicked() {
        changes::write_all(app);
    }
    if widgets::primary(ui, app.t("common.apply"), s.can_apply).clicked() {
        changes::apply_all(app);
    }
    if widgets::icon_button(ui, Icon::revert, &app.t("common.revert"), s.can_revert, theme::TEXT).clicked() {
        changes::revert_all(app);
    }
    if !s.parts.is_empty() {
        ui.spacing_mut().item_spacing.x = 4.0;
        for (i, place) in s.parts.iter().rev().enumerate() {
            if i > 0 {
                ui.label(RichText::new("·").small().color(theme::MUTED));
            }
            if widgets::link(ui, place_label(app, *place), theme::MUTED).clicked() {
                open_place(app, *place);
            }
        }
    }
    if s.pending > 0 {
        ui.label(RichText::new(app.tf("common.notAppliedN", &[("n", &s.pending.to_string())])).small().color(theme::EDITED));
    }
}

fn place_label(app: &App, p: Place) -> String {
    match p {
        Place::Actuation => app.t("keys.actuation"),
        Place::Bindings => app.t("keys.bindings"),
        Place::Gamepad => app.t("keys.gamepad"),
        Place::Macros => app.t("keys.macros"),
        Place::Lighting => app.t("tabs.lighting"),
    }
}

fn open_place(app: &mut App, p: Place) {
    let section = match p {
        Place::Lighting => {
            app.tab = Tab::Lighting;
            return;
        }
        Place::Actuation => Section::Actuation,
        Place::Bindings => Section::Bindings,
        Place::Gamepad => Section::Gamepad,
        Place::Macros => Section::Macros,
    };
    app.tab = Tab::Keys;
    crate::keys::open_section(app, section);
}

fn footer(app: &mut App, ui: &mut Ui) {
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 4.0;
        widgets::hint(ui, format!("EasyRazer {VERSION} ·"));
        if widgets::link(ui, app.t("about.title"), theme::MUTED).clicked() {
            app.dialogs.push(Dialog::About(about::About::default()));
        }
    });
}
