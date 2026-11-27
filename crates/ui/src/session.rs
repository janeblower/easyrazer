//! Which keyboard the window shows and keeping its picture in step with the backend: reads,
//! writes and profile changes, as `app/src/App.vue` did them.

use easyrazer_backend::Backend;
use easyrazer_backend::api::{Actuation, Edits, ImportView, WriteResult};
use easyrazer_backend::boards::DeviceEntry;
use easyrazer_backend::device::{MacroState, ProfilesView, Status};
use easyrazer_backend::gamepad;
use razer_core::binding::Action;

use crate::app::App;
use crate::dialogs::Dialog;
use crate::jobs::Done;
use crate::keys::model::Sent;

/// The shown keyboard stays while it is known, so a disconnected one keeps its profiles on screen.
pub fn pick(list: &[DeviceEntry], current: Option<&str>, last: Option<&str>) -> Option<String> {
    let known = |id: Option<&str>| list.iter().find(|d| Some(d.id.as_str()) == id).map(|d| d.id.clone());
    known(current)
        .or_else(|| known(last))
        .or_else(|| list.iter().find(|d| d.connected).map(|d| d.id.clone()))
        .or_else(|| list.first().map(|d| d.id.clone()))
}

/// What a status asks of the view; a reconnect reads again from the keyboard even under the same profile.
pub fn status_action(s: &Status, loaded_profile: Option<u32>, loaded_key: Option<(bool, Option<u32>)>) -> (bool, bool) {
    let revert = loaded_profile.is_some() && s.profile.is_some() && s.profile != loaded_profile;
    let load = Some((s.device, s.profile)) != loaded_key;
    (revert, load)
}

/// Everything a read brings, gathered on the worker so the window takes it in one step.
struct Loaded {
    read: Actuation,
    gamepad: Option<Result<gamepad::Config, String>>,
    macros: Result<MacroState, String>,
    profiles: ProfilesView,
}

impl App {
    pub fn load_devices(&mut self) {
        self.jobs.run(Backend::devices, |app, view| {
            app.devices = Some(view);
            app.settle();
        });
    }

    fn settle(&mut self) {
        let Some(d) = &self.devices else { return };
        let next = pick(&d.devices, self.keyboard.as_deref(), d.last.as_deref());
        if next == self.keyboard {
            return;
        }
        self.keyboard = next;
        // Another keyboard: nothing read from the last one applies.
        self.forget_keyboard();
        self.refresh();
    }

    fn forget_keyboard(&mut self) {
        crate::changes::revert_all(self);
        self.loaded_profile = None;
        self.loaded_key = None;
        self.layout_model = None;
        self.depth = [0; 256];
        crate::dev::reload_log(self);
    }

    pub fn refresh(&mut self) {
        let id = self.keyboard.clone();
        self.jobs.run(move |b| b.status(id), App::on_status);
    }

    pub fn on_status(&mut self, s: Status) {
        if !self.ours(s.id.as_deref()) {
            return;
        }
        if let Some(e) = &s.error {
            self.show_error(e.clone());
        }
        // Mid read or apply the table is being rewritten; the status is asked again once it is done.
        if self.busy {
            self.missed = true;
            return;
        }
        if let Some(model) = &s.model
            && self.layout_model.as_ref() != Some(model)
        {
            self.layout_model = Some(model.clone());
            self.layout = easyrazer_backend::api::layout(Some(model));
        }
        if !s.device {
            self.progress = None;
        }
        let synapse = s.synapse;
        let (revert, load) = status_action(&s, self.loaded_profile, self.loaded_key);
        self.status = Some(s);
        if synapse {
            self.loaded_profile = None;
            self.loaded_key = None;
            return;
        }
        if revert {
            self.revert_keys();
        }
        if load {
            self.load();
        }
    }

    pub fn set_busy(&mut self, busy: bool) {
        self.busy = busy;
        if !busy && self.missed {
            self.missed = false;
            self.refresh();
        }
    }

