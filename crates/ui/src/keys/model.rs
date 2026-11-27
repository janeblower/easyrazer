//! What the Keys tab edits: press points, Rapid Trigger, Snap Tap and both binding layers,
//! each as the keyboard last reported it plus the edits not applied yet.

use std::collections::{BTreeMap, BTreeSet};

use easyrazer_backend::api::{Actuation, ApplyResult, BindResult, WriteResult};
use easyrazer_backend::device::{Hypershift, HypershiftEdits};
use easyrazer_backend::settings::{Rapid, SnapTap};
use razer_core::binding::{Action, System};

pub type KeyMap<T> = BTreeMap<u8, T>;

pub const DEFAULT_RAPID: Rapid = Rapid { enabled: false, press: 0.4, release: 0.4, continuous: false };

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RapidMode {
    Off,
    Rt,
    /// Continuous Rapid Trigger.
    Crt,
}

pub fn factory(key: u8) -> Action {
    Action::Key { key, mods: 0 }
}

/// Synapse's factory Hypershift layer on V2, plus the app's Snap Tap switch on LShift.
const HS_FACTORY: [(u8, System); 8] = [
    (44, System::SnapTap),
    (120, System::MacroLed),
    (121, System::GameMode),
    (35, System::GamepadToggle),
    (122, System::BrightnessDown),
    (123, System::BrightnessUp),
    (126, System::Sleep),
    (129, System::NextProfile),
];

pub fn hs_factory(key: u8) -> Action {
    HS_FACTORY.iter().find(|(k, _)| *k == key).map_or(factory(key), |(_, action)| Action::System { action: *action })
}

/// The app reacts to these itself, so one key at most carries each.
pub const UNIQUE: [System; 3] = [System::NextProfile, System::SnapTap, System::GamepadToggle];

/// The value all of them share, if they do.
pub fn common<V: PartialEq + Clone>(values: impl IntoIterator<Item = V>) -> Option<V> {
    let mut it = values.into_iter();
    let first = it.next()?;
    it.all(|v| v == first).then_some(first)
}

/// A unique system action asked for a key while another key holds it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Move {
    pub from: u8,
    pub action: System,
}

#[derive(Default)]
pub struct KeyEdits {
    /// mm, as last read from the keyboard.
    pub baseline: KeyMap<f32>,
    pub edits: KeyMap<f32>,
    pub rapid_base: KeyMap<Rapid>,
    pub rapid_edits: KeyMap<Rapid>,
    pub split_on: bool,
    pub snap_base: SnapTap,
    pub snap_edit: Option<SnapTap>,
    /// The firmware's continuous Rapid Trigger, one for the whole profile.
    pub crt_base: bool,
    pub crt_edit: Option<bool>,
    /// Message from the last apply.
    pub errors: KeyMap<String>,
    /// Applied, lost on replug unless saved.
    pub unsaved: BTreeSet<u8>,
    pub bind_base: KeyMap<Option<Action>>,
    pub bind_edits: KeyMap<Action>,
    pub bind_errors: KeyMap<String>,
    pub bind_unsaved: BTreeSet<u8>,
    pub hs_base: KeyMap<f32>,
    pub hs_edits: KeyMap<f32>,
    pub hs_split_on: bool,
    pub hs_bind_base: KeyMap<Option<Action>>,
    pub hs_bind_edits: KeyMap<Action>,
    pub hs_unsaved: BTreeSet<u8>,
    pub selection: BTreeSet<u8>,
}

impl KeyEdits {
    pub fn value_of(&self, k: u8) -> Option<f32> {
        self.edits.get(&k).or(self.baseline.get(&k)).copied()
    }

    pub fn hs_of(&self, k: u8) -> Option<f32> {
        self.hs_edits.get(&k).or(self.hs_base.get(&k)).copied()
    }

    pub fn rapid_of(&self, k: u8) -> Rapid {
        self.rapid_edits.get(&k).or(self.rapid_base.get(&k)).copied().unwrap_or(DEFAULT_RAPID)
    }

    /// `fw_hw`: the firmware runs Rapid Trigger, where continuous is one switch for the profile.
    pub fn mode_of(r: &Rapid, fw_hw: bool) -> RapidMode {
        match (r.enabled, r.continuous && !fw_hw) {
            (false, _) => RapidMode::Off,
            (true, true) => RapidMode::Crt,
            (true, false) => RapidMode::Rt,
        }
    }

    pub fn binding_of(&self, k: u8) -> Option<Action> {
        self.bind_edits.get(&k).copied().or_else(|| self.bind_base.get(&k).copied().flatten())
    }

