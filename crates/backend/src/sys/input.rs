//! Injected input and the system calls the keyboard engine makes.

use windows_sys::Win32::System::Power::SetSuspendState;
use windows_sys::Win32::System::Shutdown::LockWorkStation;
use windows_sys::Win32::UI::Input::KeyboardAndMouse::{GetAsyncKeyState, INPUT, SendInput};
use windows_sys::Win32::UI::WindowsAndMessaging::{SPI_GETKEYBOARDDELAY, SPI_GETKEYBOARDSPEED, SystemParametersInfoW};

pub fn send_input(inputs: &[INPUT]) {
    if !inputs.is_empty() {
        // SAFETY: `inputs` is a live slice of that many INPUTs of the size passed.
        unsafe { SendInput(inputs.len() as u32, inputs.as_ptr(), size_of::<INPUT>() as i32) };
    }
}

/// Whether the key is down now.
pub fn key_down(vk: u16) -> bool {
    // SAFETY: takes a value only.
    (unsafe { GetAsyncKeyState(i32::from(vk)) }) < 0
}

pub fn lock_workstation() {
    // SAFETY: no arguments.
    unsafe { LockWorkStation() };
}

pub fn suspend() {
    // SAFETY: takes values only.
    unsafe { SetSuspendState(false, false, false) };
}

/// The keyboard delay and speed settings, or the given defaults where unreadable.
pub fn keyboard_repeat(mut delay: u32, mut speed: u32) -> (u32, u32) {
    // SAFETY: SPI_GETKEYBOARDDELAY writes one u32 to `delay`.
    unsafe { SystemParametersInfoW(SPI_GETKEYBOARDDELAY, 0, (&raw mut delay).cast(), 0) };
    // SAFETY: SPI_GETKEYBOARDSPEED writes one u32 to `speed`.
    unsafe { SystemParametersInfoW(SPI_GETKEYBOARDSPEED, 0, (&raw mut speed).cast(), 0) };
    (delay, speed)
}
