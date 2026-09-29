//! Owns the keyboard connection: opening, reconnecting, Synapse detection, device mode.

use std::collections::BTreeMap;
use std::os::windows::process::CommandExt;
use std::path::PathBuf;
use std::process::{Command, Output};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use hidapi::HidApi;
use razer_core::actuation::{self, Outcome};
use razer_core::analog::{self, KeyAssignment};
use razer_core::binding::{self, Action};
use razer_core::devices::DeviceSpec;
use razer_core::hid::{self, HidTransport};
use razer_core::lighting::{self, EffectInfo, Look, Rgb, Store};
use razer_core::rapid::{self, Config, Trigger};
use razer_core::transport::Error;
use razer_core::macros::{self, Event};
use razer_core::{control, keymap, layout};
use serde::Serialize;

use crate::dynamic_lighting;
use crate::engine::{self, EngineHandle, Sink};
use crate::i18n;
use crate::settings::{self, Macro, Rapid, Settings};

pub const CREATE_NO_WINDOW: u32 = 0x0800_0000;


#[derive(Clone, Debug, Serialize)]
pub struct Status {
    pub device: bool,
    pub synapse: bool,
    pub mode: Option<u8>,
    /// Driver mode chosen in the app: the host engine types, Rapid Trigger works.
    pub driver_mode: bool,
    pub profile: Option<u8>,
    pub model: Option<String>,
    /// PID of a Razer keyboard without a description.
    pub unsupported: Option<u16>,
    /// Restoring applied settings after a connect failed; reported once.
    pub error: Option<String>,
}

#[derive(Clone, Debug, Serialize)]
pub struct MacroState {
    pub macros: BTreeMap<u16, Macro>,
    /// Free bytes in the keyboard's macro store.
    pub free: Option<u32>,
}

/// Keys after a write, and the keys a replug would reset.
pub struct Written {
    pub results: Vec<(u8, Outcome)>,
    pub unsaved: Vec<u8>,
    pub bindings: Vec<(u8, Outcome)>,
    pub unsaved_bindings: Vec<u8>,
}

#[derive(Clone, Debug, Serialize)]
pub struct LightingState {
    pub effects: Vec<EffectInfo>,
    pub applied: Option<Look>,
    /// What the keyboard's flash holds; `None` when unknown or unreadable.
    pub saved: Option<Look>,
    pub dynamic_lighting: bool,
    pub confirm_write: bool,
    pub custom: Option<BTreeMap<u8, Rgb>>,
}

pub struct Device {
    api: HidApi,
    control: Option<(HidTransport, &'static DeviceSpec)>,
    /// Last known Synapse state; assumed running until checked.
    synapse: bool,
    /// A backup has been written this session.
    backed_up: bool,
    settings: Settings,
    /// Description of the last keyboard seen, so the lighting tab can be edited while it is unplugged.
    last_spec: Option<&'static DeviceSpec>,
    restore_error: Option<String>,
    /// Runs in driver mode; the keyboard only reports depth meanwhile.
    engine: Option<EngineHandle>,
    /// Live assignments read this connect and updated by writes: the engine's press points and bindings.
    live: Option<BTreeMap<u8, KeyAssignment>>,
    sink: Option<Sink>,
}

impl Device {
    pub fn new() -> Result<Self, Error> {
        let api = HidApi::new().map_err(|e| Error::Io(e.to_string()))?;
        Ok(Self {
            api,
            control: None,
            synapse: true,
            backed_up: false,
            settings: settings::load(),
            last_spec: None,
            restore_error: None,
            engine: None,
            live: None,
            sink: None,
        })
    }

    /// Reopens the control interface if the keyboard was replugged; `true` when connected.
    pub fn ensure_connected(&mut self) -> bool {
        let alive = self.control.as_ref().is_some_and(|(t, _)| control::mode(t).is_ok());
        if !alive {
            self.engine = None;
            self.live = None;
            self.control = None;
            let _ = self.api.refresh_devices();
            self.control = hid::open_control(&self.api).ok().flatten();
            if let Some((_, d)) = &self.control {
                self.last_spec = Some(*d);
                if !self.synapse {
                    self.restore();
                }
            }
        }
        self.control.is_some()
    }

    /// Brings back what was applied but not saved: the keyboard forgets it on unplug and on a mode switch, Synapse overwrites it.
    fn restore(&mut self) {
        let Some((t, d)) = &self.control else { return };
        let mut errors = Vec::new();
        if let Some(look) = &self.settings.applied
            && let Err(e) = lighting::set_look(t, d, Store::Temporary, look)
        {
            errors.push(i18n::tf(self.lang(), "backend.restoreLighting", &[("error", &e.to_string())]));
        }
        let changes = pairs(&self.settings.actuation);
        errors.extend(restore_error(self.lang(), "backend.restoreActuation", &actuation::apply(t, actuation::LIVE, &changes)));
        // In driver mode the firmware ignores bindings in the live copy; the engine applies them.
        if self.engine.is_none() {
            let bindings = pairs(&self.settings.bindings);
            errors.extend(restore_error(self.lang(), "backend.restoreBindings", &binding::apply(t, actuation::LIVE, &bindings)));
        }
        if !errors.is_empty() {
            self.restore_error = Some(errors.join("; "));
        }
    }

