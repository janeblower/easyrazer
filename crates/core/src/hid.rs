//! [`Transport`] over `hidapi`: the control interface MI_03 of the Huntsman V2 Analog.

use hidapi::{HidApi, HidDevice};

use crate::transport::{Error, Report, Transport};

pub const VID: u16 = 0x1532;
pub const PID: u16 = 0x0266;

const CONTROL_INTERFACE: i32 = 3;

pub struct HidTransport(pub HidDevice);

impl Transport for HidTransport {
    fn send_feature(&self, report: &Report) -> Result<(), Error> {
        self.0.send_feature_report(report).map_err(|e| Error::Io(e.to_string()))
    }

    fn get_feature(&self, report: &mut Report) -> Result<(), Error> {
        let n = self.0.get_feature_report(report).map_err(|e| Error::Io(e.to_string()))?;
        if n < report.len() {
            return Err(Error::Io(format!("short report ({n} bytes)")));
        }
        Ok(())
    }
}

/// Opens the control interface; `Ok(None)` when the keyboard is not connected.
pub fn open_control(api: &HidApi) -> Result<Option<HidTransport>, Error> {
    let Some(info) = api
        .device_list()
        .find(|d| d.vendor_id() == VID && d.product_id() == PID && d.interface_number() == CONTROL_INTERFACE)
    else {
        return Ok(None);
    };
    info.open_device(api).map(|d| Some(HidTransport(d))).map_err(|e| Error::Io(e.to_string()))
}
