//! The key engine on Windows: reads depth and Razer reports, types with `SendInput`.

use std::panic::AssertUnwindSafe;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, MutexGuard};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use hidapi::{HidApi, HidDevice};
use razer_core::hid::{self, Unit};
use razer_core::binding::Mouse;
use razer_core::profiles;
use razer_core::analog::DepthReport;
use razer_core::rapid::{self, Config, Engine, Media, Output};
use windows_sys::Win32::System::Power::SetSuspendState;
use windows_sys::Win32::System::Shutdown::LockWorkStation;
use windows_sys::Win32::UI::Input::KeyboardAndMouse::{
    GetAsyncKeyState, INPUT, INPUT_0, INPUT_KEYBOARD, INPUT_MOUSE, KEYBDINPUT, KEYEVENTF_EXTENDEDKEY, KEYEVENTF_KEYUP, KEYEVENTF_SCANCODE,
    MOUSEEVENTF_LEFTDOWN, MOUSEEVENTF_LEFTUP, MOUSEEVENTF_MIDDLEDOWN, MOUSEEVENTF_MIDDLEUP, MOUSEEVENTF_RIGHTDOWN,
    MOUSEEVENTF_RIGHTUP, MOUSEEVENTF_WHEEL, MOUSEEVENTF_XDOWN, MOUSEEVENTF_XUP, MOUSEINPUT, SendInput,
    VK_MEDIA_NEXT_TRACK, VK_MEDIA_PLAY_PAUSE, VK_MEDIA_PREV_TRACK, VK_MEDIA_STOP, VK_PAUSE, VK_VOLUME_DOWN,
    VK_LWIN, VK_RWIN, VK_VOLUME_MUTE, VK_VOLUME_UP,
};
use windows_sys::Win32::UI::WindowsAndMessaging::{WHEEL_DELTA, XBUTTON1, XBUTTON2};
use windows_sys::Win32::UI::WindowsAndMessaging::{SPI_GETKEYBOARDDELAY, SPI_GETKEYBOARDSPEED, SystemParametersInfoW};

use crate::wheel::Wheel;

/// Actions the engine cannot perform itself: they need the control channel or the OS.
pub type Sink = Arc<dyn Fn(Output) + Send + Sync>;

/// Longest a reader blocks, so a stop request is noticed.
const POLL: Duration = Duration::from_millis(100);
const PAUSE: u8 = 126;
const L: u8 = 39;

type Feed = Box<dyn Fn(&mut Engine, &[u8], Instant) -> Vec<Output> + Send>;

pub struct EngineHandle {
    engine: Arc<Mutex<Engine>>,
    stop: Arc<AtomicBool>,
    threads: Vec<JoinHandle<()>>,
    sink: Sink,
    _wheel: Wheel,
    gamepad: Arc<Mutex<crate::gamepad::Runtime>>,
}

fn lock(e: &Mutex<Engine>) -> MutexGuard<'_, Engine> {
    e.lock().unwrap_or_else(|e| e.into_inner())
}

fn open(api: &HidApi, unit: &Unit, col: &str) -> Result<HidDevice, String> {
    hid::interfaces(api, unit)
        .find(|d| d.interface_number() == 1 && d.path().to_string_lossy().to_lowercase().contains(col))
        .ok_or_else(|| format!("MI_01 {col} not found"))?
        .open_device(api)
        .map_err(|e| e.to_string())
}

/// Reads Col04 outside driver mode: Fn+Menu then reports `NEXT_PROFILE_CODE` instead of switching slots.
pub struct MenuListener {
    stop: Arc<AtomicBool>,
    thread: Option<JoinHandle<()>>,
}

impl MenuListener {
    /// Opens Col04 and reports each Fn+Menu press to `sink` until dropped.
    pub fn start(api: &HidApi, unit: &Unit, sink: Sink) -> Result<Self, String> {
        let dev = open(api, unit, "col04")?;
        let stop = Arc::new(AtomicBool::new(false));
        let flag = stop.clone();
        let thread = std::thread::spawn(move || {
            let mut held = Vec::new();
            let mut buf = [0u8; 64];
            while !flag.load(Ordering::SeqCst) {
                let Ok(n) = dev.read_timeout(&mut buf, POLL.as_millis() as i32) else { return };
                let Some(codes) = (n > 0).then(|| rapid::parse_razer(&buf[..n])).flatten() else { continue };
                if profiles::menu_pressed(&held, &codes) {
                    sink(Output::NextProfile);
                }
                held = codes;
            }
        });
        Ok(Self { stop, thread: Some(thread) })
    }

    /// The reader thread is still running; it ends when the keyboard goes away.
    pub fn alive(&self) -> bool {
        self.thread.as_ref().is_some_and(|t| !t.is_finished())
    }
}

