//! A subkey lock excludes the next holder of the same subkey for as long as its guard lives. The
//! table keeps its subkey mutexes weakly, so a guard that lets the table's entry go hands the next
//! caller a fresh, unlocked mutex.

use super::*;
use crate::tests::*;
use core::pin::pin;
use futures_util::poll;

/// The second holder of subkey 0 waits for the first, and subkey 1 stays free
async fn test_subkey_lock_excludes_same_subkey() {
    let registry = mock_registry::init("subkey_lock").await;
    {
        let table = StorageManagerRecordLockTable::new(registry.clone());
        let record = OpaqueRecordKey::new(CRYPTO_KIND_VLD0, BareOpaqueRecordKey::new(&[3; 32]));

        let held = table
            .lock_subkey(
                record.clone(),
                0,
                StorageManagerSubkeyLockPurpose::TransactGet,
            )
            .await;
        let mut same = pin!(table.lock_subkey(
            record.clone(),
            0,
            StorageManagerSubkeyLockPurpose::TransactGet
        ));
        assert!(poll!(same.as_mut()).is_pending(), "subkey 0 is held");
        let other = table
            .try_lock_subkey(
                record.clone(),
                1,
                StorageManagerSubkeyLockPurpose::TransactGet,
            )
            .expect("subkey 1 is free");
        drop(other);
        drop(held);
        drop(same.await);
    }
    mock_registry::terminate(registry).await;
}

pub async fn test_all() {
    test_subkey_lock_excludes_same_subkey().await;
}
