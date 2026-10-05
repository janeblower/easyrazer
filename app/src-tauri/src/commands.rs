//! Tauri commands: the only surface the UI can reach.

use std::collections::BTreeMap;
use std::sync::mpsc::Sender;
use std::sync::{Mutex, MutexGuard};

use razer_core::actuation::Outcome;
use razer_core::binding::{self, Action};
use razer_core::lighting::Look;
use razer_core::keymap;
use razer_core::layout as kb_layout;
use serde::Serialize;
use tauri::{AppHandle, Emitter, State, WebviewWindow};

use crate::device::{self, Device, LightingState, MacroState, Status, Written, mm};
use razer_core::macros::Event;
use razer_core::analog::KeyAssignment;
use crate::settings::{CloseAction, Rapid, SnapTap};
use crate::{autostart, i18n, tray};

pub struct AppState {
    pub device: Mutex<Device>,
    /// Lets the watcher connect; taken once the window listens to the read progress.
    pub window_ready: Mutex<Option<Sender<()>>>,
}

impl AppState {
    /// A command that panicked mid-way leaves the device in a usable state, so poisoning is ignored.
    pub fn device(&self) -> MutexGuard<'_, Device> {
        self.device.lock().unwrap_or_else(|e| e.into_inner())
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
    /// `null` for a binding the app cannot show, such as Hypershift.
    bindings: BTreeMap<u8, Option<Action>>,
    unsaved_bindings: Vec<u8>,
    /// Id of the loaded profile the table belongs to.
    profile: Option<u32>,
}

#[derive(Serialize)]
pub struct WriteResult {
    results: Vec<ApplyResult>,
    unsaved: Vec<u8>,
    bindings: Vec<BindResult>,
    unsaved_bindings: Vec<u8>,
}

