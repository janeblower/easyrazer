use std::sync::Once;

use windows_sys::Win32::Foundation::HANDLE;
use windows_sys::Win32::Media::{timeBeginPeriod, timeEndPeriod};
use windows_sys::Win32::System::Threading::{
    AvRevertMmThreadCharacteristics, AvSetMmThreadCharacteristicsW, GetCurrentProcess, GetCurrentThread, ProcessPowerThrottling,
    SetProcessInformation, SetThreadPriority, PROCESS_POWER_THROTTLING_CURRENT_VERSION, PROCESS_POWER_THROTTLING_EXECUTION_SPEED,
    PROCESS_POWER_THROTTLING_IGNORE_TIMER_RESOLUTION, PROCESS_POWER_THROTTLING_STATE, THREAD_PRIORITY_TIME_CRITICAL,
};

/// Holds the system timer at 1 ms, so waits of a few milliseconds are not rounded up to a tick of ~15.6 ms.
pub struct TimerResolution(bool);

impl TimerResolution {
    pub fn acquire() -> Self {
        // SAFETY: takes a value only; a successful call is paired with `timeEndPeriod` in Drop.
        let ok = unsafe { timeBeginPeriod(1) } == 0;
        if !ok {
            eprintln!("timeBeginPeriod(1) failed");
        }
        Self(ok)
    }
}

impl Drop for TimerResolution {
    fn drop(&mut self) {
        if self.0 {
            // SAFETY: takes a value only; matches the `timeBeginPeriod` in `acquire`.
            unsafe { timeEndPeriod(1) };
        }
    }
}

fn throttling_opt_out(flag: u32) -> bool {
    let state = PROCESS_POWER_THROTTLING_STATE { Version: PROCESS_POWER_THROTTLING_CURRENT_VERSION, ControlMask: flag, StateMask: 0 };
    // SAFETY: no arguments; the pseudo handle needs no closing.
    let process = unsafe { GetCurrentProcess() };
    // SAFETY: `state` is a valid PROCESS_POWER_THROTTLING_STATE of the size passed.
    unsafe { SetProcessInformation(process, ProcessPowerThrottling, (&raw const state).cast(), size_of_val(&state) as u32) != 0 }
}

/// Keeps Windows from throttling this process to efficiency cores and from ignoring its timer
/// resolution (Win11, hidden windows). Process-wide, so it runs once. Two calls: the second flag
/// is unknown before Win11 and must not take the first down with it.
pub fn opt_out_of_power_throttling() {
    static ONCE: Once = Once::new();
    ONCE.call_once(|| {
        if !throttling_opt_out(PROCESS_POWER_THROTTLING_EXECUTION_SPEED) {
            eprintln!("opting out of EcoQoS failed");
        }
        // Fails on Win10, where the timer resolution is already global.
        throttling_opt_out(PROCESS_POWER_THROTTLING_IGNORE_TIMER_RESOLUTION);
    });
}

/// Raises the calling thread for as long as it lives; only for threads that block between events.
pub struct ThreadBoost(HANDLE);

impl ThreadBoost {
    /// MMCSS "Games", else `TIME_CRITICAL`.
    pub fn apply() -> Self {
        let task = super::wide("Games");
        let mut index = 0u32;
        // SAFETY: `task` is NUL-terminated and `index` writable for the call.
        let handle = unsafe { AvSetMmThreadCharacteristicsW(task.as_ptr(), &mut index) };
        if handle.is_null() {
            // SAFETY: no arguments; the pseudo handle of this thread needs no closing.
            let thread = unsafe { GetCurrentThread() };
            // SAFETY: takes values only.
            if unsafe { SetThreadPriority(thread, THREAD_PRIORITY_TIME_CRITICAL) } == 0 {
                eprintln!("raising the thread priority failed");
            }
        }
        Self(handle)
    }
}

impl Drop for ThreadBoost {
    fn drop(&mut self) {
        if !self.0.is_null() {
            // SAFETY: the handle came from `AvSetMmThreadCharacteristicsW` on this thread and is reverted once.
            unsafe { AvRevertMmThreadCharacteristics(self.0) };
        }
    }
}

/// With `EASYRAZER_TRACE_TIMING` set, prints the gap since the previous batch of typed events: the
/// auto-repeat interval, or the step of a macro with 1 ms delays.
#[cfg(debug_assertions)]
pub fn trace_emit(events: usize) {
    use std::sync::{Mutex, OnceLock};
    use std::time::Instant;
    static ON: OnceLock<bool> = OnceLock::new();
    static LAST: Mutex<Option<Instant>> = Mutex::new(None);
    if events == 0 || !*ON.get_or_init(|| std::env::var_os("EASYRAZER_TRACE_TIMING").is_some()) {
        return;
    }
    let now = Instant::now();
    if let Some(prev) = LAST.lock().unwrap_or_else(|e| e.into_inner()).replace(now) {
        eprintln!("emit gap {} us ({events} events)", now.duration_since(prev).as_micros());
    }
}