    fn load(&mut self) {
        // Before any keyboard is known there is nothing to read, and the error would hide the status text.
        let Some(id) = self.keyboard.clone() else { return };
        self.busy = true;
        self.progress = Some((0, 1));
        let worker_id = id.clone();
        self.jobs.run(
            move |b| -> Result<Loaded, String> {
                let read = b.read_all(&worker_id)?;
                let gamepad = read.profile.map(|_| b.gamepad_config(&worker_id));
                Ok(Loaded { gamepad, macros: b.macros(&worker_id), profiles: b.profiles(&worker_id), read })
            },
            move |app, r| {
                // The keyboard changed while reading: nothing here is for the new one.
                let stale = !app.ours(Some(&id));
                match r {
                    Ok(l) if !stale => app.take_loaded(l),
                    Err(e) if !stale => app.message = e,
                    _ => {}
                }
                app.progress = None;
                app.set_busy(false);
                if stale {
                    app.refresh();
                }
            },
        );
    }

    fn take_loaded(&mut self, l: Loaded) {
        let profile = l.read.profile;
        self.keys.take_read(l.read);
        self.loaded_profile = profile;
        self.loaded_key = Some((self.online(), profile));
        self.message.clear();
        self.gamepad.ready = false;
        match l.gamepad {
            Some(Ok(config)) => self.gamepad.take(config),
            Some(Err(e)) => self.message = e,
            None => {}
        }
        match l.macros {
            Ok(m) => self.macro_state = Some(m),
            Err(e) => self.show_error(e),
        }
        self.profiles = Some(l.profiles);
    }

    pub fn load_macros(&mut self) {
        let Some(id) = self.keyboard.clone() else { return };
        let worker_id = id.clone();
        self.jobs.run(move |b| b.macros(&worker_id), move |app, r| match r {
            Ok(m) if app.ours(Some(&id)) => app.macro_state = Some(m),
            Err(e) => app.show_error(e),
            _ => {}
        });
    }

    pub fn revert_keys(&mut self) {
        self.gamepad.edit = None;
        self.keys.revert();
    }

    pub fn key_dirty(&self) -> usize {
        self.keys.dirty()
    }

    pub fn dirty(&self) -> usize {
        self.key_dirty() + usize::from(self.gamepad.edit.is_some())
    }

    pub fn can_apply_keys(&self) -> bool {
        (self.writable() && self.dirty() > 0) || (self.editable() && self.gamepad.ready && self.gamepad.edit.is_some() && self.key_dirty() == 0)
    }

    pub fn can_save_keys(&self) -> bool {
        self.writable() && self.keys.savable()
    }

    /// Loading another profile or keyboard drops unapplied edits; ask first.
    pub fn switch_to(&mut self, op: impl FnOnce(&mut App) + Send + 'static) {
        if self.dirty() > 0 {
            self.dialogs.push(Dialog::Switch(Box::new(op)));
        } else {
            op(self);
        }
    }

    pub fn pick_keyboard(&mut self, id: String) {
        if self.ours(Some(&id)) {
            return;
        }
        self.switch_to(move |app| {
            app.keyboard = Some(id.clone());
            app.forget_keyboard();
            if let Err(e) = app.jobs.backend().set_last_device(id) {
                app.show_error(e);
            }
            app.refresh();
        });
    }

    pub fn rename_keyboard(&mut self, id: String, name: String) {
        self.jobs.run(move |b| b.rename_device(&id, &name), |app, r| match r {
            Ok(v) => app.devices = Some(v),
            Err(e) => app.show_error(e),
        });
    }

    pub fn forget_keyboard_confirmed(&mut self, id: String) {
        self.jobs.run(move |b| b.forget_device(&id), |app, r| match r {
            Ok(v) => {
                app.devices = Some(v);
                app.settle();
            }
            Err(e) => app.show_error(e),
        });
    }

    pub fn set_mode(&mut self, on: bool) {
        let Some(id) = self.keyboard.clone() else { return };
        if on == self.driver() {
            return;
        }
        self.busy = true;
        self.jobs.run(move |b| b.set_driver_mode(&id, on), |app, r| {
            if let Err(e) = r {
                app.show_error(e);
            }
            app.set_busy(false);
            app.refresh();
        });
    }

