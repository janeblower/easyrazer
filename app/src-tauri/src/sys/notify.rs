//! System notifications: HID interfaces coming and going, and resume from sleep.

use std::sync::OnceLock;
use windows_sys::Win32::Devices::DeviceAndDriverInstallation::{
    CM_NOTIFY_ACTION, CM_NOTIFY_EVENT_DATA, CM_NOTIFY_FILTER, CM_NOTIFY_FILTER_0, CM_NOTIFY_FILTER_0_0, CM_NOTIFY_FILTER_TYPE_DEVICEINTERFACE,
    CM_Register_Notification, HCMNOTIFICATION,
};
use windows_sys::Win32::System::Power::{DEVICE_NOTIFY_SUBSCRIBE_PARAMETERS, PowerRegisterSuspendResumeNotification};
use windows_sys::Win32::UI::WindowsAndMessaging::{DEVICE_NOTIFY_CALLBACK, PBT_APMRESUMEAUTOMATIC};
use windows_sys::core::GUID;

const GUID_DEVINTERFACE_HID: GUID = GUID::from_u128(0x4d1e55b2_f16f_11cf_88cb_001111000030);

static ON_HID: OnceLock<fn()> = OnceLock::new();
static ON_RESUME: OnceLock<fn()> = OnceLock::new();

unsafe extern "system" fn interface(_: HCMNOTIFICATION, _: *const core::ffi::c_void, _: CM_NOTIFY_ACTION, _: *const CM_NOTIFY_EVENT_DATA, _: u32) -> u32 {
    if let Some(f) = ON_HID.get() {
        f();
    }
    0
}

unsafe extern "system" fn power(_: *const core::ffi::c_void, kind: u32, _: *const core::ffi::c_void) -> u32 {
    if kind == PBT_APMRESUMEAUTOMATIC
        && let Some(f) = ON_RESUME.get()
    {
        f();
    }
    0
}

/// Calls `on_hid` when a HID interface arrives or leaves, and `on_resume` after the
/// system resumes, for as long as the process lives. Only the first call registers.
pub fn notify(on_hid: fn(), on_resume: fn()) {
    if ON_HID.set(on_hid).is_err() || ON_RESUME.set(on_resume).is_err() {
        return;
    }
    let filter = CM_NOTIFY_FILTER {
        cbSize: size_of::<CM_NOTIFY_FILTER>() as u32,
        Flags: 0,
        FilterType: CM_NOTIFY_FILTER_TYPE_DEVICEINTERFACE,
        Reserved: 0,
        u: CM_NOTIFY_FILTER_0 { DeviceInterface: CM_NOTIFY_FILTER_0_0 { ClassGuid: GUID_DEVINTERFACE_HID } },
    };
    let mut cm: HCMNOTIFICATION = std::ptr::null_mut();
    unsafe { CM_Register_Notification(&filter, std::ptr::null(), Some(interface), &mut cm) };
    // Windows keeps the parameters for the life of the subscription, which is the process.
    let params = Box::leak(Box::new(DEVICE_NOTIFY_SUBSCRIBE_PARAMETERS { Callback: Some(power), Context: std::ptr::null_mut() }));
    let mut power_handle = std::ptr::null_mut();
    unsafe { PowerRegisterSuspendResumeNotification(DEVICE_NOTIFY_CALLBACK, (params as *mut DEVICE_NOTIFY_SUBSCRIBE_PARAMETERS).cast(), &mut power_handle) };
}
