//! Tauri commands: the backend API as the WebView sees it.

use std::collections::BTreeMap;
use std::sync::Arc;

use easyrazer_backend::api::{self, Actuation, AppSettings, Edits, ImportView, KeyView, Link, WriteResult};
use easyrazer_backend::boards::DevicesView;
use easyrazer_backend::dev::{DevKey, DeviceInfo};
use easyrazer_backend::device::{HypershiftEdits, LightingState, MacroState, ProfilesView, Status};
use easyrazer_backend::gamepad;
use easyrazer_backend::settings::{CloseAction, DevPanel, DevWindow, Rapid, SnapTap};
use easyrazer_backend::Backend;
use razer_core::binding::Action;
use razer_core::lighting::Look;
use razer_core::logged::Entry;
use razer_core::macros::Event;
use razer_core::registry::CommandInfo;
use tauri::{AppHandle, State, WebviewWindow};

type St<'a> = State<'a, Arc<Backend>>;

#[tauri::command]
pub async fn status(state: St<'_>, id: Option<String>) -> Result<Status, String> {
    Ok(state.status(id))
}

#[tauri::command]
pub async fn devices(state: St<'_>) -> Result<DevicesView, String> {
    Ok(state.devices())
}

#[tauri::command]
pub async fn rename_device(state: St<'_>, id: String, name: String) -> Result<DevicesView, String> {
    state.rename_device(&id, &name)
}

#[tauri::command]
pub async fn forget_device(state: St<'_>, id: String) -> Result<DevicesView, String> {
    state.forget_device(&id)
}

#[tauri::command]
pub async fn set_last_device(state: St<'_>, id: String) -> Result<(), String> {
    state.set_last_device(id)
}

#[tauri::command]
pub fn window_ready(state: St<'_>) {
    state.window_ready();
}

#[tauri::command]
pub fn layout(model: Option<String>) -> Vec<KeyView> {
    api::layout(model.as_deref())
}

#[tauri::command]
pub fn lighting_layout() -> Vec<KeyView> {
    api::lighting_layout()
}

#[tauri::command]
pub async fn read_all(state: St<'_>, id: String) -> Result<Actuation, String> {
    state.read_all(&id)
}

#[allow(clippy::too_many_arguments)]
#[tauri::command]
pub async fn apply(
    state: St<'_>,
    id: String,
    changes: Vec<(u8, f32)>,
    rapid: BTreeMap<u8, Rapid>,
    snap: Option<SnapTap>,
    continuous: Option<bool>,
    bindings: Vec<(u8, Action)>,
    hypershift: HypershiftEdits,
) -> Result<WriteResult, String> {
    state.apply(&id, &Edits { changes, rapid, snap, continuous, bindings, hypershift })
}

#[tauri::command]
pub async fn save(state: St<'_>, id: String, changes: Vec<(u8, f32)>, bindings: Vec<(u8, Action)>, hypershift: HypershiftEdits) -> Result<WriteResult, String> {
    state.save(&id, &changes, &bindings, &hypershift)
}

#[tauri::command]
pub async fn macros(state: St<'_>, id: String) -> Result<MacroState, String> {
    state.macros(&id)
}

#[tauri::command]
pub async fn set_macro(state: St<'_>, id: String, macro_id: Option<u16>, name: String, events: Vec<Event>) -> Result<(u16, MacroState), String> {
    state.set_macro(&id, macro_id, name, events)
}

#[tauri::command]
pub async fn write_macro(state: St<'_>, id: String, macro_id: u16) -> Result<MacroState, String> {
    state.write_macro(&id, macro_id)
}

#[tauri::command]
pub async fn delete_macro(state: St<'_>, id: String, macro_id: u16) -> Result<MacroState, String> {
    state.delete_macro(&id, macro_id)
}

#[tauri::command]
pub async fn lighting_state(state: St<'_>, id: String) -> Result<LightingState, String> {
    state.lighting_state(&id)
}

