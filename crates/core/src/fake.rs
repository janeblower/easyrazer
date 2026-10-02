//! In-memory Huntsman V2 Analog answering the commands `core` sends.

use std::cell::{Cell, RefCell};
use std::collections::{BTreeMap, BTreeSet};

use crate::analog::{self, KeyAssignment};
use crate::packet::{self, Command, Response};
use crate::transport::{Error, Report, Transport};

const OK: u8 = 0x02;
const FAIL: u8 = 0x03;
const NOT_SUPPORTED: u8 = 0x05;

/// Flash a body of `len` bytes takes: block tags plus a 6-byte header, in 8-byte units.
pub fn footprint(len: usize) -> usize {
    8 + (len + 6).div_ceil(8) * 8
}

pub struct FakeKeyboard {
    pub mode: Cell<u8>,
    pub active: Cell<u8>,
    /// Profiles in the flash list (`05:81`).
    pub listed: RefCell<BTreeSet<u8>>,
    /// Keys by `(profile, layer)`; profile 0 is the copy the keyboard types with, lost on unplug.
    /// Pages 1–5 exist whether listed or not: `05:02` does not clear them.
    pub pages: RefCell<BTreeMap<(u8, u8), BTreeMap<u8, KeyAssignment>>>,
    /// Profile names as the device stores them, UTF-16BE.
    pub names: RefCell<BTreeMap<u8, Vec<u8>>>,
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
        let keys = keys
            .iter()
            .map(|&k| {
                let a = KeyAssignment {
                    profile: 1,
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
        let mut pages = BTreeMap::new();
        for p in 0..=5 {
            for l in 0..=1 {
                let page = keys.iter().map(|(&k, a)| (k, KeyAssignment { profile: p, layer: l, ..a.clone() })).collect();
                pages.insert((p, l), page);
            }
        }
        Self {
            mode: Cell::new(0x03),
            active: Cell::new(1),
            listed: RefCell::new([1].into()),
            pages: RefCell::new(pages),
            names: RefCell::new([(1, "default".encode_utf16().flat_map(u16::to_be_bytes).collect())].into()),
            failing_writes: BTreeSet::new(),
            ignored_writes: BTreeSet::new(),
            unplugged: Cell::new(false),
            unplug_after: None,
            sent: RefCell::new(Vec::new()),
            effects: RefCell::new((0..=5).map(|p| (p, vec![0x03, 0, 0, 0])).collect()),
            brightness: RefCell::new((0..=5).map(|p| (p, 0xF2)).collect()),
            frame: RefCell::new(BTreeMap::new()),
            macros: RefCell::new(BTreeMap::new()),
            reply: RefCell::new([0; packet::LEN + 1]),
        }
    }

    pub fn key(&self, k: u8) -> KeyAssignment {
        self.key_in(self.active.get(), 0, k)
    }

    pub fn live_key(&self, k: u8) -> KeyAssignment {
        self.key_in(0, 0, k)
    }

    pub fn key_in(&self, profile: u8, layer: u8, k: u8) -> KeyAssignment {
        self.pages.borrow()[&(profile, layer)][&k].clone()
    }

    pub fn edit(&self, profile: u8, layer: u8, k: u8, f: impl FnOnce(&mut KeyAssignment)) {
        f(self.pages.borrow_mut().get_mut(&(profile, layer)).unwrap().get_mut(&k).unwrap());
    }

    /// What the firmware does on `05:04` and on a mode switch: profile 0 becomes a copy of the active one.
    fn reload_live(&self) {
        let p = self.active.get();
        let mut pages = self.pages.borrow_mut();
        for l in 0..=1 {
            let copy = pages[&(p, l)].iter().map(|(&k, a)| (k, KeyAssignment { profile: 0, ..a.clone() })).collect();
            pages.insert((0, l), copy);
        }
        let mut fx = self.effects.borrow_mut();
        if let Some(e) = fx.get(&p).cloned() {
            fx.insert(0, e);
        }
    }

    fn answer(&self, req: &Response) -> (u8, Vec<u8>) {
        let a = req.data();
        match (req.cmd.class, req.cmd.id) {
            (0x00, 0x84) => (OK, vec![self.mode.get(), 0]),
            (0x00, 0x04) => {
                self.mode.set(a[0]);
                self.reload_live();
                (OK, a[..2].to_vec())
            }
            (0x05, 0x80) => (OK, vec![self.listed.borrow().len() as u8]),
            (0x05, 0x81) => {
                let l = self.listed.borrow();
                (OK, std::iter::once(l.len() as u8).chain(l.iter().copied()).collect())
            }
            (0x05, 0x84) => (OK, vec![self.active.get()]),
            (0x05, 0x02) if (1..=5).contains(&a[0]) && self.listed.borrow_mut().insert(a[0]) => (OK, vec![a[0]]),
            (0x05, 0x03) if self.listed.borrow_mut().remove(&a[0]) => (OK, vec![a[0]]),
            (0x05, 0x04) if self.listed.borrow().contains(&a[0]) => {
                self.active.set(a[0]);
                self.reload_live();
                (OK, vec![a[0]])
            }
            (0x05, 0x08) => {
                let len = u16::from_be_bytes([a[3], a[4]]) as usize;
                self.names.borrow_mut().insert(a[0], a[5..5 + len].to_vec());
                (OK, a.to_vec())
            }
            (0x05, 0x88) => {
                let name = self.names.borrow().get(&a[0]).cloned().unwrap_or_default();
                let len = (name.len() as u16).to_be_bytes();
                // The device reports the stored length even when the bytes do not fit the reply.
                let mut out = [&[a[0], 0, 0], &len[..], &name].concat();
                out.truncate(packet::ARGS_LEN);
                (OK, out)
            }
            (0x02, 0x92) => match self.pages.borrow().get(&(a[0], a[2])).and_then(|p| p.get(&a[1]).cloned()) {
                Some(k) => (OK, analog::set_args(&k)),
                None => (FAIL, Vec::new()),
            },
            (0x02, 0x12) => {
                let Some(k) = analog::parse(a) else {
                    return (FAIL, Vec::new());
                };
                if self.failing_writes.contains(&k.key) || !self.pages.borrow().contains_key(&(k.profile, k.layer)) {
                    return (FAIL, Vec::new());
                }
                if !self.ignored_writes.contains(&k.key) {
                    let mut pages = self.pages.borrow_mut();
                    // Models the worst case the app must survive (a Normal write also replacing Hypershift in profile 0), not verified firmware behaviour.
                    if k.profile == 0 && k.layer == 0 {
                        pages.get_mut(&(0, 1)).unwrap().insert(k.key, KeyAssignment { layer: 1, ..k.clone() });
                    }
                    pages.get_mut(&(k.profile, k.layer)).unwrap().insert(k.key, k);
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
                let used: usize = self.macros.borrow().values().map(|b| footprint(b.len())).sum();
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::packet::Command;
    use crate::transport::exchange;

    const A: u8 = 31;

    fn ok(kb: &FakeKeyboard, class: u8, id: u8, args: &[u8]) -> Vec<u8> {
        exchange(kb, Command::new(class, id), 80, args).unwrap().data().to_vec()
    }

    #[test]
    fn activating_a_profile_reloads_profile_0() {
        let kb = FakeKeyboard::new(&[A]);
        ok(&kb, 0x05, 0x02, &[2]);
        kb.edit(2, 1, A, |a| a.fn_id = 0x07);
        kb.edit(0, 0, A, |a| a.threshold_low = 99);
        ok(&kb, 0x05, 0x04, &[2]);
        assert_eq!(kb.active.get(), 2);
        assert_eq!(kb.key_in(0, 1, A).fn_id, 0x07);
        assert_eq!(kb.live_key(A).threshold_low, 0);
    }

    #[test]
    fn creating_keeps_what_the_page_held() {
        let kb = FakeKeyboard::new(&[A]);
        kb.edit(3, 0, A, |a| a.threshold_low = 200);
        ok(&kb, 0x05, 0x02, &[3]);
        assert_eq!(ok(&kb, 0x05, 0x81, &[])[..3], [2, 1, 3]);
        assert_eq!(kb.key_in(3, 0, A).threshold_low, 200);
    }

    #[test]
    fn deleting_the_active_profile_leaves_it_active() {
        let kb = FakeKeyboard::new(&[A]);
        ok(&kb, 0x05, 0x02, &[2]);
        ok(&kb, 0x05, 0x04, &[2]);
        ok(&kb, 0x05, 0x03, &[2]);
        assert_eq!(ok(&kb, 0x05, 0x81, &[])[..2], [1, 1]);
        assert_eq!(ok(&kb, 0x05, 0x84, &[]), [2]);
    }

    #[test]
    fn normal_write_to_profile_0_overwrites_its_hypershift() {
        let kb = FakeKeyboard::new(&[A]);
        let mut a = kb.live_key(A);
        a.fn_id = 0x00;
        a.fn_data.clear();
        crate::actuation::write_key(&kb, &a).unwrap();
        assert_eq!(kb.key_in(0, 1, A).fn_id, 0x00);
        assert_eq!(kb.key_in(1, 1, A).fn_id, 0x02);
    }

    #[test]
    fn names_round_trip() {
        let kb = FakeKeyboard::new(&[A]);
        ok(&kb, 0x05, 0x08, &[1, 0, 0, 0, 4, 0, b'H', 0, b'i']);
        assert_eq!(ok(&kb, 0x05, 0x88, &[1, 0, 0, 0, 0x40])[..9], [1, 0, 0, 0, 4, 0, b'H', 0, b'i']);
    }
}
