mod peek_lock_guard;
mod peeks_lock_guard;
mod record_lock;
mod record_lock_guard;
mod records_lock_guard;
mod subkey_lock_guard;

pub use peek_lock_guard::*;
pub use peeks_lock_guard::*;
pub use record_lock_guard::*;
pub use records_lock_guard::*;
pub use subkey_lock_guard::*;

use super::*;
use record_lock::*;
use record_lock_guard::RecordLockGuards;
use stop_token::future::FutureExt as _;
use weak_table::WeakValueHashMap;

impl_veilid_log_facility!("stor");

/// Concurrency mode of a record lock acquisition.
///
/// Compatibility (Y = concurrent, N = blocks):
///
/// |             | Lifetime | Watch | Transaction | Subkey       | Peek |
/// |-------------|:--------:|:-----:|:-----------:|:------------:|:----:|
/// | Lifetime    |    N     |   N   |      N      |      N       |  N   |
/// | Watch       |    N     |   N   |      Y      |      Y       |  Y   |
/// | Transaction |    N     |   Y   |      N      |      N       |  Y   |
/// | Subkey      |    N     |   Y   |      N      | (per subkey) |  Y   |
/// | Peek        |    N     |   Y   |      Y      |      Y       |  Y   |
#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub enum RecordLockMode {
    Lifetime,
    Watch,
    Transaction,
}

/// What a guard holds, named as in `RecordLockMode`'s compatibility table
#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub enum RecordLockHold {
    Lifetime,
    Watch,
    Transaction,
    Subkey(ValueSubkey),
    Peek,
}

impl RecordLockHold {
    /// Whether one of the two holds blocks the other, per the compatibility table
    pub fn conflicts_with(&self, other: &RecordLockHold) -> bool {
        match (self, other) {
            (Self::Lifetime, _) | (_, Self::Lifetime) => true,
            (Self::Peek, _) | (_, Self::Peek) => false,
            (Self::Watch, Self::Watch) => true,
            (Self::Watch, _) | (_, Self::Watch) => false,
            (Self::Transaction, _) | (_, Self::Transaction) => true,
            (Self::Subkey(a), Self::Subkey(b)) => a == b,
        }
    }
}

impl From<RecordLockMode> for RecordLockHold {
    fn from(mode: RecordLockMode) -> Self {
        match mode {
            RecordLockMode::Lifetime => Self::Lifetime,
            RecordLockMode::Watch => Self::Watch,
            RecordLockMode::Transaction => Self::Transaction,
        }
    }
}

/// How an acquisition treats background work on the record: API operations pre-empt the
/// background operations they conflict with, judged by `RecordLockMode`'s compatibility table
#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub enum RecordLockPriority {
    /// Stops the conflicting background holds before waiting for its locks
    Foreground,
    /// Neither stops background holds nor yields
    Normal,
    /// Yields: its guard's `until_preempted` ends early when a conflicting foreground acquisition
    /// arrives, and starts stopped if one is already waiting
    Background,
}

pub trait RecordLockPurpose:
    fmt::Debug + Clone + Eq + PartialEq + Ord + PartialOrd + core::hash::Hash
{
    fn record_lock_mode(&self) -> RecordLockMode;
    fn record_lock_priority(&self) -> RecordLockPriority;

    /// Whether a foreground acquisition for this purpose stops a conflicting background holder for
    /// `background`; one it does not stop, it waits out like any other holder
    fn preempts(&self, _background: &Self) -> bool {
        true
    }
}

/// Run `fut` to completion, or for a background hold, until a foreground acquisition pre-empts
/// it: then None, and the holder should drop its guard without finishing. A background holder
/// runs every await that can outlast a local operation (a fanout, an admission gate) through this.
fn until_preempted<F: Future>(
    opt_background: Option<&BackgroundHold>,
    fut: F,
) -> impl Future<Output = Option<F::Output>> {
    let opt_stop_token = opt_background.map(|background| background.stop_token());
    async move {
        match opt_stop_token {
            Some(stop_token) => fut.timeout_at(stop_token).await.ok(),
            None => Some(fut.await),
        }
    }
}

/// Snapshot of the purpose for which a record or its subkeys are locked
#[derive(Debug, Clone)]
pub struct RecordLockPurposeState<R: RecordLockPurpose, S: RecordLockPurpose> {
    pub whole_record_lock_purpose: Option<R>,
    pub subkey_lock_purpose: BTreeMap<ValueSubkey, S>,
}

