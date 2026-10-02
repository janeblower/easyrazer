//! Onboard profiles: the flash slots, their names, and whole-profile snapshots.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::actuation::{self, LIVE};
use crate::analog::{KeyAssignment, Layer};
use crate::devices::DeviceSpec;
use crate::lighting::{self, CUSTOM, Look, Store};
use crate::packet::{self, Command};
use crate::transport::{Error, Transport, exchange};

pub const MAX_SLOTS: u8 = 5;
pub const NAME_CHARS: usize = 32;
/// Service code the app binds to Fn+Menu: the firmware types nothing and reports it on MI_01 Col04.
pub const NEXT_PROFILE_CODE: u8 = 0x20;
/// fwID of the Menu key.
pub const MENU: u8 = 129;

const FN_SERVICE: u8 = 0x11;
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

/// Trimmed and cut to `NAME_CHARS` UTF-16 units without splitting a surrogate pair.
pub fn fit_name(name: &str) -> String {
    let mut units = 0;
    name.trim()
        .chars()
        .take_while(|c| {
            units += c.len_utf16();
            units <= NAME_CHARS
        })
        .collect()
}

pub fn write_name(t: &impl Transport, slot: u8, name: &str) -> Result<(), Error> {
    let bytes: Vec<u8> = fit_name(name).encode_utf16().flat_map(u16::to_be_bytes).collect();
    let len = (bytes.len() as u16).to_be_bytes();
    let args = [&[slot, 0, 0][..], &len, &bytes].concat();
    exchange(t, SET_NAME, packet::ARGS_LEN as u8, &args).map(|_| ())
}

/// `codes` are the held codes of one Col04 report; `held` those of the report before it.
pub fn menu_pressed(held: &[u8], codes: &[u8]) -> bool {
    codes.contains(&NEXT_PROFILE_CODE) && !held.contains(&NEXT_PROFILE_CODE)
}

/// One key of one layer, as the flash holds it.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct RawKey {
    pub thr_low: u8,
    pub thr_high: u8,
    pub fn_id: u8,
    pub fn_data: Vec<u8>,
}

impl RawKey {
    pub fn of(a: &KeyAssignment) -> Self {
        Self { thr_low: a.threshold_low, thr_high: a.threshold_high, fn_id: a.fn_id, fn_data: a.fn_data.clone() }
    }

    pub fn at(&self, profile: u8, layer: Layer, key: u8) -> KeyAssignment {
        KeyAssignment {
            profile,
            key,
            layer: layer as u8,
            threshold_low: self.thr_low,
            threshold_high: self.thr_high,
            fn_id: self.fn_id,
            fn_data: self.fn_data.clone(),
        }
    }
}

/// Everything a flash profile holds that the app moves between profiles.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Snapshot {
    pub normal: BTreeMap<u8, RawKey>,
    pub hypershift: BTreeMap<u8, RawKey>,
    pub look: Option<Look>,
}

impl Snapshot {
    pub fn layer(&self, l: Layer) -> &BTreeMap<u8, RawKey> {
        match l {
            Layer::Normal => &self.normal,
            Layer::Hypershift => &self.hypershift,
        }
    }

    pub fn layer_mut(&mut self, l: Layer) -> &mut BTreeMap<u8, RawKey> {
        match l {
            Layer::Normal => &mut self.normal,
            Layer::Hypershift => &mut self.hypershift,
        }
    }

    /// The copy profile 0 gets while the app listens for Fn+Menu.
    pub fn with_menu_override(&self) -> Snapshot {
        let mut s = self.clone();
        let k = s.hypershift.entry(MENU).or_insert(RawKey { thr_low: 0, thr_high: 0, fn_id: 0, fn_data: Vec::new() });
        k.fn_id = FN_SERVICE;
        k.fn_data = vec![NEXT_PROFILE_CODE];
        s
    }

    /// The custom layout never reaches a slot, so it does not count as a difference.
    pub fn matches(&self, slot: &Snapshot) -> bool {
        let custom = self.look.as_ref().is_some_and(|l| l.effect.name == CUSTOM);
        self.normal == slot.normal && self.hypershift == slot.hypershift && (custom || self.look == slot.look)
    }
}

