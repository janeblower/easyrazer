//! Owns the keyboard connection: opening, reconnecting, Synapse detection, device mode.

use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;
use std::sync::Arc;
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
use razer_core::profiles::{self as slots, Snapshot};
use razer_core::{control, layout};
use serde::Serialize;
use windows_sys::Win32::Foundation::{CloseHandle, INVALID_HANDLE_VALUE};
use windows_sys::Win32::System::Diagnostics::ToolHelp::{CreateToolhelp32Snapshot, PROCESSENTRY32W, Process32FirstW, Process32NextW, TH32CS_SNAPPROCESS};

use crate::dynamic_lighting;
use crate::engine::{self, EngineHandle, MenuListener, Sink};
use crate::i18n;
use crate::profiles;
use crate::settings::{self, Macro, Rapid, Settings};

/// Marks a key of `ram` as unknown so the next load writes it; the firmware takes no `fnId` 0xFF.
const UNKNOWN_FN: u8 = 0xFF;

#[derive(Clone, Debug, Serialize)]
pub struct Status {
    pub device: bool,
    pub synapse: bool,
    /// Driver mode chosen in the app: the host engine types, Rapid Trigger works.
    pub driver_mode: bool,
    pub profile: Option<u32>,
    pub profile_name: Option<String>,
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
    pub custom: Option<BTreeMap<u8, Rgb>>,
}

/// Profiles as (id, name, slot, unsaved), the loaded id, the startup slot, whether a slot is free.
pub type ProfileList = (Vec<(u32, String, Option<u8>, bool)>, Option<u32>, Option<u8>, bool);

pub struct Device {
    api: HidApi,
    control: Option<(HidTransport, &'static DeviceSpec)>,
    /// Last known Synapse state; assumed running until checked.
    synapse: bool,
    /// Slots backed up this session.
    backed_up: BTreeSet<u8>,
    settings: Settings,
    /// Description of the last keyboard seen, so the lighting tab can be edited while it is unplugged.
    last_spec: Option<&'static DeviceSpec>,
    restore_error: Option<String>,
    /// Runs in driver mode; the keyboard only reports depth meanwhile.
    engine: Option<EngineHandle>,
    /// Injected keys cannot reach the focused window, so the firmware types meanwhile.
    blocked: bool,
    /// What profile 0 holds; `None` once the firmware reloaded it from the startup slot.
    ram: Option<Snapshot>,
    /// The slot the keyboard starts with, as last read or set.
    startup: Option<u8>,
    /// Slots were read this connect.
    synced: bool,
    /// Reads Col04, so Fn+Menu may report the next-profile code.
    menu: Option<MenuListener>,
    sink: Option<Sink>,
    progress: Option<Progress>,
    /// The keyboard shows a look the window has not applied.
    previewed: bool,
}

/// Reports a slot read: reads done, reads in all, the key when the window shows it.
pub type Progress = Arc<dyn Fn(usize, usize, Option<&KeyAssignment>) + Send + Sync>;

impl Device {
    pub fn new() -> Result<Self, Error> {
        let api = HidApi::new().map_err(|e| Error::Io(e.to_string()))?;
        Ok(Self {
            api,
            control: None,
            synapse: true,
            backed_up: BTreeSet::new(),
            settings: settings::load(),
            last_spec: None,
            restore_error: None,
            engine: None,
            blocked: false,
            ram: None,
            startup: None,
            synced: false,
            menu: None,
            sink: None,
            progress: None,
            previewed: false,
        })
    }

    /// Reopens the control interface if the keyboard was replugged; `true` when connected.
    pub fn ensure_connected(&mut self) -> bool {
        let alive = self.control.as_ref().is_some_and(|(t, _)| control::mode(t).is_ok());
        if !alive {
            self.engine = None;
            self.menu = None;
            self.ram = None;
            self.synced = false;
            self.startup = None;
            self.control = None;
            let _ = self.api.refresh_devices();
            self.control = hid::open_control(&self.api).ok().flatten();
            if let Some((_, d)) = &self.control {
                self.last_spec = Some(*d);
                if let Some(sink) = &self.sink {
                    // Fn+Menu is rebound only once someone listens, or it would do nothing.
                    self.menu = MenuListener::start(&self.api, d.pid, sink.clone()).ok();
                }
                if !self.synapse {
                    self.reload();
                }
            }
        }
        self.control.is_some()
    }

