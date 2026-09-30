//! In driver mode the firmware reports the volume wheel as a mouse wheel on MI_02.
//! A low-level hook swallows those notches and sends volume keys instead.
//!
//! The hook cannot tell devices apart, but Raw Input can, and Windows queues the
//! `WM_INPUT` of a wheel notch before it calls the hook for it (checked on hardware).

use std::cell::RefCell;
use std::collections::VecDeque;
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use razer_core::rapid::Media;
use windows_sys::Win32::Foundation::{HWND, LPARAM, LRESULT, WPARAM};
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

struct State {
    hwnd: HWND,
    /// `VID_1532&PID_xxxx&MI_02`, upper case like the device path.
    device: String,
    /// Wheel notches Raw Input saw, oldest first: when, and whether the keyboard sent it.
    seen: VecDeque<(Instant, bool)>,
}

thread_local! {
    static STATE: RefCell<Option<State>> = const { RefCell::new(None) };
}

pub struct Wheel {
    thread: u32,
    join: Option<JoinHandle<()>>,
}

impl Wheel {
    pub fn start(pid: u16) -> Self {
        let (tx, rx) = std::sync::mpsc::channel();
        let join = std::thread::spawn(move || {
            let _ = tx.send(unsafe { GetCurrentThreadId() });
            unsafe { run(format!("VID_1532&PID_{pid:04X}&MI_02")) };
        });
        Self { thread: rx.recv().unwrap_or(0), join: Some(join) }
    }
}

impl Drop for Wheel {
    fn drop(&mut self) {
        // Posted before the loop starts it stays queued, so the loop still sees it.
        unsafe { PostThreadMessageW(self.thread, WM_QUIT, 0, 0) };
        if let Some(j) = self.join.take() {
            let _ = j.join();
        }
    }
}

unsafe fn run(device: String) {
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
            STATE.set(Some(State { hwnd, device, seen: VecDeque::new() }));
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
        STATE.set(None);
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
        let mut name = [0u16; 256];
        let mut len = name.len() as u32;
        GetRawInputDeviceInfoW(ri.header.hDevice, RIDI_DEVICENAME, name.as_mut_ptr().cast(), &mut len);
        let name = String::from_utf16_lossy(&name).to_uppercase();
        STATE.with_borrow_mut(|s| {
            if let Some(s) = s {
                s.seen.push_back((Instant::now(), name.contains(&s.device)));
            }
        });
    }
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

/// Whether the notch the hook is handling came from the keyboard.
unsafe fn from_keyboard() -> bool {
    let Some(hwnd) = STATE.with_borrow(|s| s.as_ref().map(|s| s.hwnd)) else { return false };
    let mut msg: MSG = unsafe { std::mem::zeroed() };
    while unsafe { PeekMessageW(&mut msg, hwnd, WM_INPUT, WM_INPUT, PM_REMOVE) } != 0 {
        unsafe { window(hwnd, msg.message, msg.wParam, msg.lParam) };
    }
    STATE.with_borrow_mut(|s| {
        let seen = &mut s.as_mut()?.seen;
        while seen.front().is_some_and(|(at, _)| at.elapsed() > STALE) {
            seen.pop_front();
        }
        seen.pop_front().map(|(_, kb)| kb)
    })
    .unwrap_or(false)
}
