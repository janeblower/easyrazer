//! The only surface a window can reach: requests are methods, everything else arrives as [`Event`]s.

use std::collections::{BTreeMap, BTreeSet};
use std::panic::AssertUnwindSafe;
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Arc, Mutex, MutexGuard, OnceLock};
use std::time::Duration;

use razer_core::actuation::Outcome;
use razer_core::analog::KeyAssignment;
use razer_core::binding::{self, Action};
use razer_core::control::{LED_GAME, LED_MACRO};
use razer_core::devices;
use razer_core::keymap;
use razer_core::layout::{self as kb_layout, Board};
use razer_core::lighting::Look;
use razer_core::logged::Entry;
use razer_core::macros;
use razer_core::rapid::Output;
use serde::{Deserialize, Serialize};

use crate::boards::{self, Boards, DevicesView, Hooks};
use crate::device::{self, Device, Hypershift, HypershiftEdits, LightingState, MacroState, ProfilesView, Status, Written, mm};
use crate::engine::{self, Sink};
use crate::settings::{self, CloseAction, DevPanel, DevWindow, Rapid, Settings, Shared, SnapTap};
use crate::{autostart, dev, focus, gamepad, i18n, wake};

/// What the backend tells the window without being asked.
#[derive(Clone, Debug, Serialize)]
pub enum Event {
    Status(Status),
    /// The keyboard list or the picked keyboard changed.
    Devices,
    KeyDepth { id: String, key: u8, depth: u8, travel: u8, down: bool },
    /// A slot is being read; `key` is the press point just read.
    ReadProgress { id: String, done: usize, total: usize, key: Option<(u8, f32)> },
    HidLog { id: String, entry: Entry },
    SettingsChanged,
    LanguageChanged,
    SnapTap { id: String, on: bool },
    GamepadEnabled { id: String, profile: u32, enabled: bool },
    Error(String),
}

pub struct Backend {
    pub settings: Shared,
    pub boards: Boards,
    /// Set in `start`, before the watcher runs.
    hooks: OnceLock<Hooks>,
    /// Lets the watcher connect; taken once the window listens to the read progress.
    window_ready: Mutex<Option<Sender<()>>>,
    ready: Mutex<Option<Receiver<()>>>,
    events: Sender<Event>,
}

/// A command that panicked mid-way leaves the device in a usable state, so poisoning is ignored.
pub fn lock(d: &Mutex<Device>) -> MutexGuard<'_, Device> {
    d.lock().unwrap_or_else(|e| e.into_inner())
}

#[derive(Clone, Debug, Serialize)]
pub struct KeyView {
    /// Key name from `keymap`, empty for lighting zones.
    pub name: &'static str,
    #[serde(flatten)]
    pub key: kb_layout::LayoutKey,
}

fn key_views(keys: Vec<kb_layout::LayoutKey>) -> Vec<KeyView> {
    keys.into_iter().map(|key| KeyView { name: keymap::name(key.key).unwrap_or(""), key }).collect()
}

#[derive(Serialize)]
#[serde(tag = "status", rename_all = "lowercase")]
pub enum ApplyResult {
    Ok { key: u8, mm: f32 },
    Unconfirmed { key: u8, mm: f32 },
    Error { key: u8, message: String },
}

#[derive(Serialize)]
#[serde(tag = "status", rename_all = "lowercase")]
pub enum BindResult {
    Ok { key: u8, action: Option<Action> },
    Unconfirmed { key: u8, action: Option<Action> },
    Error { key: u8, message: String },
}