    /// Syncs the slots if this connect has not yet, then loads the profile into profile 0.
    fn reload(&mut self) {
        let progress = self.progress.clone();
        if !self.synced
            && let Err(e) = self.sync(|n, total, a| {
                if let Some(p) = &progress {
                    p(n, total, a);
                }
            })
        {
            self.restore_error = Some(i18n::tf(self.lang(), "backend.restoreProfile", &[("error", &e)]));
            return;
        }
        self.ram = None;
        self.restore();
    }

    /// Reads the slots into the settings, importing and migrating what is not there yet.
    fn sync(&mut self, progress: impl FnMut(usize, usize, Option<&KeyAssignment>)) -> Result<(), String> {
        let keys = editable_keys();
        let lang = self.lang().to_string();
        let fallback = move |k: u8| i18n::tf(&lang, "backend.profileSlot", &[("n", &k.to_string())]);
        let missing = self.msg("backend.noKeyboard");
        let Some((t, d)) = &self.control else { return Err(missing) };
        let startup = control::active_profile(t).map_err(|e| e.to_string())?;
        // Synapse or another tool may have rewritten the startup slot since it was cached.
        if !self.synced {
            self.settings.slots.remove(&startup);
        }
        profiles::sync_slots(t, d, &mut self.settings, &keys, startup, &fallback, progress).map_err(|e| e.to_string())?;
        if let Some(id) = self.settings.loaded {
            self.settings.take_legacy(id);
        }
        self.startup = Some(startup);
        self.synced = true;
        settings::save(&self.settings)
    }

    fn hw_menu(&self) -> bool {
        self.engine.is_none() && self.menu.as_ref().is_some_and(MenuListener::alive)
    }

