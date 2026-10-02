//! The key engine on Windows: reads depth and Razer reports, types with `SendInput`.

use std::panic::AssertUnwindSafe;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, MutexGuard};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use hidapi::{HidApi, HidDevice};
use razer_core::hid::VID;
use razer_core::binding::Mouse;
use razer_core::profiles;
use razer_core::rapid::{self, Config, Engine, Media, Output};
use windows_sys::Win32::System::Power::SetSuspendState;
use windows_sys::Win32::UI::Input::KeyboardAndMouse::{
    INPUT, INPUT_0, INPUT_KEYBOARD, INPUT_MOUSE, KEYBDINPUT, KEYEVENTF_EXTENDEDKEY, KEYEVENTF_KEYUP, KEYEVENTF_SCANCODE,
    MOUSEEVENTF_LEFTDOWN, MOUSEEVENTF_LEFTUP, MOUSEEVENTF_MIDDLEDOWN, MOUSEEVENTF_MIDDLEUP, MOUSEEVENTF_RIGHTDOWN,
    MOUSEEVENTF_RIGHTUP, MOUSEEVENTF_WHEEL, MOUSEEVENTF_XDOWN, MOUSEEVENTF_XUP, MOUSEINPUT, SendInput,
    VK_MEDIA_NEXT_TRACK, VK_MEDIA_PLAY_PAUSE, VK_MEDIA_PREV_TRACK, VK_MEDIA_STOP, VK_PAUSE, VK_VOLUME_DOWN,
    VK_VOLUME_MUTE, VK_VOLUME_UP,
};
use windows_sys::Win32::UI::WindowsAndMessaging::{WHEEL_DELTA, XBUTTON1, XBUTTON2};
use windows_sys::Win32::UI::WindowsAndMessaging::{SPI_GETKEYBOARDDELAY, SPI_GETKEYBOARDSPEED, SystemParametersInfoW};

/// Actions the engine cannot perform itself: they need the control channel or the OS.
pub type Sink = Arc<dyn Fn(Output) + Send + Sync>;

/// Longest a reader blocks, so a stop request is noticed.
const POLL: Duration = Duration::from_millis(100);
const PAUSE: u8 = 126;

type Feed = fn(&mut Engine, &[u8], Instant) -> Vec<Output>;

pub struct EngineHandle {
    engine: Arc<Mutex<Engine>>,
    stop: Arc<AtomicBool>,
    threads: Vec<JoinHandle<()>>,
    sink: Sink,
}

fn lock(e: &Mutex<Engine>) -> MutexGuard<'_, Engine> {
    e.lock().unwrap_or_else(|e| e.into_inner())
}

fn open(api: &HidApi, pid: u16, col: &str) -> Result<HidDevice, String> {
    api.device_list()
        .find(|d| {
            d.vendor_id() == VID
                && d.product_id() == pid
                && d.interface_number() == 1
                && d.path().to_string_lossy().to_lowercase().contains(col)
        })
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
    pub fn start(api: &HidApi, pid: u16, sink: Sink) -> Result<Self, String> {
        let dev = open(api, pid, "col04")?;
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
    pub fn start(api: &HidApi, pid: u16, cfg: Config, sink: Sink) -> Result<Self, String> {
        let depth = open(api, pid, "col06")?;
        let razer = open(api, pid, "col04")?;
        let engine = Arc::new(Mutex::new(Engine::new(cfg)));
        let stop = Arc::new(AtomicBool::new(false));
        let spawn = |dev: HidDevice, feed: Feed| {
            let (engine, stop, sink) = (engine.clone(), stop.clone(), sink.clone());
            std::thread::spawn(move || {
                let _ = std::panic::catch_unwind(AssertUnwindSafe(|| read(&dev, &engine, &stop, &sink, feed)));
                stop.store(true, Ordering::SeqCst);
                let out = lock(&engine).release_all();
                emit(&out, &sink);
            })
        };
        let threads = vec![spawn(depth, feed_depth), spawn(razer, feed_razer)];
        Ok(Self { engine, stop, threads, sink })
    }

    /// Stops typing and releases the held keys now; the readers exit only after their current read.
    pub fn halt(&self) {
        let out = {
            let mut e = lock(&self.engine);
            self.stop.store(true, Ordering::SeqCst);
            e.release_all()
        };
        emit(&out, &self.sink);
    }

    pub fn set_config(&self, cfg: Config) {
        lock(&self.engine).set_config(cfg);
    }

    /// `false` once a reader lost the keyboard; the handle is then replaced.
    pub fn alive(&self) -> bool {
        !self.threads.iter().any(JoinHandle::is_finished)
    }
}

impl Drop for EngineHandle {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::SeqCst);
        for t in self.threads.drain(..) {
            let _ = t.join();
        }
    }
}

