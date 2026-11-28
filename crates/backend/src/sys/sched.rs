use windows_sys::Win32::Foundation::HANDLE;
use windows_sys::Win32::Media::{timeBeginPeriod, timeEndPeriod};
use windows_sys::Win32::System::Threading::{
    AvRevertMmThreadCharacteristics, AvSetMmThreadCharacteristicsW, GetCurrentProcess, GetCurrentThread, ProcessPowerThrottling,
    SetProcessInformation, SetThreadPriority, PROCESS_POWER_THROTTLING_CURRENT_VERSION, PROCESS_POWER_THROTTLING_EXECUTION_SPEED,
    PROCESS_POWER_THROTTLING_IGNORE_TIMER_RESOLUTION, PROCESS_POWER_THROTTLING_STATE, THREAD_PRIORITY_TIME_CRITICAL,
};

/// Holds the system timer at 1 ms, so waits of a few milliseconds are not rounded up to a tick of ~15.6 ms.
pub struct TimerResolution;

impl TimerResolution {
    pub fn acquire() -> Self {
        // SAFETY: takes a value only; every successful call is paired with `timeEndPeriod` in Drop.
        if unsafe { timeBeginPeriod(1) } != 0 {
            eprintln!("timeBeginPeriod(1) failed");
        }
        Self
    }
}

impl Drop for TimerResolution {
    fn drop(&mut self) {
        // SAFETY: takes a value only; matches the `timeBeginPeriod` in `acquire`.
        unsafe { timeEndPeriod(1) };
    }
}

/// Keeps Windows from coarsening the timer or throttling this process to efficiency cores.
/// Win11 otherwise ignores the timer resolution of processes whose windows are hidden.
pub fn opt_out_of_power_throttling() {
    let state = PROCESS_POWER_THROTTLING_STATE {
        Version: PROCESS_POWER_THROTTLING_CURRENT_VERSION,
        ControlMask: PROCESS_POWER_THROTTLING_IGNORE_TIMER_RESOLUTION | PROCESS_POWER_THROTTLING_EXECUTION_SPEED,
        StateMask: 0,
    };
    // SAFETY: no arguments; the pseudo handle needs no closing.
    let process = unsafe { GetCurrentProcess() };
    // SAFETY: `state` is a valid PROCESS_POWER_THROTTLING_STATE of the size passed.
    let ok = unsafe { SetProcessInformation(process, ProcessPowerThrottling, (&raw const state).cast(), size_of_val(&state) as u32) };
    if ok == 0 {
        eprintln!("opting out of power throttling failed");
    }
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
