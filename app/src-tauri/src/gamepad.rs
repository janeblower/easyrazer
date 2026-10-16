use hidmaestro_client::{Buttons, State, Writer};
use razer_core::binding::Action;
use razer_core::hid::{self, Unit};
use razer_core::rapid::Config;
use std::os::windows::io::AsRawHandle;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Mutex, OnceLock};
#[cfg(test)]
use std::time::{Duration, Instant};
use windows_sys::Win32::Foundation::WAIT_OBJECT_0;
use windows_sys::Win32::System::Threading::{INFINITE, WaitForSingleObject};

mod ipc;
mod process;

const KEYS: [usize; 10] = [18, 31, 32, 33, 17, 19, 37, 38, 23, 24];

struct Runtime {
    pid: u16,
    container: Option<String>,
    writer: Mutex<Option<(Writer, State)>>,
    alive: AtomicBool,
}

static RUNTIME: OnceLock<Runtime> = OnceLock::new();

pub struct Session {
    _process: process::Process,
}

impl Drop for Session {
    fn drop(&mut self) {
        if let Some(runtime) = RUNTIME.get() {
            runtime.alive.store(false, Ordering::SeqCst);
            let mut slot = runtime.writer.lock().unwrap_or_else(|e| e.into_inner());
            if let Some((mut writer, _)) = slot.take()
                && let Err(e) = writer.submit(&State::default())
            {
                eprintln!("gamepad neutral: {e}");
            }
        }
    }
}

pub fn start() -> Result<Session, String> {
    let api = hidapi::HidApi::new().map_err(|e| e.to_string())?;
    let units = hid::units(&api);
    let [unit] = units.as_slice() else {
        return Err("--gamepad-test requires exactly one supported keyboard".into());
    };
    let (writer, process) = process::launch()?;
    let helper = match process.handle.try_clone() {
        Ok(h) => h,
        Err(e) => {
            drop(writer);
            return Err(e.to_string());
        }
    };
    RUNTIME
        .set(Runtime {
            pid: unit.spec.pid,
            container: unit.container.clone(),
            writer: Mutex::new(Some((writer, State::default()))),
            alive: AtomicBool::new(true),
        })
        .map_err(|_| "gamepad test already started")?;
    std::thread::spawn(move || {
        if unsafe { WaitForSingleObject(helper.as_raw_handle(), INFINITE) } == WAIT_OBJECT_0
            && let Some(runtime) = RUNTIME.get()
            && runtime.alive.swap(false, Ordering::SeqCst)
        {
            if let Some((mut writer, _)) = runtime.writer.lock().unwrap_or_else(|e| e.into_inner()).take() {
                let _ = writer.submit(&State::default());
            }
            eprintln!("gamepad helper exited");
            crate::wake::wake();
        }
    });
    Ok(Session { _process: process })
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
        let mut slot = r.writer.lock().unwrap_or_else(|e| e.into_inner());
        if !r.alive.load(Ordering::SeqCst) {
            return;
        }
        let state = from_depth(depth);
        let Some((writer, previous)) = slot.as_mut() else {
            return;
        };
        if state == *previous {
            return;
        }
        match writer.submit(&state) {
            Ok(()) => *previous = state,
            Err(e) => {
                let _ = writer.submit(&State::default());
                r.alive.store(false, Ordering::SeqCst);
                slot.take();
                eprintln!("gamepad input: {e}");
                crate::wake::wake();
            }
        }
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
    State {
        buttons,
        left_x: axis(33, 31),
        left_y: axis(18, 32),
        left_trigger: depth[17],
        right_trigger: depth[19],
        ..State::default()
    }
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
            snap: vec![SnapGroup {
                keys: vec![18, 34],
                rule: SnapRule::Last,
            }],
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