impl<R: RecordLockPurpose, S: RecordLockPurpose> Default for RecordLockPurposeState<R, S> {
    fn default() -> Self {
        Self {
            whole_record_lock_purpose: None,
            subkey_lock_purpose: BTreeMap::new(),
        }
    }
}

/// Table for all record locks that uses weak hash maps to auto-collect when guards drop
#[derive(Debug)]
struct RecordLockTableInner<R: RecordLockPurpose, S: RecordLockPurpose> {
    record_lock_table: WeakValueHashMap<OpaqueRecordKey, Weak<RecordLock<R, S>>>,
}

/// Interface to record locking mechanism
#[derive(Clone)]
pub struct RecordLockTable<R: RecordLockPurpose, S: RecordLockPurpose> {
    registry: VeilidComponentRegistry,
    inner: Arc<Mutex<RecordLockTableInner<R, S>>>,
}

impl<R: RecordLockPurpose, S: RecordLockPurpose> fmt::Debug for RecordLockTable<R, S> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("RecordLockTable")
            .field("inner", &self.inner)
            .finish()
    }
}

impl<R: RecordLockPurpose, S: RecordLockPurpose> VeilidComponentRegistryAccessor
    for RecordLockTable<R, S>
{
    fn registry(&self) -> VeilidComponentRegistry {
        self.registry.clone()
    }
}

impl<R: RecordLockPurpose, S: RecordLockPurpose> RecordLockTable<R, S> {
    pub fn new(registry: VeilidComponentRegistry) -> Self {
        Self {
            registry,
            inner: Arc::new(Mutex::new(RecordLockTableInner::<R, S> {
                record_lock_table: WeakValueHashMap::new(),
            })),
        }
    }

    fn get_or_create_record_lock(&self, record: &OpaqueRecordKey) -> Arc<RecordLock<R, S>> {
        let mut inner = self.inner.lock();
        inner.record_lock_table.remove_expired();
        inner
            .record_lock_table
            .entry(record.clone())
            .or_insert_with(|| Arc::new(RecordLock::new(self.registry.clone(), record.clone())))
    }

    pub async fn lock_record(&self, record: OpaqueRecordKey, purpose: R) -> RecordLockGuard<R, S> {
        let recorder = DurationRecorder::new("RecordLockTable::lock_record", |_name, _start| {
            #[cfg(feature = "debug-locks")]
            veilid_log!(self debug "{}[start={:#}](record: {:?}, purpose: {:?})", _name, _start, record, purpose);
        });
        recorder
            .record_fut(
                async {
                    let record_lock = self.get_or_create_record_lock(&record);
                    let mode = purpose.record_lock_mode();
                    let request = RecordLockRequest::Record(purpose.clone());
                    let _opt_foreground_wait = record_lock.preempt_background(&request);
                    let guards = acquire_record_lock_guards(&record_lock, mode).await;
                    record_lock.set_record_purpose(purpose);
                    let opt_background = record_lock.register_background(request);
                    RecordLockGuard::new(record_lock, guards, opt_background)
                },
                |_name, _start, _dur, ret| {
                    #[cfg(feature = "debug-locks")]
                    veilid_log!(self debug "{}[start={:#} dur={:#}]() done", _name, _start, _dur);
                    ret
                },
            )
            .await
    }

    pub fn try_lock_record(
        &self,
        record: OpaqueRecordKey,
        purpose: R,
    ) -> Option<RecordLockGuard<R, S>> {
        let record_lock = self.get_or_create_record_lock(&record);
        let mode = purpose.record_lock_mode();
        let guards = try_acquire_record_lock_guards(&record_lock, mode)?;
        let opt_background =
            record_lock.register_background(RecordLockRequest::Record(purpose.clone()));
        if opt_background.as_ref().is_some_and(BackgroundHold::refused) {
            return None;
        }
        record_lock.set_record_purpose(purpose);
        Some(RecordLockGuard::new(record_lock, guards, opt_background))
    }

