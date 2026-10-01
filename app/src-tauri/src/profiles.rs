//! The app's profiles and the keyboard's flash slots they are written to.

use std::collections::BTreeMap;
use std::collections::btree_map::Entry;

use razer_core::actuation::LIVE;
use razer_core::analog::KeyAssignment;
use razer_core::devices::DeviceSpec;
use razer_core::profiles::{self as slots, Snapshot};
use razer_core::transport::{Error, Transport};

use crate::settings::{Profile, Settings};

/// Reads uncached listed slots, drops gone ones, imports slots no profile owns; returns the listed slots.
pub fn sync_slots(
    t: &impl Transport,
    d: &DeviceSpec,
    s: &mut Settings,
    keys: &[u8],
    startup: u8,
    fallback_name: &dyn Fn(u8) -> String,
    mut progress: impl FnMut(usize, &KeyAssignment),
) -> Result<Vec<u8>, Error> {
    let listed = slots::list(t)?;
    s.slots.retain(|k, _| listed.contains(k));
    for p in &mut s.profiles {
        if p.slot.is_some_and(|k| !listed.contains(&k)) {
            p.slot = None;
        }
    }
    // The startup slot first: its keys are what the window shows while it reads.
    let mut order = listed.clone();
    order.sort_by_key(|&k| k != startup);
    for k in order {
        if let Entry::Vacant(e) = s.slots.entry(k) {
            let snap = if k == startup {
                slots::read_snapshot(t, d, k, keys, &mut progress)?
            } else {
                slots::read_snapshot(t, d, k, keys, |_, _| {})?
            };
            e.insert(snap);
        }
        if !s.profiles.iter().any(|p| p.slot == Some(k)) {
            let name = slots::read_name(t, k).ok().filter(|n| !n.trim().is_empty()).unwrap_or_else(|| fallback_name(k));
            let id = s.new_profile_id();
            s.profiles.push(Profile { id, name, slot: Some(k), data: s.slots[&k].clone(), rapid: BTreeMap::new(), custom: None });
        }
    }
    if s.loaded_profile().is_none() {
        s.loaded = s.profiles.iter().find(|p| p.slot == Some(startup)).or(s.profiles.first()).map(|p| p.id);
    }
    Ok(listed)
}

/// Profile `id` into profile 0, from what it holds now (`base`); returns what it holds after.
pub fn load(t: &impl Transport, d: &DeviceSpec, s: &Settings, id: u32, base: &Snapshot, menu: bool) -> Result<Snapshot, Error> {
    let p = s.profile(id).ok_or_else(|| Error::BadArgument(format!("no profile {id}")))?;
    let want = if menu { p.data.with_menu_override() } else { p.data.clone() };
    slots::write(t, d, LIVE, base, &want)
}

/// Normal keys whose press point / binding differ from the profile's slot; none without a slot.
pub fn unsaved(p: &Profile, slots: &BTreeMap<u8, Snapshot>) -> (Vec<u8>, Vec<u8>) {
    let Some(slot) = p.slot.and_then(|k| slots.get(&k)) else { return (Vec::new(), Vec::new()) };
    let (mut thr, mut bind) = (Vec::new(), Vec::new());
    for (&k, r) in &p.data.normal {
        let f = slot.normal.get(&k);
        if f.is_none_or(|f| f.thr_low != r.thr_low) {
            thr.push(k);
        }
        if f.is_none_or(|f| (f.fn_id, &f.fn_data) != (r.fn_id, &r.fn_data)) {
            bind.push(k);
        }
    }
    (thr, bind)
}

/// Whether the profile differs from its slot in anything the slot holds.
pub fn is_unsaved(p: &Profile, slots: &BTreeMap<u8, Snapshot>) -> bool {
    p.slot.is_some_and(|k| slots.get(&k).is_none_or(|slot| !p.data.matches(slot)))
}

/// The profile after the loaded one, round the list.
pub fn next(s: &Settings) -> Option<u32> {
    let i = s.profiles.iter().position(|p| Some(p.id) == s.loaded)?;
    s.profiles.get((i + 1) % s.profiles.len()).map(|p| p.id)
}

#[cfg(test)]
mod tests {
    use super::*;
    use razer_core::devices::{self, DeviceSpec};
    use razer_core::fake::FakeKeyboard;
    use razer_core::lighting::{Effect, Look};
    use razer_core::profiles as slots;

    const A: u8 = 31;
    const S: u8 = 32;
    const KEYS: &[u8] = &[A, S, slots::MENU];