    pub fn lang(&self) -> &str {
        self.settings.language.as_deref().unwrap_or("en")
    }

    fn msg(&self, key: &str) -> String {
        i18n::t(self.lang(), key)
    }

    fn keyboard(&mut self) -> Result<(&HidTransport, &'static DeviceSpec), String> {
        let missing = self.msg("backend.noKeyboard");
        self.connect().ok_or(missing)
    }

    pub fn connect(&mut self) -> Option<(&HidTransport, &'static DeviceSpec)> {
        self.ensure_connected();
        self.control.as_ref().map(|(t, d)| (t, *d))
    }

    /// `fresh_synapse` is a Synapse check made just now, outside the device lock.
    pub fn status(&mut self, fresh_synapse: Option<bool>) -> Status {
        let was = self.synapse;
        if let Some(s) = fresh_synapse {
            self.synapse = s;
        }
        let synapse = self.synapse;
        self.ensure_connected();
        if should_restore(was, fresh_synapse) {
            self.restore();
        }
        if self.control.is_none() {
            let unsupported = hid::unsupported_keyboard(&self.api);
            let error = self.restore_error.take();
            let driver_mode = self.settings.driver_mode;
            return Status { device: false, synapse, mode: None, driver_mode, profile: None, model: None, unsupported, error };
        }
        self.sync_engine();
        let error = self.restore_error.take();
        let running = self.engine.is_some();
        let Some((t, spec)) = &self.control else { unreachable!() };
        let mut mode = control::mode(t).ok();
        let released = should_release_driver_mode(fresh_synapse.is_some(), synapse, mode, running) && control::set_hardware_mode(t).is_ok();
        if released {
            mode = control::mode(t).ok();
        }
        let profile = control::active_profile(t).ok();
        let model = Some(spec.name.clone());
        if released {
            self.restore();
        }
        Status { device: true, synapse, mode, driver_mode: self.settings.driver_mode, profile, model, unsupported: None, error }
    }

    /// Assignments the keyboard types with now.
    pub fn read_all(&mut self, mut progress: impl FnMut(usize, usize)) -> Result<Vec<KeyAssignment>, String> {
        if self.synapse {
            return Err(self.msg("backend.synapseRunning"));
        }
        let (t, _) = self.keyboard()?;
        let keys = editable_keys();
        let all = actuation::read_all(t, actuation::LIVE, &keys, |n| progress(n, keys.len())).map_err(|e| e.to_string())?;
        self.live = Some(all.iter().map(|a| (a.key, a.clone())).collect());
        Ok(all)
    }

    /// What each key does now: the applied binding over the live one; `None` when the app cannot show it.
    pub fn bindings(&self) -> BTreeMap<u8, Option<Action>> {
        let live = self.live.iter().flatten().map(|(&k, a)| (k, binding::decode(a.fn_id, &a.fn_data)));
        live.chain(self.settings.bindings.iter().map(|(&k, &a)| (k, Some(a)))).collect()
    }

    /// Applies press points and bindings until the next replug (the app restores them on every
    /// connect) and Rapid Trigger, which lives only here.
    pub fn apply(&mut self, changes: &[(u8, f32)], rapid: &BTreeMap<u8, Rapid>, bindings: &[(u8, Action)]) -> Result<Written, String> {
        if self.check_synapse(synapse_running) {
            return Err(self.msg("backend.synapseRunning"));
        }
        let host = self.engine.is_some();
        let unwritten = self.msg("backend.macroUnwritten");
        let written = written_macros(&self.settings);
        let (t, _) = self.keyboard()?;
        let bound: Vec<(u8, Outcome)> = if host {
            bindings.iter().map(|&(k, a)| (k, host_bound(k, a))).collect()
        } else {
            // The firmware plays only bodies in its flash.
            bindings
                .iter()
                .flat_map(|&(k, a)| match a {
                    Action::Macro { id, .. } if !written.contains(&id) => vec![(k, Outcome::Failed(Error::BadArgument(unwritten.clone())))],
                    _ => binding::apply(t, actuation::LIVE, &[(k, a)]),
                })
                .collect()
        };
        let done: Vec<(u8, Action)> = bindings.iter().copied().filter(|(k, _)| saved(&bound).any(|s| s == *k)).collect();
        let differ = control::active_profile(t).and_then(|p| binding::unsaved(t, p, &done));
        let unsaved_bound: Vec<(u8, Action)> = match differ {
            Ok(keys) => done.iter().copied().filter(|(k, _)| keys.contains(k)).collect(),
            Err(_) => done.clone(),
        };
        let results = actuation::apply(t, actuation::LIVE, changes);
        let touched: Vec<u8> = results.iter().filter(|(_, o)| !matches!(o, Outcome::Failed(_))).map(|&(k, _)| k).collect();
        // Unknown whether they match the profile: keep them all, restoring an equal value is harmless.
        let unsaved = match control::active_profile(t).and_then(|p| actuation::unsaved(t, p, &touched)) {
            Ok(u) => u.iter().map(|a| (a.key, mm(a.threshold_low))).collect(),
            Err(_) => changes.iter().copied().filter(|(k, _)| touched.contains(k)).collect::<Vec<_>>(),
        };
        track(&mut self.settings.actuation, touched, unsaved);
        track(&mut self.settings.bindings, done.iter().map(|&(k, _)| k), unsaved_bound);
        if !host {
            self.note(&bound);
        }
        self.note(&results);
        self.settings.rapid.extend(rapid.iter().map(|(&k, &r)| (k, r)));
        settings::save(&self.settings)?;
        self.sync_engine();
        Ok(self.written(results, bound))
    }

