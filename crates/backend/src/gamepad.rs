use hidshim::{Buttons, Gamepad, State};
use razer_core::binding::{Action, UNIQUE};
use razer_core::layout::{self, Board};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::io;
use std::sync::mpsc::{Receiver, TryRecvError};

mod process;
pub mod drivers;

pub const CONTROLS: [&str; 24] = [
    "left_up", "left_down", "left_left", "left_right", "left_click",
    "right_up", "right_down", "right_left", "right_right", "right_click",
    "a", "b", "x", "y", "lb", "rb", "lt", "rt", "back", "start",
    "dpad_up", "dpad_down", "dpad_left", "dpad_right",
];
pub const ANALOG_CONTROLS: [&str; 10] = ["left_up", "left_down", "left_left", "left_right", "right_up", "right_down", "right_left", "right_right", "lt", "rt"];

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Config {
    pub enabled: bool,
    pub bindings: BTreeMap<String, u8>,
    pub max_depth: BTreeMap<String, u8>,
}

impl Default for Config {
    fn default() -> Self {
        Self { enabled: false, max_depth: BTreeMap::new(), bindings: [
            ("left_up".into(), 18), ("left_down".into(), 32),
            ("left_left".into(), 31), ("left_right".into(), 33),
            ("lt".into(), 17), ("rt".into(), 19),
        ].into() }
    }
}

impl Config {
    pub fn validate(&self, board: Board) -> Result<(), String> {
        self.validate_depth()?;
        let keys = layout::keys(board);
        for (control, key) in &self.bindings {
            if !CONTROLS.contains(&control.as_str()) {
                return Err(format!("Unsupported gamepad control: {control}"));
            }
            if !keys.iter().any(|k| k.key == *key && k.editable) {
                return Err(format!("Unsupported gamepad key: {key}"));
            }
        }
        Ok(())
    }

    pub fn validate_depth(&self) -> Result<(), String> {
        if self.max_depth.iter().any(|(id, depth)| !ANALOG_CONTROLS.contains(&id.as_str()) || !(1..=100).contains(depth)) {
            return Err("Gamepad maximum depth must be 1–100% for a stick direction or trigger".into());
        }
        Ok(())
    }

    pub fn configure(&self, cfg: &mut razer_core::rapid::Config) {
        if !self.enabled { return; }
        for &key in self.bindings.values() {
            cfg.bind[key as usize] = Some(Action::Disabled);
            // The app's own switches stay: a gamepad on G must not lose Fn+G.
            if !matches!(cfg.fn_bind[key as usize], Some(Action::System { action }) if UNIQUE.contains(&action)) {
                cfg.fn_bind[key as usize] = Some(Action::Disabled);
            }
        }
        cfg.snap.retain(|g| !g.keys.iter().any(|k| self.bindings.values().any(|v| v == k)));
    }

    fn output(&self, depth: &[u8; 256]) -> State {
        let value = |id: &str| {
            let raw = self.bindings.get(id).map_or(0, |&k| u32::from(depth[k as usize]));
            let percent = if ANALOG_CONTROLS.contains(&id) { self.max_depth.get(id).copied().unwrap_or(100).clamp(1, 100) } else { 100 };
            let maximum = (255 * u32::from(percent) + 50) / 100;
            ((raw * 255 + maximum / 2) / maximum).min(255) as u8
        };
        let axis = |positive, negative| {
            let d = i32::from(value(positive)) - i32::from(value(negative));
            (d * if d < 0 { 32768 } else { 32767 } / 255) as i16
        };
        let mut buttons = Buttons::default();
        for (id, button) in [
            ("a", Buttons::A), ("b", Buttons::B), ("x", Buttons::X), ("y", Buttons::Y),
            ("lb", Buttons::LEFT_SHOULDER), ("rb", Buttons::RIGHT_SHOULDER),
            ("left_click", Buttons::LEFT_THUMB), ("right_click", Buttons::RIGHT_THUMB),
            ("back", Buttons::BACK), ("start", Buttons::START),
            ("dpad_up", Buttons::DPAD_UP), ("dpad_down", Buttons::DPAD_DOWN),
            ("dpad_left", Buttons::DPAD_LEFT), ("dpad_right", Buttons::DPAD_RIGHT),
        ] {
            if value(id) >= 128 { buttons |= button; }
        }
        State {
            buttons,
            left_x: axis("left_right", "left_left"), left_y: axis("left_up", "left_down"),
            right_x: axis("right_right", "right_left"), right_y: axis("right_up", "right_down"),
            left_trigger: value("lt"), right_trigger: value("rt"),
        }
    }
}

