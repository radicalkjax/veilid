//! parking_lot deadlock-detector watchdog.
//!
//! Available only under the `debug-locks` feature, which also turns on
//! `parking_lot/deadlock_detection`. Callers gate the
//! [`start_deadlock_detector`] invocation on the same feature, and must call
//! it from inside an active async runtime; the watchdog lives only as long
//! as that runtime.

use super::*;

use parking_lot::deadlock;

const POLL_INTERVAL_MS: u32 = 5_000;

/// Spawn a background task on the current async runtime that periodically
/// polls `parking_lot::deadlock::check_deadlock()` and prints any cycles to
/// stderr with per-thread backtraces. Dies with the runtime that hosted it.
pub fn start_deadlock_detector() {
    spawn_detached("parking_lot-deadlock-detector", detector_loop());
}

async fn detector_loop() {
    loop {
        sleep(POLL_INTERVAL_MS).await;
        let deadlocks = deadlock::check_deadlock();
        if deadlocks.is_empty() {
            continue;
        }
        eprintln!(
            "===== parking_lot DEADLOCK DETECTED: {} cycle(s) =====",
            deadlocks.len()
        );
        for (i, threads) in deadlocks.iter().enumerate() {
            eprintln!("---- cycle #{i} ({} threads) ----", threads.len());
            for t in threads {
                eprintln!("thread id: {:#?}", t.thread_id());
                eprintln!("backtrace:\n{:#?}", t.backtrace());
            }
        }
    }
}
