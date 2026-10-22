//! Starting things through the shell.

use super::handle::owned;
use super::wide;
use std::io;
use std::os::windows::io::OwnedHandle;
use windows_sys::Win32::UI::Shell::{SEE_MASK_NOCLOSEPROCESS, SHELLEXECUTEINFOW, ShellExecuteExW};
use windows_sys::Win32::UI::WindowsAndMessaging::SW_HIDE;

/// Starts the program hidden and elevated, which asks the user, and returns its process.
pub fn run_elevated(exe: &std::path::Path, args: &str) -> io::Result<OwnedHandle> {
    use std::os::windows::ffi::OsStrExt;
    let exe: Vec<u16> = exe.as_os_str().encode_wide().chain([0]).collect();
    let verb = wide("runas");
    let args = wide(args);
    let mut info = SHELLEXECUTEINFOW {
        cbSize: size_of::<SHELLEXECUTEINFOW>() as u32,
        fMask: SEE_MASK_NOCLOSEPROCESS,
        lpVerb: verb.as_ptr(),
        lpFile: exe.as_ptr(),
        lpParameters: args.as_ptr(),
        nShow: SW_HIDE,
        ..Default::default()
    };
    // SAFETY: `info` and the strings it points to live across the call.
    if unsafe { ShellExecuteExW(&mut info) } == 0 {
        return Err(io::Error::last_os_error());
    }
    owned(info.hProcess)
}
