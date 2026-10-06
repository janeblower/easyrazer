//! The app's profiles and the keyboard's flash slots they are written to.

use std::collections::BTreeMap;
use std::collections::btree_map::Entry;

use razer_core::actuation::LIVE;
use razer_core::analog::KeyAssignment;
use razer_core::devices::DeviceSpec;
use razer_core::profiles::{self as slots, Snapshot};
use razer_core::transport::{Error, Transport};

use crate::settings::{Profile, Settings, SnapTap};

/// Reads uncached listed slots, drops gone ones, imports slots no profile owns.
pub fn sync_slots(
    t: &impl Transport,
    d: &DeviceSpec,
    s: &mut Settings,
    keys: &[u8],
    startup: u8,
    fallback_name: &dyn Fn(u8) -> String,
    mut progress: impl FnMut(usize, usize, Option<&KeyAssignment>),
) -> Result<(), Error> {
    let listed = slots::list(t)?;
    s.slots.retain(|k, _| listed.contains(k));
    for p in &mut s.profiles {
        if p.slot.is_some_and(|k| !listed.contains(&k)) {
            p.slot = None;
        }
    }
    // The startup slot first: its keys are what the window shows while it reads.
    let mut order = listed;
    order.sort_by_key(|&k| k != startup);
    // Both layers of every slot not cached yet; the window shows this as one progress.
    let per_slot = 2 * keys.len();
    let total = per_slot * order.iter().filter(|k| !s.slots.contains_key(k)).count();
    let mut done = 0;
    for k in order {
        if let Entry::Vacant(e) = s.slots.entry(k) {
            let snap = slots::read_snapshot(t, d, k, keys, |n, a| progress(done + n, total, a.filter(|_| k == startup)))?;
            done += per_slot;
            e.insert(snap);
        }
        if !s.profiles.iter().any(|p| p.slot == Some(k)) {
            let name = slots::read_name(t, k).ok().filter(|n| !n.trim().is_empty()).unwrap_or_else(|| fallback_name(k));
            let name = free_name(s, &name, numbered);
            let id = s.new_profile_id();
            let mut data = s.slots[&k].clone();
            data.bind_snap_tap();
            s.profiles.push(Profile { id, name, slot: Some(k), data, rapid: BTreeMap::new(), snap_tap: SnapTap::default(), custom: None });
        }
    }
    if s.loaded_profile().is_none() {
        s.loaded = s.profiles.iter().find(|p| p.slot == Some(startup)).or(s.profiles.first()).map(|p| p.id);
    }
    Ok(())
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

/// Hypershift keys that differ from the profile's slot; none without a slot.
pub fn unsaved_hypershift(p: &Profile, slots: &BTreeMap<u8, Snapshot>) -> Vec<u8> {
    let Some(slot) = p.slot.and_then(|k| slots.get(&k)) else { return Vec::new() };
    p.data.hypershift.iter().filter(|&(k, r)| slot.hypershift.get(k) != Some(r)).map(|(&k, _)| k).collect()
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

fn missing(id: u32) -> Error {
    Error::BadArgument(format!("no profile {id}"))
}

/// Names differing only in case or surrounding spaces count as the same.
pub fn name_taken(s: &Settings, name: &str, except: Option<u32>) -> bool {
    let key = name.trim().to_lowercase();
    s.profiles.iter().any(|p| Some(p.id) != except && p.name.trim().to_lowercase() == key)
}

/// `suffix(n)` for the n-th try of [`free_name`]: nothing first, then " 2", " 3"…
pub fn numbered(n: usize) -> String {
    if n == 1 { String::new() } else { format!(" {n}") }
}

/// `base` with the first `suffix(n)` no profile uses; the base is cut so the suffix always fits the slot.
pub fn free_name(s: &Settings, base: &str, suffix: impl Fn(usize) -> String) -> String {
    (1..)
        .map(|n| {
            let tail = suffix(n);
            let room = slots::NAME_CHARS.saturating_sub(tail.encode_utf16().count());
            let mut units = 0;
            let head: String = base.trim().chars().take_while(|c| {
                units += c.len_utf16();
                units <= room
            }).collect();
            slots::fit_name(&format!("{}{tail}", head.trim_end()))
        })
        .find(|name| !name.is_empty() && !name_taken(s, name, None))
        .expect("numbered suffixes never run out")
}

/// Copy of `from` without a slot, right after it.
pub fn copy(s: &mut Settings, from: u32, name: String) -> Option<u32> {
    let i = s.profiles.iter().position(|p| p.id == from)?;
    let mut name = slots::fit_name(&name);
    if name.is_empty() {
        name = slots::fit_name(&s.profiles[i].name);
    }
    let id = s.new_profile_id();
    let copy = Profile { id, name, slot: None, ..s.profiles[i].clone() };
    s.profiles.insert(i + 1, copy);
    Some(id)
}

/// Fitted to the slot's name; returns the slot whose name must be written too.
pub fn rename(s: &mut Settings, id: u32, name: &str) -> Result<Option<u8>, Error> {
    let p = s.profile_mut(id).ok_or_else(|| missing(id))?;
    p.name = slots::fit_name(name);
    Ok(p.slot)
}

/// `05:04` on the profile's slot; returns it.
pub fn set_startup(t: &impl Transport, s: &Settings, id: u32) -> Result<u8, Error> {
    let k = s.profile(id).ok_or_else(|| missing(id))?.slot.ok_or_else(|| Error::BadArgument("profile has no slot".into()))?;
    slots::activate(t, k)?;
    Ok(k)
}

/// Unlists the slot, switching the startup slot away from it first; returns the startup slot after.
pub fn free_slot(t: &impl Transport, s: &mut Settings, id: u32, startup: u8) -> Result<u8, Error> {
    let k = s.profile(id).ok_or_else(|| missing(id))?.slot.ok_or_else(|| Error::BadArgument("profile has no slot".into()))?;
    let listed = slots::list(t)?;
    // How the firmware behaves without any profile is unknown.
    let Some(&other) = listed.iter().find(|&&x| x != k) else {
        return Err(Error::BadArgument("the last slot cannot be freed".into()));
    };
    let startup = if startup == k {
        slots::activate(t, other)?;
        other
    } else {
        startup
    };
    slots::delete(t, k)?;
    s.slots.remove(&k);
    if let Some(p) = s.profile_mut(id) {
        p.slot = None;
    }
    Ok(startup)
}

/// Removes a profile that holds no slot, loading a neighbour if it was loaded.
pub fn remove(s: &mut Settings, id: u32) -> Result<(), Error> {
    if s.profiles.len() <= 1 {
        return Err(Error::BadArgument("the only profile cannot be deleted".into()));
    }
    let i = s.profiles.iter().position(|p| p.id == id).ok_or_else(|| missing(id))?;
    s.profiles.remove(i);
    if s.loaded == Some(id) {
        s.loaded = s.profiles.get(i).or(s.profiles.get(i.saturating_sub(1))).map(|p| p.id);
    }
    Ok(())
}

/// Writes the profile to its slot, taking a free one if it has none; returns the slot.
pub fn write_slot(t: &impl Transport, d: &DeviceSpec, s: &mut Settings, id: u32, keys: &[u8]) -> Result<u8, Error> {
    let p = s.profile(id).ok_or_else(|| missing(id))?.clone();
    let k = match p.slot {
        Some(k) => k,
        None => {
            let listed = slots::list(t)?;
            let k = (1..=slots::MAX_SLOTS).find(|k| !listed.contains(k)).ok_or_else(|| Error::BadArgument("no free slot".into()))?;
            slots::create(t, k)?;
            k
        }
    };
    // Read afresh: Synapse may have changed the slot, and a new one holds an old page.
    let base = slots::read_snapshot(t, d, k, keys, |_, _| {})?;
    let after = slots::write(t, d, k, &base, &p.data)?;
    if slots::read_name(t, k)? != slots::fit_name(&p.name) {
        slots::write_name(t, k, &p.name)?;
    }
    s.slots.insert(k, after);
    if let Some(p) = s.profile_mut(id) {
        p.slot = Some(k);
    }
    Ok(k)
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

    fn create(s: &mut Settings, name: &str) -> Option<u32> {
        let from = s.loaded?;
        copy(s, from, name.into())
    }

    fn synced(kb: &FakeKeyboard) -> Settings {
        let mut s = Settings::default();
        sync_slots(kb, spec(), &mut s, KEYS, 1, &name, |_, _, _| {}).unwrap();
        s
    }

    #[test]
    fn first_sync_imports_every_slot_and_loads_the_startup_one() {
        let kb = FakeKeyboard::new(KEYS);
        slots::create(&kb, 3).unwrap();
        kb.names.borrow_mut().insert(3, vec![0x41; 200]);
        let mut seen = Vec::new();
        let mut s = Settings::default();
        sync_slots(&kb, spec(), &mut s, KEYS, 1, &name, |n, total, a| seen.push((n, total, a.is_some()))).unwrap();
        let all = 2 * 2 * KEYS.len();
        assert_eq!(seen.last(), Some(&(all, all, false)), "progress covers both slots");
        assert_eq!(seen.iter().filter(|s| s.2).count(), KEYS.len(), "keys come from the startup slot only");
        let names: Vec<_> = s.profiles.iter().map(|p| (p.name.as_str(), p.slot)).collect();
        assert_eq!(names, [("default", Some(1)), ("Slot 3", Some(3))]);
        assert_eq!(s.loaded_profile().unwrap().slot, Some(1));
        assert_eq!(s.slots.len(), 2);
    }

    #[test]
    fn an_imported_profile_gets_the_snap_tap_switch() {
        let keys = &[A, slots::LEFT_SHIFT];
        let kb = FakeKeyboard::new(keys);
        let (fn_id, fn_data) = razer_core::binding::encode(razer_core::binding::factory(slots::LEFT_SHIFT)).unwrap();
        kb.edit(1, 1, slots::LEFT_SHIFT, |a| (a.fn_id, a.fn_data) = (fn_id, fn_data.clone()));
        let mut s = Settings::default();
        sync_slots(&kb, spec(), &mut s, keys, 1, &name, |_, _, _| {}).unwrap();
        let p = s.loaded_profile().unwrap();
        assert_eq!(p.data.hypershift[&slots::LEFT_SHIFT].fn_data, [razer_core::binding::SNAP_TAP_CODE]);
        assert_eq!(s.slots[&1].hypershift[&slots::LEFT_SHIFT].fn_id, 0x02, "the cache keeps what the flash holds");
    }

    #[test]
    fn a_slot_deleted_elsewhere_unbinds_its_profile() {
        let kb = FakeKeyboard::new(KEYS);
        slots::create(&kb, 2).unwrap();
        let mut s = synced(&kb);
        slots::delete(&kb, 2).unwrap();
        sync_slots(&kb, spec(), &mut s, KEYS, 1, &name, |_, _, _| {}).unwrap();
        assert_eq!(s.profiles.len(), 2);
        assert!(s.profiles.iter().any(|p| p.slot.is_none()));
        assert!(!s.slots.contains_key(&2));
    }

    #[test]
    fn cached_slots_are_not_read_again() {
        let kb = FakeKeyboard::new(KEYS);
        let mut s = synced(&kb);
        let sent = kb.sent.borrow().len();
        sync_slots(&kb, spec(), &mut s, KEYS, 1, &name, |_, _, _| {}).unwrap();
        assert_eq!(kb.sent.borrow().len(), sent + 1, "only the list");
    }

    #[test]
    fn loading_writes_only_what_differs_and_overrides_fn_menu() {
        let kb = FakeKeyboard::new(KEYS);
        let mut s = synced(&kb);
        let id = s.loaded.unwrap();
        let menu = s.profile_mut(id).unwrap().data.hypershift.get_mut(&slots::MENU).unwrap();
        (menu.fn_id, menu.fn_data) = (0x07, vec![0x04]);
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
    fn unsaved_hypershift_lists_changed_keys() {
        let kb = FakeKeyboard::new(KEYS);
        let mut s = synced(&kb);
        let id = s.loaded.unwrap();
        s.profile_mut(id).unwrap().data.hypershift.get_mut(&S).unwrap().thr_low = 99;
        assert_eq!(unsaved_hypershift(s.profile(id).unwrap(), &s.slots), [S]);
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

    #[test]
    fn create_and_duplicate_have_no_slot() {
        let kb = FakeKeyboard::new(KEYS);
        let mut s = synced(&kb);
        let first = s.loaded.unwrap();
        let a = create(&mut s, "New").unwrap();
        let b = copy(&mut s, first, "default (copy)".into()).unwrap();
        let order: Vec<(u32, Option<u8>)> = s.profiles.iter().map(|p| (p.id, p.slot)).collect();
        assert_eq!(order, [(first, Some(1)), (b, None), (a, None)]);
        assert_eq!(s.profile(a).unwrap().data, s.profile(first).unwrap().data);
    }

    #[test]
    fn rename_trims_cuts_and_reports_the_slot() {
        let kb = FakeKeyboard::new(KEYS);
        let mut s = synced(&kb);
        let id = s.loaded.unwrap();
        assert_eq!(rename(&mut s, id, "  Игры  ").unwrap(), Some(1));
        assert_eq!(s.profile(id).unwrap().name, "Игры");
        rename(&mut s, id, &"x".repeat(40)).unwrap();
        assert_eq!(s.profile(id).unwrap().name.chars().count(), slots::NAME_CHARS);
    }

    #[test]
    fn free_names_are_numbered_and_case_blind() {
        let kb = FakeKeyboard::new(KEYS);
        let mut s = synced(&kb);
        assert_eq!(free_name(&s, "New", numbered), "New");
        create(&mut s, "New").unwrap();
        assert_eq!(free_name(&s, "new", numbered), "new 2");
        assert_eq!(free_name(&s, "  New  ", numbered), "New 2");
        let copy = |n: usize| if n == 1 { " (copy)".to_string() } else { format!(" (copy {n})") };
        assert_eq!(free_name(&s, "default", copy), "default (copy)");
        create(&mut s, "default (copy)").unwrap();
        assert_eq!(free_name(&s, "default", copy), "default (copy 2)");
    }

    #[test]
    fn a_long_base_keeps_its_number() {
        let kb = FakeKeyboard::new(KEYS);
        let mut s = synced(&kb);
        let long = "x".repeat(40);
        create(&mut s, &long).unwrap();
        let next = free_name(&s, &long, numbered);
        assert!(next.ends_with(" 2") && next.encode_utf16().count() <= slots::NAME_CHARS, "{next}");
    }

    #[test]
    fn names_in_use_are_case_blind() {
        let kb = FakeKeyboard::new(KEYS);
        let mut s = synced(&kb);
        let first = s.loaded.unwrap();
        let other = create(&mut s, "Game").unwrap();
        assert!(name_taken(&s, " DEFAULT ", Some(other)));
        assert!(!name_taken(&s, "Default", Some(first)), "its own name in another case is fine");
    }

    #[test]
    fn imported_slots_with_one_name_get_numbers() {
        let kb = FakeKeyboard::new(KEYS);
        slots::create(&kb, 2).unwrap();
        let default = kb.names.borrow()[&1].clone();
        kb.names.borrow_mut().insert(2, default);
        let s = synced(&kb);
        let names: Vec<&str> = s.profiles.iter().map(|p| p.name.as_str()).collect();
        assert_eq!(names, ["default", "default 2"]);
    }

    #[test]
    fn a_copy_of_a_long_name_still_fits() {
        let kb = FakeKeyboard::new(KEYS);
        let mut s = synced(&kb);
        let id = s.loaded.unwrap();
        s.profile_mut(id).unwrap().name = "😀".repeat(15);
        let long = copy(&mut s, id, format!("{} (copy)", "😀".repeat(15))).unwrap();
        assert!(s.profile(long).unwrap().name.encode_utf16().count() <= slots::NAME_CHARS);
        let empty = copy(&mut s, id, "  ".into()).unwrap();
        assert_eq!(s.profile(empty).unwrap().name, s.profile(id).unwrap().name);
    }

    #[test]
    fn an_unchanged_profile_writes_no_name_the_second_time() {
        let kb = FakeKeyboard::new(KEYS);
        let mut s = synced(&kb);
        let id = s.loaded.unwrap();
        s.profile_mut(id).unwrap().name = "Ж".repeat(40);
        write_slot(&kb, spec(), &mut s, id, KEYS).unwrap();
        let sent = kb.sent.borrow().len();
        write_slot(&kb, spec(), &mut s, id, KEYS).unwrap();
        let names = kb.sent.borrow()[sent..].iter().filter(|&&c| c == razer_core::packet::Command::new(0x05, 0x08)).count();
        assert_eq!(names, 0);
    }

    #[test]
    fn writing_to_a_new_slot_diffs_against_the_old_page() {
        let kb = FakeKeyboard::new(KEYS);
        kb.edit(2, 0, A, |a| a.threshold_low = 200);
        let mut s = synced(&kb);
        let id = create(&mut s, "Game").unwrap();
        s.profile_mut(id).unwrap().data.normal.get_mut(&S).unwrap().thr_low = 77;
        let sent = kb.sent.borrow().len();
        assert_eq!(write_slot(&kb, spec(), &mut s, id, KEYS).unwrap(), 2);
        assert_eq!(kb.key_in(2, 0, A).threshold_low, 0, "leftover from the old page is overwritten");
        assert_eq!(kb.key_in(2, 0, S).threshold_low, 77);
        assert_eq!(slots::read_name(&kb, 2).unwrap(), "Game");
        assert_eq!(s.profile(id).unwrap().slot, Some(2));
        assert!(!is_unsaved(s.profile(id).unwrap(), &s.slots));
        let key_writes = kb.sent.borrow()[sent..].iter().filter(|&&c| c == razer_core::packet::Command::new(0x02, 0x12)).count();
        assert_eq!(key_writes, 2, "A back from the old page's 200, S to 77; nothing else differs");
    }

    #[test]
    fn no_free_slot_is_an_error() {
        let kb = FakeKeyboard::new(KEYS);
        for k in 2..=5 {
            slots::create(&kb, k).unwrap();
        }
        let mut s = synced(&kb);
        let id = create(&mut s, "Sixth").unwrap();
        assert!(write_slot(&kb, spec(), &mut s, id, KEYS).is_err());
        assert_eq!(s.profile(id).unwrap().slot, None);
    }

    #[test]
    fn freeing_the_startup_slot_switches_first() {
        let kb = FakeKeyboard::new(KEYS);
        slots::create(&kb, 2).unwrap();
        let mut s = synced(&kb);
        let id = s.profiles.iter().find(|p| p.slot == Some(1)).unwrap().id;
        assert_eq!(free_slot(&kb, &mut s, id, 1).unwrap(), 2);
        assert_eq!(slots::list(&kb).unwrap(), [2]);
        assert_eq!(kb.active.get(), 2);
        assert_eq!(s.profile(id).unwrap().slot, None);
        assert!(!s.slots.contains_key(&1));
    }

    #[test]
    fn the_last_slot_cannot_be_freed() {
        let kb = FakeKeyboard::new(KEYS);
        let mut s = synced(&kb);
        let id = s.loaded.unwrap();
        assert!(free_slot(&kb, &mut s, id, 1).is_err());
        assert_eq!(slots::list(&kb).unwrap(), [1]);
    }

    #[test]
    fn deleting_the_loaded_profile_loads_a_neighbour() {
        let kb = FakeKeyboard::new(KEYS);
        slots::create(&kb, 2).unwrap();
        let mut s = synced(&kb);
        let ids: Vec<u32> = s.profiles.iter().map(|p| p.id).collect();
        s.loaded = Some(ids[1]);
        assert_eq!(free_slot(&kb, &mut s, ids[1], 1).unwrap(), 1);
        remove(&mut s, ids[1]).unwrap();
        assert_eq!(s.loaded, Some(ids[0]));
        assert_eq!(slots::list(&kb).unwrap(), [1]);
        assert!(remove(&mut s, ids[0]).is_err(), "the only profile stays");
    }

    #[test]
    fn set_startup_activates_the_slot() {
        let kb = FakeKeyboard::new(KEYS);
        slots::create(&kb, 2).unwrap();
        let s = synced(&kb);
        let id = s.profiles.iter().find(|p| p.slot == Some(2)).unwrap().id;
        assert_eq!(set_startup(&kb, &s, id).unwrap(), 2);
        assert_eq!(kb.active.get(), 2);
        let free = Settings { profiles: vec![Profile { slot: None, ..s.profile(id).unwrap().clone() }], ..Default::default() };
        assert!(set_startup(&kb, &free, id).is_err());
    }
}