    /// Loads the loaded profile into profile 0: the keyboard forgets it on unplug, on a mode switch
    /// and on `05:04`, and Synapse overwrites it.
    fn restore(&mut self) {
        if !self.synced || self.synapse {
            return;
        }
        let menu = self.hw_menu();
        let Some((t, d)) = &self.control else { return };
        let Some(id) = self.settings.loaded else { return };
        let base = match (&self.ram, self.startup.and_then(|k| self.settings.slots.get(&k))) {
            (Some(r), _) => r.clone(),
            (None, Some(s)) => s.clone(),
            (None, None) => Snapshot::default(),
        };
        match profiles::load(t, d, &self.settings, id, &base, menu) {
            Ok(after) => self.ram = Some(after),
            Err(e) => {
                // Profile 0 holds a partial write, so the next load must write every key.
                self.ram = Some(Snapshot::default());
                self.restore_error = Some(i18n::tf(self.lang(), "backend.restoreProfile", &[("error", &e.to_string())]));
            }
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
        // Synapse overwrites the temporary store; once it is gone, bring the applied look back.
        if was && fresh_synapse == Some(false) {
            self.synced = false;
            self.reload();
        }
        let (profile, profile_name) = match self.settings.loaded_profile() {
            Some(p) if self.synced => (Some(p.id), Some(p.name.clone())),
            _ => (None, None),
        };
        if self.control.is_none() {
            let unsupported = hid::unsupported_keyboard(&self.api);
            let error = self.restore_error.take();
            let driver_mode = self.settings.driver_mode;
            return Status { device: false, synapse, driver_mode, profile: None, profile_name: None, model: None, unsupported, error };
        }
        self.sync_engine();
        let error = self.restore_error.take();
        let running = self.engine.is_some();
        let Some((t, spec)) = &self.control else { unreachable!() };
        let mode = control::mode(t).ok();
        let released = should_release_driver_mode(fresh_synapse.is_some(), synapse, mode, running) && control::set_hardware_mode(t).is_ok();
        let model = Some(spec.name.clone());
        if released {
            self.ram = None;
            self.restore();
        }
        Status { device: true, synapse, driver_mode: self.settings.driver_mode, profile, profile_name, model, unsupported: None, error }
    }

    /// Reads the slots once per connect and loads the profile; returns its Normal layer.
    pub fn read_all(&mut self, progress: impl FnMut(usize, usize, Option<&KeyAssignment>)) -> Result<Vec<KeyAssignment>, String> {
        if self.synapse {
            return Err(self.msg("backend.synapseRunning"));
        }
        self.sync(progress)?;
        self.restore();
        self.sync_engine();
        let missing = self.msg("backend.noKeyboard");
        let p = self.settings.loaded_profile().ok_or(missing)?;
        Ok(p.data.normal.iter().map(|(&k, r)| r.at(actuation::LIVE, analog::Layer::Normal, k)).collect())
    }

    /// What each key does now; `None` when the app cannot show it.
    pub fn bindings(&self) -> BTreeMap<u8, Option<Action>> {
        let Some(p) = self.settings.loaded_profile() else { return BTreeMap::new() };
        p.data.normal.iter().map(|(&k, r)| (k, binding::decode(r.fn_id, &r.fn_data))).collect()
    }

    /// Keys whose press point and binding differ from the profile's slot.
    pub fn unsaved(&self) -> (Vec<u8>, Vec<u8>) {
        self.settings.loaded_profile().map_or_else(Default::default, |p| profiles::unsaved(p, &self.settings.slots))
    }

    pub fn rapid(&self) -> BTreeMap<u8, Rapid> {
        self.settings.loaded_profile().map(|p| p.rapid.clone()).unwrap_or_default()
    }

    /// Id of the loaded profile.
    pub fn loaded(&self) -> Option<u32> {
        self.settings.loaded
    }

    pub fn load_profile(&mut self, id: u32) -> Result<(), String> {
        if self.settings.profile(id).is_none() {
            return Err(format!("no profile {id}"));
        }
        self.settings.loaded = Some(id);
        settings::save(&self.settings)?;
        self.restore();
        self.sync_engine();
        Ok(())
    }

    /// Fn+Menu.
    pub fn next_profile(&mut self) {
        if let Some(id) = profiles::next(&self.settings) {
            let _ = self.load_profile(id);
        }
    }

    /// The new profile is loaded at once, so the window can rename it in place.
    pub fn create_profile(&mut self) -> Result<(), String> {
        let name = profiles::free_name(&self.settings, &self.msg("profiles.new"), profiles::numbered);
        let id = profiles::create(&mut self.settings, name).ok_or_else(|| self.msg("backend.noKeyboard"))?;
        self.load_profile(id)
    }

    pub fn duplicate_profile(&mut self, id: u32) -> Result<(), String> {
        let base = self.settings.profile(id).map(|p| p.name.clone()).ok_or_else(|| format!("no profile {id}"))?;
        let lang = self.lang().to_string();
        let copy = |n: usize| match n {
            1 => i18n::tf(&lang, "profiles.copy", &[("name", "")]),
            n => i18n::tf(&lang, "profiles.copyN", &[("name", ""), ("n", &n.to_string())]),
        };
        let name = profiles::free_name(&self.settings, &base, copy);
        profiles::duplicate(&mut self.settings, id, name);
        settings::save(&self.settings)
    }

    pub fn rename_profile(&mut self, id: u32, name: &str) -> Result<(), String> {
        let old = self.settings.profile(id).map(|p| p.name.clone()).ok_or_else(|| format!("no profile {id}"))?;
        if slots::fit_name(name).is_empty() {
            return Err(self.msg("backend.emptyName"));
        }
        if slots::fit_name(name) == old {
            return Ok(());
        }
        if profiles::name_taken(&self.settings, &slots::fit_name(name), Some(id)) {
            return Err(self.msg("backend.nameTaken"));
        }
        let slot = profiles::rename(&mut self.settings, id, name).map_err(|e| e.to_string())?;
        let written = match slot {
            Some(k) => self.write_slot_name(id, k),
            None => Ok(()),
        };
        // The name must not outlive a failed slot write, or the next save would split the two.
        if let Err(e) = written.and_then(|()| settings::save(&self.settings)) {
            if let Some(p) = self.settings.profile_mut(id) {
                p.name = old;
            }
            return Err(e);
        }
        Ok(())
    }

    fn write_slot_name(&mut self, id: u32, slot: u8) -> Result<(), String> {
        if self.check_synapse(synapse_running) {
            return Err(self.msg("backend.synapseRunning"));
        }
        let new = self.settings.profile(id).map(|p| p.name.clone()).unwrap_or_default();
        let (t, _) = self.keyboard()?;
        slots::write_name(t, slot, &new).map_err(|e| e.to_string())
    }

    /// Ops that change the flash list; every one of them may reload profile 0, `reloads` says it always does.
    fn slot_op(&mut self, reloads: bool, op: impl FnOnce(&HidTransport, &mut Settings, u8) -> Result<u8, Error>) -> Result<(), String> {
        if self.check_synapse(synapse_running) {
            return Err(self.msg("backend.synapseRunning"));
        }
        let startup = self.startup.ok_or_else(|| self.msg("backend.noKeyboard"))?;
        let missing = self.msg("backend.noKeyboard");
        let Some((t, _)) = &self.control else { return Err(missing) };
        let after = match op(t, &mut self.settings, startup) {
            Ok(a) => a,
            Err(e) => {
                // A half-done `05:04`/`05:03` leaves the keyboard in an unknown state.
                self.synced = false;
                self.ram = None;
                return Err(e.to_string());
            }
        };
        if reloads || after != startup {
            self.ram = None;
        }
        self.startup = Some(after);
        settings::save(&self.settings)?;
        self.restore();
        self.sync_engine();
        Ok(())
    }

    pub fn delete_profile(&mut self, id: u32) -> Result<(), String> {
        let slotted = self.settings.profile(id).ok_or_else(|| format!("no profile {id}"))?.slot.is_some();
        if self.settings.profiles.len() <= 1 {
            return Err(self.msg("backend.onlyProfile"));
        }
        if slotted && self.slots_used() <= 1 {
            return Err(self.msg("backend.lastSlot"));
        }
        if !slotted {
            let was_loaded = self.settings.loaded == Some(id);
            profiles::remove(&mut self.settings, id).map_err(|e| e.to_string())?;
            settings::save(&self.settings)?;
            if was_loaded {
                self.restore();
                self.sync_engine();
            }
            return Ok(());
        }
        self.slot_op(false, |t, s, startup| profiles::delete(t, s, id, startup))
    }

    pub fn free_slot(&mut self, id: u32) -> Result<(), String> {
        if self.slots_used() <= 1 {
            return Err(self.msg("backend.lastSlot"));
        }
        self.slot_op(false, |t, s, startup| profiles::free_slot(t, s, id, startup))
    }

    /// One erase of the keyboard's settings page.
    pub fn set_startup(&mut self, id: u32) -> Result<(), String> {
        // `05:04` reloads profile 0 even when the slot was already the startup one.
        self.slot_op(true, |t, s, _| profiles::set_startup(t, s, id))
    }

    /// Before the app exits: Fn+Menu goes back to the loaded profile's own binding.
    pub fn shutdown(&mut self) {
        self.stop_engine();
        if !self.hw_menu() || self.synapse {
            return;
        }
        self.menu = None;
        self.restore();
    }

    fn slots_used(&self) -> usize {
        self.settings.profiles.iter().filter(|p| p.slot.is_some()).count()
    }

    pub fn profiles_view(&self) -> ProfileList {
        let s = &self.settings;
        let list = s.profiles.iter().map(|p| (p.id, p.name.clone(), p.slot, profiles::is_unsaved(p, &s.slots))).collect();
        (list, s.loaded, self.startup, self.slots_used() < slots::MAX_SLOTS as usize)
    }

    /// Applies press points and bindings to the loaded profile and profile 0 until the next replug,
    /// and Rapid Trigger, which lives only here.
    pub fn apply(&mut self, changes: &[(u8, f32)], rapid: &BTreeMap<u8, Rapid>, bindings: &[(u8, Action)]) -> Result<Written, String> {
        if self.check_synapse(synapse_running) {
            return Err(self.msg("backend.synapseRunning"));
        }
        let host = self.engine.is_some();
        let unwritten = self.msg("backend.macroUnwritten");
        let missing = self.msg("backend.noKeyboard");
        if self.settings.loaded_profile().is_none() {
            return Err(missing);
        }
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
        let results = actuation::apply(t, actuation::LIVE, changes);
        let Some(p) = self.settings.loaded_mut() else { return Err(missing) };
        for (k, o) in &results {
            if let (Outcome::Ok(a) | Outcome::Unconfirmed(a), Some(r)) = (o, p.data.normal.get_mut(k)) {
                (r.thr_low, r.thr_high) = (a.threshold_low, a.threshold_high);
            }
        }
        for (k, o) in &bound {
            if let (Outcome::Ok(a) | Outcome::Unconfirmed(a), Some(r)) = (o, p.data.normal.get_mut(k)) {
                (r.fn_id, r.fn_data) = (a.fn_id, a.fn_data.clone());
            }
        }
        p.rapid.extend(rapid.iter().map(|(&k, &r)| (k, r)));
        if let Some(ram) = &mut self.ram {
            for (k, o) in &results {
                if let (Outcome::Ok(a) | Outcome::Unconfirmed(a), Some(r)) = (o, ram.normal.get_mut(k)) {
                    (r.thr_low, r.thr_high) = (a.threshold_low, a.threshold_high);
                }
            }
            // In driver mode bindings never reach profile 0: the engine plays them.
            if !host {
                for (k, o) in &bound {
                    if let (Outcome::Ok(a) | Outcome::Unconfirmed(a), Some(r)) = (o, ram.normal.get_mut(k)) {
                        (r.fn_id, r.fn_data) = (a.fn_id, a.fn_data.clone());
                    }
                }
                // A Normal write to profile 0 overwrote the key's Hypershift, Fn+Menu's code among them.
                for (k, _) in results.iter().chain(&bound) {
                    if let Some(h) = ram.hypershift.get_mut(k) {
                        h.fn_id = UNKNOWN_FN;
                    }
                }
            }
        }
        settings::save(&self.settings)?;
        if !host && !(results.is_empty() && bound.is_empty()) {
            self.restore();
        }
        self.sync_engine();
        Ok(self.written(results, bound))
    }

    /// Applies the edits, then writes the loaded profile to its slot.
    pub fn save(&mut self, edits: &[(u8, f32)], bind_edits: &[(u8, Action)]) -> Result<Written, String> {
        let applied = if edits.is_empty() && bind_edits.is_empty() {
            self.written(Vec::new(), Vec::new())
        } else {
            self.apply(edits, &BTreeMap::new(), bind_edits)?
        };
        let id = self.settings.loaded.ok_or_else(|| self.msg("backend.noKeyboard"))?;
        self.write_profile(id)?;
        Ok(Written { results: applied.results, bindings: applied.bindings, ..self.written(Vec::new(), Vec::new()) })
    }

    /// Writes the profile to its slot, a free one if it has none; backs each slot up before its first write.
    pub fn write_profile(&mut self, id: u32) -> Result<(), String> {
        if self.check_synapse(synapse_running) {
            return Err(self.msg("backend.synapseRunning"));
        }
        if self.settings.profile(id).is_some_and(|p| p.slot.is_none()) && self.slots_used() >= slots::MAX_SLOTS as usize {
            return Err(self.msg("backend.noFreeSlot"));
        }
        let keys = editable_keys();
        let bound: Vec<(u8, Action)> = self
            .settings
            .profile(id)
            .map(|p| p.data.normal.iter().filter_map(|(&k, r)| Some((k, binding::decode(r.fn_id, &r.fn_data)?))).collect())
            .unwrap_or_default();
        let bodies = bodies_to_write(&self.settings, &bound);
        let slot = self.settings.profile(id).and_then(|p| p.slot);
        let missing = self.msg("backend.noKeyboard");
        self.ensure_connected();
        // Field borrows, not `keyboard()`: the slot write needs `self.settings` mutably at the same time.
        let Some((t, d)) = &self.control else { return Err(missing) };
        if let Some(k) = slot.filter(|k| !self.backed_up.contains(k)) {
            let all = actuation::read_all(t, k, &keys, |_, _| {}).map_err(|e| e.to_string())?;
            write_backup(&actuation::format_backup(k, &all))?;
            self.backed_up.insert(k);
        }
        for (mid, body) in &bodies {
            macros::write(t, *mid, body).map_err(|e| e.to_string())?;
        }
        let k = profiles::write_slot(t, d, &mut self.settings, id, &keys).map_err(|e| e.to_string())?;
        for (mid, _) in bodies {
            if let Some(m) = self.settings.macros.get_mut(&mid) {
                m.written = true;
            }
        }
        // The firmware copies a write to the startup slot into profile 0, whichever profile it was.
        if Some(k) == self.startup {
            self.ram = None;
            self.restore();
        }
        settings::save(&self.settings)
    }

    fn written(&self, results: Vec<(u8, Outcome)>, bindings: Vec<(u8, Outcome)>) -> Written {
        let (unsaved, unsaved_bindings) = self.unsaved();
        Written { results, unsaved, bindings, unsaved_bindings }
    }

    pub fn lighting_state(&mut self) -> Result<LightingState, String> {
        let dynamic_lighting = dynamic_lighting::enabled();
        self.ensure_connected();
        let p = self.settings.loaded_profile();
        let saved = p.and_then(|p| p.slot).and_then(|k| self.settings.slots.get(&k)).and_then(|s| s.look.clone());
        let applied = p.and_then(|p| p.data.look.clone());
        Ok(LightingState {
            effects: self.last_spec.map(lighting::effect_infos).unwrap_or_default(),
            applied: match self.last_spec {
                Some(d) => usable(d, applied),
                None => applied,
            },
            saved,
            dynamic_lighting,
            custom: p.and_then(|p| p.custom.clone()),
        })
    }

    pub fn lighting_preview(&mut self, look: &Look) -> Result<(), String> {
        if self.synapse {
            return Err(self.msg("backend.synapseRunning"));
        }
        self.previewed = true;
        let (t, d) = self.keyboard()?;
        lighting::set_look(t, d, Store::Temporary, look).map_err(|e| e.to_string())
    }

    /// Brings back the look the lighting tab reverts to, once the window has dropped its preview.
    pub fn drop_preview(&mut self) -> Result<(), String> {
        if !std::mem::take(&mut self.previewed) {
            return Ok(());
        }
        let state = self.lighting_state()?;
        let Some(look) = state.applied.or(state.saved) else { return Ok(()) };
        let (t, d) = self.keyboard()?;
        lighting::set_look(t, d, Store::Temporary, &look).map_err(|e| e.to_string())
    }

    /// Remembers the look in the loaded profile; without a keyboard it is shown on the next connect.
    pub fn lighting_apply(&mut self, look: Look) -> Result<(), String> {
        if self.synapse {
            return Err(self.msg("backend.synapseRunning"));
        }
        let missing = self.msg("backend.noKeyboard");
        if self.settings.loaded_profile().is_none() {
            return Err(missing);
        }
        self.previewed = false;
        if let Some((t, d)) = self.connect() {
            lighting::set_look(t, d, Store::Temporary, &look).map_err(|e| e.to_string())?;
        }
        if let Some(r) = &mut self.ram {
            r.look = Some(look.clone());
        }
        let p = self.settings.loaded_mut().ok_or(missing)?;
        if look.effect.name == lighting::CUSTOM {
            p.custom = look.effect.colors.clone();
        }
        p.data.look = Some(look);
        settings::save(&self.settings)
    }

    /// Writes the look into the profile's slot.
    pub fn lighting_write(&mut self, look: Look) -> Result<(), String> {
        if look.effect.name == lighting::CUSTOM {
            return Err(self.msg("backend.customNotSaved"));
        }
        self.lighting_apply(look)?;
        let id = self.settings.loaded.ok_or_else(|| self.msg("backend.noKeyboard"))?;
        self.write_profile(id)
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
        if let (Some((t, _)), Some(look)) = (&self.control, self.settings.loaded_profile().and_then(|p| p.data.look.as_ref())) {
            lighting::set_look(t, d, Store::Temporary, look).map_err(|e| e.to_string())?;
        }
        Ok(())
    }

    pub fn set_sink(&mut self, sink: Sink) {
        self.sink = Some(sink);
    }

    pub fn set_progress(&mut self, progress: Progress) {
        self.progress = Some(progress);
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

    /// Falls back to hardware mode while the focused window cannot take injected keys.
    pub fn set_input_blocked(&mut self, blocked: bool) {
        if self.blocked != blocked {
            self.blocked = blocked;
            self.sync_engine();
        }
    }

    /// Runs the engine, and driver mode with it, while the app is in driver mode, Synapse is away
    /// and injected keys reach the focused window.
    fn sync_engine(&mut self) {
        if self.engine.as_ref().is_some_and(|e| !e.alive()) {
            self.engine = None;
        }
        if self.synapse || self.blocked || !self.settings.driver_mode || self.control.is_none() {
            self.stop_engine();
            return;
        }
        let Some(p) = self.settings.loaded_profile() else { return };
        let thresholds = p.data.normal.iter().map(|(&k, r)| (k, r.thr_low)).collect();
        let mut cfg = engine_config(&thresholds, &self.bindings(), &p.rapid, engine::repeat_timing());
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
                self.ram = None;
                self.restore();
            }
            Err(e) => self.restore_error = Some(i18n::tf(self.lang(), "backend.engine", &[("error", &e)])),
        }
    }