#[derive(Serialize)]
pub struct Actuation {
    pub values: BTreeMap<u8, f32>,
    /// Keys whose press point a replug would reset.
    pub unsaved: Vec<u8>,
    /// Rapid Trigger per key, from the app's settings.
    pub rapid: BTreeMap<u8, Rapid>,
    /// Snap Tap of the loaded profile.
    pub snap: SnapTap,
    /// Firmware continuous Rapid Trigger of the loaded profile.
    pub continuous: bool,
    /// `null` for a binding the app cannot show, such as the Fn key.
    pub bindings: BTreeMap<u8, Option<Action>>,
    pub unsaved_bindings: Vec<u8>,
    pub hypershift: Hypershift,
    /// Id of the loaded profile the table belongs to.
    pub profile: Option<u32>,
}

/// Edits sent to the keyboard without saving them to a slot.
pub struct Edits {
    pub changes: Vec<(u8, f32)>,
    pub rapid: BTreeMap<u8, Rapid>,
    pub snap: Option<SnapTap>,
    pub continuous: Option<bool>,
    pub bindings: Vec<(u8, Action)>,
    pub hypershift: HypershiftEdits,
}

#[derive(Serialize)]
pub struct WriteResult {
    pub results: Vec<ApplyResult>,
    pub unsaved: Vec<u8>,
    pub bindings: Vec<BindResult>,
    pub unsaved_bindings: Vec<u8>,
    pub hypershift: Hypershift,
}

impl From<Written> for WriteResult {
    fn from(w: Written) -> Self {
        let results = w
            .results
            .into_iter()
            .map(|(key, o)| match o {
                Outcome::Ok(a) => ApplyResult::Ok { key, mm: mm(w.scale, a.threshold_low) },
                Outcome::Unconfirmed(a) => ApplyResult::Unconfirmed { key, mm: mm(w.scale, a.threshold_low) },
                Outcome::Failed(e) => ApplyResult::Error { key, message: e.to_string() },
            })
            .collect();
        let action = |a: KeyAssignment| binding::decode(a.fn_id, &a.fn_data);
        let bindings = w
            .bindings
            .into_iter()
            .map(|(key, o)| match o {
                Outcome::Ok(a) => BindResult::Ok { key, action: action(a) },
                Outcome::Unconfirmed(a) => BindResult::Unconfirmed { key, action: action(a) },
                Outcome::Failed(e) => BindResult::Error { key, message: e.to_string() },
            })
            .collect();
        Self { results, unsaved: w.unsaved, bindings, unsaved_bindings: w.unsaved_bindings, hypershift: w.hypershift }
    }
}

#[derive(Serialize)]
pub struct AppSettings {
    pub autostart: bool,
    pub close_action: CloseAction,
    pub watch_synapse: bool,
    pub confirm_write: bool,
    pub autostart_offered: bool,
    pub language: Option<String>,
    pub dev_mode: bool,
    pub dev_log_size: usize,
    pub unverified_writes: bool,
    pub dev_panel: Option<DevPanel>,
    pub dev_window: Option<DevWindow>,
}

#[derive(Clone, Copy, Debug, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Link {
    Repo,
    License,
}

#[derive(Serialize)]
pub struct ImportView {
    pub view: ProfilesView,
    pub name: String,
    pub dropped: usize,
}

/// The keys of a model's map; the full-size board for an unknown model.
pub fn layout(model: Option<&str>) -> Vec<KeyView> {
    let spec = devices::all().iter().find(|d| model == Some(d.name.as_str()));
    key_views(kb_layout::keys(spec.map_or(Board::Full, |d| d.layout)))
}

pub fn lighting_layout() -> Vec<KeyView> {
    key_views(kb_layout::lighting_keys())
}

/// Every command the developer console knows by name.
pub fn dev_commands() -> BTreeMap<String, razer_core::registry::CommandInfo> {
    razer_core::registry::all().clone()
}

/// Opens the Dynamic Lighting page of Windows Settings, where it can be switched per device.
pub fn open_dynamic_lighting_settings() -> Result<(), String> {
    open::that_detached("ms-settings:personalization-lighting").map_err(|e| e.to_string())
}

