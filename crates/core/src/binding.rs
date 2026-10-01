//! What a key does in the normal layer: the `fnId` and data of its assignment.

use serde::{Deserialize, Serialize};

use crate::actuation::{self, Outcome};
use crate::rapid::Media;
use crate::transport::{Error, Transport};

const FN_DISABLED: u8 = 0x00;
const FN_MOUSE: u8 = 0x01;
const FN_KEY: u8 = 0x02;
const FN_MACRO_TIMES: u8 = 0x03;
const FN_MACRO_HOLD: u8 = 0x04;
const FN_MACRO_TOGGLE: u8 = 0x05;
const FN_CONSUMER: u8 = 0x0A;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum Action {
    Disabled,
    /// `key` is a fwID; `mods` is the HID modifier byte, bit 0 left Ctrl to bit 7 right GUI.
    Key { key: u8, mods: u8 },
    Mouse { button: Mouse },
    Media { media: Media },
    /// Plays the body stored under `id`; `count` passes for `Times`.
    Macro { id: u16, mode: MacroMode, count: u8 },
}

/// A press during `Times` is ignored; `Hold` and `Toggle` finish the pass they stop in.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum MacroMode {
    Times,
    Hold,
    Toggle,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Mouse {
    Left,
    Right,
    Middle,
    Back,
    Forward,
    WheelUp,
    WheelDown,
}

/// fwIDs of the modifier keys by bit of the HID modifier byte.
pub const MODIFIERS: [u8; 8] = [58, 44, 60, 127, 64, 57, 62, 59];

/// fwID to HID keyboard usage; the factory binding of every key is `02 [00 usage]`.
const USAGES: &[(u8, u8)] = &[
    (1, 0x35), (2, 0x1E), (3, 0x1F), (4, 0x20), (5, 0x21), (6, 0x22), (7, 0x23), (8, 0x24), (9, 0x25), (10, 0x26),
    (11, 0x27), (12, 0x2D), (13, 0x2E), (14, 0x89), (15, 0x2A), (16, 0x2B), (17, 0x14), (18, 0x1A), (19, 0x08),
    (20, 0x15), (21, 0x17), (22, 0x1C), (23, 0x18), (24, 0x0C), (25, 0x12), (26, 0x13), (27, 0x2F), (28, 0x30),
    (29, 0x31), (30, 0x39), (31, 0x04), (32, 0x16), (33, 0x07), (34, 0x09), (35, 0x0A), (36, 0x0B), (37, 0x0D),
    (38, 0x0E), (39, 0x0F), (40, 0x33), (41, 0x34), (42, 0x32), (43, 0x28), (44, 0xE1), (45, 0x64), (46, 0x1D),
    (47, 0x1B), (48, 0x06), (49, 0x19), (50, 0x05), (51, 0x11), (52, 0x10), (53, 0x36), (54, 0x37), (55, 0x38),
    (56, 0x87), (57, 0xE5), (58, 0xE0), (59, 0xE7), (60, 0xE2), (61, 0x2C), (62, 0xE6), (64, 0xE4), (75, 0x49),
    (76, 0x4C), (79, 0x50), (80, 0x4A), (81, 0x4D), (83, 0x52), (84, 0x51), (85, 0x4B), (86, 0x4E), (89, 0x4F),
    (90, 0x53), (91, 0x5F), (92, 0x5C), (93, 0x59), (95, 0x54), (96, 0x60), (97, 0x5D), (98, 0x5A), (99, 0x62),
    (100, 0x55), (101, 0x61), (102, 0x5E), (103, 0x5B), (104, 0x63), (105, 0x56), (106, 0x57), (107, 0x67),
    (108, 0x58), (110, 0x29), (112, 0x3A), (113, 0x3B), (114, 0x3C), (115, 0x3D), (116, 0x3E), (117, 0x3F),
    (118, 0x40), (119, 0x41), (120, 0x42), (121, 0x43), (122, 0x44), (123, 0x45), (124, 0x46), (125, 0x47),
    (126, 0x48), (127, 0xE3), (129, 0x65), (150, 0x8B), (151, 0x88), (152, 0x91), (153, 0x90), (154, 0x8A),
];

const MOUSE: [(Mouse, u8); 7] = [
    (Mouse::Left, 0x01),
    (Mouse::Right, 0x02),
    (Mouse::Middle, 0x03),
    (Mouse::Back, 0x04),
    (Mouse::Forward, 0x05),
    (Mouse::WheelUp, 0x09),
    (Mouse::WheelDown, 0x0A),
];

