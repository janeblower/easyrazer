use crate::autostart::wide;
use std::io;
use std::mem::size_of;
use std::os::windows::io::{AsRawHandle, FromRawHandle, OwnedHandle};
use std::ptr;
use std::sync::atomic::{Ordering, fence};
use windows_sys::Win32::Foundation::{ERROR_ALREADY_EXISTS, GetLastError, HANDLE, INVALID_HANDLE_VALUE, WAIT_OBJECT_0, WAIT_TIMEOUT};
use windows_sys::Win32::Security::Cryptography::{BCRYPT_USE_SYSTEM_PREFERRED_RNG, BCryptGenRandom};
use windows_sys::Win32::System::{Memory::*, Threading::*};

pub(super) fn owned(handle: HANDLE) -> io::Result<OwnedHandle> {
    if handle.is_null() || handle == INVALID_HANDLE_VALUE {
        Err(io::Error::last_os_error())
    } else {
        // The caller transfers a newly opened handle to this owner.
        Ok(unsafe { OwnedHandle::from_raw_handle(handle) })
    }
}

#[derive(Clone, Copy)]
#[repr(C)]
struct Reply {
    handles: [usize; 3],
    length: u32,
    error: [u8; 512],
}

pub(super) struct Channel {
    pub(super) name: String,
    pub(super) ready: OwnedHandle,
    pub(super) stop: OwnedHandle,
    pub(super) attached: OwnedHandle,
    view: MEMORY_MAPPED_VIEW_ADDRESS,
    _mapping: OwnedHandle,
}

impl Channel {
    pub(super) fn create() -> io::Result<Self> {
        let mut token = [0u8; 32];
        if unsafe { BCryptGenRandom(ptr::null_mut(), token.as_mut_ptr(), token.len() as u32, BCRYPT_USE_SYSTEM_PREFERRED_RNG) } < 0 {
            return Err(io::Error::other("cannot generate IPC token"));
        }
        let secret: String = token.iter().map(|b| format!("{b:02x}")).collect();
        Self::connect(&format!("Local\\EasyRazerGamepad-{}-{secret}", std::process::id()), true)
    }

    pub(super) fn open(name: &str) -> io::Result<Self> {
        Self::connect(name, false)
    }

    fn connect(name: &str, create: bool) -> io::Result<Self> {
        let mapping = if create {
            let handle = owned(unsafe {
                CreateFileMappingW(
                    INVALID_HANDLE_VALUE,
                    ptr::null(),
                    PAGE_READWRITE,
                    0,
                    size_of::<Reply>() as u32,
                    wide(name).as_ptr(),
                )
            })?;
            if unsafe { GetLastError() } == ERROR_ALREADY_EXISTS {
                return Err(io::Error::new(io::ErrorKind::AlreadyExists, "control mapping already exists"));
            }
            handle
        } else {
            owned(unsafe { OpenFileMappingW(FILE_MAP_READ | FILE_MAP_WRITE, 0, wide(name).as_ptr()) })?
        };
        let mut events = Vec::new();
        for suffix in ["Ready", "Stop", "Attached"] {
            let name = wide(&format!("{name}-{suffix}"));
            let handle = if create {
                let handle = owned(unsafe { CreateEventW(ptr::null(), 1, 0, name.as_ptr()) })?;
                if unsafe { GetLastError() } == ERROR_ALREADY_EXISTS {
                    return Err(io::Error::new(io::ErrorKind::AlreadyExists, "control event already exists"));
                }
                handle
            } else {
                owned(unsafe { OpenEventW(EVENT_MODIFY_STATE | SYNCHRONIZATION_SYNCHRONIZE, 0, name.as_ptr()) })?
            };
            events.push(handle);
        }
        let view = unsafe { MapViewOfFile(mapping.as_raw_handle(), FILE_MAP_READ | FILE_MAP_WRITE, 0, 0, size_of::<Reply>()) };
        if view.Value.is_null() {
            return Err(io::Error::last_os_error());
        }
        let attached = events.pop().unwrap();
        let stop = events.pop().unwrap();
        let ready = events.pop().unwrap();
        Ok(Self {
            name: name.into(),
            ready,
            stop,
            attached,
            view,
            _mapping: mapping,
        })
    }

    pub(super) fn reply(&self, handles: [usize; 3], error: Option<&str>) {
        let mut reply = Reply {
            handles,
            length: 0,
            error: [0; 512],
        };
        if let Some(error) = error {
            let bytes = error.as_bytes();
            let size = bytes.len().min(reply.error.len());
            reply.error[..size].copy_from_slice(&bytes[..size]);
            reply.length = size as u32;
        }
        // The helper is the only writer; the parent reads after Ready or process exit.
        unsafe {
            self.view.Value.cast::<Reply>().write_volatile(reply);
        }
        fence(Ordering::SeqCst);
    }

    pub(super) fn signal_ready(&self) -> io::Result<()> {
        if unsafe { SetEvent(self.ready.as_raw_handle()) } == 0 {
            Err(io::Error::last_os_error())
        } else {
            Ok(())
        }
    }

    pub(super) fn wait_ready(&self, process: HANDLE) -> io::Result<()> {
        match unsafe { WaitForMultipleObjects(2, [self.ready.as_raw_handle(), process].as_ptr(), 0, 30_000) } {
            WAIT_OBJECT_0 => Ok(()),
            WAIT_TIMEOUT => Err(io::Error::new(io::ErrorKind::TimedOut, "helper startup")),
            n if n == WAIT_OBJECT_0 + 1 => Err(io::Error::other("helper exited before becoming ready")),
            _ => Err(io::Error::last_os_error()),
        }
    }

    pub(super) fn result(&self) -> io::Result<[usize; 3]> {
        fence(Ordering::SeqCst);
        // Ready/process termination synchronizes the one writer with this reader.
        let reply = unsafe { self.view.Value.cast::<Reply>().read_volatile() };
        if reply.length as usize > reply.error.len() {
            return Err(io::Error::new(io::ErrorKind::InvalidData, "invalid helper response"));
        }
        if reply.length != 0 {
            return Err(io::Error::other(String::from_utf8_lossy(&reply.error[..reply.length as usize]).into_owned()));
        }
        Ok(reply.handles)
    }
}

impl Drop for Channel {
    fn drop(&mut self) {
        // All accesses to this process's view have ended.
        unsafe {
            UnmapViewOfFile(self.view);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn startup_errors_cross_the_control_channel() {
        let channel = Channel::create().unwrap();
        let name = channel.name.clone();
        let task = std::thread::spawn(move || {
            let peer = Channel::open(&name).unwrap();
            peer.reply([0; 3], Some("device setup failed"));
            peer.signal_ready().unwrap();
        });
        let process = owned(unsafe { OpenProcess(PROCESS_SYNCHRONIZE, 0, std::process::id()) }).unwrap();
        channel.wait_ready(process.as_raw_handle()).unwrap();
        task.join().unwrap();
        assert_eq!(channel.result().unwrap_err().to_string(), "device setup failed");
    }

    #[test]
    fn control_channel_transfers_handle_values() {
        let channel = Channel::create().unwrap();
        let peer = Channel::open(&channel.name).unwrap();
        peer.reply([123, 456, 789], None);
        peer.signal_ready().unwrap();
        let process = owned(unsafe { OpenProcess(PROCESS_SYNCHRONIZE, 0, std::process::id()) }).unwrap();
        channel.wait_ready(process.as_raw_handle()).unwrap();
        assert_eq!(channel.result().unwrap(), [123, 456, 789]);
    }
}