    /// Writes applied press points and bindings with the edits on top to the flash; checks Synapse
    /// afresh and backs up the keyboard before the first write.
    pub fn save(&mut self, edits: &[(u8, f32)], bind_edits: &[(u8, Action)]) -> Result<Written, String> {
        let changes = to_save(&self.settings.actuation, edits);
        let bindings = to_save(&self.settings.bindings, bind_edits);
        if changes.is_empty() && bindings.is_empty() {
            return Ok(self.written(Vec::new(), Vec::new()));
        }
        if self.check_synapse(synapse_running) {
            return Err(self.msg("backend.synapseRunning"));
        }
        let (backed_up, host) = (self.backed_up, self.engine.is_some());
        // A key bound in the flash must find its macro there too.
        let bodies = bodies_to_write(&self.settings, &bindings);
        let (t, _) = self.keyboard()?;
        let profile = control::active_profile(t).map_err(|e| e.to_string())?;
        if !backed_up {
            let all = actuation::read_all(t, profile, &editable_keys(), |_| {}).map_err(|e| e.to_string())?;
            write_backup(&actuation::format_backup(profile, &all))?;
        }
        for (id, body) in &bodies {
            macros::write(t, *id, body).map_err(|e| e.to_string())?;
        }
        let results = actuation::save(t, profile, &changes);
        let bound = binding::save(t, profile, &bindings, !host);
        for (id, _) in bodies {
            if let Some(m) = self.settings.macros.get_mut(&id) {
                m.written = true;
            }
        }
        self.backed_up = true;
        self.note(&results);
        self.note(&bound);
        track(&mut self.settings.actuation, saved(&results), []);
        track(&mut self.settings.bindings, saved(&bound), []);
        settings::save(&self.settings)?;
        self.sync_engine();
        Ok(self.written(results, bound))
    }

    fn written(&self, results: Vec<(u8, Outcome)>, bindings: Vec<(u8, Outcome)>) -> Written {
        Written {
            results,
            unsaved: self.settings.actuation.keys().copied().collect(),
            bindings,
            unsaved_bindings: self.settings.bindings.keys().copied().collect(),
        }
    }

    pub fn lighting_state(&mut self) -> Result<LightingState, String> {
        let dynamic_lighting = dynamic_lighting::enabled();
        self.ensure_connected();
        let saved = match (&self.control, self.synapse) {
            (Some((t, d)), false) => lighting::get_look(t, d, Store::Saved).ok().flatten(),
            _ => None,
        };
        Ok(LightingState {
            effects: self.last_spec.map(lighting::effect_infos).unwrap_or_default(),
            applied: match self.last_spec {
                Some(d) => usable(d, self.settings.applied.clone()),
                None => self.settings.applied.clone(),
            },
            saved,
            dynamic_lighting,
            confirm_write: self.settings.confirm_write,
            custom: self.settings.custom.clone(),
        })
    }

    pub fn lighting_preview(&mut self, look: &Look) -> Result<(), String> {
        if self.synapse {
            return Err(self.msg("backend.synapseRunning"));
        }
        let (t, d) = self.keyboard()?;
        lighting::set_look(t, d, Store::Temporary, look).map_err(|e| e.to_string())
    }

    /// Remembers the look; without a keyboard it is shown on the next connect.
    pub fn lighting_apply(&mut self, look: Look) -> Result<(), String> {
        if self.synapse {
            return Err(self.msg("backend.synapseRunning"));
        }
        if let Some((t, d)) = self.connect() {
            lighting::set_look(t, d, Store::Temporary, &look).map_err(|e| e.to_string())?;
        }
        if look.effect.name == lighting::CUSTOM {
            self.settings.custom = look.effect.colors.clone();
        }
        self.settings.applied = Some(look);
        settings::save(&self.settings)
    }