fn feed_depth(e: &mut Engine, report: &[u8], now: Instant) -> Vec<Output> {
    let Some(d) = rapid::parse_depth(report) else { return Vec::new() };
    let mut out = e.feed_depth(&d, now);
    out.extend(e.lead(&d));
    out
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
    for &o in out {
        match o {
            Output::Key { key, down } => send_key(key, down),
            Output::Mouse { button, down } => send_mouse(button, down),
            Output::Media(m) => send_media(m),
            other => sink(other),
        }
    }
}

fn send_mouse(button: Mouse, down: bool) {
    let pick = |d, u| if down { d } else { u };
    let (flags, data) = match button {
        Mouse::Left => (pick(MOUSEEVENTF_LEFTDOWN, MOUSEEVENTF_LEFTUP), 0),
        Mouse::Right => (pick(MOUSEEVENTF_RIGHTDOWN, MOUSEEVENTF_RIGHTUP), 0),
        Mouse::Middle => (pick(MOUSEEVENTF_MIDDLEDOWN, MOUSEEVENTF_MIDDLEUP), 0),
        Mouse::Back => (pick(MOUSEEVENTF_XDOWN, MOUSEEVENTF_XUP), u32::from(XBUTTON1)),
        Mouse::Forward => (pick(MOUSEEVENTF_XDOWN, MOUSEEVENTF_XUP), u32::from(XBUTTON2)),
        // One notch per press, like the firmware.
        Mouse::WheelUp | Mouse::WheelDown if !down => return,
        Mouse::WheelUp => (MOUSEEVENTF_WHEEL, WHEEL_DELTA),
        Mouse::WheelDown => (MOUSEEVENTF_WHEEL, WHEEL_DELTA.wrapping_neg()),
    };
    let mi = MOUSEINPUT { dx: 0, dy: 0, mouseData: data, dwFlags: flags, time: 0, dwExtraInfo: 0 };
    let input = INPUT { r#type: INPUT_MOUSE, Anonymous: INPUT_0 { mi } };
    unsafe { SendInput(1, &input, size_of::<INPUT>() as i32) };
}

fn send(ki: KEYBDINPUT) {
    let input = INPUT { r#type: INPUT_KEYBOARD, Anonymous: INPUT_0 { ki } };
    unsafe { SendInput(1, &input, size_of::<INPUT>() as i32) };
}

fn send_key(key: u8, down: bool) {
    let up = if down { 0 } else { KEYEVENTF_KEYUP };
    if key == PAUSE {
        return send(KEYBDINPUT { wVk: VK_PAUSE, wScan: 0, dwFlags: up, time: 0, dwExtraInfo: 0 });
    }
    let Some(sc) = scancode(key) else { return };
    let ext = if sc > 0xFF { KEYEVENTF_EXTENDEDKEY } else { 0 };
    send(KEYBDINPUT { wVk: 0, wScan: sc & 0xFF, dwFlags: KEYEVENTF_SCANCODE | ext | up, time: 0, dwExtraInfo: 0 });
}

fn send_media(m: Media) {
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
        send(KEYBDINPUT { wVk: vk, wScan: 0, dwFlags: flags, time: 0, dwExtraInfo: 0 });
    }
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

    #[test]
    fn every_editable_key_has_an_output() {
        for k in razer_core::layout::keys().into_iter().filter(|k| k.editable) {
            assert!(scancode(k.key).is_some() || k.key == PAUSE, "fwID {} {}", k.key, k.label);
        }
    }

    #[test]
    fn repeat_timing_follows_windows_ranges() {
        assert_eq!(timing(0, 0), (Duration::from_millis(250), Duration::from_millis(400)));
        assert_eq!(timing(1, 31), (Duration::from_millis(500), Duration::from_micros(33_333)));
        assert_eq!(timing(9, 99), timing(3, 31));
    }
}
