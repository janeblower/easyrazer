//! Per-key analog key assignment (`02:12`/`02:92`) and each model's press-point scale.
//!
//! On the Huntsman V2 Analog the multi-key actuation and rapid trigger commands (`02:19`/`02:1A`)
//! answer "not supported"; actuation lives inside the key assignment instead.
//! Layout: `[profile, key, layer, thresholdL, thresholdH, fnId, fnSize, fnData..]`.
//! Thresholds map 0..=255 onto the model's [`Analog`] scale: 1.5..=3.6 mm on V2
//! (`obmEngineKeyboard.convertUIThresholdToFWThreshold`), the high byte of the u16 threshold on V3.

use serde::{Deserialize, Serialize};

use crate::packet::Command;

pub const SET_KEY_ASSIGNMENT: Command = Command::new(0x02, 0x12);
pub const GET_KEY_ASSIGNMENT: Command = Command::new(0x02, 0x92);
pub const KEY_ASSIGNMENT_SIZE: u8 = 80;

/// How a model maps millimetres onto thresholds and depth.
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Analog {
    /// Threshold and depth 0.
    pub zero_mm: f32,
    /// Threshold and depth 255.
    pub full_mm: f32,
    /// Press points the window offers.
    pub range: [f32; 2],
    /// Press points the firmware keeps to in hardware mode, if narrower than `range`.
    pub hw_range: Option<[f32; 2]>,
    /// Rapid Trigger and Snap Tap run in the firmware in hardware mode (`02:1A`, `02:21`).
    #[serde(default)]
    pub firmware_rapid: bool,
    #[serde(skip_serializing)]
    pub depth: DepthReport,
}

/// Where driver mode streams key depth: an MI_01 collection.
#[derive(Clone, Debug, Deserialize)]
pub struct DepthReport {
    /// Collection in the device path, such as `col06`.
    pub collection: String,
    #[serde(deserialize_with = "crate::devices::hex_u8")]
    pub id: u8,
    /// `true` for a u16 BE depth on the u16 threshold scale, `false` for one byte.
    #[serde(default)]
    pub wide: bool,
}

impl Analog {
    pub fn threshold(&self, mm: f32) -> u8 {
        ((mm - self.zero_mm) / (self.full_mm - self.zero_mm) * 255.0).round().clamp(0.0, 255.0) as u8
    }

    pub fn mm(&self, t: u8) -> f32 {
        t as f32 / 255.0 * (self.full_mm - self.zero_mm) + self.zero_mm
    }

    /// A travel distance in threshold units, at least one.
    pub fn units(&self, mm: f32) -> u8 {
        (mm / (self.full_mm - self.zero_mm) * 255.0).round().clamp(1.0, 255.0) as u8
    }
}

/// Key layer: normal or Hypershift.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum Layer {
    Normal = 0,
    Hypershift = 1,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct KeyAssignment {
    pub profile: u8,
    pub key: u8,
    pub layer: u8,
    pub threshold_low: u8,
    pub threshold_high: u8,
    pub fn_id: u8,
    pub fn_data: Vec<u8>,
}

pub fn parse(data: &[u8]) -> Option<KeyAssignment> {
    let [profile, key, layer, threshold_low, threshold_high, fn_id, fn_size, rest @ ..] = data else {
        return None;
    };
    Some(KeyAssignment {
        profile: *profile,
        key: *key,
        layer: *layer,
        threshold_low: *threshold_low,
        threshold_high: *threshold_high,
        fn_id: *fn_id,
        fn_data: rest.get(..*fn_size as usize)?.to_vec(),
    })
}

pub fn set_args(a: &KeyAssignment) -> Vec<u8> {
    let mut v = vec![a.profile, a.key, a.layer, a.threshold_low, a.threshold_high, a.fn_id, a.fn_data.len() as u8];
    v.extend(&a.fn_data);
    v
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scale(pid: u16) -> &'static Analog {
        &crate::devices::by_pid(pid).unwrap().analog
    }

    #[test]
    fn v2_threshold_conversion() {
        let a = scale(0x0266);
        assert_eq!(a.threshold(1.5), 0);
        assert_eq!(a.threshold(3.6), 255);
        assert_eq!(a.threshold(2.4), 109);
        assert!((a.mm(0x6F) - 2.41).abs() < 0.01);
        assert_eq!(a.units(0.1), 12);
    }

    #[test]
    fn v3_thresholds_are_the_high_byte_of_the_u16() {
        let a = scale(0x02A6);
        // Synapse's default 19656 (1.2 mm) is 76.8 in high bytes.
        assert_eq!(a.threshold(1.2), 77);
        assert_eq!(a.threshold(4.0), 255);
        for tenth in 1..=40 {
            let mm = tenth as f32 / 10.0;
            assert_eq!((a.mm(a.threshold(mm)) * 10.0).round() / 10.0, mm, "the window's 0.1 mm steps survive");
        }
    }

    #[test]
    fn parses_device_reply_and_roundtrips() {
        // Captured from key A, profile 3.
        let data = [0x03, 0x1F, 0x00, 0x8F, 0x6F, 0x02, 0x02, 0x00, 0x04];
        let a = parse(&data).unwrap();
        assert_eq!((a.key, a.threshold_low, a.threshold_high, a.fn_id), (31, 0x8F, 0x6F, 2));
        assert_eq!(a.fn_data, [0x00, 0x04]);
        assert_eq!(set_args(&a), data);
    }
}
