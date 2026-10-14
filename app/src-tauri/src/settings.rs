//! EasyRazer's own settings: `%APPDATA%\EasyRazer\settings.json`.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, MutexGuard};

use razer_core::lighting::Rgb;
use razer_core::macros::Event;
use razer_core::profiles::Snapshot;
use razer_core::rapid::SnapGroup;
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum CloseAction {
    #[default]
    Ask,
    Tray,
    Exit,
}

/// An unknown value must not reset the whole file.
fn lenient<'de, D: serde::Deserializer<'de>, T: serde::de::DeserializeOwned + Default>(d: D) -> Result<T, D::Error> {
    let v = serde_json::Value::deserialize(d)?;
    Ok(serde_json::from_value(v).unwrap_or_default())
}

/// Rapid Trigger of one key in mm; kept while off, so turning it back on brings the values back.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct Rapid {
    pub enabled: bool,
    pub press: f32,
    pub release: f32,
    #[serde(default)]
    pub continuous: bool,
}

/// Snap Tap of a profile; the groups stay while it is off, so Fn+LShift brings them back.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct SnapTap {
    pub enabled: bool,
    pub groups: Vec<SnapGroup>,
}

/// A macro of the app's library; the keyboard keeps only its body, under the library's id.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Macro {
    pub name: String,
    pub events: Vec<Event>,
    /// The keyboard's flash holds these events.
    pub written: bool,
}

/// A profile of the app; written to a flash slot it also works without the app.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Profile {
    /// Never reused, so a stale id from the window cannot hit another profile.
    pub id: u32,
    pub name: String,
    /// Flash slot 1–5 the profile is written to.
    pub slot: Option<u8>,
    #[serde(with = "crate::stored::snapshot")]
    pub data: Snapshot,
    /// Rapid Trigger per fwID; works only in driver mode.
    #[serde(with = "crate::stored::grouped")]
    pub rapid: BTreeMap<u8, Rapid>,
    /// Works only in driver mode.
    pub snap_tap: SnapTap,
    /// Continuous Rapid Trigger of a firmware that runs it, one switch for the whole profile;
    /// the host engine reads `Rapid::continuous` per key instead.
    pub continuous: bool,
    /// Last painted custom layout, kept while another effect is applied.
    #[serde(with = "crate::stored::colors")]
    pub custom: Option<BTreeMap<u8, Rgb>>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Dock {
    Left,
    Right,
    Bottom,
    Window,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct DevPanel {
    pub open: bool,
    pub dock: Dock,
    /// Logical px across the dock.
    pub size: u32,
}

impl Default for DevPanel {
    fn default() -> Self {
        Self { open: true, dock: Dock::Right, size: 420 }
    }
}

/// Logical px of the separate developer window.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct DevWindow {
    pub x: f64,
    pub y: f64,
    pub w: f64,
    pub h: f64,
}

pub const DEV_LOG_SIZE: usize = 10_000;
/// Bounds of `dev_log_size`: entries hold whole reports, and every window keeps a copy.
pub const DEV_LOG_SIZES: std::ops::RangeInclusive<usize> = 1000..=50_000;

/// Settings from before per-model storage hold the profiles of the only model supported then.
const LEGACY_MODEL: u16 = 0x0266;

/// Profiles, macros and mode of one physical keyboard.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct DeviceData {
    pub pid: u16,
    /// Shown instead of the model's name.
    pub name: Option<String>,
    /// The host engine types instead of the firmware.
    pub driver_mode: bool,
    /// Order of the list and of Fn+Menu.
    pub profiles: Vec<Profile>,
    pub loaded: Option<u32>,
    #[serde(with = "crate::stored::slots")]
    pub slots: BTreeMap<u8, Snapshot>,
    pub next_profile_id: u32,
    pub macros: BTreeMap<u16, Macro>,
    /// The profiles from before the Snap Tap switch was a binding have it on Fn+LShift.
    pub snap_tap_bound: bool,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    /// By firmware serial, see [`device_id`].
    pub devices: BTreeMap<String, DeviceData>,
    /// By PID: data saved before it was kept per keyboard, until a keyboard of that model claims it.
    pub legacy: BTreeMap<u16, DeviceData>,
    /// Ask before writing the keyboard's flash.
    pub confirm_write: bool,
    #[serde(deserialize_with = "lenient")]
    pub close_action: CloseAction,
    /// Off: Synapse is taken as absent and `tasklist` is never run.
    pub watch_synapse: bool,
    /// The first-run autostart question has been answered.
    pub autostart_offered: bool,
    /// Interface language; `None` until the window picks one from the system.
    pub language: Option<String>,
    pub dev_mode: bool,
    /// Entries the developer log keeps while the mode is on.
    pub dev_log_size: usize,
    /// RAM writes to keyboard models nobody has checked on hardware yet.
    pub unverified_writes: bool,
    /// `None` until the mode is first switched on.
    #[serde(deserialize_with = "lenient")]
    pub dev_panel: Option<DevPanel>,
    #[serde(deserialize_with = "lenient")]
    pub dev_window: Option<DevWindow>,
    /// The keyboard picked last; the window starts on it.
    pub last_device: Option<String>,
}