    pub async fn lock_records(
        &self,
        mut records: Vec<OpaqueRecordKey>,
        purpose: R,
    ) -> RecordsLockGuard<R, S> {
        let recorder = DurationRecorder::new("RecordLockTable::lock_records", |_name, _start| {
            #[cfg(feature = "debug-locks")]
            veilid_log!(self debug "{}[start={:#}](records: {:?}, purpose: {:?})", _name, _start, records, purpose);
        });
        recorder
            .record_fut(
                async {
                    // Always lock in sorted order to avoid deadlocks
                    records.sort_unstable();
                    let mode = purpose.record_lock_mode();

                    let record_locks = {
                        let mut inner = self.inner.lock();
                        inner.record_lock_table.remove_expired();
                        records
                            .into_iter()
                            .map(|record| {
                                inner
                                    .record_lock_table
                                    .entry(record.clone())
                                    .or_insert_with(|| {
                                        Arc::new(RecordLock::new(
                                            self.registry.clone(),
                                            record.clone(),
                                        ))
                                    })
                            })
                            .collect::<Vec<_>>()
                    };

                    let request = RecordLockRequest::Record(purpose.clone());
                    let _foreground_waits = record_locks
                        .iter()
                        .filter_map(|record_lock| record_lock.preempt_background(&request))
                        .collect::<Vec<_>>();

                    let mut record_lock_guards = vec![];
                    for record_lock in record_locks {
                        let guards = acquire_record_lock_guards(&record_lock, mode).await;
                        record_lock.set_record_purpose(purpose.clone());
                        let opt_background = record_lock.register_background(request.clone());
                        record_lock_guards.push(RecordLockGuard::new(
                            record_lock,
                            guards,
                            opt_background,
                        ));
                    }

                    RecordsLockGuard::new(record_lock_guards)
                },
                |_name, _start, _dur, ret| {
                    #[cfg(feature = "debug-locks")]
                    veilid_log!(self debug "{}[start={:#} dur={:#}]() done", _name, _start, _dur);
                    ret
                },
            )
            .await
    }

    #[expect(dead_code)]
    pub fn try_lock_records(
        &self,
        mut records: Vec<OpaqueRecordKey>,
        purpose: R,
    ) -> Option<RecordsLockGuard<R, S>> {
        // Always lock in sorted order to avoid deadlocks
        records.sort_unstable();
        let mode = purpose.record_lock_mode();

        let record_locks = {
            let mut inner = self.inner.lock();
            inner.record_lock_table.remove_expired();
            records
                .into_iter()
                .map(|record| {
                    inner
                        .record_lock_table
                        .entry(record.clone())
                        .or_insert_with(|| {
                            Arc::new(RecordLock::new(self.registry.clone(), record.clone()))
                        })
                })
                .collect::<Vec<_>>()
        };

        let mut record_lock_guards = vec![];
        for record_lock in record_locks {
            let guards = try_acquire_record_lock_guards(&record_lock, mode)?;
            let opt_background =
                record_lock.register_background(RecordLockRequest::Record(purpose.clone()));
            if opt_background.as_ref().is_some_and(BackgroundHold::refused) {
                return None;
            }
            record_lock.set_record_purpose(purpose.clone());
            record_lock_guards.push(RecordLockGuard::new(record_lock, guards, opt_background));
        }

        Some(RecordsLockGuard::new(record_lock_guards))
    }

    /// Bulk, non-blocking peek-lock acquisition: all records or none
    pub fn try_peek_locks(
        &self,
        mut records: Vec<OpaqueRecordKey>,
    ) -> Option<PeeksLockGuard<R, S>> {
        // Always lock in sorted order to avoid deadlocks
        records.sort_unstable();
        let mut guards = vec![];
        for record in records {
            let record_lock = self.get_or_create_record_lock(&record);
            let lifetime_guard = record_lock.get_lifetime_lock().try_read_arc()?;
            record_lock.add_peek();
            guards.push(PeekLockGuard::new(record_lock, lifetime_guard, None));
        }
        Some(PeeksLockGuard::new(guards))
    }

