//! Reading and applying per-key press points in the normal layer of one profile.

use std::fmt::Write as _;

use crate::analog::{self, KeyAssignment, MAX_MM, MIN_MM, Layer};
use crate::keymap;
use crate::transport::{Error, Transport, exchange};

/// Profile 0: the working copy the keyboard types with. It is not persisted and comes back from
/// the active profile after a replug.
pub const LIVE: u8 = 0;

pub fn read_key(t: &impl Transport, profile: u8, key: u8) -> Result<KeyAssignment, Error> {
    let r = exchange(
        t,
        analog::GET_KEY_ASSIGNMENT,
        analog::KEY_ASSIGNMENT_SIZE,
        &[profile, key, Layer::Normal as u8],
    )?;
    analog::parse(r.data()).ok_or(Error::ShortReply(analog::GET_KEY_ASSIGNMENT))
}

pub fn write_key(t: &impl Transport, a: &KeyAssignment) -> Result<(), Error> {
    exchange(t, analog::SET_KEY_ASSIGNMENT, analog::KEY_ASSIGNMENT_SIZE, &analog::set_args(a)).map(|_| ())
}

pub fn read_all(
    t: &impl Transport,
    profile: u8,
    keys: &[u8],
    mut progress: impl FnMut(usize, &KeyAssignment),
) -> Result<Vec<KeyAssignment>, Error> {
    keys.iter()
        .enumerate()
        .map(|(i, &k)| {
            let a = read_key(t, profile, k)?;
            progress(i + 1, &a);
            Ok(a)
        })
        .collect()
}

/// Result of writing one key; the assignment is what the device reported afterwards.
#[derive(Clone, Debug, PartialEq)]
pub enum Outcome {
    Ok(KeyAssignment),
    Unconfirmed(KeyAssignment),
    Failed(Error),
}

/// Sets the press point of each `(key, mm)`; one failing key does not stop the rest.
pub fn apply(t: &impl Transport, profile: u8, changes: &[(u8, f32)]) -> Vec<(u8, Outcome)> {
    apply_with(t, profile, changes, set_mm)
}

/// Writes the profile and then the live copy, so the change takes effect without a replug.
pub fn save(t: &impl Transport, profile: u8, changes: &[(u8, f32)]) -> Vec<(u8, Outcome)> {
    save_with(t, profile, changes, true, set_mm)
}

type Set<T, C> = fn(&T, u8, u8, &C) -> Result<Outcome, Error>;

pub(crate) fn apply_with<T: Transport, C>(t: &T, profile: u8, changes: &[(u8, C)], set: Set<T, C>) -> Vec<(u8, Outcome)> {
    changes.iter().map(|(key, c)| (*key, set(t, profile, *key, c).unwrap_or_else(Outcome::Failed))).collect()
}

/// Without `live` only the profile is written and judged.
pub(crate) fn save_with<T: Transport, C>(t: &T, profile: u8, changes: &[(u8, C)], live: bool, set: Set<T, C>) -> Vec<(u8, Outcome)> {
    apply_with(t, profile, changes, set)
        .into_iter()
        .zip(changes)
        .map(|((key, saved), (_, c))| match saved {
            Outcome::Failed(_) => (key, saved),
            _ if !live => (key, saved),
            _ => match set(t, LIVE, key, c) {
                Ok(Outcome::Ok(_)) => (key, saved),
                Ok(live) => (key, live),
                Err(e) => (key, Outcome::Failed(e)),
            },
        })
        .collect()
}

/// Live assignments of `keys` whose press point differs from `profile`: what a replug would lose.
pub fn unsaved(t: &impl Transport, profile: u8, keys: &[u8]) -> Result<Vec<KeyAssignment>, Error> {
    let mut out = Vec::new();
    for &k in keys {
        let live = read_key(t, LIVE, k)?;
        if live.threshold_low != read_key(t, profile, k)?.threshold_low {
            out.push(live);
        }
    }
    Ok(out)
}

fn set_mm(t: &impl Transport, profile: u8, key: u8, &mm: &f32) -> Result<Outcome, Error> {
    // The UI works in 0.1 mm steps; rounding absorbs float drift from the slider.
    let mm = (mm * 10.0).round() / 10.0;
    if !(MIN_MM..=MAX_MM).contains(&mm) {
        return Err(Error::BadArgument(format!("{mm} mm is outside {MIN_MM}..={MAX_MM}")));
    }
    update(t, profile, key, |a| a.threshold_low = analog::mm_to_threshold(mm))
}