    /// Runs a profile command and takes the list it returns.
    pub fn profile_op(&mut self, op: impl FnOnce(&Backend, &str) -> Result<ProfilesView, String> + Send + 'static) {
        let Some(id) = self.keyboard.clone() else { return };
        self.busy = true;
        let worker_id = id.clone();
        self.jobs.run(move |b| op(b, &worker_id), move |app, r| {
            match r {
                Ok(v) if app.ours(Some(&id)) => app.profiles = Some(v),
                Err(e) => app.show_error(e),
                _ => {}
            }
            app.set_busy(false);
            app.refresh();
        });
    }

    pub fn export_profile(&mut self, profile: u32) {
        let Some(id) = self.keyboard.clone() else { return };
        let name = self.profiles.as_ref().and_then(|v| v.profiles.iter().find(|p| p.id == profile)).map_or("profile".into(), |p| p.name.clone());
        let Some(path) = rfd::FileDialog::new().set_file_name(format!("{name}.easyrazer.json")).add_filter("EasyRazer", &["json"]).save_file() else {
            return;
        };
        self.jobs.run(move |b| b.export_profile(&id, profile, &path), move |app, r| match r {
            Ok(()) => app.message = app.tf("profiles.exported", &[("name", &name)]),
            Err(e) => app.show_error(e),
        });
    }

    pub fn import_profile(&mut self) {
        let Some(id) = self.keyboard.clone() else { return };
        let Some(path) = rfd::FileDialog::new().add_filter("EasyRazer", &["json"]).pick_file() else { return };
        let worker_id = id.clone();
        self.jobs.run(move |b| b.import_profile(&worker_id, &path), move |app, r: Result<ImportView, String>| match r {
            Ok(v) if app.ours(Some(&id)) => {
                app.profiles = Some(v.view);
                let n = v.dropped.to_string();
                app.message = if v.dropped > 0 {
                    app.tf("profiles.importedDropped", &[("name", &v.name), ("n", &n)])
                } else {
                    app.tf("profiles.imported", &[("name", &v.name)])
                };
            }
            Err(e) => app.show_error(e),
            _ => {}
        });
    }

    /// Apply in HW mode needs the macros it binds in the keyboard's flash: the firmware plays only those.
    pub fn apply_keys(&mut self, then: Done) {
        if self.key_dirty() == 0 && self.gamepad.edit.is_some() {
            let Some(config) = self.gamepad.edit.clone() else { return };
            self.busy = true;
            self.store_gamepad(config, Box::new(move |app: &mut App| {
                app.message = app.t("gamepad.applied");
                app.set_busy(false);
                app.refresh();
                then(app);
            }));
            return;
        }
        let ids: Vec<u16> = if self.driver() {
            Vec::new()
        } else {
            let written = |id: &u16| self.macro_state.as_ref().and_then(|m| m.macros.get(id)).is_some_and(|m| m.written);
            let mut ids: Vec<u16> = self
                .keys
                .bind_edits
                .values()
                .chain(self.keys.hs_bind_edits.values())
                .filter_map(|a| match a {
                    Action::Macro { id, .. } if !written(id) => Some(*id),
                    _ => None,
                })
                .collect();
            ids.sort_unstable();
            ids.dedup();
            ids
        };
        if ids.is_empty() {
            self.write_keys(false, then);
        } else {
            self.dialogs.push(Dialog::WriteMacros { ids, then });
        }
    }

    pub fn write_macros_then_apply(&mut self, ids: Vec<u16>, then: Done) {
        let Some(kb) = self.keyboard.clone() else { return };
        self.busy = true;
        let worker_kb = kb.clone();
        self.jobs.run(
            move |b| {
                let mut last = None;
                for id in ids {
                    last = Some(b.write_macro(&worker_kb, id)?);
                }
                Ok(last)
            },
            move |app, r: Result<Option<MacroState>, String>| {
                if !app.ours(Some(&kb)) {
                    app.set_busy(false);
                    return;
                }
                match r {
                    Ok(state) => {
                        if let Some(s) = state {
                            app.macro_state = Some(s);
                        }
                        app.busy = false;
                        app.write_keys(false, then);
                    }
                    Err(e) => {
                        app.message = e;
                        app.set_busy(false);
                    }
                }
            },
        );
    }

