//! The profile menu: load, create, rename and manage the profiles of the shown keyboard.

use egui::{RichText, Stroke, Ui};

use crate::app::App;
use crate::dialogs::Dialog;
use crate::icons::{self, Icon};
use crate::theme;

use super::truncate;

/// The keyboard keeps names this long.
const NAME_MAX: usize = 32;

#[derive(Default)]
pub struct Menu {
    /// Row whose ⋯ actions are open.
    actions: Option<u32>,
    editing: Option<u32>,
    draft: String,
    focus: bool,
    /// A created profile arrives loaded; it is then renamed in place.
    creating: bool,
    last_loaded: Option<u32>,
}

fn badge(ui: &mut Ui, text: String, flash: bool) {
    let (color, line) = if flash { (theme::WRITE, theme::WRITE_LINE) } else { (theme::MUTED, theme::LINE) };
    egui::Frame::new().stroke(Stroke::new(1.0, line)).corner_radius(9).inner_margin(egui::Margin::symmetric(6, 1)).show(ui, |ui| {
        ui.label(RichText::new(text).size(11.0).color(color));
    });
}

pub fn menu(app: &mut App, ui: &mut Ui) {
    let view = app.profiles.clone();
    let (writable, editable) = (app.writable(), app.editable());
    let loaded = view.as_ref().and_then(|v| v.profiles.iter().find(|p| Some(p.id) == v.loaded)).cloned();
    if let Some(v) = &view {
        follow_created(app, v.loaded);
    }
    let r = ui
        .add_enabled_ui(view.is_some(), |ui| {
            let title = loaded.as_ref().map_or("…".to_string(), |p| truncate(&p.name, 24));
            let slot = loaded.as_ref().and_then(|p| p.slot).map(|s| format!("  {s}/5")).unwrap_or_default();
            ui.add(egui::Button::new(format!("{title}{slot}  ▾")).min_size(egui::vec2(190.0, 32.0)))
        })
        .inner;
    let Some(view) = view else { return };
    let next_combo = crate::keys::combo(app, crate::keys::holder(app, razer_core::binding::System::NextProfile));
    let popup = egui::Popup::from_toggle_button_response(&r)
        .align(egui::RectAlign::BOTTOM_END)
        .width(340.0)
        .close_behavior(egui::PopupCloseBehavior::CloseOnClickOutside);
    let shown = popup.show(|ui| {
        ui.spacing_mut().item_spacing.y = 2.0;
        let slotted = view.profiles.iter().filter(|q| q.slot.is_some()).count();
        for p in &view.profiles {
            let on = Some(p.id) == view.loaded;
            let row = ui.horizontal(|ui| {
                ui.set_min_height(28.0);
                let startup = p.slot.is_some() && p.slot == view.startup;
                ui.add_sized([16.0, 20.0], egui::Label::new(RichText::new(if startup { "⏻" } else { "" }).color(theme::ACCENT)))
                    .on_hover_text(app.t("profiles.isStartup"));
                let m = &mut app.shell.profiles;
                let mut picked = false;
                if m.editing == Some(p.id) {
                    let taken = name_taken(&view.profiles, p.id, &m.draft);
                    let mut edit = egui::TextEdit::singleline(&mut m.draft).char_limit(NAME_MAX).desired_width(170.0);
                    if taken {
                        edit = edit.text_color(theme::ERROR);
                    }
                    let resp = ui.add(edit);
                    if m.focus {
                        resp.request_focus();
                        m.focus = false;
                    }
                    if taken {
                        resp.show_tooltip_text(easyrazer_backend::i18n::t(&app.lang, "profiles.nameTakenHint"));
                    }
                    let m = &mut app.shell.profiles;
                    if ui.input(|i| i.key_pressed(egui::Key::Escape)) {
                        m.editing = None;
                    } else if resp.lost_focus() {
                        let (id, name) = (p.id, m.draft.trim().to_string());
                        m.editing = None;
                        if !name.is_empty() && !taken {
                            app.profile_op(move |b, kb| b.rename_profile(kb, id, &name));
                        }
                    }
                } else {
                    let label = RichText::new(truncate(&p.name, 22));
                    picked = ui.add(egui::Label::new(label).sense(egui::Sense::click())).clicked();
                }
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    let open = app.shell.profiles.actions == Some(p.id);
                    let dots = egui::Button::new(RichText::new("⋯").color(if open { theme::TEXT } else { theme::MUTED })).frame(open).min_size(egui::vec2(26.0, 22.0));
                    if ui.add(dots).on_hover_text(app.t("profiles.menu")).clicked() {
                        app.shell.profiles.actions = if open { None } else { Some(p.id) };
                    }
                    match p.slot {
                        Some(s) => badge(ui, format!("{s}/5"), true),
                        None => badge(ui, app.t("profiles.onlyHere"), false),
                    }
                    if p.unsaved {
                        ui.label(RichText::new(app.t("profiles.unsaved")).small().color(theme::EDITED));
                    }
                });
                picked
            });
            if on {
                let rect = row.response.rect;
                ui.painter().vline(rect.left() - 2.0, rect.y_range(), Stroke::new(2.0, theme::ACCENT));
            }
            if row.inner && editable && app.shell.profiles.editing.is_none() {
                egui::Popup::close_all(ui.ctx());
                if !on {
                    let id = p.id;
                    app.switch_to(move |app| app.profile_op(move |b, kb| b.load_profile(kb, id)));
                }
            }
            if app.shell.profiles.actions == Some(p.id) {
                actions(app, ui, &view, p, writable, editable, slotted);
            }
        }
        ui.separator();
        let new = egui::Button::new(RichText::new(format!("+  {}", app.t("profiles.new"))).color(theme::MUTED)).frame(false);
        if ui.add_enabled(editable, new).clicked() {
            let m = &mut app.shell.profiles;
            m.actions = None;
            m.creating = true;
            app.switch_to(|app| app.profile_op(|b, kb| b.create_profile(kb)));
        }
        let import = egui::Button::new(RichText::new(format!("⇩  {}", app.t("profiles.import"))).color(theme::MUTED)).frame(false);
        if ui.add_enabled(editable, import).clicked() {
            egui::Popup::close_all(ui.ctx());
            app.import_profile();
        }
        ui.separator();
        let combo = next_combo.clone().unwrap_or_else(|| app.t("profiles.noCombo"));
        ui.label(RichText::new(app.tf("profiles.legend", &[("combo", &combo)])).size(11.0).color(theme::MUTED));
    });
    if shown.is_none() {
        let m = &mut app.shell.profiles;
        m.actions = None;
        m.editing = None;
        m.creating = false;
    }
}

