//! Tauri commands: the only surface the UI can reach.

use std::collections::BTreeMap;
use std::sync::mpsc::Sender;
use std::sync::{Arc, Mutex, MutexGuard, OnceLock};

use razer_core::actuation::Outcome;
use razer_core::binding::{self, Action};
use razer_core::lighting::Look;
use razer_core::keymap;
use razer_core::devices;
use razer_core::layout::{self as kb_layout, Board};
use serde::Serialize;
use tauri::{AppHandle, Emitter, State, WebviewWindow};

use crate::boards::{Boards, DevicesView, Hooks};
use crate::device::{self, Device, Hypershift, HypershiftEdits, LightingState, MacroState, ProfilesView, Status, Written, mm};
use razer_core::macros::Event;
use razer_core::analog::KeyAssignment;
use crate::settings::{self, CloseAction, DevPanel, DevWindow, Rapid, Settings, Shared, SnapTap};
use crate::{autostart, i18n, tray, wake};

pub struct AppState {
    pub settings: Shared,
    pub boards: Boards,
    /// Set in `setup`, before the watcher starts.
    pub hooks: OnceLock<Hooks>,
    /// Lets the watcher connect; taken once the window listens to the read progress.
    pub window_ready: Mutex<Option<Sender<()>>>,
}

/// A command that panicked mid-way leaves the device in a usable state, so poisoning is ignored.
pub fn lock(d: &Mutex<Device>) -> MutexGuard<'_, Device> {
    d.lock().unwrap_or_else(|e| e.into_inner())
}

impl AppState {
    pub fn device(&self, id: &str) -> Result<Arc<Mutex<Device>>, String> {
        self.boards.get_or_offline(id, &self.settings).ok_or_else(|| i18n::t(&self.lang(), "backend.noKeyboard"))
    }

    pub fn lang(&self) -> String {
        self.settings().language.clone().unwrap_or_else(|| "en".into())
    }

    pub fn settings(&self) -> MutexGuard<'_, Settings> {
        settings::lock(&self.settings)
    }

    pub fn update_settings(&self, f: impl FnOnce(&mut Settings)) -> Result<(), String> {
        let mut s = self.settings();
        f(&mut s);
        settings::save(&s)
    }
}

#[derive(Serialize)]
pub struct KeyView {
    /// Key name from `keymap`, empty for lighting zones.
    name: &'static str,
    #[serde(flatten)]
    key: kb_layout::LayoutKey,
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
    values: BTreeMap<u8, f32>,
    /// Keys whose press point a replug would reset.
    unsaved: Vec<u8>,
    /// Rapid Trigger per key, from the app's settings.
    rapid: BTreeMap<u8, Rapid>,
    /// Snap Tap of the loaded profile.
    snap: SnapTap,
    /// Firmware continuous Rapid Trigger of the loaded profile.
    continuous: bool,
    /// `null` for a binding the app cannot show, such as the Fn key.
    bindings: BTreeMap<u8, Option<Action>>,
    unsaved_bindings: Vec<u8>,
    hypershift: Hypershift,
    /// Id of the loaded profile the table belongs to.
    profile: Option<u32>,
}

