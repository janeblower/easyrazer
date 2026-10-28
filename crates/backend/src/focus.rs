//! Watches where typed keys land: `SendInput` cannot reach the secure desktop (lock screen, UAC)
//! or a window of a higher integrity level, such as an app run as administrator.

use std::cell::Cell;
use std::os::windows::io::AsHandle;
use std::sync::mpsc::{self, Receiver, Sender};

use windows_sys::Win32::System::Threading::PROCESS_QUERY_LIMITED_INFORMATION;
use windows_sys::Win32::UI::WindowsAndMessaging::{EVENT_SYSTEM_DESKTOPSWITCH, EVENT_SYSTEM_FOREGROUND};

use crate::sys;

thread_local! {
    static WATCH: Cell<Option<(Sender<bool>, u32, bool)>> = const { Cell::new(None) };
}

/// Reports each change of whether injected keys would be dropped, starting from `false`.
pub fn watch() -> Receiver<bool> {
    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || {
        let own = sys::integrity(sys::current_process()).unwrap_or(0);
        WATCH.set(Some((tx, own, false)));
        changed();
        sys::watch_win_events(&[EVENT_SYSTEM_FOREGROUND, EVENT_SYSTEM_DESKTOPSWITCH], changed);
    });
    rx
}

fn changed() {
    let Some((tx, own, was)) = WATCH.take() else { return };
    let now = blocked(own);
    if now != was {
        let _ = tx.send(now);
    }
    WATCH.set(Some((tx, own, now)));
}

fn blocked(own: u32) -> bool {
    // The user's session may not open the secure desktop.
    if !sys::input_desktop_open() {
        return true;
    }
    let pid = sys::foreground_process_id();
    // Only SYSTEM and protected processes refuse to open; anti-cheat guarded games are among
    // them, and losing driver mode there is worse than missing a rare SYSTEM window.
    let process = sys::open_process(pid, PROCESS_QUERY_LIMITED_INFORMATION);
    let Ok(process) = process.as_ref().map(AsHandle::as_handle) else { return false };
    if pid == 0 {
        return false;
    }
    // A same-user app at our level always shows its token; an elevated Electron app, such as
    // VS Code run as administrator, hides it.
    sys::integrity(process).is_none_or(|l| l > own)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_own_integrity_level() {
        assert!(sys::integrity(sys::current_process()).is_some_and(|l| l >= 0x2000));
    }
}
