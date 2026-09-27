//! EasyRazer's own settings: `%APPDATA%\EasyRazer\lighting.json`.

use std::path::PathBuf;

use razer_core::lighting::Look;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    /// Applied lighting, restored into the temporary store on every connect.
    pub applied: Option<Look>,
    /// Ask before writing the keyboard's flash.
    pub confirm_write: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Self { applied: None, confirm_write: true }
    }
}

fn path() -> Option<PathBuf> {
    std::env::var_os("APPDATA").map(|a| PathBuf::from(a).join("EasyRazer").join("lighting.json"))
}

pub fn load() -> Settings {
    path().and_then(|p| std::fs::read_to_string(p).ok()).map_or_else(Settings::default, |t| parse(&t))
}

/// A damaged file is ignored rather than keeping the app from starting.
fn parse(text: &str) -> Settings {
    serde_json::from_str(text).unwrap_or_default()
}

pub fn save(s: &Settings) -> Result<(), String> {
    let p = path().ok_or("APPDATA не задан")?;
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
            confirm_write: false,
        };
        assert_eq!(parse(&serde_json::to_string(&s).unwrap()), s);
    }
}
