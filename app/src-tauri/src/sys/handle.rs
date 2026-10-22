//! Kernel handles: processes, events, mutexes, waits and handle transfer.

use super::wide;
use std::io;
use std::os::windows::io::{AsHandle, AsRawHandle, BorrowedHandle, FromRawHandle, OwnedHandle};
use std::ptr::null_mut;
use windows_sys::Win32::Devices::DeviceAndDriverInstallation::DiUninstallDriverW;
use windows_sys::Win32::Foundation::{
    CloseHandle, DUPLICATE_CLOSE_SOURCE, DuplicateHandle, ERROR_ALREADY_EXISTS, GetLastError, HANDLE, INVALID_HANDLE_VALUE, WAIT_OBJECT_0,
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

#[cfg(test)]
pub fn process_id(process: BorrowedHandle<'_>) -> u32 {
    // SAFETY: `process` is borrowed live for the call.
    unsafe { GetProcessId(process.as_raw_handle()) }
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

/// A handle this process duplicated into another one, not yet handed over.
#[derive(Clone, Copy)]
pub struct Remote<'p> {
    process: BorrowedHandle<'p>,
    value: usize,
}
impl Remote<'_> {
    /// The handle's value in the other process.
    pub fn value(&self) -> usize {
        self.value
    }

    /// Closes the handle in the other process; false if it was no longer open there.
    pub fn close(self) -> bool {
        let mut local = null_mut();
        // SAFETY: no arguments.
        let here = unsafe { GetCurrentProcess() };
        // SAFETY: `value` names a handle in `process`, which is borrowed live; the source closes
        // there, and the copy here is new and only this call owns it.
        let ok = unsafe { DuplicateHandle(self.process.as_raw_handle(), self.value as HANDLE, here, &mut local, 0, 0, DUPLICATE_CLOSE_SOURCE) } != 0;
        if ok {
            drop(owned(local));
        }
        ok
    }
}

/// Duplicates a handle into another process.
pub fn duplicate_into<'p>(source: BorrowedHandle<'_>, process: BorrowedHandle<'p>, access: u32) -> io::Result<Remote<'p>> {
    let mut target = null_mut();
    // SAFETY: no arguments.
    let here = unsafe { GetCurrentProcess() };
    // SAFETY: `source` and `process` are borrowed live for the call; `target` is writable.
    if unsafe { DuplicateHandle(here, source.as_raw_handle(), process.as_raw_handle(), &mut target, access, 0, 0) } == 0 {
        return Err(io::Error::last_os_error());
    }
    Ok(Remote { process, value: target as usize })
}

pub struct Event(OwnedHandle);
impl Event {
    /// Creates a named event, failing if the name is taken.
    pub(super) fn create(name: &str, manual: bool) -> io::Result<Self> {
        let name = wide(name);
        // SAFETY: `name` is NUL-terminated and alive for the call; null means default security.
        let handle = owned(unsafe { CreateEventW(std::ptr::null(), manual.into(), 0, name.as_ptr()) })?;
        // SAFETY: no arguments; read right after CreateEventW on this thread.
        if unsafe { GetLastError() } == ERROR_ALREADY_EXISTS {
            return Err(io::Error::new(io::ErrorKind::AlreadyExists, "control event already exists"));
        }
        Ok(Self(handle))
    }

    pub(super) fn open(name: &str, access: u32) -> io::Result<Self> {
        // SAFETY: the name is a NUL-terminated temporary alive for the call.
        owned(unsafe { OpenEventW(access, 0, wide(name).as_ptr()) }).map(Self)
    }

    pub fn set(&self) -> io::Result<()> {
        // SAFETY: the event handle is owned by `self`.
        if unsafe { SetEvent(self.0.as_raw_handle()) } == 0 { Err(io::Error::last_os_error()) } else { Ok(()) }
    }

    #[cfg(test)]
    pub fn wait(&self, ms: u32) -> io::Result<bool> {
        wait(self.handle(), ms)
    }

    pub fn handle(&self) -> BorrowedHandle<'_> {
        self.0.as_handle()
    }
}

/// An unnamed auto-reset event.
#[cfg(test)]
pub fn event() -> io::Result<OwnedHandle> {
    // SAFETY: null attributes and name ask for an unnamed event with default security.
    owned(unsafe { CreateEventW(std::ptr::null(), 0, 0, std::ptr::null()) })
}

/// A pagefile-backed section, named or not.
#[cfg(test)]
pub fn create_mapping(name: Option<&str>, size: u32) -> io::Result<OwnedHandle> {
    use windows_sys::Win32::System::Memory::{CreateFileMappingW, PAGE_READWRITE};
    let name = name.map(wide);
    let name = name.as_ref().map_or(std::ptr::null(), |n| n.as_ptr());
    // SAFETY: `name` is null or points into the NUL-terminated buffer above, alive for the call.
    owned(unsafe { CreateFileMappingW(INVALID_HANDLE_VALUE, std::ptr::null(), PAGE_READWRITE, 0, size, name) })
}

#[cfg(test)]
pub fn mapping_exists(name: &str) -> bool {
    use windows_sys::Win32::System::Memory::{FILE_MAP_READ, OpenFileMappingW};
    // SAFETY: the name is a NUL-terminated temporary alive for the call.
    owned(unsafe { OpenFileMappingW(FILE_MAP_READ, 0, wide(name).as_ptr()) }).is_ok()
}

/// Creates or opens a named mutex without acquiring it; true when it already existed.
pub fn create_mutex(name: &str) -> io::Result<(OwnedHandle, bool)> {
    let name = wide(name);
    // SAFETY: `name` is NUL-terminated and alive for the call; null means default security.
    let handle = owned(unsafe { CreateMutexW(std::ptr::null(), 0, name.as_ptr()) })?;
    // SAFETY: no arguments; read right after CreateMutexW on this thread.
    Ok((handle, unsafe { GetLastError() } == ERROR_ALREADY_EXISTS))
}

/// Whether a process with this executable name runs.
pub fn process_running(exe: &str) -> io::Result<bool> {
    // SAFETY: takes values only; the snapshot is closed below.
    let snap = unsafe { CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0) };
    if snap == INVALID_HANDLE_VALUE {
        return Err(io::Error::last_os_error());
    }
    // SAFETY: PROCESSENTRY32W is plain data; all zeroes is a valid value.
    let mut entry: PROCESSENTRY32W = unsafe { std::mem::zeroed() };
    entry.dwSize = std::mem::size_of::<PROCESSENTRY32W>() as u32;
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
