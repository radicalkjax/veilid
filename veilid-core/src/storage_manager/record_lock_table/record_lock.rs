use super::*;
#[cfg(feature = "debug-locks")]
use std::backtrace::Backtrace;

#[cfg(feature = "debug-locks")]
pub(super) static GUARD_ID: AtomicUsize = AtomicUsize::new(0);

/// Subkey lock management structure
#[derive(Debug)]
struct RecordLockInner<R: RecordLockPurpose, S: RecordLockPurpose> {
    /// Per-subkey mutexes, collected when no guard holds them
    subkey_lock_table: WeakValueHashMap<ValueSubkey, Weak<AsyncMutex<()>>>,
    /// What the current holders hold the record and its subkeys for
    purpose_state: RecordLockPurposeState<R, S>,
    /// Number of live peek guards
    peek_count: usize,
    /// Background holds a foreground acquisition can stop, by id
    background_holds: HashMap<u64, BackgroundHoldEntry<R, S>>,
    /// What waiting foreground acquisitions will hold and why, by id
    foreground_waits: HashMap<u64, RecordLockRequest<R, S>>,
    /// Next id for a background hold or a foreground wait
    next_id: u64,
}

/// What an acquisition holds and why: its purpose, or for a peek, which has none, its priority
#[derive(Debug, Clone)]
pub(super) enum RecordLockRequest<R: RecordLockPurpose, S: RecordLockPurpose> {
    Record(R),
    Subkey(ValueSubkey, S),
    Peek(RecordLockPriority),
}

impl<R: RecordLockPurpose, S: RecordLockPurpose> RecordLockRequest<R, S> {
    pub fn hold(&self) -> RecordLockHold {
        match self {
            Self::Record(purpose) => purpose.record_lock_mode().into(),
            Self::Subkey(subkey, _) => RecordLockHold::Subkey(*subkey),
            Self::Peek(_) => RecordLockHold::Peek,
        }
    }

    pub fn priority(&self) -> RecordLockPriority {
        match self {
            Self::Record(purpose) => purpose.record_lock_priority(),
            Self::Subkey(_, purpose) => purpose.record_lock_priority(),
            Self::Peek(priority) => *priority,
        }
    }

    /// Whether this foreground request stops `background`: their holds conflict, and this purpose
    /// does not wait that one out
    fn preempts(&self, background: &Self) -> bool {
        if !self.hold().conflicts_with(&background.hold()) {
            return false;
        }
        match (self, background) {
            (Self::Record(purpose), Self::Record(background)) => purpose.preempts(background),
            (Self::Subkey(_, purpose), Self::Subkey(_, background)) => purpose.preempts(background),
            _ => true,
        }
    }
}

/// A registered background hold
#[derive(Debug)]
struct BackgroundHoldEntry<R: RecordLockPurpose, S: RecordLockPurpose> {
    /// What the holder holds and why
    request: RecordLockRequest<R, S>,
    /// Dropped to stop the holder; None once a foreground acquisition has
    opt_stop_source: Option<StopSource>,
}

/// A background holder's registration, carried by its guard
#[derive(Debug)]
pub(super) struct BackgroundHold {
    /// None when a foreground acquisition that pre-empts it was already waiting, so it starts stopped
    opt_id: Option<u64>,
    /// Fires when a foreground acquisition pre-empts the holder
    stop_token: StopToken,
}

impl BackgroundHold {
    /// Refused at registration: a foreground acquisition that pre-empts it was already waiting
    pub fn refused(&self) -> bool {
        self.opt_id.is_none()
    }

    pub fn stop_token(&self) -> StopToken {
        self.stop_token.clone()
    }
}

/// A foreground acquisition's wait for its locks. Until it drops, background holds that would
/// conflict with it start stopped.
#[must_use]
pub(super) struct ForegroundWait<R: RecordLockPurpose, S: RecordLockPurpose> {
    /// The record being waited on
    record_lock: Arc<RecordLock<R, S>>,
    /// The wait's id in `foreground_waits`
    id: u64,
}

impl<R: RecordLockPurpose, S: RecordLockPurpose> Drop for ForegroundWait<R, S> {
    fn drop(&mut self) {
        self.record_lock
            .inner
            .lock()
            .foreground_waits
            .remove(&self.id);
    }
}

/// Record lock management structure
#[derive(Debug)]
pub(super) struct RecordLock<R: RecordLockPurpose, S: RecordLockPurpose> {
    /// Registry for logging
    registry: VeilidComponentRegistry,
    /// The record this lock covers
    record: OpaqueRecordKey,
    /// Write = Lifetime mode. Read = any non-Lifetime mode
    lifetime_lock: Arc<AsyncRwLock<()>>,
    /// Write = Transaction mode. Read = Subkey mode
    transaction_lock: Arc<AsyncRwLock<()>>,
    /// Mutual exclusion among Watch ops on this record
    watch_lock: Arc<AsyncMutex<()>>,
    /// Subkey locks, purpose state and peek count
    inner: Mutex<RecordLockInner<R, S>>,
    /// Backtrace of each live guard, for lock debugging
    #[cfg(feature = "debug-locks")]
    active_guards: Arc<Mutex<HashMap<usize, Backtrace>>>,
}

impl<R: RecordLockPurpose, S: RecordLockPurpose> VeilidComponentRegistryAccessor
    for RecordLock<R, S>
{
    fn registry(&self) -> VeilidComponentRegistry {
        self.registry.clone()
    }
}

