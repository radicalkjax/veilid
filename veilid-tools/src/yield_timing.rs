use super::*;

/// Total wall time from enqueue to wakeup at which a yield reports itself
pub const YIELD_SLOW_LOG_US: u64 = 1_000_000;

/// Wall-clock phases of one closure handed to a worker pool: queue wait, run, and the
/// gap between the result being ready and the awaiting task being polled again.
///
/// The three phases attribute a slow yield to the pool queue, the closure itself, or the
/// executor. A long stall in every phase at once means the process was not being run.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct YieldTiming {
    /// Microseconds when the closure was handed to the pool
    pub enqueued_us: u64,
    /// Microseconds when a pool thread began running it
    pub started_us: u64,
    /// Microseconds when it returned
    pub finished_us: u64,
    /// Microseconds when the awaiting task observed the result
    pub woken_us: u64,
}

impl YieldTiming {
    /// Time spent waiting in the pool queue
    #[must_use]
    pub fn wait_us(&self) -> u64 {
        self.started_us.saturating_sub(self.enqueued_us)
    }
    /// Time the closure ran
    #[must_use]
    pub fn run_us(&self) -> u64 {
        self.finished_us.saturating_sub(self.started_us)
    }
    /// Time between the result being ready and the awaiting task resuming
    #[must_use]
    pub fn wake_us(&self) -> u64 {
        self.woken_us.saturating_sub(self.finished_us)
    }
    /// Enqueue to wakeup
    #[must_use]
    pub fn total_us(&self) -> u64 {
        self.woken_us.saturating_sub(self.enqueued_us)
    }
    /// Whether the total crosses the reporting floor
    #[must_use]
    pub fn is_slow(&self) -> bool {
        self.total_us() >= YIELD_SLOW_LOG_US
    }
    /// Log a bare executor yield (no pool) whose wakeup took at least the reporting floor
    pub fn log_inline_yield_if_slow(yielded_us: u64, woken_us: u64) {
        let wake_us = woken_us.saturating_sub(yielded_us);
        if wake_us >= YIELD_SLOW_LOG_US {
            debug!("yield slow (inline): wake {}", human_duration(wake_us));
        }
    }
    /// Log the phase split when slow; `depth` is the pool's queue depth at enqueue, for
    /// pools that expose one
    pub fn log_if_slow(&self, pool: &str, depth: Option<usize>) {
        if !self.is_slow() {
            return;
        }
        let depth = depth.map_or_else(String::new, |d| format!(" (queue depth {})", d));
        debug!(
            "yield slow ({}): wait {} run {} wake {}{}",
            pool,
            human_duration(self.wait_us()),
            human_duration(self.run_us()),
            human_duration(self.wake_us()),
            depth
        );
    }
}
