//! Downloads over HTTPS and their checksums.

use super::wide;
use std::ffi::c_void;
use std::io;
use std::ptr::{null, null_mut};
use windows_sys::Win32::Networking::WinHttp::*;
use windows_sys::Win32::Security::Cryptography::{BCRYPT_SHA256_ALG_HANDLE, BCryptHash};

const TIMEOUT_MS: i32 = 60_000;
const STATUS_OK: u32 = 200;

struct Internet(*mut c_void);

impl Internet {
    fn new(handle: *mut c_void) -> io::Result<Self> {
        if handle.is_null() { Err(io::Error::last_os_error()) } else { Ok(Self(handle)) }
    }
}

impl Drop for Internet {
    fn drop(&mut self) {
        // SAFETY: the handle came from WinHTTP and is closed only here.
        unsafe { WinHttpCloseHandle(self.0) };
    }
}

fn check(ok: windows_sys::core::BOOL) -> io::Result<()> {
    if ok == 0 { Err(io::Error::last_os_error()) } else { Ok(()) }
}

/// Fetches `https://{host}{path}`, following redirects to other HTTPS hosts, and fails
/// past `limit` bytes.
pub fn download(host: &str, path: &str, limit: usize) -> io::Result<Vec<u8>> {
    let request = send(host, path)?;
    let mut status = 0u32;
    let mut size = size_of::<u32>() as u32;
    // SAFETY: `request` is open; `status` and `size` are writable for the call.
    check(unsafe {
        WinHttpQueryHeaders(
            request.request.0, WINHTTP_QUERY_STATUS_CODE | WINHTTP_QUERY_FLAG_NUMBER, null(),
            (&raw mut status).cast(), &mut size, null_mut(),
        )
    })?;
    if status != STATUS_OK {
        return Err(io::Error::other(format!("HTTP {status}")));
    }
    let mut body = Vec::new();
    let mut buffer = vec![0u8; 64 * 1024];
    loop {
        let mut read = 0u32;
        // SAFETY: `request` is open and `buffer` is writable for its full length.
        check(unsafe { WinHttpReadData(request.request.0, buffer.as_mut_ptr().cast(), buffer.len() as u32, &mut read) })?;
        if read == 0 { return Ok(body); }
        body.extend_from_slice(&buffer[..read as usize]);
        if body.len() > limit {
            return Err(io::Error::other("download is larger than expected"));
        }
    }
}

/// Fields drop in order: the request before its connection and session.
struct Request {
    request: Internet,
    _connection: Internet,
    _session: Internet,
}

/// A GET request whose response headers have arrived.
fn send(host: &str, path: &str) -> io::Result<Request> {
    let agent = wide("EasyRazer");
    let (host, path, verb) = (wide(host), wide(path), wide("GET"));
    // SAFETY: `agent` is NUL-terminated; the other strings may be null.
    let session = Internet::new(unsafe { WinHttpOpen(agent.as_ptr(), WINHTTP_ACCESS_TYPE_AUTOMATIC_PROXY, null(), null(), 0) })?;
    // SAFETY: `session` is open.
    check(unsafe { WinHttpSetTimeouts(session.0, TIMEOUT_MS, TIMEOUT_MS, TIMEOUT_MS, TIMEOUT_MS) })?;
    // SAFETY: `session` is open and `host` is NUL-terminated.
    let connection = Internet::new(unsafe { WinHttpConnect(session.0, host.as_ptr(), INTERNET_DEFAULT_HTTPS_PORT, 0) })?;
    // SAFETY: `connection` is open; `verb` and `path` are NUL-terminated, the rest may be null.
    let request = Internet::new(unsafe {
        WinHttpOpenRequest(connection.0, verb.as_ptr(), path.as_ptr(), null(), null(), null(), WINHTTP_FLAG_SECURE)
    })?;
    // SAFETY: `request` is open and sends no headers or body.
    check(unsafe { WinHttpSendRequest(request.0, null(), 0, null(), 0, 0, 0) })?;
    // SAFETY: `request` was just sent.
    check(unsafe { WinHttpReceiveResponse(request.0, null_mut()) })?;
    Ok(Request { request, _connection: connection, _session: session })
}

pub fn sha256(data: &[u8]) -> io::Result<[u8; 32]> {
    let size = u32::try_from(data.len()).map_err(|_| io::Error::other("too much data to hash"))?;
    let mut digest = [0u8; 32];
    // SAFETY: `data` and `digest` are live for the call with the lengths passed.
    let status = unsafe { BCryptHash(BCRYPT_SHA256_ALG_HANDLE, null(), 0, data.as_ptr(), size, digest.as_mut_ptr(), 32) };
    if status != 0 {
        return Err(io::Error::other(format!("SHA-256 failed: NTSTATUS {status:#010x}")));
    }
    Ok(digest)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sha256_matches_the_standard_vector() {
        let hex: String = sha256(b"abc").unwrap().iter().map(|b| format!("{b:02x}")).collect();
        assert_eq!(hex, "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad");
    }
}
