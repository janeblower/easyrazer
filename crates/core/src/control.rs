//! Device-wide state: operating mode and the active onboard profile.

use crate::packet::Command;
use crate::transport::{Error, Transport, exchange};

pub const MODE_HARDWARE: u8 = 0x00;
pub const MODE_DRIVER: u8 = 0x03;

const GET_MODE: Command = Command::new(0x00, 0x84);
const SET_MODE: Command = Command::new(0x00, 0x04);
const GET_ACTIVE_PROFILE: Command = Command::new(0x05, 0x84);

pub fn mode(t: &impl Transport) -> Result<u8, Error> {
    Ok(exchange(t, GET_MODE, 2, &[])?.args[0])
}

/// Hands key processing back to the firmware.
pub fn set_hardware_mode(t: &impl Transport) -> Result<(), Error> {
    exchange(t, SET_MODE, 2, &[MODE_HARDWARE, 0]).map(|_| ())
}

/// The keyboard stops typing and only reports depth; the host emits every key.
/// The only other mode `core` sets: mode 0x01 drops the keyboard off the bus until replugged.
pub fn set_driver_mode(t: &impl Transport) -> Result<(), Error> {
    exchange(t, SET_MODE, 2, &[MODE_DRIVER, 0]).map(|_| ())
}

pub fn active_profile(t: &impl Transport) -> Result<u8, Error> {
    Ok(exchange(t, GET_ACTIVE_PROFILE, 1, &[])?.args[0])
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fake::FakeKeyboard;

    #[test]
    fn reads_mode_and_active_profile() {
        let kb = FakeKeyboard::new(&[31]);
        assert_eq!(mode(&kb).unwrap(), MODE_DRIVER);
        assert_eq!(active_profile(&kb).unwrap(), 1);
    }

    #[test]
    fn switches_to_hardware_mode() {
        let kb = FakeKeyboard::new(&[31]);
        set_hardware_mode(&kb).unwrap();
        assert_eq!(kb.mode.get(), MODE_HARDWARE);
    }

    #[test]
    fn switches_to_driver_mode() {
        let kb = FakeKeyboard::new(&[31]);
        set_hardware_mode(&kb).unwrap();
        set_driver_mode(&kb).unwrap();
        assert_eq!(kb.mode.get(), MODE_DRIVER);
    }
}