    /// Writes the keyboard's flash; checks Synapse afresh like actuation writes.
    pub fn lighting_write(&mut self, look: Look) -> Result<(), String> {
        if self.check_synapse(synapse_running) {
            return Err(self.msg("backend.synapseRunning"));
        }
        let (t, d) = self.keyboard()?;
        lighting::set_look(t, d, Store::Saved, &look).map_err(|e| e.to_string())?;
        lighting::set_look(t, d, Store::Temporary, &look).map_err(|e| e.to_string())?;
        self.settings.applied = Some(look);
        settings::save(&self.settings)
    }

    pub fn set_confirm_write(&mut self, on: bool) -> Result<(), String> {
        self.update_settings(|s| s.confirm_write = on)
    }

    /// Fresh check before a write, honouring `watch_synapse`.
    fn check_synapse(&mut self, running: impl FnOnce() -> bool) -> bool {
        self.synapse = synapse_check(self.settings.watch_synapse, running);
        self.synapse
    }

    pub fn settings(&self) -> &Settings {
        &self.settings
    }

    pub fn update_settings(&mut self, f: impl FnOnce(&mut Settings)) -> Result<(), String> {
        f(&mut self.settings);
        settings::save(&self.settings)
    }

    pub fn set_dynamic_lighting(&mut self, on: bool) -> Result<(), String> {
        if !on && self.synapse {
            return Err(self.msg("backend.synapseRunning"));
        }
        dynamic_lighting::set(on)?;
        if on {
            return Ok(());
        }
        let d = self.keyboard()?.1;
        hid::set_autonomous(&self.api, d, true).map_err(|e| e.to_string())?;
        if let (Some((t, _)), Some(look)) = (&self.control, &self.settings.applied) {
            lighting::set_look(t, d, Store::Temporary, look).map_err(|e| e.to_string())?;
        }
        Ok(())
    }

    pub fn set_sink(&mut self, sink: Sink) {
        self.sink = Some(sink);
    }

    /// Keeps the live map in step with what a write left in the keyboard.
    fn note(&mut self, results: &[(u8, Outcome)]) {
        let Some(live) = &mut self.live else { return };
        for (k, o) in results {
            if let (Outcome::Ok(a) | Outcome::Unconfirmed(a), Some(l)) = (o, live.get_mut(k)) {
                *l = KeyAssignment { profile: l.profile, ..a.clone() };
            }
        }
    }

    /// Switches between hardware and driver mode; stays in hardware mode if the engine cannot start.
    pub fn set_driver_mode(&mut self, on: bool) -> Result<(), String> {
        if self.synapse {
            return Err(self.msg("backend.synapseRunning"));
        }
        self.settings.driver_mode = on;
        self.sync_engine();
        if on && self.engine.is_none() {
            self.settings.driver_mode = false;
            return Err(self.restore_error.take().unwrap_or_else(|| self.msg("backend.noKeyboard")));
        }
        settings::save(&self.settings)
    }

    /// Runs the engine, and driver mode with it, while the app is in driver mode and Synapse is away.
    fn sync_engine(&mut self) {
        if self.engine.as_ref().is_some_and(|e| !e.alive()) {
            self.engine = None;
        }
        if !wants_engine(self.synapse, self.settings.driver_mode) || self.control.is_none() {
            self.stop_engine();
            return;
        }
        if self.live.is_none() {
            let (t, _) = self.control.as_ref().unwrap();
            match actuation::read_all(t, actuation::LIVE, &editable_keys(), |_| {}) {
                Ok(all) => self.live = Some(all.into_iter().map(|a| (a.key, a)).collect()),
                Err(e) => {
                    self.restore_error = Some(i18n::tf(self.lang(), "backend.engine", &[("error", &e.to_string())]));
                    return;
                }
            }
        }
        let thresholds = self.live.iter().flatten().map(|(&k, a)| (k, a.threshold_low)).collect();
        let mut cfg = engine_config(&thresholds, &self.bindings(), &self.settings.rapid, engine::repeat_timing());
        cfg.macros = self.settings.macros.iter().map(|(&id, m)| (id, m.events.clone())).collect();
        if let Some(e) = &self.engine {
            e.set_config(cfg);
            return;
        }
        let Some(sink) = self.sink.clone() else { return };
        let (t, d) = self.control.as_ref().unwrap();
        // Readers first: if they cannot open, the keyboard never leaves hardware mode.
        let started = EngineHandle::start(&self.api, d.pid, cfg, sink)
            .and_then(|h| control::set_driver_mode(t).map(|()| h).map_err(|e| e.to_string()));
        match started {
            Ok(h) => {
                self.engine = Some(h);
                self.restore();
            }
            Err(e) => self.restore_error = Some(i18n::tf(self.lang(), "backend.engine", &[("error", &e)])),
        }
    }