impl Drop for MenuListener {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::SeqCst);
        if let Some(t) = self.thread.take() {
            let _ = t.join();
        }
    }
}

impl EngineHandle {
    /// Opens the report collections and starts reading; the keyboard only reports once in driver mode.
    pub fn start(api: &HidApi, unit: &Unit, mut cfg: Config, sink: Sink, profile: u32, gamepad_config: crate::gamepad::Config, launch: bool, gamepad: Arc<Mutex<crate::gamepad::Runtime>>) -> Result<Self, String> {
        let report = &unit.spec.analog.depth;
        let depth = open(api, unit, &report.collection)?;
        let razer = open(api, unit, "col04")?;
        // After the readers: a helper started for an engine that never runs would get no input.
        let mut runtime = gamepad.lock().unwrap_or_else(|e| e.into_inner());
        runtime.configure(profile, gamepad_config, launch);
        runtime.config.configure(&mut cfg);
        drop(runtime);
        let engine = Arc::new(Mutex::new(Engine::new(cfg)));
        let stop = Arc::new(AtomicBool::new(false));
        let spawn = |dev: HidDevice, feed: Feed| {
            let (engine, stop, sink, gamepad) = (engine.clone(), stop.clone(), sink.clone(), gamepad.clone());
            std::thread::spawn(move || {
                let _ = std::panic::catch_unwind(AssertUnwindSafe(|| read(&dev, &engine, &stop, &sink, feed)));
                // Not stopped by its owner: the watcher replaces the engine.
                if !stop.swap(true, Ordering::SeqCst) {
                    crate::wake::wake();
                }
                let out = {
                    let mut e = lock(&engine);
                    gamepad.lock().unwrap_or_else(|e| e.into_inner()).neutral();
                    e.release_all()
                };
                emit(&out, &sink);
            })
        };
        let threads = vec![spawn(depth, Box::new(feed_depth(report.clone(), gamepad.clone()))), spawn(razer, Box::new(feed_razer))];
        Ok(Self { engine, stop, threads, sink, _wheel: Wheel::start(unit), gamepad })
    }

    /// Stops typing and releases the held keys now; the readers exit only after their current read.
    /// The gamepad only goes neutral: the next engine reuses it without another UAC prompt.
    pub fn halt(&self) {
        let out = {
            let mut e = lock(&self.engine);
            self.stop.store(true, Ordering::SeqCst);
            self.gamepad.lock().unwrap_or_else(|e| e.into_inner()).neutral();
            e.release_all()
        };
        emit(&out, &self.sink);
    }

    pub fn set_config(&self, mut cfg: Config, profile: u32, config: crate::gamepad::Config, launch: bool) {
        let mut e = lock(&self.engine);
        let mut gamepad = self.gamepad.lock().unwrap_or_else(|e| e.into_inner());
        if gamepad.config != config || gamepad.profile != profile || (config.enabled && launch) {
            emit(&e.release_outputs(), &self.sink);
        }
        gamepad.configure(profile, config, launch);
        gamepad.config.configure(&mut cfg);
        e.set_config(cfg);
    }

    pub fn gamepad_input(&self) -> crate::gamepad::Input {
        self.gamepad.lock().unwrap_or_else(|e| e.into_inner()).input()
    }

    /// `false` once a reader lost the keyboard; the handle is then replaced.
    pub fn alive(&self) -> bool {
        !self.threads.iter().any(JoinHandle::is_finished)
    }
}

impl Drop for EngineHandle {
    fn drop(&mut self) {
        self.halt();
        for t in self.threads.drain(..) {
            let _ = t.join();
        }
    }
}

fn feed_depth(r: DepthReport, gamepad: Arc<Mutex<crate::gamepad::Runtime>>) -> impl Fn(&mut Engine, &[u8], Instant) -> Vec<Output> {
    move |e, report, now| {
        let Some(d) = rapid::parse_depth(report, &r) else { return Vec::new() };
        gamepad.lock().unwrap_or_else(|e| e.into_inner()).feed(&d);
        let mut out = e.feed_depth(&d, now);
        out.extend(e.lead(&d));
        out
    }
}

fn feed_razer(e: &mut Engine, report: &[u8], _: Instant) -> Vec<Output> {
    rapid::parse_razer(report).map_or_else(Vec::new, |c| e.feed_razer(&c))
}

