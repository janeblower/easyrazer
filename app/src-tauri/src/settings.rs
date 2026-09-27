//! EasyRazer's own settings: `%APPDATA%\EasyRazer\settings.json`.

use std::collections::BTreeMap;
use std::path::PathBuf;

use razer_core::lighting::{Look, Rgb};
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

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    /// Applied lighting, restored into the temporary store on every connect.
    pub applied: Option<Look>,
    /// Press points (fwID -> mm) applied to the live profile but not saved, restored on every connect.
    pub actuation: BTreeMap<u8, f32>,
    /// Last painted custom layout, kept while another effect is applied.
    pub custom: Option<BTreeMap<u8, Rgb>>,
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
}

impl Default for Settings {
    fn default() -> Self {
        Self { applied: None, actuation: BTreeMap::new(), custom: None, confirm_write: true, close_action: CloseAction::Ask, watch_synapse: true, autostart_offered: false, language: None }
    }
}

fn path() -> Option<PathBuf> {
    std::env::var_os("APPDATA").map(|a| PathBuf::from(a).join("EasyRazer").join("settings.json"))
}

pub fn load() -> Settings {
    path().and_then(|p| std::fs::read_to_string(p).ok()).map_or_else(Settings::default, |t| parse(&t))
}

/// A damaged file is ignored rather than keeping the app from starting.
fn parse(text: &str) -> Settings {
    serde_json::from_str(text).unwrap_or_default()
}

pub fn save(s: &Settings) -> Result<(), String> {
    let p = path().ok_or("APPDATA is not set")?;
    if let Some(dir) = p.parent() {
        std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    }
    let text = serde_json::to_string_pretty(s).map_err(|e| e.to_string())?;
    std::fs::write(p, text).map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use razer_core::lighting::Effect;

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
        assert_eq!((s.applied, s.confirm_write), (None, false));
    }

    #[test]
    fn round_trips() {
        let s = Settings {
            applied: Some(Look { effect: Effect { name: "static".into(), rgb1: Some([1, 2, 3]), ..Default::default() }, brightness: 9 }),
            actuation: [(31, 2.4), (32, 3.6)].into(),
            custom: Some([(31, [1, 2, 3]), (202, [4, 5, 6])].into()),
            confirm_write: false,
            close_action: CloseAction::Tray,
            watch_synapse: false,
            autostart_offered: true,
            language: Some("ru".into()),
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
}
