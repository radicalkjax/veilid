//! AsyncSemaphore
use super::*;

use async_lock::Semaphore as InnerAsyncSemaphore;
use async_lock::SemaphoreGuard as InnerAsyncSemaphoreGuard;
use async_lock::SemaphoreGuardArc as InnerAsyncSemaphoreGuardArc;

/// An async counting semaphore bounding the number of concurrent permit holders.
///
/// Wraps `async_lock::Semaphore`; under the `debug-locks` feature, acquisitions that
/// stall past [`DEBUG_LOCKS_DURATION_MS`] are reported as deadlocks.
#[derive(Debug)]
pub struct AsyncSemaphore {
    inner: Arc<InnerAsyncSemaphore>,
    #[cfg(feature = "debug-locks")]
    lock_id_container: LockIdContainer,
}

impl AsyncSemaphore {
    /// Create a semaphore with `n` permits.
    #[inline]
    #[must_use]
    pub fn new(n: usize) -> AsyncSemaphore {
        AsyncSemaphore {
            inner: Arc::new(InnerAsyncSemaphore::new(n)),
            #[cfg(feature = "debug-locks")]
            lock_id_container: LockIdContainer::next(),
        }
    }

    /// Try to acquire one permit without waiting, or `None` if none are available.
    ///
    /// Never blocks. On success the guard holds the permit until dropped.
    #[inline]
    #[must_use]
    pub fn try_acquire(&self) -> Option<AsyncSemaphoreGuard<'_>> {
        let inner = self.inner.try_acquire()?;
        let out = AsyncSemaphoreGuard {
            inner,
            #[cfg(feature = "debug-locks")]
            _guard_id_container: GuardIdContainer::next(self.lock_id_container.clone()),
        };

        Some(out)
    }

    /// Acquire one permit, waiting until one is available.
    ///
    /// Blocks until a permit frees up. The returned guard holds the permit until dropped.
    /// Under `debug-locks`, a wait past [`DEBUG_LOCKS_DURATION_MS`] is reported as a deadlock.
    #[inline]
    #[must_use]
    pub async fn acquire(&self) -> AsyncSemaphoreGuard<'_> {
        cfg_if! {
            if #[cfg(feature = "debug-locks")] {
                let inner = match timeout(DEBUG_LOCKS_DURATION_MS, self.inner.acquire()).await {
                    Ok(v) => v,
                    Err(_) => {
                        self.lock_id_container.report_deadlock("AsyncSemaphore::acquire deadlock");
                    }
                };
            } else {
                let inner = self.inner.acquire().await;
            }
        }

        AsyncSemaphoreGuard {
            inner,
            #[cfg(feature = "debug-locks")]
            _guard_id_container: GuardIdContainer::next(self.lock_id_container.clone()),
        }
    }

    /// Try to acquire one permit without waiting, returning an owned guard, or `None` if none are available.
    ///
    /// Never blocks. On success the guard holds the permit until dropped.
    #[inline]
    #[must_use]
    pub fn try_acquire_arc(self: &Arc<Self>) -> Option<AsyncSemaphoreGuardArc> {
        let inner = self.inner.try_acquire_arc()?;
        let out = AsyncSemaphoreGuardArc {
            inner,
            #[cfg(feature = "debug-locks")]
            _guard_id_container: GuardIdContainer::next(self.lock_id_container.clone()),
        };

        Some(out)
    }

    /// Acquire one permit, waiting until one is available, returning an owned guard.
    ///
    /// Blocks until a permit frees up. The returned guard holds the permit until dropped.
    /// Under `debug-locks`, a wait past [`DEBUG_LOCKS_DURATION_MS`] is reported as a deadlock.
    #[inline]
    #[must_use]
    pub async fn acquire_arc(self: &Arc<Self>) -> AsyncSemaphoreGuardArc {
        cfg_if! {
            if #[cfg(feature = "debug-locks")] {
                let inner = match timeout(DEBUG_LOCKS_DURATION_MS, self.inner.acquire_arc()).await {
                    Ok(v) => v,
                    Err(_) => {
                        self.lock_id_container.report_deadlock("AsyncSemaphore::acquire_arc deadlock");
                    }
                };
            } else {
                let inner = self.inner.acquire_arc().await;
            }
        }

        AsyncSemaphoreGuardArc {
            inner,
            #[cfg(feature = "debug-locks")]
            _guard_id_container: GuardIdContainer::next(self.lock_id_container.clone()),
        }
    }
}

/// A borrowed guard that releases its permit on drop.
#[clippy::has_significant_drop]
#[derive(Debug)]
pub struct AsyncSemaphoreGuard<'a> {
    #[allow(dead_code)]
    inner: InnerAsyncSemaphoreGuard<'a>,
    #[cfg(feature = "debug-locks")]
    _guard_id_container: GuardIdContainer,
}

/// An owned guard that releases the acquired permit.
#[clippy::has_significant_drop]
#[derive(Debug)]
pub struct AsyncSemaphoreGuardArc {
    #[allow(dead_code)]
    inner: InnerAsyncSemaphoreGuardArc,
    #[cfg(feature = "debug-locks")]
    _guard_id_container: GuardIdContainer,
}
