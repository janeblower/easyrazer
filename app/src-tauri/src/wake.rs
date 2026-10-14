//! What wakes the watcher: a HID interface came or went, the system resumed, or something
//! changed that the next step must see. Between them the watcher only checks for Synapse.

use std::sync::OnceLock;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver, Sender};

use windows_sys::Win32::Devices::DeviceAndDriverInstallation::{
    CM_NOTIFY_ACTION, CM_NOTIFY_EVENT_DATA, CM_NOTIFY_FILTER, CM_NOTIFY_FILTER_0, CM_NOTIFY_FILTER_0_0,
    CM_NOTIFY_FILTER_TYPE_DEVICEINTERFACE, CM_Register_Notification, HCMNOTIFICATION,
};
use windows_sys::Win32::System::Power::{DEVICE_NOTIFY_SUBSCRIBE_PARAMETERS, PowerRegisterSuspendResumeNotification};
use windows_sys::Win32::UI::WindowsAndMessaging::{DEVICE_NOTIFY_CALLBACK, PBT_APMRESUMEAUTOMATIC};
use windows_sys::core::GUID;

const GUID_DEVINTERFACE_HID: GUID = GUID::from_u128(0x4d1e55b2_f16f_11cf_88cb_001111000030);

static TX: OnceLock<Sender<()>> = OnceLock::new();
static RETRY: AtomicBool = AtomicBool::new(false);

/// Starts the notifications; each one arrives as a message. They last as long as the process.
pub fn listen() -> Receiver<()> {
    let (tx, rx) = mpsc::channel();
    let _ = TX.set(tx);
    unsafe {
        let filter = CM_NOTIFY_FILTER {
            cbSize: size_of::<CM_NOTIFY_FILTER>() as u32,
            Flags: 0,
            FilterType: CM_NOTIFY_FILTER_TYPE_DEVICEINTERFACE,
            Reserved: 0,
            u: CM_NOTIFY_FILTER_0 { DeviceInterface: CM_NOTIFY_FILTER_0_0 { ClassGuid: GUID_DEVINTERFACE_HID } },
        };
        let mut cm: HCMNOTIFICATION = std::ptr::null_mut();
        CM_Register_Notification(&filter, std::ptr::null(), Some(interface), &mut cm);
        let params = Box::leak(Box::new(DEVICE_NOTIFY_SUBSCRIBE_PARAMETERS { Callback: Some(power), Context: std::ptr::null_mut() }));
        let mut power_handle = std::ptr::null_mut();
        PowerRegisterSuspendResumeNotification(DEVICE_NOTIFY_CALLBACK, (params as *mut DEVICE_NOTIFY_SUBSCRIBE_PARAMETERS).cast(), &mut power_handle);
    }
    rx
}

/// Runs a watcher step now.
pub fn wake() {
    if let Some(tx) = TX.get() {
        let _ = tx.send(());
    }
}

/// Runs a watcher step on the next Synapse check: a busy board or a failure to show and try again.
pub fn retry() {
    RETRY.store(true, Ordering::SeqCst);
}

pub fn take_retry() -> bool {
    RETRY.swap(false, Ordering::SeqCst)
}

unsafe extern "system" fn interface(_: HCMNOTIFICATION, _: *const core::ffi::c_void, _: CM_NOTIFY_ACTION, _: *const CM_NOTIFY_EVENT_DATA, _: u32) -> u32 {
    wake();
    0
}

/// After a resume a keyboard may come back without being plugged again.
unsafe extern "system" fn power(_: *const core::ffi::c_void, kind: u32, _: *const core::ffi::c_void) -> u32 {
    if kind == PBT_APMRESUMEAUTOMATIC {
        wake();
    }
    0
}
