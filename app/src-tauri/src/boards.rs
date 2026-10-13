//! Every keyboard the app has met this session, by id; the commands reach them through here.

use std::collections::BTreeMap;
use std::sync::{Arc, Mutex, MutexGuard, TryLockError};

use hidapi::HidApi;
use razer_core::control;
use razer_core::hid::{self, Unit};
use serde::Serialize;

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
    pub name: String,
    pub model: String,
    pub connected: bool,
    pub driver_mode: bool,
}

#[derive(Serialize)]
pub struct DevicesView {
    pub devices: Vec<DeviceEntry>,
    /// PID of a Razer keyboard without a description.
    pub unsupported: Option<u16>,
    pub last: Option<String>,
}

fn model_name(pid: u16) -> String {
    razer_core::devices::by_pid(pid).map_or_else(|| format!("{pid:04X}"), |d| d.name.clone())
}

/// The name each keyboard is shown by: its own, else the model's, told apart by the serial's tail
/// from another of the same model.
pub fn display_names(entries: &[(String, u16, Option<String>)]) -> Vec<String> {
    entries
        .iter()
        .map(|(id, pid, name)| {
            if let Some(n) = name {
                return n.clone();
            }
            let model = model_name(*pid);
            let twins = entries.iter().filter(|e| e.1 == *pid).count() > 1;
            if !twins || id.ends_with(":noserial") {
                return model;
            }
            let tail: Vec<char> = id.chars().rev().take(4).collect();
            format!("{model} ·{}", tail.into_iter().rev().collect::<String>())
        })
        .collect()
}

pub struct Boards {
    boards: Mutex<BTreeMap<String, Board>>,
    /// Enumerates for discovery; each device has its own for its interfaces.
    api: Mutex<HidApi>,
    /// PID of the unsupported Razer keyboard seen by the last discovery.
    unsupported: Mutex<Option<u16>>,
    /// Container -> id of a unit that read the id of a connected board, so later polls skip it unopened.
    twins: Mutex<BTreeMap<String, String>>,
}

fn guard<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|e| e.into_inner())
}

/// Like `guard`, but `None` while someone holds the mutex.
pub fn try_guard<T>(m: &Mutex<T>) -> Option<MutexGuard<'_, T>> {
    match m.try_lock() {
        Ok(g) => Some(g),
        Err(TryLockError::Poisoned(e)) => Some(e.into_inner()),
        Err(TryLockError::WouldBlock) => None,
    }
}

/// Units no board holds and not known to be a twin of one. A board without a container holds any
/// unit, the way it was before containers were read: two such keyboards of one model cannot be told apart.
pub fn units_to_attach(units: Vec<Unit>, held: &[Option<String>], twins: &BTreeMap<String, String>) -> Vec<Unit> {
    let twin = |u: &Unit| u.container.as_ref().is_some_and(|c| twins.contains_key(c));
    units.into_iter().filter(|u| !twin(u) && !held.iter().any(|c| c.is_none() || *c == u.container)).collect()
}

