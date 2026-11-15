#![cfg(not(all(target_arch = "wasm32", target_os = "unknown")))]

use super::*;
use std::sync::mpsc;

/// A plain OS thread that sleeps on a fixed period and reports when it wakes late.
///
/// Async tick lag cannot tell an overloaded executor from a process the OS is not
/// scheduling. This thread touches no executor, so its lateness is the OS's alone:
/// when it matches the async lag, the whole process was held (App Nap, sleep, CPU
/// starvation); when it stays punctual while tasks lag, the executor or a pool is the
/// bottleneck. Dropping the handle stops the thread at its next wakeup.
pub struct ProcessWatchdog {
    _stop: mpsc::Sender<()>,
}

impl ProcessWatchdog {
    /// Start the thread. `on_late(late_us, now_us)` runs on the watchdog thread whenever
    /// a wakeup is at least `late_threshold_ms` past its period.
    #[must_use]
    pub fn start(
        period_ms: u64,
        late_threshold_ms: u64,
        on_late: impl Fn(u64, u64) + Send + 'static,
    ) -> Self {
        let (stop, wake) = mpsc::channel::<()>();
        let period = Duration::from_millis(period_ms);
        let late_threshold_us = late_threshold_ms * 1000;
        let thread = std::thread::Builder::new()
            .name("veilid-watchdog".to_owned())
            .spawn(move || loop {
                let before = get_raw_timestamp();
                match wake.recv_timeout(period) {
                    Err(mpsc::RecvTimeoutError::Timeout) => {}
                    Ok(()) | Err(mpsc::RecvTimeoutError::Disconnected) => break,
                }
                let now = get_raw_timestamp();
                let late_us = now.saturating_sub(before).saturating_sub(period_ms * 1000);
                if late_us >= late_threshold_us {
                    on_late(late_us, now);
                }
            });
        match thread {
            Ok(_) => debug!("process watchdog started: period {}ms", period_ms),
            Err(e) => warn!("process watchdog thread not started: {}", e),
        }
        Self { _stop: stop }
    }
}
