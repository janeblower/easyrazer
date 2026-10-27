//! Message-based control channel for the elevated gamepad helper.

use super::handle::owned;
use super::{wait_any, wide};
use serde::{Deserialize, Serialize};
use std::io;
use std::os::windows::io::{AsHandle, AsRawHandle, BorrowedHandle, OwnedHandle};
use std::ptr;
use windows_sys::Win32::Foundation::{
    ERROR_IO_PENDING, ERROR_MORE_DATA, ERROR_PIPE_CONNECTED, LocalFree,
};
use windows_sys::Win32::Security::Authorization::{
    ConvertStringSecurityDescriptorToSecurityDescriptorW, SDDL_REVISION_1,
};
use windows_sys::Win32::Security::SECURITY_ATTRIBUTES;
use windows_sys::Win32::Storage::FileSystem::*;
use windows_sys::Win32::System::IO::{CancelIoEx, GetOverlappedResult, OVERLAPPED};
use windows_sys::Win32::System::Pipes::*;
use windows_sys::Win32::System::Threading::{CreateEventW, GetProcessId};

const MAX_MESSAGE: usize = 4096;
const TIMEOUT: u32 = 30_000;

#[derive(Debug, PartialEq, Serialize, Deserialize)]
pub enum ControlMessage {
    Connected,
    Finished(Option<String>),
}

impl ControlMessage {
    pub fn finished(error: Option<&str>) -> Self {
        Self::Finished(error.map(|s| {
            let mut end = s.len().min(512);
            while !s.is_char_boundary(end) {
                end -= 1;
            }
            s[..end].to_owned()
        }))
    }
}

pub struct Channel {
    pub name: String,
    pipe: OwnedHandle,
}

