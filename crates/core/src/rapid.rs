//! Host-side key engine for driver mode, where the keyboard only reports key depth.
//!
//! Depth and press points share one scale: 0..=255 over 1.5..3.6 mm, like `02:12` thresholds.

use crate::analog::{MAX_MM, MIN_MM};

/// Release sits this far above the press point, like Synapse's `minBreak` (0.1 mm);
/// without it a key resting at the point chatters on sensor noise.
pub const HYSTERESIS: u8 = 12;

/// MI_01 Col06: `07 (fwID depth)* 00`; keys missing from a report are up.
pub const DEPTH_REPORT: u8 = 0x07;
/// MI_01 Col04: `04 code*`, all zero when nothing is held.
pub const RAZER_REPORT: u8 = 0x04;
pub const RAZER_FN: u8 = 0x01;

/// Rapid Trigger distances in depth units.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Trigger {
    pub press: u8,
    pub release: u8,
}

pub fn mm_to_depth(mm: f32) -> u8 {
    (mm / (MAX_MM - MIN_MM) * 255.0).round().clamp(1.0, 255.0) as u8
}

pub fn parse_depth(report: &[u8]) -> Option<[u8; 256]> {
    let (&id, pairs) = report.split_first()?;
    if id != DEPTH_REPORT {
        return None;
    }
    let mut depth = [0u8; 256];
    for p in pairs.chunks_exact(2).take_while(|p| p[0] != 0) {
        depth[p[0] as usize] = p[1];
    }
    Some(depth)
}

pub fn parse_razer(report: &[u8]) -> Option<Vec<u8>> {
    let (&id, codes) = report.split_first()?;
    (id == RAZER_REPORT).then(|| codes.iter().copied().filter(|&c| c != 0).collect())
}

#[derive(Clone, Copy, Debug, Default)]
struct KeyState {
    down: bool,
    /// Deepest point while down, shallowest while up, since the last transition.
    extreme: u8,
    /// Taken by the Fn layer: its release emits nothing.
    fn_layer: bool,
}

/// One depth sample of a key; `Some(down)` on a transition.
/// With Rapid Trigger, above the release line the key goes down after moving `press`
/// deeper than its shallowest point and up after rising `release` from its deepest one.
fn step(k: &mut KeyState, depth: u8, act: u8, rt: Option<Trigger>) -> Option<bool> {
    let was = k.down;
    let act = act.max(1);
    if depth == 0 || depth < act.saturating_sub(HYSTERESIS) {
        k.down = false;
        k.extreme = depth;
    } else if let Some(r) = rt.filter(|_| k.down || depth >= act) {
        // In u16: at the bottom of the travel depth + distance would saturate and release at once.
        let d = u16::from(depth);
        if k.down {
            k.extreme = k.extreme.max(depth);
            if d + u16::from(r.release) <= u16::from(k.extreme) {
                k.down = false;
                k.extreme = depth;
            }
        } else {
            k.extreme = k.extreme.min(depth);
            if d >= u16::from(k.extreme) + u16::from(r.press) || k.extreme < act {
                k.down = true;
                k.extreme = depth;
            }
        }
    } else {
        k.down |= depth >= act;
        // Kept current so Rapid Trigger turned on mid-press measures from the right point.
        k.extreme = if k.down { k.extreme.max(depth) } else { k.extreme.min(depth) };
    }
    (k.down != was).then_some(k.down)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(depths: &[u8], act: u8, rt: Option<Trigger>) -> Vec<bool> {
        let mut k = KeyState::default();
        depths.iter().filter_map(|&d| step(&mut k, d, act, rt)).collect()
    }

    const RT10: Option<Trigger> = Some(Trigger { press: 10, release: 10 });

    #[test]
    fn fixed_point_releases_below_the_hysteresis() {
        assert_eq!(run(&[0, 50, 100, 120, 90, 20, 0], 100, None), [true, false]);
    }

    #[test]
    fn noise_at_the_point_does_not_chatter() {
        let noisy = [0, 99, 100, 99, 101, 98, 100, 95, 87];
        assert_eq!(run(&noisy, 100, None), [true, false]);
        assert_eq!(run(&noisy, 100, Some(Trigger { press: 30, release: 30 })), [true, false]);
    }

    #[test]
    fn minimum_point_releases_when_the_key_is_gone() {
        assert_eq!(run(&[0, 3, 1, 2, 0], 0, None), [true, false]);
    }

    #[test]
    fn rapid_trigger_rearms_mid_travel() {
        assert_eq!(run(&[0, 110, 200, 190, 185, 195, 150, 90], 100, RT10), [true, false, true, false]);
    }

    #[test]
    fn rapid_trigger_ignores_jitter() {
        assert_eq!(run(&[0, 150, 145, 148, 150, 142], 100, RT10), [true]);
    }

    #[test]
    fn split_press_and_release() {
        let rt = Some(Trigger { press: 10, release: 30 });
        assert_eq!(run(&[0, 110, 200, 185, 170, 175, 180], 100, rt), [true, false, true]);
    }

    #[test]
    fn deepest_point_does_not_overflow() {
        assert_eq!(run(&[0, 255, 255, 200], 100, Some(Trigger { press: 255, release: 50 })), [true, false]);
    }

    #[test]
    fn sensitivity_in_depth_units() {
        assert_eq!(mm_to_depth(0.1), 12);
        assert_eq!(mm_to_depth(0.4), 49);
        assert_eq!(mm_to_depth(0.0), 1);
    }

    #[test]
    fn parses_depth_reports() {
        let d = parse_depth(&[0x07, 31, 100, 18, 5, 0, 0, 44, 9]).unwrap();
        assert_eq!((d[31], d[18], d[44], d[1]), (100, 5, 0, 0));
        assert!(parse_depth(&[0x04, 1]).is_none());
    }

    #[test]
    fn parses_razer_reports() {
        assert_eq!(parse_razer(&[0x04, 0x01, 0x55, 0, 0]), Some(vec![0x01, 0x55]));
        assert_eq!(parse_razer(&[0x04, 0, 0]), Some(vec![]));
        assert_eq!(parse_razer(&[0x07, 31, 9]), None);
    }
}
