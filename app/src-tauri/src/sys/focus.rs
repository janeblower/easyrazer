//! Where typed keys land: the input desktop, the foreground process and integrity levels.

use std::cell::Cell;
use std::os::windows::io::{AsRawHandle, BorrowedHandle};
use std::ptr::null_mut;
use windows_sys::Win32::Foundation::{CloseHandle, HWND};
use windows_sys::Win32::Security::{GetSidSubAuthority, GetSidSubAuthorityCount, GetTokenInformation, TOKEN_MANDATORY_LABEL, TOKEN_QUERY, TokenIntegrityLevel};
use windows_sys::Win32::System::StationsAndDesktops::{CloseDesktop, DESKTOP_READOBJECTS, OpenInputDesktop};
use windows_sys::Win32::System::Threading::OpenProcessToken;
use windows_sys::Win32::UI::Accessibility::{HWINEVENTHOOK, SetWinEventHook};
use windows_sys::Win32::UI::WindowsAndMessaging::{GetForegroundWindow, GetMessageW, GetWindowThreadProcessId, MSG, WINEVENT_OUTOFCONTEXT};

/// Whether this session may open the input desktop, which it may not while the secure desktop shows.
pub fn input_desktop_open() -> bool {
    let desk = unsafe { OpenInputDesktop(0, 0, DESKTOP_READOBJECTS) };
    if desk.is_null() {
        return false;
    }
    unsafe { CloseDesktop(desk) };
    true
}

/// The process of the foreground window, or 0 when there is none.
pub fn foreground_process_id() -> u32 {
    let mut pid = 0;
    unsafe { GetWindowThreadProcessId(GetForegroundWindow(), &mut pid) };
    pid
}

/// The mandatory integrity level of a process: 0x2000 for a normal app, 0x3000 for an elevated one.
pub fn integrity(process: BorrowedHandle<'_>) -> Option<u32> {
    unsafe {
        let mut token = null_mut();
        if OpenProcessToken(process.as_raw_handle(), TOKEN_QUERY, &mut token) == 0 {
            return None;
        }
        let mut buf = [0u64; 16];
        let mut len = 0;
        let ok = GetTokenInformation(token, TokenIntegrityLevel, buf.as_mut_ptr().cast(), size_of_val(&buf) as u32, &mut len);
        CloseHandle(token);
        if ok == 0 {
            return None;
        }
        // The aligned buffer now holds a TOKEN_MANDATORY_LABEL whose SID points into it.
        let sid = (*buf.as_ptr().cast::<TOKEN_MANDATORY_LABEL>()).Label.Sid;
        Some(*GetSidSubAuthority(sid, u32::from(*GetSidSubAuthorityCount(sid)) - 1))
    }
}

thread_local! {
    static ON_EVENT: Cell<Option<fn()>> = const { Cell::new(None) };
}

unsafe extern "system" fn win_event(_: HWINEVENTHOOK, _: u32, _: HWND, _: i32, _: i32, _: u32, _: u32) {
    if let Some(f) = ON_EVENT.get() {
        f();
    }
}

/// Calls `on_event` on this thread for each of the system events, forever.
pub fn watch_win_events(events: &[u32], on_event: fn()) {
    ON_EVENT.set(Some(on_event));
    for &event in events {
        unsafe { SetWinEventHook(event, event, null_mut(), Some(win_event), 0, 0, WINEVENT_OUTOFCONTEXT) };
    }
    let mut msg: MSG = unsafe { std::mem::zeroed() };
    while unsafe { GetMessageW(&mut msg, null_mut(), 0, 0) } > 0 {}
}