fn store(slot: u8) -> Store {
    if slot == LIVE { Store::Temporary } else { Store::Slot(slot) }
}

/// `progress` counts the reads of both layers; only the Normal ones, which the window shows, carry the key.
pub fn read_snapshot(
    t: &impl Transport,
    d: &DeviceSpec,
    slot: u8,
    keys: &[u8],
    mut progress: impl FnMut(usize, Option<&KeyAssignment>),
) -> Result<Snapshot, Error> {
    let normal = actuation::read_all(t, slot, keys, |n, a| progress(n, Some(a)))?;
    let mut hypershift = BTreeMap::new();
    for (i, &k) in keys.iter().enumerate() {
        hypershift.insert(k, RawKey::of(&actuation::read_layer(t, slot, k, Layer::Hypershift)?));
        progress(keys.len() + i + 1, None);
    }
    // An effect the description does not know is kept by the slot as it is.
    let look = lighting::get_look(t, d, store(slot)).ok().flatten();
    Ok(Snapshot { normal: normal.iter().map(|a| (a.key, RawKey::of(a))).collect(), hypershift, look })
}

/// Keys of `want` that `base` holds otherwise, Normal first; keys `want` lacks are left alone.
pub fn diff(base: &Snapshot, want: &Snapshot) -> Vec<(Layer, u8, RawKey)> {
    [Layer::Normal, Layer::Hypershift]
        .into_iter()
        .flat_map(|l| {
            let b = base.layer(l);
            want.layer(l).iter().filter(move |&(k, r)| b.get(k) != Some(r)).map(move |(&k, r)| (l, k, r.clone()))
        })
        .collect()
}

