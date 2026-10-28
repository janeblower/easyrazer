//! The keyboard menu: pick, rename or forget a keyboard the app knows.

use egui::{RichText, Ui};

use crate::app::App;
use crate::dialogs::Dialog;
use crate::icons::{self, Icon};
use crate::theme;

/// Longer names do not fit the button.
const NAME_MAX: usize = 40;

#[derive(Default)]
pub struct Menu {
    editing: Option<String>,
    draft: String,
    focus: bool,
}

pub fn menu(app: &mut App, ui: &mut Ui) {
    let Some(view) = app.devices.clone() else { return };
    let current = view.devices.iter().find(|d| app.ours(Some(&d.id))).cloned();
    let connected = current.as_ref().is_some_and(|d| d.connected);
    let plug = |on: bool| icons::image(if on { Icon::plug } else { Icon::plug_off }, 16.0, if on { theme::ACCENT } else { theme::MUTED });
    let name = current.as_ref().map_or("…".to_string(), |d| d.name.clone());
    let button = egui::Button::image_and_text(plug(connected), format!("{}  ▾", truncate(&name, 28))).min_size(egui::vec2(190.0, 32.0));
    let r = ui.add(button);
    let mut pick = None;
    egui::Popup::from_toggle_button_response(&r)
        .align(egui::RectAlign::BOTTOM_END)
        .width(320.0)
        .close_behavior(egui::PopupCloseBehavior::CloseOnClickOutside)
        .show(|ui| {
            ui.spacing_mut().item_spacing.y = 2.0;
            for d in &view.devices {
                let on = app.ours(Some(&d.id));
                ui.horizontal(|ui| {
                    ui.set_min_height(28.0);
                    if on {
                        let rect = ui.max_rect();
                        ui.painter().vline(rect.left(), rect.y_range(), egui::Stroke::new(2.0, theme::ACCENT));
                    }
                    ui.add_space(6.0);
                    ui.add(plug(d.connected));
                    let mode = if d.connected { app.t(if d.driver_mode { "mode.driver" } else { "mode.hw" }) } else { app.t("devices.offline") };
                    if app.shell.devices.editing.as_deref() == Some(&d.id) {
                        let m = &mut app.shell.devices;
                        let edit = ui.add(egui::TextEdit::singleline(&mut m.draft).char_limit(NAME_MAX).desired_width(200.0));
                        if m.focus {
                            edit.request_focus();
                            m.focus = false;
                        }
                        if ui.input(|i| i.key_pressed(egui::Key::Escape)) {
                            m.editing = None;
                        } else if edit.lost_focus() {
                            let (id, name) = (d.id.clone(), m.draft.clone());
                            m.editing = None;
                            app.rename_keyboard(id, name);
                        }
                    } else if ui.add(egui::Label::new(truncate(&d.name, 30)).sense(egui::Sense::click())).clicked() {
                        pick = Some(d.id.clone());
                    }
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        ui.label(RichText::new(mode).small().color(theme::MUTED));
                    });
                });
            }
            ui.separator();
            if ui.add_enabled(current.is_some(), egui::Button::new(format!("✎  {}", app.t("devices.rename"))).frame(false)).clicked()
                && let Some(d) = &current
            {
                let m = &mut app.shell.devices;
                m.editing = Some(d.id.clone());
                m.draft = d.name.clone();
                m.focus = true;
            }
            if let Some(d) = current.as_ref().filter(|d| !d.connected)
                && ui.add(egui::Button::image_and_text(icons::image(Icon::remove, 14.0, theme::ERROR), RichText::new(app.t("devices.forget")).color(theme::ERROR)).frame(false)).clicked()
            {
                app.dialogs.push(Dialog::Forget(d.id.clone()));
                egui::Popup::close_all(ui.ctx());
            }
        });
    if let Some(id) = pick {
        egui::Popup::close_all(ui.ctx());
        app.pick_keyboard(id);
    }
}

pub fn truncate(s: &str, n: usize) -> String {
    if s.chars().count() <= n { s.to_string() } else { format!("{}…", s.chars().take(n - 1).collect::<String>()) }
}
