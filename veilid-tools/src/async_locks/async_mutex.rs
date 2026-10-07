//! AsyncMutex
use super::*;

// Inner type and macro API must agree per executor; keep them in one cfg_if so they can't
// drift apart under cargo feature unification (e.g. both rt-async-std and rt-tokio enabled).
cfg_if! {
    if #[cfg(all(target_arch = "wasm32", target_os = "unknown"))] {
        use async_lock::Mutex as InnerAsyncMutex;
        use async_lock::MutexGuard as InnerAsyncMutexGuard;
        use async_lock::MutexGuardArc as InnerAsyncMutexGuardArc;
        macro_rules! asyncmutex_try_lock { ($x:expr) => { $x.try_lock() }; }
        macro_rules! asyncmutex_lock_arc { ($x:expr) => { $x.lock_arc() }; }
        macro_rules! asyncmutex_try_lock_arc { ($x:expr) => { $x.try_lock_arc() }; }
    } else if #[cfg(feature="rt-async-std")] {
        use async_std::sync::Mutex as InnerAsyncMutex;
        use async_std::sync::MutexGuard as InnerAsyncMutexGuard;
        use async_std::sync::MutexGuardArc as InnerAsyncMutexGuardArc;
        macro_rules! asyncmutex_try_lock { ($x:expr) => { $x.try_lock() }; }
        macro_rules! asyncmutex_lock_arc { ($x:expr) => { $x.lock_arc() }; }
        macro_rules! asyncmutex_try_lock_arc { ($x:expr) => { $x.try_lock_arc() }; }
    } else if #[cfg(feature="rt-tokio")] {
        use tokio::sync::Mutex as InnerAsyncMutex;
        use tokio::sync::MutexGuard as InnerAsyncMutexGuard;
        use tokio::sync::OwnedMutexGuard as InnerAsyncMutexGuardArc;
        macro_rules! asyncmutex_try_lock { ($x:expr) => { $x.try_lock().ok() }; }
        macro_rules! asyncmutex_lock_arc { ($x:expr) => { $x.clone().lock_owned() }; }
        macro_rules! asyncmutex_try_lock_arc { ($x:expr) => { $x.clone().try_lock_owned().ok() }; }
    } else {
        compile_error!("needs executor implementation");
    }
}

/// An async mutex giving one holder at a time exclusive access to `T`.
///
/// Backed by the executor's mutex (`async_lock`, `async-std`, or `tokio` depending on
/// build features); under the `debug-locks` feature, acquisitions that stall past
/// [`DEBUG_LOCKS_DURATION_MS`] are reported as deadlocks.
#[derive(Debug)]
pub struct AsyncMutex<T: ?Sized> {
    inner: Arc<InnerAsyncMutex<T>>,
    #[cfg(feature = "debug-locks")]
    lock_id_container: LockIdContainer,
}

impl<T: ?Sized> AsyncMutex<T> {
    /// Create a new mutex holding `t`, initially unlocked.
    #[inline]
    pub fn new(t: T) -> Self
    where
        T: Sized,
    {
        Self {
            inner: Arc::new(InnerAsyncMutex::new(t)),
            #[cfg(feature = "debug-locks")]
            lock_id_container: LockIdContainer::next(),
        }
    }

