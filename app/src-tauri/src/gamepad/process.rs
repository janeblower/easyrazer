use super::{TIMEOUT, ipc};
use crate::autostart::wide;
use hidmaestro_client::Controller;
use std::io::{self, Read, Write};
use std::net::{Ipv4Addr, TcpListener, TcpStream};
use std::ptr::null_mut;
use std::time::{Duration, Instant};
use windows_sys::Win32::Foundation::{CloseHandle, HANDLE, WAIT_TIMEOUT};
use windows_sys::Win32::Security::Cryptography::{BCRYPT_USE_SYSTEM_PREFERRED_RNG, BCryptGenRandom};
use windows_sys::Win32::System::Threading::WaitForSingleObject;
use windows_sys::Win32::UI::Shell::{SEE_MASK_NOCLOSEPROCESS, SHELLEXECUTEINFOW, ShellExecuteExW};
use windows_sys::Win32::UI::WindowsAndMessaging::SW_HIDE;

pub(super) struct Process(HANDLE);

impl Process {
    pub(super) fn wait(&self) {
        if unsafe { WaitForSingleObject(self.0, 5000) } == WAIT_TIMEOUT {
            eprintln!("gamepad helper has not finished cleanup");
        }
    }
}

impl Drop for Process {
    fn drop(&mut self) {
        unsafe {
            CloseHandle(self.0);
        }
    }
}

pub(super) fn launch() -> Result<(TcpStream, Process), String> {
    launch_inner().map_err(|e| format!("gamepad helper: {e}"))
}

fn launch_inner() -> io::Result<(TcpStream, Process)> {
    let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0))?;
    listener.set_nonblocking(true)?;
    let mut token = [0u8; 32];
    if unsafe { BCryptGenRandom(null_mut(), token.as_mut_ptr(), token.len() as u32, BCRYPT_USE_SYSTEM_PREFERRED_RNG) } < 0 {
        return Err(io::Error::other("cannot generate IPC token"));
    }
    let secret: String = token.iter().map(|b| format!("{b:02x}")).collect();
    let exe = std::env::current_exe()?;
    use std::os::windows::ffi::OsStrExt;
    let exe: Vec<u16> = exe.as_os_str().encode_wide().chain([0]).collect();
    let verb = wide("runas");
    let args = wide(&format!("--gamepad-helper {} {secret}", listener.local_addr()?.port()));
    let mut info = SHELLEXECUTEINFOW {
        cbSize: size_of::<SHELLEXECUTEINFOW>() as u32,
        fMask: SEE_MASK_NOCLOSEPROCESS,
        lpVerb: verb.as_ptr(),
        lpFile: exe.as_ptr(),
        lpParameters: args.as_ptr(),
        nShow: SW_HIDE,
        ..unsafe { std::mem::zeroed() }
    };
    if unsafe { ShellExecuteExW(&mut info) } == 0 {
        return Err(io::Error::last_os_error());
    }
    if info.hProcess.is_null() {
        return Err(io::Error::other("helper process handle missing"));
    }
    let process = Process(info.hProcess);
    let until = Instant::now() + Duration::from_secs(30);
    loop {
        if Instant::now() >= until {
            return Err(io::Error::new(io::ErrorKind::TimedOut, "helper did not connect"));
        }
        if unsafe { WaitForSingleObject(process.0, 0) } != WAIT_TIMEOUT {
            return Err(io::Error::other("helper exited before connecting"));
        }
        match listener.accept() {
            Ok((mut stream, _)) => {
                ipc::configure_stream(&stream)?;
                let mut received = [0u8; 32];
                if stream.read_exact(&mut received).is_err() || received != token {
                    continue;
                }
                stream.write_all(&[0])?;
                // Device enumeration can take several seconds; frame traffic has a shorter deadline.
                stream.set_read_timeout(Some(Duration::from_secs(30)))?;
                let mut size = [0; 2];
                stream.read_exact(&mut size).map_err(|e| io::Error::new(e.kind(), format!("helper startup response: {e}")))?;
                let size = usize::from(u16::from_le_bytes(size));
                if size > 512 {
                    return Err(io::Error::new(io::ErrorKind::InvalidData, "oversized helper error"));
                }
                if size != 0 {
                    let mut message = vec![0; size];
                    stream.read_exact(&mut message)?;
                    return Err(io::Error::other(String::from_utf8_lossy(&message).into_owned()));
                }
                stream.set_read_timeout(Some(TIMEOUT))?;
                return Ok((stream, process));
            }
            Err(e) if e.kind() == io::ErrorKind::WouldBlock => std::thread::sleep(Duration::from_millis(10)),
            Err(e) => return Err(e),
        }
    }
}

fn endpoint(args: &[String]) -> io::Result<(u16, [u8; 32])> {
    let [port, secret] = args else {
        return Err(io::Error::new(io::ErrorKind::InvalidInput, "expected port and IPC token"));
    };
    let port = port.parse::<u16>().ok().filter(|p| *p != 0).ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "invalid port"))?;
    if secret.len() != 64 || !secret.is_ascii() {
        return Err(io::Error::new(io::ErrorKind::InvalidInput, "invalid IPC token"));
    }
    let mut token = [0; 32];
    for (i, byte) in token.iter_mut().enumerate() {
        *byte = u8::from_str_radix(&secret[i * 2..i * 2 + 2], 16)
            .map_err(|_| io::Error::new(io::ErrorKind::InvalidInput, "invalid IPC token"))?;
    }
    Ok((port, token))
}

pub(super) fn helper(args: &[String]) -> io::Result<()> {
    let (port, token) = endpoint(args)?;
    let mut stream = TcpStream::connect_timeout(&(Ipv4Addr::LOCALHOST, port).into(), TIMEOUT)?;
    ipc::configure_stream(&stream)?;
    stream.write_all(&token)?;
    let mut ack = [255];
    stream.read_exact(&mut ack)?;
    if ack != [0] {
        return Err(io::Error::new(io::ErrorKind::PermissionDenied, "IPC handshake rejected"));
    }
    let mut controller = match Controller::create() {
        Ok(c) => c,
        Err(e) => {
            let message = e.to_string();
            let message = &message.as_bytes()[..message.len().min(512)];
            stream.write_all(&(message.len() as u16).to_le_bytes())?;
            stream.write_all(message)?;
            return Err(e);
        }
    };
    let result = (|| {
        stream.write_all(&[0, 0])?;
        ipc::serve(&mut stream, |state| controller.submit(&state))
    })();
    let cleanup = controller.close();
    if let Err(e) = &cleanup {
        eprintln!("gamepad cleanup: {e}");
    }
    result.and(cleanup)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn malformed_endpoint_is_rejected_before_connecting() {
        let token = "ab".repeat(32);
        for args in [
            vec![],
            vec!["1234".into()],
            vec!["0".into(), token.clone()],
            vec!["65536".into(), token.clone()],
            vec!["1234".into(), "gg".repeat(32)],
            vec!["1234".into(), "é".repeat(32)],
            vec!["1234".into(), "ab".repeat(31)],
        ] {
            assert_eq!(endpoint(&args).unwrap_err().kind(), io::ErrorKind::InvalidInput);
        }
        assert_eq!(endpoint(&["1234".into(), token]).unwrap(), (1234, [0xab; 32]));
    }
}