    /// Stops the engine and hands typing back to the firmware, unless Synapse owns the keyboard now.
    pub fn stop_engine(&mut self) {
        let Some(engine) = self.engine.take() else { return };
        // Keys go up before the firmware reports the held ones down. The readers take up to a read
        // timeout to exit, so the firmware types meanwhile.
        engine.halt();
        if !self.synapse
            && let Some((t, _)) = &self.control
            && control::set_hardware_mode(t).is_ok()
        {
            self.ram = None;
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
        let applied = self.settings.loaded_profile().and_then(|p| p.data.look.as_ref()).map(|l| l.brightness);
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
        if shown && let Some(look) = self.settings.loaded_mut().and_then(|p| p.data.look.as_mut()) {
            look.brightness = value;
            let _ = settings::save(&self.settings);
        }
    }
}

/// Press point in the UI's 0.1 mm steps.
pub fn mm(threshold: u8) -> f32 {
    (analog::threshold_to_mm(threshold) * 10.0).round() / 10.0
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

/// Settings from an older description may name effects that no longer exist.
fn usable(d: &DeviceSpec, look: Option<Look>) -> Option<Look> {
    look.filter(|l| lighting::encode(d, &l.effect).is_ok())
}

/// With watching off Synapse is taken as absent, so the lighting is restored if it was blocked before.
pub fn synapse_check(watch: bool, running: impl FnOnce() -> bool) -> bool {
    watch && running()
}

/// A failed snapshot counts as "running": writing while Synapse is alive gets overwritten.
pub fn synapse_running() -> bool {
    unsafe {
        let snap = CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0);
        if snap == INVALID_HANDLE_VALUE {
            return true;
        }
        let mut entry: PROCESSENTRY32W = std::mem::zeroed();
        entry.dwSize = std::mem::size_of::<PROCESSENTRY32W>() as u32;
        let mut found = false;
        let mut ok = Process32FirstW(snap, &mut entry);
        while ok != 0 && !found {
            let len = entry.szExeFile.iter().position(|&c| c == 0).unwrap_or(entry.szExeFile.len());
            found = String::from_utf16_lossy(&entry.szExeFile[..len]).eq_ignore_ascii_case("RazerAppEngine.exe");
            ok = Process32NextW(snap, &mut entry);
        }
        CloseHandle(snap);
        found
    }
}

#[cfg(test)]
mod tests {
    use razer_core::lighting::Effect;
    use super::*;

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
        let mut d = Device { api, control: None, synapse: true, backed_up: BTreeSet::new(), settings, last_spec: None, restore_error: None, engine: None, blocked: false, ram: None, startup: None, synced: false, menu: None, sink: None, progress: None, previewed: false };
        assert!(!d.check_synapse(|| true));
        assert!(!d.synapse);
    }