impl From<Written> for WriteResult {
    fn from(w: Written) -> Self {
        let results = w
            .results
            .into_iter()
            .map(|(key, o)| match o {
                Outcome::Ok(a) => ApplyResult::Ok { key, mm: mm(a.threshold_low) },
                Outcome::Unconfirmed(a) => ApplyResult::Unconfirmed { key, mm: mm(a.threshold_low) },
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
        Self { results, unsaved: w.unsaved, bindings, unsaved_bindings: w.unsaved_bindings }
    }
}

/// One watcher step: the Synapse check runs outside the lock, `tasklist` takes a few hundred ms.
pub fn poll(state: &AppState) -> Status {
    let watch = state.device().settings().watch_synapse;
    let synapse = device::synapse_check(watch, device::synapse_running);
    state.device().status(Some(synapse))
}

#[tauri::command]
pub async fn status(state: State<'_, AppState>) -> Result<Status, String> {
    Ok(poll(&state))
}

#[tauri::command]
pub fn window_ready(state: State<'_, AppState>) {
    if let Some(tx) = state.window_ready.lock().unwrap_or_else(|e| e.into_inner()).take() {
        let _ = tx.send(());
    }
}

#[tauri::command]
pub fn layout() -> Vec<KeyView> {
    key_views(kb_layout::keys())
}

#[tauri::command]
pub fn lighting_layout() -> Vec<KeyView> {
    key_views(kb_layout::lighting_keys())
}

/// The window fills the key map from these while a slot is read.
pub fn emit_progress(app: &AppHandle, done: usize, total: usize, a: Option<&KeyAssignment>) {
    let _ = app.emit("read-progress", (done, total, a.map(|a| a.key), a.map(|a| mm(a.threshold_low)), None::<Rapid>));
}

#[tauri::command]
pub async fn read_all(app: AppHandle, state: State<'_, AppState>) -> Result<Actuation, String> {
    let mut device = state.device();
    let all = device.read_all(|done, total, a| emit_progress(&app, done, total, a))?;
    let (unsaved, unsaved_bindings) = device.unsaved();
    Ok(Actuation {
        values: all.iter().map(|a| (a.key, mm(a.threshold_low))).collect(),
        unsaved,
        rapid: device.rapid(),
        snap: device.snap_tap(),
        unsaved_bindings,
        bindings: device.bindings(),
        profile: device.loaded(),
    })
}

#[tauri::command]
pub async fn apply(
    state: State<'_, AppState>,
    changes: Vec<(u8, f32)>,
    rapid: BTreeMap<u8, Rapid>,
    snap: Option<SnapTap>,
    bindings: Vec<(u8, Action)>,
) -> Result<WriteResult, String> {
    state.device().apply(&changes, &rapid, snap.as_ref(), &bindings).map(Into::into)
}

#[tauri::command]
pub async fn save(state: State<'_, AppState>, changes: Vec<(u8, f32)>, bindings: Vec<(u8, Action)>) -> Result<WriteResult, String> {
    state.device().save(&changes, &bindings).map(Into::into)
}

#[tauri::command]
pub async fn macros(state: State<'_, AppState>) -> Result<MacroState, String> {
    Ok(state.device().macros())
}

/// The macro's id, new for a new macro, and the library after the change.
#[tauri::command]
pub async fn set_macro(state: State<'_, AppState>, id: Option<u16>, name: String, events: Vec<Event>) -> Result<(u16, MacroState), String> {
    let mut device = state.device();
    let id = device.set_macro(id, name, events)?;
    Ok((id, device.macros()))
}

#[tauri::command]
pub async fn write_macro(state: State<'_, AppState>, id: u16) -> Result<MacroState, String> {
    let mut device = state.device();
    device.write_macro(id)?;
    Ok(device.macros())
}

#[tauri::command]
pub async fn delete_macro(state: State<'_, AppState>, id: u16) -> Result<MacroState, String> {
    let mut device = state.device();
    device.delete_macro(id)?;
    Ok(device.macros())
}

#[tauri::command]
pub async fn lighting_state(state: State<'_, AppState>) -> Result<LightingState, String> {
    state.device().lighting_state()
}

#[tauri::command]
pub async fn lighting_preview(state: State<'_, AppState>, look: Look) -> Result<(), String> {
    state.device().lighting_preview(&look)
}

#[tauri::command]
pub async fn lighting_apply(state: State<'_, AppState>, look: Look) -> Result<(), String> {
    state.device().lighting_apply(look)
}

#[tauri::command]
pub async fn lighting_write(state: State<'_, AppState>, look: Look) -> Result<(), String> {
    state.device().lighting_write(look)
}

#[tauri::command]
pub async fn set_confirm_write(state: State<'_, AppState>, on: bool) -> Result<(), String> {
    state.device().update_settings(|s| s.confirm_write = on)
}

#[tauri::command]
pub async fn set_driver_mode(state: State<'_, AppState>, on: bool) -> Result<(), String> {
    state.device().set_driver_mode(on)
}

#[tauri::command]
pub async fn set_dynamic_lighting(state: State<'_, AppState>, on: bool) -> Result<(), String> {
    state.device().set_dynamic_lighting(on)
}

#[derive(Serialize)]
pub struct AppSettings {
    autostart: bool,
    close_action: CloseAction,
    watch_synapse: bool,
    confirm_write: bool,
    autostart_offered: bool,
    language: Option<String>,
}

#[tauri::command]
pub async fn app_settings(state: State<'_, AppState>) -> Result<AppSettings, String> {
    let s = state.device().settings().clone();
    Ok(AppSettings {
        autostart: autostart::enabled(),
        close_action: s.close_action,
        watch_synapse: s.watch_synapse,
        confirm_write: s.confirm_write,
        autostart_offered: s.autostart_offered,
        language: s.language,
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
    state.device().update_settings(|s| s.autostart_offered = true)
}

#[tauri::command]
pub async fn set_close_action(state: State<'_, AppState>, action: CloseAction) -> Result<(), String> {
    state.device().update_settings(|s| s.close_action = action)
}

#[tauri::command]
pub async fn set_watch_synapse(state: State<'_, AppState>, on: bool) -> Result<(), String> {
    state.device().update_settings(|s| s.watch_synapse = on)
}

#[tauri::command]
pub async fn set_language(app: AppHandle, state: State<'_, AppState>, language: String) -> Result<(), String> {
    if !i18n::known(&language) {
        return Err(format!("unknown language {language}"));
    }
    state.device().update_settings(|s| s.language = Some(language))?;
    tray::retitle(&app);
    Ok(())
}

#[tauri::command]
pub async fn hide_window(state: State<'_, AppState>, window: WebviewWindow) -> Result<(), String> {
    window.destroy().map_err(|e| e.to_string())?;
    state.device().drop_preview()
}

#[tauri::command]
pub fn quit(app: AppHandle) {
    app.exit(0);
}

#[derive(Serialize)]
pub struct ProfileView {
    id: u32,
    name: String,
    slot: Option<u8>,
    /// Differs from its slot.
    unsaved: bool,
}

#[derive(Serialize)]
pub struct ProfilesView {
    profiles: Vec<ProfileView>,
    loaded: Option<u32>,
    /// The slot the keyboard starts with without the app.
    startup: Option<u8>,
    free_slot: bool,
}

fn view(device: &Device) -> ProfilesView {
    let (list, loaded, startup, free_slot) = device.profiles_view();
    let profiles = list.into_iter().map(|(id, name, slot, unsaved)| ProfileView { id, name, slot, unsaved }).collect();
    ProfilesView { profiles, loaded, startup, free_slot }
}

#[tauri::command]
pub async fn profiles(state: State<'_, AppState>) -> Result<ProfilesView, String> {
    Ok(view(&state.device()))
}

#[tauri::command]
pub async fn load_profile(state: State<'_, AppState>, id: u32) -> Result<ProfilesView, String> {
    let mut d = state.device();
    d.load_profile(id)?;
    Ok(view(&d))
}

#[tauri::command]
pub async fn create_profile(state: State<'_, AppState>) -> Result<ProfilesView, String> {
    let mut d = state.device();
    d.create_profile()?;
    Ok(view(&d))
}

#[tauri::command]
pub async fn duplicate_profile(state: State<'_, AppState>, id: u32) -> Result<ProfilesView, String> {
    let mut d = state.device();
    d.duplicate_profile(id)?;
    Ok(view(&d))
}

#[tauri::command]
pub async fn rename_profile(state: State<'_, AppState>, id: u32, name: String) -> Result<ProfilesView, String> {
    let mut d = state.device();
    d.rename_profile(id, &name)?;
    Ok(view(&d))
}

#[tauri::command]
pub async fn delete_profile(state: State<'_, AppState>, id: u32) -> Result<ProfilesView, String> {
    let mut d = state.device();
    d.delete_profile(id)?;
    Ok(view(&d))
}

#[tauri::command]
pub async fn set_startup(state: State<'_, AppState>, id: u32) -> Result<ProfilesView, String> {
    let mut d = state.device();
    d.set_startup(id)?;
    Ok(view(&d))
}

#[tauri::command]
pub async fn free_slot(state: State<'_, AppState>, id: u32) -> Result<ProfilesView, String> {
    let mut d = state.device();
    d.free_slot(id)?;
    Ok(view(&d))
}

#[tauri::command]
pub async fn write_profile(state: State<'_, AppState>, id: u32) -> Result<ProfilesView, String> {
    let mut d = state.device();
    d.write_profile(id)?;
    Ok(view(&d))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_panic_under_the_lock_does_not_lock_everyone_out() {
        let state = AppState { device: Mutex::new(Device::new().unwrap()), window_ready: Mutex::new(None) };
        let _ = std::thread::scope(|s| {
            s.spawn(|| {
                let _guard = state.device();
                panic!("command failed");
            })
            .join()
        });
        assert!(state.device.is_poisoned());
        let _ = state.device().settings();
    }
}
