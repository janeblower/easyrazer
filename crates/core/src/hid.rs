//! [`Transport`] over `hidapi`, for any keyboard with an embedded description.

use hidapi::{HidApi, HidDevice};

use crate::devices::{self, DeviceSpec};
use crate::transport::{Error, Report, Transport};

pub const VID: u16 = 0x1532;

const USAGE_PAGE_DESKTOP: u16 = 0x01;
const USAGE_KEYBOARD: u16 = 0x06;
const USAGE_PAGE_LAMP_ARRAY: u16 = 0x59;

pub struct HidTransport {
    dev: HidDevice,
    tid: u8,
}

impl Transport for HidTransport {
    fn send_feature(&self, report: &Report) -> Result<(), Error> {
        self.dev.send_feature_report(report).map_err(io)
    }

    fn get_feature(&self, report: &mut Report) -> Result<(), Error> {
        let n = self.dev.get_feature_report(report).map_err(io)?;
        if n < report.len() {
            return Err(Error::Io(format!("short report ({n} bytes)")));
        }
        Ok(())
    }

    fn tid(&self) -> u8 {
        self.tid
    }
}

fn io(e: hidapi::HidError) -> Error {
    Error::Io(e.to_string())
}

/// Opens the control interface of the first described keyboard; `Ok(None)` when none is connected.
pub fn open_control(api: &HidApi) -> Result<Option<(HidTransport, &'static DeviceSpec)>, Error> {
    for info in api.device_list().filter(|d| d.vendor_id() == VID) {
        let Some(spec) = devices::by_pid(info.product_id()) else {
            continue;
        };
        if info.interface_number() != spec.control_interface {
            continue;
        }
        let dev = info.open_device(api).map_err(io)?;
        return Ok(Some((HidTransport { dev, tid: spec.tid }, spec)));
    }
    Ok(None)
}

/// PID of a connected Razer keyboard we have no description for.
pub fn unsupported_keyboard(api: &HidApi) -> Option<u16> {
    api.device_list()
        .find(|d| {
            d.vendor_id() == VID
                && d.usage_page() == USAGE_PAGE_DESKTOP
                && d.usage() == USAGE_KEYBOARD
                && devices::by_pid(d.product_id()).is_none()
        })
        .map(|d| d.product_id())
}

/// Hands the lamps back to the firmware (`on`) or to the host. Going from host to firmware
/// restarts the factory spectrum, so callers re-send their effect afterwards.
pub fn set_autonomous(api: &HidApi, d: &DeviceSpec, on: bool) -> Result<(), Error> {
    let report = d.lamp_array.as_ref().ok_or_else(|| Error::Io(format!("{}: no LampArray", d.name)))?.control_report;
    open_lamp_array(api, d)?.send_feature_report(&[report, on as u8]).map_err(io)
}

pub fn open_lamp_array(api: &HidApi, d: &DeviceSpec) -> Result<HidDevice, Error> {
    let info = api
        .device_list()
        .find(|i| i.vendor_id() == d.vid && i.product_id() == d.pid && i.usage_page() == USAGE_PAGE_LAMP_ARRAY)
        .ok_or_else(|| Error::Io("LampArray collection not found".into()))?;
    info.open_device(api).map_err(io)
}

/// The ContainerId Windows gives every interface of one USB device; `None` where it cannot be read.
#[cfg(windows)]
pub fn container_id(path: &std::ffi::CStr) -> Option<String> {
    use windows_sys::Win32::Devices::DeviceAndDriverInstallation::{
        CM_Get_DevNode_PropertyW, CM_Get_Device_Interface_PropertyW, CM_LOCATE_DEVNODE_NORMAL, CM_Locate_DevNodeW, CR_SUCCESS,
    };
    use windows_sys::Win32::Devices::Properties::{DEVPKEY_Device_ContainerId, DEVPKEY_Device_InstanceId};
    use windows_sys::core::GUID;

    let path: Vec<u16> = path.to_str().ok()?.encode_utf16().chain([0]).collect();
    unsafe {
        let mut ty = 0;
        let mut id = [0u16; 512];
        let mut size = size_of_val(&id) as u32;
        if CM_Get_Device_Interface_PropertyW(path.as_ptr(), &DEVPKEY_Device_InstanceId, &mut ty, id.as_mut_ptr().cast(), &mut size, 0) != CR_SUCCESS {
            return None;
        }
        let mut node = 0;
        if CM_Locate_DevNodeW(&mut node, id.as_ptr(), CM_LOCATE_DEVNODE_NORMAL) != CR_SUCCESS {
            return None;
        }
        let mut g: GUID = std::mem::zeroed();
        let mut size = size_of::<GUID>() as u32;
        if CM_Get_DevNode_PropertyW(node, &DEVPKEY_Device_ContainerId, &mut ty, (&raw mut g).cast(), &mut size, 0) != CR_SUCCESS {
            return None;
        }
        let tail: String = g.data4.iter().map(|b| format!("{b:02X}")).collect();
        Some(format!("{:08X}-{:04X}-{:04X}-{}-{}", g.data1, g.data2, g.data3, &tail[..4], &tail[4..]))
    }
}

#[cfg(not(windows))]
pub fn container_id(_path: &std::ffi::CStr) -> Option<String> {
    None
}
