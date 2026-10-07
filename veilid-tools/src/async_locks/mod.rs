//! Debuggable Async Locks
use super::*;

mod async_keyed_cache;
mod async_mutex;
mod async_rw_lock;
mod async_semaphore;
mod async_tag_lock;
mod async_weighted_semaphore;
#[cfg(feature = "debug-locks")]
mod debug_locks;

pub use async_keyed_cache::*;
pub use async_mutex::*;
pub use async_rw_lock::*;
pub use async_semaphore::*;
pub use async_tag_lock::*;
pub use async_weighted_semaphore::*;

#[cfg(feature = "debug-locks")]
use debug_locks::*;

/// Timeout under the `debug-locks` feature after which a pending acquisition is reported as a deadlock.
pub const DEBUG_LOCKS_DURATION_MS: u32 = 30000;

/// Acquisition backtrace for deadlock reports. On by default under `debug-locks` (you've already
/// opted into deadlock debugging, and the report is useless without it); set
/// VEILID_DEBUG_LOCK_BACKTRACES=0 to disable if the per-acquire capture cost matters (e.g. release
/// profiling). Unresolved so symbolication is deferred to the report site.
#[cfg(feature = "debug-locks")]
pub(crate) fn debug_lock_backtrace() -> backtrace::Backtrace {
    use std::sync::LazyLock;
    static ENABLED: LazyLock<bool> = LazyLock::new(|| {
        std::env::var("VEILID_DEBUG_LOCK_BACKTRACES")
            .map(|v| v != "0")
            .unwrap_or(true)
    });
    if *ENABLED {
        backtrace::Backtrace::new_unresolved()
    } else {
        backtrace::Backtrace::from(Vec::new())
    }
}
