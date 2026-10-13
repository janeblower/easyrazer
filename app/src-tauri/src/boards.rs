//! Every keyboard the app has met this session, by id; the commands reach them through here.

use std::collections::BTreeMap;
use std::sync::{Arc, Mutex, MutexGuard};

use hidapi::HidApi;
use razer_core::control;
use razer_core::hid::{self, Unit};
use serde::Serialize;

use crate::commands::try_lock;
use crate::device::{Device, Progress, Status};
use crate::engine::Sink;
use crate::settings::{self, Settings, Shared};

pub struct Board {
    pub device: Arc<Mutex<Device>>,
    /// Of the unit attached last; a replug may bring another.
    pub container: Option<String>,
    /// As of its last status; kept here so listing never waits on a device busy with a full read.
    pub connected: bool,
}

/// What each new keyboard is given to report through; set once the window exists.
pub struct Hooks {
    pub sink: Arc<dyn Fn(&str) -> Sink + Send + Sync>,
    pub progress: Arc<dyn Fn(&str) -> Progress + Send + Sync>,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct DeviceEntry {
    pub id: String,
    pub model: String,
    pub connected: bool,
    pub driver_mode: bool,
}

#[derive(Serialize)]
pub struct DevicesView {
    pub devices: Vec<DeviceEntry>,
    /// PID of a Razer keyboard without a description.
    pub unsupported: Option<u16>,
}

pub struct Boards {
    boards: Mutex<BTreeMap<String, Board>>,
    /// Enumerates for discovery; each device has its own for its interfaces.
    api: Mutex<HidApi>,
    /// PID of the unsupported Razer keyboard seen by the last discovery.
    unsupported: Mutex<Option<u16>>,
}

fn guard<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|e| e.into_inner())
}

/// Units no board holds. A board without a container holds any unit, the way it was before
/// containers were read: two such keyboards of one model cannot be told apart.
pub fn units_to_attach(units: Vec<Unit>, held: &[Option<String>]) -> Vec<Unit> {
    units.into_iter().filter(|u| !held.iter().any(|c| c.is_none() || *c == u.container)).collect()
}

impl Boards {
    pub fn new() -> Result<Self, String> {
        Ok(Self { boards: Mutex::new(BTreeMap::new()), api: Mutex::new(HidApi::new().map_err(|e| e.to_string())?), unsupported: Mutex::new(None) })
    }

    pub fn get(&self, id: &str) -> Option<Arc<Mutex<Device>>> {
        guard(&self.boards).get(id).map(|b| b.device.clone())
    }

    /// No keyboard met this session; a board merely busy still counts.
    pub fn is_empty(&self) -> bool {
        guard(&self.boards).is_empty()
    }

    pub fn all(&self) -> Vec<(String, Arc<Mutex<Device>>)> {
        guard(&self.boards).iter().map(|(id, b)| (id.clone(), b.device.clone())).collect()
    }

    fn board_for(&self, id: &str, container: Option<String>, make: impl FnOnce() -> Arc<Mutex<Device>>) -> Arc<Mutex<Device>> {
        let mut boards = guard(&self.boards);
        let b = boards.entry(id.to_string()).or_insert_with(|| Board { device: make(), container: None, connected: false });
        b.container = container;
        b.connected = true;
        b.device.clone()
    }

    pub fn set_connected(&self, id: &str, connected: bool) -> bool {
        let mut boards = guard(&self.boards);
        let Some(b) = boards.get_mut(id) else { return false };
        let changed = b.connected != connected;
        b.connected = connected;
        changed
    }

    /// `true` when the unsupported keyboard differs from the last one noted.
    fn note_unsupported(&self, now: Option<u16>) -> bool {
        std::mem::replace(&mut *guard(&self.unsupported), now) != now
    }

    /// The status of every board not busy with a command; a busy one reports for itself when it
    /// finishes, and waiting here would stall the others. `true` when a connection changed.
    pub fn statuses(&self, synapse: bool) -> (Vec<Status>, bool) {
        let mut changed = false;
        let mut out = Vec::new();
        for (id, d) in self.all() {
            let Some(mut dev) = try_lock(&d) else { continue };
            let s = dev.status(synapse);
            drop(dev);
            changed |= self.set_connected(&id, s.device);
            out.push(s);
        }
        (out, changed)
    }

