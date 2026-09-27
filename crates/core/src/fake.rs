//! In-memory Huntsman V2 Analog answering the commands `core` sends.

use std::cell::{Cell, RefCell};
use std::collections::{BTreeMap, BTreeSet};

use crate::analog::{self, KeyAssignment};
use crate::packet::{self, Command, Response};
use crate::transport::{Error, Report, Transport};

const OK: u8 = 0x02;
const FAIL: u8 = 0x03;
const NOT_SUPPORTED: u8 = 0x05;

pub struct FakeKeyboard {
    pub mode: Cell<u8>,
    pub active_profile: u8,
    pub keys: RefCell<BTreeMap<u8, KeyAssignment>>,
    /// `02:12` for these keys answers "fail".
    pub failing_writes: BTreeSet<u8>,
    /// `02:12` for these keys answers "ok" but stores nothing.
    pub ignored_writes: BTreeSet<u8>,
    pub unplugged: Cell<bool>,
    /// Unplugs itself once this many commands have been sent.
    pub unplug_after: Option<usize>,
    pub sent: RefCell<Vec<Command>>,
    reply: RefCell<Report>,
}

impl FakeKeyboard {
    /// Driver mode, profile 1, every key at 1.5 mm and mapped to its own HID code.
    pub fn new(keys: &[u8]) -> Self {
        let profile = 1;
        let keys = keys
            .iter()
            .map(|&k| {
                let a = KeyAssignment {
                    profile,
                    key: k,
                    mode: 0,
                    threshold_low: 0,
                    threshold_high: 0,
                    fn_id: 2,
                    fn_data: vec![0, k],
                };
                (k, a)
            })
            .collect();
        Self {
            mode: Cell::new(0x03),
            active_profile: profile,
            keys: RefCell::new(keys),
            failing_writes: BTreeSet::new(),
            ignored_writes: BTreeSet::new(),
            unplugged: Cell::new(false),
            unplug_after: None,
            sent: RefCell::new(Vec::new()),
            reply: RefCell::new([0; packet::LEN + 1]),
        }
    }

    pub fn key(&self, k: u8) -> KeyAssignment {
        self.keys.borrow()[&k].clone()
    }

    fn answer(&self, req: &Response) -> (u8, Vec<u8>) {
        let a = req.data();
        match (req.cmd.class, req.cmd.id) {
            (0x00, 0x84) => (OK, vec![self.mode.get(), 0]),
            (0x00, 0x04) => {
                self.mode.set(a[0]);
                (OK, a[..2].to_vec())
            }
            (0x05, 0x84) => (OK, vec![self.active_profile]),
            (0x02, 0x92) => match self.keys.borrow().get(&a[1]) {
                Some(k) if a[0] == self.active_profile => (OK, analog::set_args(k)),
                _ => (FAIL, Vec::new()),
            },
            (0x02, 0x12) => {
                let Some(k) = analog::parse(a) else {
                    return (FAIL, Vec::new());
                };
                if self.failing_writes.contains(&k.key) {
                    return (FAIL, Vec::new());
                }
                if !self.ignored_writes.contains(&k.key) {
                    self.keys.borrow_mut().insert(k.key, k);
                }
                (OK, a.to_vec())
            }
            _ => (NOT_SUPPORTED, Vec::new()),
        }
    }
}

impl Transport for FakeKeyboard {
    fn send_feature(&self, r: &Report) -> Result<(), Error> {
        if self.unplug_after.is_some_and(|n| self.sent.borrow().len() >= n) {
            self.unplugged.set(true);
        }
        if self.unplugged.get() {
            return Err(Error::Io("unplugged".into()));
        }
        let req = Response::parse(r[1..].try_into().unwrap());
        self.sent.borrow_mut().push(req.cmd);
        let (status, data) = self.answer(&req);
        let mut rep = [0u8; packet::LEN + 1];
        rep[1..].copy_from_slice(&packet::request(req.tid, req.cmd, data.len() as u8, &data));
        rep[1] = status;
        *self.reply.borrow_mut() = rep;
        Ok(())
    }

    fn get_feature(&self, r: &mut Report) -> Result<(), Error> {
        if self.unplugged.get() {
            return Err(Error::Io("unplugged".into()));
        }
        *r = *self.reply.borrow();
        Ok(())
    }
}
