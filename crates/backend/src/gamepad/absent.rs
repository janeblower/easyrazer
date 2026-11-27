//! A build without the gamepad: the same calls, and none of them reaches hidshim.

use super::{Config, Input};

const MISSING: &str = "This build of EasyRazer has no gamepad";

pub mod drivers {
    pub use super::super::DriverStatus as Status;
    use super::MISSING;

    pub fn status() -> Result<Status, String> {
        Err(MISSING.into())
    }

    pub fn install(_replace: bool) -> Result<(), String> {
        Err(MISSING.into())
    }

    pub fn set_boot(_on: bool) -> Result<(), String> {
        Err(MISSING.into())
    }

    pub fn uninstall() -> Result<(), String> {
        Err(MISSING.into())
    }

    pub fn update(_release: impl FnOnce()) -> Result<(), String> {
        Err(MISSING.into())
    }

    pub fn available_update() -> Result<Option<String>, String> {
        Err(MISSING.into())
    }
}

pub fn driver_helper(_action: &str, _args: &[String]) -> Result<(), String> {
    Err(MISSING.into())
}

/// Keeps the profile's layout for the window; never opens a pad.
pub struct Runtime {
    pub config: Config,
    pub profile: u32,
    depth: [u8; 256],
}

impl Default for Runtime {
    fn default() -> Self {
        Self::new()
    }
}

impl Runtime {
    pub fn new() -> Self {
        Self { config: Config::default(), profile: 0, depth: [0; 256] }
    }

    pub fn configure(&mut self, profile: u32, config: Config, _request: bool) {
        self.profile = profile;
        self.config = config;
    }

    pub fn feed(&mut self, depth: &[u8; 256]) {
        self.depth = *depth;
    }

    pub fn neutral(&mut self) {
        self.depth = [0; 256];
    }

    pub fn stop(&mut self) {
        self.neutral();
    }

    pub fn suspend(&mut self) {
        self.neutral();
    }

    pub fn hold(&mut self) {}

    pub fn resume(&mut self) {}

    pub fn input(&mut self) -> Input {
        let error = self.config.enabled.then(|| MISSING.to_string());
        Input { depth: self.depth.to_vec(), running: false, error }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_pad_turned_on_says_it_is_missing_and_keys_keep_typing() {
        let mut runtime = Runtime::new();
        let config = Config { enabled: true, ..Default::default() };
        runtime.configure(1, config.clone(), true);
        assert_eq!(runtime.input().error.as_deref(), Some(MISSING));
        let mut cfg = razer_core::rapid::Config {
            act: [128; 256], fn_act: [128; 256], rapid: [None; 256],
            bind: [None; 256], fn_bind: [None; 256],
            repeat_delay: std::time::Duration::from_millis(500), repeat_interval: std::time::Duration::from_millis(30),
            macros: Default::default(), margin: 1, snap_on: true, snap: Vec::new(),
        };
        config.configure(&mut cfg);
        assert!(cfg.bind.iter().all(Option::is_none));
    }
}
