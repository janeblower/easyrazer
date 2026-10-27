//! Kernel handles: processes, mutexes and waits.

use super::wide;
use std::io;
use std::os::windows::io::{AsRawHandle, BorrowedHandle, FromRawHandle, OwnedHandle};
use std::ptr::null_mut;
use windows_sys::Win32::Devices::DeviceAndDriverInstallation::DiUninstallDriverW;
use windows_sys::Win32::Foundation::{
    CloseHandle, ERROR_ALREADY_EXISTS, HANDLE, INVALID_HANDLE_VALUE, WAIT_OBJECT_0,
    WAIT_TIMEOUT,
};
use windows_sys::Win32::System::Diagnostics::ToolHelp::{CreateToolhelp32Snapshot, PROCESSENTRY32W, Process32FirstW, Process32NextW, TH32CS_SNAPPROCESS};
use windows_sys::Win32::System::Threading::*;

/// Owns a handle a Win32 call just returned, failing on its error values.
pub(super) fn owned(handle: HANDLE) -> io::Result<OwnedHandle> {
    if handle.is_null() || handle == INVALID_HANDLE_VALUE {
        Err(io::Error::last_os_error())
    } else {
        // SAFETY: the call returned a fresh handle that nothing else owns.
        Ok(unsafe { OwnedHandle::from_raw_handle(handle) })
    }
}

/// Whether the create call just made on this thread opened an existing object.
pub(super) fn already_exists() -> bool {
    io::Error::last_os_error().raw_os_error() == Some(ERROR_ALREADY_EXISTS as i32)
}

/// This process, through its pseudo-handle, which stays valid for its whole life. Waits
/// reject pseudo-handles; open the process by its ID to wait on it.
pub fn current_process() -> BorrowedHandle<'static> {
    // SAFETY: no arguments.
    let raw = unsafe { GetCurrentProcess() };
    // SAFETY: the pseudo-handle is never closed and stays valid for the process's life.
    unsafe { BorrowedHandle::borrow_raw(raw) }
}

pub fn open_process(pid: u32, access: u32) -> io::Result<OwnedHandle> {
    // SAFETY: takes values only; the returned handle goes straight to `owned`.
    owned(unsafe { OpenProcess(access, 0, pid) })
}

pub fn exit_code(process: BorrowedHandle<'_>) -> io::Result<u32> {
    let mut code = 0;
    // SAFETY: `process` is borrowed live for the call; `code` is writable.
    if unsafe { GetExitCodeProcess(process.as_raw_handle(), &mut code) } == 0 {
        return Err(io::Error::last_os_error());
    }
    Ok(code)
}

/// Waits up to `ms` for the handle; false on timeout.
pub fn wait(handle: BorrowedHandle<'_>, ms: u32) -> io::Result<bool> {
    // SAFETY: `handle` is borrowed live for the whole wait.
    match unsafe { WaitForSingleObject(handle.as_raw_handle(), ms) } {
        WAIT_OBJECT_0 => Ok(true),
        WAIT_TIMEOUT => Ok(false),
        _ => Err(io::Error::last_os_error()),
    }
}

/// Waits up to `ms` for any of the handles; the index of a signaled one, or `None` on timeout.
pub fn wait_any(handles: &[BorrowedHandle<'_>], ms: u32) -> io::Result<Option<usize>> {
    let raw: Vec<HANDLE> = handles.iter().map(|h| h.as_raw_handle()).collect();
    // SAFETY: `raw` holds `len` handles borrowed live for the whole wait.
    match unsafe { WaitForMultipleObjects(raw.len() as u32, raw.as_ptr(), 0, ms) } {
        WAIT_TIMEOUT => Ok(None),
        rc if (rc as usize) < WAIT_OBJECT_0 as usize + raw.len() => Ok(Some((rc - WAIT_OBJECT_0) as usize)),
        _ => Err(io::Error::last_os_error()),
    }
}

/// Creates or opens a named mutex without acquiring it; true when it already existed.
pub fn create_mutex(name: &str) -> io::Result<(OwnedHandle, bool)> {
    let name = wide(name);
    // SAFETY: `name` is NUL-terminated and alive for the call; null means default security.
    let handle = owned(unsafe { CreateMutexW(std::ptr::null(), 0, name.as_ptr()) })?;
    Ok((handle, already_exists()))
}

/// Whether a process with this executable name runs.
pub fn process_running(exe: &str) -> io::Result<bool> {
    // SAFETY: takes values only; the snapshot is closed below.
    let snap = unsafe { CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0) };
    if snap == INVALID_HANDLE_VALUE {
        return Err(io::Error::last_os_error());
    }
    let mut entry = PROCESSENTRY32W { dwSize: std::mem::size_of::<PROCESSENTRY32W>() as u32, ..Default::default() };
    let mut found = false;
    // SAFETY: `snap` is open; `entry` is writable and its dwSize is set.
    let mut ok = unsafe { Process32FirstW(snap, &mut entry) };
    while ok != 0 && !found {
        let len = entry.szExeFile.iter().position(|&c| c == 0).unwrap_or(entry.szExeFile.len());
        found = String::from_utf16_lossy(&entry.szExeFile[..len]).eq_ignore_ascii_case(exe);
        // SAFETY: as for Process32FirstW.
        ok = unsafe { Process32NextW(snap, &mut entry) };
    }
    // SAFETY: `snap` is ours and closed once.
    unsafe { CloseHandle(snap) };
    Ok(found)
}

/// Removes a driver package by its INF; true when Windows needs a restart.
pub fn uninstall_driver(inf: &std::path::Path) -> io::Result<bool> {
    let mut needed = 0;
    // SAFETY: the path is a NUL-terminated temporary alive for the call; `needed` is writable.
    if unsafe { DiUninstallDriverW(null_mut(), wide(&inf.to_string_lossy()).as_ptr(), 0, &mut needed) } == 0 {
        return Err(io::Error::last_os_error());
    }
    Ok(needed != 0)
}
