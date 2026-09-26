//! Per-key analog key assignment (`02:12`/`02:92`) used by the Huntsman V2 Analog.
//!
//! The newer multi-key actuation and rapid trigger commands (`02:19`/`02:1A`) answer
//! "not supported" on this device; actuation lives inside the key assignment instead.
//! Layout: `[profile, key, mode, thresholdL, thresholdH, fnId, fnSize, fnData..]`.
//! Thresholds map 0..=255 onto 1.5..=3.6 mm
//! (`obmEngineKeyboard.convertUIThresholdToFWThreshold`); 0 means firmware default.

use crate::packet::Command;

pub const SET_KEY_ASSIGNMENT: Command = Command::new(0x02, 0x12);
pub const GET_KEY_ASSIGNMENT: Command = Command::new(0x02, 0x92);
pub const KEY_ASSIGNMENT_SIZE: u8 = 80;

pub const MIN_MM: f32 = 1.5;
pub const MAX_MM: f32 = 3.6;

pub fn mm_to_threshold(mm: f32) -> u8 {
    ((mm.clamp(MIN_MM, MAX_MM) - MIN_MM) / (MAX_MM - MIN_MM) * 255.0).round() as u8
}

pub fn threshold_to_mm(t: u8) -> f32 {
    t as f32 / 255.0 * (MAX_MM - MIN_MM) + MIN_MM
}

/// Key layer: normal or Hypershift.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum Mode {
    Normal = 0,
    Hypershift = 1,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct KeyAssignment {
    pub profile: u8,
    pub key: u8,
    pub mode: u8,
    pub threshold_low: u8,
    pub threshold_high: u8,
    pub fn_id: u8,
    pub fn_data: Vec<u8>,
}

pub fn get_args(profile: u8, key: u8, mode: Mode) -> [u8; 3] {
    [profile, key, mode as u8]
}

pub fn parse(data: &[u8]) -> Option<KeyAssignment> {
    let [profile, key, mode, threshold_low, threshold_high, fn_id, fn_size, rest @ ..] = data else {
        return None;
    };
    Some(KeyAssignment {
        profile: *profile,
        key: *key,
        mode: *mode,
        threshold_low: *threshold_low,
        threshold_high: *threshold_high,
        fn_id: *fn_id,
        fn_data: rest.get(..*fn_size as usize)?.to_vec(),
    })
}

pub fn set_args(a: &KeyAssignment) -> Vec<u8> {
    let mut v = vec![a.profile, a.key, a.mode, a.threshold_low, a.threshold_high, a.fn_id, a.fn_data.len() as u8];
    v.extend(&a.fn_data);
    v
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn threshold_conversion() {
        assert_eq!(mm_to_threshold(1.5), 0);
        assert_eq!(mm_to_threshold(3.6), 255);
        assert_eq!(mm_to_threshold(2.4), 109);
        assert!((threshold_to_mm(0x6F) - 2.41).abs() < 0.01);
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