#[tauri::command]
pub async fn lighting_preview(state: St<'_>, id: String, look: Look) -> Result<(), String> {
    state.lighting_preview(&id, &look)
}

#[tauri::command]
pub async fn lighting_apply(state: St<'_>, id: String, look: Look) -> Result<(), String> {
    state.lighting_apply(&id, look)
}

#[tauri::command]
pub async fn lighting_write(state: St<'_>, id: String, look: Look) -> Result<(), String> {
    state.lighting_write(&id, look)
}

#[tauri::command]
pub async fn set_confirm_write(state: St<'_>, on: bool) -> Result<(), String> {
    state.set_confirm_write(on)
}

#[tauri::command]
pub async fn set_driver_mode(state: St<'_>, id: String, on: bool) -> Result<(), String> {
    state.set_driver_mode(&id, on)
}

#[tauri::command]
pub async fn set_lighting_delegated(state: St<'_>, id: String, on: bool) -> Result<(), String> {
    state.set_lighting_delegated(&id, on)
}

#[tauri::command]
pub fn open_dynamic_lighting_settings() -> Result<(), String> {
    api::open_dynamic_lighting_settings()
}

#[tauri::command]
pub async fn app_settings(state: St<'_>) -> Result<AppSettings, String> {
    Ok(state.app_settings())
}

#[tauri::command]
pub async fn set_autostart(state: St<'_>, on: bool) -> Result<(), String> {
    state.set_autostart(on)
}

#[tauri::command]
pub async fn autostart_answered(state: St<'_>, on: bool) -> Result<(), String> {
    state.autostart_answered(on)
}

#[tauri::command]
pub async fn set_close_action(state: St<'_>, action: CloseAction) -> Result<(), String> {
    state.set_close_action(action)
}

#[tauri::command]
pub async fn set_watch_synapse(state: St<'_>, on: bool) -> Result<(), String> {
    state.set_watch_synapse(on)
}

#[tauri::command]
pub async fn set_unverified_writes(state: St<'_>, on: bool) -> Result<(), String> {
    state.set_unverified_writes(on)
}

#[tauri::command]
pub async fn set_language(state: St<'_>, language: String) -> Result<(), String> {
    state.set_language(language)
}

#[tauri::command]
pub async fn hide_window(state: St<'_>, window: WebviewWindow) -> Result<(), String> {
    window.destroy().map_err(|e| e.to_string())?;
    state.drop_previews()
}

#[tauri::command]
pub fn quit(app: AppHandle) {
    app.exit(0);
}

#[tauri::command]
pub fn open_link(link: Link) -> Result<(), String> {
    api::open_link(link)
}

#[tauri::command]
pub async fn profiles(state: St<'_>, id: String) -> Result<ProfilesView, String> {
    Ok(state.profiles(&id))
}

#[tauri::command]
pub async fn load_profile(state: St<'_>, id: String, profile: u32) -> Result<ProfilesView, String> {
    state.load_profile(&id, profile)
}

#[tauri::command]
pub async fn create_profile(state: St<'_>, id: String) -> Result<ProfilesView, String> {
    state.create_profile(&id)
}

#[tauri::command]
pub async fn duplicate_profile(state: St<'_>, id: String, profile: u32) -> Result<ProfilesView, String> {
    state.duplicate_profile(&id, profile)
}

#[tauri::command]
pub async fn rename_profile(state: St<'_>, id: String, profile: u32, name: String) -> Result<ProfilesView, String> {
    state.rename_profile(&id, profile, &name)
}

#[tauri::command]
pub async fn delete_profile(state: St<'_>, id: String, profile: u32) -> Result<ProfilesView, String> {
    state.delete_profile(&id, profile)
}

#[tauri::command]
pub async fn set_startup(state: St<'_>, id: String, profile: u32) -> Result<ProfilesView, String> {
    state.set_startup(&id, profile)
}

