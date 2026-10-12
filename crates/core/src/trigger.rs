//! Rapid Trigger and Snap Tap run by the firmware in hardware mode, as on the Huntsman V3 Pro.
//!
//! `02:1A`: `mode profile (fwID on up:u16 down:u16)*`, mode `00` for u16 BE distances at
//! 1638 per 0.1 mm; a zero distance keeps the stored one. `fwID` `FF` sets every key and the
//! profile's own switch, which per-key entries leave alone. `02:21`: `profile on fwID fwID`, one pair.
//! Profile 0 is RAM only.

use crate::packet::{self, Command};
use crate::transport::{Error, Transport, exchange};

pub(crate) const SET_RAPID: Command = Command::new(0x02, 0x1A);
pub(crate) const SET_SNAP_TAP: Command = Command::new(0x02, 0x21);
const U16: u8 = 0;
const ALL_KEYS: u8 = 0xFF;
const ENTRY: usize = 6;
const UNITS_PER_MM: f32 = 16380.0;

fn distance(mm: f32) -> [u8; 2] {
    ((mm * UNITS_PER_MM).round().clamp(1.0, u16::MAX as f32) as u16).to_be_bytes()
}

fn entry(key: u8, rt: Option<(f32, f32)>) -> [u8; ENTRY] {
    let (on, [d0, d1], [u0, u1]) = match rt {
        Some((press, release)) => (1, distance(press), distance(release)),
        None => (0, [0; 2], [0; 2]),
    };
    [key, on, u0, u1, d0, d1]
}

/// Gives each of `keys` its Rapid Trigger, `Some((press, release))` in mm or off.
pub fn set_rapid(t: &impl Transport, profile: u8, keys: &[(u8, Option<(f32, f32)>)]) -> Result<(), Error> {
    let switch = [ALL_KEYS, keys.iter().any(|(_, rt)| rt.is_some()).into(), 0, 0, 0, 0];
    let entries: Vec<[u8; ENTRY]> = std::iter::once(switch).chain(keys.iter().map(|&(k, rt)| entry(k, rt))).collect();
    for chunk in entries.chunks((packet::ARGS_LEN - 2) / ENTRY) {
        let args = [&[U16, profile][..], chunk.as_flattened()].concat();
        exchange(t, SET_RAPID, args.len() as u8, &args)?;
    }
    Ok(())
}

/// One Snap Tap pair, or none.
pub fn set_snap_tap(t: &impl Transport, profile: u8, pair: Option<(u8, u8)>) -> Result<(), Error> {
    let (a, b) = pair.unwrap_or_default();
    exchange(t, SET_SNAP_TAP, 4, &[profile, pair.is_some().into(), a, b]).map(|_| ())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fake::FakeKeyboard;

    const A: u8 = 31;
    const S: u8 = 32;

    #[test]
    fn rapid_trigger_turns_the_profile_switch_on_first() {
        let kb = FakeKeyboard::new(&[A, S]);
        set_rapid(&kb, 0, &[(A, Some((0.1, 0.4))), (S, None)]).unwrap();
        let sent = kb.rapid_sent.borrow();
        assert_eq!(sent.len(), 1);
        assert_eq!(sent[0], [0, 0, 0xFF, 1, 0, 0, 0, 0, A, 1, 0x19, 0x98, 0x06, 0x66, S, 0, 0, 0, 0, 0]);
    }

    #[test]
    fn rapid_trigger_off_everywhere_clears_the_switch() {
        let kb = FakeKeyboard::new(&[A]);
        set_rapid(&kb, 0, &[(A, None)]).unwrap();
        assert_eq!(kb.rapid_sent.borrow()[0][..8], [0, 0, 0xFF, 0, 0, 0, 0, 0]);
    }

    #[test]
    fn many_keys_split_into_packets_of_thirteen() {
        let keys: Vec<(u8, Option<(f32, f32)>)> = (1..=30).map(|k| (k, Some((0.5, 0.5)))).collect();
        let kb = FakeKeyboard::new(&[A]);
        set_rapid(&kb, 0, &keys).unwrap();
        let sizes: Vec<usize> = kb.rapid_sent.borrow().iter().map(Vec::len).collect();
        assert_eq!(sizes, [80, 80, 2 + 5 * ENTRY], "the switch entry rides in the first packet");
    }

    #[test]
    fn snap_tap_pair() {
        let kb = FakeKeyboard::new(&[A, S]);
        set_snap_tap(&kb, 0, Some((A, S))).unwrap();
        assert_eq!(kb.snap.borrow()[&0], [1, A, S]);
        set_snap_tap(&kb, 0, None).unwrap();
        assert_eq!(kb.snap.borrow()[&0], [0, 0, 0]);
    }
}