fn read(dev: &HidDevice, engine: &Mutex<Engine>, stop: &AtomicBool, sink: &Sink, feed: Feed) {
    let mut buf = [0u8; 64];
    while !stop.load(Ordering::SeqCst) {
        let wait = lock(engine).deadline().map_or(POLL, |d| d.saturating_duration_since(Instant::now()).min(POLL));
        // Round up: a zero timeout would spin until the repeat is due.
        let Ok(n) = dev.read_timeout(&mut buf, wait.as_micros().div_ceil(1000) as i32) else { return };
        let now = Instant::now();
        let out = {
            let mut e = lock(engine);
            // The other reader may have released every key already.
            if stop.load(Ordering::SeqCst) {
                return;
            }
            let mut out = if n > 0 { feed(&mut e, &buf[..n], now) } else { Vec::new() };
            out.extend(e.tick(now));
            out
        };
        emit(&out, sink);
    }
}

fn emit(out: &[Output], sink: &Sink) {
    let mut inputs = Vec::new();
    for &o in out {
        match o {
            Output::Key { key, down } => key_input(key, down, &mut inputs),
            Output::Mouse { button, down } => inputs.extend(mouse_input(button, down)),
            Output::Media(m) => media_input(m, &mut inputs),
            _ => {}
        }
    }
    // One call per report: a call costs about 0.5 ms however many events it carries.
    send(&inputs);
    // Windows ignores an injected Win+L; the L still goes out, or the Win release opens Start.
    let win = || [VK_LWIN, VK_RWIN].iter().any(|&vk| unsafe { GetAsyncKeyState(i32::from(vk)) } < 0);
    if out.iter().any(|o| matches!(o, Output::Key { key: L, down: true })) && win() {
        unsafe { LockWorkStation() };
    }
    for &o in out {
        if !matches!(o, Output::Key { .. } | Output::Mouse { .. } | Output::Media(_)) {
            sink(o);
        }
    }
}

fn send(inputs: &[INPUT]) {
    if !inputs.is_empty() {
        unsafe { SendInput(inputs.len() as u32, inputs.as_ptr(), size_of::<INPUT>() as i32) };
    }
}

