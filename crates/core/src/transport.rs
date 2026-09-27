//! Request/response exchange over any feature-report transport.

use std::fmt;
use std::thread::sleep;
use std::time::Duration;

use crate::packet::{self, Command, Response, Status};

/// HID report ID 0 followed by the 90-byte Razer report.
pub type Report = [u8; packet::LEN + 1];

pub const TID: u8 = 0x1F;

const POLL_DELAY: Duration = Duration::from_millis(15);
const MAX_POLLS: usize = 10;

#[derive(Clone, Debug, PartialEq)]
pub enum Error {
    Io(String),
    Status(Command, Status),
    Busy(Command),
    WrongReply { sent: Command, got: Command, tid: u8 },
    Crc(Command),
    ShortReply(Command),
    OutOfRange(f32),
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Error::Io(e) => write!(f, "HID: {e}"),
            Error::Status(c, s) => write!(f, "{c}: status {s:?}"),
            Error::Busy(c) => write!(f, "{c}: device stayed busy"),
            Error::WrongReply { sent, got, tid } => write!(f, "{sent}: reply is for {got} tid {tid:02X}"),
            Error::Crc(c) => write!(f, "{c}: reply crc mismatch"),
            Error::ShortReply(c) => write!(f, "{c}: reply too short"),
            Error::OutOfRange(mm) => write!(f, "{mm} mm is outside 1.5..=3.6"),
        }
    }
}

impl std::error::Error for Error {}

/// Feature-report access to the control interface.
pub trait Transport {
    fn send_feature(&self, report: &Report) -> Result<(), Error>;
    fn get_feature(&self, report: &mut Report) -> Result<(), Error>;
}

/// Sends one command and polls until the device answers it.
pub fn exchange(t: &impl Transport, cmd: Command, size: u8, args: &[u8]) -> Result<Response, Error> {
    let mut tx = [0u8; packet::LEN + 1];
    tx[1..].copy_from_slice(&packet::request(TID, cmd, size, args));
    t.send_feature(&tx)?;
    for _ in 0..MAX_POLLS {
        sleep(POLL_DELAY);
        let mut rx = [0u8; packet::LEN + 1];
        t.get_feature(&mut rx)?;
        let r = Response::parse(rx[1..].try_into().unwrap());
        match r.status {
            Status::Busy => continue,
            Status::Ok if r.cmd != cmd || r.tid != TID => {
                return Err(Error::WrongReply { sent: cmd, got: r.cmd, tid: r.tid });
            }
            Status::Ok if !r.crc_ok => return Err(Error::Crc(cmd)),
            Status::Ok => return Ok(r),
            s => return Err(Error::Status(cmd, s)),
        }
    }
    Err(Error::Busy(cmd))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::packet::{self, Command};
    use std::cell::RefCell;

    const CMD: Command = Command::new(0x00, 0x81);

    struct Scripted {
        sent: RefCell<Vec<Report>>,
        replies: RefCell<Vec<Report>>,
    }

    impl Scripted {
        fn new(mut replies: Vec<Report>) -> Self {
            replies.reverse();
            Self { sent: RefCell::new(Vec::new()), replies: RefCell::new(replies) }
        }
    }

    impl Transport for Scripted {
        fn send_feature(&self, r: &Report) -> Result<(), Error> {
            self.sent.borrow_mut().push(*r);
            Ok(())
        }

        fn get_feature(&self, r: &mut Report) -> Result<(), Error> {
            *r = self.replies.borrow_mut().pop().ok_or(Error::Io("no reply".into()))?;
            Ok(())
        }
    }

    fn reply(status: u8, tid: u8, cmd: Command, data: &[u8]) -> Report {
        let mut r = [0u8; packet::LEN + 1];
        r[1..].copy_from_slice(&packet::request(tid, cmd, data.len() as u8, data));
        r[1] = status;
        r
    }

    #[test]
    fn returns_ok_reply_and_sends_report_id_zero() {
        let t = Scripted::new(vec![reply(0x02, TID, CMD, &[1, 1])]);
        let r = exchange(&t, CMD, 2, &[]).unwrap();
        assert_eq!(r.data(), [1, 1]);
        let sent = t.sent.borrow()[0];
        assert_eq!(sent[0], 0);
        assert_eq!(&sent[1..], &packet::request(TID, CMD, 2, &[]));
    }

    #[test]
    fn retries_while_busy() {
        let busy = reply(0x01, TID, CMD, &[]);
        let t = Scripted::new(vec![busy, busy, reply(0x02, TID, CMD, &[7])]);
        assert_eq!(exchange(&t, CMD, 1, &[]).unwrap().data(), [7]);
    }

    #[test]
    fn gives_up_when_always_busy() {
        let t = Scripted::new(vec![reply(0x01, TID, CMD, &[]); 10]);
        assert_eq!(exchange(&t, CMD, 1, &[]).unwrap_err(), Error::Busy(CMD));
    }

    #[test]
    fn reports_device_status() {
        let t = Scripted::new(vec![reply(0x05, TID, CMD, &[])]);
        assert_eq!(exchange(&t, CMD, 1, &[]).unwrap_err(), Error::Status(CMD, Status::NotSupported));
    }

    #[test]
    fn rejects_reply_for_other_command_or_tid() {
        let other = Command::new(0x00, 0x82);
        let t = Scripted::new(vec![reply(0x02, TID, other, &[])]);
        assert_eq!(exchange(&t, CMD, 1, &[]).unwrap_err(), Error::WrongReply { sent: CMD, got: other, tid: TID });
        let t = Scripted::new(vec![reply(0x02, 0x3F, CMD, &[])]);
        assert_eq!(exchange(&t, CMD, 1, &[]).unwrap_err(), Error::WrongReply { sent: CMD, got: CMD, tid: 0x3F });
    }

    #[test]
    fn rejects_bad_crc() {
        let mut r = reply(0x02, TID, CMD, &[1]);
        r[89] ^= 0xFF;
        let t = Scripted::new(vec![r]);
        assert_eq!(exchange(&t, CMD, 1, &[]).unwrap_err(), Error::Crc(CMD));
    }
}
