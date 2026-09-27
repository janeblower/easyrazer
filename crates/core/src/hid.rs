//! [`Transport`] over `hidapi`, for any keyboard with an embedded description.

use hidapi::{HidApi, HidDevice};

use crate::devices::{self, DeviceSpec};
use crate::transport::{Error, Report, Transport};

pub const VID: u16 = 0x1532;

const USAGE_PAGE_DESKTOP: u16 = 0x01;
const USAGE_KEYBOARD: u16 = 0x06;

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
