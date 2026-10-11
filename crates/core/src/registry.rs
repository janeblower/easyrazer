//! Reference for the developer console, from `protocols/commands.json`.

use std::collections::BTreeMap;
use std::sync::OnceLock;

use serde::{Deserialize, Serialize};

use crate::packet::Command;

/// Whether a write lands in the flash, where every write wears it.
#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(untagged)]
pub enum Flash {
    Always(bool),
    /// The arg holding a slot; zero is the temporary store.
    Arg(u8),
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct CommandInfo {
    #[serde(deserialize_with = "crate::devices::command")]
    pub cmd: Command,
    /// `None`: as many as the args.
    #[serde(default)]
    pub size: Option<u8>,
    pub help: String,
    #[serde(default)]
    pub args: Vec<(String, String)>,
    #[serde(default)]
    pub example: String,
    #[serde(default)]
    pub flash: Option<Flash>,
    /// An arg and its everyday values; any other value is confirmed first.
    #[serde(default)]
    pub safe: Option<(u8, Vec<u8>)>,
}

pub fn all() -> &'static BTreeMap<String, CommandInfo> {
    static ALL: OnceLock<BTreeMap<String, CommandInfo>> = OnceLock::new();
    ALL.get_or_init(|| serde_json::from_str(include_str!("../protocols/commands.json")).expect("embedded command registry"))
}

#[cfg(test)]
mod tests {
    use std::collections::HashSet;

    use super::*;
    use crate::{analog, control, devices, macros, profiles};

    #[test]
    fn every_command_the_code_sends_is_described() {
        let p = devices::by_pid(0x0266).unwrap().protocol();
        let used = [
            analog::SET_KEY_ASSIGNMENT,
            analog::GET_KEY_ASSIGNMENT,
            control::GET_MODE,
            control::SET_MODE,
            control::GET_ACTIVE_PROFILE,
            control::SET_LED,
            control::SET_LED_BLINK,
            control::GET_FIRMWARE,
            control::GET_SERIAL,
            macros::ALLOCATE,
            macros::WRITE,
            macros::DELETE,
            macros::STATS,
            profiles::LIST,
            profiles::CREATE,
            profiles::DELETE,
            profiles::ACTIVATE,
            profiles::SET_NAME,
            profiles::GET_NAME,
            p.set_effect,
            p.get_effect,
            p.set_brightness,
            p.get_brightness,
            p.set_frame.unwrap(),
        ];
        let known: Vec<Command> = all().values().map(|c| c.cmd).collect();
        for c in used {
            assert!(known.contains(&c), "{c} is missing from commands.json");
        }
    }

    #[test]
    fn each_code_is_described_once() {
        let mut seen = HashSet::new();
        for c in all().values() {
            assert!(seen.insert((c.cmd.class, c.cmd.id)), "{} is described twice", c.cmd);
        }
    }

    #[test]
    fn flash_marks_only_writes_and_points_at_an_arg() {
        for (name, c) in all() {
            let Some(f) = c.flash else { continue };
            assert!(!c.cmd.is_get(), "{name} reads, yet is marked flash");
            if let Flash::Arg(i) = f {
                assert!((i as usize) < c.args.len(), "{name}: flash arg {i} is not an arg");
            }
        }
        assert_eq!(all()["macro_write"].flash, Some(Flash::Always(true)));
        assert_eq!(all()["set_key_assignment"].flash, Some(Flash::Arg(0)));
        assert_eq!(all()["set_mode"].flash, None);
    }

    #[test]
    fn safe_values_point_at_an_arg() {
        for (name, c) in all() {
            let Some((i, values)) = &c.safe else { continue };
            assert!((*i as usize) < c.args.len(), "{name}: safe arg {i} is not an arg");
            assert!(!values.is_empty(), "{name}: no safe values");
        }
        // Mode 01 reboots into the bootloader; the console must ask first.
        assert_eq!(all()["set_mode"].safe, Some((0, vec![0, 3])));
    }
}