    /// Stops the engine and hands typing back to the firmware, unless Synapse owns the keyboard now.
    pub fn stop_engine(&mut self) {
        if self.engine.take().is_some()
            && !self.synapse
            && let Some((t, _)) = &self.control
            && control::set_hardware_mode(t).is_ok()
        {
            self.restore();
        }
    }

    /// The library and the free flash; `None` without a keyboard to ask.
    pub fn macros(&mut self) -> MacroState {
        let free = match (self.synapse, self.connect()) {
            (false, Some((t, _))) => macros::free(t).ok(),
            _ => None,
        };
        MacroState { macros: self.settings.macros.clone(), free }
    }

    /// Stores a macro in the library, a new one under a fresh id; the engine plays it at once.
    pub fn set_macro(&mut self, id: Option<u16>, name: String, events: Vec<Event>) -> Result<u16, String> {
        if macros::encode(&events).is_none() {
            return Err(format!("cannot encode {events:?}"));
        }
        let id = id.unwrap_or_else(|| next_macro_id(&self.settings.macros));
        let written = self.settings.macros.get(&id).is_some_and(|m| m.written && m.events == events);
        self.settings.macros.insert(id, Macro { name, events, written });
        settings::save(&self.settings)?;
        self.sync_engine();
        Ok(id)
    }

    /// Puts the body into the keyboard's flash, replacing the one written before.
    pub fn write_macro(&mut self, id: u16) -> Result<(), String> {
        if self.check_synapse(synapse_running) {
            return Err(self.msg("backend.synapseRunning"));
        }
        let body = self.settings.macros.get(&id).and_then(|m| macros::encode(&m.events)).ok_or_else(|| format!("no macro {id}"))?;
        let (t, _) = self.keyboard()?;
        macros::write(t, id, &body).map_err(|e| e.to_string())?;
        if let Some(m) = self.settings.macros.get_mut(&id) {
            m.written = true;
        }
        settings::save(&self.settings)
    }

    /// Removes the macro from the library and its body from the flash.
    pub fn delete_macro(&mut self, id: u16) -> Result<(), String> {
        if self.settings.macros.get(&id).is_some_and(|m| m.written) {
            if self.check_synapse(synapse_running) {
                return Err(self.msg("backend.synapseRunning"));
            }
            let (t, _) = self.keyboard()?;
            macros::delete(t, id).map_err(|e| e.to_string())?;
        }
        self.settings.macros.remove(&id);
        settings::save(&self.settings)?;
        self.sync_engine();
        Ok(())
    }

    /// Fn+F11/F12 in driver mode; the firmware's own step is about a tenth.
    pub fn step_brightness(&mut self, up: bool) {
        let applied = self.settings.applied.as_ref().map(|l| l.brightness);
        let Some((t, d)) = self.connect() else { return };
        let current = match applied {
            Some(b) => b,
            None => match lighting::get_look(t, d, Store::Temporary) {
                Ok(Some(l)) => l.brightness,
                _ => return,
            },
        };
        let value = if up { current.saturating_add(26) } else { current.saturating_sub(26) };
        let shown = lighting::set_brightness(t, d, Store::Temporary, value).is_ok();
        if shown && let Some(look) = &mut self.settings.applied {
            look.brightness = value;
            let _ = settings::save(&self.settings);
        }
    }
}

/// Press point in the UI's 0.1 mm steps.
pub fn mm(threshold: u8) -> f32 {
    (analog::threshold_to_mm(threshold) * 10.0).round() / 10.0
}

/// The `touched` keys are now unsaved exactly as `unsaved` says.
fn track<V>(overrides: &mut BTreeMap<u8, V>, touched: impl IntoIterator<Item = u8>, unsaved: impl IntoIterator<Item = (u8, V)>) {
    for k in touched {
        overrides.remove(&k);
    }
    overrides.extend(unsaved);
}

/// Saving makes permanent what the keyboard types with now, plus the pending edits.
fn to_save<V: Copy>(overrides: &BTreeMap<u8, V>, edits: &[(u8, V)]) -> Vec<(u8, V)> {
    let mut all = overrides.clone();
    all.extend(edits.iter().copied());
    all.into_iter().collect()
}

fn pairs<V: Copy>(m: &BTreeMap<u8, V>) -> Vec<(u8, V)> {
    m.iter().map(|(&k, &v)| (k, v)).collect()
}

fn saved(results: &[(u8, Outcome)]) -> impl Iterator<Item = u8> + '_ {
    results.iter().filter(|(_, o)| matches!(o, Outcome::Ok(_))).map(|&(k, _)| k)
}

fn written_macros(s: &Settings) -> Vec<u16> {
    s.macros.iter().filter(|(_, m)| m.written).map(|(&id, _)| id).collect()
}

