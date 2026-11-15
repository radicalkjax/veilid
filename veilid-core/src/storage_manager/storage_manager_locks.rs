use super::*;

#[derive(Debug, Clone, Eq, PartialEq, Ord, PartialOrd, Hash)]
pub enum StorageManagerRecordLockPurpose {
    /// API create_dht_record
    Create,
    /// API open_dht_record
    Open,
    /// API close_dht_record, and closing every record
    Close,
    /// API delete_dht_record, and debug purges
    Delete,
    /// API watch_dht_values and cancel_dht_watch
    Watch,
    /// Watch bookkeeping and value change reports, with no network wait
    WatchUpdate,
    /// The watch tick's dead, cancel, renew and reconcile operations
    BackgroundWatch,
    /// Transaction begin
    TransactBegin,
    /// Transaction extend
    TransactExtend,
    /// Transaction end and commit
    TransactEndAndCommit,
    /// Transaction rollback
    TransactRollback,
    /// Transaction drop, including expired ones
    TransactDrop,
}

impl RecordLockPurpose for StorageManagerRecordLockPurpose {
    fn record_lock_mode(&self) -> RecordLockMode {
        match self {
            Self::Create | Self::Open | Self::Close | Self::Delete => RecordLockMode::Lifetime,
            Self::Watch | Self::WatchUpdate | Self::BackgroundWatch => RecordLockMode::Watch,
            Self::TransactBegin
            | Self::TransactExtend
            | Self::TransactEndAndCommit
            | Self::TransactRollback
            | Self::TransactDrop => RecordLockMode::Transaction,
        }
    }

    fn record_lock_priority(&self) -> RecordLockPriority {
        match self {
            Self::Create | Self::Open | Self::Close | Self::Delete | Self::Watch => {
                RecordLockPriority::Foreground
            }
            Self::BackgroundWatch => RecordLockPriority::Background,
            // Background transactions give way between operations: preempt_background_transactions_locked
            Self::WatchUpdate
            | Self::TransactBegin
            | Self::TransactExtend
            | Self::TransactEndAndCommit
            | Self::TransactRollback
            | Self::TransactDrop => RecordLockPriority::Normal,
        }
    }
}

#[derive(Debug, Clone, Eq, PartialEq, Ord, PartialOrd, Hash)]
pub enum StorageManagerSubkeyLockPurpose {
    /// API get_dht_value
    Get,
    /// API set_dht_value
    Set,
    /// The watch change inspection's fetch of a changed subkey
    BackgroundGet,
    /// The offline subkey write flush
    BackgroundSet,
    /// Transaction get
    TransactGet,
    /// Transaction set
    TransactSet,
}

impl RecordLockPurpose for StorageManagerSubkeyLockPurpose {
    fn record_lock_mode(&self) -> RecordLockMode {
        RecordLockMode::Lifetime
    }

    fn record_lock_priority(&self) -> RecordLockPriority {
        match self {
            Self::Get | Self::Set => RecordLockPriority::Foreground,
            Self::BackgroundGet | Self::BackgroundSet => RecordLockPriority::Background,
            Self::TransactGet | Self::TransactSet => RecordLockPriority::Normal,
        }
    }

    // A get cannot replace a pending write's local copy, so stopping the flush only starves it
    fn preempts(&self, background: &Self) -> bool {
        !matches!((self, background), (Self::Get, Self::BackgroundSet))
    }
}

pub type StorageManagerRecordLockTable =
    RecordLockTable<StorageManagerRecordLockPurpose, StorageManagerSubkeyLockPurpose>;

pub type StorageManagerRecordLockGuard =
    RecordLockGuard<StorageManagerRecordLockPurpose, StorageManagerSubkeyLockPurpose>;
pub type StorageManagerRecordsLockGuard =
    RecordsLockGuard<StorageManagerRecordLockPurpose, StorageManagerSubkeyLockPurpose>;
pub type StorageManagerSubkeyLockGuard =
    SubkeyLockGuard<StorageManagerRecordLockPurpose, StorageManagerSubkeyLockPurpose>;
#[expect(dead_code)]
pub type StorageManagerPeekLockGuard =
    PeekLockGuard<StorageManagerRecordLockPurpose, StorageManagerSubkeyLockPurpose>;
pub type StorageManagerPeeksLockGuard =
    PeeksLockGuard<StorageManagerRecordLockPurpose, StorageManagerSubkeyLockPurpose>;
