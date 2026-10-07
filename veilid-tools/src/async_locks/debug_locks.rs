use super::*;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Mutex as StdMutex;

static NEXT_GUARD_ID: AtomicUsize = AtomicUsize::new(0);

#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct RawGuardId(usize);

/// Per-lock debug state; active guards live in the lock so acquire/release never touches global state.
#[derive(Debug)]
struct LockIdContainerInner {
    create_backtrace: backtrace::Backtrace,
    // std mutex keeps this out of parking_lot's deadlock detector
    active_guards: StdMutex<HashMap<RawGuardId, backtrace::Backtrace>>,
}

#[derive(Clone, Debug)]
pub(super) struct LockIdContainer {
    inner: Arc<LockIdContainerInner>,
}

impl LockIdContainer {
    pub fn next() -> LockIdContainer {
        LockIdContainer {
            inner: Arc::new(LockIdContainerInner {
                create_backtrace: debug_lock_backtrace(),
                active_guards: StdMutex::new(HashMap::new()),
            }),
        }
    }

    pub fn report_deadlock(&self, desc: &str) -> ! {
        let active_guard_backtraces: Vec<String> = {
            let active_guards = self.inner.active_guards.lock().unwrap();
            active_guards
                .values()
                .map(|bt| bt.trim().to_string())
                .collect()
        };

        use core::fmt::Write as _;
        let mut r = String::new();
        let _ = writeln!(
            r,
            "Deadlock detected! ({})\n\nLock creation backtrace:\n{}\n\nActive guard backtraces:\n",
            desc,
            self.inner.create_backtrace.trim(),
        );
        for (n, agb) in active_guard_backtraces.iter().enumerate() {
            let _ = writeln!(r, "{}:\n{}\n", n, indent::indent_all_by(4, agb));
        }

        let report_backtrace = backtrace::Backtrace::new_unresolved().trim();
        let _ = writeln!(r, "Panic backtrace:\n{}", report_backtrace);

        eprintln!("{r}");
        // Native stderr isn't always captured by the harness (e.g. flutter test), so
        // also drop the report to a well-known file where a filesystem exists.
        #[cfg(not(all(target_arch = "wasm32", target_os = "unknown")))]
        {
            let _ = std::fs::write(
                std::env::temp_dir().join("veilid_async_lock_deadlock.txt"),
                r.as_bytes(),
            );
        }

        std::process::exit(1);
    }
}

#[derive(Debug)]
pub struct GuardIdContainerInner {
    raw_guard_id: RawGuardId,
    lock_id_container: LockIdContainer,
}

impl Drop for GuardIdContainerInner {
    fn drop(&mut self) {
        self.lock_id_container
            .inner
            .active_guards
            .lock()
            .unwrap()
            .remove(&self.raw_guard_id);
    }
}

#[derive(Clone, Debug)]
pub struct GuardIdContainer {
    inner: Arc<GuardIdContainerInner>,
}

impl GuardIdContainer {
    #[expect(dead_code)]
    pub fn id(&self) -> RawGuardId {
        self.inner.raw_guard_id
    }

    pub fn next(lock_id_container: LockIdContainer) -> GuardIdContainer {
        let raw_guard_id = RawGuardId(NEXT_GUARD_ID.fetch_add(1, Ordering::Relaxed));
        // capture outside the guard mutex so the unwind is never in a critical section
        let lock_backtrace = debug_lock_backtrace();
        lock_id_container
            .inner
            .active_guards
            .lock()
            .unwrap()
            .insert(raw_guard_id, lock_backtrace);
        GuardIdContainer {
            inner: Arc::new(GuardIdContainerInner {
                raw_guard_id,
                lock_id_container,
            }),
        }
    }
}
