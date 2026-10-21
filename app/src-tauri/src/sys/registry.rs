//! Values under HKEY_CURRENT_USER.

use super::wide;
use std::ptr::null_mut;
use windows_sys::Win32::Foundation::ERROR_SUCCESS;
use windows_sys::Win32::System::Registry::{
    HKEY_CURRENT_USER, REG_DWORD, REG_SZ, RRF_RT_REG_DWORD, RRF_RT_REG_SZ, RegDeleteKeyValueW, RegGetValueW, RegSetKeyValueW,
};

fn check(status: u32) -> Result<(), String> {
    if status == ERROR_SUCCESS { Ok(()) } else { Err(format!("registry: {}", std::io::Error::from_raw_os_error(status as i32))) }
}

pub fn has_string(key: &str, value: &str) -> bool {
    let status = unsafe { RegGetValueW(HKEY_CURRENT_USER, wide(key).as_ptr(), wide(value).as_ptr(), RRF_RT_REG_SZ, null_mut(), null_mut(), null_mut()) };
    status == ERROR_SUCCESS
}

pub fn set_string(key: &str, value: &str, data: &str) -> Result<(), String> {
    let data = wide(data);
    check(unsafe { RegSetKeyValueW(HKEY_CURRENT_USER, wide(key).as_ptr(), wide(value).as_ptr(), REG_SZ, data.as_ptr().cast(), (data.len() * 2) as u32) })
}

pub fn delete_value(key: &str, value: &str) -> Result<(), String> {
    check(unsafe { RegDeleteKeyValueW(HKEY_CURRENT_USER, wide(key).as_ptr(), wide(value).as_ptr()) })
}

pub fn dword(key: &str, value: &str) -> Option<u32> {
    let mut data = 0u32;
    let mut size = 4u32;
    let status = unsafe { RegGetValueW(HKEY_CURRENT_USER, wide(key).as_ptr(), wide(value).as_ptr(), RRF_RT_REG_DWORD, null_mut(), (&mut data as *mut u32).cast(), &mut size) };
    (status == ERROR_SUCCESS).then_some(data)
}

pub fn set_dword(key: &str, value: &str, data: u32) -> Result<(), String> {
    check(unsafe { RegSetKeyValueW(HKEY_CURRENT_USER, wide(key).as_ptr(), wide(value).as_ptr(), REG_DWORD, (&data as *const u32).cast(), 4) })
}