fn mouse_input(button: Mouse, down: bool) -> Option<INPUT> {
    let pick = |d, u| if down { d } else { u };
    let (flags, data) = match button {
        Mouse::Left => (pick(MOUSEEVENTF_LEFTDOWN, MOUSEEVENTF_LEFTUP), 0),
        Mouse::Right => (pick(MOUSEEVENTF_RIGHTDOWN, MOUSEEVENTF_RIGHTUP), 0),
        Mouse::Middle => (pick(MOUSEEVENTF_MIDDLEDOWN, MOUSEEVENTF_MIDDLEUP), 0),
        Mouse::Back => (pick(MOUSEEVENTF_XDOWN, MOUSEEVENTF_XUP), u32::from(XBUTTON1)),
        Mouse::Forward => (pick(MOUSEEVENTF_XDOWN, MOUSEEVENTF_XUP), u32::from(XBUTTON2)),
        // One notch per press, like the firmware.
        Mouse::WheelUp | Mouse::WheelDown if !down => return None,
        Mouse::WheelUp => (MOUSEEVENTF_WHEEL, WHEEL_DELTA),
        Mouse::WheelDown => (MOUSEEVENTF_WHEEL, WHEEL_DELTA.wrapping_neg()),
    };
    let mi = MOUSEINPUT { dx: 0, dy: 0, mouseData: data, dwFlags: flags, time: 0, dwExtraInfo: 0 };
    Some(INPUT { r#type: INPUT_MOUSE, Anonymous: INPUT_0 { mi } })
}

fn keyboard(ki: KEYBDINPUT) -> INPUT {
    INPUT { r#type: INPUT_KEYBOARD, Anonymous: INPUT_0 { ki } }
}

fn key_input(key: u8, down: bool, inputs: &mut Vec<INPUT>) {
    let up = if down { 0 } else { KEYEVENTF_KEYUP };
    if key == PAUSE {
        return inputs.push(keyboard(KEYBDINPUT { wVk: VK_PAUSE, wScan: 0, dwFlags: up, time: 0, dwExtraInfo: 0 }));
    }
    let Some(sc) = scancode(key) else { return };
    let ext = if sc > 0xFF { KEYEVENTF_EXTENDEDKEY } else { 0 };
    inputs.push(keyboard(KEYBDINPUT { wVk: 0, wScan: sc & 0xFF, dwFlags: KEYEVENTF_SCANCODE | ext | up, time: 0, dwExtraInfo: 0 }));
}

fn media_input(m: Media, inputs: &mut Vec<INPUT>) {
    let vk = match m {
        Media::Prev => VK_MEDIA_PREV_TRACK,
        Media::Play => VK_MEDIA_PLAY_PAUSE,
        Media::Next => VK_MEDIA_NEXT_TRACK,
        Media::Stop => VK_MEDIA_STOP,
        Media::Mute => VK_VOLUME_MUTE,
        Media::VolumeUp => VK_VOLUME_UP,
        Media::VolumeDown => VK_VOLUME_DOWN,
    };
    for flags in [KEYEVENTF_EXTENDEDKEY, KEYEVENTF_EXTENDEDKEY | KEYEVENTF_KEYUP] {
        inputs.push(keyboard(KEYBDINPUT { wVk: vk, wScan: 0, dwFlags: flags, time: 0, dwExtraInfo: 0 }));
    }
}

pub(crate) fn send_media(m: Media) {
    let mut inputs = Vec::new();
    media_input(m, &mut inputs);
    send(&inputs);
}

pub fn sleep() {
    unsafe { SetSuspendState(false, false, false) };
}

/// Windows' own key repeat, which injected keys do not get.
pub fn repeat_timing() -> (Duration, Duration) {
    let (mut delay, mut speed) = (1u32, 31u32);
    unsafe {
        SystemParametersInfoW(SPI_GETKEYBOARDDELAY, 0, (&raw mut delay).cast(), 0);
        SystemParametersInfoW(SPI_GETKEYBOARDSPEED, 0, (&raw mut speed).cast(), 0);
    }
    timing(delay, speed)
}

/// Delay 0..=3 is 250..=1000 ms; speed 0..=31 is about 2.5..=30 repeats a second.
fn timing(delay: u32, speed: u32) -> (Duration, Duration) {
    let rate = 2.5 + f64::from(speed.min(31)) * 27.5 / 31.0;
    (Duration::from_millis(250 * (u64::from(delay.min(3)) + 1)), Duration::from_micros((1e6 / rate).round() as u64))
}

/// Set-1 scancode of a fwID; `0xE0xx` marks an extended key.
fn scancode(id: u8) -> Option<u16> {
    Some(match id {
        1 => 0x29,
        2..=11 => 0x02 + u16::from(id - 2),
        12 => 0x0C,
        13 => 0x0D,
        14 => 0x7D,
        15 => 0x0E,
        16 => 0x0F,
        17..=26 => 0x10 + u16::from(id - 17),
        27 => 0x1A,
        28 => 0x1B,
        29 | 42 => 0x2B,
        30 => 0x3A,
        31..=39 => 0x1E + u16::from(id - 31),
        40 => 0x27,
        41 => 0x28,
        43 => 0x1C,
        44 => 0x2A,
        45 => 0x56,
        46..=52 => 0x2C + u16::from(id - 46),
        53 => 0x33,
        54 => 0x34,
        55 => 0x35,
        56 => 0x73,
        57 => 0x36,
        58 => 0x1D,
        59 => 0xE05C,
        60 => 0x38,
        61 => 0x39,
        62 => 0xE038,
        64 => 0xE01D,
        75 => 0xE052,
        76 => 0xE053,
        79 => 0xE04B,
        80 => 0xE047,
        81 => 0xE04F,
        83 => 0xE048,
        84 => 0xE050,
        85 => 0xE049,
        86 => 0xE051,
        89 => 0xE04D,
        90 => 0x45,
        91 => 0x47,
        92 => 0x4B,
        93 => 0x4F,
        95 => 0xE035,
        96 => 0x48,
        97 => 0x4C,
        98 => 0x50,
        99 => 0x52,
        100 => 0x37,
        101 => 0x49,
        102 => 0x4D,
        103 => 0x51,
        104 => 0x53,
        105 => 0x4A,
        106 => 0x4E,
        107 => 0x59,
        108 => 0xE01C,
        110 => 0x01,
        112..=121 => 0x3B + u16::from(id - 112),
        122 => 0x57,
        123 => 0x58,
        124 => 0xE037,
        125 => 0x46,
        127 => 0xE05B,
        129 => 0xE05D,
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use razer_core::layout::Board;

    #[test]
    fn every_editable_key_has_an_output() {
        for board in [Board::Full, Board::Mini, Board::Tkl, Board::ProTkl] {
            for k in razer_core::layout::keys(board).into_iter().filter(|k| k.editable) {
                assert!(scancode(k.key).is_some() || k.key == PAUSE, "fwID {} {}", k.key, k.label);
            }
        }
    }

    #[test]
    fn lock_key_is_l() {
        assert_eq!(razer_core::keymap::by_name("L"), Some(L));
    }

    #[test]
    fn repeat_timing_follows_windows_ranges() {
        assert_eq!(timing(0, 0), (Duration::from_millis(250), Duration::from_millis(400)));
        assert_eq!(timing(1, 31), (Duration::from_millis(500), Duration::from_micros(33_333)));
        assert_eq!(timing(9, 99), timing(3, 31));
    }
}
