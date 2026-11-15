use super::*;
#[cfg(feature = "debug-locks")]
use std::backtrace::Backtrace;

/// Per-mode lock guards held by a RecordLockGuard.
#[derive(Debug)]
pub(super) enum RecordLockGuards {
    Lifetime {
        _lifetime: AsyncRwLockWriteGuardArc<()>,
    },
    Watch {
        _lifetime: AsyncRwLockReadGuardArc<()>,
        _watch: AsyncMutexGuardArc<()>,
    },
    Transaction {
        _lifetime: AsyncRwLockReadGuardArc<()>,
        _transaction: AsyncRwLockWriteGuardArc<()>,
    },
}

/// Record lock guard for a single record
#[derive(Debug)]
#[must_use]
pub struct RecordLockGuard<R: RecordLockPurpose, S: RecordLockPurpose> {
    record_lock: Arc<RecordLock<R, S>>,
    _guards: RecordLockGuards,
    opt_background: Option<BackgroundHold>,
    #[cfg(feature = "debug-locks")]
    id: usize,
    #[cfg(feature = "debug-locks")]
    active_guards: Arc<Mutex<HashMap<usize, Backtrace>>>,
}

impl<R: RecordLockPurpose, S: RecordLockPurpose> RecordLockGuard<R, S> {
    pub(super) fn new(
        record_lock: Arc<RecordLock<R, S>>,
        guards: RecordLockGuards,
        opt_background: Option<BackgroundHold>,
    ) -> Self {
        #[cfg(feature = "debug-locks")]
        let (id, active_guards) = {
            let id = GUARD_ID.fetch_add(1, Ordering::AcqRel);
            let active_guards = record_lock.get_active_guards();
            active_guards.lock().insert(id, Backtrace::force_capture());
            (id, active_guards)
        };

        Self {
            record_lock,
            _guards: guards,
            opt_background,
            #[cfg(feature = "debug-locks")]
            id,
            #[cfg(feature = "debug-locks")]
            active_guards,
        }
    }

    pub fn record(&self) -> OpaqueRecordKey {
        self.record_lock.record()
    }

    /// `fut`'s output, or None when a foreground acquisition pre-empts this background hold first
    pub fn until_preempted<F: Future>(&self, fut: F) -> impl Future<Output = Option<F::Output>> {
        until_preempted(self.opt_background.as_ref(), fut)
    }
}

impl<R: RecordLockPurpose, S: RecordLockPurpose> Drop for RecordLockGuard<R, S> {
    fn drop(&mut self) {
        #[cfg(feature = "debug-locks")]
        self.active_guards.lock().remove(&self.id);

        if let Some(background) = &self.opt_background {
            self.record_lock.drop_background_hold(background);
        }
        self.record_lock.drop_record_lock_guard();
    }
}

impl<R: RecordLockPurpose, S: RecordLockPurpose> fmt::Display for RecordLockGuard<R, S> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Record({})", self.record())
    }
}
