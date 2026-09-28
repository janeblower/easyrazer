//! Host-side key engine for driver mode, where the keyboard only reports key depth.
//!
//! Depth and press points share one scale: 0..=255 over 1.5..3.6 mm, like `02:12` thresholds.

use std::time::{Duration, Instant};

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
            if d >= u16::from(k.extreme) + u16::from(r.press) || k.extreme <= act.saturating_sub(HYSTERESIS) {
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

const F9: u8 = 120;
const F10: u8 = 121;
const F11: u8 = 122;
const F12: u8 = 123;
const PAUSE: u8 = 126;
const MENU: u8 = 129;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Media {
    Prev,
    Play,
    Next,
    Mute,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Output {
    Key { key: u8, down: bool },
    Media(Media),
    /// One brightness step down (-1) or up (+1).
    Brightness(i8),
    Sleep,
}

#[derive(Clone, Debug)]
pub struct Config {
    /// Press point per fwID, the key's onboard `thrL`.
    pub act: [u8; 256],
    pub rapid: [Option<Trigger>; 256],
    pub repeat_delay: Duration,
    pub repeat_interval: Duration,
}

/// What the firmware's Fn layer does on V2 (Synapse defaults); an empty slice swallows the key.
fn fn_layer(key: u8) -> Option<&'static [Output]> {
    Some(match key {
        F11 => &[Output::Brightness(-1)],
        F12 => &[Output::Brightness(1)],
        PAUSE => &[Output::Sleep],
        F9 | F10 | MENU => &[],
        _ => return None,
    })
}

/// Razer report codes, taken from one capture of the keys pressed left to right.
fn media(code: u8) -> Option<Media> {
    Some(match code {
        0x53 => Media::Prev,
        0x55 => Media::Play,
        0x54 => Media::Next,
        0x52 => Media::Mute,
        _ => return None,
    })
}

pub struct Engine {
    cfg: Config,
    keys: [KeyState; 256],
    /// Razer codes held now.
    held: Vec<u8>,
    /// Key to repeat and when; like Windows, only the newest key repeats.
    repeat: Option<(u8, Instant)>,
}

impl Engine {
    pub fn new(cfg: Config) -> Self {
        Self { cfg, keys: [KeyState::default(); 256], held: Vec::new(), repeat: None }
    }

    pub fn set_config(&mut self, cfg: Config) {
        self.cfg = cfg;
    }

    pub fn feed_depth(&mut self, depth: &[u8; 256], now: Instant) -> Vec<Output> {
        let mut out = Vec::new();
        for id in 1..256 {
            if let Some(down) = step(&mut self.keys[id], depth[id], self.cfg.act[id], self.cfg.rapid[id]) {
                self.transition(id as u8, down, now, &mut out);
            }
        }
        out
    }

    fn transition(&mut self, key: u8, down: bool, now: Instant, out: &mut Vec<Output>) {
        let k = &mut self.keys[key as usize];
        if down
            && self.held.contains(&RAZER_FN)
            && let Some(action) = fn_layer(key)
        {
            k.fn_layer = true;
            out.extend_from_slice(action);
            return;
        }
        if !down && std::mem::take(&mut k.fn_layer) {
            return;
        }
        out.push(Output::Key { key, down });
        if down {
            self.repeat = Some((key, now + self.cfg.repeat_delay));
        } else if self.repeat.is_some_and(|(r, _)| r == key) {
            self.repeat = None;
        }
    }

    pub fn feed_razer(&mut self, codes: &[u8]) -> Vec<Output> {
        let out = codes.iter().filter(|c| !self.held.contains(c)).filter_map(|&c| media(c)).map(Output::Media).collect();
        self.held = codes.to_vec();
        out
    }

    pub fn tick(&mut self, now: Instant) -> Vec<Output> {
        match self.repeat {
            Some((key, at)) if now >= at => {
                // After a stall, repeat once and resume the cadence from now.
                self.repeat = Some((key, (at + self.cfg.repeat_interval).max(now)));
                vec![Output::Key { key, down: true }]
            }
            _ => Vec::new(),
        }
    }

    pub fn deadline(&self) -> Option<Instant> {
        self.repeat.map(|(_, at)| at)
    }

    /// Lifts every key the host holds down, for when the engine stops or loses the keyboard.
    pub fn release_all(&mut self) -> Vec<Output> {
        let out = (1..=255u8)
            .filter(|&id| self.keys[id as usize].down && !self.keys[id as usize].fn_layer)
            .map(|key| Output::Key { key, down: false })
            .collect();
        self.keys = [KeyState::default(); 256];
        self.held.clear();
        self.repeat = None;
        out
    }
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
    fn noise_while_lifting_through_the_point_does_not_press() {
        let rt = Some(Trigger { press: 12, release: 12 });
        assert_eq!(run(&[0, 140, 128, 110, 99, 100, 97, 0], 100, rt), [true, false]);
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

    const A: u8 = 31;
    const S: u8 = 32;

    fn engine() -> Engine {
        Engine::new(Config {
            act: [100; 256],
            rapid: [None; 256],
            repeat_delay: Duration::from_millis(500),
            repeat_interval: Duration::from_millis(33),
        })
    }

    fn depth(keys: &[(u8, u8)]) -> [u8; 256] {
        let mut d = [0; 256];
        for &(k, v) in keys {
            d[k as usize] = v;
        }
        d
    }

    fn key(key: u8, down: bool) -> Output {
        Output::Key { key, down }
    }

    #[test]
    fn autorepeat_after_the_delay_at_the_interval() {
        let (mut e, t) = (engine(), Instant::now());
        assert_eq!(e.feed_depth(&depth(&[(A, 150)]), t), [key(A, true)]);
        assert!(e.tick(t + Duration::from_millis(499)).is_empty());
        assert_eq!(e.tick(t + Duration::from_millis(500)), [key(A, true)]);
        assert!(e.tick(t + Duration::from_millis(520)).is_empty());
        assert_eq!(e.tick(t + Duration::from_millis(533)), [key(A, true)]);
        assert_eq!(e.feed_depth(&depth(&[]), t + Duration::from_millis(540)), [key(A, false)]);
        assert!(e.tick(t + Duration::from_secs(2)).is_empty());
        assert_eq!(e.deadline(), None);
    }

    #[test]
    fn autorepeat_follows_the_newest_key() {
        let (mut e, t) = (engine(), Instant::now());
        e.feed_depth(&depth(&[(A, 150)]), t);
        e.feed_depth(&depth(&[(A, 150), (S, 150)]), t + Duration::from_millis(100));
        assert_eq!(e.tick(t + Duration::from_millis(600)), [key(S, true)]);
        e.feed_depth(&depth(&[(A, 150)]), t + Duration::from_millis(700));
        assert!(e.tick(t + Duration::from_secs(2)).is_empty());
    }

    #[test]
    fn fn_layer_takes_brightness_keys() {
        let (mut e, t) = (engine(), Instant::now());
        e.feed_razer(&[RAZER_FN]);
        assert_eq!(e.feed_depth(&depth(&[(F12, 150)]), t), [Output::Brightness(1)]);
        assert!(e.feed_depth(&depth(&[]), t).is_empty());
        assert_eq!(e.feed_depth(&depth(&[(F11, 150)]), t), [Output::Brightness(-1)]);
        e.feed_depth(&depth(&[]), t);
        assert_eq!(e.feed_depth(&depth(&[(PAUSE, 150)]), t), [Output::Sleep]);
        e.feed_depth(&depth(&[]), t);
        assert!(e.feed_depth(&depth(&[(F9, 150)]), t).is_empty());
        e.feed_depth(&depth(&[]), t);
        e.feed_razer(&[]);
        assert_eq!(e.feed_depth(&depth(&[(F12, 150)]), t), [key(F12, true)]);
    }

    #[test]
    fn fn_layer_keys_do_not_repeat() {
        let (mut e, t) = (engine(), Instant::now());
        e.feed_razer(&[RAZER_FN]);
        e.feed_depth(&depth(&[(F12, 150)]), t);
        assert!(e.tick(t + Duration::from_secs(2)).is_empty());
    }

    #[test]
    fn fn_key_release_is_swallowed_after_fn_lifts() {
        let (mut e, t) = (engine(), Instant::now());
        e.feed_razer(&[RAZER_FN]);
        e.feed_depth(&depth(&[(F12, 150)]), t);
        e.feed_razer(&[]);
        assert!(e.feed_depth(&depth(&[]), t).is_empty());
    }

    #[test]
    fn media_keys_fire_once_per_press() {
        let mut e = engine();
        assert_eq!(e.feed_razer(&[0x55]), [Output::Media(Media::Play)]);
        assert!(e.feed_razer(&[0x55]).is_empty());
        assert!(e.feed_razer(&[]).is_empty());
        assert_eq!(e.feed_razer(&[0x52, 0x53]), [Output::Media(Media::Mute), Output::Media(Media::Prev)]);
        assert_eq!(e.feed_razer(&[0x54]), [Output::Media(Media::Next)]);
    }

    #[test]
    fn release_all_lifts_held_keys() {
        let (mut e, t) = (engine(), Instant::now());
        e.feed_razer(&[RAZER_FN]);
        e.feed_depth(&depth(&[(A, 150), (F12, 150)]), t);
        assert_eq!(e.release_all(), [key(A, false)]);
        assert_eq!(e.deadline(), None);
        assert_eq!(e.feed_depth(&depth(&[(A, 150)]), t), [key(A, true)]);
    }

    #[test]
    fn new_config_keeps_held_keys() {
        let (mut e, t) = (engine(), Instant::now());
        e.feed_depth(&depth(&[(A, 150)]), t);
        let mut cfg = e.cfg.clone();
        cfg.rapid[A as usize] = Some(Trigger { press: 10, release: 10 });
        e.set_config(cfg);
        assert_eq!(e.feed_depth(&depth(&[(A, 135)]), t), [key(A, false)]);
    }

    #[test]
    fn parses_razer_reports() {
        assert_eq!(parse_razer(&[0x04, 0x01, 0x55, 0, 0]), Some(vec![0x01, 0x55]));
        assert_eq!(parse_razer(&[0x04, 0, 0]), Some(vec![]));
        assert_eq!(parse_razer(&[0x07, 31, 9]), None);
    }
}
