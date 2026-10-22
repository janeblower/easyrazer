//! The gamepad helper's control channel: a reply section and three events.

use super::handle::{Event, owned};
use super::wide;
use std::cell::Cell;
use std::io;
use std::mem::size_of;
use std::os::windows::io::{AsRawHandle, BorrowedHandle, OwnedHandle};
use std::ptr;
use std::sync::atomic::{Ordering, fence};
use windows_sys::Win32::Foundation::{ERROR_ALREADY_EXISTS, GetLastError, INVALID_HANDLE_VALUE, WAIT_OBJECT_0, WAIT_TIMEOUT};
use windows_sys::Win32::Security::Cryptography::{BCRYPT_USE_SYSTEM_PREFERRED_RNG, BCryptGenRandom};
use windows_sys::Win32::System::{Memory::*, Threading::*};

#[derive(Clone, Copy)]
#[repr(C)]
struct Reply {
    handles: [usize; 3],
    length: u32,
    error: [u8; 512],
}

pub struct Channel {
    pub name: String,
    pub ready: Event,
    pub stop: Event,
    pub attached: Event,
    view: MEMORY_MAPPED_VIEW_ADDRESS,
    _mapping: OwnedHandle,
    attached_once: Cell<bool>,
}

// SAFETY: the view is process-wide memory that stays mapped across thread moves; its owner
// serializes access.
unsafe impl Send for Channel {}

impl Channel {
    pub fn create() -> io::Result<Self> {
        let mut token = [0u8; 32];
        // SAFETY: `token` is writable for the length passed; a null algorithm with the
        // system-preferred flag is the documented form.
        if unsafe { BCryptGenRandom(ptr::null_mut(), token.as_mut_ptr(), token.len() as u32, BCRYPT_USE_SYSTEM_PREFERRED_RNG) } < 0 {
            return Err(io::Error::other("cannot generate IPC token"));
        }
        let secret: String = token.iter().map(|b| format!("{b:02x}")).collect();
        Self::connect(&format!("Local\\EasyRazerGamepad-{}-{secret}", std::process::id()), true)
    }

    pub fn open(name: &str) -> io::Result<Self> {
        Self::connect(name, false)
    }

    fn connect(name: &str, create: bool) -> io::Result<Self> {
        let mapping = if create {
            let wide_name = wide(name);
            // SAFETY: `wide_name` is NUL-terminated and alive for the call; null means default security.
            let handle = owned(unsafe {
                CreateFileMappingW(INVALID_HANDLE_VALUE, ptr::null(), PAGE_READWRITE, 0, size_of::<Reply>() as u32, wide_name.as_ptr())
            })?;
            // SAFETY: no arguments; read right after CreateFileMappingW on this thread.
            if unsafe { GetLastError() } == ERROR_ALREADY_EXISTS {
                return Err(io::Error::new(io::ErrorKind::AlreadyExists, "control mapping already exists"));
            }
            handle
        } else {
            // SAFETY: the name is a NUL-terminated temporary alive for the call.
            owned(unsafe { OpenFileMappingW(FILE_MAP_READ | FILE_MAP_WRITE, 0, wide(name).as_ptr()) })?
        };
        let mut events = Vec::new();
        for suffix in ["Ready", "Stop", "Attached"] {
            let name = format!("{name}-{suffix}");
            events.push(if create { Event::create(&name, true)? } else { Event::open(&name, EVENT_MODIFY_STATE | SYNCHRONIZATION_SYNCHRONIZE)? });
        }
        // SAFETY: `mapping` is owned here; the view is unmapped in Drop.
        let view = unsafe { MapViewOfFile(mapping.as_raw_handle(), FILE_MAP_READ | FILE_MAP_WRITE, 0, 0, size_of::<Reply>()) };
        if view.Value.is_null() {
            return Err(io::Error::last_os_error());
        }
        let attached = events.pop().unwrap();
        let stop = events.pop().unwrap();
        let ready = events.pop().unwrap();
        Ok(Self { name: name.into(), ready, stop, attached, view, _mapping: mapping, attached_once: Cell::new(false) })
    }