/// Library macros the bindings play that the flash does not hold yet.
fn bodies_to_write(s: &Settings, bindings: &[(u8, Action)]) -> Vec<(u16, Vec<u8>)> {
    let mut ids: Vec<u16> = bindings.iter().filter_map(|&(_, a)| if let Action::Macro { id, .. } = a { Some(id) } else { None }).collect();
    ids.sort_unstable();
    ids.dedup();
    ids.into_iter()
        .filter_map(|id| s.macros.get(&id).filter(|m| !m.written).and_then(|m| Some((id, macros::encode(&m.events)?))))
        .collect()
}

/// Ids from `0x8000` up, away from the small ones Synapse is likely to take.
fn next_macro_id(library: &BTreeMap<u16, Macro>) -> u16 {
    library.keys().next_back().map_or(0x8000, |&id| id.max(0x7FFF) + 1)
}

/// Driver mode: nothing goes to the keyboard, the engine takes the binding as it is.
fn host_bound(key: u8, a: Action) -> Outcome {
    match binding::encode(a) {
        Some((fn_id, fn_data)) => Outcome::Ok(KeyAssignment { profile: actuation::LIVE, key, layer: 0, threshold_low: 0, threshold_high: 0, fn_id, fn_data }),
        None => Outcome::Failed(Error::BadArgument(format!("cannot bind {a:?}"))),
    }
}

fn restore_error(lang: &str, message: &str, results: &[(u8, Outcome)]) -> Option<String> {
    let bad: Vec<&str> = results
        .iter()
        .filter(|(_, o)| !matches!(o, Outcome::Ok(_)))
        .map(|&(k, _)| keymap::name(k).unwrap_or("?"))
        .collect();
    (!bad.is_empty()).then(|| i18n::tf(lang, message, &[("keys", &bad.join(", "))]))
}

fn editable_keys() -> Vec<u8> {
    layout::keys().into_iter().filter(|k| k.editable).map(|k| k.key).collect()
}

fn write_backup(text: &str) -> Result<(), String> {
    let appdata = std::env::var("APPDATA").map_err(|e| e.to_string())?;
    let dir = PathBuf::from(appdata).join("EasyRazer");
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let secs = SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |d| d.as_secs());
    std::fs::write(dir.join(format!("backup-{secs}.txt")), text).map_err(|e| e.to_string())
}

/// Synapse leaves the keyboard in driver mode, where it types nothing on its own.
/// Take it back only right after confirming Synapse is gone, never on a stale answer:
/// otherwise we would fight a Synapse that has just started. Our own engine keeps it.
fn should_release_driver_mode(fresh_check: bool, synapse: bool, mode: Option<u8>, engine: bool) -> bool {
    fresh_check && !synapse && !engine && mode == Some(control::MODE_DRIVER)
}

fn wants_engine(synapse: bool, driver_mode: bool) -> bool {
    !synapse && driver_mode
}

fn engine_config(
    thresholds: &BTreeMap<u8, u8>,
    bindings: &BTreeMap<u8, Option<Action>>,
    rapid: &BTreeMap<u8, Rapid>,
    (repeat_delay, repeat_interval): (Duration, Duration),
) -> Config {
    let mut c = Config { act: [0; 256], rapid: [None; 256], bind: [None; 256], repeat_delay, repeat_interval, macros: BTreeMap::new() };
    for (&k, &thr) in thresholds {
        c.act[k as usize] = thr;
    }
    for (&k, &a) in bindings {
        c.bind[k as usize] = a;
    }
    for (&k, r) in rapid.iter().filter(|(_, r)| r.enabled) {
        c.rapid[k as usize] = Some(Trigger { press: rapid::mm_to_depth(r.press), release: rapid::mm_to_depth(r.release) });
    }
    c
}

/// Last resort when the process is going down: a keyboard left in driver mode types nothing.
pub fn release_keyboard() {
    if let Ok(api) = HidApi::new()
        && let Ok(Some((t, _))) = hid::open_control(&api)
    {
        let _ = control::set_hardware_mode(&t);
    }
}

/// Synapse overwrites the temporary store; once it is gone, bring the applied look back.
fn should_restore(was_running: bool, fresh_synapse: Option<bool>) -> bool {
    was_running && fresh_synapse == Some(false)
}

/// Settings from an older description may name effects that no longer exist.
fn usable(d: &DeviceSpec, look: Option<Look>) -> Option<Look> {
    look.filter(|l| lighting::encode(d, &l.effect).is_ok())
}

/// With watching off Synapse is taken as absent, so the lighting is restored if it was blocked before.
pub fn synapse_check(watch: bool, running: impl FnOnce() -> bool) -> bool {
    watch && running()
}

pub fn synapse_running() -> bool {
    let out = Command::new("tasklist")
        .args(["/FI", "IMAGENAME eq RazerAppEngine.exe", "/NH"])
        .creation_flags(CREATE_NO_WINDOW)
        .output();
    synapse_in(out.ok())
}