impl<R: RecordLockPurpose, S: RecordLockPurpose> RecordLock<R, S> {
    pub fn new(registry: VeilidComponentRegistry, record: OpaqueRecordKey) -> Self {
        Self {
            registry,
            record,
            lifetime_lock: Arc::new(AsyncRwLock::new(())),
            transaction_lock: Arc::new(AsyncRwLock::new(())),
            watch_lock: Arc::new(AsyncMutex::new(())),
            inner: Mutex::new(RecordLockInner {
                subkey_lock_table: WeakValueHashMap::new(),
                purpose_state: RecordLockPurposeState {
                    whole_record_lock_purpose: None,
                    subkey_lock_purpose: BTreeMap::new(),
                },
                peek_count: 0,
                background_holds: HashMap::new(),
                foreground_waits: HashMap::new(),
                next_id: 0,
            }),
            #[cfg(feature = "debug-locks")]
            active_guards: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    pub fn record(&self) -> OpaqueRecordKey {
        self.record.clone()
    }
    pub fn get_lifetime_lock(&self) -> Arc<AsyncRwLock<()>> {
        self.lifetime_lock.clone()
    }
    pub fn get_transaction_lock(&self) -> Arc<AsyncRwLock<()>> {
        self.transaction_lock.clone()
    }
    pub fn get_watch_lock(&self) -> Arc<AsyncMutex<()>> {
        self.watch_lock.clone()
    }
    pub fn purpose_state(&self) -> RecordLockPurposeState<R, S> {
        self.inner.lock().purpose_state.clone()
    }
    pub fn set_record_purpose(&self, purpose: R) {
        let mut inner = self.inner.lock();
        inner.purpose_state.whole_record_lock_purpose = Some(purpose);
        inner.purpose_state.subkey_lock_purpose.clear();
    }
    pub fn get_subkey_lock(&self, subkey: ValueSubkey) -> Arc<AsyncMutex<()>> {
        let mut inner = self.inner.lock();
        inner.subkey_lock_table.remove_expired();
        inner
            .subkey_lock_table
            .entry(subkey)
            .or_insert_with(|| Arc::new(AsyncMutex::new(())))
    }

    pub fn set_subkey_purpose(&self, subkey: ValueSubkey, purpose: S) {
        let mut inner = self.inner.lock();
        inner.purpose_state.whole_record_lock_purpose = None;
        inner
            .purpose_state
            .subkey_lock_purpose
            .insert(subkey, purpose);
    }

    #[expect(dead_code)]
    pub fn get_peek_count(&self) -> usize {
        self.inner.lock().peek_count
    }

    pub fn add_peek(&self) {
        self.inner.lock().peek_count += 1;
    }

    #[cfg(feature = "debug-locks")]
    pub(super) fn get_active_guards(&self) -> Arc<Mutex<HashMap<usize, Backtrace>>> {
        self.active_guards.clone()
    }

    pub(super) fn drop_record_lock_guard(&self) {
        self.inner.lock().purpose_state.whole_record_lock_purpose = None;
    }

    pub(super) fn drop_subkey_lock_guard(&self, subkey: ValueSubkey) {
        self.inner
            .lock()
            .purpose_state
            .subkey_lock_purpose
            .remove(&subkey);
    }

    pub(super) fn drop_peek_lock_guard(&self) {
        self.inner.lock().peek_count -= 1;
    }

    /// For a foreground `request`: stop the background holds it pre-empts, and start later ones
    /// stopped until the returned wait drops. None for other priorities.
    pub(super) fn preempt_background(
        self: &Arc<Self>,
        request: &RecordLockRequest<R, S>,
    ) -> Option<ForegroundWait<R, S>> {
        if request.priority() != RecordLockPriority::Foreground {
            return None;
        }
        let mut stopped = vec![];
        let id = {
            let mut inner = self.inner.lock();
            for entry in inner.background_holds.values_mut() {
                if request.preempts(&entry.request) {
                    if let Some(stop_source) = entry.opt_stop_source.take() {
                        stopped.push((entry.request.clone(), stop_source));
                    }
                }
            }
            let id = inner.next_id;
            inner.next_id += 1;
            inner.foreground_waits.insert(id, request.clone());
            id
        };
        // Dropping the sources wakes their holders, so outside the inner lock
        for (background, stop_source) in stopped {
            veilid_log!(self debug "RecordLock({}): {:?} pre-empts background {:?}", self.record, request, background);
            drop(stop_source);
        }
        Some(ForegroundWait {
            record_lock: self.clone(),
            id,
        })
    }

    /// For a background `request`: register it so a foreground acquisition that pre-empts it can
    /// stop it. None for other priorities.
    pub(super) fn register_background(
        &self,
        request: RecordLockRequest<R, S>,
    ) -> Option<BackgroundHold> {
        if request.priority() != RecordLockPriority::Background {
            return None;
        }
        let stop_source = StopSource::new();
        let stop_token = stop_source.token();
        let mut inner = self.inner.lock();
        if inner
            .foreground_waits
            .values()
            .any(|foreground| foreground.preempts(&request))
        {
            drop(inner);
            drop(stop_source);
            return Some(BackgroundHold {
                opt_id: None,
                stop_token,
            });
        }
        let id = inner.next_id;
        inner.next_id += 1;
        inner.background_holds.insert(
            id,
            BackgroundHoldEntry {
                request,
                opt_stop_source: Some(stop_source),
            },
        );
        Some(BackgroundHold {
            opt_id: Some(id),
            stop_token,
        })
    }

    pub(super) fn drop_background_hold(&self, background_hold: &BackgroundHold) {
        if let Some(id) = background_hold.opt_id {
            self.inner.lock().background_holds.remove(&id);
        }
    }
}
