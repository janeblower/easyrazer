//! Certificates in the local machine's stores.

use super::wide;
use std::ffi::c_void;
use std::io;
use std::ptr::{null, null_mut};
use windows_sys::Win32::Foundation::NTE_BAD_KEYSET;
use windows_sys::Win32::Security::Cryptography::*;

struct MachineStore(HCERTSTORE);

impl MachineStore {
    fn open(name: &str) -> io::Result<Self> {
        let name = wide(name);
        // SAFETY: `name` is NUL-terminated and read during the call only.
        let store = unsafe {
            CertOpenStore(
                CERT_STORE_PROV_SYSTEM_W, 0, 0,
                CERT_SYSTEM_STORE_LOCAL_MACHINE_ID << CERT_SYSTEM_STORE_LOCATION_SHIFT,
                name.as_ptr().cast::<c_void>(),
            )
        };
        if store.is_null() { Err(io::Error::last_os_error()) } else { Ok(Self(store)) }
    }
}

impl Drop for MachineStore {
    fn drop(&mut self) {
        // SAFETY: the store was opened in `open` and is closed only here.
        unsafe { CertCloseStore(self.0, 0) };
    }
}

fn check(ok: windows_sys::core::BOOL) -> io::Result<()> {
    if ok == 0 { Err(io::Error::last_os_error()) } else { Ok(()) }
}

fn hresult(code: windows_sys::core::HRESULT) -> io::Result<()> {
    if code == 0 { Ok(()) } else { Err(io::Error::from_raw_os_error(code)) }
}

/// Removes the certificates whose common name is `name` from a machine store, and with
/// `delete_key` their private keys too.
pub fn remove_machine_certificates(store: &str, name: &str, delete_key: bool) -> io::Result<()> {
    let store = MachineStore::open(store)?;
    let subject = wide(name);
    let mut found: *const CERT_CONTEXT = null();
    loop {
        // SAFETY: `found` is null or the previous result, which the call frees.
        found = unsafe {
            CertFindCertificateInStore(store.0, X509_ASN_ENCODING, 0, CERT_FIND_SUBJECT_STR_W, subject.as_ptr().cast(), found)
        };
        if found.is_null() { return Ok(()); }
        // The search matches substrings of the subject.
        if common_name(found) != name { continue; }
        if delete_key { remove_key(found)?; }
        // SAFETY: `found` is valid; the duplicate keeps it so for the next search step.
        let duplicate = unsafe { CertDuplicateCertificateContext(found) };
        // SAFETY: the call takes and frees `duplicate`.
        check(unsafe { CertDeleteCertificateFromStore(duplicate) })?;
    }
}

fn common_name(cert: *const CERT_CONTEXT) -> String {
    let mut buffer = [0u16; 256];
    // SAFETY: `cert` is valid and `buffer`'s length is passed.
    let len = unsafe { CertGetNameStringW(cert, CERT_NAME_SIMPLE_DISPLAY_TYPE, 0, null(), buffer.as_mut_ptr(), buffer.len() as u32) };
    String::from_utf16_lossy(&buffer[..len.saturating_sub(1) as usize])
}

/// Deletes the certificate's private key; one already gone counts as deleted.
fn remove_key(cert: *const CERT_CONTEXT) -> io::Result<()> {
    let mut size = 0u32;
    // SAFETY: `cert` is valid; a null buffer asks for the size.
    if unsafe { CertGetCertificateContextProperty(cert, CERT_KEY_PROV_INFO_PROP_ID, null_mut(), &mut size) } == 0 {
        return Ok(());
    }
    let mut buffer = vec![0u64; (size as usize).div_ceil(8)];
    // SAFETY: `buffer` is aligned for the structure and holds `size` bytes.
    check(unsafe { CertGetCertificateContextProperty(cert, CERT_KEY_PROV_INFO_PROP_ID, buffer.as_mut_ptr().cast(), &mut size) })?;
    // SAFETY: the property is a CRYPT_KEY_PROV_INFO whose strings point into `buffer`.
    let info = unsafe { &*buffer.as_ptr().cast::<CRYPT_KEY_PROV_INFO>() };
    let result = if info.dwProvType == 0 { remove_cng_key(info) } else { remove_legacy_key(info) };
    match result {
        Err(e) if e.raw_os_error() == Some(NTE_BAD_KEYSET) => Ok(()),
        other => other,
    }
}

fn remove_cng_key(info: &CRYPT_KEY_PROV_INFO) -> io::Result<()> {
    let mut provider = 0;
    // SAFETY: the provider name is NUL-terminated or null for the default provider.
    hresult(unsafe { NCryptOpenStorageProvider(&mut provider, info.pwszProvName, 0) })?;
    let mut key = 0;
    let flags = (info.dwFlags & NCRYPT_MACHINE_KEY_FLAG) | NCRYPT_SILENT_FLAG;
    // SAFETY: `provider` is open and the container name is NUL-terminated.
    let opened = hresult(unsafe { NCryptOpenKey(provider, &mut key, info.pwszContainerName, info.dwKeySpec, flags) });
    let result = opened.and_then(|()| {
        // SAFETY: `key` is open; the call frees it when it succeeds.
        let deleted = hresult(unsafe { NCryptDeleteKey(key, NCRYPT_SILENT_FLAG) });
        if deleted.is_err() {
            // SAFETY: the failed deletion left `key` open.
            unsafe { NCryptFreeObject(key) };
        }
        deleted
    });
    // SAFETY: `provider` is open and freed only here.
    unsafe { NCryptFreeObject(provider) };
    result
}

fn remove_legacy_key(info: &CRYPT_KEY_PROV_INFO) -> io::Result<()> {
    let mut provider = 0;
    let flags = CRYPT_DELETEKEYSET | (info.dwFlags & CRYPT_MACHINE_KEYSET);
    // SAFETY: the names are NUL-terminated; deleting a key set returns no handle to free.
    check(unsafe { CryptAcquireContextW(&mut provider, info.pwszContainerName, info.pwszProvName, info.dwProvType, flags) })
}