/// Opens a project page; the URLs are fixed so the UI cannot open other targets.
pub fn open_link(link: Link) -> Result<(), String> {
    open::that_detached(match link {
        Link::Repo => "https://github.com/janeblower/easyrazer",
        Link::License => "https://github.com/janeblower/easyrazer/blob/master/LICENSE",
    })
    .map_err(|e| e.to_string())
}

pub fn gamepad_driver() -> Result<gamepad::drivers::Status, String> {
    gamepad::drivers::status()
}

pub fn install_gamepad_driver(replace: bool) -> Result<(), String> {
    gamepad::drivers::install(replace)
}

pub fn set_gamepad_driver_boot(on: bool) -> Result<(), String> {
    gamepad::drivers::set_boot(on)
}

pub fn uninstall_gamepad_driver() -> Result<(), String> {
    gamepad::drivers::uninstall()
}

impl Backend {
    pub fn new() -> Result<(Arc<Self>, Receiver<Event>), String> {
        let (events, rx) = mpsc::channel();
        let (ready_tx, ready) = mpsc::channel();
        let backend = Self {
            settings: Arc::new(Mutex::new(settings::load())),
            boards: Boards::new().map_err(|e| e.to_string())?,
            hooks: OnceLock::new(),
            window_ready: Mutex::new(Some(ready_tx)),
            ready: Mutex::new(Some(ready)),
            events,
        };
        Ok((Arc::new(backend), rx))
    }

    /// Starts watching for keyboards and for the focused window. With `wait_for_window` the
    /// first connect waits for [`Backend::window_ready`], so a shown window sees the first read.
    pub fn start(self: &Arc<Self>, wait_for_window: bool) {
        let (sink_self, progress_self) = (Arc::downgrade(self), Arc::downgrade(self));
        // A connect reads the slots before the window asks for them.
        let _ = self.hooks.set(Hooks {
            sink: Arc::new(move |id: &str| sink(sink_self.clone(), id.to_string())),
            progress: Arc::new(move |id: &str| {
                let (b, id) = (progress_self.clone(), id.to_string());
                Arc::new(move |done, total, key| {
                    if let Some(b) = b.upgrade() {
                        b.progress(&id, done, total, key);
                    }
                })
            }),
        });
        self.watch_focus();
        let ready = self.ready.lock().unwrap_or_else(|e| e.into_inner()).take().filter(|_| wait_for_window);
        let me = self.clone();
        std::thread::spawn(move || me.watch(ready));
    }

    fn watch_focus(self: &Arc<Self>) {
        let blocked = focus::watch();
        let me = self.clone();
        std::thread::spawn(move || {
            while let Ok(b) = blocked.recv() {
                // A switch waits for the device lock; by then only the latest state matters.
                let b = blocked.try_iter().last().unwrap_or(b);
                let _ = std::panic::catch_unwind(AssertUnwindSafe(|| {
                    for (_, d) in me.boards.all() {
                        lock(&d).set_input_blocked(b);
                    }
                }));
            }
        });
    }

