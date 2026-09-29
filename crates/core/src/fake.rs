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
    /// Profile 0: the copy the keyboard types with, lost on unplug.
    pub live: RefCell<BTreeMap<u8, KeyAssignment>>,
    /// `02:12` for these keys answers "fail".
    pub failing_writes: BTreeSet<u8>,
    /// `02:12` for these keys answers "ok" but stores nothing.
    pub ignored_writes: BTreeSet<u8>,
    pub unplugged: Cell<bool>,
    /// Unplugs itself once this many commands have been sent.
    pub unplug_after: Option<usize>,
    pub sent: RefCell<Vec<Command>>,
    /// Effect bytes after `store led`, per store.
    pub effects: RefCell<BTreeMap<u8, Vec<u8>>>,
    pub brightness: RefCell<BTreeMap<u8, u8>>,
    /// Custom frame rows of the temporary store: RGB bytes per row.
    pub frame: RefCell<BTreeMap<u8, Vec<u8>>>,
    /// Macro bodies by id.
    pub macros: RefCell<BTreeMap<u16, Vec<u8>>>,
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
                    layer: 0,
                    threshold_low: 0,
                    threshold_high: 0,
                    fn_id: 2,
                    fn_data: vec![0, k],
                };
                (k, a)
            })
            .collect::<BTreeMap<_, _>>();
        let live = keys.iter().map(|(&k, a)| (k, KeyAssignment { profile: 0, ..a.clone() })).collect();
        Self {
            mode: Cell::new(0x03),
            active_profile: profile,
            keys: RefCell::new(keys),
            live: RefCell::new(live),
            failing_writes: BTreeSet::new(),
            ignored_writes: BTreeSet::new(),
            unplugged: Cell::new(false),
            unplug_after: None,
            sent: RefCell::new(Vec::new()),
            effects: RefCell::new([(0, vec![0x03, 0, 0, 0]), (1, vec![0x03, 0, 0, 0])].into()),
            brightness: RefCell::new([(0, 0xF2), (1, 0xF2)].into()),
            frame: RefCell::new(BTreeMap::new()),
            macros: RefCell::new(BTreeMap::new()),
            reply: RefCell::new([0; packet::LEN + 1]),
        }
    }

    pub fn key(&self, k: u8) -> KeyAssignment {
        self.keys.borrow()[&k].clone()
    }

    pub fn live_key(&self, k: u8) -> KeyAssignment {
        self.live.borrow()[&k].clone()
    }

    fn profile(&self, p: u8) -> Option<&RefCell<BTreeMap<u8, KeyAssignment>>> {
        match p {
            0 => Some(&self.live),
            p if p == self.active_profile => Some(&self.keys),
            _ => None,
        }
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
            (0x02, 0x92) => match self.profile(a[0]).and_then(|p| p.borrow().get(&a[1]).cloned()) {
                Some(k) => (OK, analog::set_args(&k)),
                None => (FAIL, Vec::new()),
            },
            (0x02, 0x12) => {
                let Some(k) = analog::parse(a) else {
                    return (FAIL, Vec::new());
                };
                let Some(store) = self.profile(k.profile) else {
                    return (FAIL, Vec::new());
                };
                if self.failing_writes.contains(&k.key) {
                    return (FAIL, Vec::new());
                }
                if !self.ignored_writes.contains(&k.key) {
                    store.borrow_mut().insert(k.key, k);
                }
                (OK, a.to_vec())
            }
            (0x0F, 0x02) => {
                self.effects.borrow_mut().insert(a[0], a[2..].to_vec());
                (OK, a.to_vec())
            }
            (0x0F, 0x82) => match self.effects.borrow().get(&a[0]) {
                Some(e) => (OK, [&a[..2], e.as_slice()].concat()),
                None => (FAIL, Vec::new()),
            },
            (0x0F, 0x03) if a[0] == 0 && a.len() == 5 + 3 * (a[4] - a[3] + 1) as usize => {
                self.frame.borrow_mut().insert(a[2], a[5..].to_vec());
                (OK, a.to_vec())
            }
            (0x0F, 0x04) => {
                self.brightness.borrow_mut().insert(a[0], a[2]);
                (OK, a.to_vec())
            }
            (0x0F, 0x84) => match self.brightness.borrow().get(&a[0]) {
                Some(&b) => (OK, vec![a[0], a[1], b]),
                None => (FAIL, Vec::new()),
            },
            (0x06, 0x03) => match self.macros.borrow_mut().remove(&u16::from_be_bytes([a[0], a[1]])) {
                Some(_) => (OK, a[..2].to_vec()),
                None => (FAIL, Vec::new()),
            },
            (0x06, 0x08) => {
                let size = u32::from_be_bytes(a[2..6].try_into().unwrap()) as usize;
                self.macros.borrow_mut().insert(u16::from_be_bytes([a[0], a[1]]), vec![0; size]);
                (OK, a[..6].to_vec())
            }
            (0x06, 0x09) => {
                let (off, len) = (u32::from_be_bytes(a[2..6].try_into().unwrap()) as usize, a[6] as usize);
                let mut store = self.macros.borrow_mut();
                match store.get_mut(&u16::from_be_bytes([a[0], a[1]])) {
                    Some(body) if off + len <= body.len() => {
                        body[off..off + len].copy_from_slice(&a[7..7 + len]);
                        (OK, a[..7].to_vec())
                    }
                    _ => (FAIL, Vec::new()),
                }
            }
            (0x06, 0x86) => {
                let used: usize = self.macros.borrow().values().map(|b| crate::macros::footprint(b.len())).sum();
                let free = (0x6FA78 - used as u32).to_be_bytes();
                let count = (self.macros.borrow().len() as u16).to_be_bytes();
                (OK, [&count[..], &[0, 0], &free, &free].concat())
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
