//! In driver mode the firmware reports the volume wheel as a mouse wheel on MI_02.
//! A low-level hook swallows those notches and sends volume keys instead.
//!
//! The hook cannot tell devices apart, but Raw Input can, and Windows queues the
//! `WM_INPUT` of a wheel notch before it calls the hook for it (checked on hardware).

use std::collections::{HashMap, VecDeque};
use std::sync::{LazyLock, Mutex, MutexGuard};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use razer_core::hid::{self, Unit};
use razer_core::rapid::Media;
use windows_sys::Win32::UI::WindowsAndMessaging::WHEEL_DELTA;

use crate::engine;
use crate::sys::{self, WheelEvents};

/// A queued notch older than this lost its hook call to another hook that swallowed it.
const STALE: Duration = Duration::from_millis(100);

/// The keyboards whose wheel is handled. Windows keeps one Raw Input target per usage and
/// process, and the hook sees every notch, so all keyboards share one window and one hook.
#[derive(Default)]
struct Registry {
    units: Vec<(u64, Unit)>,
    next: u64,
    /// Which unit each Raw Input device belongs to; resolving one takes CfgMgr calls.
    owners: HashMap<isize, Option<u64>>,
    /// Wheel notches Raw Input saw, oldest first: when, and the unit that sent it.
    seen: VecDeque<(Instant, Option<u64>)>,
}

impl Registry {
    /// Returns the unit's id and whether it is the first, which starts Raw Input.
    fn add(&mut self, unit: Unit) -> (u64, bool) {
        let first = self.units.is_empty();
        self.next += 1;
        self.units.push((self.next, unit));
        self.owners.clear();
        (self.next, first)
    }

    /// Returns whether no unit is left, which stops Raw Input.
    fn remove(&mut self, id: u64) -> bool {
        self.units.retain(|(i, _)| *i != id);
        self.owners.clear();
        self.units.is_empty()
    }

    fn owner(&self, pid: u16, container: Option<&str>) -> Option<u64> {
        self.units.iter().find(|(_, u)| u.owns(pid, container)).map(|(i, _)| *i)
    }

    /// Takes the unit of the oldest notch that is not stale.
    fn take(&mut self) -> Option<u64> {
        while self.seen.front().is_some_and(|(at, _)| at.elapsed() > STALE) {
            self.seen.pop_front();
        }
        self.seen.pop_front().and_then(|(_, owner)| owner)
    }
}

static REGISTRY: LazyLock<Mutex<Registry>> = LazyLock::new(Mutex::default);
/// Held across a unit joining or leaving, so a new thread never starts while the old one still owns Raw Input.
static THREAD: Mutex<Option<(u32, JoinHandle<()>)>> = Mutex::new(None);

fn registry() -> MutexGuard<'static, Registry> {
    REGISTRY.lock().unwrap_or_else(|e| e.into_inner())
}

/// One keyboard's share of the wheel handling; the last one dropped stops it.
pub struct Wheel {
    id: u64,
}

impl Wheel {
    pub fn start(unit: &Unit) -> Self {
        let mut thread = THREAD.lock().unwrap_or_else(|e| e.into_inner());
        let (id, first) = registry().add(unit.clone());
        if first {
            let (tx, rx) = std::sync::mpsc::channel();
            let join = std::thread::spawn(move || {
                // A thread has no message queue until it first asks for one; without it a WM_QUIT
                // posted before `run_wheel` gets there fails and the join in Drop would never return.
                let _ = tx.send(sys::start_message_queue());
                sys::run_wheel(WheelEvents { notch: record, turn });
            });
            *thread = Some((rx.recv().unwrap_or(0), join));
        }
        Self { id }
    }
}

impl Drop for Wheel {
    fn drop(&mut self) {
        let mut thread = THREAD.lock().unwrap_or_else(|e| e.into_inner());
        let last = registry().remove(self.id);
        if last && let Some((tid, join)) = thread.take() {
            // Posted before the loop starts it stays queued, so the loop still sees it. It can only
            // fail once the thread has exited, which the join below then returns from at once.
            sys::quit_thread(tid);
            let _ = join.join();
        }
    }
}

