//! Mouse wheel plumbing: a message-only window for Raw Input and a low-level mouse hook.

use std::cell::Cell;
use windows_sys::Win32::Foundation::{HWND, LPARAM, LRESULT, WPARAM};
use windows_sys::Win32::System::Threading::GetCurrentThreadId;
use windows_sys::Win32::UI::Input::{
    GetRawInputData, GetRawInputDeviceInfoW, HRAWINPUT, RAWINPUT, RAWINPUTDEVICE, RAWINPUTHEADER, RID_INPUT, RIDEV_INPUTSINK, RIDEV_REMOVE,
    RIDI_DEVICENAME, RegisterRawInputDevices,
};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    CallNextHookEx, CreateWindowExW, DefWindowProcW, DestroyWindow, DispatchMessageW, GetMessageW, HWND_MESSAGE, LLMHF_INJECTED, MSG,
    MSLLHOOKSTRUCT, PM_NOREMOVE, PM_REMOVE, PeekMessageW, PostThreadMessageW, RI_MOUSE_WHEEL, RegisterClassW, SetWindowsHookExW,
    UnhookWindowsHookEx, WH_MOUSE_LL, WM_INPUT, WM_MOUSEWHEEL, WM_QUIT, WM_USER, WNDCLASSW,
};

/// What the wheel loop reports.
#[derive(Clone, Copy)]
pub struct WheelEvents {
    /// Raw Input saw a wheel notch from this device.
    pub notch: fn(device: isize),
    /// The hook sees a physical wheel turn of `delta`; true swallows it.
    pub turn: fn(delta: i16) -> bool,
}

thread_local! {
    static WINDOW: Cell<HWND> = const { Cell::new(std::ptr::null_mut()) };
    static EVENTS: Cell<Option<WheelEvents>> = const { Cell::new(None) };
}

/// Gives this thread a message queue, so a quit posted before the loop starts stays queued.
pub fn start_message_queue() -> u32 {
    let mut msg = MSG::default();
    // SAFETY: `msg` is writable; PM_NOREMOVE leaves the queue as it was.
    unsafe { PeekMessageW(&mut msg, std::ptr::null_mut(), WM_USER, WM_USER, PM_NOREMOVE) };
    // SAFETY: no arguments.
    unsafe { GetCurrentThreadId() }
}

/// Ends the wheel loop of a thread; fails only once that thread has exited.
pub fn quit_thread(thread: u32) {
    // SAFETY: takes values only; a stale thread id only fails the call.
    unsafe { PostThreadMessageW(thread, WM_QUIT, 0, 0) };
}

/// Watches the wheel on this thread until it is told to quit.
pub fn run_wheel(events: WheelEvents) {
    EVENTS.set(Some(events));
    let class: Vec<u16> = "EasyRazerWheel\0".encode_utf16().collect();
    let wc = WNDCLASSW { lpfnWndProc: Some(window), lpszClassName: class.as_ptr(), ..Default::default() };
    // SAFETY: `wc` and the class name live across the call; `window` matches WNDPROC.
    unsafe { RegisterClassW(&wc) };
    // SAFETY: `class` is NUL-terminated and outlives the window, which is destroyed below.
    let hwnd = unsafe {
        CreateWindowExW(0, class.as_ptr(), class.as_ptr(), 0, 0, 0, 0, 0, HWND_MESSAGE, std::ptr::null_mut(), std::ptr::null_mut(), std::ptr::null())
    };
    if hwnd.is_null() {
        return;
    }
    let mut rid = RAWINPUTDEVICE { usUsagePage: 0x01, usUsage: 0x02, dwFlags: RIDEV_INPUTSINK, hwndTarget: hwnd };
    let size = size_of::<RAWINPUTDEVICE>() as u32;
    // SAFETY: `rid` is one RAWINPUTDEVICE of the size passed, naming this thread's window.
    let hook = if unsafe { RegisterRawInputDevices(&rid, 1, size) } != 0 {
        WINDOW.set(hwnd);
        // SAFETY: `hook` matches HOOKPROC; a low-level hook needs no module and runs on this thread's loop.
        unsafe { SetWindowsHookExW(WH_MOUSE_LL, Some(hook), std::ptr::null_mut(), 0) }
    } else {
        std::ptr::null_mut()
    };
    let mut msg = MSG::default();
    // SAFETY: `msg` is writable; pumping this thread's queue is what runs the hook.
    while !hook.is_null() && unsafe { GetMessageW(&mut msg, std::ptr::null_mut(), 0, 0) } > 0 {
        // SAFETY: `msg` was just filled by GetMessageW.
        unsafe { DispatchMessageW(&msg) };
    }
    if !hook.is_null() {
        // SAFETY: `hook` was installed above and is removed once.
        unsafe { UnhookWindowsHookEx(hook) };
        rid.dwFlags = RIDEV_REMOVE;
        rid.hwndTarget = std::ptr::null_mut();
        // SAFETY: as for the registration; RIDEV_REMOVE takes a null target.
        unsafe { RegisterRawInputDevices(&rid, 1, size) };
    }
    WINDOW.set(std::ptr::null_mut());
    // SAFETY: `hwnd` was created on this thread and is destroyed once.
    unsafe { DestroyWindow(hwnd) };
}