impl Default for Settings {
    fn default() -> Self {
        Self { devices: BTreeMap::new(), legacy: BTreeMap::new(), confirm_write: true, close_action: CloseAction::Ask, watch_synapse: true, autostart_offered: false, language: None, dev_mode: false, dev_log_size: DEV_LOG_SIZE, unverified_writes: false, dev_panel: None, dev_window: None, last_device: None }
    }
}

/// Where a keyboard's data is kept: its firmware serial, or its model when the serial is empty.
pub fn device_id(pid: u16, serial: &str) -> String {
    match serial.trim() {
        "" => format!("{pid:04x}:noserial"),
        s => s.to_string(),
    }
}

/// The layout before per-keyboard data: one model's data at the top, the others by PID.
#[derive(Deserialize, Default)]
#[serde(default)]
struct V1 {
    model: Option<u16>,
    other_models: BTreeMap<u16, DeviceData>,
    profiles: Vec<Profile>,
    loaded: Option<u32>,
    slots: BTreeMap<u8, Snapshot>,
    next_profile_id: u32,
    macros: BTreeMap<u16, Macro>,
    driver_mode: bool,
    snap_tap_bound: bool,
}

/// Moves data of the old layout in `text` to `legacy`; a file already in the new layout is left as it is.
/// `None` when the old data is damaged.
fn migrate(mut s: Settings, text: &str) -> Option<Settings> {
    if !s.devices.is_empty() || !s.legacy.is_empty() {
        return Some(s);
    }
    let v1 = serde_json::from_str::<V1>(text).ok()?;
    let top = DeviceData {
        pid: 0,
        name: None,
        driver_mode: v1.driver_mode,
        profiles: v1.profiles,
        loaded: v1.loaded,
        slots: v1.slots,
        next_profile_id: v1.next_profile_id,
        macros: v1.macros,
        snap_tap_bound: v1.snap_tap_bound,
    };
    for (pid, d) in v1.other_models.into_iter().chain([(v1.model.unwrap_or(LEGACY_MODEL), top)]) {
        if d.profiles.is_empty() && d.macros.is_empty() && d.slots.is_empty() {
            continue;
        }
        s.legacy.insert(pid, DeviceData { pid, driver_mode: v1.driver_mode, ..d });
    }
    Some(s)
}

/// The old file stays next to the new one: a downgrade or a bad migration must not cost the app-only profiles.
fn backup(p: &Path, s: &Settings, text: &str) {
    let v1 = p.with_file_name("settings.json.v1");
    if s.devices.is_empty() && !s.legacy.is_empty() && !v1.exists() {
        let _ = std::fs::write(v1, text);
    }
}

static NO_DATA: DeviceData = DeviceData { pid: 0, name: None, driver_mode: false, profiles: Vec::new(), loaded: None, slots: BTreeMap::new(), next_profile_id: 0, macros: BTreeMap::new(), snap_tap_bound: false };

/// The app's settings, shared by every keyboard and the app-wide commands.
pub type Shared = Arc<Mutex<Settings>>;

/// A writer that panicked left the settings as they were before its save, so poisoning is ignored.
pub fn lock(s: &Shared) -> MutexGuard<'_, Settings> {
    s.lock().unwrap_or_else(|e| e.into_inner())
}

impl Settings {
    /// The data of keyboard `id`; empty when there is none.
    pub fn device(&self, id: Option<&str>) -> &DeviceData {
        id.and_then(|id| self.devices.get(id)).unwrap_or(&NO_DATA)
    }