/// A failed check counts as "running": writing while Synapse is alive gets overwritten.
/// With no match tasklist still prints a (localized) notice and exits 0, so empty output is a failure too.
fn synapse_in(tasklist: Option<Output>) -> bool {
    tasklist.filter(|o| o.status.success() && !o.stdout.is_empty()).is_none_or(|o| {
        String::from_utf8_lossy(&o.stdout).contains("RazerAppEngine")
    })
}

#[cfg(test)]
mod tests {
    use razer_core::lighting::Effect;
    use super::*;

    use std::os::windows::process::ExitStatusExt;
    use std::process::ExitStatus;

    fn tasklist(code: u32, stdout: &[u8]) -> Option<Output> {
        Some(Output { status: ExitStatus::from_raw(code), stdout: stdout.to_vec(), stderr: Vec::new() })
    }

    #[test]
    fn synapse_is_not_checked_when_watching_is_off() {
        let mut ran = false;
        assert!(!synapse_check(false, || {
            ran = true;
            true
        }));
        assert!(!ran);
        assert!(synapse_check(true, || true));
    }

    #[test]
    fn writes_skip_the_synapse_check_when_watching_is_off() {
        let settings = Settings { watch_synapse: false, ..Default::default() };
        let api = HidApi::new().unwrap();
        let mut d = Device { api, control: None, synapse: true, backed_up: false, settings, last_spec: None, restore_error: None, engine: None, live: None, sink: None };
        assert!(!d.check_synapse(|| true));
        assert!(!d.synapse);
    }

    #[test]
    fn tasklist_failure_counts_as_synapse_running() {
        assert!(synapse_in(None));
        assert!(synapse_in(tasklist(1, b"")));
        assert!(synapse_in(tasklist(0, b"")));
    }

    #[test]
    fn detects_synapse_in_tasklist_output() {
        assert!(synapse_in(tasklist(0, b"RazerAppEngine.exe  1234 Console  1  80 000 K")));
        assert!(!synapse_in(tasklist(0, b"INFO: No tasks are running which match the specified criteria.")));
    }

    #[test]
    fn driver_mode_is_released_only_right_after_a_fresh_check() {
        assert!(should_release_driver_mode(true, false, Some(control::MODE_DRIVER), false));
        assert!(!should_release_driver_mode(false, false, Some(control::MODE_DRIVER), false));
        assert!(!should_release_driver_mode(true, true, Some(control::MODE_DRIVER), false));
        assert!(!should_release_driver_mode(true, false, Some(control::MODE_DRIVER), true));
    }

    #[test]
    fn engine_runs_in_driver_mode_while_synapse_is_away() {
        assert!(wants_engine(false, true));
        assert!(!wants_engine(true, true));
        assert!(!wants_engine(false, false));
    }

    #[test]
    fn engine_config_from_press_points_and_rapid_trigger() {
        let thresholds = BTreeMap::from([(31, 43), (33, 0)]);
        let rapid = BTreeMap::from([
            (31, Rapid { enabled: true, press: 0.4, release: 0.1 }),
            (33, Rapid { enabled: false, press: 0.4, release: 0.4 }),
        ]);
        let t = (Duration::from_millis(500), Duration::from_millis(33));
        let bindings = BTreeMap::from([(31, Some(Action::Disabled)), (33, None)]);
        let c = engine_config(&thresholds, &bindings, &rapid, t);
        assert_eq!((c.act[31], c.act[33], c.act[18]), (43, 0, 0));
        assert_eq!((c.bind[31], c.bind[33]), (Some(Action::Disabled), None));
        assert_eq!(c.rapid[31], Some(Trigger { press: 49, release: 12 }));
        assert_eq!(c.rapid[33], None);
        assert_eq!((c.repeat_delay, c.repeat_interval), t);
    }

    #[test]
    fn applied_look_is_restored_once_synapse_is_gone() {
        assert!(should_restore(true, Some(false)));
        assert!(!should_restore(true, Some(true)));
        assert!(!should_restore(false, Some(false)));
        assert!(!should_restore(true, None));
    }

    #[test]
    fn applied_look_the_description_cannot_encode_is_dropped() {
        let d = razer_core::devices::by_pid(0x0266).unwrap();
        let stale = Look { effect: Effect { name: "reactive".into(), speed: Some(2), ..Default::default() }, brightness: 9 };
        assert_eq!(usable(d, Some(stale)), None);
        let ok = Look { effect: Effect { name: "spectrum".into(), ..Default::default() }, brightness: 9 };
        assert_eq!(usable(d, Some(ok.clone())), Some(ok));
    }

    use razer_core::{analog, keymap};

