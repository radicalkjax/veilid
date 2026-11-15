//! Windows slows the execution of a process it judges to be in the background (power
//! throttling, shown as efficiency mode). A node that prefers the foreground opts the process
//! out while a guard wants it, then hands the decision back to the system.

use super::*;
use ::windows::Win32::System::Threading::{
    GetCurrentProcess, ProcessPowerThrottling, SetProcessInformation,
    PROCESS_POWER_THROTTLING_CURRENT_VERSION, PROCESS_POWER_THROTTLING_EXECUTION_SPEED,
    PROCESS_POWER_THROTTLING_STATE,
};

/// Guards that want the process kept out of power throttling
static FOREGROUND: Mutex<usize> = Mutex::new(0);

pub(super) fn begin_foreground() {
    let mut guards = FOREGROUND.lock();
    *guards += 1;
    if *guards == 1 {
        set_execution_speed_throttling(false);
    }
}

pub(super) fn end_foreground() {
    let mut guards = FOREGROUND.lock();
    *guards = guards.saturating_sub(1);
    if *guards == 0 {
        set_execution_speed_throttling(true);
    }
}

/// Opts out of execution speed throttling, or with `allowed` lets the system decide again
fn set_execution_speed_throttling(allowed: bool) {
    let state = PROCESS_POWER_THROTTLING_STATE {
        Version: PROCESS_POWER_THROTTLING_CURRENT_VERSION,
        ControlMask: if allowed {
            0
        } else {
            PROCESS_POWER_THROTTLING_EXECUTION_SPEED
        },
        StateMask: 0,
    };
    // SAFETY: the current-process pseudo handle needs no closing, and the pointer and size
    // describe `state`, which outlives the call
    let result = unsafe {
        SetProcessInformation(
            GetCurrentProcess(),
            ProcessPowerThrottling,
            std::ptr::from_ref(&state).cast(),
            size_of::<PROCESS_POWER_THROTTLING_STATE>() as u32,
        )
    };
    if let Err(e) = result {
        warn!("could not change power throttling: {}", e);
    }
}
