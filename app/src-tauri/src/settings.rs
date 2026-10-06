//! EasyRazer's own settings: `%APPDATA%\EasyRazer\settings.json`.

use std::collections::BTreeMap;
use std::path::PathBuf;

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
fn lenient<'de, D: serde::Deserializer<'de>>(d: D) -> Result<CloseAction, D::Error> {
    let v = serde_json::Value::deserialize(d)?;
    Ok(serde_json::from_value(v).unwrap_or_default())
}

/// Rapid Trigger of one key in mm; kept while off, so turning it back on brings the values back.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct Rapid {
    pub enabled: bool,
    pub press: f32,
    pub release: f32,
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
    pub data: Snapshot,
    /// Rapid Trigger per fwID; works only in driver mode.
    pub rapid: BTreeMap<u8, Rapid>,
    /// Works only in driver mode.
    pub snap_tap: SnapTap,
    /// Last painted custom layout, kept while another effect is applied.
    pub custom: Option<BTreeMap<u8, Rgb>>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    /// Order of the list and of Fn+Menu.
    pub profiles: Vec<Profile>,
    pub loaded: Option<u32>,
    /// What each flash slot holds, as last read or written.
    pub slots: BTreeMap<u8, Snapshot>,
    pub next_profile_id: u32,
    pub macros: BTreeMap<u16, Macro>,
    /// The host engine types instead of the firmware.
    pub driver_mode: bool,
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
    /// The profiles from before the Snap Tap switch was a binding have it on Fn+LShift.
    pub snap_tap_bound: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Self { profiles: Vec::new(), loaded: None, slots: BTreeMap::new(), next_profile_id: 0, macros: BTreeMap::new(), driver_mode: false, confirm_write: true, close_action: CloseAction::Ask, watch_synapse: true, autostart_offered: false, language: None, snap_tap_bound: false }
    }
}

impl Settings {
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

    pub fn new_profile_id(&mut self) -> u32 {
        self.next_profile_id += 1;
        self.next_profile_id
    }
}

pub fn dir() -> Option<PathBuf> {
    std::env::var_os("APPDATA").map(|a| PathBuf::from(a).join("EasyRazer"))
}

fn path() -> Option<PathBuf> {
    dir().map(|d| d.join("settings.json"))
}

/// A damaged file is set aside, not overwritten: the next save would destroy the only copy of the app-only profiles.
pub fn load() -> Settings {
    let Some(p) = path() else { return Settings::default() };
    let Ok(text) = std::fs::read_to_string(&p) else { return Settings::default() };
    serde_json::from_str(&text).unwrap_or_else(|_| {
        let secs = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_secs());
        let _ = std::fs::rename(&p, p.with_file_name(format!("settings.json.bad-{secs}")));
        Settings::default()
    })
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
    let text = serde_json::to_string_pretty(s).map_err(|e| e.to_string())?;
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
        Profile { id, name: format!("p{id}"), slot, data: snap(&[31, 32]), rapid: BTreeMap::new(), snap_tap: SnapTap::default(), custom: None }
    }

    #[test]
    fn snap_tap_switch_is_bound_once() {
        let mut p = profile(1, None);
        p.data = snap(&[31, 44]);
        let (fn_id, fn_data) = binding::encode(binding::factory(44)).unwrap();
        p.data.hypershift.insert(44, RawKey { thr_low: 0, thr_high: 0, fn_id, fn_data });
        let mut s = Settings { profiles: vec![p], ..Default::default() };
        s.bind_snap_tap_once();
        assert_eq!(s.profiles[0].data.hypershift[&44].fn_data, [binding::SNAP_TAP_CODE]);
        let plain = s.profiles[0].data.normal[&44].clone();
        s.profiles[0].data.hypershift.insert(44, plain);
        s.bind_snap_tap_once();
        assert_eq!(s.profiles[0].data.hypershift[&44].fn_id, 2, "removed later, it stays removed");
        assert!(parse(&serde_json::to_string(&s).unwrap()).snap_tap_bound);
    }

    #[test]
    fn profiles_round_trip() {
        let s = Settings { profiles: vec![profile(1, Some(1)), profile(2, None)], loaded: Some(2), slots: [(1, snap(&[31]))].into(), next_profile_id: 2, ..Default::default() };
        assert_eq!(parse(&serde_json::to_string(&s).unwrap()), s);
    }

    #[test]
    fn a_profile_missing_new_fields_still_parses() {
        let s = parse(r#"{"profiles": [{"id": 3, "name": "a", "slot": 2, "data": {"normal": {}, "hypershift": {}, "look": null}}]}"#);
        assert_eq!((s.profiles.len(), s.profiles[0].id, s.profiles[0].slot), (1, 3, Some(2)));
    }

    #[test]
    fn profile_ids_are_never_reused() {
        let mut s = Settings::default();
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
        assert_eq!((s.driver_mode, s.confirm_write), (false, false));
    }

    #[test]
    fn round_trips() {
        let s = Settings {
            profiles: vec![profile(1, Some(1))],
            loaded: Some(1),
            slots: BTreeMap::new(),
            next_profile_id: 1,
            macros: [(0x8000, Macro { name: "ab".into(), events: vec![Event::Key { key: 31, down: true }, Event::Delay { ms: 20 }], written: true })].into(),
            driver_mode: true,
            confirm_write: false,
            close_action: CloseAction::Tray,
            watch_synapse: false,
            autostart_offered: true,
            language: Some("ru".into()),
            snap_tap_bound: true,
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
}