    #[allow(dead_code, reason = "used again for unconnected keyboards")]
    pub fn device_mut(&mut self, id: Option<&str>) -> Option<&mut DeviceData> {
        self.devices.get_mut(id?)
    }

    /// Creates the data of keyboard `id` on first sight; the first keyboard of a model takes its legacy data.
    pub fn claim(&mut self, id: &str, pid: u16) -> bool {
        if self.devices.contains_key(id) {
            return false;
        }
        let d = self.legacy.remove(&pid).unwrap_or_else(|| DeviceData { pid, ..Default::default() });
        self.devices.insert(id.to_string(), d);
        true
    }

    pub fn put(&mut self, id: &str, d: &DeviceData) {
        self.devices.insert(id.to_string(), d.clone());
    }
}

impl DeviceData {
    /// Binds the Snap Tap switch to Fn+LShift in every profile, once: one removed later stays removed.
    pub fn bind_snap_tap_once(&mut self) {
        if self.snap_tap_bound {
            return;
        }
        for p in &mut self.profiles {
            p.data.bind_snap_tap();
        }
        self.snap_tap_bound = true;
    }

    pub fn profile(&self, id: u32) -> Option<&Profile> {
        self.profiles.iter().find(|p| p.id == id)
    }

    pub fn profile_mut(&mut self, id: u32) -> Option<&mut Profile> {
        self.profiles.iter_mut().find(|p| p.id == id)
    }

    pub fn loaded_profile(&self) -> Option<&Profile> {
        self.loaded.and_then(|id| self.profile(id))
    }

    pub fn loaded_mut(&mut self) -> Option<&mut Profile> {
        let id = self.loaded?;
        self.profile_mut(id)
    }

    pub fn slots_used(&self) -> usize {
        self.profiles.iter().filter(|p| p.slot.is_some()).count()
    }

    pub fn new_profile_id(&mut self) -> u32 {
        self.next_profile_id += 1;
        self.next_profile_id
    }
}

pub fn dir() -> Option<PathBuf> {
    #[cfg(test)]
    return Some(TEST_DIR.with(|d| d.0.clone()));
    #[cfg(not(test))]
    std::env::var_os("APPDATA").map(|a| PathBuf::from(a).join("EasyRazer"))
}

/// Every test thread saves into a directory of its own, removed when the thread ends.
#[cfg(test)]
struct TestDir(PathBuf);

#[cfg(test)]
impl Drop for TestDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
        let _ = std::fs::remove_file(&self.0);
    }
}

#[cfg(test)]
thread_local! {
    static TEST_DIR: TestDir = {
        static NEXT: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);
        let n = NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        TestDir(std::env::temp_dir().join(format!("easyrazer-test-{}-{n}", std::process::id())))
    };
}

fn path() -> Option<PathBuf> {
    dir().map(|d| d.join("settings.json"))
}

/// A damaged file is set aside, not overwritten: the next save would destroy the only copy of the app-only profiles.
pub fn load() -> Settings {
    path().map_or_else(Settings::default, |p| load_from(&p))
}

fn load_from(p: &Path) -> Settings {
    let Ok(text) = std::fs::read_to_string(p) else { return Settings::default() };
    let Some(s) = serde_json::from_str(&text).ok().and_then(|s| migrate(s, &text)) else {
        let secs = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_secs());
        let _ = std::fs::rename(p, p.with_file_name(format!("settings.json.bad-{secs}")));
        return Settings::default();
    };
    backup(p, &s, &text);
    s
}

#[cfg(test)]
fn parse(text: &str) -> Settings {
    serde_json::from_str(text).unwrap_or_default()
}