    /// Attaches every unit no board holds; `true` when a keyboard appeared or the unsupported one changed.
    pub fn discover(&self, shared: &Shared, hooks: &Hooks, synapse: bool) -> bool {
        let held: Vec<Option<String>> = guard(&self.boards).values().filter(|b| b.connected).map(|b| b.container.clone()).collect();
        let mut api = guard(&self.api);
        let _ = api.refresh_devices();
        let mut changed = self.note_unsupported(hid::unsupported_keyboard(&api));
        for unit in units_to_attach(hid::units(&api), &held) {
            // A failed serial read would file the keyboard's data under another key; the next poll retries.
            let Ok(t) = hid::open_control(&api, &unit) else { continue };
            let Ok(serial) = control::serial(&t) else { continue };
            let id = settings::device_id(unit.spec.pid, &serial);
            let device = self.board_for(&id, unit.container.clone(), || {
                Arc::new(Mutex::new(Device::new(shared.clone()).expect("hidapi init")))
            });
            crate::commands::lock(&device).attach(unit, t, &id, (hooks.sink)(&id), (hooks.progress)(&id), synapse);
            changed = true;
        }
        changed
    }

    /// Keyboards met this session and those only in the settings; never locks a device.
    pub fn list(&self, s: &Settings) -> Vec<DeviceEntry> {
        let boards = guard(&self.boards);
        s.devices
            .iter()
            .map(|(id, d)| DeviceEntry {
                id: id.clone(),
                model: razer_core::devices::by_pid(d.pid).map_or_else(|| format!("{:04X}", d.pid), |spec| spec.name.clone()),
                connected: boards.get(id).is_some_and(|b| b.connected),
                driver_mode: d.driver_mode,
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use razer_core::devices;

    fn unit(container: Option<&str>) -> Unit {
        Unit { spec: devices::by_pid(0x0266).unwrap(), container: container.map(Into::into) }
    }

    #[test]
    fn units_to_attach_skips_held_containers() {
        let units = vec![unit(Some("A")), unit(Some("B")), unit(None)];
        let got: Vec<_> = units_to_attach(units, &[Some("A".into())]).into_iter().map(|u| u.container).collect();
        assert_eq!(got, [Some("B".into()), None]);
    }

    #[test]
    fn a_unit_without_container_is_held_by_any_board_of_its_model() {
        assert!(units_to_attach(vec![unit(None)], &[None]).is_empty());
    }

    #[test]
    fn list_does_not_lock_devices() {
        let boards = Boards::new().unwrap();
        let shared: Shared = Default::default();
        let device = Arc::new(Mutex::new(Device::new(shared.clone()).unwrap()));
        boards.boards.lock().unwrap().insert("A".into(), Board { device: device.clone(), container: Some("C".into()), connected: true });
        let mut s = Settings::default();
        s.claim("A", 0x0266);
        s.claim("B", 0x0266);
        let _busy = device.lock().unwrap();
        let list = boards.list(&s);
        assert_eq!(list.iter().map(|e| (e.id.as_str(), e.connected)).collect::<Vec<_>>(), [("A", true), ("B", false)]);
    }

    #[test]
    fn a_busy_device_does_not_stall_the_others() {
        let boards = Boards::new().unwrap();
        let shared: Shared = Default::default();
        let busy = Arc::new(Mutex::new(Device::new(shared.clone()).unwrap()));
        let free = Arc::new(Mutex::new(Device::new(shared).unwrap()));
        for (id, d) in [("A", &busy), ("B", &free)] {
            boards.boards.lock().unwrap().insert(id.into(), Board { device: d.clone(), container: None, connected: true });
        }
        let _held = busy.lock().unwrap();
        let (statuses, changed) = boards.statuses(false);
        assert!(statuses.len() == 1 && statuses[0].id.is_none());
        assert!(changed);
        let boards_map = boards.boards.lock().unwrap();
        assert!(boards_map["A"].connected && !boards_map["B"].connected);
    }

    #[test]
    fn a_registry_of_busy_boards_is_not_empty() {
        let boards = Boards::new().unwrap();
        assert!(boards.is_empty());
        let device = Arc::new(Mutex::new(Device::new(Default::default()).unwrap()));
        boards.boards.lock().unwrap().insert("A".into(), Board { device: device.clone(), container: None, connected: true });
        let _held = device.lock().unwrap();
        assert!(!boards.is_empty() && boards.statuses(false).0.is_empty());
    }

    #[test]
    fn a_new_unsupported_keyboard_counts_as_a_change() {
        let boards = Boards::new().unwrap();
        assert!(!boards.note_unsupported(None));
        assert!(boards.note_unsupported(Some(0x1234)));
        assert!(!boards.note_unsupported(Some(0x1234)));
        assert!(boards.note_unsupported(None));
    }

    #[test]
    fn attach_reuses_the_device() {
        let boards = Boards::new().unwrap();
        let shared: Shared = Default::default();
        let device = Arc::new(Mutex::new(Device::new(shared).unwrap()));
        boards.boards.lock().unwrap().insert("A".into(), Board { device: device.clone(), container: Some("C1".into()), connected: false });
        let again = boards.board_for("A", Some("C2".into()), || unreachable!("a known id gets its device back"));
        assert!(Arc::ptr_eq(&again, &device));
        assert_eq!(boards.boards.lock().unwrap()["A"].container.as_deref(), Some("C2"));
    }
}