#[derive(Serialize)]
pub struct WriteResult {
    results: Vec<ApplyResult>,
    unsaved: Vec<u8>,
    bindings: Vec<BindResult>,
    unsaved_bindings: Vec<u8>,
    hypershift: Hypershift,
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

/// One watcher step; `true` when the keyboards changed.
pub fn poll(state: &AppState, synapse: bool) -> (Vec<Status>, bool) {
    let mut changed = state.hooks.get().is_some_and(|h| state.boards.discover(&state.settings, h, synapse));
    let (out, moved) = state.boards.statuses(synapse);
    changed |= moved;
    (out, changed)
}

/// The status of a keyboard not attached this session, or of none at all.
pub fn idle_status(state: &AppState, id: Option<String>, synapse: bool) -> Status {
    let driver_mode = state.settings().device(id.as_deref()).driver_mode;
    Status { id, device: false, synapse, driver_mode, profile: None, profile_name: None, model: None, verified: true, analog: None, error: None }
}

#[tauri::command]
pub async fn status(state: State<'_, AppState>, id: Option<String>) -> Result<Status, String> {
    let watch = state.settings().watch_synapse;
    let synapse = watch && device::synapse_running();
    match id.as_deref().and_then(|id| state.boards.get_or_offline(id, &state.settings)) {
        Some(d) => Ok(lock(&d).status(synapse)),
        None => Ok(idle_status(&state, id, synapse)),
    }
}

fn devices_view(state: &AppState) -> DevicesView {
    let s = state.settings().clone();
    DevicesView { devices: state.boards.list(&s), unsupported: state.boards.unsupported(), last: s.last_device.clone() }
}

#[tauri::command]
pub async fn devices(state: State<'_, AppState>) -> Result<DevicesView, String> {
    Ok(devices_view(&state))
}

#[tauri::command]
pub async fn rename_device(state: State<'_, AppState>, id: String, name: String) -> Result<DevicesView, String> {
    let d = state.device(&id)?;
    lock(&d).rename_device(&name)?;
    Ok(devices_view(&state))
}

#[tauri::command]
pub async fn forget_device(state: State<'_, AppState>, id: String) -> Result<DevicesView, String> {
    state.boards.forget(&id, &state.settings)?;
    Ok(devices_view(&state))
}

#[tauri::command]
pub async fn set_last_device(app: AppHandle, state: State<'_, AppState>, id: String) -> Result<(), String> {
    state.update_settings(|s| s.last_device = Some(id))?;
    // The detached dev window follows the pick through this.
    let _ = app.emit("devices", ());
    Ok(())
}

#[tauri::command]
pub fn window_ready(state: State<'_, AppState>) {
    if let Some(tx) = state.window_ready.lock().unwrap_or_else(|e| e.into_inner()).take() {
        let _ = tx.send(());
    }
}

#[tauri::command]
pub fn layout(model: Option<String>) -> Vec<KeyView> {
    let spec = devices::all().iter().find(|d| model.as_deref() == Some(d.name.as_str()));
    key_views(kb_layout::keys(spec.map_or(Board::Full, |d| d.layout)))
}

#[tauri::command]
pub fn lighting_layout() -> Vec<KeyView> {
    key_views(kb_layout::lighting_keys())
}

/// The window fills the key map from these while a slot is read.
pub fn emit_progress(app: &AppHandle, id: &str, done: usize, total: usize, key: Option<(u8, f32)>) {
    let _ = app.emit("read-progress", (id, done, total, key.map(|k| k.0), key.map(|k| k.1)));
}

#[tauri::command]
pub async fn read_all(app: AppHandle, state: State<'_, AppState>, id: String) -> Result<Actuation, String> {
    let d = state.device(&id)?;
    let mut device = lock(&d);
    let all = device.read_all(|done, total, a| emit_progress(&app, &id, done, total, a))?;
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

#[tauri::command]
pub async fn apply(
    state: State<'_, AppState>,
    id: String,
    changes: Vec<(u8, f32)>,
    rapid: BTreeMap<u8, Rapid>,
    snap: Option<SnapTap>,
    continuous: Option<bool>,
    bindings: Vec<(u8, Action)>,
    hypershift: HypershiftEdits,
) -> Result<WriteResult, String> {
    let d = state.device(&id)?;
    let r = lock(&d).apply(&changes, &rapid, snap.as_ref(), continuous, &bindings, &hypershift);
    r.map(Into::into)
}

#[tauri::command]
pub async fn save(
    state: State<'_, AppState>,
    id: String,
    changes: Vec<(u8, f32)>,
    bindings: Vec<(u8, Action)>,
    hypershift: HypershiftEdits,
) -> Result<WriteResult, String> {
    let d = state.device(&id)?;
    let r = lock(&d).apply_and_write(&changes, &bindings, &hypershift);
    r.map(Into::into)
}

#[tauri::command]
pub async fn macros(state: State<'_, AppState>, id: String) -> Result<MacroState, String> {
    let dev = state.device(&id)?;
    Ok(lock(&dev).macros())
}

/// The macro's id, new for a new macro, and the library after the change.
#[tauri::command]
pub async fn set_macro(state: State<'_, AppState>, id: String, macro_id: Option<u16>, name: String, events: Vec<Event>) -> Result<(u16, MacroState), String> {
    let dev = state.device(&id)?;
    let mut device = lock(&dev);
    let new_id = device.set_macro(macro_id, name, events)?;
    Ok((new_id, device.macros()))
}

#[tauri::command]
pub async fn write_macro(state: State<'_, AppState>, id: String, macro_id: u16) -> Result<MacroState, String> {
    let dev = state.device(&id)?;
    let mut device = lock(&dev);
    device.write_macro(macro_id)?;
    Ok(device.macros())
}

#[tauri::command]
pub async fn delete_macro(state: State<'_, AppState>, id: String, macro_id: u16) -> Result<MacroState, String> {
    let dev = state.device(&id)?;
    let mut device = lock(&dev);
    device.delete_macro(macro_id)?;
    Ok(device.macros())
}

#[tauri::command]
pub async fn lighting_state(state: State<'_, AppState>, id: String) -> Result<LightingState, String> {
    let dev = state.device(&id)?;
    lock(&dev).lighting_state()
}

#[tauri::command]
pub async fn lighting_preview(state: State<'_, AppState>, id: String, look: Look) -> Result<(), String> {
    let dev = state.device(&id)?;
    lock(&dev).lighting_preview(&look)
}

#[tauri::command]
pub async fn lighting_apply(state: State<'_, AppState>, id: String, look: Look) -> Result<(), String> {
    let dev = state.device(&id)?;
    lock(&dev).lighting_apply(look)
}

#[tauri::command]
pub async fn lighting_write(state: State<'_, AppState>, id: String, look: Look) -> Result<(), String> {
    let dev = state.device(&id)?;
    lock(&dev).lighting_write(look)
}

#[tauri::command]
pub async fn set_confirm_write(state: State<'_, AppState>, on: bool) -> Result<(), String> {
    state.update_settings(|s| s.confirm_write = on)
}

#[tauri::command]
pub async fn set_driver_mode(state: State<'_, AppState>, id: String, on: bool) -> Result<(), String> {
    let dev = state.device(&id)?;
    lock(&dev).set_driver_mode(on)
}

/// Hands the keyboard's lamps to Windows or takes them back; switching Dynamic Lighting itself is the user's call.
#[tauri::command]
pub async fn set_lighting_delegated(state: State<'_, AppState>, id: String, on: bool) -> Result<(), String> {
    let watch = state.settings().watch_synapse;
    if watch && device::synapse_running() {
        return Err(i18n::t(&state.lang(), "backend.synapseRunning"));
    }
    let dev = state.device(&id)?;
    lock(&dev).set_lighting_delegated(on)
}

/// Opens the Dynamic Lighting page of Windows Settings, where it can be switched per device.
#[tauri::command]
pub fn open_dynamic_lighting_settings() -> Result<(), String> {
    open::that_detached("ms-settings:personalization-lighting").map_err(|e| e.to_string())
}

#[derive(Serialize)]
pub struct AppSettings {
    autostart: bool,
    close_action: CloseAction,
    watch_synapse: bool,
    confirm_write: bool,
    autostart_offered: bool,
    language: Option<String>,
    dev_mode: bool,
    dev_log_size: usize,
    unverified_writes: bool,
    dev_panel: Option<DevPanel>,
    dev_window: Option<DevWindow>,
}

#[tauri::command]
pub async fn app_settings(state: State<'_, AppState>) -> Result<AppSettings, String> {
    let s = state.settings().clone();
    Ok(AppSettings {
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
    })
}

#[tauri::command]
pub async fn set_autostart(app: AppHandle, on: bool) -> Result<(), String> {
    let r = autostart::set(on);
    tray::sync_autostart(&app);
    r
}

/// Any answer to the first-run question is final; a failed registry write leaves it unanswered.
#[tauri::command]
pub async fn autostart_answered(app: AppHandle, state: State<'_, AppState>, on: bool) -> Result<(), String> {
    if on {
        set_autostart(app, true).await?;
    }
    state.update_settings(|s| s.autostart_offered = true)
}

#[tauri::command]
pub async fn set_close_action(state: State<'_, AppState>, action: CloseAction) -> Result<(), String> {
    state.update_settings(|s| s.close_action = action)
}

#[tauri::command]
pub async fn set_watch_synapse(state: State<'_, AppState>, on: bool) -> Result<(), String> {
    state.update_settings(|s| s.watch_synapse = on)?;
    wake::wake();
    Ok(())
}

#[tauri::command]
pub async fn set_unverified_writes(state: State<'_, AppState>, on: bool) -> Result<(), String> {
    state.update_settings(|s| s.unverified_writes = on)?;
    // The engine starts only for keyboards the app may write to.
    wake::wake();
    Ok(())
}

#[tauri::command]
pub async fn set_language(app: AppHandle, state: State<'_, AppState>, language: String) -> Result<(), String> {
    if !i18n::known(&language) {
        return Err(format!("unknown language {language}"));
    }
    state.update_settings(|s| s.language = Some(language))?;
    tray::retitle(&app);
    Ok(())
}

#[tauri::command]
pub async fn hide_window(state: State<'_, AppState>, window: WebviewWindow) -> Result<(), String> {
    window.destroy().map_err(|e| e.to_string())?;
    let mut first = None;
    for (_, d) in state.boards.all() {
        if let Err(e) = lock(&d).drop_preview() {
            first.get_or_insert(e);
        }
    }
    first.map_or(Ok(()), Err)
}

#[tauri::command]
pub fn quit(app: AppHandle) {
    app.exit(0);
}

#[derive(serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Link {
    Repo,
    License,
}

/// Opens a project page; the URLs are fixed so the UI cannot open other targets.
#[tauri::command]
pub fn open_link(link: Link) -> Result<(), String> {
    open::that_detached(match link {
        Link::Repo => "https://github.com/janeblower/easyrazer",
        Link::License => "https://github.com/janeblower/easyrazer/blob/master/LICENSE",
    })
    .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn profiles(state: State<'_, AppState>, id: String) -> Result<ProfilesView, String> {
    if let Some(d) = state.boards.get(&id) {
        return Ok(lock(&d).profiles_view());
    }
    let s = state.settings();
    Ok(device::profiles_view(s.device(Some(&id)), None))
}

#[tauri::command]
pub async fn load_profile(state: State<'_, AppState>, id: String, profile: u32) -> Result<ProfilesView, String> {
    let dev = state.device(&id)?;
    let mut d = lock(&dev);
    d.load_profile(profile)?;
    Ok(d.profiles_view())
}

#[tauri::command]
pub async fn create_profile(state: State<'_, AppState>, id: String) -> Result<ProfilesView, String> {
    let dev = state.device(&id)?;
    let mut d = lock(&dev);
    d.create_profile()?;
    Ok(d.profiles_view())
}

#[tauri::command]
pub async fn duplicate_profile(state: State<'_, AppState>, id: String, profile: u32) -> Result<ProfilesView, String> {
    let dev = state.device(&id)?;
    let mut d = lock(&dev);
    d.duplicate_profile(profile)?;
    Ok(d.profiles_view())
}

#[tauri::command]
pub async fn rename_profile(state: State<'_, AppState>, id: String, profile: u32, name: String) -> Result<ProfilesView, String> {
    let dev = state.device(&id)?;
    let mut d = lock(&dev);
    d.rename_profile(profile, &name)?;
    Ok(d.profiles_view())
}

#[tauri::command]
pub async fn delete_profile(state: State<'_, AppState>, id: String, profile: u32) -> Result<ProfilesView, String> {
    let dev = state.device(&id)?;
    let mut d = lock(&dev);
    d.delete_profile(profile)?;
    Ok(d.profiles_view())
}

#[tauri::command]
pub async fn set_startup(state: State<'_, AppState>, id: String, profile: u32) -> Result<ProfilesView, String> {
    let dev = state.device(&id)?;
    let mut d = lock(&dev);
    d.set_startup(profile)?;
    Ok(d.profiles_view())
}

#[tauri::command]
pub async fn free_slot(state: State<'_, AppState>, id: String, profile: u32) -> Result<ProfilesView, String> {
    let dev = state.device(&id)?;
    let mut d = lock(&dev);
    d.free_slot(profile)?;
    Ok(d.profiles_view())
}

#[tauri::command]
pub async fn write_profile(state: State<'_, AppState>, id: String, profile: u32) -> Result<ProfilesView, String> {
    let dev = state.device(&id)?;
    let mut d = lock(&dev);
    d.write_profile(profile)?;
    Ok(d.profiles_view())
}

#[derive(Serialize)]
pub struct ImportView {
    view: ProfilesView,
    name: String,
    dropped: usize,
}

#[tauri::command]
pub async fn export_profile(state: State<'_, AppState>, id: String, profile: u32, path: String) -> Result<(), String> {
    let dev = state.device(&id)?;
    lock(&dev).export_profile(profile, path.as_ref())
}

#[tauri::command]
pub async fn import_profile(state: State<'_, AppState>, id: String, path: String) -> Result<ImportView, String> {
    let dev = state.device(&id)?;
    let mut d = lock(&dev);
    let got = d.import_profile(path.as_ref())?;
    Ok(ImportView { view: d.profiles_view(), name: got.name, dropped: got.dropped })
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

#[tauri::command]
pub async fn gamepad_config(state: State<'_, AppState>, id: String) -> Result<crate::gamepad::Config, String> {
    let dev = state.device(&id)?;
    Ok(lock(&dev).gamepad_config())
}

#[tauri::command]
pub async fn set_gamepad_config(state: State<'_, AppState>, id: String, profile: u32, config: crate::gamepad::Config) -> Result<(), String> {
    let dev = state.device(&id)?;
    lock(&dev).set_gamepad_config(profile, config)
}

#[tauri::command]
pub async fn gamepad_input(state: State<'_, AppState>, id: String) -> Result<crate::gamepad::Input, String> {
    let dev = state.device(&id)?;
    Ok(lock(&dev).gamepad_input())
}

#[tauri::command]
pub async fn gamepad_driver() -> Result<crate::gamepad::drivers::Status, String> {
    tauri::async_runtime::spawn_blocking(crate::gamepad::drivers::status).await.map_err(|e| e.to_string())?
}

#[tauri::command]
pub async fn install_gamepad_driver(replace: bool) -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(move || crate::gamepad::drivers::install(replace)).await.map_err(|e| e.to_string())?
}

#[tauri::command]
pub async fn set_gamepad_driver_boot(on: bool) -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(move || crate::gamepad::drivers::set_boot(on)).await.map_err(|e| e.to_string())?
}

#[tauri::command]
pub async fn uninstall_gamepad_driver() -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(crate::gamepad::drivers::uninstall).await.map_err(|e| e.to_string())?
}