    fn watch(&self, ready: Option<Receiver<()>>) {
        let events = wake::listen();
        // The timeout keeps the keyboard restored if the window never loads.
        if let Some(ready) = ready {
            let _ = ready.recv_timeout(Duration::from_secs(10));
        }
        // A board busy when the list changed gets its listener on a later step.
        let mut unsynced = BTreeSet::new();
        let (mut woke, mut synapse_was) = (true, None);
        loop {
            // One failed step must not end the watching for the rest of the session.
            let _ = std::panic::catch_unwind(AssertUnwindSafe(|| {
                let synapse = self.settings().watch_synapse && device::synapse_running();
                let retry = wake::take_retry();
                if !woke && !retry && unsynced.is_empty() && synapse_was == Some(synapse) {
                    return;
                }
                synapse_was = Some(synapse);
                let (statuses, changed) = self.poll(synapse);
                if changed {
                    unsynced.extend(self.boards.all().into_iter().map(|(id, _)| id));
                }
                if !unsynced.is_empty() {
                    let (mode, size) = { let s = self.settings(); (s.dev_mode, s.dev_log_size) };
                    unsynced.retain(|id| {
                        let Some(d) = self.boards.get(id) else { return false };
                        let Some(dev) = boards::try_guard(&d) else { return true };
                        self.sync_listener(mode, size, id, &dev);
                        false
                    });
                }
                if changed {
                    self.emit(Event::Devices);
                }
                if self.boards.is_empty() {
                    self.emit(Event::Status(self.idle_status(None, synapse)));
                }
                for status in statuses {
                    self.emit(Event::Status(status));
                }
            }));
            // Synapse has no start event to wait for; the rest wakes the loop.
            woke = events.recv_timeout(Duration::from_secs(2)).is_ok();
            if woke {
                // A keyboard plugs in as a burst of interfaces; one step after them sees it whole.
                std::thread::sleep(Duration::from_millis(500));
                while events.try_recv().is_ok() {}
            }
        }
    }

    /// One watcher step; `true` when the keyboards changed.
    fn poll(&self, synapse: bool) -> (Vec<Status>, bool) {
        let mut changed = self.hooks.get().is_some_and(|h| self.boards.discover(&self.settings, h, synapse));
        let (out, moved) = self.boards.statuses(synapse);
        changed |= moved;
        (out, changed)
    }

    /// Restores every keyboard before the process exits.
    pub fn shutdown(&self) {
        for (_, d) in self.boards.all() {
            lock(&d).shutdown();
        }
    }

    pub(crate) fn emit(&self, e: Event) {
        let _ = self.events.send(e);
    }

    fn progress(&self, id: &str, done: usize, total: usize, key: Option<(u8, f32)>) {
        self.emit(Event::ReadProgress { id: id.to_string(), done, total, key });
    }

    pub fn device(&self, id: &str) -> Result<Arc<Mutex<Device>>, String> {
        self.boards.get_or_offline(id, &self.settings).ok_or_else(|| i18n::t(&self.lang(), "backend.noKeyboard"))
    }

    pub fn lang(&self) -> String {
        self.settings().language.clone().unwrap_or_else(|| "en".into())
    }