#[derive(Clone, Debug, Serialize)]
pub struct Input {
    pub depth: Vec<u8>,
    pub running: bool,
    pub error: Option<String>,
}

impl Default for Input {
    fn default() -> Self { Self { depth: vec![0; 256], running: false, error: None } }
}

struct Session {
    pad: Gamepad,
    previous: State,
}

impl Drop for Session {
    fn drop(&mut self) { let _ = self.pad.submit(&State::default()); }
}

pub struct Runtime {
    pub config: Config,
    pub profile: u32,
    depth: [u8; 256],
    session: Option<Session>,
    pending: Option<Receiver<Result<Gamepad, String>>>,
    error: Option<String>,
    open: fn() -> io::Result<Gamepad>,
}

impl Default for Runtime {
    fn default() -> Self {
        Self::new()
    }
}

impl Runtime {
    pub fn new() -> Self {
        Self { config: Config::default(), profile: 0, depth: [0; 256], session: None, pending: None, error: None, open: Gamepad::open }
    }

    /// Opens the pad when it is enabled. `request` means the user just turned it on, which may
    /// ask for elevation to bring the pad back, as after a reboot.
    pub fn configure(&mut self, profile: u32, config: Config, request: bool) {
        self.check();
        if self.config != config || self.profile != profile {
            self.neutral();
        }
        self.profile = profile;
        self.config = config;
        if !self.config.enabled {
            self.stop();
            self.error = None;
            return;
        }
        if self.session.is_some() || self.pending.is_some() {
            return;
        }
        match (self.open)() {
            Ok(pad) => {
                self.session = Some(Session { pad, previous: State::default() });
                self.error = None;
            }
            Err(e) if e.kind() == io::ErrorKind::NotFound && request => {
                self.pending = Some(drivers::restore());
                self.error = None;
            }
            Err(e) if e.kind() == io::ErrorKind::NotFound => {
                self.error = Some(format!("{e}. Retry to start the gamepad."));
            }
            Err(e) => self.error = Some(e.to_string()),
        }
    }

    fn check(&mut self) {
        let Some(pending) = &self.pending else { return };
        match pending.try_recv() {
            Ok(result) => {
                self.pending = None;
                match result {
                    Ok(pad) => {
                        self.session = Some(Session { pad, previous: State::default() });
                        self.error = None;
                    }
                    Err(e) => self.error = Some(e),
                }
            }
            Err(TryRecvError::Disconnected) => {
                self.pending = None;
                self.error = Some("Gamepad startup interrupted. Retry to start it.".into());
            }
            Err(TryRecvError::Empty) => {}
        }
    }

    pub fn feed(&mut self, depth: &[u8; 256]) {
        self.depth = *depth;
        self.check();
        let state = self.config.output(depth);
        if let Some(s) = &mut self.session {
            if state == s.previous { return; }
            match s.pad.submit(&state) {
                Ok(()) => s.previous = state,
                Err(e) => {
                    self.error = Some(e.to_string());
                    self.session = None;
                }
            }
        }
    }

    pub fn neutral(&mut self) {
        self.depth = [0; 256];
        if let Some(s) = &mut self.session {
            match s.pad.submit(&State::default()) {
                Ok(()) => s.previous = State::default(),
                Err(e) => {
                    self.error = Some(e.to_string());
                    self.session = None;
                }
            }
        }
    }

    pub fn stop(&mut self) {
        self.suspend();
        self.pending = None;
    }

    pub fn suspend(&mut self) {
        self.neutral();
        self.session = None;
    }

    pub fn input(&mut self) -> Input {
        self.check();
        Input { depth: self.depth.to_vec(), running: self.session.is_some(), error: self.error.clone() }
    }
}