/// Changes one key's assignment and reads it back to confirm.
pub(crate) fn update(t: &impl Transport, profile: u8, key: u8, change: impl FnOnce(&mut KeyAssignment)) -> Result<Outcome, Error> {
    let mut wanted = read_key(t, profile, key)?;
    change(&mut wanted);
    write_key(t, &wanted)?;
    let stored = read_key(t, profile, key)?;
    Ok(if stored == wanted { Outcome::Ok(stored) } else { Outcome::Unconfirmed(stored) })
}

/// Plain-text snapshot, one key per line, same fields as `razer-probe dump`.
pub fn format_backup(profile: u8, keys: &[KeyAssignment]) -> String {
    let mut out = format!("profile {profile}\n");
    for a in keys {
        let fn_data: Vec<String> = a.fn_data.iter().map(|b| format!("{b:02X}")).collect();
        let _ = writeln!(
            out,
            "{:3} {:<22} low {:3} high {:3} fn {:02X} [{}]",
            a.key,
            keymap::name(a.key).unwrap_or("?"),
            a.threshold_low,
            a.threshold_high,
            a.fn_id,
            fn_data.join(" ")
        );
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fake::FakeKeyboard;
    use crate::packet::Status;

    const A: u8 = 31;
    const S: u8 = 32;
    const W: u8 = 18;

    #[test]
    fn read_all_returns_every_key_and_reports_progress() {
        let kb = FakeKeyboard::new(&[A, S, W]);
        let mut seen = Vec::new();
        let all = read_all(&kb, 1, &[A, S, W], |n, a| seen.push((n, a.key))).unwrap();
        assert_eq!(all.iter().map(|a| a.key).collect::<Vec<_>>(), [A, S, W]);
        assert_eq!(seen, [(1, A), (2, S), (3, W)]);
    }

    #[test]
    fn apply_changes_only_the_press_point() {
        let kb = FakeKeyboard::new(&[A]);
        kb.edit(1, 0, A, |a| a.threshold_high = 77);
        let before = kb.key(A);
        let expected = KeyAssignment { threshold_low: 109, ..before };
        assert_eq!(apply(&kb, 1, &[(A, 2.4)]), [(A, Outcome::Ok(expected.clone()))]);
        assert_eq!(kb.key(A), expected);
    }

    #[test]
    fn apply_continues_after_a_failed_key() {
        let mut kb = FakeKeyboard::new(&[A, S, W]);
        kb.failing_writes.insert(S);
        let r = apply(&kb, 1, &[(A, 2.0), (S, 2.0), (W, 2.0)]);
        assert!(matches!(r[0], (A, Outcome::Ok(_))), "{r:?}");
        assert!(matches!(r[1], (S, Outcome::Failed(Error::Status(_, Status::Fail)))), "{r:?}");
        assert!(matches!(r[2], (W, Outcome::Ok(_))), "{r:?}");
    }

    #[test]
    fn unconfirmed_when_read_back_differs() {
        let mut kb = FakeKeyboard::new(&[A]);
        kb.ignored_writes.insert(A);
        assert_eq!(apply(&kb, 1, &[(A, 3.0)]), [(A, Outcome::Unconfirmed(kb.key(A)))]);
        assert_eq!(kb.key(A).threshold_low, 0);
    }

    #[test]
    fn out_of_range_is_rejected_without_touching_the_device() {
        let kb = FakeKeyboard::new(&[A]);
        let r = apply(&kb, 1, &[(A, 3.7), (A, 1.44), (A, f32::NAN)]);
        assert!(r.iter().all(|(_, o)| matches!(o, Outcome::Failed(Error::BadArgument(_)))), "{r:?}");
        assert!(kb.sent.borrow().is_empty());
    }

    #[test]
    fn slider_float_drift_is_rounded_to_a_tenth() {
        let kb = FakeKeyboard::new(&[A, S]);
        let r = apply(&kb, 1, &[(A, 3.600_000_1), (S, 1.499_999_9)]);
        assert!(matches!(&r[0].1, Outcome::Ok(a) if a.threshold_low == 255), "{r:?}");
        assert!(matches!(&r[1].1, Outcome::Ok(a) if a.threshold_low == 0), "{r:?}");
    }

    #[test]
    fn unplugged_device_fails_every_key() {
        let kb = FakeKeyboard::new(&[A, S]);
        kb.unplugged.set(true);
        let r = apply(&kb, 1, &[(A, 2.0), (S, 2.0)]);
        assert!(r.iter().all(|(_, o)| matches!(o, Outcome::Failed(Error::Io(_)))), "{r:?}");
    }

    #[test]
    fn unplugging_mid_apply_fails_the_remaining_keys() {
        let mut kb = FakeKeyboard::new(&[A, S, W]);
        kb.unplug_after = Some(4);
        let r = apply(&kb, 1, &[(A, 2.0), (S, 2.0), (W, 2.0)]);
        assert!(matches!(r[0], (A, Outcome::Ok(_))), "{r:?}");
        assert!(matches!(r[1], (S, Outcome::Failed(Error::Io(_)))), "{r:?}");
        assert!(matches!(r[2], (W, Outcome::Failed(Error::Io(_)))), "{r:?}");
    }

    #[test]
    fn empty_changes_send_nothing() {
        let kb = FakeKeyboard::new(&[A]);
        assert!(apply(&kb, 1, &[]).is_empty());
        assert!(kb.sent.borrow().is_empty());
    }

    #[test]
    fn write_key_stores_the_whole_assignment() {
        let kb = FakeKeyboard::new(&[A]);
        let a = KeyAssignment { threshold_low: 143, threshold_high: 111, ..kb.key(A) };
        write_key(&kb, &a).unwrap();
        assert_eq!(read_key(&kb, 1, A).unwrap(), a);
    }

    #[test]
    fn backup_lists_every_key() {
        let kb = FakeKeyboard::new(&[A, S]);
        let all = read_all(&kb, 1, &[A, S], |_, _| {}).unwrap();
        let text = format_backup(1, &all);
        assert!(text.starts_with("profile 1\n"), "{text}");
        assert!(text.lines().any(|l| l.starts_with(" 31 A ") && l.ends_with("fn 02 [00 1F]")), "{text}");
        assert!(text.lines().any(|l| l.starts_with(" 32 S ")), "{text}");
    }

    #[test]
    fn applying_to_the_live_copy_leaves_the_profile_alone() {
        let kb = FakeKeyboard::new(&[A]);
        let r = apply(&kb, LIVE, &[(A, 3.6)]);
        assert!(matches!(&r[0].1, Outcome::Ok(a) if a.threshold_low == 255), "{r:?}");
        assert_eq!(kb.live_key(A).threshold_low, 255);
        assert_eq!(kb.key(A).threshold_low, 0);
    }

    #[test]
    fn save_updates_the_profile_and_the_live_copy() {
        let kb = FakeKeyboard::new(&[A, S]);
        let r = save(&kb, 1, &[(A, 3.6)]);
        assert!(matches!(&r[0].1, Outcome::Ok(a) if a.profile == 1 && a.threshold_low == 255), "{r:?}");
        assert_eq!(kb.key(A).threshold_low, 255);
        assert_eq!(kb.live_key(A).threshold_low, 255);
        assert_eq!(kb.live_key(S).threshold_low, 0);
    }

    #[test]
    fn save_fails_the_key_when_the_live_copy_is_not_updated() {
        let mut kb = FakeKeyboard::new(&[A]);
        kb.unplug_after = Some(3);
        let r = save(&kb, 1, &[(A, 3.6)]);
        assert!(matches!(r[0], (A, Outcome::Failed(Error::Io(_)))), "{r:?}");
    }

    #[test]
    fn unsaved_lists_live_press_points_that_differ_from_the_profile() {
        let kb = FakeKeyboard::new(&[A, S, W]);
        apply(&kb, LIVE, &[(A, 2.0), (S, 1.5)]);
        let u = unsaved(&kb, 1, &[A, S, W]).unwrap();
        assert_eq!(u.iter().map(|a| (a.profile, a.key)).collect::<Vec<_>>(), [(LIVE, A)]);
    }
}