    pub fn settings(&self) -> MutexGuard<'_, Settings> {
        settings::lock(&self.settings)
    }

    fn update_settings(&self, f: impl FnOnce(&mut Settings)) -> Result<(), String> {
        let mut s = self.settings();
        f(&mut s);
        settings::save(&s)
    }

    fn synapse(&self) -> bool {
        let watch = self.settings().watch_synapse;
        watch && device::synapse_running()
    }

    /// The status of a keyboard not attached this session, or of none at all.
    fn idle_status(&self, id: Option<String>, synapse: bool) -> Status {
        let driver_mode = self.settings().device(id.as_deref()).driver_mode;
        Status { id, device: false, synapse, driver_mode, profile: None, profile_name: None, model: None, verified: true, analog: None, error: None }
    }

    pub fn status(&self, id: Option<String>) -> Status {
        let synapse = self.synapse();
        match id.as_deref().and_then(|id| self.boards.get_or_offline(id, &self.settings)) {
            Some(d) => lock(&d).status(synapse),
            None => self.idle_status(id, synapse),
        }
    }

    pub fn devices(&self) -> DevicesView {
        let s = self.settings().clone();
        DevicesView { devices: self.boards.list(&s), unsupported: self.boards.unsupported(), last: s.last_device.clone() }
    }

    pub fn rename_device(&self, id: &str, name: &str) -> Result<DevicesView, String> {
        let d = self.device(id)?;
        lock(&d).rename_device(name)?;
        Ok(self.devices())
    }

    pub fn forget_device(&self, id: &str) -> Result<DevicesView, String> {
        self.boards.forget(id, &self.settings)?;
        Ok(self.devices())
    }

    pub fn set_last_device(&self, id: String) -> Result<(), String> {
        self.update_settings(|s| s.last_device = Some(id))?;
        // The detached dev window follows the pick through this.
        self.emit(Event::Devices);
        Ok(())
    }

    pub fn window_ready(&self) {
        if let Some(tx) = self.window_ready.lock().unwrap_or_else(|e| e.into_inner()).take() {
            let _ = tx.send(());
        }
    }

    pub fn read_all(&self, id: &str) -> Result<Actuation, String> {
        let d = self.device(id)?;
        let mut device = lock(&d);
        let all = device.read_all(|done, total, key| self.progress(id, done, total, key))?;
        let (unsaved, unsaved_bindings) = device.unsaved();
        Ok(Actuation {
            values: all.into_iter().collect(),
            unsaved,
            rapid: device.rapid(),
            snap: device.snap_tap(),
            continuous: device.continuous(),
            unsaved_bindings,
            bindings: device.bindings(),
            hypershift: device.hypershift(),
            profile: device.loaded(),
        })
    }

    pub fn apply(&self, id: &str, e: &Edits) -> Result<WriteResult, String> {
        let d = self.device(id)?;
        let r = lock(&d).apply(&e.changes, &e.rapid, e.snap.as_ref(), e.continuous, &e.bindings, &e.hypershift);
        r.map(Into::into)
    }

    pub fn save(&self, id: &str, changes: &[(u8, f32)], bindings: &[(u8, Action)], hypershift: &HypershiftEdits) -> Result<WriteResult, String> {
        let d = self.device(id)?;
        let r = lock(&d).apply_and_write(changes, bindings, hypershift);
        r.map(Into::into)
    }

    pub fn macros(&self, id: &str) -> Result<MacroState, String> {
        let dev = self.device(id)?;
        Ok(lock(&dev).macros())
    }

    /// The macro's id, new for a new macro, and the library after the change.
    pub fn set_macro(&self, id: &str, macro_id: Option<u16>, name: String, events: Vec<macros::Event>) -> Result<(u16, MacroState), String> {
        let dev = self.device(id)?;
        let mut device = lock(&dev);
        let new_id = device.set_macro(macro_id, name, events)?;
        Ok((new_id, device.macros()))
    }

    pub fn write_macro(&self, id: &str, macro_id: u16) -> Result<MacroState, String> {
        let dev = self.device(id)?;
        let mut device = lock(&dev);
        device.write_macro(macro_id)?;
        Ok(device.macros())
    }

    pub fn delete_macro(&self, id: &str, macro_id: u16) -> Result<MacroState, String> {
        let dev = self.device(id)?;
        let mut device = lock(&dev);
        device.delete_macro(macro_id)?;
        Ok(device.macros())
    }

    pub fn lighting_state(&self, id: &str) -> Result<LightingState, String> {
        let dev = self.device(id)?;
        lock(&dev).lighting_state()
    }

    pub fn lighting_preview(&self, id: &str, look: &Look) -> Result<(), String> {
        let dev = self.device(id)?;
        lock(&dev).lighting_preview(look)
    }

    pub fn lighting_apply(&self, id: &str, look: Look) -> Result<(), String> {
        let dev = self.device(id)?;
        lock(&dev).lighting_apply(look)
    }

    pub fn lighting_write(&self, id: &str, look: Look) -> Result<(), String> {
        let dev = self.device(id)?;
        lock(&dev).lighting_write(look)
    }

    pub fn set_confirm_write(&self, on: bool) -> Result<(), String> {
        self.update_settings(|s| s.confirm_write = on)
    }

    pub fn set_driver_mode(&self, id: &str, on: bool) -> Result<(), String> {
        let dev = self.device(id)?;
        lock(&dev).set_driver_mode(on)
    }

    /// Hands the keyboard's lamps to Windows or takes them back; switching Dynamic Lighting itself is the user's call.
    pub fn set_lighting_delegated(&self, id: &str, on: bool) -> Result<(), String> {
        if self.synapse() {
            return Err(i18n::t(&self.lang(), "backend.synapseRunning"));
        }
        let dev = self.device(id)?;
        lock(&dev).set_lighting_delegated(on)
    }

    pub fn app_settings(&self) -> AppSettings {
        let s = self.settings().clone();
        AppSettings {
            autostart: autostart::enabled(),
            close_action: s.close_action,
            watch_synapse: s.watch_synapse,
            confirm_write: s.confirm_write,
            autostart_offered: s.autostart_offered,
            language: s.language,
            dev_mode: s.dev_mode,
            dev_log_size: s.dev_log_size,
            unverified_writes: s.unverified_writes,
            dev_panel: s.dev_panel,
            dev_window: s.dev_window,
        }
    }

    pub fn set_autostart(&self, on: bool) -> Result<(), String> {
        let r = autostart::set(on);
        // The tray shows the registry's answer, whatever came of the write.
        self.emit(Event::SettingsChanged);
        r
    }

    /// Any answer to the first-run question is final; a failed registry write leaves it unanswered.
    pub fn autostart_answered(&self, on: bool) -> Result<(), String> {
        if on {
            self.set_autostart(true)?;
        }
        self.update_settings(|s| s.autostart_offered = true)
    }

    pub fn set_close_action(&self, action: CloseAction) -> Result<(), String> {
        self.update_settings(|s| s.close_action = action)
    }

    pub fn close_action(&self) -> CloseAction {
        self.settings().close_action
    }

    pub fn set_watch_synapse(&self, on: bool) -> Result<(), String> {
        self.update_settings(|s| s.watch_synapse = on)?;
        wake::wake();
        Ok(())
    }

    pub fn set_unverified_writes(&self, on: bool) -> Result<(), String> {
        self.update_settings(|s| s.unverified_writes = on)?;
        // The engine starts only for keyboards the app may write to.
        wake::wake();
        Ok(())
    }

    pub fn set_language(&self, language: String) -> Result<(), String> {
        if !i18n::known(&language) {
            return Err(format!("unknown language {language}"));
        }
        self.update_settings(|s| s.language = Some(language))?;
        self.emit(Event::LanguageChanged);
        Ok(())
    }

    /// Restores the saved lighting on every keyboard once nobody sees the preview.
    pub fn drop_previews(&self) -> Result<(), String> {
        let mut first = None;
        for (_, d) in self.boards.all() {
            if let Err(e) = lock(&d).drop_preview() {
                first.get_or_insert(e);
            }
        }
        first.map_or(Ok(()), Err)
    }

    pub fn profiles(&self, id: &str) -> ProfilesView {
        if let Some(d) = self.boards.get(id) {
            return lock(&d).profiles_view();
        }
        let s = self.settings();
        device::profiles_view(s.device(Some(id)), None)
    }

    fn with_profiles(&self, id: &str, f: impl FnOnce(&mut Device) -> Result<(), String>) -> Result<ProfilesView, String> {
        let dev = self.device(id)?;
        let mut d = lock(&dev);
        f(&mut d)?;
        Ok(d.profiles_view())
    }

    pub fn load_profile(&self, id: &str, profile: u32) -> Result<ProfilesView, String> {
        self.with_profiles(id, |d| d.load_profile(profile))
    }

    pub fn create_profile(&self, id: &str) -> Result<ProfilesView, String> {
        self.with_profiles(id, |d| d.create_profile())
    }

    pub fn duplicate_profile(&self, id: &str, profile: u32) -> Result<ProfilesView, String> {
        self.with_profiles(id, |d| d.duplicate_profile(profile))
    }

    pub fn rename_profile(&self, id: &str, profile: u32, name: &str) -> Result<ProfilesView, String> {
        self.with_profiles(id, |d| d.rename_profile(profile, name))
    }

    pub fn delete_profile(&self, id: &str, profile: u32) -> Result<ProfilesView, String> {
        self.with_profiles(id, |d| d.delete_profile(profile))
    }

    pub fn set_startup(&self, id: &str, profile: u32) -> Result<ProfilesView, String> {
        self.with_profiles(id, |d| d.set_startup(profile))
    }

    pub fn free_slot(&self, id: &str, profile: u32) -> Result<ProfilesView, String> {
        self.with_profiles(id, |d| d.free_slot(profile))
    }

    pub fn write_profile(&self, id: &str, profile: u32) -> Result<ProfilesView, String> {
        self.with_profiles(id, |d| d.write_profile(profile))
    }

    pub fn export_profile(&self, id: &str, profile: u32, path: &std::path::Path) -> Result<(), String> {
        let dev = self.device(id)?;
        lock(&dev).export_profile(profile, path)
    }

    pub fn import_profile(&self, id: &str, path: &std::path::Path) -> Result<ImportView, String> {
        let dev = self.device(id)?;
        let mut d = lock(&dev);
        let got = d.import_profile(path)?;
        Ok(ImportView { view: d.profiles_view(), name: got.name, dropped: got.dropped })
    }

    pub fn gamepad_config(&self, id: &str) -> Result<gamepad::Config, String> {
        let dev = self.device(id)?;
        Ok(lock(&dev).gamepad_config())
    }

    pub fn set_gamepad_config(&self, id: &str, profile: u32, config: gamepad::Config) -> Result<(), String> {
        let dev = self.device(id)?;
        lock(&dev).set_gamepad_config(profile, config)
    }

    pub fn gamepad_input(&self, id: &str) -> Result<gamepad::Input, String> {
        let dev = self.device(id)?;
        Ok(lock(&dev).gamepad_input())
    }

    fn report_error(&self, d: &Mutex<Device>, e: &str) {
        let model = lock(d).name();
        self.emit(Event::Error(format!("{model}: {e}")));
    }

    /// Runs what the engine asked for; off the reader thread, since the device lock can be held
    /// for seconds by a full read.
    fn handle_output(&self, id: &str, o: Output) {
        let Ok(d) = self.device(id) else { return };
        match o {
            Output::Brightness(step) => lock(&d).step_brightness(step > 0),
            Output::Sleep => engine::sleep(),
            Output::NextProfile => {
                lock(&d).next_profile();
                let synapse = self.synapse();
                let status = lock(&d).status(synapse);
                self.emit(Event::Status(status));
            }
            Output::MacroLed => {
                lock(&d).set_led(LED_MACRO, true, true);
                std::thread::sleep(Duration::from_secs(1));
                lock(&d).set_led(LED_MACRO, false, false);
            }
            Output::GameMode(on) => {
                lock(&d).set_led(LED_GAME, on, false);
            }
            Output::ToggleGamepad => {
                let result = lock(&d).toggle_gamepad();
                match result {
                    Ok((profile, enabled)) => self.emit(Event::GamepadEnabled { id: id.to_string(), profile, enabled }),
                    Err(e) => self.report_error(&d, &e),
                }
            }
            Output::SnapTap(on) => {
                let r = lock(&d).set_snap_tap(on);
                if let Err(e) = r {
                    self.report_error(&d, &e);
                }
                self.emit(Event::SnapTap { id: id.to_string(), on });
            }
            _ => {}
        }
    }
}