    pub fn hs_binding_of(&self, k: u8) -> Option<Action> {
        self.hs_bind_edits.get(&k).copied().or_else(|| self.hs_bind_base.get(&k).copied().flatten())
    }

    pub fn snap(&self) -> &SnapTap {
        self.snap_edit.as_ref().unwrap_or(&self.snap_base)
    }

    pub fn crt(&self) -> bool {
        self.crt_edit.unwrap_or(self.crt_base)
    }

    pub fn hs_split(&self) -> bool {
        self.hs_split_on || self.selection.iter().any(|&k| self.hs_of(k) != self.value_of(k))
    }

    pub fn split(&self) -> bool {
        self.split_on || self.selection.iter().any(|&k| {
            let r = self.rapid_of(k);
            r.release != r.press
        })
    }

    pub fn set_value(&mut self, mm: f32) {
        let together = !self.hs_split();
        for &k in &self.selection {
            if self.baseline.get(&k) == Some(&mm) {
                self.edits.remove(&k);
            } else {
                self.edits.insert(k, mm);
            }
        }
        if together {
            self.set_hs(|_| mm);
        }
    }

    pub fn set_hs(&mut self, mm: impl Fn(u8) -> f32) {
        for &k in &self.selection {
            let v = mm(k);
            if self.hs_base.get(&k) == Some(&v) {
                self.hs_edits.remove(&k);
            } else {
                self.hs_edits.insert(k, v);
            }
        }
    }

    pub fn on_hs_split(&mut self, on: bool) {
        self.hs_split_on = on;
        if !on {
            let values: KeyMap<f32> = self.selection.iter().filter_map(|&k| Some((k, self.value_of(k)?))).collect();
            self.set_hs(|k| values.get(&k).copied().unwrap_or_default());
        }
    }

    pub fn set_rapid(&mut self, change: impl Fn(Rapid) -> Rapid) {
        for &k in &self.selection.clone() {
            let r = change(self.rapid_of(k));
            if r == self.rapid_base.get(&k).copied().unwrap_or(DEFAULT_RAPID) {
                self.rapid_edits.remove(&k);
            } else {
                self.rapid_edits.insert(k, r);
            }
        }
    }

    pub fn on_split(&mut self, on: bool) {
        self.split_on = on;
        if !on {
            self.set_rapid(|r| Rapid { release: r.press, ..r });
        }
    }

    pub fn set_snap(&mut self, s: SnapTap) {
        self.snap_edit = (s != self.snap_base).then_some(s);
    }

    pub fn set_crt(&mut self, on: bool) {
        self.crt_edit = (on != self.crt_base).then_some(on);
    }

    pub fn set_binding(&mut self, k: u8, a: Action) {
        if Some(a) == self.bind_base.get(&k).copied().flatten() {
            self.bind_edits.remove(&k);
        } else {
            self.bind_edits.insert(k, a);
        }
    }

    fn put_hs(&mut self, k: u8, a: Action) {
        if Some(a) == self.hs_bind_base.get(&k).copied().flatten() {
            self.hs_bind_edits.remove(&k);
        } else {
            self.hs_bind_edits.insert(k, a);
        }
    }

    /// The key holding a unique system action, unapplied edits included.
    pub fn holder(&self, keys: impl IntoIterator<Item = u8>, s: System) -> Option<u8> {
        keys.into_iter().find(|&k| self.hs_binding_of(k) == Some(Action::System { action: s }))
    }

    /// Sets the Hypershift binding, or asks first when it would take a unique action from another key.
    pub fn set_hs_binding(&mut self, keys: impl IntoIterator<Item = u8>, k: u8, a: Action) -> Option<Move> {
        if let Action::System { action } = a
            && UNIQUE.contains(&action)
            && let Some(from) = self.holder(keys, action)
            && from != k
        {
            return Some(Move { from, action });
        }
        self.put_hs(k, a);
        None
    }

    /// The old combination goes back to its own key, not to its factory system action.
    pub fn confirm_move(&mut self, k: u8, m: Move) {
        self.put_hs(m.from, factory(m.from));
        self.put_hs(k, Action::System { action: m.action });
    }

    /// Edited keys and the profile-wide switches, counted like the window shows them.
    pub fn dirty(&self) -> usize {
        let keys: BTreeSet<u8> = self
            .edits
            .keys()
            .chain(self.rapid_edits.keys())
            .chain(self.bind_edits.keys())
            .chain(self.hs_edits.keys())
            .chain(self.hs_bind_edits.keys())
            .copied()
            .collect();
        keys.len() + usize::from(self.snap_edit.is_some()) + usize::from(self.crt_edit.is_some())
    }