enum Operation<'a> {
    Connect,
    Read(&'a mut [u8]),
    Write(&'a [u8]),
}

impl Channel {
    pub fn create() -> io::Result<Self> {
        let mut token = [0u8; 32];
        getrandom::fill(&mut token).map_err(|_| io::Error::other("cannot generate IPC token"))?;
        let secret: String = token.iter().map(|b| format!("{b:02x}")).collect();
        let name = format!(r"\\.\pipe\EasyRazerGamepad-{}-{secret}", std::process::id());
        let mut descriptor = ptr::null_mut();
        // Limit access to the owner, administrators and SYSTEM, rather than the pipe default ACL.
        let sddl = wide("D:P(A;;GA;;;SY)(A;;GA;;;BA)(A;;GA;;;OW)");
        // SAFETY: the input is NUL-terminated; the output pointer is writable.
        if unsafe {
            ConvertStringSecurityDescriptorToSecurityDescriptorW(
                sddl.as_ptr(),
                SDDL_REVISION_1,
                &mut descriptor,
                ptr::null_mut(),
            )
        } == 0
        {
            return Err(io::Error::last_os_error());
        }
        let attributes = SECURITY_ATTRIBUTES {
            nLength: std::mem::size_of::<SECURITY_ATTRIBUTES>() as u32,
            lpSecurityDescriptor: descriptor,
            bInheritHandle: 0,
        };
        // SAFETY: the descriptor and NUL-terminated name stay alive through creation.
        let pipe = owned(unsafe {
            CreateNamedPipeW(
                wide(&name).as_ptr(),
                PIPE_ACCESS_DUPLEX | FILE_FLAG_FIRST_PIPE_INSTANCE | FILE_FLAG_OVERLAPPED,
                PIPE_TYPE_MESSAGE | PIPE_READMODE_MESSAGE | PIPE_REJECT_REMOTE_CLIENTS,
                1,
                16384,
                16384,
                0,
                &attributes,
            )
        });
        // SAFETY: the converter allocated this descriptor with LocalAlloc; creation copied it.
        unsafe { LocalFree(descriptor) };
        Ok(Self { name, pipe: pipe? })
    }

    pub fn open(name: &str, parent: u32) -> io::Result<Self> {
        // Identification prevents the unprivileged pipe server from impersonating the helper.
        // SAFETY: the name is NUL-terminated and lives through the call; no handle is inherited.
        let pipe = owned(unsafe {
            CreateFileW(
                wide(name).as_ptr(),
                FILE_READ_DATA | FILE_WRITE_DATA | FILE_WRITE_ATTRIBUTES,
                0,
                ptr::null(),
                OPEN_EXISTING,
                FILE_FLAG_OVERLAPPED | SECURITY_SQOS_PRESENT | SECURITY_IDENTIFICATION,
                ptr::null_mut(),
            )
        })?;
        let mut pid = 0;
        // SAFETY: the pipe is open; pid is writable.
        if unsafe { GetNamedPipeServerProcessId(pipe.as_raw_handle(), &mut pid) } == 0 {
            return Err(io::Error::last_os_error());
        }
        if pid != parent {
            return Err(io::Error::new(
                io::ErrorKind::PermissionDenied,
                "unexpected control server",
            ));
        }
        let mode = PIPE_READMODE_MESSAGE;
        // SAFETY: pipe is open and mode lives through the call; optional parameters are null.
        if unsafe { SetNamedPipeHandleState(pipe.as_raw_handle(), &mode, ptr::null(), ptr::null()) }
            == 0
        {
            return Err(io::Error::last_os_error());
        }
        Ok(Self { name: name.into(), pipe })
    }

    pub fn connect(&mut self, process: BorrowedHandle<'_>) -> io::Result<()> {
        self.transfer(Operation::Connect, Some(process), TIMEOUT)?;
        let mut pid = 0;
        // SAFETY: the borrowed process is live for the call.
        let expected = unsafe { GetProcessId(process.as_raw_handle()) };
        // SAFETY: the connected pipe is live; pid is writable.
        if unsafe { GetNamedPipeClientProcessId(self.pipe.as_raw_handle(), &mut pid) } == 0 {
            return Err(io::Error::last_os_error());
        }
        if expected == 0 || pid != expected {
            return Err(io::Error::new(
                io::ErrorKind::PermissionDenied,
                "unexpected control client",
            ));
        }
        self.send(&ControlMessage::Connected, TIMEOUT)
    }

    pub fn wait_connected(&mut self, parent: BorrowedHandle<'_>) -> io::Result<()> {
        match self.receive(parent, TIMEOUT)? {
            ControlMessage::Connected => Ok(()),
            _ => Err(invalid_message()),
        }
    }

    fn transfer(
        &mut self,
        mut operation: Operation<'_>,
        peer: Option<BorrowedHandle<'_>>,
        timeout: u32,
    ) -> io::Result<usize> {
        // SAFETY: null name/attributes request an unnamed manual-reset event.
        let event = owned(unsafe { CreateEventW(ptr::null(), 1, 0, ptr::null()) })?;
        let mut overlapped = OVERLAPPED {
            hEvent: event.as_raw_handle(),
            ..Default::default()
        };
        let mut bytes = 0;
        let pipe = self.pipe.as_raw_handle();
        let connecting = matches!(operation, Operation::Connect);
        let ok = match operation {
            // SAFETY: pipe is overlapped; event and OVERLAPPED live until completion or cancellation.
            Operation::Connect => unsafe { ConnectNamedPipe(pipe, &mut overlapped) },
            // SAFETY: the borrowed buffer and OVERLAPPED live until completion or cancellation.
            Operation::Read(ref mut buf) => unsafe {
                ReadFile(
                    pipe,
                    buf.as_mut_ptr(),
                    buf.len() as u32,
                    &mut bytes,
                    &mut overlapped,
                )
            },
            // SAFETY: the borrowed buffer and OVERLAPPED live until completion or cancellation.
            Operation::Write(buf) => unsafe {
                WriteFile(
                    pipe,
                    buf.as_ptr(),
                    buf.len() as u32,
                    &mut bytes,
                    &mut overlapped,
                )
            },
        };
        if ok != 0 {
            return Ok(bytes as usize);
        }
        let error = io::Error::last_os_error();
        if connecting && error.raw_os_error() == Some(ERROR_PIPE_CONNECTED as i32) {
            return Ok(0);
        }
        if error.raw_os_error() != Some(ERROR_IO_PENDING as i32) {
            return Err(error);
        }
        let mut handles = vec![event.as_handle()];
        handles.extend(peer);
        let waited = wait_any(&handles, timeout);
        if !matches!(waited, Ok(Some(0))) {
            // SAFETY: pipe is live and this OVERLAPPED identifies only our pending operation.
            unsafe { CancelIoEx(pipe, ptr::addr_of!(overlapped)) };
            // SAFETY: drain cancellation before releasing the operation's storage, even if it raced completion.
            unsafe { GetOverlappedResult(pipe, ptr::addr_of!(overlapped), &mut bytes, 1) };
            return Err(match waited {
                Err(e) => e,
                Ok(None) => io::Error::new(io::ErrorKind::TimedOut, "control channel timeout"),
                Ok(Some(_)) => io::Error::new(io::ErrorKind::BrokenPipe, "control peer exited"),
            });
        }
        // SAFETY: the completion event is signaled and all operation storage is still live.
        if unsafe { GetOverlappedResult(pipe, ptr::addr_of!(overlapped), &mut bytes, 0) } == 0 {
            return Err(io::Error::last_os_error());
        }
        Ok(bytes as usize)
    }

    pub fn send(&mut self, message: &ControlMessage, timeout: u32) -> io::Result<()> {
        let bytes = serde_json::to_vec(message)?;
        if bytes.len() > MAX_MESSAGE {
            return Err(invalid_message());
        }
        let written = self.transfer(Operation::Write(&bytes), None, timeout)?;
        if written != bytes.len() {
            return Err(io::Error::new(
                io::ErrorKind::WriteZero,
                "incomplete control message",
            ));
        }
        Ok(())
    }

    pub fn receive(
        &mut self,
        peer: BorrowedHandle<'_>,
        timeout: u32,
    ) -> io::Result<ControlMessage> {
        let mut bytes = [0u8; MAX_MESSAGE];
        let length = self
            .transfer(Operation::Read(&mut bytes), Some(peer), timeout)
            .map_err(|e| {
                if e.raw_os_error() == Some(ERROR_MORE_DATA as i32) {
                    invalid_message()
                } else {
                    e
                }
            })?;
        serde_json::from_slice(&bytes[..length]).map_err(|_| invalid_message())
    }

    pub fn result(&mut self, process: BorrowedHandle<'_>) -> io::Result<()> {
        match self.receive(process, TIMEOUT)? {
            ControlMessage::Finished(None) => Ok(()),
            ControlMessage::Finished(Some(error)) => Err(io::Error::other(error)),
            _ => Err(invalid_message()),
        }
    }
}

fn invalid_message() -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, "invalid control message")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sys::open_process;
    use windows_sys::Win32::System::Threading::{
        PROCESS_QUERY_LIMITED_INFORMATION, PROCESS_SYNCHRONIZE,
    };

    fn pair() -> (Channel, Channel, OwnedHandle) {
        let mut server = Channel::create().unwrap();
        let mut peer = Channel::open(&server.name, std::process::id()).unwrap();
        let process = open_process(
            std::process::id(),
            PROCESS_SYNCHRONIZE | PROCESS_QUERY_LIMITED_INFORMATION,
        )
        .unwrap();
        server.connect(process.as_handle()).unwrap();
        peer.wait_connected(process.as_handle()).unwrap();
        (server, peer, process)
    }

    #[test]
    fn helper_errors_cross_the_control_channel() {
        let (mut server, mut peer, process) = pair();
        peer.send(&ControlMessage::finished(Some("install failed")), TIMEOUT).unwrap();
        assert_eq!(
            server.result(process.as_handle()).unwrap_err().to_string(),
            "install failed"
        );
        let (mut server, mut peer, process) = pair();
        peer.send(&ControlMessage::finished(None), TIMEOUT).unwrap();
        server.result(process.as_handle()).unwrap();
    }

    #[test]
    fn malformed_and_oversized_messages_are_rejected() {
        for bytes in [
            b"garbage".to_vec(),
            b"{\"Finished\":[1,2]}".to_vec(),
            vec![b'x'; MAX_MESSAGE + 1],
        ] {
            let (mut server, mut peer, process) = pair();
            peer.transfer(Operation::Write(&bytes), None, TIMEOUT)
                .unwrap();
            assert_eq!(
                server
                    .receive(process.as_handle(), TIMEOUT)
                    .unwrap_err()
                    .kind(),
                io::ErrorKind::InvalidData
            );
        }
    }

    #[test]
    fn timeout_cancels_read_before_reusing_the_channel() {
        let (mut server, mut peer, process) = pair();
        assert_eq!(
            server.receive(process.as_handle(), 0).unwrap_err().kind(),
            io::ErrorKind::TimedOut
        );
        peer.send(&ControlMessage::Finished(None), TIMEOUT).unwrap();
        assert_eq!(
            server.receive(process.as_handle(), TIMEOUT).unwrap(),
            ControlMessage::Finished(None)
        );
    }

    #[test]
    fn disconnected_peer_is_an_error() {
        let (mut server, peer, process) = pair();
        drop(peer);
        assert!(server.receive(process.as_handle(), TIMEOUT).is_err());
    }

    #[test]
    fn unexpected_server_is_rejected() {
        let server = Channel::create().unwrap();
        assert!(
            matches!(Channel::open(&server.name, 0), Err(e) if e.kind() == io::ErrorKind::PermissionDenied)
        );
    }

    #[test]
    fn error_text_preserves_utf8_boundaries() {
        let message = ControlMessage::finished(Some(&"я".repeat(300)));
        assert_eq!(message, ControlMessage::Finished(Some("я".repeat(256))));
    }

    #[test]
    fn escaped_error_text_fits_the_message_limit() {
        let (mut server, mut peer, process) = pair();
        let message = ControlMessage::finished(Some(&"\0".repeat(512)));
        peer.send(&message, TIMEOUT).unwrap();
        assert_eq!(
            server.receive(process.as_handle(), TIMEOUT).unwrap(),
            message
        );
    }

    #[test]
    fn wrong_message_is_not_a_result() {
        let (mut server, mut peer, process) = pair();
        peer.send(&ControlMessage::Connected, TIMEOUT).unwrap();
        assert_eq!(
            server.result(process.as_handle()).unwrap_err().kind(),
            io::ErrorKind::InvalidData
        );
    }

    #[test]
    fn timeout_cancels_a_blocked_write() {
        let (mut server, mut peer, _) = pair();
        let bytes = [b'x'; MAX_MESSAGE];
        let mut blocked = false;
        for _ in 0..64 {
            match peer.transfer(Operation::Write(&bytes), None, 0) {
                Ok(_) => {}
                Err(e) => {
                    assert_eq!(e.kind(), io::ErrorKind::TimedOut);
                    blocked = true;
                    break;
                }
            }
        }
        assert!(blocked, "pipe buffer never filled");
        while server
            .transfer(Operation::Read(&mut [0; MAX_MESSAGE]), None, 0)
            .is_ok()
        {}
        peer.send(&ControlMessage::Finished(None), TIMEOUT).unwrap();
        let mut bytes = [0; MAX_MESSAGE];
        let count = server
            .transfer(Operation::Read(&mut bytes), None, TIMEOUT)
            .unwrap();
        assert_eq!(
            serde_json::from_slice::<ControlMessage>(&bytes[..count]).unwrap(),
            ControlMessage::Finished(None)
        );
    }

    #[test]
    fn timeout_cancels_connect_before_retry() {
        let mut server = Channel::create().unwrap();
        assert_eq!(
            server
                .transfer(Operation::Connect, None, 0)
                .unwrap_err()
                .kind(),
            io::ErrorKind::TimedOut
        );
        let mut peer = Channel::open(&server.name, std::process::id()).unwrap();
        let process = open_process(
            std::process::id(),
            PROCESS_SYNCHRONIZE | PROCESS_QUERY_LIMITED_INFORMATION,
        )
        .unwrap();
        server.connect(process.as_handle()).unwrap();
        peer.wait_connected(process.as_handle()).unwrap();
    }

    #[test]
    fn unexpected_client_is_rejected() {
        use std::os::windows::process::CommandExt;
        let mut server = Channel::create().unwrap();
        let _peer = Channel::open(&server.name, std::process::id()).unwrap();
        let mut other = std::process::Command::new("cmd.exe")
            .args(["/c", "exit", "0"])
            .creation_flags(0x08000000)
            .spawn()
            .unwrap();
        let result = server.connect(other.as_handle());
        other.wait().unwrap();
        assert_eq!(result.unwrap_err().kind(), io::ErrorKind::PermissionDenied);
    }
}
