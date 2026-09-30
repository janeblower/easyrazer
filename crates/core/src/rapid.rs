//! Host-side key engine for driver mode, where the keyboard only reports key depth.
//!
//! Depth and press points share one scale: 0..=255 over 1.5..3.6 mm, like `02:12` thresholds.

use std::collections::BTreeMap;
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};

use crate::analog::{MAX_MM, MIN_MM};
use crate::binding::{self, Action, MODIFIERS, MacroMode, Mouse};
use crate::macros::Event;

/// Release sits this far above the press point, like Synapse's `minBreak` (0.1 mm);
/// without it a key resting at the point chatters on sensor noise.
const HYSTERESIS: u8 = 12;

/// MI_01 Col06: `07 (fwID depth)* 00`; keys missing from a report are up.
const DEPTH_REPORT: u8 = 0x07;
/// MI_01 Col04: `04 code*`, all zero when nothing is held.
const RAZER_REPORT: u8 = 0x04;
const RAZER_FN: u8 = 0x01;
/// Smallest depth change worth showing; below it the UI would follow sensor noise.
const DEPTH_STEP: u8 = 3;

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
    /// What the press did, so the release undoes that even if the binding changed meanwhile.
    sent: Option<Action>,
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

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Media {
    Prev,
    Play,
    Next,
    Stop,
    Mute,
    VolumeUp,
    VolumeDown,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Output {
    Key { key: u8, down: bool },
    Mouse { button: Mouse, down: bool },
    Media(Media),
    /// One brightness step down (-1) or up (+1).
    Brightness(i8),
    Sleep,
    /// The first key still pressed, for the UI: its depth (0 when none is), how far it moved
    /// since its last extreme — what Rapid Trigger measures — and whether it is down.
    Depth { depth: u8, travel: u8, down: bool },
}

#[derive(Clone, Debug)]
pub struct Config {
    /// Press point per fwID, the key's onboard `thrL`.
    pub act: [u8; 256],
    pub rapid: [Option<Trigger>; 256],
    /// `None` types the key itself.
    pub bind: [Option<Action>; 256],
    pub repeat_delay: Duration,
    pub repeat_interval: Duration,
    /// Bodies the macro bindings play, by id.
    pub macros: BTreeMap<u16, Vec<Event>>,
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

fn press(a: Action, out: &mut Vec<Output>) {
    match a {
        Action::Disabled => {}
        Action::Key { key, mods } => {
            out.extend(bits(mods).map(|m| Output::Key { key: m, down: true }));
            out.push(Output::Key { key, down: true });
        }
        Action::Mouse { button } => out.push(Output::Mouse { button, down: true }),
        Action::Media { media } => out.push(Output::Media(media)),
        // Started by the engine, which keeps the macro's state.
        Action::Macro { .. } => {}
    }
}

fn release(a: Action, out: &mut Vec<Output>) {
    match a {
        Action::Key { key, mods } => {
            out.push(Output::Key { key, down: false });
            out.extend(bits(mods).rev().map(|m| Output::Key { key: m, down: false }));
        }
        Action::Mouse { button } => out.push(Output::Mouse { button, down: false }),
        Action::Disabled | Action::Media { .. } | Action::Macro { .. } => {}
    }
}

/// Modifier keys set in a HID modifier byte.
fn bits(mods: u8) -> impl DoubleEndedIterator<Item = u8> {
    (0..8).filter(move |i| mods & (1 << i) != 0).map(|i| MODIFIERS[i])
}

/// A macro playing for the key that started it.
struct Play {
    key: u8,
    events: Vec<Event>,
    pos: usize,
    /// When the next event is due.
    at: Instant,
    /// Passes left, the current one included; `None` loops.
    passes: Option<u8>,
    /// Keys and buttons the macro holds down, lifted if the engine stops mid-pass.
    held: Vec<Output>,
}

impl Play {
    fn run(&mut self, now: Instant, out: &mut Vec<Output>) {
        while self.passes != Some(0) && self.at <= now {
            let Some(&e) = self.events.get(self.pos) else {
                self.pos = 0;
                self.passes = self.passes.map(|n| n - 1);
                // A loop without delays would never let the reader go.
                self.at += Duration::from_millis(1);
                continue;
            };
            self.pos += 1;
            let o = match e {
                Event::Delay { ms } => {
                    self.at = now + Duration::from_millis(ms.into());
                    continue;
                }
                Event::Key { key, down } => Output::Key { key, down },
                Event::Mouse { button, down } => Output::Mouse { button, down },
            };
            if let Some(p) = pressed(o) {
                if o == p {
                    self.held.push(o);
                } else {
                    self.held.retain(|&h| h != p);
                }
            }
            out.push(o);
        }
    }
}

/// The press of a key or button event; wheels have no release.
fn pressed(o: Output) -> Option<Output> {
    match o {
        Output::Key { key, .. } => Some(Output::Key { key, down: true }),
        Output::Mouse { button: Mouse::WheelUp | Mouse::WheelDown, .. } => None,
        Output::Mouse { button, .. } => Some(Output::Mouse { button, down: true }),
        _ => None,
    }
}

pub struct Engine {
    cfg: Config,
    keys: [KeyState; 256],
    /// Razer codes held now.
    held: Vec<u8>,
    /// Key to repeat and when; like Windows, only the newest key repeats.
    repeat: Option<(u8, Instant)>,
    plays: Vec<Play>,
    /// Key whose depth the UI shows.
    lead: Option<u8>,
    shown: (u8, bool),
}

impl Engine {
    pub fn new(cfg: Config) -> Self {
        Self { cfg, keys: [KeyState::default(); 256], held: Vec::new(), repeat: None, plays: Vec::new(), lead: None, shown: (0, false) }
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

    /// `Output::Depth` for the depth report just fed, unless the lead key barely moved.
    pub fn lead(&mut self, depth: &[u8; 256]) -> Option<Output> {
        if self.lead.is_none_or(|k| depth[k as usize] == 0) {
            self.lead = (1..256).find(|&k| depth[k] != 0).map(|k| k as u8);
        }
        let (d, k) = self.lead.map_or((0, KeyState::default()), |k| (depth[k as usize], self.keys[k as usize]));
        let (shown, down) = self.shown;
        if d.abs_diff(shown) < DEPTH_STEP && (d == 0) == (shown == 0) && k.down == down {
            return None;
        }
        self.shown = (d, k.down);
        Some(Output::Depth { depth: d, travel: d.abs_diff(k.extreme), down: k.down })
    }

    fn transition(&mut self, key: u8, down: bool, now: Instant, out: &mut Vec<Output>) {
        let k = &mut self.keys[key as usize];
        if !down {
            if let Some(a) = k.sent.take() {
                release(a, out);
                if let Action::Macro { mode: MacroMode::Hold, .. } = a {
                    self.finish(key);
                }
            }
            if self.repeat.is_some_and(|(r, _)| r == key) {
                self.repeat = None;
            }
            return;
        }
        let action = if self.held.contains(&RAZER_FN)
            && let Some(fn_out) = fn_layer(key)
        {
            out.extend_from_slice(fn_out);
            Action::Disabled
        } else {
            self.cfg.bind[key as usize].unwrap_or(binding::factory(key))
        };
        k.sent = Some(action);
        press(action, out);
        match action {
            Action::Key { .. } => self.repeat = Some((key, now + self.cfg.repeat_delay)),
            Action::Macro { id, mode, count } => self.start(key, id, mode, count, now),
            _ => {}
        }
    }

    /// Like the firmware: a press while playing `Times` is ignored, while playing `Toggle` it stops.
    fn start(&mut self, key: u8, id: u16, mode: MacroMode, count: u8, now: Instant) {
        if self.plays.iter().any(|p| p.key == key) {
            if mode == MacroMode::Toggle {
                self.finish(key);
            }
            return;
        }
        let Some(events) = self.cfg.macros.get(&id) else { return };
        let passes = (mode == MacroMode::Times).then_some(count.max(1));
        self.plays.push(Play { key, events: events.clone(), pos: 0, at: now, passes, held: Vec::new() });
    }

    /// Stops the key's macro looping once the current pass ends.
    fn finish(&mut self, key: u8) {
        for p in self.plays.iter_mut().filter(|p| p.key == key && p.passes.is_none()) {
            p.passes = Some(1);
        }
    }

    pub fn feed_razer(&mut self, codes: &[u8]) -> Vec<Output> {
        let out = codes.iter().filter(|c| !self.held.contains(c)).filter_map(|&c| media(c)).map(Output::Media).collect();
        self.held = codes.to_vec();
        out
    }

    pub fn tick(&mut self, now: Instant) -> Vec<Output> {
        let mut out = self.repeat_tick(now);
        for p in &mut self.plays {
            p.run(now, &mut out);
        }
        self.plays.retain(|p| p.passes != Some(0));
        out
    }

    fn repeat_tick(&mut self, now: Instant) -> Vec<Output> {
        match self.repeat {
            Some((source, at)) if now >= at => {
                // After a stall, repeat once and resume the cadence from now.
                let next = at + self.cfg.repeat_interval;
                self.repeat = Some((source, if next < now { now + self.cfg.repeat_interval } else { next }));
                match self.keys[source as usize].sent {
                    Some(Action::Key { key, .. }) => vec![Output::Key { key, down: true }],
                    _ => Vec::new(),
                }
            }
            _ => Vec::new(),
        }
    }

    pub fn deadline(&self) -> Option<Instant> {
        self.repeat.map(|(_, at)| at).into_iter().chain(self.plays.iter().map(|p| p.at)).min()
    }

    /// Lifts every key the host holds down, for when the engine stops or loses the keyboard.
    pub fn release_all(&mut self) -> Vec<Output> {
        let mut out = Vec::new();
        for a in self.keys.iter().filter_map(|k| k.sent) {
            release(a, &mut out);
        }
        for p in self.plays.drain(..) {
            out.extend(p.held.into_iter().rev().map(|o| match o {
                Output::Key { key, .. } => Output::Key { key, down: false },
                Output::Mouse { button, .. } => Output::Mouse { button, down: false },
                o => o,
            }));
        }
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
            bind: [None; 256],
            repeat_delay: Duration::from_millis(500),
            repeat_interval: Duration::from_millis(33),
            macros: BTreeMap::new(),
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
    fn lead_follows_the_first_key_held() {
        let (mut e, t) = (engine(), Instant::now());
        let mut feed = |keys: &[(u8, u8)]| {
            let d = depth(keys);
            e.feed_depth(&d, t);
            e.lead(&d).map(|o| match o {
                Output::Depth { depth, travel, down } => (depth, travel, down),
                _ => unreachable!(),
            })
        };
        assert_eq!(feed(&[(S, 50)]), Some((50, 0, false)));
        assert_eq!(feed(&[(A, 200), (S, 52)]), None);
        assert_eq!(feed(&[(A, 200), (S, 150)]), Some((150, 0, true)));
        assert_eq!(feed(&[(A, 200), (S, 120)]), Some((120, 30, true)));
        assert_eq!(feed(&[(A, 200)]), Some((200, 0, true)));
        assert_eq!(feed(&[]), Some((0, 0, false)));
        assert_eq!(feed(&[]), None);
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
    fn autorepeat_after_a_stall_repeats_once() {
        let (mut e, t) = (engine(), Instant::now());
        e.feed_depth(&depth(&[(A, 150)]), t);
        assert_eq!(e.tick(t + Duration::from_millis(600)), [key(A, true)]);
        assert!(e.tick(t + Duration::from_millis(632)).is_empty());
        assert_eq!(e.tick(t + Duration::from_millis(633)), [key(A, true)]);
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

    fn bound(pairs: &[(u8, Action)]) -> Engine {
        let mut e = engine();
        for &(k, a) in pairs {
            e.cfg.bind[k as usize] = Some(a);
        }
        e
    }

    const CTRL: u8 = 58;
    const SHIFT: u8 = 44;
    const C: u8 = 48;

    #[test]
    fn bound_key_types_its_target_with_modifiers() {
        let (mut e, t) = (bound(&[(A, Action::Key { key: C, mods: 0x03 })]), Instant::now());
        assert_eq!(e.feed_depth(&depth(&[(A, 150)]), t), [key(CTRL, true), key(SHIFT, true), key(C, true)]);
        assert_eq!(e.tick(t + Duration::from_millis(500)), [key(C, true)]);
        assert_eq!(e.feed_depth(&depth(&[]), t), [key(C, false), key(SHIFT, false), key(CTRL, false)]);
    }

    #[test]
    fn disabled_key_types_nothing() {
        let (mut e, t) = (bound(&[(A, Action::Disabled)]), Instant::now());
        assert!(e.feed_depth(&depth(&[(A, 150)]), t).is_empty());
        assert!(e.tick(t + Duration::from_secs(2)).is_empty());
        assert!(e.feed_depth(&depth(&[]), t).is_empty());
    }

    #[test]
    fn mouse_and_media_bindings() {
        let m = Action::Mouse { button: Mouse::Left };
        let (mut e, t) = (bound(&[(A, m), (S, Action::Media { media: Media::VolumeUp })]), Instant::now());
        let left = |down| Output::Mouse { button: Mouse::Left, down };
        assert_eq!(e.feed_depth(&depth(&[(A, 150), (S, 150)]), t), [left(true), Output::Media(Media::VolumeUp)]);
        assert!(e.tick(t + Duration::from_secs(2)).is_empty());
        assert_eq!(e.feed_depth(&depth(&[]), t), [left(false)]);
    }

    #[test]
    fn release_follows_the_binding_of_the_press() {
        let (mut e, t) = (engine(), Instant::now());
        e.feed_depth(&depth(&[(A, 150)]), t);
        let mut cfg = e.cfg.clone();
        cfg.bind[A as usize] = Some(Action::Disabled);
        e.set_config(cfg);
        assert_eq!(e.release_all(), [key(A, false)]);
    }

    #[test]
    fn parses_razer_reports() {
        assert_eq!(parse_razer(&[0x04, 0x01, 0x55, 0, 0]), Some(vec![0x01, 0x55]));
        assert_eq!(parse_razer(&[0x04, 0, 0]), Some(vec![]));
        assert_eq!(parse_razer(&[0x07, 31, 9]), None);
    }

    const M: u16 = 7;

    fn with_macro(mode: MacroMode, count: u8) -> Engine {
        let mut e = bound(&[(A, Action::Macro { id: M, mode, count })]);
        let ms = Event::Delay { ms: 10 };
        e.cfg.macros.insert(M, vec![Event::Key { key: C, down: true }, ms, Event::Key { key: C, down: false }, ms]);
        e
    }

    fn at(t: Instant, ms: u64) -> Instant {
        t + Duration::from_millis(ms)
    }

    /// Presses A (`Some(true)`), releases it or leaves it, then ticks.
    fn step_at(e: &mut Engine, t: Instant, down: Option<bool>) -> Vec<Output> {
        let mut out = match down {
            Some(true) => e.feed_depth(&depth(&[(A, 150)]), t),
            Some(false) => e.feed_depth(&depth(&[]), t),
            None => Vec::new(),
        };
        out.extend(e.tick(t));
        out
    }

    #[test]
    fn macro_plays_its_passes_with_delays() {
        let (mut e, t) = (with_macro(MacroMode::Times, 2), Instant::now());
        assert_eq!(step_at(&mut e, t, Some(true)), [key(C, true)]);
        assert_eq!(e.deadline(), Some(at(t, 10)));
        assert!(step_at(&mut e, at(t, 5), Some(false)).is_empty());
        assert_eq!(step_at(&mut e, at(t, 10), None), [key(C, false)]);
        assert_eq!(step_at(&mut e, at(t, 21), None), [key(C, true)]);
        assert_eq!(step_at(&mut e, at(t, 31), None), [key(C, false)]);
        assert!(step_at(&mut e, at(t, 60), None).is_empty());
        assert_eq!(e.deadline(), None);
    }

    #[test]
    fn a_press_while_playing_times_is_ignored() {
        let (mut e, t) = (with_macro(MacroMode::Times, 1), Instant::now());
        step_at(&mut e, t, Some(true));
        step_at(&mut e, at(t, 1), Some(false));
        assert!(step_at(&mut e, at(t, 2), Some(true)).is_empty());
        assert_eq!(step_at(&mut e, at(t, 10), None), [key(C, false)]);
        assert!(step_at(&mut e, at(t, 30), None).is_empty());
    }

    #[test]
    fn hold_loops_until_release_and_finishes_the_pass() {
        let (mut e, t) = (with_macro(MacroMode::Hold, 1), Instant::now());
        step_at(&mut e, t, Some(true));
        step_at(&mut e, at(t, 10), None);
        assert_eq!(step_at(&mut e, at(t, 21), None), [key(C, true)]);
        assert!(step_at(&mut e, at(t, 25), Some(false)).is_empty());
        assert_eq!(step_at(&mut e, at(t, 31), None), [key(C, false)]);
        assert!(step_at(&mut e, at(t, 100), None).is_empty());
        assert_eq!(e.deadline(), None);
    }

    #[test]
    fn toggle_stops_on_the_next_press() {
        let (mut e, t) = (with_macro(MacroMode::Toggle, 1), Instant::now());
        step_at(&mut e, t, Some(true));
        step_at(&mut e, at(t, 1), Some(false));
        step_at(&mut e, at(t, 10), None);
        assert_eq!(step_at(&mut e, at(t, 21), None), [key(C, true)]);
        assert!(step_at(&mut e, at(t, 22), Some(true)).is_empty());
        assert_eq!(step_at(&mut e, at(t, 31), None), [key(C, false)]);
        assert!(step_at(&mut e, at(t, 100), None).is_empty());
    }

    #[test]
    fn release_all_lifts_what_a_macro_holds() {
        let (mut e, t) = (with_macro(MacroMode::Hold, 1), Instant::now());
        step_at(&mut e, t, Some(true));
        assert_eq!(e.release_all(), [key(C, false)]);
        assert_eq!(e.deadline(), None);
    }

    #[test]
    fn a_macro_without_delays_yields_between_passes() {
        let mut e = bound(&[(A, Action::Macro { id: M, mode: MacroMode::Hold, count: 1 })]);
        e.cfg.macros.insert(M, vec![Event::Key { key: C, down: true }, Event::Key { key: C, down: false }]);
        let t = Instant::now();
        assert_eq!(step_at(&mut e, t, Some(true)), [key(C, true), key(C, false)]);
        assert_eq!(e.deadline(), Some(at(t, 1)));
    }
}
