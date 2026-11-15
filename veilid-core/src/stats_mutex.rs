//! A `TrackedMutex` that also measures how long callers wait for and hold it.
//!
//! Only critical, contended locks warrant the two extra timestamp reads per
//! section; wrap those in `StatsMutex` and leave everything else a plain lock.
//! With `stats` off this is a transparent alias for `TrackedMutex`: no guard
//! wrapper, no accounting, no cost.

#[cfg(feature = "stats")]
mod imp {
    use super::super::*;

    /// Rolling wait/hold accounting for one named lock
    #[derive(Debug, Default)]
    struct LockHoldAccounting {
        count: u64,
        wait_accounting: LatencyStatsAccounting,
        held_accounting: LatencyStatsAccounting,
    }

    impl LockHoldAccounting {
        fn record(&mut self, wait: TimestampDuration, held: TimestampDuration) {
            self.count += 1;
            self.wait_accounting.record_latency(wait);
            self.held_accounting.record_latency(held);
        }
    }

    /// One lock's telemetry, read by a component stats snapshot
    #[derive(Clone)]
    pub(crate) struct LockStats {
        pub name: &'static str,
        pub count: u64,
        pub wait: LatencyStats,
        pub held: LatencyStats,
    }

    /// A `TrackedMutex` that times acquisition wait and hold duration per section
    pub(crate) struct StatsMutex<T> {
        inner: TrackedMutex<T>,
        accounting: Mutex<LockHoldAccounting>,
        name: &'static str,
    }

    impl<T: fmt::Debug> fmt::Debug for StatsMutex<T> {
        fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            self.inner.fmt(f)
        }
    }

    impl<T> StatsMutex<T> {
        pub(crate) fn new(value: T, name: &'static str) -> Self {
            Self {
                inner: TrackedMutex::new(value),
                accounting: Mutex::new(LockHoldAccounting::default()),
                name,
            }
        }

        pub(crate) fn lock(&self) -> StatsMutexGuard<'_, T> {
            // try_lock first, so the wait-start timestamp is read only when the lock would block
            let (guard, acquired, wait) = match self.inner.try_lock() {
                Some(guard) => (
                    guard,
                    Timestamp::now_non_decreasing(),
                    TimestampDuration::new(0),
                ),
                None => {
                    let wait_start = Timestamp::now_non_decreasing();
                    let guard = self.inner.lock();
                    let acquired = Timestamp::now_non_decreasing();
                    (guard, acquired, acquired.duration_since(wait_start))
                }
            };
            StatsMutexGuard {
                guard,
                accounting: &self.accounting,
                wait,
                acquired,
            }
        }

        pub(crate) fn lock_stats(&self) -> LockStats {
            let a = self.accounting.lock();
            LockStats {
                name: self.name,
                count: a.count,
                wait: a.wait_accounting.latency_stats().unwrap_or_default(),
                held: a.held_accounting.latency_stats().unwrap_or_default(),
            }
        }
    }

    pub(crate) struct StatsMutexGuard<'a, T> {
        guard: TrackedMutexGuard<'a, T>,
        accounting: &'a Mutex<LockHoldAccounting>,
        wait: TimestampDuration,
        acquired: Timestamp,
    }

    impl<T> core::ops::Deref for StatsMutexGuard<'_, T> {
        type Target = T;
        fn deref(&self) -> &T {
            &self.guard
        }
    }
    impl<T> core::ops::DerefMut for StatsMutexGuard<'_, T> {
        fn deref_mut(&mut self) -> &mut T {
            &mut self.guard
        }
    }
    impl<T> Drop for StatsMutexGuard<'_, T> {
        fn drop(&mut self) {
            let held = TimestampDuration::since_non_decreasing(self.acquired);
            self.accounting.lock().record(self.wait, held);
        }
    }
}

#[cfg(not(feature = "stats"))]
mod imp {
    use super::super::*;

    /// Transparent alias: with `stats` off this adds nothing over `TrackedMutex`.
    #[derive(Debug)]
    pub(crate) struct StatsMutex<T> {
        inner: TrackedMutex<T>,
    }

    impl<T> StatsMutex<T> {
        pub(crate) fn new(value: T, _name: &'static str) -> Self {
            Self {
                inner: TrackedMutex::new(value),
            }
        }
        pub(crate) fn lock(&self) -> TrackedMutexGuard<'_, T> {
            self.inner.lock()
        }
    }
}

pub(crate) use imp::*;