    pub fn reply(&self, handles: [usize; 3], error: Option<&str>) {
        let mut reply = Reply { handles, length: 0, error: [0; 512] };
        if let Some(error) = error {
            let bytes = error.as_bytes();
            let size = bytes.len().min(reply.error.len());
            reply.error[..size].copy_from_slice(&bytes[..size]);
            reply.length = size as u32;
        }
        // SAFETY: the page-aligned view spans a Reply and lives as long as `self`. The helper is the
        // only writer; the parent reads after Ready or process exit.
        unsafe {
            self.view.Value.cast::<Reply>().write_volatile(reply);
        }
        fence(Ordering::SeqCst);
    }

    pub fn signal_ready(&self) -> io::Result<()> {
        self.ready.set()
    }

    pub fn wait_ready(&self, process: BorrowedHandle<'_>) -> io::Result<()> {
        // SAFETY: the array holds two handles borrowed live for the whole wait.
        match unsafe { WaitForMultipleObjects(2, [self.ready.handle().as_raw_handle(), process.as_raw_handle()].as_ptr(), 0, 30_000) } {
            WAIT_OBJECT_0 => Ok(()),
            WAIT_TIMEOUT => Err(io::Error::new(io::ErrorKind::TimedOut, "helper startup")),
            n if n == WAIT_OBJECT_0 + 1 => Err(io::Error::other("helper exited before becoming ready")),
            _ => Err(io::Error::last_os_error()),
        }
    }

    /// The handle values the helper reported, or the error it sent instead.
    pub fn result(&self) -> io::Result<[usize; 3]> {
        fence(Ordering::SeqCst);
        // SAFETY: the page-aligned view spans a Reply, which any bytes form. Ready or process
        // termination synchronizes the one writer with this reader.
        let reply = unsafe { self.view.Value.cast::<Reply>().read_volatile() };
        if reply.length as usize > reply.error.len() {
            return Err(io::Error::new(io::ErrorKind::InvalidData, "invalid helper response"));
        }
        if reply.length != 0 {
            return Err(io::Error::other(String::from_utf8_lossy(&reply.error[..reply.length as usize]).into_owned()));
        }
        Ok(reply.handles)
    }

    /// Tells the helper the parent takes over, then owns the handles it duplicated here. Once only.
    ///
    /// # Safety
    /// The reply must come from a peer that duplicated each value into this process for this
    /// channel alone and revokes them only until Attached is set, as the gamepad helper does.
    /// Values it did not create here would become owners of unrelated handles.
    pub unsafe fn attach(&self) -> io::Result<[io::Result<OwnedHandle>; 3]> {
        let values = self.result()?;
        if self.attached_once.replace(true) {
            return Err(io::Error::other("helper handles were already taken"));
        }
        self.attached.set()?;
        Ok(values.map(|h| owned(h as _)))
    }
}

impl Drop for Channel {
    fn drop(&mut self) {
        // SAFETY: the view was mapped in `connect`, and all accesses to it have ended.
        unsafe {
            UnmapViewOfFile(self.view);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sys::open_process;
    use std::os::windows::io::AsHandle;

    #[test]
    fn startup_errors_cross_the_control_channel() {
        let channel = Channel::create().unwrap();
        let name = channel.name.clone();
        let task = std::thread::spawn(move || {
            let peer = Channel::open(&name).unwrap();
            peer.reply([0; 3], Some("device setup failed"));
            peer.signal_ready().unwrap();
        });
        let process = open_process(std::process::id(), PROCESS_SYNCHRONIZE).unwrap();
        channel.wait_ready(process.as_handle()).unwrap();
        task.join().unwrap();
        assert_eq!(channel.result().unwrap_err().to_string(), "device setup failed");
    }

    #[test]
    fn control_channel_transfers_handle_values() {
        let channel = Channel::create().unwrap();
        let peer = Channel::open(&channel.name).unwrap();
        peer.reply([123, 456, 789], None);
        peer.signal_ready().unwrap();
        let process = open_process(std::process::id(), PROCESS_SYNCHRONIZE).unwrap();
        channel.wait_ready(process.as_handle()).unwrap();
        assert_eq!(channel.result().unwrap(), [123, 456, 789]);
    }
}