    pub async fn lock_subkey(
        &self,
        record: OpaqueRecordKey,
        subkey: ValueSubkey,
        purpose: S,
    ) -> SubkeyLockGuard<R, S> {
        let recorder = DurationRecorder::new("RecordLockTable::lock_subkey", |_name, _start| {
            #[cfg(feature = "debug-locks")]
            veilid_log!(self debug "{}[start={:#}](record: {:?}, subkey: {}, purpose: {:?})", _name, _start, record, subkey, purpose);
        });
        recorder
            .record_fut(
                async {
                    let record_lock = self.get_or_create_record_lock(&record);
                    let request = RecordLockRequest::Subkey(subkey, purpose.clone());
                    let _opt_foreground_wait = record_lock.preempt_background(&request);

                    let lifetime_guard = acquire_lifetime_read(&record_lock, "lock_subkey").await;
                    let transaction_guard =
                        acquire_transaction_read(&record_lock, "lock_subkey").await;

                    let subkey_lock = record_lock.get_subkey_lock(subkey);
                    let subkey_lock_guard = subkey_lock
                        .lock_arc()
                        .measure_debug(TimestampDuration::new_secs(10), |msg| {
                            veilid_log!(self debug "RecordLockTable::lock_subkey lock_arc: {}", msg);
                        })
                        .await;
                    record_lock.set_subkey_purpose(subkey, purpose);
                    let opt_background = record_lock.register_background(request);

                    SubkeyLockGuard::new(
                        record_lock,
                        lifetime_guard,
                        transaction_guard,
                        subkey_lock_guard,
                        subkey_lock,
                        subkey,
                        opt_background,
                    )
                },
                |_name, _start, _dur, ret| {
                    #[cfg(feature = "debug-locks")]
                    veilid_log!(self debug "{}[start={:#} dur={:#}]() done", _name, _start, _dur);
                    ret
                },
            )
            .await
    }

    pub fn try_lock_subkey(
        &self,
        record: OpaqueRecordKey,
        subkey: ValueSubkey,
        purpose: S,
    ) -> Option<SubkeyLockGuard<R, S>> {
        let record_lock = self.get_or_create_record_lock(&record);

        let lifetime_guard = record_lock.get_lifetime_lock().try_read_arc()?;
        let transaction_guard = record_lock.get_transaction_lock().try_read_arc()?;

        let subkey_lock = record_lock.get_subkey_lock(subkey);
        let subkey_lock_guard = subkey_lock.try_lock_arc()?;
        let opt_background =
            record_lock.register_background(RecordLockRequest::Subkey(subkey, purpose.clone()));
        if opt_background.as_ref().is_some_and(BackgroundHold::refused) {
            return None;
        }
        record_lock.set_subkey_purpose(subkey, purpose);

        Some(SubkeyLockGuard::new(
            record_lock,
            lifetime_guard,
            transaction_guard,
            subkey_lock_guard,
            subkey_lock,
            subkey,
            opt_background,
        ))
    }

    /// Peeks carry no purpose, so the caller gives the priority
    pub async fn peek_lock(
        &self,
        record: OpaqueRecordKey,
        priority: RecordLockPriority,
    ) -> PeekLockGuard<R, S> {
        let recorder = DurationRecorder::new("RecordLockTable::peek_lock", |_name, _start| {
            #[cfg(feature = "debug-locks")]
            veilid_log!(self debug "{}[start={:#}](record: {:?}, priority: {:?})", _name, _start, record, priority);
        });
        recorder
            .record_fut(
                async {
                    let record_lock = self.get_or_create_record_lock(&record);
                    let request = RecordLockRequest::Peek(priority);
                    let _opt_foreground_wait = record_lock.preempt_background(&request);
                    let lifetime_guard = acquire_lifetime_read(&record_lock, "peek_lock").await;
                    record_lock.add_peek();
                    let opt_background = record_lock.register_background(request);
                    PeekLockGuard::new(record_lock, lifetime_guard, opt_background)
                },
                |_name, _start, _dur, ret| {
                    #[cfg(feature = "debug-locks")]
                    veilid_log!(self debug "{}[start={:#} dur={:#}]() done", _name, _start, _dur);
                    ret
                },
            )
            .await
    }

    #[allow(dead_code)]
    pub fn try_peek_lock(&self, record: OpaqueRecordKey) -> Option<PeekLockGuard<R, S>> {
        let record_lock = self.get_or_create_record_lock(&record);
        let lifetime_guard = record_lock.get_lifetime_lock().try_read_arc()?;
        record_lock.add_peek();
        Some(PeekLockGuard::new(record_lock, lifetime_guard, None))
    }