    fn store_gamepad(&mut self, config: gamepad::Config, then: Done) {
        let (Some(id), Some(profile)) = (self.keyboard.clone(), self.loaded_profile) else {
            then(self);
            return;
        };
        let worker_id = id.clone();
        let stored = config.clone();
        self.jobs.run(move |b| b.set_gamepad_config(&worker_id, profile, config), move |app, r| {
            match r {
                Ok(()) if app.ours(Some(&id)) && app.loaded_profile == Some(profile) => {
                    app.gamepad.base = stored;
                    app.gamepad.edit = None;
                }
                Err(e) => app.show_error(e),
                _ => {}
            }
            then(app);
        });
    }

    /// Apply sends the edits to the keyboard's memory; Save also puts them into its flash.
    pub fn write_keys(&mut self, save: bool, then: Done) {
        let Some(id) = self.keyboard.clone() else { return };
        self.busy = true;
        let gamepad = (!save).then(|| self.gamepad.edit.clone()).flatten();
        let k = &self.keys;
        let edits = Edits {
            changes: k.changes(),
            rapid: if save { Default::default() } else { k.rapid_edits.clone() },
            snap: if save { None } else { k.snap_edit.clone() },
            continuous: if save { None } else { k.crt_edit },
            bindings: k.bindings(),
            hypershift: k.hypershift(),
        };
        let sent = (!save).then(|| Sent { rapid: edits.rapid.clone(), snap: edits.snap.clone(), continuous: edits.continuous });
        let worker_id = id.clone();
        let profile = self.loaded_profile;
        self.jobs.run(
            move |b| -> Result<WriteResult, String> {
                if let (Some(config), Some(profile)) = (gamepad, profile) {
                    b.set_gamepad_config(&worker_id, profile, config)?;
                }
                if save { b.save(&worker_id, &edits.changes, &edits.bindings, &edits.hypershift) } else { b.apply(&worker_id, &edits) }
            },
            move |app, r| {
                let mut failed = false;
                match r {
                    // The window moved to another keyboard meanwhile; its edits were dropped with the switch.
                    Ok(_) if !app.ours(Some(&id)) => {}
                    Ok(w) => {
                        if !save && let Some(c) = app.gamepad.edit.take() {
                            app.gamepad.base = c;
                        }
                        let lang = app.lang.clone();
                        let bad = app.keys.take_written(w, sent, |mm| match mm {
                            Some(v) => easyrazer_backend::i18n::tf(&lang, "actuation.unconfirmed", &[("v", &format!("{v:.1}"))]),
                            None => easyrazer_backend::i18n::t(&lang, "bindings.unconfirmed"),
                        });
                        app.message = if bad > 0 {
                            app.tf("actuation.failed", &[("n", &bad.to_string())])
                        } else {
                            app.t(if save { "actuation.saved" } else { "actuation.applied" })
                        };
                    }
                    Err(e) => {
                        app.message = e;
                        failed = true;
                    }
                }
                app.set_busy(false);
                if failed {
                    app.refresh();
                }
                then(app);
            },
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(id: &str, connected: bool) -> DeviceEntry {
        DeviceEntry { id: id.into(), name: id.into(), model: "Kb".into(), connected, driver_mode: false }
    }

    fn status(device: bool, profile: Option<u32>) -> Status {
        Status { id: None, device, synapse: false, driver_mode: false, profile, profile_name: None, model: None, verified: true, analog: None, error: None }
    }

    #[test]
    fn the_shown_keyboard_stays_while_known() {
        let list = [entry("a", false), entry("b", true)];
        assert_eq!(pick(&list, Some("a"), Some("b")).as_deref(), Some("a"));
        assert_eq!(pick(&list, Some("gone"), Some("b")).as_deref(), Some("b"));
        assert_eq!(pick(&list, None, None).as_deref(), Some("b"), "a connected one first");
        assert_eq!(pick(&[], None, None), None);
    }

    #[test]
    fn a_reconnect_reads_again_and_another_profile_drops_edits() {
        assert_eq!(status_action(&status(true, Some(1)), Some(1), Some((false, Some(1)))), (false, true));
        assert_eq!(status_action(&status(true, Some(2)), Some(1), Some((true, Some(1)))), (true, true));
        assert_eq!(status_action(&status(true, Some(1)), Some(1), Some((true, Some(1)))), (false, false));
    }
}