const CONSUMER: [(Media, u16); 7] = [
    (Media::Next, 0xB5),
    (Media::Prev, 0xB6),
    (Media::Stop, 0xB7),
    (Media::Play, 0xCD),
    (Media::Mute, 0xE2),
    (Media::VolumeUp, 0xE9),
    (Media::VolumeDown, 0xEA),
];

pub(crate) fn usage(key: u8) -> Option<u8> {
    USAGES.iter().find(|&&(k, _)| k == key).map(|&(_, u)| u)
}

pub(crate) fn mouse_code(button: Mouse) -> u8 {
    MOUSE.iter().find(|&&(m, _)| m == button).map_or(0, |&(_, c)| c)
}

pub fn factory(key: u8) -> Action {
    Action::Key { key, mods: 0 }
}

/// `fnId` and data; `None` for a key without a HID usage.
pub fn encode(a: Action) -> Option<(u8, Vec<u8>)> {
    Some(match a {
        Action::Disabled => (FN_DISABLED, Vec::new()),
        Action::Key { key, mods } => (FN_KEY, vec![mods, usage(key)?]),
        Action::Mouse { button } => (FN_MOUSE, vec![mouse_code(button)]),
        Action::Media { media } => {
            let u = CONSUMER.iter().find(|&&(m, _)| m == media)?.1;
            (FN_CONSUMER, u.to_be_bytes().to_vec())
        }
        Action::Macro { id, mode, count } => {
            let [hi, lo] = id.to_be_bytes();
            match mode {
                MacroMode::Times => (FN_MACRO_TIMES, vec![hi, lo, count]),
                MacroMode::Hold => (FN_MACRO_HOLD, vec![hi, lo]),
                MacroMode::Toggle => (FN_MACRO_TOGGLE, vec![hi, lo]),
            }
        }
    })
}

/// `None` for what the app does not edit: Hypershift, profiles, service keys.
pub fn decode(fn_id: u8, data: &[u8]) -> Option<Action> {
    match (fn_id, data) {
        (FN_DISABLED, _) => Some(Action::Disabled),
        // A lone modifier bit is the modifier key itself.
        (FN_KEY, &[mods, 0]) if mods.count_ones() == 1 => Some(factory(MODIFIERS[mods.trailing_zeros() as usize])),
        (FN_KEY, &[mods, usage]) => USAGES.iter().find(|&&(_, u)| u == usage).map(|&(key, _)| Action::Key { key, mods }),
        (FN_MOUSE, &[b]) => MOUSE.iter().find(|&&(_, c)| c == b).map(|&(button, _)| Action::Mouse { button }),
        (FN_CONSUMER, &[hi, lo]) => {
            let u = u16::from_be_bytes([hi, lo]);
            CONSUMER.iter().find(|&&(_, c)| c == u).map(|&(media, _)| Action::Media { media })
        }
        (FN_MACRO_TIMES, &[hi, lo, count]) => Some(Action::Macro { id: u16::from_be_bytes([hi, lo]), mode: MacroMode::Times, count }),
        (FN_MACRO_HOLD, &[hi, lo]) => Some(Action::Macro { id: u16::from_be_bytes([hi, lo]), mode: MacroMode::Hold, count: 1 }),
        (FN_MACRO_TOGGLE, &[hi, lo]) => Some(Action::Macro { id: u16::from_be_bytes([hi, lo]), mode: MacroMode::Toggle, count: 1 }),
        _ => None,
    }
}

/// Binds each `(key, action)`, keeping its press points; one failing key does not stop the rest.
pub fn apply(t: &impl Transport, profile: u8, changes: &[(u8, Action)]) -> Vec<(u8, Outcome)> {
    actuation::apply_with(t, profile, changes, set)
}

/// Writes the profile, then with `live` the live copy. In driver mode the firmware drops bindings
/// written to the live copy, so only the profile is judged there.
pub fn save(t: &impl Transport, profile: u8, changes: &[(u8, Action)], live: bool) -> Vec<(u8, Outcome)> {
    actuation::save_with(t, profile, changes, live, set)
}

/// Keys of `changes` bound otherwise in `profile`: what a replug would lose.
pub fn unsaved(t: &impl Transport, profile: u8, changes: &[(u8, Action)]) -> Result<Vec<u8>, Error> {
    let mut out = Vec::new();
    for &(k, a) in changes {
        let stored = actuation::read_key(t, profile, k)?;
        if decode(stored.fn_id, &stored.fn_data) != Some(a) {
            out.push(k);
        }
    }
    Ok(out)
}

