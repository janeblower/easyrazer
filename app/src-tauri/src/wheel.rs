//! In driver mode the firmware reports the volume wheel as a mouse wheel on MI_02.
//! A low-level hook swallows those notches and sends volume keys instead.
//!
//! The hook cannot tell devices apart, but Raw Input can, and Windows queues the
//! `WM_INPUT` of a wheel notch before it calls the hook for it (checked on hardware).

use std::cell::Cell;
use std::collections::{HashMap, VecDeque};
use std::sync::{LazyLock, Mutex, MutexGuard};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use razer_core::hid::{self, Unit};
use razer_core::rapid::Media;
use windows_sys::Win32::Foundation::{HANDLE, HWND, LPARAM, LRESULT, WPARAM};
use windows_sys::Win32::System::Threading::GetCurrentThreadId;
use windows_sys::Win32::UI::Input::{
    GetRawInputData, GetRawInputDeviceInfoW, HRAWINPUT, RAWINPUT, RAWINPUTDEVICE, RAWINPUTHEADER, RID_INPUT,
    RIDEV_INPUTSINK, RIDEV_REMOVE, RIDI_DEVICENAME, RegisterRawInputDevices,
};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    CallNextHookEx, CreateWindowExW, DefWindowProcW, DestroyWindow, DispatchMessageW, GetMessageW, HWND_MESSAGE,
    LLMHF_INJECTED, MSG, MSLLHOOKSTRUCT, PM_REMOVE, PeekMessageW, PostThreadMessageW, RI_MOUSE_WHEEL, RegisterClassW,
    SetWindowsHookExW, UnhookWindowsHookEx, WH_MOUSE_LL, WHEEL_DELTA, WM_INPUT, WM_MOUSEWHEEL, WM_QUIT, WNDCLASSW,
};

use crate::engine;

/// A queued notch older than this lost its hook call to another hook that swallowed it.
const STALE: Duration = Duration::from_millis(100);

/// The keyboards whose wheel is handled. Windows keeps one Raw Input target per usage and
/// process, and the hook sees every notch, so all keyboards share one window and one hook.
#[derive(Default)]
struct Registry {
    units: Vec<(u64, Unit)>,
    next: u64,
    /// Which unit each Raw Input device belongs to; resolving one takes CfgMgr calls.
    owners: HashMap<isize, Option<u64>>,
    /// Wheel notches Raw Input saw, oldest first: when, and the unit that sent it.
    seen: VecDeque<(Instant, Option<u64>)>,
}

impl Registry {
    /// Returns the unit's id and whether it is the first, which starts Raw Input.
    fn add(&mut self, unit: Unit) -> (u64, bool) {
        let first = self.units.is_empty();
        self.next += 1;
        self.units.push((self.next, unit));
        self.owners.clear();
        (self.next, first)
    }

    /// Returns whether no unit is left, which stops Raw Input.
    fn remove(&mut self, id: u64) -> bool {
        self.units.retain(|(i, _)| *i != id);
        self.owners.clear();
        self.units.is_empty()
    }

    fn owner(&self, pid: u16, container: Option<&str>) -> Option<u64> {
        self.units.iter().find(|(_, u)| u.owns(pid, container)).map(|(i, _)| *i)
    }

    /// Takes the unit of the oldest notch that is not stale.
    fn take(&mut self) -> Option<u64> {
        while self.seen.front().is_some_and(|(at, _)| at.elapsed() > STALE) {
            self.seen.pop_front();
        }
        self.seen.pop_front().and_then(|(_, owner)| owner)
    }
}

static REGISTRY: LazyLock<Mutex<Registry>> = LazyLock::new(Mutex::default);
/// Held across a unit joining or leaving, so a new thread never starts while the old one still owns Raw Input.
static THREAD: Mutex<Option<(u32, JoinHandle<()>)>> = Mutex::new(None);

fn registry() -> MutexGuard<'static, Registry> {
    REGISTRY.lock().unwrap_or_else(|e| e.into_inner())
}

