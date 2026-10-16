use hidmaestro_client::{Buttons, State};
use razer_core::binding::Action;
use razer_core::hid::{self, Unit};
use razer_core::rapid::Config;
use std::io::{self, Read, Write};
use std::net::{Shutdown, TcpStream};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

mod ipc;
mod process;

const KEYS: [usize; 10] = [18, 31, 32, 33, 17, 19, 37, 38, 23, 24];
const TIMEOUT: Duration = Duration::from_millis(500);

struct Runtime {
    pid: u16,
    container: Option<String>,
    state: Arc<Mutex<State>>,
    alive: Arc<AtomicBool>,
}

static RUNTIME: OnceLock<Runtime> = OnceLock::new();

pub struct Session {
    stream: TcpStream,
    stop: Arc<AtomicBool>,
    thread: Option<JoinHandle<()>>,
    process: process::Process,
}

impl Drop for Session {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::SeqCst);
        let _ = self.stream.shutdown(Shutdown::Both);
        if let Some(t) = self.thread.take() {
            let _ = t.join();
        }
        self.process.wait();
    }
}

pub fn start() -> Result<Session, String> {
    let api = hidapi::HidApi::new().map_err(|e| e.to_string())?;
    let units = hid::units(&api);
    let [unit] = units.as_slice() else {
        return Err("--gamepad-test requires exactly one supported keyboard".into());
    };
    let (mut stream, process) = process::launch()?;
    let state = Arc::new(Mutex::new(State::default()));
    let alive = Arc::new(AtomicBool::new(true));
    RUNTIME
        .set(Runtime { pid: unit.spec.pid, container: unit.container.clone(), state: state.clone(), alive: alive.clone() })
        .map_err(|_| "gamepad test already started")?;
    let stop = Arc::new(AtomicBool::new(false));
    let flag = stop.clone();
    let socket = stream.try_clone().map_err(|e| e.to_string())?;
    let task = thread::spawn(move || {
        let result = (|| -> io::Result<()> {
            let mut previous = None;
            let mut sent = Instant::now();
            while !flag.load(Ordering::SeqCst) {
                let next = *state.lock().unwrap_or_else(|e| e.into_inner());
                if previous != Some(next) || sent.elapsed() >= Duration::from_millis(100) {
                    stream.write_all(&ipc::encode(next))?;
                    let mut ack = [255];
                    stream.read_exact(&mut ack)?;
                    if ack != [0] {
                        return Err(io::Error::other("invalid gamepad acknowledgement"));
                    }
                    previous = Some(next);
                    sent = Instant::now();
                }
                thread::sleep(Duration::from_millis(4));
            }
            Ok(())
        })();
        alive.store(false, Ordering::SeqCst);
        let _ = stream.shutdown(Shutdown::Both);
        if let Err(e) = result
            && !flag.load(Ordering::SeqCst)
        {
            eprintln!("gamepad helper: {e}");
            crate::wake::wake();
        }
    });
    Ok(Session { stream: socket, stop, thread: Some(task), process })
}

pub fn enabled_for(unit: &Unit) -> bool {
    RUNTIME.get().is_some_and(|r| r.pid == unit.spec.pid && r.container == unit.container)
}

pub fn healthy() -> bool {
    RUNTIME.get().is_some_and(|r| r.alive.load(Ordering::SeqCst))
}

pub fn configure(cfg: &mut Config) {
    for key in KEYS {
        cfg.bind[key] = Some(Action::Disabled);
        cfg.fn_bind[key] = Some(Action::Disabled);
    }
    cfg.snap.retain(|group| !group.keys.iter().any(|k| KEYS.contains(&(*k as usize))));
}

pub fn feed(depth: &[u8; 256]) {
    if let Some(r) = RUNTIME.get() {
        *r.state.lock().unwrap_or_else(|e| e.into_inner()) = from_depth(depth);
    }
}

pub fn neutral() {
    feed(&[0; 256]);
}

pub fn helper(args: &[String]) -> Result<(), String> {
    process::helper(args).map_err(|e| e.to_string())
}

fn from_depth(depth: &[u8; 256]) -> State {
    let axis = |positive: usize, negative: usize| {
        let d = i32::from(depth[positive]) - i32::from(depth[negative]);
        (d * if d < 0 { 32768 } else { 32767 } / 255) as i16
    };
    let mut buttons = Buttons::default();
    for (key, button) in [(37, Buttons::A), (38, Buttons::B), (23, Buttons::X), (24, Buttons::Y)] {
        if depth[key] >= 128 {
            buttons = buttons | button;
        }
    }
    State { buttons, left_x: axis(33, 31), left_y: axis(18, 32), left_trigger: depth[17], right_trigger: depth[19], ..State::default() }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mapped_keys_do_not_type_or_participate_in_snap_tap() {
        use razer_core::rapid::{Engine, Output, SnapGroup, SnapRule};
        let mut cfg = Config {
            act: [128; 256],
            fn_act: [128; 256],
            rapid: [None; 256],
            bind: [None; 256],
            fn_bind: [None; 256],
            repeat_delay: Duration::from_millis(500),
            repeat_interval: Duration::from_millis(30),
            macros: Default::default(),
            margin: 1,
            snap_on: true,
            snap: vec![SnapGroup { keys: vec![18, 34], rule: SnapRule::Last }],
        };
        configure(&mut cfg);
        assert!(cfg.snap.is_empty());
        for key in KEYS {
            assert_eq!(cfg.fn_bind[key], Some(Action::Disabled));
        }
        let mut depth = [0; 256];
        for key in KEYS {
            depth[key] = 255;
        }
        depth[34] = 255;
        let mut engine = Engine::new(cfg);
        let out = engine.feed_depth(&depth, Instant::now());
        assert_eq!(out, [Output::Key { key: 34, down: true }]);
    }

    #[test]
    fn analog_axes_and_triggers_follow_depth() {
        let mut depth = [0; 256];
        depth[18] = 255;
        depth[31] = 255;
        depth[17] = 128;
        depth[19] = 255;
        let state = from_depth(&depth);
        assert_eq!((state.left_x, state.left_y), (-32768, 32767));
        assert_eq!((state.left_trigger, state.right_trigger), (128, 255));
        depth[33] = 255;
        depth[32] = 255;
        let state = from_depth(&depth);
        assert_eq!((state.left_x, state.left_y), (0, 0));
        assert_eq!(from_depth(&[0; 256]), State::default());
    }

    #[test]
    fn buttons_press_at_half_travel() {
        let mut depth = [0; 256];
        for key in [37, 38, 23, 24] {
            depth[key] = 127;
        }
        assert_eq!(from_depth(&depth).buttons, Buttons::default());
        for key in [37, 38, 23, 24] {
            depth[key] = 128;
        }
        assert_eq!(from_depth(&depth).buttons, Buttons::A | Buttons::B | Buttons::X | Buttons::Y);
    }
}