pub fn driver_helper(action: &str, args: &[String]) -> Result<(), String> {
    process::driver_helper(action, args)
}

#[cfg(test)]
mod editor_tests {
    use super::*;

    fn missing() -> io::Result<Gamepad> {
        Err(io::Error::new(io::ErrorKind::NotFound, "hidshim is not installed"))
    }

    fn offline() -> Runtime {
        Runtime { open: missing, ..Runtime::new() }
    }

    #[test]
    fn pending_start_survives_a_temporary_input_block_and_reports_failure() {
        let mut runtime = offline();
        let (tx, rx) = std::sync::mpsc::channel();
        runtime.pending = Some(rx);
        let config = Config { enabled: true, ..Default::default() };
        runtime.configure(1, config.clone(), false);
        assert!(runtime.input().error.is_none());
        runtime.suspend();
        assert!(runtime.pending.is_some());
        runtime.configure(1, config, false);
        tx.send(Err("UAC cancelled".into())).unwrap();
        assert_eq!(runtime.input().error.as_deref(), Some("UAC cancelled"));
        let (_tx, rx) = std::sync::mpsc::channel();
        runtime.pending = Some(rx);
        runtime.stop();
        assert!(runtime.pending.is_none());
    }

    #[test]
    fn analog_maximum_uses_full_depth_by_default_and_scales_each_binding() {
        let mut config = Config::default();
        let mut depth = [0; 256];
        depth[18] = 128;
        depth[17] = 128;
        depth[19] = 128;
        let half = config.output(&depth);
        assert_eq!((half.left_y, half.left_trigger, half.right_trigger), (16447, 128, 128));
        config.max_depth = [("left_up".into(), 50), ("lt".into(), 50)].into();
        let full = config.output(&depth);
        assert_eq!((full.left_y, full.left_trigger, full.right_trigger), (32767, 255, 128));
        depth[17] = 255;
        assert_eq!(config.output(&depth).left_trigger, 255);
        assert!(config.validate_depth().is_ok());
        for (id, maximum) in [("lt", 0), ("lt", 101), ("a", 50), ("unknown", 50)] {
            config.max_depth = [(id.into(), maximum)].into();
            assert!(config.validate_depth().is_err());
        }
        let old: Config = serde_json::from_str(r#"{"enabled":true,"bindings":{"lt":17}}"#).unwrap();
        assert!(old.max_depth.is_empty());
        assert_eq!(old.output(&depth).left_trigger, 255);
    }

    #[test]
    fn default_layout_uses_wasd_and_qe_without_overwriting_custom_bindings() {
        let config = Config::default();
        assert!(!config.enabled);
        assert_eq!(config.bindings, [
            ("left_up".into(), 18), ("left_down".into(), 32),
            ("left_left".into(), 31), ("left_right".into(), 33),
            ("lt".into(), 17), ("rt".into(), 19),
        ].into());
        assert_eq!(serde_json::from_str::<Config>("{}").unwrap(), config);
        let custom: Config = serde_json::from_str(r#"{"bindings":{"left_up":37}}"#).unwrap();
        assert_eq!(custom.bindings, [("left_up".into(), 37)].into());
        let cleared: Config = serde_json::from_str(r#"{"bindings":{}}"#).unwrap();
        assert!(cleared.bindings.is_empty());
    }

    #[test]
    fn disabled_capture_and_profile_changes_reset_input_without_launching() {
        let mut runtime = offline();
        let mut depth = [0; 256];
        depth[18] = 201;
        runtime.feed(&depth);
        assert_eq!(runtime.input().depth[18], 201);
        assert!(!runtime.input().running);
        runtime.configure(2, Config::default(), false);
        assert_eq!(runtime.input().depth, vec![0; 256]);
        runtime.configure(2, Config { enabled: true, bindings: Default::default(), ..Default::default() }, false);
        assert!(runtime.input().error.is_some());
        assert!(runtime.pending.is_none(), "only a request may ask for elevation");
        runtime.configure(2, Config::default(), false);
        assert!(runtime.input().error.is_none());
    }

    #[test]
    fn every_digital_control_uses_half_travel() {
        for id in CONTROLS.into_iter().filter(|id| !id.ends_with("_up") && !id.ends_with("_down") && !id.ends_with("_left") && !id.ends_with("_right") && !["lt", "rt"].contains(id)).chain(["dpad_up", "dpad_down", "dpad_left", "dpad_right"]) {
            let cfg = Config { enabled: true, bindings: [(id.into(), 18)].into(), ..Default::default() };
            let mut depth = [0; 256];
            depth[18] = 127;
            assert_eq!(cfg.output(&depth).buttons, Buttons::default(), "{id}");
            depth[18] = 128;
            assert_ne!(cfg.output(&depth).buttons, Buttons::default(), "{id}");
        }
    }

    #[test]
    fn mapped_keys_are_suppressed_in_both_layers_and_snap_groups() {
        use razer_core::rapid::{Engine, Output, SnapGroup, SnapRule};
        use std::time::{Duration, Instant};
        let mut cfg = razer_core::rapid::Config {
            act: [128; 256], fn_act: [128; 256], rapid: [None; 256],
            bind: [None; 256], fn_bind: [None; 256],
            repeat_delay: Duration::from_millis(500), repeat_interval: Duration::from_millis(30),
            macros: Default::default(), margin: 1, snap_on: true,
            snap: vec![SnapGroup { keys: vec![18, 34], rule: SnapRule::Last }],
        };
        Config { enabled: true, bindings: [("left_up".into(), 18)].into(), ..Default::default() }.configure(&mut cfg);
        assert!(cfg.snap.is_empty());
        assert_eq!(cfg.fn_bind[18], Some(Action::Disabled));
        let mut depth = [0; 256];
        depth[18] = 255;
        depth[34] = 255;
        assert_eq!(Engine::new(cfg).feed_depth(&depth, Instant::now()), [Output::Key { key: 34, down: true }]);
    }

    #[test]
    fn mapped_keys_keep_the_app_switches_on_fn() {
        use razer_core::binding::System;
        let mut cfg = razer_core::rapid::Config {
            act: [128; 256], fn_act: [128; 256], rapid: [None; 256],
            bind: [None; 256], fn_bind: [None; 256],
            repeat_delay: std::time::Duration::from_millis(500), repeat_interval: std::time::Duration::from_millis(30),
            macros: Default::default(), margin: 1, snap_on: true, snap: Vec::new(),
        };
        let toggle = Some(Action::System { action: System::GamepadToggle });
        let game = Some(Action::System { action: System::GameMode });
        (cfg.fn_bind[18], cfg.fn_bind[19]) = (toggle, game);
        Config { enabled: true, bindings: [("a".into(), 18), ("b".into(), 19)].into(), ..Default::default() }.configure(&mut cfg);
        assert_eq!((cfg.bind[18], cfg.fn_bind[18], cfg.fn_bind[19]), (Some(Action::Disabled), toggle, Some(Action::Disabled)));
    }

    #[test]
    fn defaults_and_control_validation() {
        assert_eq!(serde_json::from_str::<Config>("{}").unwrap(), Config::default());
        let mut cfg = Config::default();
        cfg.bindings.insert("bogus".into(), 18);
        assert!(cfg.validate(razer_core::layout::Board::Full).is_err());
        cfg.bindings = [("a".into(), 255)].into();
        assert!(cfg.validate(razer_core::layout::Board::Full).is_err());
    }

    #[test]
    fn configured_axes_buttons_and_triggers() {
        let cfg = Config { enabled: true, bindings: [("right_up".into(), 18), ("right_left".into(), 31), ("right_right".into(), 33), ("lt".into(), 17), ("a".into(), 37)].into(), ..Default::default() };
        let mut depth = [0; 256];
        depth[18] = 255;
        depth[31] = 255;
        depth[17] = 123;
        depth[37] = 127;
        let s = cfg.output(&depth);
        assert_eq!((s.right_x, s.right_y, s.left_trigger, s.buttons), (-32768, 32767, 123, Buttons::default()));
        depth[33] = 255;
        depth[37] = 128;
        let s = cfg.output(&depth);
        assert_eq!((s.right_x, s.buttons), (0, Buttons::A));
    }
}