thread_local! {
    static WINDOW: Cell<HWND> = const { Cell::new(std::ptr::null_mut()) };
}

/// One keyboard's share of the wheel handling; the last one dropped stops it.
pub struct Wheel {
    id: u64,
}

impl Wheel {
    pub fn start(unit: &Unit) -> Self {
        let mut thread = THREAD.lock().unwrap_or_else(|e| e.into_inner());
        let (id, first) = registry().add(unit.clone());
        if first {
            let (tx, rx) = std::sync::mpsc::channel();
            let join = std::thread::spawn(move || {
                let _ = tx.send(unsafe { GetCurrentThreadId() });
                unsafe { run() };
            });
            *thread = Some((rx.recv().unwrap_or(0), join));
        }
        Self { id }
    }
}

impl Drop for Wheel {
    fn drop(&mut self) {
        let mut thread = THREAD.lock().unwrap_or_else(|e| e.into_inner());
        let last = registry().remove(self.id);
        if last && let Some((tid, join)) = thread.take() {
            // Posted before the loop starts it stays queued, so the loop still sees it.
            unsafe { PostThreadMessageW(tid, WM_QUIT, 0, 0) };
            let _ = join.join();
        }
    }
}

unsafe fn run() {
    unsafe {
        let class: Vec<u16> = "EasyRazerWheel\0".encode_utf16().collect();
        let wc = WNDCLASSW { lpfnWndProc: Some(window), lpszClassName: class.as_ptr(), ..std::mem::zeroed() };
        RegisterClassW(&wc);
        let hwnd = CreateWindowExW(0, class.as_ptr(), class.as_ptr(), 0, 0, 0, 0, 0, HWND_MESSAGE, std::ptr::null_mut(), std::ptr::null_mut(), std::ptr::null());
        if hwnd.is_null() {
            return;
        }
        let mut rid = RAWINPUTDEVICE { usUsagePage: 0x01, usUsage: 0x02, dwFlags: RIDEV_INPUTSINK, hwndTarget: hwnd };
        let size = size_of::<RAWINPUTDEVICE>() as u32;
        let hook = if RegisterRawInputDevices(&rid, 1, size) != 0 {
            WINDOW.set(hwnd);
            SetWindowsHookExW(WH_MOUSE_LL, Some(hook), std::ptr::null_mut(), 0)
        } else {
            std::ptr::null_mut()
        };
        let mut msg: MSG = std::mem::zeroed();
        while !hook.is_null() && GetMessageW(&mut msg, std::ptr::null_mut(), 0, 0) > 0 {
            DispatchMessageW(&msg);
        }
        if !hook.is_null() {
            UnhookWindowsHookEx(hook);
            rid.dwFlags = RIDEV_REMOVE;
            rid.hwndTarget = std::ptr::null_mut();
            RegisterRawInputDevices(&rid, 1, size);
        }
        WINDOW.set(std::ptr::null_mut());
        DestroyWindow(hwnd);
    }
}

unsafe extern "system" fn window(hwnd: HWND, msg: u32, w: WPARAM, l: LPARAM) -> LRESULT {
    if msg == WM_INPUT {
        unsafe { record(l as HRAWINPUT) };
    }
    unsafe { DefWindowProcW(hwnd, msg, w, l) }
}

/// Queues a wheel notch from Raw Input with its source.
unsafe fn record(handle: HRAWINPUT) {
    unsafe {
        let mut ri: RAWINPUT = std::mem::zeroed();
        let mut size = size_of::<RAWINPUT>() as u32;
        let header = size_of::<RAWINPUTHEADER>() as u32;
        if GetRawInputData(handle, RID_INPUT, (&raw mut ri).cast(), &mut size, header) == u32::MAX {
            return;
        }
        if u32::from(ri.data.mouse.Anonymous.Anonymous.usButtonFlags) & RI_MOUSE_WHEEL == 0 {
            return;
        }
        let device = ri.header.hDevice;
        let mut r = registry();
        let owner = match r.owners.get(&(device as isize)) {
            Some(o) => *o,
            None => {
                let o = identify(device).and_then(|(pid, container)| r.owner(pid, container.as_deref()));
                r.owners.insert(device as isize, o);
                o
            }
        };
        r.seen.push_back((Instant::now(), owner));
    }
}