#[tauri::command]
pub async fn free_slot(state: St<'_>, id: String, profile: u32) -> Result<ProfilesView, String> {
    state.free_slot(&id, profile)
}

#[tauri::command]
pub async fn write_profile(state: St<'_>, id: String, profile: u32) -> Result<ProfilesView, String> {
    state.write_profile(&id, profile)
}

#[tauri::command]
pub async fn export_profile(state: St<'_>, id: String, profile: u32, path: String) -> Result<(), String> {
    state.export_profile(&id, profile, path.as_ref())
}

#[tauri::command]
pub async fn import_profile(state: St<'_>, id: String, path: String) -> Result<ImportView, String> {
    state.import_profile(&id, path.as_ref())
}

#[tauri::command]
pub async fn gamepad_config(state: St<'_>, id: String) -> Result<gamepad::Config, String> {
    state.gamepad_config(&id)
}

#[tauri::command]
pub async fn set_gamepad_config(state: St<'_>, id: String, profile: u32, config: gamepad::Config) -> Result<(), String> {
    state.set_gamepad_config(&id, profile, config)
}

#[tauri::command]
pub async fn gamepad_input(state: St<'_>, id: String) -> Result<gamepad::Input, String> {
    state.gamepad_input(&id)
}

#[tauri::command]
pub async fn gamepad_driver() -> Result<gamepad::drivers::Status, String> {
    tauri::async_runtime::spawn_blocking(api::gamepad_driver).await.map_err(|e| e.to_string())?
}

#[tauri::command]
pub async fn install_gamepad_driver(replace: bool) -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(move || api::install_gamepad_driver(replace)).await.map_err(|e| e.to_string())?
}

#[tauri::command]
pub async fn set_gamepad_driver_boot(on: bool) -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(move || api::set_gamepad_driver_boot(on)).await.map_err(|e| e.to_string())?
}

#[tauri::command]
pub async fn uninstall_gamepad_driver() -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(api::uninstall_gamepad_driver).await.map_err(|e| e.to_string())?
}

#[tauri::command]
pub async fn set_dev_mode(state: St<'_>, on: bool) -> Result<(), String> {
    state.set_dev_mode(on)
}

#[tauri::command]
pub async fn set_dev_log_size(state: St<'_>, size: usize) -> Result<(), String> {
    state.set_dev_log_size(size)
}

#[tauri::command]
pub async fn set_dev_panel(state: St<'_>, panel: DevPanel) -> Result<(), String> {
    state.set_dev_panel(panel)
}

#[tauri::command]
pub async fn set_dev_window(state: St<'_>, geom: DevWindow) -> Result<(), String> {
    state.set_dev_window(geom)
}

#[tauri::command]
pub async fn dev_log(state: St<'_>, id: String) -> Result<Vec<Entry>, String> {
    state.dev_log(&id)
}

#[tauri::command]
pub fn dev_commands() -> BTreeMap<String, CommandInfo> {
    api::dev_commands()
}

#[tauri::command]
pub async fn dev_device(state: St<'_>, id: String) -> Result<Option<DeviceInfo>, String> {
    state.dev_device(&id)
}

#[tauri::command]
pub async fn dev_exchange(state: St<'_>, id: String, class: u8, cmd: u8, size: u8, args: Vec<u8>) -> Result<Entry, String> {
    state.dev_exchange(&id, class, cmd, size, &args)
}

#[tauri::command]
pub async fn dev_keys(state: St<'_>, id: String, keys: Vec<u8>) -> Result<BTreeMap<u8, DevKey>, String> {
    state.dev_keys(&id, keys)
}

#[tauri::command]
pub async fn dev_macro(state: St<'_>, id: String, macro_id: u16) -> Result<Option<Vec<u8>>, String> {
    state.dev_macro(&id, macro_id)
}

#[tauri::command]
pub async fn dev_diagnostics(state: St<'_>, id: String, uncensored: bool) -> Result<String, String> {
    state.dev_diagnostics(&id, uncensored)
}
