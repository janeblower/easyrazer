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

/// The interfaces of one physical keyboard.
#[derive(Clone, Debug)]
pub struct Unit {
    pub spec: &'static DeviceSpec,
    /// `None` where Windows gave no container: the unit then takes every interface of its PID.
    pub container: Option<String>,
}

impl Unit {
    /// An interface whose container cannot be read goes by its PID, as before containers were read.
    pub fn owns(&self, pid: u16, container: Option<&str>) -> bool {
        pid == self.spec.pid && (self.container.is_none() || container.is_none() || self.container.as_deref() == container)
    }
}

/// Keyboards among `(pid, interface, container)` entries, in order of their control interface.
pub fn group(ifaces: impl IntoIterator<Item = (u16, i32, Option<String>)>) -> Vec<Unit> {
    let mut units: Vec<Unit> = Vec::new();
    for (pid, interface, container) in ifaces {
        let Some(spec) = devices::by_pid(pid) else { continue };
        if interface != spec.control_interface || units.iter().any(|u| u.spec.pid == pid && u.container == container) {
            continue;
        }
        units.push(Unit { spec, container });
    }
    units
}

pub fn units(api: &HidApi) -> Vec<Unit> {
    group(api.device_list().filter(|d| d.vendor_id() == VID).map(|d| (d.product_id(), d.interface_number(), container_id(d.path()))))
}

pub fn interfaces<'a>(api: &'a HidApi, unit: &'a Unit) -> impl Iterator<Item = &'a hidapi::DeviceInfo> + 'a {
    api.device_list().filter(move |d| d.vendor_id() == VID && unit.owns(d.product_id(), container_id(d.path()).as_deref()))
}

pub fn open_control(api: &HidApi, unit: &Unit) -> Result<HidTransport, Error> {
    let info = interfaces(api, unit)
        .find(|d| d.interface_number() == unit.spec.control_interface)
        .ok_or_else(|| Error::Io("control interface not found".into()))?;
    Ok(HidTransport { dev: info.open_device(api).map_err(io)?, tid: unit.spec.tid })
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
pub fn set_autonomous(api: &HidApi, unit: &Unit, on: bool) -> Result<(), Error> {
    let d = unit.spec;
    let report = d.lamp_array.as_ref().ok_or_else(|| Error::Io(format!("{}: no LampArray", d.name)))?.control_report;
    open_lamp_array(api, unit)?.send_feature_report(&[report, on as u8]).map_err(io)
}

pub fn open_lamp_array(api: &HidApi, unit: &Unit) -> Result<HidDevice, Error> {
    let info = interfaces(api, unit)
        .find(|i| i.usage_page() == USAGE_PAGE_LAMP_ARRAY)
        .ok_or_else(|| Error::Io("LampArray collection not found".into()))?;
    info.open_device(api).map_err(io)
}

/// The ContainerId Windows gives every interface of one USB device; `None` where it cannot be read.
#[cfg(windows)]
#[allow(unsafe_code)]
pub fn container_id(path: &std::ffi::CStr) -> Option<String> {
    use windows_sys::Win32::Devices::DeviceAndDriverInstallation::{
        CM_Get_DevNode_PropertyW, CM_Get_Device_Interface_PropertyW, CM_LOCATE_DEVNODE_NORMAL, CM_Locate_DevNodeW, CR_SUCCESS,
    };
    use windows_sys::Win32::Devices::Properties::{DEVPKEY_Device_ContainerId, DEVPKEY_Device_InstanceId};
    use windows_sys::core::GUID;

    let path: Vec<u16> = path.to_str().ok()?.encode_utf16().chain([0]).collect();
    let mut ty = 0;
    let mut id = [0u16; 512];
    let mut size = size_of_val(&id) as u32;
    // SAFETY: `path` is NUL-terminated; `id` is writable for `size` bytes.
    if unsafe { CM_Get_Device_Interface_PropertyW(path.as_ptr(), &DEVPKEY_Device_InstanceId, &mut ty, id.as_mut_ptr().cast(), &mut size, 0) } != CR_SUCCESS {
        return None;
    }
    let mut node = 0;
    // SAFETY: on success `id` holds a NUL-terminated instance id within the buffer.
    if unsafe { CM_Locate_DevNodeW(&mut node, id.as_ptr(), CM_LOCATE_DEVNODE_NORMAL) } != CR_SUCCESS {
        return None;
    }
    let mut g = GUID::from_u128(0);
    let mut size = size_of::<GUID>() as u32;
    // SAFETY: `g` is writable for `size` bytes, the size of the GUID the property holds.
    if unsafe { CM_Get_DevNode_PropertyW(node, &DEVPKEY_Device_ContainerId, &mut ty, (&raw mut g).cast(), &mut size, 0) } != CR_SUCCESS {
        return None;
    }
    let tail: String = g.data4.iter().map(|b| format!("{b:02X}")).collect();
    Some(format!("{:08X}-{:04X}-{:04X}-{}-{}", g.data1, g.data2, g.data3, &tail[..4], &tail[4..]))
}

#[cfg(not(windows))]
pub fn container_id(_path: &std::ffi::CStr) -> Option<String> {
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    const V2: u16 = 0x0266;

    fn c(s: &str) -> Option<String> {
        Some(s.into())
    }

    #[test]
    fn interfaces_of_one_keyboard_are_one_unit() {
        let units = group([(V2, 1, c("A")), (V2, 2, c("A")), (V2, 3, c("A")), (V2, 4, c("A"))]);
        assert_eq!(units.len(), 1);
        assert_eq!((units[0].spec.pid, units[0].container.as_deref()), (V2, Some("A")));
    }

    #[test]
    fn same_pid_in_two_containers_is_two_units() {
        let units = group([(V2, 3, c("A")), (V2, 1, c("B")), (V2, 3, c("B")), (V2, 1, c("A"))]);
        let found: Vec<_> = units.iter().map(|u| u.container.as_deref()).collect();
        assert_eq!(found, [Some("A"), Some("B")]);
    }

    #[test]
    fn a_unit_needs_its_control_interface() {
        assert!(group([(V2, 1, c("A")), (V2, 2, c("A"))]).is_empty());
    }

    #[test]
    fn unknown_pids_are_skipped() {
        assert!(group([(0xFFFF, 3, c("A"))]).is_empty());
    }

    #[test]
    fn group_without_container_keeps_the_pid() {
        let units = group([(V2, 3, None), (V2, 1, None)]);
        assert_eq!(units.len(), 1);
        assert!(units[0].owns(V2, None) && units[0].owns(V2, Some("anything")));
    }

    #[test]
    fn owns_only_its_container() {
        let u = &group([(V2, 3, c("A"))])[0];
        assert!(u.owns(V2, Some("A")));
        assert!(!u.owns(V2, Some("B")));
        assert!(!u.owns(0x02A6, Some("A")));
    }

    #[test]
    fn an_interface_without_container_belongs_to_its_pid() {
        let u = &group([(V2, 3, c("A"))])[0];
        assert!(u.owns(V2, None), "a container that cannot be read must not cost the interface");
        assert!(!u.owns(0x02A6, None));
    }
}
