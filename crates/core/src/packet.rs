//! 90-byte Razer feature report (without the leading HID report ID).

use std::fmt;

pub const LEN: usize = 90;
pub const ARGS_LEN: usize = 80;

const ARGS: usize = 8;
const CRC: usize = 88;

/// A command identified by class and id. Getters conventionally have bit 7 of `id` set.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Command {
    pub class: u8,
    pub id: u8,
}

impl Command {
    pub const fn new(class: u8, id: u8) -> Self {
        Self { class, id }
    }

    pub fn is_get(self) -> bool {
        self.id & 0x80 != 0
    }
}

impl fmt::Display for Command {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:02X}:{:02X}", self.class, self.id)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Status {
    New,
    Busy,
    Ok,
    Fail,
    Timeout,
    NotSupported,
    Unknown(u8),
}

impl From<u8> for Status {
    fn from(b: u8) -> Self {
        match b {
            0x00 => Status::New,
            0x01 => Status::Busy,
            0x02 => Status::Ok,
            0x03 => Status::Fail,
            0x04 => Status::Timeout,
            0x05 => Status::NotSupported,
            b => Status::Unknown(b),
        }
    }
}

/// Builds a request. `args` longer than 80 bytes is a programming error.
pub fn request(tid: u8, cmd: Command, size: u8, args: &[u8]) -> [u8; LEN] {
    assert!(args.len() <= ARGS_LEN, "args too long: {}", args.len());
    let mut r = [0u8; LEN];
    r[1] = tid;
    r[5] = size;
    r[6] = cmd.class;
    r[7] = cmd.id;
    r[ARGS..ARGS + args.len()].copy_from_slice(args);
    r[CRC] = crc(&r);
    r
}

pub fn crc(r: &[u8; LEN]) -> u8 {
    r[2..CRC].iter().fold(0, |a, b| a ^ b)
}

#[derive(Clone, Debug)]
pub struct Response {
    pub status: Status,
    pub tid: u8,
    pub size: u8,
    pub cmd: Command,
    pub args: [u8; ARGS_LEN],
    pub crc_ok: bool,
}

impl Response {
    pub fn parse(r: &[u8; LEN]) -> Self {
        Self {
            status: r[0].into(),
            tid: r[1],
            size: r[5],
            cmd: Command::new(r[6], r[7]),
            args: r[ARGS..CRC].try_into().unwrap(),
            crc_ok: crc(r) == r[CRC],
        }
    }

    /// Payload bytes the device reported as meaningful.
    pub fn data(&self) -> &[u8] {
        &self.args[..(self.size as usize).min(ARGS_LEN)]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn request_layout_and_roundtrip() {
        let r = request(0x1F, Command::new(0x00, 0x82), 0x16, &[]);
        assert_eq!(&r[..8], &[0, 0x1F, 0, 0, 0, 0x16, 0x00, 0x82]);
        assert_eq!(r[CRC], 0x16 ^ 0x82);

        let mut reply = request(0x1F, Command::new(0x02, 0x99), 7, &[0, 1, 31, 0x0F, 0x5B, 0x0A, 0x00]);
        reply[0] = 0x02;
        let p = Response::parse(&reply);
        assert_eq!(p.status, Status::Ok);
        assert!(p.crc_ok);
        assert_eq!(p.data(), &[0, 1, 31, 0x0F, 0x5B, 0x0A, 0x00]);
    }
}