    #[test]
    #[ignore = "needs the keyboard connected and Synapse closed"]
    fn temporary_apply_round_trip_on_hardware() {
        assert!(!synapse_running(), "close Synapse first");
        let mut dev = Device::new().unwrap();
        let a = keymap::by_name("A").unwrap();
        let (profile, saved, live) = {
            let (t, _) = dev.connect().expect("keyboard not connected");
            let profile = control::active_profile(t).unwrap();
            (profile, actuation::read_key(t, profile, a).unwrap(), actuation::read_key(t, actuation::LIVE, a).unwrap())
        };
        assert!(!dev.settings.actuation.contains_key(&a), "A is applied in the app; save or revert it first");
        let w = dev.apply(&[(a, 2.4)], &BTreeMap::new(), &[]).unwrap();
        let (t, _) = dev.connect().unwrap();
        actuation::write_key(t, &live).unwrap();
        let profile_after = actuation::read_key(t, profile, a).unwrap();
        dev.update_settings(|s| {
            s.actuation.remove(&a);
        })
        .unwrap();
        assert!(matches!(&w.results[0].1, Outcome::Ok(s) if s.threshold_low == analog::mm_to_threshold(2.4)), "{:?}", w.results);
        assert_eq!(w.unsaved, [a]);
        assert_eq!(profile_after, saved);
    }


    #[test]
    #[ignore = "needs the keyboard connected and Synapse closed"]
    fn lighting_preview_round_trip_on_hardware() {
        assert!(!synapse_running(), "close Synapse first");
        let mut dev = Device::new().unwrap();
        dev.status(Some(false));
        let (original, saved_before) = {
            let (t, d) = dev.connect().expect("keyboard not connected");
            let original = lighting::get_look(t, d, Store::Temporary).unwrap().expect("known effect in the temporary store");
            (original, lighting::get_look(t, d, Store::Saved).unwrap())
        };
        let test = Look { effect: Effect { name: "static".into(), rgb1: Some([0x12, 0x34, 0x56]), ..Default::default() }, brightness: 0x80 };
        dev.lighting_preview(&test).unwrap();
        let (t, d) = dev.connect().unwrap();
        let read = lighting::get_look(t, d, Store::Temporary).unwrap();
        lighting::set_look(t, d, Store::Temporary, &original).unwrap();
        assert_eq!(read, Some(test));
        assert_eq!(lighting::get_look(t, d, Store::Saved).unwrap(), saved_before);
    }

    #[test]
    fn saving_writes_the_bodies_of_bound_unwritten_macros() {
        let m = |written| Macro { name: String::new(), events: vec![Event::Delay { ms: 5 }], written };
        let s = Settings { macros: [(1, m(true)), (2, m(false)), (3, m(false))].into(), ..Default::default() };
        let play = |id| Action::Macro { id, mode: binding::MacroMode::Times, count: 1 };
        let b = [(31, play(1)), (32, play(2)), (33, play(2)), (18, play(9)), (19, Action::Disabled)];
        assert_eq!(bodies_to_write(&s, &b), [(2, vec![0x11, 5])]);
        assert_eq!(written_macros(&s), [1]);
    }

    #[test]
    fn macro_ids_start_high_and_grow() {
        let m = || Macro { name: String::new(), events: Vec::new(), written: false };
        assert_eq!(next_macro_id(&BTreeMap::new()), 0x8000);
        assert_eq!(next_macro_id(&[(3, m())].into()), 0x8000);
        assert_eq!(next_macro_id(&[(3, m()), (0x8004, m())].into()), 0x8005);
    }

    #[test]
    fn tracking_replaces_touched_keys_with_what_is_still_unsaved() {
        let mut o = BTreeMap::from([(31, 2.0), (32, 2.5), (18, 3.0)]);
        track(&mut o, [31, 32], [(32, 3.6)]);
        assert_eq!(o, BTreeMap::from([(32, 3.6), (18, 3.0)]));
    }

    #[test]
    fn saving_writes_applied_press_points_with_edits_on_top() {
        let o = BTreeMap::from([(31, 2.0), (32, 2.5)]);
        assert_eq!(to_save(&o, &[(32, 3.0), (18, 1.5)]), [(18, 1.5), (31, 2.0), (32, 3.0)]);
    }

    #[test]
    fn restore_reports_keys_that_were_not_applied() {
        let a = |t| KeyAssignment { profile: 0, key: 31, layer: 0, threshold_low: t, threshold_high: 0, fn_id: 2, fn_data: vec![] };
        let key = "backend.restoreActuation";
        assert_eq!(restore_error("ru", key, &[(31, Outcome::Ok(a(0)))]), None);
        let e = restore_error("ru", key, &[(31, Outcome::Ok(a(0))), (32, Outcome::Unconfirmed(a(9))), (18, Outcome::Failed(Error::Io("x".into())))]);
        assert_eq!(e.as_deref(), Some("Не удалось вернуть точки срабатывания клавиш: S, W"));
    }
}
