//! Macro bodies in the keyboard's flash store (`06`), which keys play by id.

use serde::{Deserialize, Serialize};

use crate::binding::{self, Mouse};
use crate::packet::Command;
use crate::transport::{Error, Transport, exchange};

const ALLOCATE: Command = Command::new(0x06, 0x08);
const WRITE: Command = Command::new(0x06, 0x09);
const DELETE: Command = Command::new(0x06, 0x03);
const STATS: Command = Command::new(0x06, 0x86);

/// Largest `06:09` payload.
const CHUNK: usize = 72;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum Event {
    /// `key` is a fwID.
    Key { key: u8, down: bool },
    /// A wheel turns one notch on `down` and does nothing on release.
    Mouse { button: Mouse, down: bool },
    Delay { ms: u32 },
}

/// Firmware events. Delays count polling intervals, which are milliseconds at the app's 1000 Hz.
/// `None` when a key has no HID usage.
pub fn encode(events: &[Event]) -> Option<Vec<u8>> {
    let mut out = Vec::new();
    let mut buttons = 0u8;
    for &e in events {
        match e {
            Event::Key { key, down } => out.extend([if down { 0x01 } else { 0x02 }, binding::usage(key)?]),
            Event::Mouse { button: Mouse::WheelUp, down: true } => out.extend([0x0A, 0x01]),
            Event::Mouse { button: Mouse::WheelDown, down: true } => out.extend([0x0A, 0xFF]),
            Event::Mouse { button: Mouse::WheelUp | Mouse::WheelDown, down: false } => {}
            Event::Mouse { button, down } => {
                // The event carries every button's state, bits as in a HID mouse report.
                let bit = 1 << (binding::mouse_code(button) - 1);
                buttons = if down { buttons | bit } else { buttons & !bit };
                out.extend([0x08, buttons]);
            }
            // `11`..`14` carry 1..4 bytes.
            Event::Delay { ms } if ms > 0 => {
                let b = ms.to_be_bytes();
                let skip = b.iter().take_while(|&&x| x == 0).count();
                out.push(0x10 + (4 - skip) as u8);
                out.extend(&b[skip..]);
            }
            Event::Delay { .. } => {}
        }
    }
    Some(out)
}

/// Replaces the body stored under `id`. The firmware buffers it and writes the flash ~100 ms later.
pub fn write(t: &impl Transport, id: u16, body: &[u8]) -> Result<(), Error> {
    // Allocating over an existing id is unexplored; a missing id just fails.
    let _ = delete(t, id);
    let id = id.to_be_bytes();
    let size = u32::try_from(body.len()).map_err(|_| Error::BadArgument("macro too long".into()))?;
    let args = [&id[..], &size.to_be_bytes()].concat();
    exchange(t, ALLOCATE, args.len() as u8, &args)?;
    for (i, chunk) in body.chunks(CHUNK).enumerate() {
        let offset = (i * CHUNK) as u32;
        let args = [&id[..], &offset.to_be_bytes(), &[chunk.len() as u8], chunk].concat();
        exchange(t, WRITE, args.len() as u8, &args)?;
    }
    Ok(())
}

pub fn delete(t: &impl Transport, id: u16) -> Result<(), Error> {
    exchange(t, DELETE, 2, &id.to_be_bytes()).map(|_| ())
}

/// Free bytes in the store, counted afresh on every request.
pub fn free(t: &impl Transport) -> Result<u32, Error> {
    let r = exchange(t, STATS, 12, &[])?;
    let d = r.data();
    d.get(8..12).map(|b| u32::from_be_bytes(b.try_into().unwrap())).ok_or(Error::ShortReply(STATS))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fake::{FakeKeyboard, footprint};

    const A: u8 = 31;
    const B: u8 = 50;

    fn key(key: u8, down: bool) -> Event {
        Event::Key { key, down }
    }

    #[test]
    fn types_ab_like_the_documented_example() {
        let ms = Event::Delay { ms: 0x32 };
        let body = encode(&[key(A, true), ms, key(A, false), key(B, true), ms, key(B, false)]).unwrap();
        assert_eq!(body, [0x01, 0x04, 0x11, 0x32, 0x02, 0x04, 0x01, 0x05, 0x11, 0x32, 0x02, 0x05]);
    }

    #[test]
    fn delays_take_the_shortest_form() {
        let d = |ms| encode(&[Event::Delay { ms }]).unwrap();
        assert!(d(0).is_empty());
        assert_eq!(d(300), [0x12, 0x01, 0x2C]);
        assert_eq!(d(0x01_0000), [0x13, 0x01, 0x00, 0x00]);
        assert_eq!(d(0x0100_0000), [0x14, 0x01, 0x00, 0x00, 0x00]);
    }

    #[test]
    fn mouse_events_carry_every_held_button() {
        let m = |button, down| Event::Mouse { button, down };
        let body = encode(&[m(Mouse::Left, true), m(Mouse::Right, true), m(Mouse::Left, false), m(Mouse::WheelDown, true), m(Mouse::WheelDown, false)]);
        assert_eq!(body.unwrap(), [0x08, 0x01, 0x08, 0x03, 0x08, 0x02, 0x0A, 0xFF]);
    }

    #[test]
    fn a_key_without_usage_is_not_encoded() {
        assert_eq!(encode(&[key(200, true)]), None);
    }

    #[test]
    fn footprint_matches_the_hardware() {
        // 4 bytes took 0x18 of 0x6FA78 on the keyboard.
        assert_eq!(footprint(4), 0x18);
        assert_eq!(footprint(2), 16);
        assert_eq!(footprint(73), 8 + 80);
    }

    #[test]
    fn write_sends_the_body_in_chunks() {
        let kb = FakeKeyboard::new(&[A]);
        let body: Vec<u8> = (0..100).collect();
        write(&kb, 0x8001, &body).unwrap();
        let store = kb.macros.borrow();
        assert_eq!(store[&0x8001], body);
        let sent: Vec<_> = kb.sent.borrow().iter().map(|c| (c.class, c.id)).collect();
        assert_eq!(sent, [(0x06, 0x03), (0x06, 0x08), (0x06, 0x09), (0x06, 0x09)]);
    }

    #[test]
    fn free_space_follows_writes_and_deletes() {
        let kb = FakeKeyboard::new(&[A]);
        let empty = free(&kb).unwrap();
        write(&kb, 1, &[1, 4, 2, 4]).unwrap();
        assert_eq!(free(&kb).unwrap(), empty - 0x18);
        delete(&kb, 1).unwrap();
        assert_eq!(free(&kb).unwrap(), empty);
    }
}