fn sink(backend: std::sync::Weak<Backend>, id: String) -> Sink {
    Arc::new(move |o| {
        let Some(b) = backend.upgrade() else { return };
        if let Output::Depth { key, depth, travel, down } = o {
            // Up to 1 kHz while a key moves: a send into the channel never makes the reader wait.
            b.emit(Event::KeyDepth { id: id.clone(), key, depth, travel, down });
            return;
        }
        let id = id.clone();
        std::thread::spawn(move || b.handle_output(&id, o));
    })
}

impl Backend {
    /// Streams the log to the window and keeps the larger log while developer mode is on.
    fn sync_listener(&self, dev_mode: bool, log_size: usize, id: &str, device: &Device) {
        let events = self.events.clone();
        dev::sync_listener(dev_mode, log_size, id, device, move |id, entry| {
            let _ = events.send(Event::HidLog { id: id.to_string(), entry: entry.clone() });
        });
    }

    fn sync_listeners(&self) {
        let (mode, size) = { let s = self.settings(); (s.dev_mode, s.dev_log_size) };
        for (id, d) in self.boards.all() {
            self.sync_listener(mode, size, &id, &lock(&d));
        }
    }

    pub fn set_dev_mode(&self, on: bool) -> Result<(), String> {
        self.update_settings(|s| {
            s.dev_mode = on;
            if on && s.dev_panel.is_none() {
                s.dev_panel = Some(DevPanel::default());
            }
        })?;
        self.sync_listeners();
        self.emit(Event::SettingsChanged);
        Ok(())
    }

