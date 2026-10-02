//! Watches where typed keys land: `SendInput` cannot reach the secure desktop (lock screen, UAC)
//! or a window of a higher integrity level, such as an app run as administrator.

use std::cell::Cell;
use std::ptr::null_mut;
use std::sync::mpsc::{self, Receiver, Sender};

use windows_sys::Win32::Foundation::{CloseHandle, HANDLE, HWND};
use windows_sys::Win32::Security::{
    GetSidSubAuthority, GetSidSubAuthorityCount, GetTokenInformation, TOKEN_MANDATORY_LABEL, TOKEN_QUERY, TokenIntegrityLevel,
};
use windows_sys::Win32::System::StationsAndDesktops::{CloseDesktop, DESKTOP_READOBJECTS, OpenInputDesktop};
use windows_sys::Win32::System::Threading::{GetCurrentProcess, OpenProcess, OpenProcessToken, PROCESS_QUERY_LIMITED_INFORMATION};
use windows_sys::Win32::UI::Accessibility::{HWINEVENTHOOK, SetWinEventHook};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    EVENT_SYSTEM_DESKTOPSWITCH, EVENT_SYSTEM_FOREGROUND, GetForegroundWindow, GetMessageW, GetWindowThreadProcessId, MSG,
    WINEVENT_OUTOFCONTEXT,
};

thread_local! {
    static WATCH: Cell<Option<(Sender<bool>, u32, bool)>> = const { Cell::new(None) };
}

/// Reports each change of whether injected keys would be dropped, starting from `false`.
pub fn watch() -> Receiver<bool> {
    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || unsafe {
        let own = integrity(GetCurrentProcess()).unwrap_or(0);
        WATCH.set(Some((tx, own, false)));
        changed(null_mut(), 0, null_mut(), 0, 0, 0, 0);
        for event in [EVENT_SYSTEM_FOREGROUND, EVENT_SYSTEM_DESKTOPSWITCH] {
            SetWinEventHook(event, event, null_mut(), Some(changed), 0, 0, WINEVENT_OUTOFCONTEXT);
        }
        let mut msg: MSG = std::mem::zeroed();
        while GetMessageW(&mut msg, null_mut(), 0, 0) > 0 {}
    });
    rx
}

unsafe extern "system" fn changed(_: HWINEVENTHOOK, _: u32, _: HWND, _: i32, _: i32, _: u32, _: u32) {
    let Some((tx, own, was)) = WATCH.take() else { return };
    let now = blocked(own);
    if now != was {
        let _ = tx.send(now);
    }
    WATCH.set(Some((tx, own, now)));
}

fn blocked(own: u32) -> bool {
    unsafe {
        // The user's session may not open the secure desktop.
        let desk = OpenInputDesktop(0, 0, DESKTOP_READOBJECTS);
        if desk.is_null() {
            return true;
        }
        CloseDesktop(desk);
        let mut pid = 0;
        GetWindowThreadProcessId(GetForegroundWindow(), &mut pid);
        // Only SYSTEM and protected processes refuse to open; anti-cheat guarded games are among
        // them, and losing driver mode there is worse than missing a rare SYSTEM window.
        let process = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid);
        if pid == 0 || process.is_null() {
            return false;
        }
        let level = integrity(process);
        CloseHandle(process);
        level.is_some_and(|l| l > own)
    }
}

/// The mandatory integrity level of a process: 0x2000 for a normal app, 0x3000 for an elevated one.
fn integrity(process: HANDLE) -> Option<u32> {
    unsafe {
        let mut token = null_mut();
        if OpenProcessToken(process, TOKEN_QUERY, &mut token) == 0 {
            return None;
        }
        let mut buf = [0u64; 16];
        let mut len = 0;
        let ok = GetTokenInformation(token, TokenIntegrityLevel, buf.as_mut_ptr().cast(), size_of_val(&buf) as u32, &mut len);
        CloseHandle(token);
        if ok == 0 {
            return None;
        }
        let sid = (*buf.as_ptr().cast::<TOKEN_MANDATORY_LABEL>()).Label.Sid;
        Some(*GetSidSubAuthority(sid, u32::from(*GetSidSubAuthorityCount(sid)) - 1))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_own_integrity_level() {
        assert!(unsafe { integrity(GetCurrentProcess()) }.is_some_and(|l| l >= 0x2000));
    }
}