impl Boards {
    pub fn new() -> Result<Self, String> {
        Ok(Self { boards: Mutex::new(BTreeMap::new()), api: Mutex::new(HidApi::new().map_err(|e| e.to_string())?), unsupported: Mutex::new(None), twins: Mutex::default() })
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

    /// The board of `id`, creating an offline one from the settings when the id is known there.
    pub fn get_or_offline(&self, id: &str, shared: &Shared) -> Option<Arc<Mutex<Device>>> {
        if let Some(d) = self.get(id) {
            return Some(d);
        }
        let known = settings::lock(shared).devices.contains_key(id);
        if !known {
            return None;
        }
        let mut device = Device::new(shared.clone()).ok()?;
        device.offline(id);
        let device = Arc::new(Mutex::new(device));
        Some(guard(&self.boards).entry(id.to_string()).or_insert_with(|| Board { device, container: None, connected: false }).device.clone())
    }

    fn board_for(&self, id: &str, container: Option<String>, device: Arc<Mutex<Device>>) -> Arc<Mutex<Device>> {
        let mut boards = guard(&self.boards);
        let b = boards.entry(id.to_string()).or_insert_with(|| Board { device, container: None, connected: false });
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

    /// PID of the unsupported Razer keyboard the last discovery saw.
    pub fn unsupported(&self) -> Option<u16> {
        *guard(&self.unsupported)
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
            let Some(mut dev) = try_guard(&d) else { continue };
            let s = dev.status(synapse);
            drop(dev);
            changed |= self.set_connected(&id, s.device);
            out.push(s);
        }
        (out, changed)
    }

    fn is_connected(&self, id: &str) -> bool {
        guard(&self.boards).get(id).is_some_and(|b| b.connected)
    }

    /// Attaches every unit no board holds; `true` when a keyboard appeared or the unsupported one changed.
    pub fn discover(&self, shared: &Shared, hooks: &Hooks, synapse: bool) -> bool {
        let held: Vec<Option<String>> = guard(&self.boards).values().filter(|b| b.connected).map(|b| b.container.clone()).collect();
        let mut api = guard(&self.api);
        let _ = api.refresh_devices();
        let mut changed = self.note_unsupported(hid::unsupported_keyboard(&api));
        let units = hid::units(&api);
        let mut twins = guard(&self.twins);
        twins.retain(|c, id| self.is_connected(id) && units.iter().any(|u| u.container.as_ref() == Some(c)));
        for unit in units_to_attach(units, &held, &twins) {
            // A failed serial read would file the keyboard's data under another key; the next poll retries.
            let Ok(t) = hid::open_control(&api, &unit) else { continue };
            let Ok(serial) = control::serial(&t) else { continue };
            let id = settings::device_id(unit.spec.pid, &serial);
            // Keyboards without a readable serial share an id; taking the live board's device would detach it.
            if self.is_connected(&id) {
                twins.extend(unit.container.clone().map(|c| (c, id)));
                continue;
            }
            let device = match self.get(&id) {
                Some(d) => d,
                None => match Device::new(shared.clone()) {
                    Ok(d) => Arc::new(Mutex::new(d)),
                    Err(_) => continue,
                },
            };
            let device = self.board_for(&id, unit.container.clone(), device);
            // A command may hold a disconnected device for long; waiting here would stall every board.
            let Some(mut dev) = try_guard(&device) else {
                self.set_connected(&id, false);
                continue;
            };
            dev.attach(unit, t, &id, (hooks.sink)(&id), (hooks.progress)(&id), synapse);
            changed = true;
        }
        changed
    }

    /// Drops a keyboard that is not plugged in, with its data; nothing re-creates it until it is plugged in again.
    pub fn forget(&self, id: &str, shared: &Shared) -> Result<(), String> {
        let lang = settings::lock(shared).language.clone().unwrap_or_else(|| "en".into());
        // Held until the settings are cleared, so no ghost board appears in between.
        let mut boards = guard(&self.boards);
        if let Some(b) = boards.get(id) {
            if b.connected {
                return Err(crate::i18n::t(&lang, "backend.forgetConnected"));
            }
            let Some(mut dev) = try_guard(&b.device) else { return Err(crate::i18n::t(&lang, "backend.busy")) };
            dev.release();
        }
        boards.remove(id);
        let mut s = settings::lock(shared);
        s.devices.remove(id);
        if s.last_device.as_deref() == Some(id) {
            s.last_device = None;
        }
        settings::save(&s)
    }

    /// Keyboards met this session and those only in the settings; never locks a device.
    pub fn list(&self, s: &Settings) -> Vec<DeviceEntry> {
        let boards = guard(&self.boards);
        let entries: Vec<_> = s.devices.iter().map(|(id, d)| (id.clone(), d.pid, d.name.clone())).collect();
        let names = display_names(&entries);
        s.devices
            .iter()
            .zip(names)
            .map(|((id, d), name)| DeviceEntry {
                id: id.clone(),
                name,
                model: model_name(d.pid),
                connected: boards.get(id).is_some_and(|b| b.connected),
                driver_mode: d.driver_mode,
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::settings::DeviceData;
    use razer_core::devices;

    fn unit(container: Option<&str>) -> Unit {
        Unit { spec: devices::by_pid(0x0266).unwrap(), container: container.map(Into::into) }
    }

    #[test]
    fn units_to_attach_skips_held_containers() {
        let units = vec![unit(Some("A")), unit(Some("B")), unit(None)];
        let got: Vec<_> = units_to_attach(units, &[Some("A".into())], &BTreeMap::new()).into_iter().map(|u| u.container).collect();
        assert_eq!(got, [Some("B".into()), None]);
    }

    #[test]
    fn a_board_without_container_holds_any_unit() {
        assert!(units_to_attach(vec![unit(None), unit(Some("A"))], &[None], &BTreeMap::new()).is_empty());
    }

    #[test]
    fn a_known_twin_is_skipped_before_it_is_opened() {
        let twins = BTreeMap::from([("A".to_string(), "66:noserial".to_string())]);
        let got: Vec<_> = units_to_attach(vec![unit(Some("A")), unit(Some("B")), unit(None)], &[], &twins).into_iter().map(|u| u.container).collect();
        assert_eq!(got, [Some("B".into()), None]);
    }

    #[test]
    fn a_connected_board_is_not_taken_over_by_a_unit_with_its_id() {
        let boards = Boards::new().unwrap();
        let device = Arc::new(Mutex::new(Device::new(Shared::default()).unwrap()));
        let mut map = boards.boards.lock().unwrap();
        map.insert("66:noserial".into(), Board { device, container: Some("A".into()), connected: true });
        drop(map);
        assert!(boards.is_connected("66:noserial"));
        boards.set_connected("66:noserial", false);
        assert!(!boards.is_connected("66:noserial"), "a replugged board may attach again");
        assert!(!boards.is_connected("other"));
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
        let again = boards.board_for("A", Some("C2".into()), Arc::new(Mutex::new(Device::new(Default::default()).unwrap())));
        assert!(Arc::ptr_eq(&again, &device));
        assert_eq!(boards.boards.lock().unwrap()["A"].container.as_deref(), Some("C2"));
    }

    #[test]
    fn a_known_keyboard_gets_an_offline_board() {
        let boards = Boards::new().unwrap();
        let shared: Shared = Default::default();
        settings::lock(&shared).devices.insert("K".into(), DeviceData { pid: 0x0266, ..Default::default() });
        let d = boards.get_or_offline("K", &shared).expect("known id");
        assert!(!boards.is_connected("K"));
        assert_eq!(guard(&d).name(), "Razer Huntsman V2 Analog");
        assert!(boards.get_or_offline("unknown", &shared).is_none());
    }

    #[test]
    fn an_offline_board_attaches_in_place() {
        let boards = Boards::new().unwrap();
        let shared: Shared = Default::default();
        settings::lock(&shared).devices.insert("K".into(), DeviceData { pid: 0x0266, ..Default::default() });
        let offline = boards.get_or_offline("K", &shared).unwrap();
        let attached = boards.board_for("K", Some("C".into()), offline.clone());
        assert!(Arc::ptr_eq(&offline, &attached), "discovery reuses the offline device");
    }

    #[test]
    fn two_of_one_model_get_the_serial_tail() {
        let names = display_names(&[
            ("PM2322F51200078".into(), 0x0266, None),
            ("PM2322F51200099".into(), 0x0266, None),
            ("X1".into(), 0x02A6, None),
            ("PM2322F51200111".into(), 0x0266, Some("Desk".into())),
        ]);
        let v2 = &razer_core::devices::by_pid(0x0266).unwrap().name;
        assert_eq!(names[0], format!("{v2} ·0078"));
        assert_eq!(names[1], format!("{v2} ·0099"));
        assert_eq!(names[2], razer_core::devices::by_pid(0x02A6).unwrap().name);
        assert_eq!(names[3], "Desk");
    }

    #[test]
    fn a_noserial_id_gets_no_tail() {
        let names = display_names(&[("0266:noserial".into(), 0x0266, None), ("S1".into(), 0x0266, None)]);
        assert_eq!(names[0], razer_core::devices::by_pid(0x0266).unwrap().name);
        assert_eq!(names[1], format!("{} ·S1", razer_core::devices::by_pid(0x0266).unwrap().name));
    }

    #[test]
    fn a_forgotten_device_cannot_write_its_data_back() {
        let boards = Boards::new().unwrap();
        let shared: Shared = Default::default();
        settings::lock(&shared).devices.insert("K".into(), DeviceData { pid: 0x0266, ..Default::default() });
        let d = boards.get_or_offline("K", &shared).unwrap();
        boards.forget("K", &shared).unwrap();
        assert!(guard(&d).rename_device("x").is_err());
        assert!(!settings::lock(&shared).devices.contains_key("K"));
    }

    #[test]
    fn a_busy_board_is_not_forgotten() {
        let boards = Boards::new().unwrap();
        let shared: Shared = Default::default();
        settings::lock(&shared).devices.insert("K".into(), DeviceData { pid: 0x0266, ..Default::default() });
        let d = boards.get_or_offline("K", &shared).unwrap();
        let _held = guard(&d);
        assert!(boards.forget("K", &shared).is_err());
        assert!(boards.get("K").is_some() && settings::lock(&shared).devices.contains_key("K"));
    }

    #[test]
    fn forget_refuses_a_connected_board() {
        let boards = Boards::new().unwrap();
        let shared: Shared = Default::default();
        settings::lock(&shared).devices.insert("K".into(), DeviceData { pid: 0x0266, ..Default::default() });
        settings::lock(&shared).last_device = Some("K".into());
        let d = boards.get_or_offline("K", &shared).unwrap();
        boards.set_connected("K", true);
        assert!(boards.forget("K", &shared).is_err());
        boards.set_connected("K", false);
        drop(d);
        boards.forget("K", &shared).unwrap();
        assert!(!settings::lock(&shared).devices.contains_key("K"));
        assert_eq!(settings::lock(&shared).last_device, None);
        assert!(boards.get("K").is_none());
    }
}
