//! Onboard profiles: the flash slots, their names, and whole-profile snapshots.

use crate::packet::{self, Command};
use crate::transport::{Error, Transport, exchange};

pub const MAX_SLOTS: u8 = 5;
pub const NAME_CHARS: usize = 32;
/// Service code the app binds to Fn+Menu: the firmware types nothing and reports it on MI_01 Col04.
pub const NEXT_PROFILE_CODE: u8 = 0x20;
/// fwID of the Menu key.
pub const MENU: u8 = 129;

const LIST: Command = Command::new(0x05, 0x81);
const CREATE: Command = Command::new(0x05, 0x02);
const DELETE: Command = Command::new(0x05, 0x03);
const ACTIVATE: Command = Command::new(0x05, 0x04);
const SET_NAME: Command = Command::new(0x05, 0x08);
const GET_NAME: Command = Command::new(0x05, 0x88);
/// `id off16 len16` before the name bytes.
const NAME_HEADER: usize = 5;

/// Slots in the flash list.
pub fn list(t: &impl Transport) -> Result<Vec<u8>, Error> {
    let r = exchange(t, LIST, packet::ARGS_LEN as u8, &[])?;
    let n = (r.args[0] as usize).min(packet::ARGS_LEN - 1);
    Ok(r.args[1..=n].to_vec())
}

/// Lists the slot; its page keeps whatever it held before, so write the whole profile next.
pub fn create(t: &impl Transport, slot: u8) -> Result<(), Error> {
    exchange(t, CREATE, 1, &[slot]).map(|_| ())
}

/// Unlists the slot. Deleting the active one leaves `05:84` on it: activate another first.
pub fn delete(t: &impl Transport, slot: u8) -> Result<(), Error> {
    exchange(t, DELETE, 1, &[slot]).map(|_| ())
}

/// Makes the slot the one the keyboard starts with; reloads profile 0 from it and erases the settings page.
pub fn activate(t: &impl Transport, slot: u8) -> Result<(), Error> {
    exchange(t, ACTIVATE, 1, &[slot]).map(|_| ())
}

/// Empty when the stored length does not fit the reply: such pages hold leftovers, not a name.
pub fn read_name(t: &impl Transport, slot: u8) -> Result<String, Error> {
    let r = exchange(t, GET_NAME, packet::ARGS_LEN as u8, &[slot, 0, 0, 0, (NAME_CHARS * 2) as u8])?;
    let len = u16::from_be_bytes([r.args[3], r.args[4]]) as usize;
    let Some(bytes) = r.args.get(NAME_HEADER..NAME_HEADER + len) else { return Ok(String::new()) };
    let units: Vec<u16> = bytes.as_chunks::<2>().0.iter().map(|c| u16::from_be_bytes(*c)).take_while(|&u| u != 0).collect();
    Ok(String::from_utf16_lossy(&units))
}

pub fn write_name(t: &impl Transport, slot: u8, name: &str) -> Result<(), Error> {
    let short: String = name.chars().take(NAME_CHARS).collect();
    let bytes: Vec<u8> = short.encode_utf16().take(NAME_CHARS).flat_map(u16::to_be_bytes).collect();
    let len = (bytes.len() as u16).to_be_bytes();
    let args = [&[slot, 0, 0][..], &len, &bytes].concat();
    exchange(t, SET_NAME, packet::ARGS_LEN as u8, &args).map(|_| ())
}

/// `codes` are the held codes of one Col04 report; `held` those of the report before it.
pub fn menu_pressed(held: &[u8], codes: &[u8]) -> bool {
    codes.contains(&NEXT_PROFILE_CODE) && !held.contains(&NEXT_PROFILE_CODE)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fake::FakeKeyboard;

    const A: u8 = 31;

    #[test]
    fn create_list_activate_delete() {
        let kb = FakeKeyboard::new(&[A]);
        assert_eq!(list(&kb).unwrap(), [1]);
        create(&kb, 3).unwrap();
        assert_eq!(list(&kb).unwrap(), [1, 3]);
        activate(&kb, 3).unwrap();
        assert_eq!(crate::control::active_profile(&kb).unwrap(), 3);
        activate(&kb, 1).unwrap();
        delete(&kb, 3).unwrap();
        assert_eq!(list(&kb).unwrap(), [1]);
    }

    #[test]
    fn names_are_utf16_be_and_cut_to_32_chars() {
        let kb = FakeKeyboard::new(&[A]);
        assert_eq!(read_name(&kb, 1).unwrap(), "default");
        write_name(&kb, 1, "Игры").unwrap();
        assert_eq!(read_name(&kb, 1).unwrap(), "Игры");
        write_name(&kb, 1, &"x".repeat(40)).unwrap();
        assert_eq!(read_name(&kb, 1).unwrap(), "x".repeat(NAME_CHARS));
    }

    #[test]
    fn a_name_longer_than_the_reply_reads_as_empty() {
        let kb = FakeKeyboard::new(&[A]);
        kb.names.borrow_mut().insert(2, vec![0x41; 200]);
        assert_eq!(read_name(&kb, 2).unwrap(), "");
    }

    #[test]
    fn menu_press_is_reported_once_per_press() {
        assert!(menu_pressed(&[1], &[1, NEXT_PROFILE_CODE]));
        assert!(!menu_pressed(&[1, NEXT_PROFILE_CODE], &[1, NEXT_PROFILE_CODE]));
        assert!(!menu_pressed(&[1, NEXT_PROFILE_CODE], &[1]));
    }
}