    pub fn get_record_lock_purpose_state(
        &self,
        record: &OpaqueRecordKey,
    ) -> RecordLockPurposeState<R, S> {
        // Get record lock
        let record_lock = {
            let mut inner = self.inner.lock();
            inner.record_lock_table.remove_expired();
            let Some(record_lock) = inner.record_lock_table.get(record) else {
                return RecordLockPurposeState::default();
            };
            record_lock
        };

        // Get a snapshot of the lock purpose state
        record_lock.purpose_state()
    }
}

async fn acquire_lifetime_write<R: RecordLockPurpose, S: RecordLockPurpose>(
    record_lock: &RecordLock<R, S>,
    site: &'static str,
) -> AsyncRwLockWriteGuardArc<()> {
    let recorder = DurationRecorder::new(
        "RecordLockTable::acquire_lifetime_write",
        |_name, _start| {
            #[cfg(feature = "debug-locks")]
            veilid_log!(record_lock debug "{}[start={:#}](site: {})", _name, _start, site);
        },
    );
    recorder.record_fut(
        async {
            cfg_if! {
                if #[cfg(feature = "debug-locks")] {
                    match timeout(30000, record_lock.get_lifetime_lock().write_arc()).await {
                        Ok(v) => v,
                        Err(_) => {
                            veilid_log!(record_lock error "active guards: {:#?}", record_lock.get_active_guards().lock().values().collect::<Vec<_>>());
                            panic!("{} lifetime_lock deadlock", site);
                        }
                    }
                } else {
                    record_lock
                        .get_lifetime_lock()
                        .write_arc()
                        .measure_debug(TimestampDuration::new_secs(10), move |msg| {
                            veilid_log!(record_lock debug "RecordLockTable::{} lifetime_lock write_arc: {}", site, msg);
                        })
                        .await
                }
            }
        },
        |_name, _start, _dur, ret| {
            #[cfg(feature = "debug-locks")]
            veilid_log!(record_lock debug "{}[start={:#} dur={:#}]() done", _name, _start, _dur);
            ret
        },
    ).await
}

async fn acquire_lifetime_read<R: RecordLockPurpose, S: RecordLockPurpose>(
    record_lock: &RecordLock<R, S>,
    site: &'static str,
) -> AsyncRwLockReadGuardArc<()> {
    let recorder =
        DurationRecorder::new("RecordLockTable::acquire_lifetime_read", |_name, _start| {
            #[cfg(feature = "debug-locks")]
            veilid_log!(record_lock debug "{}[start={:#}](site: {})", _name, _start, site);
        });
    recorder.record_fut(
        async {
            cfg_if! {
                if #[cfg(feature = "debug-locks")] {
                    match timeout(30000, record_lock.get_lifetime_lock().read_arc()).await {
                        Ok(v) => v,
                        Err(_) => {
                            veilid_log!(record_lock error "active guards: {:#?}", record_lock.get_active_guards().lock().values().collect::<Vec<_>>());
                            panic!("{} lifetime_lock read deadlock", site);
                        }
                    }
                } else {
                    record_lock
                        .get_lifetime_lock()
                        .read_arc()
                        .measure_debug(TimestampDuration::new_secs(10), move |msg| {
                            veilid_log!(record_lock debug "RecordLockTable::{} lifetime_lock read_arc: {}", site, msg);
                        })
                        .await
                }
            }
        },
        |_name, _start, _dur, ret| {
            #[cfg(feature = "debug-locks")]
            veilid_log!(record_lock debug "{}[start={:#} dur={:#}]() done", _name, _start, _dur);
            ret
        },
    ).await
}

async fn acquire_transaction_write<R: RecordLockPurpose, S: RecordLockPurpose>(
    record_lock: &RecordLock<R, S>,
    site: &'static str,
) -> AsyncRwLockWriteGuardArc<()> {
    let recorder = DurationRecorder::new(
        "RecordLockTable::acquire_transaction_write",
        |_name, _start| {
            #[cfg(feature = "debug-locks")]
            veilid_log!(record_lock debug "{}[start={:#}](site: {})", _name, _start, site);
        },
    );
    recorder
        .record_fut(
            async {
                record_lock
                    .get_transaction_lock()
                    .write_arc()
                    .measure_debug(TimestampDuration::new_secs(10), move |msg| {
                        veilid_log!(record_lock debug "RecordLockTable::{} transaction_lock write_arc: {}", site, msg);
                    })
                    .await
            },
            |_name, _start, _dur, ret| {
                #[cfg(feature = "debug-locks")]
                veilid_log!(record_lock debug "{}[start={:#} dur={:#}]() done", _name, _start, _dur);
                ret
            },
        )
        .await
}