    pub fn set_dev_log_size(&self, size: usize) -> Result<(), String> {
        let range = settings::DEV_LOG_SIZES;
        self.update_settings(|s| s.dev_log_size = size.clamp(*range.start(), *range.end()))?;
        self.sync_listeners();
        self.emit(Event::SettingsChanged);
        Ok(())
    }

    pub fn set_dev_panel(&self, panel: DevPanel) -> Result<(), String> {
        self.update_settings(|s| s.dev_panel = Some(panel))?;
        self.emit(Event::SettingsChanged);
        Ok(())
    }

    pub fn set_dev_window(&self, geom: DevWindow) -> Result<(), String> {
        self.update_settings(|s| s.dev_window = Some(geom))
    }

    pub fn dev_log(&self, id: &str) -> Result<Vec<Entry>, String> {
        let d = self.device(id)?;
        Ok(lock(&d).log().entries())
    }

    pub fn dev_device(&self, id: &str) -> Result<Option<dev::DeviceInfo>, String> {
        let d = self.device(id)?;
        Ok(dev::device_info(&mut lock(&d)))
    }

    pub fn dev_exchange(&self, id: &str, class: u8, cmd: u8, size: u8, args: &[u8]) -> Result<Entry, String> {
        dev::check_args(args)?;
        let d = self.device(id)?;
        lock(&d).dev_exchange(razer_core::packet::Command::new(class, cmd), size, args)
    }

    pub fn dev_keys(&self, id: &str, keys: Vec<u8>) -> Result<BTreeMap<u8, dev::DevKey>, String> {
        let d = self.device(id)?;
        Ok(dev::keys(&lock(&d), keys))
    }

    pub fn dev_macro(&self, id: &str, macro_id: u16) -> Result<Option<Vec<u8>>, String> {
        let d = self.device(id)?;
        let r = lock(&d).data().macros.get(&macro_id).and_then(|m| macros::encode(&m.events));
        Ok(r)
    }

    pub fn dev_diagnostics(&self, id: &str, uncensored: bool) -> Result<String, String> {
        let s = self.settings().clone();
        let d = self.device(id)?;
        let mut device = lock(&d);
        let info = dev::device_info(&mut device);
        Ok(dev::report(info.as_ref(), &s, device.data(), &device.log().entries(), uncensored))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_panic_under_the_lock_does_not_lock_everyone_out() {
        let shared: Shared = Default::default();
        let device = Mutex::new(Device::new(shared).unwrap());
        let _ = std::thread::scope(|s| {
            s.spawn(|| {
                let _guard = lock(&device);
                panic!("command failed");
            })
            .join()
        });
        assert!(device.is_poisoned());
        let _ = lock(&device).data();
    }
}
