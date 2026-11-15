//! Call-site rate limiting for log events whose rate nobody chose.
//!
//! A receive loop that retries a failure without backoff emits at CPU speed, and where the log
//! sink crosses an FFI boundary every event is a synchronous post into another runtime. Bounding
//! that where it is emitted is what keeps a verbose level from being a denial of service: the
//! filter level decides whether an event is interesting, not how many of it per second the
//! process can afford.
//!
//! Used through the `throttle:` option on `veilid_log!` rather than directly.

use super::*;

/// Per-call-site emission gate: lets one event through per interval and counts what it drops.
///
/// Cheap enough for the hot path: one clock read and one atomic on the suppressed path.
pub struct LogThrottle {
    /// Minimum gap between emissions
    interval_us: u64,
    /// Raw timestamp of the last emission; 0 means nothing has been emitted yet
    pub(super) last_us: AtomicU64,
    /// Events dropped since the last emission
    pub(super) suppressed: AtomicU64,
}

impl LogThrottle {
    pub const fn new_ms(interval_ms: u64) -> Self {
        Self::new_us(interval_ms.saturating_mul(1000))
    }

    pub const fn new_us(interval_us: u64) -> Self {
        Self {
            interval_us,
            last_us: AtomicU64::new(0),
            suppressed: AtomicU64::new(0),
        }
    }

    /// `Some(n)` when the caller should emit, carrying how many events were dropped since the
    /// last emission. `None` to stay silent.
    pub fn check(&self) -> Option<u64> {
        let now = get_raw_timestamp();
        let last = self.last_us.load(Ordering::Relaxed);

        // now < last is a wall clock stepped back: emit rather than stay silent until it catches up
        if last != 0 && now >= last && now - last < self.interval_us {
            self.suppressed.fetch_add(1, Ordering::Relaxed);
            return None;
        }

        // Two threads can pass the window check together; only the one that claims the slot emits
        if self
            .last_us
            .compare_exchange(last, now, Ordering::Relaxed, Ordering::Relaxed)
            .is_err()
        {
            self.suppressed.fetch_add(1, Ordering::Relaxed);
            return None;
        }

        Some(self.suppressed.swap(0, Ordering::Relaxed))
    }
}

/// Renders a suppressed count as a message suffix, and as nothing at all when none were dropped.
pub struct SuppressedCount(pub u64);

impl fmt::Display for SuppressedCount {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.0 == 0 {
            return Ok(());
        }
        write!(f, " [{} more suppressed]", self.0)
    }
}
