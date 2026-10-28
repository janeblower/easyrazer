//! Questions the window asks over everything else, one at a time.

use easyrazer_backend::settings::CloseAction;
use egui::RichText;

use crate::app::App;
use crate::jobs::Done;
use crate::keys::model::Move;
use crate::text::tag;
use crate::widgets::{self, Answer};
use crate::{about, changes, theme};

pub enum Dialog {
    Close { remember: bool },
    /// Edits not applied would go with the window.
    Hiding,
    ConfirmWrite { dont_ask: bool },
    /// Bindings Apply would send name macros the keyboard's flash lacks.
    WriteMacros { ids: Vec<u16>, then: Done },
    Move { key: u8, m: Move },
    RemoveProfile(u32),
    Forget(String),
    /// Loading another profile or keyboard drops the edits.
    Switch(Box<dyn FnOnce(&mut App) + Send>),
    Autostart,
    About(about::About),
    /// HIDMaestro from an earlier version is still installed.
    LegacyDriver,
}

pub fn show(app: &mut App, ctx: &egui::Context) {
    let Some(d) = app.dialogs.pop() else { return };
    if let Some(d) = ask(app, ctx, d) {
        app.dialogs.push(d);
    }
}

/// Shows one dialog; gives it back while it stays open.
fn ask(app: &mut App, ctx: &egui::Context, d: Dialog) -> Option<Dialog> {
    let t = |k: &str| app.t(k);
    match d {
        Dialog::Close { mut remember } => {
            let unapplied = changes::has_unapplied(app);
            let (text, lost, dont, exit, tray) = (t("dialogs.close.text"), t("dialogs.close.unapplied"), t("common.dontAsk"), t("dialogs.close.exit"), t("dialogs.close.tray"));
            let a = widgets::modal(
                ctx,
                "close",
                &t("dialogs.close.title"),
                |ui| {
                    widgets::para(ui, &text);
                    if unapplied {
                        widgets::para(ui, &lost);
                    }
                    ui.checkbox(&mut remember, RichText::new(dont).color(theme::MUTED));
                },
                |ui| {
                    if widgets::primary(ui, &tray, true).clicked() {
                        return Some(CloseAction::Tray);
                    }
                    ui.button(&exit).clicked().then_some(CloseAction::Exit)
                },
            );
            match a {
                Answer::Open => return Some(Dialog::Close { remember }),
                Answer::Cancel => {}
                Answer::Pick(action) => {
                    // The choice still applies this session even if it could not be saved.
                    if remember && let Err(e) = app.jobs.backend().set_close_action(action) {
                        app.show_error(e);
                    }
                    if action == CloseAction::Tray { app.hide(ctx) } else { app.quit(ctx) }
                }
            }
        }
        Dialog::Hiding => {
            let (text, cancel, tray) = (t("dialogs.close.unapplied"), t("profiles.cancel"), t("dialogs.close.tray"));
            match widgets::modal(ctx, "hiding", &t("dialogs.close.title"), |ui| widgets::para(ui, &text), |ui| yes_no(ui, &tray, &cancel)) {
                Answer::Open => return Some(Dialog::Hiding),
                Answer::Pick(true) => app.hide(ctx),
                _ => {}
            }
        }
        Dialog::ConfirmWrite { mut dont_ask } => {
            let (text, dont, yes, no) = (t("dialogs.write.text"), t("common.dontAsk"), t("dialogs.write.yes"), t("dialogs.write.no"));
            let a = widgets::modal(
                ctx,
                "write",
                &t("dialogs.write.title"),
                |ui| {
                    widgets::para(ui, &text);
                    ui.checkbox(&mut dont_ask, RichText::new(dont).color(theme::MUTED));
                },
                |ui| no_yes(ui, &yes, &no),
            );
            match a {
                Answer::Open => return Some(Dialog::ConfirmWrite { dont_ask }),
                Answer::Pick(true) => {
                    if dont_ask && let Err(e) = app.jobs.backend().set_confirm_write(false) {
                        app.message = e;
                    }
                    changes::write_from(app, 0);
                }
                _ => {}
            }
        }
        Dialog::WriteMacros { ids, then } => {
            let names: Vec<String> = ids
                .iter()
                .map(|id| app.macro_state.as_ref().and_then(|m| m.macros.get(id)).map_or(format!("#{id}"), |m| m.name.clone()))
                .collect();
            let text = app.tf("bindings.writeMacros", &[("names", &names.join(", "))]);
            let (yes, no) = (t("dialogs.write.yes"), t("dialogs.write.no"));
            match widgets::modal(ctx, "write-macros", &t("dialogs.write.title"), |ui| widgets::para(ui, &text), |ui| no_yes(ui, &yes, &no)) {
                Answer::Open => return Some(Dialog::WriteMacros { ids, then }),
                Answer::Pick(true) => app.write_macros_then_apply(ids, then),
                _ => {}
            }
        }
        Dialog::Move { key, m } => {
            let action = t(&format!("bindings.system.{}", tag(&m.action)));
            let to = crate::keys::combo(app, Some(key)).unwrap_or_default();
            let from = crate::keys::combo(app, Some(m.from)).unwrap_or_default();
            let title = app.tf("bindings.moveTitle", &[("action", &action), ("to", &to)]);
            let text = app.tf("bindings.moveText", &[("from", &from)]);
            let (yes, no) = (t("bindings.move"), t("profiles.cancel"));
            match widgets::modal(ctx, "move", &title, |ui| widgets::para(ui, &text), |ui| yes_no(ui, &yes, &no)) {
                Answer::Open => return Some(Dialog::Move { key, m }),
                Answer::Pick(true) => app.keys.confirm_move(key, m),
                _ => {}
            }
        }
        Dialog::RemoveProfile(id) => {
            let name = app.profiles.as_ref().and_then(|v| v.profiles.iter().find(|p| p.id == id)).map_or(String::new(), |p| p.name.clone());
            let title = app.tf("profiles.removeTitle", &[("name", &name)]);
            let (text, yes, no) = (t("profiles.removeText"), t("profiles.remove"), t("profiles.cancel"));
            match widgets::modal(ctx, "remove", &title, |ui| widgets::para(ui, &text), |ui| yes_no(ui, &yes, &no)) {
                Answer::Open => return Some(Dialog::RemoveProfile(id)),
                Answer::Pick(true) => app.profile_op(move |b, kb| b.delete_profile(kb, id)),
                _ => {}
            }
        }
        Dialog::Forget(id) => {
            let name = app.devices.as_ref().and_then(|v| v.devices.iter().find(|d| d.id == id)).map_or(String::new(), |d| d.name.clone());
            let text = app.tf("devices.forgetConfirm", &[("name", &name)]);
            let (yes, no) = (t("devices.forget"), t("profiles.cancel"));
            match widgets::modal(ctx, "forget", &t("devices.forget"), |ui| widgets::para(ui, &text), |ui| yes_no(ui, &yes, &no)) {
                Answer::Open => return Some(Dialog::Forget(id)),
                Answer::Pick(true) => app.forget_keyboard_confirmed(id),
                _ => {}
            }
        }
        Dialog::Switch(op) => {
            let (yes, no) = (t("profiles.drop"), t("profiles.cancel"));
            match widgets::modal(ctx, "switch", &t("profiles.dropTitle"), |_| {}, |ui| yes_no(ui, &yes, &no)) {
                Answer::Open => return Some(Dialog::Switch(op)),
                Answer::Pick(true) => {
                    crate::changes::revert_all(app);
                    op(app);
                }
                _ => {}
            }
        }
        Dialog::Autostart => {
            let (text, yes, no) = (t("dialogs.autostart.text"), t("dialogs.autostart.yes"), t("dialogs.autostart.no"));
            match widgets::modal(ctx, "autostart", &t("dialogs.autostart.title"), |ui| widgets::para(ui, &text), |ui| yes_no(ui, &yes, &no)) {
                Answer::Open => return Some(Dialog::Autostart),
                Answer::Pick(on) => {
                    if let Err(e) = app.jobs.backend().autostart_answered(on) {
                        app.show_error(e);
                    }
                }
                Answer::Cancel => {}
            }
        }
        Dialog::About(mut a) => {
            if about::show(app, ctx, &mut a) {
                return Some(Dialog::About(a));
            }
        }
        Dialog::LegacyDriver => {
            let (text, remove, keep) = (t("gamepad.legacyText"), t("gamepad.legacyRemove"), t("gamepad.legacyKeep"));
            match widgets::modal(ctx, "legacy", &t("gamepad.legacyTitle"), |ui| widgets::para(ui, &text), |ui| yes_no(ui, &remove, &keep)) {
                Answer::Open => return Some(Dialog::LegacyDriver),
                Answer::Pick(remove) => crate::gamepad::change_driver(app, crate::gamepad::DriverOp::Install, remove),
                Answer::Cancel => {}
            }
        }
    }
    None
}

/// The primary button says yes; the plain one no.
fn yes_no(ui: &mut egui::Ui, yes: &str, no: &str) -> Option<bool> {
    if widgets::primary(ui, yes, true).clicked() {
        return Some(true);
    }
    ui.button(no).clicked().then_some(false)
}

/// The safe answer is the primary one: the plain button says yes.
fn no_yes(ui: &mut egui::Ui, yes: &str, no: &str) -> Option<bool> {
    if widgets::primary(ui, no, true).clicked() {
        return Some(false);
    }
    ui.button(yes).clicked().then_some(true)
}
