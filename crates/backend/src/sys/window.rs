//! Window subclassing for the main window.

use windows_sys::Win32::Foundation::{HWND, LPARAM, LRESULT, WPARAM};
use windows_sys::Win32::UI::Shell::{DefSubclassProc, SetWindowSubclass};
use windows_sys::Win32::UI::WindowsAndMessaging::{ASFW_ANY, AllowSetForegroundWindow, SC_KEYMENU, WM_SYSCOMMAND};

unsafe extern "system" fn no_alt_menu_proc(hwnd: HWND, msg: u32, wp: WPARAM, lp: LPARAM, _: usize, _: usize) -> LRESULT {
    if msg == WM_SYSCOMMAND && (wp & 0xFFF0) == SC_KEYMENU as usize && lp == 0 {
        return 0;
    }
    // SAFETY: passes on what Windows gave this subclass procedure.
    unsafe { DefSubclassProc(hwnd, msg, wp, lp) }
}

/// Stops a lone Alt from opening the window menu; Alt+Space still opens the system menu.
///
/// `hwnd` is a window handle value; Windows rejects one that names no window of this thread.
pub fn no_alt_menu(hwnd: isize) {
    // SAFETY: Windows rejects a bad `hwnd`; `no_alt_menu_proc` matches SUBCLASSPROC and lives for the program.
    unsafe { SetWindowSubclass(hwnd as HWND, Some(no_alt_menu_proc), 1, 0) };
}

/// Lets whichever process asks next take the foreground; a second start passes on the right it got
/// from the user's click.
pub fn allow_foreground() {
    // SAFETY: takes no pointers.
    unsafe { AllowSetForegroundWindow(ASFW_ANY) };
}