/// Raw Input names are device interface paths, so they resolve to a container like hidapi's.
unsafe fn identify(device: HANDLE) -> Option<(u16, Option<String>)> {
    let mut name = [0u16; 256];
    let mut len = name.len() as u32;
    if unsafe { GetRawInputDeviceInfoW(device, RIDI_DEVICENAME, name.as_mut_ptr().cast(), &mut len) } == u32::MAX {
        return None;
    }
    let end = name.iter().position(|&c| c == 0).unwrap_or(name.len());
    let name = String::from_utf16_lossy(&name[..end]);
    let upper = name.to_uppercase();
    let rest = upper.split_once("VID_1532&PID_")?.1;
    let pid = u16::from_str_radix(rest.get(..4)?, 16).ok()?;
    if !rest[4..].starts_with("&MI_02") {
        return None;
    }
    let path = std::ffi::CString::new(name).ok()?;
    Some((pid, hid::container_id(&path)))
}

unsafe extern "system" fn hook(code: i32, w: WPARAM, l: LPARAM) -> LRESULT {
    unsafe {
        if code >= 0 && w as u32 == WM_MOUSEWHEEL {
            let m = &*(l as *const MSLLHOOKSTRUCT);
            if m.flags & LLMHF_INJECTED == 0 && from_keyboard() {
                let delta = (m.mouseData >> 16) as i16;
                let media = if delta > 0 { Media::VolumeUp } else { Media::VolumeDown };
                for _ in 0..(delta.unsigned_abs() / WHEEL_DELTA as u16).max(1) {
                    engine::send_media(media);
                }
                return 1;
            }
        }
        CallNextHookEx(std::ptr::null_mut(), code, w, l)
    }
}

/// Whether the notch the hook is handling came from a registered keyboard.
unsafe fn from_keyboard() -> bool {
    let hwnd = WINDOW.get();
    if hwnd.is_null() {
        return false;
    }
    let mut msg: MSG = unsafe { std::mem::zeroed() };
    while unsafe { PeekMessageW(&mut msg, hwnd, WM_INPUT, WM_INPUT, PM_REMOVE) } != 0 {
        unsafe { window(hwnd, msg.message, msg.wParam, msg.lParam) };
    }
    registry().take().is_some()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn unit(container: &str) -> Unit {
        Unit { spec: razer_core::devices::by_pid(0x0266).unwrap(), container: Some(container.into()) }
    }

    #[test]
    fn raw_input_runs_while_any_unit_remains() {
        let mut r = Registry::default();
        let (a, first_a) = r.add(unit("A"));
        let (b, first_b) = r.add(unit("B"));
        assert!(first_a && !first_b);
        assert!(!r.remove(a), "B still needs the registration");
        assert!(r.remove(b));
        assert!(r.add(unit("C")).1, "a later unit registers again");
    }

    #[test]
    fn a_record_goes_to_the_unit_that_owns_the_device() {
        let mut r = Registry::default();
        let (a, _) = r.add(unit("A"));
        let (b, _) = r.add(unit("B"));
        assert_eq!(r.owner(0x0266, Some("A")), Some(a));
        assert_eq!(r.owner(0x0266, Some("B")), Some(b));
        assert_eq!(r.owner(0x0266, Some("Z")), None);
        r.remove(b);
        assert_eq!(r.owner(0x0266, Some("B")), None, "a gone unit no longer claims notches");
    }

    #[test]
    fn take_skips_stale_and_foreign_notches() {
        let mut r = Registry::default();
        r.seen.push_back((Instant::now() - STALE * 2, Some(1)));
        r.seen.push_back((Instant::now(), None));
        r.seen.push_back((Instant::now(), Some(2)));
        assert_eq!(r.take(), None);
        assert_eq!(r.take(), Some(2));
    }
}
