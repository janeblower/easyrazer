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
        // The call returned a fresh handle that nothing else owns.
        Ok(unsafe { OwnedHandle::from_raw_handle(handle) })
    }
}

/// This process, through its pseudo-handle, which stays valid for its whole life. Waits
/// reject pseudo-handles; open the process by its ID to wait on it.
pub fn current_process() -> BorrowedHandle<'static> {
    unsafe { BorrowedHandle::borrow_raw(GetCurrentProcess()) }
}

pub fn open_process(pid: u32, access: u32) -> io::Result<OwnedHandle> {
    owned(unsafe { OpenProcess(access, 0, pid) })
}

pub fn exit_code(process: BorrowedHandle<'_>) -> io::Result<u32> {
    let mut code = 0;
    if unsafe { GetExitCodeProcess(process.as_raw_handle(), &mut code) } == 0 {
        return Err(io::Error::last_os_error());
    }
    Ok(code)
}

#[cfg(test)]
pub fn process_id(process: BorrowedHandle<'_>) -> u32 {
    unsafe { GetProcessId(process.as_raw_handle()) }
}

/// Waits up to `ms` for the handle; false on timeout.
pub fn wait(handle: BorrowedHandle<'_>, ms: u32) -> io::Result<bool> {
    match unsafe { WaitForSingleObject(handle.as_raw_handle(), ms) } {
        WAIT_OBJECT_0 => Ok(true),
        WAIT_TIMEOUT => Ok(false),
        _ => Err(io::Error::last_os_error()),
    }
}

/// Waits up to `ms` for any of the handles; the index of a signaled one, or `None` on timeout.
pub fn wait_any(handles: &[BorrowedHandle<'_>], ms: u32) -> io::Result<Option<usize>> {
    let raw: Vec<HANDLE> = handles.iter().map(|h| h.as_raw_handle()).collect();
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
        // The source closes in the other process; the copy here is new and only this call owns it.
        let ok = unsafe {
            DuplicateHandle(self.process.as_raw_handle(), self.value as HANDLE, GetCurrentProcess(), &mut local, 0, 0, DUPLICATE_CLOSE_SOURCE)
        } != 0;
        if ok {
            drop(owned(local));
        }
        ok
    }
}

/// Duplicates a handle into another process.
pub fn duplicate_into<'p>(source: BorrowedHandle<'_>, process: BorrowedHandle<'p>, access: u32) -> io::Result<Remote<'p>> {
    let mut target = null_mut();
    if unsafe { DuplicateHandle(GetCurrentProcess(), source.as_raw_handle(), process.as_raw_handle(), &mut target, access, 0, 0) } == 0 {
        return Err(io::Error::last_os_error());
    }
    Ok(Remote { process, value: target as usize })
}

pub struct Event(OwnedHandle);
impl Event {
    /// Creates a named event, failing if the name is taken.
    pub(super) fn create(name: &str, manual: bool) -> io::Result<Self> {
        let name = wide(name);
        let handle = owned(unsafe { CreateEventW(std::ptr::null(), manual.into(), 0, name.as_ptr()) })?;
        if unsafe { GetLastError() } == ERROR_ALREADY_EXISTS {
            return Err(io::Error::new(io::ErrorKind::AlreadyExists, "control event already exists"));
        }
        Ok(Self(handle))
    }

    pub(super) fn open(name: &str, access: u32) -> io::Result<Self> {
        owned(unsafe { OpenEventW(access, 0, wide(name).as_ptr()) }).map(Self)
    }

    pub fn set(&self) -> io::Result<()> {
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
    owned(unsafe { CreateEventW(std::ptr::null(), 0, 0, std::ptr::null()) })
}

/// A pagefile-backed section, named or not.
#[cfg(test)]
pub fn create_mapping(name: Option<&str>, size: u32) -> io::Result<OwnedHandle> {
    use windows_sys::Win32::System::Memory::{CreateFileMappingW, PAGE_READWRITE};
    let name = name.map(wide);
    let name = name.as_ref().map_or(std::ptr::null(), |n| n.as_ptr());
    owned(unsafe { CreateFileMappingW(INVALID_HANDLE_VALUE, std::ptr::null(), PAGE_READWRITE, 0, size, name) })
}

#[cfg(test)]
pub fn mapping_exists(name: &str) -> bool {
    use windows_sys::Win32::System::Memory::{FILE_MAP_READ, OpenFileMappingW};
    owned(unsafe { OpenFileMappingW(FILE_MAP_READ, 0, wide(name).as_ptr()) }).is_ok()
}

/// Creates or opens a named mutex without acquiring it; true when it already existed.
pub fn create_mutex(name: &str) -> io::Result<(OwnedHandle, bool)> {
    let name = wide(name);
    let handle = owned(unsafe { CreateMutexW(std::ptr::null(), 0, name.as_ptr()) })?;
    Ok((handle, unsafe { GetLastError() } == ERROR_ALREADY_EXISTS))
}

/// Whether a process with this executable name runs.
pub fn process_running(exe: &str) -> io::Result<bool> {
    let snap = unsafe { CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0) };
    if snap == INVALID_HANDLE_VALUE {
        return Err(io::Error::last_os_error());
    }
    let mut entry: PROCESSENTRY32W = unsafe { std::mem::zeroed() };
    entry.dwSize = std::mem::size_of::<PROCESSENTRY32W>() as u32;
    let mut found = false;
    let mut ok = unsafe { Process32FirstW(snap, &mut entry) };
    while ok != 0 && !found {
        let len = entry.szExeFile.iter().position(|&c| c == 0).unwrap_or(entry.szExeFile.len());
        found = String::from_utf16_lossy(&entry.szExeFile[..len]).eq_ignore_ascii_case(exe);
        ok = unsafe { Process32NextW(snap, &mut entry) };
    }
    unsafe { CloseHandle(snap) };
    Ok(found)
}

/// Removes a driver package by its INF; true when Windows needs a restart.
pub fn uninstall_driver(inf: &std::path::Path) -> io::Result<bool> {
    let mut needed = 0;
    if unsafe { DiUninstallDriverW(null_mut(), wide(&inf.to_string_lossy()).as_ptr(), 0, &mut needed) } == 0 {
        return Err(io::Error::last_os_error());
    }
    Ok(needed != 0)
}
