//! Reference for the developer console, from `protocols/commands.json`.

use std::collections::BTreeMap;
use std::sync::OnceLock;

use serde::{Deserialize, Serialize};

use crate::packet::Command;

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
}