    pub fn actuation_dirty(&self) -> bool {
        !self.edits.is_empty() || !self.rapid_edits.is_empty() || !self.hs_edits.is_empty() || self.snap_edit.is_some() || self.crt_edit.is_some()
    }

    pub fn bindings_dirty(&self) -> bool {
        !self.bind_edits.is_empty() || !self.hs_bind_edits.is_empty()
    }

    /// Something a flash write would keep; Rapid Trigger lives on the host and is not part of it.
    pub fn savable(&self) -> bool {
        !self.edits.is_empty()
            || !self.unsaved.is_empty()
            || !self.bind_edits.is_empty()
            || !self.bind_unsaved.is_empty()
            || !self.hs_edits.is_empty()
            || !self.hs_bind_edits.is_empty()
            || !self.hs_unsaved.is_empty()
    }

    pub fn revert(&mut self) {
        self.edits.clear();
        self.rapid_edits.clear();
        self.snap_edit = None;
        self.crt_edit = None;
        self.errors.clear();
        self.bind_edits.clear();
        self.hs_edits.clear();
        self.hs_bind_edits.clear();
        self.bind_errors.clear();
    }

    fn take_hypershift(&mut self, h: Hypershift) {
        self.hs_base = h.values.into_iter().collect();
        self.hs_bind_base = h.bindings.into_iter().collect();
        self.hs_unsaved = h.unsaved.into_iter().collect();
    }

    /// A fresh read: edits that now match the keyboard go.
    pub fn take_read(&mut self, a: Actuation) {
        self.baseline = a.values.into_iter().collect();
        self.rapid_base = a.rapid.into_iter().collect();
        let rapid_base = &self.rapid_base;
        self.rapid_edits.retain(|k, r| *r != rapid_base.get(k).copied().unwrap_or(DEFAULT_RAPID));
        self.snap_base = a.snap;
        if self.snap_edit.as_ref() == Some(&self.snap_base) {
            self.snap_edit = None;
        }
        self.crt_base = a.continuous;
        if self.crt_edit == Some(a.continuous) {
            self.crt_edit = None;
        }
        self.unsaved = a.unsaved.into_iter().collect();
        let base = &self.baseline;
        self.edits.retain(|k, mm| base.get(k) != Some(mm));
        self.errors.clear();
        self.bind_base = a.bindings.into_iter().collect();
        let bind_base = &self.bind_base;
        self.bind_edits.retain(|k, x| bind_base.get(k).copied().flatten() != Some(*x));
        self.bind_unsaved = a.unsaved_bindings.into_iter().collect();
        self.take_hypershift(a.hypershift);
        let hs_base = &self.hs_base;
        self.hs_edits.retain(|k, mm| hs_base.get(k) != Some(mm));
        let hs_bind_base = &self.hs_bind_base;
        self.hs_bind_edits.retain(|k, x| hs_bind_base.get(k).copied().flatten() != Some(*x));
        self.bind_errors.clear();
    }

    pub fn changes(&self) -> Vec<(u8, f32)> {
        self.edits.iter().map(|(k, v)| (*k, *v)).collect()
    }

    pub fn bindings(&self) -> Vec<(u8, Action)> {
        self.bind_edits.iter().map(|(k, a)| (*k, *a)).collect()
    }

    pub fn hypershift(&self) -> HypershiftEdits {
        HypershiftEdits {
            values: self.hs_edits.iter().map(|(k, v)| (*k, *v)).collect(),
            bindings: self.hs_bind_edits.iter().map(|(k, a)| (*k, *a)).collect(),
        }
    }