/// Brings `slot` from `base` to `want` with no pause between commands: the firmware saves a flash
/// profile once its changes stop, so one burst costs one page erase. Returns what the slot holds after.
pub fn write(t: &impl Transport, d: &DeviceSpec, slot: u8, base: &Snapshot, want: &Snapshot) -> Result<Snapshot, Error> {
    let mut changes = diff(base, want);
    if slot == LIVE {
        // A Normal write to profile 0 overwrites the key's Hypershift too.
        let normal: Vec<u8> = changes.iter().filter(|c| c.0 == Layer::Normal).map(|c| c.1).collect();
        for k in normal {
            let queued = changes.iter().any(|c| c.0 == Layer::Hypershift && c.1 == k);
            if let Some(h) = want.hypershift.get(&k).filter(|_| !queued) {
                changes.push((Layer::Hypershift, k, h.clone()));
            }
        }
    }
    for (l, k, r) in &changes {
        actuation::write_key(t, &r.at(slot, *l, *k))?;
    }
    let mut out = want.clone();
    for l in [Layer::Normal, Layer::Hypershift] {
        for (k, r) in base.layer(l) {
            out.layer_mut(l).entry(*k).or_insert_with(|| r.clone());
        }
    }
    out.look = base.look.clone();
    if let Some(look) = &want.look
        && want.look != base.look
        && (slot == LIVE || look.effect.name != CUSTOM)
    {
        lighting::set_look(t, d, store(slot), look)?;
        out.look = want.look.clone();
    }
    Ok(out)
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
    fn names_fit_in_utf16_units_without_splitting_pairs() {
        let kb = FakeKeyboard::new(&[A]);
        let emoji = "😀".repeat(40);
        assert!(fit_name(&emoji).encode_utf16().count() <= NAME_CHARS);
        write_name(&kb, 1, &emoji).unwrap();
        let back = read_name(&kb, 1).unwrap();
        assert!(!back.contains('\u{FFFD}') && back == fit_name(&emoji), "{back}");
        assert_eq!(fit_name(&"ы".repeat(40)).chars().count(), NAME_CHARS);
        assert_eq!(fit_name("  a  "), "a");
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

    use crate::devices;
    use crate::lighting::Effect;

    const S: u8 = 32;

    fn spec() -> &'static crate::devices::DeviceSpec {
        devices::by_pid(0x0266).unwrap()
    }

    fn look(name: &str) -> Look {
        Look { effect: Effect { name: name.into(), ..Default::default() }, brightness: 0x80 }
    }

    #[test]
    fn snapshot_reads_both_layers_and_the_look() {
        let kb = FakeKeyboard::new(&[A, S]);
        kb.edit(2, 1, A, |a| a.fn_id = 0x07);
        let mut seen = Vec::new();
        let s = read_snapshot(&kb, spec(), 2, &[A, S], |n, a| seen.push((n, a.map(|a| a.key)))).unwrap();
        assert_eq!(seen, [(1, Some(A)), (2, Some(S)), (3, None), (4, None)]);
        assert_eq!(s.hypershift[&A].fn_id, 0x07);
        assert_eq!(s.normal[&A].fn_data, [0, A]);
        assert!(s.look.is_some());
    }

    #[test]
    fn equal_snapshots_write_nothing() {
        let kb = FakeKeyboard::new(&[A, S]);
        let s = read_snapshot(&kb, spec(), 1, &[A, S], |_, _| {}).unwrap();
        let sent = kb.sent.borrow().len();
        assert_eq!(write(&kb, spec(), 1, &s, &s).unwrap(), s);
        assert_eq!(kb.sent.borrow().len(), sent);
    }

    #[test]
    fn only_differing_keys_are_written() {
        let kb = FakeKeyboard::new(&[A, S]);
        let base = read_snapshot(&kb, spec(), 2, &[A, S], |_, _| {}).unwrap();
        let mut want = base.clone();
        want.normal.get_mut(&S).unwrap().thr_low = 150;
        let sent = kb.sent.borrow().len();
        let after = write(&kb, spec(), 2, &base, &want).unwrap();
        assert_eq!(kb.sent.borrow().len(), sent + 1);
        assert_eq!(kb.key_in(2, 0, S).threshold_low, 150);
        assert_eq!(after, want);
    }

    #[test]
    fn live_write_restores_hypershift_after_normal() {
        let kb = FakeKeyboard::new(&[A]);
        let base = read_snapshot(&kb, spec(), 0, &[A], |_, _| {}).unwrap();
        let mut want = base.clone();
        want.normal.get_mut(&A).unwrap().fn_id = 0x00;
        want.normal.get_mut(&A).unwrap().fn_data.clear();
        write(&kb, spec(), 0, &base, &want).unwrap();
        assert_eq!(kb.key_in(0, 0, A).fn_id, 0x00);
        assert_eq!(kb.key_in(0, 1, A).fn_id, 0x02, "Hypershift must be written back after the Normal write");
    }

    #[test]
    fn menu_override_reports_the_code_on_fn_menu() {
        let kb = FakeKeyboard::new(&[MENU]);
        let s = read_snapshot(&kb, spec(), 0, &[MENU], |_, _| {}).unwrap().with_menu_override();
        assert_eq!((s.hypershift[&MENU].fn_id, s.hypershift[&MENU].fn_data.as_slice()), (0x11, &[NEXT_PROFILE_CODE][..]));
        assert_eq!(s.normal[&MENU].fn_id, 0x02);
    }

    #[test]
    fn custom_look_is_not_written_to_a_slot() {
        let kb = FakeKeyboard::new(&[A]);
        let base = read_snapshot(&kb, spec(), 2, &[A], |_, _| {}).unwrap();
        let mut custom = look("custom");
        custom.effect.colors = Some([(A, [1, 2, 3])].into());
        let want = Snapshot { look: Some(custom), ..base.clone() };
        let after = write(&kb, spec(), 2, &base, &want).unwrap();
        assert_eq!(after.look, base.look);
        assert!(want.matches(&after));
    }

    #[test]
    fn a_new_look_goes_to_the_slot() {
        let kb = FakeKeyboard::new(&[A]);
        let base = read_snapshot(&kb, spec(), 2, &[A], |_, _| {}).unwrap();
        let want = Snapshot { look: Some(look("spectrum")), ..base.clone() };
        write(&kb, spec(), 2, &base, &want).unwrap();
        assert_eq!(crate::lighting::get_look(&kb, spec(), Store::Slot(2)).unwrap(), want.look);
    }
}
