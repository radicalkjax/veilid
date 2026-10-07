//! AsyncWeightedSemaphore
//!
//! A semaphore whose capacity (the "limit") can be changed at any time, and
//! whose permits carry a weight rather than counting as one. Acquisition waits
//! until the in-flight weight plus the requested weight fits under the current
//! limit. Unlike a fixed-permit semaphore, the limit can be lowered while
//! permits are held: new acquisitions wait until enough weight drains.
//!
//! Used to bound total in-flight bytes across all concurrent DHT operations
//! against an adaptively-estimated uplink budget.
use super::*;

use core::sync::atomic::{AtomicU64, Ordering};
use event_listener::Event;

/// A semaphore admitting weighted permits under a runtime-adjustable capacity limit.
///
/// Acquisition waits until the in-flight weight plus the requested weight fits under
/// the current limit. The limit can be raised or lowered while permits are held; a
/// single permit whose weight exceeds the limit is admitted when nothing else is in
/// flight, so it cannot deadlock.
#[derive(Debug)]
pub struct AsyncWeightedSemaphore {
    /// Total weight currently held by live permits
    in_flight: AtomicU64,
    /// Current capacity; acquisitions wait until in_flight + weight <= limit
    limit: AtomicU64,
    /// Wakes waiters when weight is released or the limit is raised
    event: Event,
}

impl AsyncWeightedSemaphore {
    /// Create a semaphore with the given initial capacity limit.
    #[must_use]
    pub fn new(limit: u64) -> Arc<Self> {
        Arc::new(Self {
            in_flight: AtomicU64::new(0),
            limit: AtomicU64::new(limit),
            event: Event::new(),
        })
    }

    /// Current capacity limit
    pub fn limit(&self) -> u64 {
        self.limit.load(Ordering::Acquire)
    }

    /// Total weight currently held by live permits
    pub fn in_flight(&self) -> u64 {
        self.in_flight.load(Ordering::Acquire)
    }

    /// Change the capacity limit. Raising it wakes waiters to re-check; lowering
    /// it applies backpressure, waiting acquisitions until in-flight weight drains.
    pub fn set_limit(&self, new_limit: u64) {
        let old = self.limit.swap(new_limit, Ordering::AcqRel);
        if new_limit > old {
            self.event.notify(usize::MAX);
        }
    }

    /// Raise the limit to at least `candidate`, never lowering it; wakes waiters if raised.
    /// Returns the previous limit so callers can detect/log an actual raise.
    ///
    /// Monotonic: a `candidate` at or below the current limit is a no-op.
    pub fn raise_limit(&self, candidate: u64) -> u64 {
        let prev = self.limit.fetch_max(candidate, Ordering::AcqRel);
        if candidate > prev {
            self.event.notify(usize::MAX);
        }
        prev
    }

    /// Try to acquire `weight` units without waiting.
    ///
    /// Never blocks. On success the guard holds the weight until dropped. A zero-weight
    /// request always succeeds and accounts nothing.
    #[must_use]
    pub fn try_acquire(self: &Arc<Self>, weight: u64) -> Option<AsyncWeightedSemaphoreGuard> {
        // Zero-weight permits always succeed and account nothing
        if weight == 0 {
            return Some(AsyncWeightedSemaphoreGuard {
                sem: self.clone(),
                weight: 0,
            });
        }
        loop {
            let cur = self.in_flight.load(Ordering::Acquire);
            let lim = self.limit.load(Ordering::Acquire);
            // Progress guarantee: admit one operation when nothing is in flight even
            // if its weight exceeds the limit, so a single large op can't deadlock.
            if cur != 0 && cur.saturating_add(weight) > lim {
                return None;
            }
            if self
                .in_flight
                .compare_exchange_weak(
                    cur,
                    cur.saturating_add(weight),
                    Ordering::AcqRel,
                    Ordering::Acquire,
                )
                .is_ok()
            {
                return Some(AsyncWeightedSemaphoreGuard {
                    sem: self.clone(),
                    weight,
                });
            }
            // CAS lost a race; retry
        }
    }

    /// Acquire `weight` units, waiting until they fit under the current limit.
    ///
    /// Blocks until enough weight drains (or the limit is raised). The returned guard holds
    /// the weight until dropped. No deadlock timeout: an over-limit request is admitted only
    /// when nothing else is in flight.
    #[must_use]
    pub async fn acquire(self: &Arc<Self>, weight: u64) -> AsyncWeightedSemaphoreGuard {
        loop {
            if let Some(g) = self.try_acquire(weight) {
                return g;
            }
            // Register before re-checking so a release/raise between the failed try
            // and the await is not missed.
            let listener = self.event.listen();
            if let Some(g) = self.try_acquire(weight) {
                return g;
            }
            listener.await;
        }
    }

    fn release(&self, weight: u64) {
        if weight == 0 {
            return;
        }
        self.in_flight.fetch_sub(weight, Ordering::AcqRel);
        self.event.notify(usize::MAX);
    }
}

/// A guard holding `weight` units; releases them back to the semaphore on drop.
#[clippy::has_significant_drop]
#[derive(Debug)]
pub struct AsyncWeightedSemaphoreGuard {
    sem: Arc<AsyncWeightedSemaphore>,
    weight: u64,
}

impl Drop for AsyncWeightedSemaphoreGuard {
    fn drop(&mut self) {
        self.sem.release(self.weight);
    }
}