async fn acquire_transaction_read<R: RecordLockPurpose, S: RecordLockPurpose>(
    record_lock: &RecordLock<R, S>,
    site: &'static str,
) -> AsyncRwLockReadGuardArc<()> {
    let recorder = DurationRecorder::new(
        "RecordLockTable::acquire_transaction_read",
        |_name, _start| {
            #[cfg(feature = "debug-locks")]
            veilid_log!(record_lock debug "{}[start={:#}](site: {})", _name, _start, site);
        },
    );
    recorder
        .record_fut(
            async {
                record_lock
                    .get_transaction_lock()
                    .read_arc()
                    .measure_debug(TimestampDuration::new_secs(10), move |msg| {
                        veilid_log!(record_lock debug "RecordLockTable::{} transaction_lock read_arc: {}", site, msg);
                    })
                    .await
            },
            |_name, _start, _dur, ret| {
                #[cfg(feature = "debug-locks")]
                veilid_log!(record_lock debug "{}[start={:#} dur={:#}]() done", _name, _start, _dur);
                ret
            },
        )
        .await
}

async fn acquire_watch<R: RecordLockPurpose, S: RecordLockPurpose>(
    record_lock: &RecordLock<R, S>,
    site: &'static str,
) -> AsyncMutexGuardArc<()> {
    let recorder = DurationRecorder::new("RecordLockTable::acquire_watch", |_name, _start| {
        #[cfg(feature = "debug-locks")]
        veilid_log!(record_lock debug "{}[start={:#}](site: {})", _name, _start, site);
    });
    recorder
        .record_fut(
            async {
                record_lock
                    .get_watch_lock()
                    .lock_arc()
                    .measure_debug(TimestampDuration::new_secs(10), move |msg| {
                        veilid_log!(record_lock debug "RecordLockTable::{} watch_lock lock_arc: {}", site, msg);
                    })
                    .await
            },
            |_name, _start, _dur, ret| {
                #[cfg(feature = "debug-locks")]
                veilid_log!(record_lock debug "{}[start={:#} dur={:#}]() done", _name, _start, _dur);
                ret
            },
        )
        .await
}

async fn acquire_record_lock_guards<R: RecordLockPurpose, S: RecordLockPurpose>(
    record_lock: &RecordLock<R, S>,
    mode: RecordLockMode,
) -> RecordLockGuards {
    match mode {
        RecordLockMode::Lifetime => RecordLockGuards::Lifetime {
            _lifetime: acquire_lifetime_write(record_lock, "lock_record").await,
        },
        RecordLockMode::Watch => {
            let lifetime = acquire_lifetime_read(record_lock, "lock_record").await;
            let watch = acquire_watch(record_lock, "lock_record").await;
            RecordLockGuards::Watch {
                _lifetime: lifetime,
                _watch: watch,
            }
        }
        RecordLockMode::Transaction => {
            let lifetime = acquire_lifetime_read(record_lock, "lock_record").await;
            let transaction = acquire_transaction_write(record_lock, "lock_record").await;
            RecordLockGuards::Transaction {
                _lifetime: lifetime,
                _transaction: transaction,
            }
        }
    }
}

fn try_acquire_record_lock_guards<R: RecordLockPurpose, S: RecordLockPurpose>(
    record_lock: &RecordLock<R, S>,
    mode: RecordLockMode,
) -> Option<RecordLockGuards> {
    match mode {
        RecordLockMode::Lifetime => Some(RecordLockGuards::Lifetime {
            _lifetime: record_lock.get_lifetime_lock().try_write_arc()?,
        }),
        RecordLockMode::Watch => {
            let lifetime = record_lock.get_lifetime_lock().try_read_arc()?;
            let watch = record_lock.get_watch_lock().try_lock_arc()?;
            Some(RecordLockGuards::Watch {
                _lifetime: lifetime,
                _watch: watch,
            })
        }
        RecordLockMode::Transaction => {
            let lifetime = record_lock.get_lifetime_lock().try_read_arc()?;
            let transaction = record_lock.get_transaction_lock().try_write_arc()?;
            Some(RecordLockGuards::Transaction {
                _lifetime: lifetime,
                _transaction: transaction,
            })
        }
    }
}
