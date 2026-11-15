//! Monotonically non-decreasing and monotonically increasing microseconds-since epoch timestamp

use super::*;

aligned_u64_type!(Timestamp);
aligned_u64_type_default_debug_impl!(Timestamp);

static LAST_NON_DECREASING_RAW_TIMESTAMP: AtomicU64 = AtomicU64::new(0u64);
static LAST_INCREASING_RAW_TIMESTAMP: AtomicU64 = AtomicU64::new(0u64);

impl fmt::Display for Timestamp {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if f.alternate() {
            write!(f, "{}", human_timestamp(self.as_u64()))
        } else {
            write!(f, "{}", self.as_u64())
        }
    }
}

impl FromStr for Timestamp {
    type Err = <u64 as FromStr>::Err;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(Timestamp(u64::from_str(s)?))
    }
}

impl Timestamp {
    /// Current wall-clock time. May move backward if the system clock is adjusted.
    pub fn now() -> Self {
        Self::new(get_raw_timestamp())
    }

    /// Returns true if the timestamp is zero.
    #[must_use]
    pub fn is_zero(&self) -> bool {
        self.0 == 0
    }

    /// Current time, clamped to never read earlier than a previous call. Repeated values are allowed.
    pub fn now_non_decreasing() -> Self {
        let now = get_raw_timestamp();
        // Never allow time to go backward
        let last = LAST_NON_DECREASING_RAW_TIMESTAMP.fetch_max(now, Ordering::AcqRel);
        Self::new(now.max(last))
    }

    /// Current time, clamped to always read at least 1us later than a previous call. Never repeats a value.
    pub fn now_increasing() -> Self {
        let now = get_raw_timestamp();
        let mut last = LAST_INCREASING_RAW_TIMESTAMP.load(Ordering::Acquire);
        loop {
            // Never go backward and never issue the same value twice; 1us between calls is fine
            let next = if now <= last { last + 1 } else { now };
            match LAST_INCREASING_RAW_TIMESTAMP.compare_exchange_weak(
                last,
                next,
                Ordering::AcqRel,
                Ordering::Acquire,
            ) {
                Ok(_) => return Self::new(next),
                Err(observed) => last = observed,
            }
        }
    }

    /// Timestamp shifted later by a duration, saturating at the maximum value.
    pub fn later(self, rhs: TimestampDuration) -> Self {
        Self::new(self.0.saturating_add(rhs.as_u64()))
    }

    /// Timestamp shifted earlier by a duration, saturating at zero.
    pub fn earlier(self, rhs: TimestampDuration) -> Self {
        Self::new(self.0.saturating_sub(rhs.as_u64()))
    }

    /// Duration from `older` to this timestamp, saturating to zero if `older` is later.
    pub fn duration_since(self, older: Self) -> TimestampDuration {
        TimestampDuration::new(self.0.saturating_sub(older.0))
    }
}