    fn spec() -> &'static DeviceSpec {
        devices::by_pid(0x0266).unwrap()
    }

    fn name(k: u8) -> String {
        format!("Slot {k}")
    }

    fn synced(kb: &FakeKeyboard) -> Settings {
        let mut s = Settings::default();
        sync_slots(kb, spec(), &mut s, KEYS, 1, &name, |_, _| {}).unwrap();
        s
    }

    #[test]
    fn first_sync_imports_every_slot_and_loads_the_startup_one() {
        let kb = FakeKeyboard::new(KEYS);
        slots::create(&kb, 3).unwrap();
        kb.names.borrow_mut().insert(3, vec![0x41; 200]);
        let mut seen = 0;
        let mut s = Settings::default();
        let listed = sync_slots(&kb, spec(), &mut s, KEYS, 1, &name, |n, _| seen = n).unwrap();
        assert_eq!(listed, [1, 3]);
        assert_eq!(seen, KEYS.len(), "progress follows the startup slot");
        let names: Vec<_> = s.profiles.iter().map(|p| (p.name.as_str(), p.slot)).collect();
        assert_eq!(names, [("default", Some(1)), ("Slot 3", Some(3))]);
        assert_eq!(s.loaded_profile().unwrap().slot, Some(1));
        assert_eq!(s.slots.len(), 2);
    }

    #[test]
    fn a_slot_deleted_elsewhere_unbinds_its_profile() {
        let kb = FakeKeyboard::new(KEYS);
        slots::create(&kb, 2).unwrap();
        let mut s = synced(&kb);
        slots::delete(&kb, 2).unwrap();
        sync_slots(&kb, spec(), &mut s, KEYS, 1, &name, |_, _| {}).unwrap();
        assert_eq!(s.profiles.len(), 2);
        assert!(s.profiles.iter().any(|p| p.slot.is_none()));
        assert!(!s.slots.contains_key(&2));
    }

    #[test]
    fn cached_slots_are_not_read_again() {
        let kb = FakeKeyboard::new(KEYS);
        let mut s = synced(&kb);
        let sent = kb.sent.borrow().len();
        sync_slots(&kb, spec(), &mut s, KEYS, 1, &name, |_, _| {}).unwrap();
        assert_eq!(kb.sent.borrow().len(), sent + 1, "only the list");
    }

    #[test]
    fn loading_writes_only_what_differs_and_overrides_fn_menu() {
        let kb = FakeKeyboard::new(KEYS);
        let mut s = synced(&kb);
        let id = s.loaded.unwrap();
        s.profile_mut(id).unwrap().data.normal.get_mut(&S).unwrap().thr_low = 99;
        let base = s.slots[&1].clone();
        let after = load(&kb, spec(), &s, id, &base, true).unwrap();
        assert_eq!(kb.live_key(S).threshold_low, 99);
        assert_eq!(kb.key_in(0, 1, slots::MENU).fn_id, 0x11);
        assert_eq!(after.hypershift[&slots::MENU].fn_data, [slots::NEXT_PROFILE_CODE]);
        let sent = kb.sent.borrow().len();
        load(&kb, spec(), &s, id, &after, true).unwrap();
        assert_eq!(kb.sent.borrow().len(), sent, "nothing left to write");
    }

    #[test]
    fn load_after_reload_uses_the_startup_slot_as_base() {
        let kb = FakeKeyboard::new(KEYS);
        let mut s = synced(&kb);
        let id = s.loaded.unwrap();
        s.profile_mut(id).unwrap().data.normal.get_mut(&A).unwrap().thr_low = 50;
        let after = load(&kb, spec(), &s, id, &s.slots[&1], true).unwrap();
        slots::activate(&kb, 1).unwrap();
        assert_eq!(kb.live_key(A).threshold_low, 0, "the firmware reloaded profile 0");
        load(&kb, spec(), &s, id, &s.slots[&1], true).unwrap();
        assert_eq!(kb.live_key(A).threshold_low, 50);
        assert_ne!(after, s.slots[&1]);
    }

    #[test]
    fn unsaved_lists_keys_that_differ_from_the_slot() {
        let kb = FakeKeyboard::new(KEYS);
        let mut s = synced(&kb);
        let id = s.loaded.unwrap();
        let p = s.profile_mut(id).unwrap();
        p.data.normal.get_mut(&A).unwrap().thr_low = 10;
        p.data.normal.get_mut(&S).unwrap().fn_id = 0;
        let p = s.profile(id).unwrap();
        assert_eq!(unsaved(p, &s.slots), (vec![A], vec![S]));
        assert!(is_unsaved(p, &s.slots));
        let free = Profile { slot: None, ..p.clone() };
        assert_eq!(unsaved(&free, &s.slots), (vec![], vec![]));
        assert!(!is_unsaved(&free, &s.slots));
    }

    #[test]
    fn custom_look_does_not_make_a_profile_unsaved() {
        let kb = FakeKeyboard::new(KEYS);
        let mut s = synced(&kb);
        let id = s.loaded.unwrap();
        let custom = Look { effect: Effect { name: "custom".into(), colors: Some([(A, [1, 2, 3])].into()), ..Default::default() }, brightness: 9 };
        s.profile_mut(id).unwrap().data.look = Some(custom);
        assert!(!is_unsaved(s.profile(id).unwrap(), &s.slots));
    }

    #[test]
    fn next_goes_round_the_list() {
        let kb = FakeKeyboard::new(KEYS);
        slots::create(&kb, 2).unwrap();
        let mut s = synced(&kb);
        let ids: Vec<u32> = s.profiles.iter().map(|p| p.id).collect();
        s.loaded = Some(ids[0]);
        assert_eq!(next(&s), Some(ids[1]));
        s.loaded = Some(ids[1]);
        assert_eq!(next(&s), Some(ids[0]));
    }
}
