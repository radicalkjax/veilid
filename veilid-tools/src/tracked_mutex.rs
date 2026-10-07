//! `TrackedMutex<T>`: a drop-in `parking_lot::Mutex<T>` wrapper that, under the
//! `debug-locks` feature, records the owning thread plus an acquisition backtrace
//! and reports the holder when an acquisition cannot complete — covering the case
//! the async `parking_lot` deadlock detector cannot: a sync lock whose holder is a
//! suspended task or a re-entrant acquire on a runtime so starved that the
//! detector task itself can never be scheduled to run.
//!
//! - Re-entrancy (a thread locking a `Mutex` it already holds) is reported
//!   immediately, without waiting out the timeout.
//! - A holder still stuck after `DEBUG_LOCKS_DURATION_MS` is reported with both
//!   the blocked-acquire and holder-acquire backtraces, then the process exits.
//!
//! Without `debug-locks` this is exactly `parking_lot::Mutex<T>` via type alias —
//! zero overhead, identical semantics.

use cfg_if::cfg_if;

cfg_if! {
    if #[cfg(feature = "debug-locks")] {
        use std::sync::atomic::{AtomicBool, Ordering};
        use std::sync::Mutex as StdMutex;
        use std::thread::ThreadId;

        // One report is enough; the other blocked threads would print the same standoff.
        static REPORTED: AtomicBool = AtomicBool::new(false);

        #[derive(Debug)]
        struct HolderInfo {
            thread_id: ThreadId,
            thread_name: String,
            backtrace: backtrace::Backtrace,
        }

        /// See module docs. Holder state lives in a `std` mutex so it never enters
        /// `parking_lot`'s own deadlock bookkeeping.
        pub struct TrackedMutex<T> {
            holder: StdMutex<Option<HolderInfo>>,
            inner: parking_lot::Mutex<T>,
        }

        impl<T> TrackedMutex<T> {
            /// Wrap `val` in a tracked mutex.
            pub fn new(val: T) -> Self {
                Self {
                    holder: StdMutex::new(None),
                    inner: parking_lot::Mutex::new(val),
                }
            }

            /// Lock, recording the holder. A re-entrant acquire reports the standoff
            /// and exits the process immediately; off-wasm a non-re-entrant contended
            /// acquire waits up to `DEBUG_LOCKS_DURATION_MS` before reporting, on
            /// single-threaded wasm it reports at once. The lock is held until the
            /// returned guard drops.
            pub fn lock(&self) -> TrackedMutexGuard<'_, T> {
                // Fast path: uncontended.
                if let Some(guard) = self.inner.try_lock() {
                    self.record_holder();
                    return TrackedMutexGuard { mutex: self, guard };
                }
                // Contended: a re-entrant acquire would otherwise wait the full timeout for
                // a lock this very thread holds, so catch it now.
                if self.held_by_current_thread() {
                    self.report("re-entrant lock (the blocked thread already holds it)");
                }
                // Otherwise hand off to the per-target wait strategy.
                self.wait_or_report()
            }

            // Has a timer and (potentially) other threads: contention may be transient, so
            // wait — only a holder still stuck past the timeout is a deadlock.
            #[cfg(not(all(target_arch = "wasm32", target_os = "unknown")))]
            fn wait_or_report(&self) -> TrackedMutexGuard<'_, T> {
                let timeout =
                    std::time::Duration::from_millis(super::DEBUG_LOCKS_DURATION_MS as u64);
                match self.inner.try_lock_for(timeout) {
                    Some(guard) => {
                        self.record_holder();
                        TrackedMutexGuard { mutex: self, guard }
                    }
                    None => self.report("acquisition timed out"),
                }
            }

            // Single-threaded wasm: there is no timer (try_lock_for would panic on
            // Instant::now), and the holder is a suspended task that cannot run while we
            // block the only thread — so any contention here is already a deadlock.
            #[cfg(all(target_arch = "wasm32", target_os = "unknown"))]
            fn wait_or_report(&self) -> TrackedMutexGuard<'_, T> {
                self.report("contended on single-threaded executor (holder is a suspended task)")
            }

            /// Lock without blocking, recording the holder on success.
            /// Returns `None` immediately if the lock is held; the lock is held
            /// until the returned guard drops.
            pub fn try_lock(&self) -> Option<TrackedMutexGuard<'_, T>> {
                let guard = self.inner.try_lock()?;
                self.record_holder();
                Some(TrackedMutexGuard { mutex: self, guard })
            }

            fn record_holder(&self) {
                let cur = std::thread::current();
                *self.holder.lock().unwrap() = Some(HolderInfo {
                    thread_id: cur.id(),
                    thread_name: cur.name().unwrap_or("<unnamed>").to_string(),
                    backtrace: backtrace::Backtrace::new_unresolved(),
                });
            }

            fn clear_holder(&self) {
                *self.holder.lock().unwrap() = None;
            }

            fn held_by_current_thread(&self) -> bool {
                let cur = std::thread::current().id();
                self.holder
                    .lock()
                    .unwrap()
                    .as_ref()
                    .is_some_and(|h| h.thread_id == cur)
            }

            fn report(&self, reason: &str) -> ! {
                use core::fmt::Write as _;
                // Only the first blocked thread prints; wedge the rest so output isn't interleaved.
                if REPORTED.swap(true, Ordering::SeqCst) {
                    loop {
                        std::thread::park();
                    }
                }
                let cur = std::thread::current();
                let mut r = String::new();
                let _ = writeln!(r, "\n===== TrackedMutex DEADLOCK: {reason} =====");
                let _ = writeln!(
                    r,
                    "blocked thread: {:?} ({})",
                    cur.id(),
                    cur.name().unwrap_or("<unnamed>")
                );
                let mut blocked_bt = backtrace::Backtrace::new_unresolved();
                blocked_bt.resolve();
                let _ = writeln!(
                    r,
                    "blocked-acquire backtrace:\n{}",
                    indent::indent_all_by(4, format!("{blocked_bt:?}"))
                );
                match self.holder.lock().unwrap().as_ref() {
                    Some(h) => {
                        let reentrant = h.thread_id == cur.id();
                        let _ = writeln!(
                            r,
                            "current holder: {:?} ({}){}",
                            h.thread_id,
                            h.thread_name,
                            if reentrant {
                                "   <<< SAME THREAD — RE-ENTRANT DEADLOCK"
                            } else {
                                ""
                            }
                        );
                        let mut hbt = h.backtrace.clone();
                        hbt.resolve();
                        let _ = writeln!(
                            r,
                            "holder-acquire backtrace:\n{}",
                            indent::indent_all_by(4, format!("{hbt:?}"))
                        );
                    }
                    None => {
                        let _ = writeln!(
                            r,
                            "current holder: <none recorded> — contended but unowned (lost wakeup?)"
                        );
                    }
                }
                let _ = writeln!(r, "===== end TrackedMutex deadlock report =====\n");
                // Belt-and-suspenders: native stderr may not be captured by the test
                // harness, so also drop the report to a well-known file.
                eprintln!("{r}");
                // Native stderr isn't always captured by the harness (e.g. flutter test),
                // so also drop the report to a well-known file where a filesystem exists.
                #[cfg(not(all(target_arch = "wasm32", target_os = "unknown")))]
                {
                    let _ = std::fs::write(
                        std::env::temp_dir().join("veilid_tracked_mutex_deadlock.txt"),
                        r.as_bytes(),
                    );
                }
                std::process::exit(1);
            }
        }

        impl<T: core::fmt::Debug> core::fmt::Debug for TrackedMutex<T> {
            fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
                // Mirror parking_lot::Mutex's Debug (prints data, or <locked>).
                core::fmt::Debug::fmt(&self.inner, f)
            }
        }

        /// Guard for a `TrackedMutex`; clears the recorded holder on drop.
        pub struct TrackedMutexGuard<'a, T> {
            mutex: &'a TrackedMutex<T>,
            guard: parking_lot::MutexGuard<'a, T>,
        }

        impl<T> Drop for TrackedMutexGuard<'_, T> {
            fn drop(&mut self) {
                self.mutex.clear_holder();
            }
        }

        impl<T> core::ops::Deref for TrackedMutexGuard<'_, T> {
            type Target = T;
            fn deref(&self) -> &T {
                &self.guard
            }
        }

        impl<T> core::ops::DerefMut for TrackedMutexGuard<'_, T> {
            fn deref_mut(&mut self) -> &mut T {
                &mut self.guard
            }
        }
    } else {
        /// Without `debug-locks`, a plain `parking_lot::Mutex<T>`.
        pub type TrackedMutex<T> = parking_lot::Mutex<T>;
        /// Without `debug-locks`, a plain `parking_lot::MutexGuard<T>`.
        pub type TrackedMutexGuard<'a, T> = parking_lot::MutexGuard<'a, T>;
    }
}