fn set(t: &impl Transport, profile: u8, key: u8, &a: &Action) -> Result<Outcome, Error> {
    let (fn_id, fn_data) = encode(a).ok_or_else(|| Error::BadArgument(format!("cannot bind {a:?}")))?;
    actuation::update(t, profile, key, |k| {
        k.fn_id = fn_id;
        k.fn_data = fn_data;
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::actuation::LIVE;
    use crate::fake::FakeKeyboard;
    use crate::keymap;

    const A: u8 = 31;
    const S: u8 = 32;

    #[test]
    fn every_key_has_a_usage() {
        for &(name, id) in keymap::KEYS {
            assert!(encode(factory(id)).is_some(), "{name}");
        }
    }

    #[test]
    fn factory_bindings_from_a_backup() {
        assert_eq!(encode(factory(A)), Some((FN_KEY, vec![0x00, 0x04])));
        assert_eq!(decode(FN_KEY, &[0x00, 0x35]), Some(factory(1)));
        assert_eq!(decode(FN_KEY, &[0x00, 0xE5]), Some(factory(57)));
        assert_eq!(decode(FN_KEY, &[0x08, 0x00]), Some(factory(127)));
    }

    #[test]
    fn actions_roundtrip() {
        let all = [
            Action::Disabled,
            Action::Key { key: 48, mods: 0x01 },
            Action::Mouse { button: Mouse::WheelDown },
            Action::Media { media: Media::VolumeUp },
            Action::Macro { id: 0x8001, mode: MacroMode::Times, count: 3 },
            Action::Macro { id: 2, mode: MacroMode::Hold, count: 1 },
            Action::Macro { id: 2, mode: MacroMode::Toggle, count: 1 },
        ];
        for a in all {
            let (id, data) = encode(a).unwrap();
            assert_eq!(decode(id, &data), Some(a));
        }
        assert_eq!(encode(Action::Mouse { button: Mouse::Middle }), Some((FN_MOUSE, vec![0x03])));
        assert_eq!(encode(Action::Media { media: Media::Play }), Some((FN_CONSUMER, vec![0x00, 0xCD])));
        let m = Action::Macro { id: 0x0102, mode: MacroMode::Times, count: 1 };
        assert_eq!(encode(m), Some((FN_MACRO_TIMES, vec![0x01, 0x02, 0x01])));
    }

    #[test]
    fn unknown_bindings_are_not_decoded() {
        assert_eq!(decode(0x0F, &[0x00, 0x01]), None);
        assert_eq!(decode(0x11, &[0x01]), None);
        assert_eq!(decode(FN_KEY, &[0x00, 0x68]), None);
        assert_eq!(encode(factory(200)), None);
    }

    #[test]
    fn apply_keeps_the_press_points() {
        let kb = FakeKeyboard::new(&[A]);
        kb.edit(1, 0, A, |a| a.threshold_low = 77);
        let r = apply(&kb, 1, &[(A, Action::Disabled)]);
        assert!(matches!(&r[0].1, Outcome::Ok(a) if a.fn_id == FN_DISABLED && a.threshold_low == 77), "{r:?}");
    }

    #[test]
    fn a_key_without_usage_fails_without_touching_the_device() {
        let kb = FakeKeyboard::new(&[A]);
        let r = apply(&kb, 1, &[(A, factory(200))]);
        assert!(matches!(r[0].1, Outcome::Failed(Error::BadArgument(_))), "{r:?}");
        assert!(kb.sent.borrow().is_empty());
    }

    #[test]
    fn save_without_live_leaves_the_live_copy() {
        let kb = FakeKeyboard::new(&[A]);
        let r = save(&kb, 1, &[(A, Action::Disabled)], false);
        assert!(matches!(r[0].1, Outcome::Ok(_)), "{r:?}");
        assert_eq!(kb.key(A).fn_id, FN_DISABLED);
        assert_eq!(kb.live_key(A).fn_id, FN_KEY);
        save(&kb, 1, &[(A, Action::Disabled)], true);
        assert_eq!(kb.live_key(A).fn_id, FN_DISABLED);
    }

    #[test]
    fn unsaved_lists_keys_bound_otherwise_in_the_profile() {
        let kb = FakeKeyboard::new(&[A, S]);
        apply(&kb, LIVE, &[(A, Action::Disabled)]);
        let as_stored = decode(FN_KEY, &[0, S]).unwrap();
        let u = unsaved(&kb, 1, &[(A, Action::Disabled), (S, as_stored)]).unwrap();
        assert_eq!(u, [A]);
    }
}
