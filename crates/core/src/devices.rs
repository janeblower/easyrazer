//! Device and protocol descriptions embedded from `devices/*.json` and `protocols/*.json`.

use std::collections::BTreeMap;
use std::sync::OnceLock;

use serde::{Deserialize, Deserializer};

use crate::packet::Command;

const DEVICES: &[&str] = &[include_str!("../devices/1532-0266.json")];
const PROTOCOLS: &[(&str, &str)] = &[("extended", include_str!("../protocols/extended.json"))];

#[derive(Debug, Deserialize)]
pub struct DeviceSpec {
    pub name: String,
    #[serde(deserialize_with = "hex_u16")]
    pub vid: u16,
    #[serde(deserialize_with = "hex_u16")]
    pub pid: u16,
    pub control_interface: i32,
    #[serde(deserialize_with = "hex_u8")]
    pub tid: u8,
    pub lighting: Lighting,
    pub lamp_array: Option<LampArray>,
}

#[derive(Debug, Deserialize)]
pub struct Lighting {
    pub protocol: String,
    #[serde(deserialize_with = "hex_u8")]
    pub led: u8,
    pub effects: Vec<String>,
}

#[derive(Debug, Deserialize)]
pub struct LampArray {
    /// Feature report carrying AutonomousMode.
    #[serde(deserialize_with = "hex_u8")]
    pub control_report: u8,
}

#[derive(Debug, Deserialize)]
pub struct Protocol {
    #[serde(deserialize_with = "command")]
    pub set_effect: Command,
    #[serde(deserialize_with = "command")]
    pub get_effect: Command,
    #[serde(deserialize_with = "command")]
    pub set_brightness: Command,
    #[serde(deserialize_with = "command")]
    pub get_brightness: Command,
    /// Per-key colors of the `custom` effect: `store 00 row first last RGB…`.
    #[serde(default, deserialize_with = "opt_command")]
    pub set_frame: Option<Command>,
    pub stores: Stores,
    pub effects: BTreeMap<String, EffectTemplate>,
}

#[derive(Debug, Deserialize)]
pub struct Stores {
    pub temporary: u8,
    pub saved: u8,
}

/// Effect bytes after `store led`: hex literals and `{rgb1}`, `{rgb2}`, `{dir}`, `{speed}`.
#[derive(Debug, Deserialize)]
pub struct EffectTemplate {
    pub bytes: String,
    #[serde(default)]
    pub dir: BTreeMap<String, u8>,
    pub speed: Option<[u8; 2]>,
    /// `"low"` when a lower speed value animates faster.
    pub fast: Option<String>,
}

pub fn all() -> &'static [DeviceSpec] {
    static ALL: OnceLock<Vec<DeviceSpec>> = OnceLock::new();
    ALL.get_or_init(|| DEVICES.iter().map(|s| serde_json::from_str(s).expect("embedded device description")).collect())
}

pub fn by_pid(pid: u16) -> Option<&'static DeviceSpec> {
    all().iter().find(|d| d.pid == pid)
}

fn protocols() -> &'static BTreeMap<&'static str, Protocol> {
    static ALL: OnceLock<BTreeMap<&'static str, Protocol>> = OnceLock::new();
    ALL.get_or_init(|| {
        PROTOCOLS.iter().map(|(name, s)| (*name, serde_json::from_str(s).expect("embedded protocol"))).collect()
    })
}

impl DeviceSpec {
    pub fn protocol(&self) -> &'static Protocol {
        &protocols()[self.lighting.protocol.as_str()]
    }
}

fn hex_u8<'de, D: Deserializer<'de>>(d: D) -> Result<u8, D::Error> {
    u8::from_str_radix(&String::deserialize(d)?, 16).map_err(serde::de::Error::custom)
}

fn hex_u16<'de, D: Deserializer<'de>>(d: D) -> Result<u16, D::Error> {
    u16::from_str_radix(&String::deserialize(d)?, 16).map_err(serde::de::Error::custom)
}

fn opt_command<'de, D: Deserializer<'de>>(d: D) -> Result<Option<Command>, D::Error> {
    command(d).map(Some)
}

fn command<'de, D: Deserializer<'de>>(d: D) -> Result<Command, D::Error> {
    let s = String::deserialize(d)?;
    let (class, id) = s.split_once(':').ok_or_else(|| serde::de::Error::custom(format!("{s}: expected CC:II")))?;
    let byte = |h: &str| u8::from_str_radix(h, 16).map_err(serde::de::Error::custom);
    Ok(Command::new(byte(class)?, byte(id)?))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_description_uses_known_protocol_effects() {
        for d in all() {
            let p = d.protocol();
            for name in &d.lighting.effects {
                assert!(p.effects.contains_key(name), "{}: effect {name} is not in protocol", d.name);
            }
        }
    }

    #[test]
    fn templates_are_well_formed() {
        for d in all() {
            for (name, t) in &d.protocol().effects {
                for tok in t.bytes.split_whitespace() {
                    match tok {
                        "{rgb1}" | "{rgb2}" => {}
                        "{dir}" => assert!(!t.dir.is_empty(), "{name}: {{dir}} without dir map"),
                        "{speed}" => assert!(t.speed.is_some(), "{name}: {{speed}} without range"),
                        hex => assert!(u8::from_str_radix(hex, 16).is_ok(), "{name}: bad byte {hex}"),
                    }
                }
            }
        }
    }

    #[test]
    fn pids_are_unique() {
        let mut pids: Vec<u16> = all().iter().map(|d| d.pid).collect();
        pids.sort();
        pids.dedup();
        assert_eq!(pids.len(), all().len());
    }

    #[test]
    fn finds_huntsman_v2_analog_by_pid() {
        let d = by_pid(0x0266).unwrap();
        assert_eq!(d.name, "Razer Huntsman V2 Analog");
        assert_eq!((d.vid, d.control_interface, d.tid, d.lighting.led), (0x1532, 3, 0x1F, 0x05));
        assert_eq!(d.lamp_array.as_ref().unwrap().control_report, 6);
        assert_eq!(d.protocol().set_effect, Command::new(0x0F, 0x02));
        assert!(by_pid(0xFFFF).is_none());
    }
}