#[allow(clippy::too_many_arguments)]
fn actions(app: &mut App, ui: &mut Ui, view: &easyrazer_backend::device::ProfilesView, p: &easyrazer_backend::device::ProfileView, writable: bool, editable: bool, slotted: usize) {
    egui::Frame::new().fill(theme::KEY).corner_radius(6).inner_margin(egui::Margin::symmetric(8, 4)).show(ui, |ui| {
        ui.set_width(ui.available_width());
        let id = p.id;
        let item = |ui: &mut Ui, glyph: &str, text: String, enabled: bool| ui.add_enabled(enabled, egui::Button::new(format!("{glyph}  {text}")).frame(false));
        let mut close = false;
        match p.slot {
            Some(slot) => {
                let starts = Some(slot) == view.startup;
                let key = if starts { "profiles.isStartup" } else { "profiles.startup" };
                if item(ui, "⏻", app.t(key), writable && !starts).clicked() {
                    app.profile_op(move |b, kb| b.set_startup(kb, id));
                    close = true;
                }
            }
            None => {
                let tip = if view.free_slot { String::new() } else { app.t("profiles.noSlot") };
                let r = ui.add_enabled(
                    writable && view.free_slot,
                    egui::Button::image_and_text(icons::image(Icon::write, 14.0, theme::WRITE), app.t("profiles.write")).frame(false),
                );
                if r.on_disabled_hover_text(tip).clicked() {
                    app.profile_op(move |b, kb| b.write_profile(kb, id));
                    close = true;
                }
            }
        }
        if item(ui, "✎", app.t("profiles.rename"), if p.slot.is_some() { writable } else { editable }).clicked() {
            let m = &mut app.shell.profiles;
            m.actions = None;
            m.editing = Some(id);
            m.draft = p.name.clone();
            m.focus = true;
        }
        if item(ui, "⧉", app.t("profiles.duplicate"), editable).clicked() {
            app.profile_op(move |b, kb| b.duplicate_profile(kb, id));
            close = true;
        }
        if item(ui, "⇧", app.t("profiles.exportOne"), true).clicked() {
            egui::Popup::close_all(ui.ctx());
            app.export_profile(id);
            close = true;
        }
        if let Some(slot) = p.slot
            && item(ui, "⊘", app.tf("profiles.free", &[("slot", &slot.to_string())]), writable && slotted > 1).clicked()
        {
            app.profile_op(move |b, kb| b.free_slot(kb, id));
            close = true;
        }
        ui.separator();
        let can_remove = view.profiles.len() > 1 && if p.slot.is_some() { writable } else { editable };
        let remove = egui::Button::image_and_text(icons::image(Icon::remove, 14.0, theme::ERROR), RichText::new(app.t("profiles.remove")).color(theme::ERROR)).frame(false);
        if ui.add_enabled(can_remove, remove).clicked() {
            app.dialogs.push(Dialog::RemoveProfile(id));
            egui::Popup::close_all(ui.ctx());
            close = true;
        }
        if close {
            app.shell.profiles.actions = None;
        }
    });
}

/// The backend refuses such a rename too; checked here so the field can say why.
fn name_taken(list: &[easyrazer_backend::device::ProfileView], id: u32, draft: &str) -> bool {
    let key = draft.trim().to_lowercase();
    list.iter().any(|p| p.id != id && p.name.trim().to_lowercase() == key)
}

/// A profile just created arrives loaded and goes straight into renaming.
fn follow_created(app: &mut App, loaded: Option<u32>) {
    let m = &mut app.shell.profiles;
    let changed = loaded != m.last_loaded;
    let before = m.last_loaded;
    m.last_loaded = loaded;
    if !m.creating || !changed || loaded.is_none() || before.is_none() {
        return;
    }
    m.creating = false;
    if let Some(p) = app.profiles.as_ref().and_then(|v| v.profiles.iter().find(|p| Some(p.id) == loaded)) {
        let name = p.name.clone();
        let m = &mut app.shell.profiles;
        m.editing = loaded;
        m.draft = name;
        m.focus = true;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use easyrazer_backend::device::ProfileView;

    #[test]
    fn a_name_is_taken_regardless_of_case_and_spaces() {
        let list = [ProfileView { id: 1, name: "Games".into(), slot: None, unsaved: false }, ProfileView { id: 2, name: "Work".into(), slot: None, unsaved: false }];
        assert!(name_taken(&list, 2, " games "));
        assert!(!name_taken(&list, 1, "Games"), "its own name is free");
    }
}