pub fn save(s: &Settings) -> Result<(), String> {
    let p = path().ok_or("APPDATA is not set")?;
    if let Some(dir) = p.parent() {
        std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    }
    let text = crate::stored::to_text(s)?;
    let tmp = p.with_file_name("settings.json.tmp");
    std::fs::write(&tmp, text).map_err(|e| e.to_string())?;
    std::fs::rename(&tmp, &p).map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use razer_core::binding;
    use razer_core::profiles::RawKey;
    use razer_core::rapid::SnapRule;

    fn snap(keys: &[u8]) -> Snapshot {
        let raw = |k| RawKey { thr_low: 0, thr_high: 0, fn_id: 2, fn_data: vec![0, k] };
        Snapshot { normal: keys.iter().map(|&k| (k, raw(k))).collect(), hypershift: keys.iter().map(|&k| (k, raw(k))).collect(), look: None }
    }

    fn profile(id: u32, slot: Option<u8>) -> Profile {
        Profile { id, name: format!("p{id}"), slot, data: snap(&[31, 32]), rapid: BTreeMap::new(), snap_tap: SnapTap::default(), continuous: false, custom: None }
    }

    const OLD: &str = r#"{"model":614,"profiles":[{"id":1,"name":"p1","slot":1,"data":{"normal":{},"hypershift":{},"look":null}}],
        "loaded":1,"next_profile_id":1,"driver_mode":true,
        "other_models":{"678":{"profiles":[{"id":7,"name":"p7","slot":null,"data":{"normal":{},"hypershift":{},"look":null}}],"next_profile_id":7}}}"#;

    #[test]
    fn device_id_is_the_serial_or_the_model() {
        assert_eq!(device_id(0x0266, " PM2322F51200078 "), "PM2322F51200078");
        assert_eq!(device_id(0x0266, ""), "0266:noserial");
    }

    #[test]
    fn old_files_move_to_legacy_by_model() {
        let s = migrate(parse(OLD), OLD).unwrap();
        assert!(s.devices.is_empty());
        let v2 = &s.legacy[&0x0266];
        assert_eq!((v2.pid, v2.profiles[0].id, v2.loaded, v2.driver_mode), (0x0266, 1, Some(1), true));
        assert_eq!((s.legacy[&678].pid, s.legacy[&678].profiles[0].id), (678, 7));
        assert!(s.legacy[&678].driver_mode, "the mode was global");
    }

    #[test]
    fn files_from_before_per_model_data_belong_to_the_v2_analog() {
        let text = r#"{"profiles": [{"id": 3, "name": "a", "slot": 2, "data": {"normal": {}, "hypershift": {}, "look": null}}]}"#;
        let s = migrate(parse(text), text).unwrap();
        assert_eq!((s.legacy[&LEGACY_MODEL].profiles[0].id, s.legacy[&LEGACY_MODEL].profiles[0].slot), (3, Some(2)));
    }

    #[test]
    fn a_model_without_data_is_not_kept() {
        let text = r#"{"model":678,"driver_mode":true}"#;
        assert!(migrate(parse(text), text).unwrap().legacy.is_empty());
    }

    #[test]
    fn first_keyboard_of_a_model_claims_its_legacy() {
        let mut s = migrate(parse(OLD), OLD).unwrap();
        assert!(s.claim("PM1", 0x0266));
        assert_eq!(s.devices["PM1"].profiles[0].id, 1);
        assert!(!s.legacy.contains_key(&0x0266));
        assert!(!s.claim("PM1", 0x0266), "already there");
    }

    #[test]
    fn second_keyboard_of_a_model_starts_empty() {
        let mut s = migrate(parse(OLD), OLD).unwrap();
        s.claim("PM1", 0x0266);
        assert!(s.claim("PM2", 0x0266));
        assert!(s.devices["PM2"].profiles.is_empty());
        assert_eq!((s.devices["PM2"].pid, s.devices["PM1"].profiles.len()), (0x0266, 1));
    }

    #[test]
    fn legacy_survives_a_round_trip() {
        let mut s = migrate(parse(OLD), OLD).unwrap();
        s.claim("PM1", 0x0266);
        let text = serde_json::to_string(&s).unwrap();
        let back = migrate(parse(&text), &text).unwrap();
        assert_eq!(back, s);
        assert_eq!(back.legacy[&678].profiles[0].id, 7);
    }

    #[test]
    fn new_files_are_not_migrated_again() {
        let mut s = Settings::default();
        s.claim("PM1", 0x0266);
        let text = serde_json::to_string(&s).unwrap();
        assert_eq!(migrate(parse(&text), &text).unwrap(), s);
    }

    #[test]
    fn a_damaged_old_file_is_set_aside() {
        let dir = std::env::temp_dir().join(format!("easyrazer-damaged-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let p = dir.join("settings.json");
        let text = r#"{"profiles":[{"id":1,"name":"p1","slot":300}],"confirm_write":false}"#;
        std::fs::write(&p, text).unwrap();
        assert_eq!(load_from(&p), Settings::default());
        assert!(!p.exists(), "the next save must not overwrite the only copy");
        let bad: Vec<_> = std::fs::read_dir(&dir).unwrap().filter_map(|e| e.ok()).map(|e| e.file_name().into_string().unwrap()).collect();
        assert!(bad.iter().any(|n| n.starts_with("settings.json.bad-")), "{bad:?}");
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn backup_is_written_once() {
        let dir = std::env::temp_dir().join(format!("easyrazer-backup-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let p = dir.join("settings.json");
        let v1 = dir.join("settings.json.v1");
        let s = migrate(parse(OLD), OLD).unwrap();
        backup(&p, &s, OLD);
        assert_eq!(std::fs::read_to_string(&v1).unwrap(), OLD);
        backup(&p, &s, "{}");
        assert_eq!(std::fs::read_to_string(&v1).unwrap(), OLD, "never overwritten");
        let mut claimed = s.clone();
        claimed.claim("PM1", 0x0266);
        std::fs::remove_file(&v1).unwrap();
        backup(&p, &claimed, OLD);
        assert!(!v1.exists(), "new layout: nothing to back up");
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn snap_tap_switch_is_bound_once() {
        let mut p = profile(1, None);
        p.data = snap(&[31, 44]);
        let (fn_id, fn_data) = binding::encode(binding::factory(44)).unwrap();
        p.data.hypershift.insert(44, RawKey { thr_low: 0, thr_high: 0, fn_id, fn_data });
        let mut s = DeviceData { profiles: vec![p], ..Default::default() };
        s.bind_snap_tap_once();
        assert_eq!(s.profiles[0].data.hypershift[&44].fn_data, [binding::SNAP_TAP_CODE]);
        let plain = s.profiles[0].data.normal[&44].clone();
        s.profiles[0].data.hypershift.insert(44, plain);
        s.bind_snap_tap_once();
        assert_eq!(s.profiles[0].data.hypershift[&44].fn_id, 2, "removed later, it stays removed");
        assert!(serde_json::from_str::<DeviceData>(&serde_json::to_string(&s).unwrap()).unwrap().snap_tap_bound);
    }

    #[test]
    fn put_replaces_only_its_keyboard() {
        let mut s = Settings::default();
        s.claim("A", 0x0266);
        s.claim("B", 0x0266);
        s.legacy.insert(0x02A6, DeviceData { pid: 0x02A6, profiles: vec![profile(9, None)], ..Default::default() });
        s.put("A", &DeviceData { pid: 0x0266, profiles: vec![profile(1, None)], ..Default::default() });
        assert_eq!(s.devices["A"].profiles[0].id, 1);
        assert!(s.devices["B"].profiles.is_empty());
        assert_eq!(s.legacy[&0x02A6].profiles[0].id, 9);
    }

    #[test]
    fn profiles_round_trip() {
        let d = DeviceData { pid: 0x0266, profiles: vec![profile(1, Some(1)), profile(2, None)], loaded: Some(2), slots: [(1, snap(&[31]))].into(), next_profile_id: 2, ..Default::default() };
        let s = Settings { devices: [("PM1".into(), d)].into(), ..Default::default() };
        assert_eq!(parse(&serde_json::to_string(&s).unwrap()), s);
    }

    #[test]
    fn profile_ids_are_never_reused() {
        let mut s = DeviceData::default();
        assert_eq!((s.new_profile_id(), s.new_profile_id()), (1, 2));
    }

    #[test]
    fn broken_or_empty_file_gives_defaults() {
        for text in ["", "{", "[]", "{}", "{\"applied\": 5}"] {
            assert_eq!(parse(text), Settings::default(), "{text:?}");
        }
        assert!(Settings::default().confirm_write);
    }

    #[test]
    fn missing_fields_keep_their_defaults() {
        let s = parse("{\"confirm_write\": false}");
        assert_eq!((s.watch_synapse, s.confirm_write), (true, false));
    }

    #[test]
    fn round_trips() {
        let s = Settings {
            devices: [(
                "PM1".into(),
                DeviceData {
                    pid: 0x0266,
                    name: Some("Desk".into()),
                    driver_mode: true,
                    profiles: vec![profile(1, Some(1))],
                    loaded: Some(1),
                    slots: BTreeMap::new(),
                    next_profile_id: 1,
                    macros: [(0x8000, Macro { name: "ab".into(), events: vec![Event::Key { key: 31, down: true }, Event::Delay { ms: 20 }], written: true })].into(),
                    snap_tap_bound: true,
                },
            )]
            .into(),
            legacy: [(0x02A6, DeviceData { pid: 0x02A6, profiles: vec![profile(2, None)], ..Default::default() })].into(),
            confirm_write: false,
            close_action: CloseAction::Tray,
            watch_synapse: false,
            autostart_offered: true,
            language: Some("ru".into()),
            dev_mode: true,
            dev_log_size: 2000,
            unverified_writes: true,
            dev_panel: Some(DevPanel { open: false, dock: Dock::Bottom, size: 300 }),
            dev_window: Some(DevWindow { x: 10.0, y: 20.0, w: 800.0, h: 600.0 }),
            last_device: Some("PM1".into()),
        };
        assert_eq!(parse(&serde_json::to_string(&s).unwrap()), s);
    }

    #[test]
    fn new_fields_have_defaults() {
        let s = parse("{}");
        assert_eq!((s.close_action, s.watch_synapse, s.autostart_offered), (CloseAction::Ask, true, false));
    }

    #[test]
    fn unknown_close_action_is_ask_and_keeps_the_rest() {
        let s = parse("{\"close_action\": \"minimize\", \"confirm_write\": false, \"watch_synapse\": false}");
        assert_eq!((s.close_action, s.confirm_write, s.watch_synapse), (CloseAction::Ask, false, false));
    }

    #[test]
    fn an_unreadable_dev_panel_is_dropped_and_keeps_the_rest() {
        let s = parse("{\"dev_panel\": {\"open\": true, \"dock\": \"nowhere\", \"size\": 1}, \"confirm_write\": false}");
        assert_eq!((s.dev_panel, s.confirm_write), (None, false));
    }

    #[test]
    fn reads_close_action() {
        assert_eq!(parse("{\"close_action\": \"tray\"}").close_action, CloseAction::Tray);
        assert_eq!(parse("{\"close_action\": \"exit\"}").close_action, CloseAction::Exit);
    }

    #[test]
    fn profile_without_snap_tap_reads_as_off() {
        let p: Profile = serde_json::from_str(r#"{"id": 1, "name": "p"}"#).unwrap();
        assert_eq!(p.snap_tap, SnapTap::default());
        assert!(!p.snap_tap.enabled);
    }

    #[test]
    fn snap_tap_groups_in_the_profile() {
        let p: Profile = serde_json::from_str(r#"{"snap_tap": {"enabled": true, "groups": [{"keys": [31, 33], "rule": "deeper"}]}}"#).unwrap();
        assert!(p.snap_tap.enabled);
        assert_eq!(p.snap_tap.groups, [SnapGroup { keys: vec![31, 33], rule: SnapRule::Deeper }]);
    }

    #[test]
    fn names_and_last_device_round_trip() {
        let mut s = Settings::default();
        s.devices.insert("K".into(), DeviceData { pid: 0x0266, name: Some("Desk".into()), ..Default::default() });
        s.last_device = Some("K".into());
        let back: Settings = serde_json::from_str(&serde_json::to_string(&s).unwrap()).unwrap();
        assert_eq!(back.devices["K"].name.as_deref(), Some("Desk"));
        assert_eq!(back.last_device.as_deref(), Some("K"));
    }
    #[test]
    fn saves_a_profile_compactly() {
        let layer = || (1..=103u8).filter_map(|k| Some((k, crate::stored::factory(k)?))).collect::<BTreeMap<_, _>>();
        let snap = Snapshot { normal: layer(), hypershift: layer(), look: None };
        let rapid = (1..=103).map(|k| (k, Rapid { enabled: true, press: 1.0, release: 1.0, continuous: false })).collect();
        let custom = Some((1..=103).map(|k| (k, [255, 64, 128])).collect());
        let p = Profile { id: 1, name: "Игры".into(), slot: Some(1), data: snap.clone(), rapid, snap_tap: SnapTap::default(), continuous: false, custom };
        let d = DeviceData { pid: 0x0266, profiles: vec![p], slots: BTreeMap::from([(1, snap)]), ..Default::default() };
        let s = Settings { devices: BTreeMap::from([("K".into(), d)]), ..Default::default() };
        let text = crate::stored::to_text(&s).unwrap();
        assert!(text.lines().count() < 80, "{text}");
        assert_eq!(parse(&text), s);
    }
}