    #[test]
    fn driver_mode_is_released_only_right_after_a_fresh_check() {
        assert!(should_release_driver_mode(true, false, Some(control::MODE_DRIVER), false));
        assert!(!should_release_driver_mode(false, false, Some(control::MODE_DRIVER), false));
        assert!(!should_release_driver_mode(true, true, Some(control::MODE_DRIVER), false));
        assert!(!should_release_driver_mode(true, false, Some(control::MODE_DRIVER), true));
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
        dev.read_all(|_, _, _| {}).unwrap();
        let w = dev.apply(&[(a, 2.4)], &BTreeMap::new(), &[]).unwrap();
        let (t, _) = dev.connect().unwrap();
        let profile_after = actuation::read_key(t, profile, a).unwrap();
        dev.apply(&[(a, mm(live.threshold_low))], &BTreeMap::new(), &[]).unwrap();
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
            (original, lighting::get_look(t, d, Store::Slot(1)).unwrap())
        };
        let test = Look { effect: Effect { name: "static".into(), rgb1: Some([0x12, 0x34, 0x56]), ..Default::default() }, brightness: 0x80 };
        dev.lighting_preview(&test).unwrap();
        let (t, d) = dev.connect().unwrap();
        let read = lighting::get_look(t, d, Store::Temporary).unwrap();
        lighting::set_look(t, d, Store::Temporary, &original).unwrap();
        assert_eq!(read, Some(test));
        assert_eq!(lighting::get_look(t, d, Store::Slot(1)).unwrap(), saved_before);
    }

    #[test]
    #[ignore = "needs the keyboard connected and Synapse closed; erases flash pages"]
    fn profile_slot_round_trip_on_hardware() {
        assert!(!synapse_running(), "close Synapse first");
        let mut dev = Device::new().unwrap();
        dev.read_all(|_, _, _| {}).unwrap();
        let before = dev.settings.profiles.len();
        dev.create_profile().unwrap();
        let id = dev.settings.profiles.iter().find(|p| p.slot.is_none()).map(|p| p.id).unwrap();
        dev.write_profile(id).unwrap();
        let p = dev.settings.profile(id).unwrap().clone();
        let k = p.slot.expect("written to a slot");
        let (t, d) = dev.connect().unwrap();
        let read = slots::read_snapshot(t, d, k, &editable_keys(), |_, _| {}).unwrap();
        assert!(p.data.matches(&read));
        assert_eq!(slots::read_name(t, k).unwrap(), p.name);
        dev.delete_profile(id).unwrap();
        let (t, _) = dev.connect().unwrap();
        assert!(!slots::list(t).unwrap().contains(&k));
        assert_eq!(dev.settings.profiles.len(), before);
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
}
