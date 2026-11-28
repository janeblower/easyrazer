use windows_sys::Win32::Media::{timeBeginPeriod, timeEndPeriod};
use windows_sys::Win32::System::Threading::{
    GetCurrentProcess, ProcessPowerThrottling, SetProcessInformation, PROCESS_POWER_THROTTLING_CURRENT_VERSION,
    PROCESS_POWER_THROTTLING_EXECUTION_SPEED, PROCESS_POWER_THROTTLING_IGNORE_TIMER_RESOLUTION, PROCESS_POWER_THROTTLING_STATE,
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
