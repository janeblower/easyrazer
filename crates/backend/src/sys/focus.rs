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
    // SAFETY: flags only; the returned handle is ours to close.
    let desk = unsafe { OpenInputDesktop(0, 0, DESKTOP_READOBJECTS) };
    if desk.is_null() {
        return false;
    }
    // SAFETY: `desk` was opened above and is closed once.
    unsafe { CloseDesktop(desk) };
    true
}

/// The process of the foreground window, or 0 when there is none.
pub fn foreground_process_id() -> u32 {
    let mut pid = 0;
    // SAFETY: no arguments.
    let hwnd = unsafe { GetForegroundWindow() };
    // SAFETY: a null or stale `hwnd` only fails the call and leaves `pid` 0; `pid` is writable.
    unsafe { GetWindowThreadProcessId(hwnd, &mut pid) };
    pid
}

/// The mandatory integrity level of a process: 0x2000 for a normal app, 0x3000 for an elevated one.
pub fn integrity(process: BorrowedHandle<'_>) -> Option<u32> {
    let mut token = null_mut();
    // SAFETY: `process` is a live handle borrowed for the call; `token` is writable.
    if unsafe { OpenProcessToken(process.as_raw_handle(), TOKEN_QUERY, &mut token) } == 0 {
        return None;
    }
    let mut buf = [0u64; 16];
    let mut len = 0;
    // SAFETY: `token` was opened above; `buf` is writable for the size passed.
    let ok = unsafe { GetTokenInformation(token, TokenIntegrityLevel, buf.as_mut_ptr().cast(), size_of_val(&buf) as u32, &mut len) };
    // SAFETY: `token` is ours and closed once.
    unsafe { CloseHandle(token) };
    if ok == 0 {
        return None;
    }
    // SAFETY: the u64 buffer is aligned for, and now holds, a TOKEN_MANDATORY_LABEL whose SID points into it.
    let sid = unsafe { (*buf.as_ptr().cast::<TOKEN_MANDATORY_LABEL>()).Label.Sid };
    // SAFETY: `sid` is a valid SID inside `buf`, which is still alive.
    let count = unsafe { GetSidSubAuthorityCount(sid) };
    // SAFETY: the count points into the SID in `buf`.
    let last = u32::from(unsafe { *count }) - 1;
    // SAFETY: an integrity SID has exactly one sub-authority, so `last` indexes inside it.
    let level = unsafe { GetSidSubAuthority(sid, last) };
    // SAFETY: the sub-authority points into the SID in `buf`.
    Some(unsafe { *level })
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
        // SAFETY: `win_event` matches WINEVENTPROC and lives for the program; out-of-context hooks need no module.
        unsafe { SetWinEventHook(event, event, null_mut(), Some(win_event), 0, 0, WINEVENT_OUTOFCONTEXT) };
    }
    let mut msg = MSG::default();
    // SAFETY: `msg` is writable; pumping this thread's queue is what delivers the hook calls.
    while unsafe { GetMessageW(&mut msg, null_mut(), 0, 0) } > 0 {}
}