unsafe extern "system" fn window(hwnd: HWND, msg: u32, w: WPARAM, l: LPARAM) -> LRESULT {
    if msg == WM_INPUT {
        // SAFETY: for WM_INPUT the lParam is the HRAWINPUT of the input.
        unsafe { record(l as HRAWINPUT) };
    }
    // SAFETY: passes on what Windows gave this window procedure.
    unsafe { DefWindowProcW(hwnd, msg, w, l) }
}

/// Reports a wheel notch from Raw Input with its source.
unsafe fn record(handle: HRAWINPUT) {
    let mut ri = RAWINPUT::default();
    let mut size = size_of::<RAWINPUT>() as u32;
    let header = size_of::<RAWINPUTHEADER>() as u32;
    // SAFETY: `ri` is writable for `size` bytes, and Windows rejects a bad `handle`.
    if unsafe { GetRawInputData(handle, RID_INPUT, (&raw mut ri).cast(), &mut size, header) } == u32::MAX {
        return;
    }
    // SAFETY: only the mouse is registered, so the data is the mouse variant.
    let mouse = unsafe { ri.data.mouse };
    // SAFETY: both views of the button union are plain integers, valid for any bytes.
    let flags = unsafe { mouse.Anonymous.Anonymous.usButtonFlags };
    if u32::from(flags) & RI_MOUSE_WHEEL == 0 {
        return;
    }
    if let Some(events) = EVENTS.get() {
        (events.notch)(ri.header.hDevice as isize);
    }
}

/// Handles the Raw Input queued before the hook call; false without a wheel window on this thread.
pub fn drain_raw_input() -> bool {
    let hwnd = WINDOW.get();
    if hwnd.is_null() {
        return false;
    }
    let mut msg = MSG::default();
    // SAFETY: `msg` is writable; `hwnd` is this thread's wheel window.
    while unsafe { PeekMessageW(&mut msg, hwnd, WM_INPUT, WM_INPUT, PM_REMOVE) } != 0 {
        // SAFETY: hands the window procedure the WM_INPUT message just removed for its window.
        unsafe { window(hwnd, msg.message, msg.wParam, msg.lParam) };
    }
    true
}

/// The device interface path of a Raw Input device.
pub fn raw_input_device_name(device: isize) -> Option<String> {
    let mut name = [0u16; 256];
    let mut len = name.len() as u32;
    // SAFETY: `name` is writable for `len` characters, the unit RIDI_DEVICENAME counts in.
    if unsafe { GetRawInputDeviceInfoW(device as _, RIDI_DEVICENAME, name.as_mut_ptr().cast(), &mut len) } == u32::MAX {
        return None;
    }
    let end = name.iter().position(|&c| c == 0).unwrap_or(name.len());
    Some(String::from_utf16_lossy(&name[..end]))
}

unsafe extern "system" fn hook(code: i32, w: WPARAM, l: LPARAM) -> LRESULT {
    if code >= 0 && w as u32 == WM_MOUSEWHEEL {
        // SAFETY: for WM_MOUSEWHEEL the hook's lParam points at the event's MSLLHOOKSTRUCT,
        // valid for this call.
        let m = unsafe { &*(l as *const MSLLHOOKSTRUCT) };
        if m.flags & LLMHF_INJECTED == 0
            && let Some(events) = EVENTS.get()
            && (events.turn)((m.mouseData >> 16) as i16)
        {
            return 1;
        }
    }
    // SAFETY: passes on what Windows gave this hook.
    unsafe { CallNextHookEx(std::ptr::null_mut(), code, w, l) }
}