    /// The result of Apply (`rapid` holds what it sent of Rapid Trigger and Snap Tap) or Save.
    /// Confirmed and unconfirmed keys take the device value and leave the edits; only failed ones keep them.
    pub fn take_written(&mut self, w: WriteResult, rapid: Option<Sent>, unconfirmed: impl Fn(Option<f32>) -> String) -> usize {
        self.take_hypershift(w.hypershift);
        self.hs_edits.clear();
        self.hs_bind_edits.clear();
        if let Some(sent) = rapid {
            self.rapid_base.extend(sent.rapid);
            self.rapid_edits.clear();
            if let Some(s) = sent.snap {
                self.snap_base = s;
            }
            self.snap_edit = None;
            if let Some(c) = sent.continuous {
                self.crt_base = c;
            }
            self.crt_edit = None;
        }
        self.unsaved = w.unsaved.into_iter().collect();
        self.errors.clear();
        for r in w.results {
            match r {
                ApplyResult::Error { key, message } => {
                    self.errors.insert(key, message);
                }
                ApplyResult::Ok { key, mm } | ApplyResult::Unconfirmed { key, mm } => {
                    let unsure = matches!(r, ApplyResult::Unconfirmed { .. });
                    self.baseline.insert(key, mm);
                    self.edits.remove(&key);
                    if unsure {
                        self.errors.insert(key, unconfirmed(Some(mm)));
                    }
                }
            }
        }
        self.bind_unsaved = w.unsaved_bindings.into_iter().collect();
        self.bind_errors.clear();
        for r in w.bindings {
            match r {
                BindResult::Error { key, message } => {
                    self.bind_errors.insert(key, message);
                }
                BindResult::Ok { key, action } | BindResult::Unconfirmed { key, action } => {
                    let unsure = matches!(r, BindResult::Unconfirmed { .. });
                    self.bind_base.insert(key, action);
                    self.bind_edits.remove(&key);
                    if unsure {
                        self.bind_errors.insert(key, unconfirmed(None));
                    }
                }
            }
        }
        self.errors.len() + self.bind_errors.len()
    }
}

/// What Apply sent of the host-side settings.
pub struct Sent {
    pub rapid: KeyMap<Rapid>,
    pub snap: Option<SnapTap>,
    pub continuous: Option<bool>,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn with_selection(keys: &[u8]) -> KeyEdits {
        KeyEdits { selection: keys.iter().copied().collect(), ..Default::default() }
    }

    #[test]
    fn an_edit_back_to_the_keyboard_value_goes() {
        let mut e = with_selection(&[1, 2]);
        e.baseline = [(1, 1.5), (2, 2.0)].into();
        e.hs_base = e.baseline.clone();
        e.set_value(2.0);
        assert_eq!(e.edits, [(1, 2.0)].into());
        assert_eq!(e.hs_edits, [(1, 2.0)].into(), "the Hypershift layer follows while not split");
        e.set_value(1.5);
        assert_eq!(e.edits, [(2, 1.5)].into());
    }

    #[test]
    fn a_split_layer_keeps_its_own_value() {
        let mut e = with_selection(&[1]);
        e.baseline = [(1, 1.5)].into();
        e.hs_base = [(1, 3.0)].into();
        e.set_value(2.0);
        assert!(e.hs_edits.is_empty());
        e.on_hs_split(false);
        assert_eq!(e.hs_edits, [(1, 2.0)].into());
    }

    #[test]
    fn rapid_edits_back_to_default_go() {
        let mut e = with_selection(&[7]);
        e.set_rapid(|r| Rapid { enabled: true, ..r });
        assert_eq!(e.dirty(), 1);
        e.set_rapid(|r| Rapid { enabled: false, ..r });
        assert_eq!(e.dirty(), 0);
    }

    #[test]
    fn a_unique_action_moves_after_asking() {
        let mut e = KeyEdits::default();
        e.hs_bind_base = [(129, Some(Action::System { action: System::NextProfile })), (130, Some(factory(130)))].into();
        let ask = e.set_hs_binding([129, 130], 130, Action::System { action: System::NextProfile });
        let m = ask.expect("asks first");
        assert_eq!(m.from, 129);
        e.confirm_move(130, m);
        assert_eq!(e.hs_binding_of(129), Some(factory(129)));
        assert_eq!(e.hs_binding_of(130), Some(Action::System { action: System::NextProfile }));
    }

    #[test]
    fn a_failed_key_keeps_its_edit() {
        let mut e = KeyEdits::default();
        e.edits = [(1, 2.0), (2, 3.0)].into();
        let w = WriteResult {
            results: vec![ApplyResult::Ok { key: 1, mm: 2.0 }, ApplyResult::Error { key: 2, message: "no".into() }],
            unsaved: vec![1],
            bindings: vec![],
            unsaved_bindings: vec![],
            hypershift: Hypershift::default(),
        };
        let bad = e.take_written(w, None, |_| String::new());
        assert_eq!(bad, 1);
        assert_eq!(e.edits, [(2, 3.0)].into());
        assert_eq!(e.baseline.get(&1), Some(&2.0));
        assert!(e.unsaved.contains(&1));
    }

    #[test]
    fn common_needs_every_value_equal() {
        assert_eq!(common([1, 1, 1]), Some(1));
        assert_eq!(common([1, 2]), None);
        assert_eq!(common(Vec::<u8>::new()), None);
    }
}
