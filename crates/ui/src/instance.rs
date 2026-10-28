//! One copy of the app per session: a second start brings the first one's window forward.

use std::os::windows::io::{AsHandle, OwnedHandle};
use std::sync::atomic::{AtomicBool, Ordering};

use easyrazer_backend::sys;

const MUTEX: &str = "Local\\EasyRazer.Instance";
const ACTIVATE: &str = "Local\\EasyRazer.Activate";

/// Set by a second start; the window shows itself on its next pass.
pub static ACTIVATE_REQUESTED: AtomicBool = AtomicBool::new(false);

pub struct Instance {
    mutex: OwnedHandle,
    activate: OwnedHandle,
}

/// `None` when another copy runs; it has been asked to show its window.
pub fn acquire() -> Option<Instance> {
    let (mutex, existed) = sys::create_mutex(MUTEX).ok()?;
    let (activate, _) = sys::create_event(ACTIVATE).ok()?;
    if existed {
        sys::allow_foreground();
        let _ = sys::set_event(activate.as_handle());
        return None;
    }
    Some(Instance { mutex, activate })
}

impl Instance {
    /// Wakes the window whenever a second start asks for it; the handles live as long as the process.
    pub fn on_activate(self, ctx: egui::Context) {
        std::thread::spawn(move || {
            let _held = self.mutex;
            while sys::wait(self.activate.as_handle(), u32::MAX).unwrap_or(false) {
                ACTIVATE_REQUESTED.store(true, Ordering::Relaxed);
                ctx.request_repaint();
            }
        });
    }
}