/// Queues a wheel notch from Raw Input with its source.
fn record(device: isize) {
    let mut r = registry();
    let owner = match r.owners.get(&device) {
        Some(o) => *o,
        None => {
            let o = identify(device).and_then(|(pid, container)| r.owner(pid, container.as_deref()));
            r.owners.insert(device, o);
            o
        }
    };
    r.seen.push_back((Instant::now(), owner));
}

/// Raw Input names are device interface paths, so they resolve to a container like hidapi's.
fn identify(device: isize) -> Option<(u16, Option<String>)> {
    let name = sys::raw_input_device_name(device)?;
    let upper = name.to_uppercase();
    let rest = upper.split_once("VID_1532&PID_")?.1;
    let pid = u16::from_str_radix(rest.get(..4)?, 16).ok()?;
    if !rest[4..].starts_with("&MI_02") {
        return None;
    }
    let path = std::ffi::CString::new(name).ok()?;
    Some((pid, hid::container_id(&path)))
}

/// Swallows a wheel turn from a registered keyboard and sends volume keys instead.
fn turn(delta: i16) -> bool {
    if !from_keyboard() {
        return false;
    }
    let media = if delta > 0 { Media::VolumeUp } else { Media::VolumeDown };
    for _ in 0..(delta.unsigned_abs() / WHEEL_DELTA as u16).max(1) {
        engine::send_media(media);
    }
    true
}

/// Whether the notch the hook is handling came from a registered keyboard.
fn from_keyboard() -> bool {
    sys::drain_raw_input() && registry().take().is_some()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn unit(container: &str) -> Unit {
        Unit { spec: razer_core::devices::by_pid(0x0266).unwrap(), container: Some(container.into()) }
    }

    /// Wheel::start and drop share one global thread, so these tests must not overlap.
    static SERIAL: Mutex<()> = Mutex::new(());

    #[test]
    fn dropping_right_after_start_does_not_hang() {
        let _serial = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
        let (tx, rx) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            for _ in 0..200 {
                drop(Wheel::start(&unit("A")));
            }
            let _ = tx.send(());
        });
        assert!(rx.recv_timeout(Duration::from_secs(20)).is_ok(), "an early drop never stopped the thread");
    }

    #[test]
    fn units_share_one_thread() {
        let _serial = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
        let a = Wheel::start(&unit("A"));
        let tid = THREAD.lock().unwrap().as_ref().map(|(t, _)| *t);
        let b = Wheel::start(&unit("B"));
        assert_eq!(THREAD.lock().unwrap().as_ref().map(|(t, _)| *t), tid);
        drop(a);
        assert!(THREAD.lock().unwrap().is_some(), "B still needs the thread");
        drop(b);
        assert!(THREAD.lock().unwrap().is_none());
    }

    #[test]
    fn raw_input_runs_while_any_unit_remains() {
        let mut r = Registry::default();
        let (a, first_a) = r.add(unit("A"));
        let (b, first_b) = r.add(unit("B"));
        assert!(first_a && !first_b);
        assert!(!r.remove(a), "B still needs the registration");
        assert!(r.remove(b));
        assert!(r.add(unit("C")).1, "a later unit registers again");
    }

    #[test]
    fn a_record_goes_to_the_unit_that_owns_the_device() {
        let mut r = Registry::default();
        let (a, _) = r.add(unit("A"));
        let (b, _) = r.add(unit("B"));
        assert_eq!(r.owner(0x0266, Some("A")), Some(a));
        assert_eq!(r.owner(0x0266, Some("B")), Some(b));
        assert_eq!(r.owner(0x0266, Some("Z")), None);
        r.remove(b);
        assert_eq!(r.owner(0x0266, Some("B")), None, "a gone unit no longer claims notches");
    }

    #[test]
    fn take_skips_stale_and_foreign_notches() {
        let mut r = Registry::default();
        r.seen.push_back((Instant::now() - STALE * 2, Some(1)));
        r.seen.push_back((Instant::now(), None));
        r.seen.push_back((Instant::now(), Some(2)));
        assert_eq!(r.take(), None);
        assert_eq!(r.take(), Some(2));
    }
}