    /// Acquire the mutex, waiting until it is free.
    ///
    /// Blocks until the lock is free. The returned guard holds the lock until dropped.
    /// Under `debug-locks`, a wait past [`DEBUG_LOCKS_DURATION_MS`] is reported as a deadlock.
    #[inline]
    pub async fn lock(&self) -> AsyncMutexGuard<'_, T> {
        cfg_if! {
            if #[cfg(feature = "debug-locks")] {
                let inner = match timeout(DEBUG_LOCKS_DURATION_MS, self.inner.lock()).await {
                    Ok(v) => v,
                    Err(_) => {
                        self.lock_id_container.report_deadlock("AsyncMutex::lock deadlock");
                    }
                };
            } else {
                let inner = self.inner.lock().await;
            }
        }

        AsyncMutexGuard {
            inner,
            #[cfg(feature = "debug-locks")]
            _guard_id_container: GuardIdContainer::next(self.lock_id_container.clone()),
        }
    }

    /// Acquire the mutex, waiting until it is free, returning an owned guard.
    ///
    /// Blocks until the lock is free. The returned guard holds the lock until dropped.
    /// Under `debug-locks`, a wait past [`DEBUG_LOCKS_DURATION_MS`] is reported as a deadlock.
    #[inline]
    pub async fn lock_arc(self: &Arc<Self>) -> AsyncMutexGuardArc<T> {
        cfg_if! {
            if #[cfg(feature = "debug-locks")] {
                let inner = match timeout(DEBUG_LOCKS_DURATION_MS, asyncmutex_lock_arc!(self.inner)).await {
                    Ok(v) => v,
                    Err(_) => {
                        self.lock_id_container.report_deadlock("AsyncMutex::lock_arc deadlock");
                    }
                };
            } else {
                let inner = asyncmutex_lock_arc!(self.inner).await;
            }
        }

        AsyncMutexGuardArc {
            inner,
            #[cfg(feature = "debug-locks")]
            _guard_id_container: GuardIdContainer::next(self.lock_id_container.clone()),
        }
    }

    /// Try to acquire the mutex without waiting, or `None` if it is already held.
    ///
    /// Never blocks. On success the guard holds the lock until dropped.
    #[inline]
    #[must_use]
    pub fn try_lock(&self) -> Option<AsyncMutexGuard<'_, T>> {
        let inner = asyncmutex_try_lock!(self.inner)?;

        let out = AsyncMutexGuard {
            inner,
            #[cfg(feature = "debug-locks")]
            _guard_id_container: GuardIdContainer::next(self.lock_id_container.clone()),
        };

        Some(out)
    }

    /// Try to acquire the mutex without waiting, returning an owned guard, or `None` if it is already held.
    ///
    /// Never blocks. On success the guard holds the lock until dropped.
    #[inline]
    #[must_use]
    pub fn try_lock_arc(self: &Arc<Self>) -> Option<AsyncMutexGuardArc<T>> {
        let inner = asyncmutex_try_lock_arc!(self.inner)?;

        let out = AsyncMutexGuardArc {
            inner,
            #[cfg(feature = "debug-locks")]
            _guard_id_container: GuardIdContainer::next(self.lock_id_container.clone()),
        };

        Some(out)
    }
}

impl<T> From<T> for AsyncMutex<T> {
    fn from(s: T) -> Self {
        Self::new(s)
    }
}

/// Borrowed guard holding the mutex; derefs to `T` (mutably) and unlocks on drop.
#[clippy::has_significant_drop]
#[must_use = "if unused the Mutex will immediately unlock"]
#[derive(Debug)]
pub struct AsyncMutexGuard<'a, T: ?Sized> {
    inner: InnerAsyncMutexGuard<'a, T>,
    #[cfg(feature = "debug-locks")]
    _guard_id_container: GuardIdContainer,
}

impl<T: fmt::Display + ?Sized> fmt::Display for AsyncMutexGuard<'_, T> {
    #[inline]
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.inner.fmt(f)
    }
}

impl<T: ?Sized> std::ops::Deref for AsyncMutexGuard<'_, T> {
    type Target = T;

    #[inline]
    fn deref(&self) -> &T {
        &self.inner
    }
}

impl<T: ?Sized> std::ops::DerefMut for AsyncMutexGuard<'_, T> {
    #[inline]
    fn deref_mut(&mut self) -> &mut T {
        &mut self.inner
    }
}

/// Owned guard holding the mutex; derefs to `T` (mutably) and unlocks on drop.
#[clippy::has_significant_drop]
#[derive(Debug)]
pub struct AsyncMutexGuardArc<T: ?Sized> {
    inner: InnerAsyncMutexGuardArc<T>,
    #[cfg(feature = "debug-locks")]
    _guard_id_container: GuardIdContainer,
}

impl<T: fmt::Display + ?Sized> fmt::Display for AsyncMutexGuardArc<T> {
    #[inline]
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.inner.fmt(f)
    }
}

impl<T: ?Sized> std::ops::Deref for AsyncMutexGuardArc<T> {
    type Target = T;

    #[inline]
    fn deref(&self) -> &T {
        &self.inner
    }
}

impl<T: ?Sized> std::ops::DerefMut for AsyncMutexGuardArc<T> {
    #[inline]
    fn deref_mut(&mut self) -> &mut T {
        &mut self.inner
    }
}
