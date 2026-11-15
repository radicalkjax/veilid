//! Which background lock holds a foreground acquisition stops. An API call on a record must not
//! wait out a background operation's network round: the background holder stops at its next
//! network await when an API call needs a lock it conflicts with, as the compatibility table in
//! `record_lock_table` judges conflicts.

use super::*;
use crate::tests::*;
use core::pin::pin;
use futures_util::future::{join, join4, pending};
use futures_util::poll;

fn make_record_key(n: u8) -> OpaqueRecordKey {
    OpaqueRecordKey::new(CRYPTO_KIND_VLD0, BareOpaqueRecordKey::new(&[n; 32]))
}

/// Whether a holder has been told to stop: `until_preempted` of a round that never ends returns
async fn is_preempted(fut: impl Future<Output = Option<()>>) -> bool {
    let mut fut = pin!(fut);
    poll!(fut.as_mut()).is_ready()
}

/// The holds conflict as the compatibility table says, both ways round
fn test_hold_conflicts() {
    use RecordLockHold::*;
    let holds = [Lifetime, Watch, Transaction, Subkey(0), Peek];
    // In `holds` order; Subkey against Subkey is the same subkey
    let table = [
        [true, true, true, true, true],
        [true, true, false, false, false],
        [true, false, true, true, false],
        [true, false, true, true, false],
        [true, false, false, false, false],
    ];
    for (i, a) in holds.iter().enumerate() {
        for (j, b) in holds.iter().enumerate() {
            assert_eq!(a.conflicts_with(b), table[i][j], "{:?} against {:?}", a, b);
        }
    }
    assert!(!Subkey(0).conflicts_with(&Subkey(1)));
}

/// A foreground acquisition stops the background holds it conflicts with and no others, and
/// gets its locks once they let go. A holder that is not background is waited out, and so is
/// the flush of a subkey an app get wants.
async fn test_foreground_stops_only_conflicting_background_holds() {
    let registry = mock_registry::init("record_lock_preemption").await;
    {
        let table = StorageManagerRecordLockTable::new(registry.clone());
        let record = make_record_key(1);

        let watch_op = table
            .try_lock_record(
                record.clone(),
                StorageManagerRecordLockPurpose::BackgroundWatch,
            )
            .expect("the record is free");
        let inspect = table
            .peek_lock(record.clone(), RecordLockPriority::Background)
            .await;
        let fetch = table
            .lock_subkey(
                record.clone(),
                0,
                StorageManagerSubkeyLockPurpose::BackgroundGet,
            )
            .await;
        let flush = table
            .lock_subkey(
                record.clone(),
                1,
                StorageManagerSubkeyLockPurpose::BackgroundSet,
            )
            .await;

        // A get of subkey 1 waits the flush out instead of stopping it
        let mut get =
            pin!(table.lock_subkey(record.clone(), 1, StorageManagerSubkeyLockPurpose::Get));
        assert!(poll!(get.as_mut()).is_pending(), "the flush holds subkey 1");
        assert!(!is_preempted(flush.until_preempted(pending())).await);
        drop(flush);
        drop(get.await);
        let flush = table
            .lock_subkey(
                record.clone(),
                1,
                StorageManagerSubkeyLockPurpose::BackgroundSet,
            )
            .await;

        // A set of subkey 1 stops the flush of subkey 1 and nothing else
        let mut set =
            pin!(table.lock_subkey(record.clone(), 1, StorageManagerSubkeyLockPurpose::Set));
        assert!(poll!(set.as_mut()).is_pending(), "the flush holds subkey 1");
        assert!(is_preempted(flush.until_preempted(pending())).await);
        assert!(!is_preempted(fetch.until_preempted(pending())).await);
        assert!(!is_preempted(watch_op.until_preempted(pending())).await);
        assert!(!is_preempted(inspect.until_preempted(pending())).await);
        drop(flush);
        drop(set.await);

        // A watch call stops the watch operation, not the inspection or the fetch
        let mut watch =
            pin!(table.lock_record(record.clone(), StorageManagerRecordLockPurpose::Watch));
        assert!(
            poll!(watch.as_mut()).is_pending(),
            "the watch operation holds the record"
        );
        assert!(is_preempted(watch_op.until_preempted(pending())).await);
        assert!(!is_preempted(inspect.until_preempted(pending())).await);
        assert!(!is_preempted(fetch.until_preempted(pending())).await);
        drop(watch_op);
        drop(watch.await);

        // A close stops everything left
        let mut close =
            pin!(table.lock_record(record.clone(), StorageManagerRecordLockPurpose::Close));
        assert!(poll!(close.as_mut()).is_pending());
        assert!(is_preempted(inspect.until_preempted(pending())).await);
        assert!(is_preempted(fetch.until_preempted(pending())).await);
        drop(inspect);
        drop(fetch);
        drop(close.await);

        let update = table
            .lock_record(record.clone(), StorageManagerRecordLockPurpose::WatchUpdate)
            .await;
        let mut watch =
            pin!(table.lock_record(record.clone(), StorageManagerRecordLockPurpose::Watch));
        assert!(poll!(watch.as_mut()).is_pending());
        assert!(!is_preempted(update.until_preempted(pending())).await);
        drop(update);
        drop(watch.await);
    }
    mock_registry::terminate(registry).await;
}

/// A background acquisition that gets its lock while a conflicting foreground acquisition waits
/// starts stopped, so the lock passes straight on rather than waiting out another round
async fn test_background_behind_foreground_starts_stopped() {
    let registry = mock_registry::init("record_lock_preemption").await;
    {
        let table = StorageManagerRecordLockTable::new(registry.clone());
        let record = make_record_key(2);

        // Neither foreground nor background, so nothing stops it
        let holder = table
            .lock_subkey(
                record.clone(),
                0,
                StorageManagerSubkeyLockPurpose::TransactGet,
            )
            .await;
        // The flush queues for the subkey first, an API set behind it
        let mut flush = pin!(table.lock_subkey(
            record.clone(),
            0,
            StorageManagerSubkeyLockPurpose::BackgroundSet
        ));
        assert!(poll!(flush.as_mut()).is_pending());
        let mut set =
            pin!(table.lock_subkey(record.clone(), 0, StorageManagerSubkeyLockPurpose::Set));
        assert!(poll!(set.as_mut()).is_pending());

        drop(holder);
        let flush = flush.await;
        assert!(
            is_preempted(flush.until_preempted(pending())).await,
            "the flush got the subkey while a set waited for it"
        );
        assert!(poll!(set.as_mut()).is_pending());
        drop(flush);
        drop(set.await);

        // Nor does a get waiting behind it stop it
        let holder = table
            .lock_subkey(
                record.clone(),
                0,
                StorageManagerSubkeyLockPurpose::TransactGet,
            )
            .await;
        let mut flush = pin!(table.lock_subkey(
            record.clone(),
            0,
            StorageManagerSubkeyLockPurpose::BackgroundSet
        ));
        assert!(poll!(flush.as_mut()).is_pending());
        let mut get =
            pin!(table.lock_subkey(record.clone(), 0, StorageManagerSubkeyLockPurpose::Get));
        assert!(poll!(get.as_mut()).is_pending());
        drop(holder);
        let flush = flush.await;
        assert!(
            !is_preempted(flush.until_preempted(pending())).await,
            "a waiting get leaves the flush running"
        );
        assert!(poll!(get.as_mut()).is_pending());
        drop(flush);
        drop(get.await);

        // With no foreground acquisition waiting, it runs
        let flush = table
            .lock_subkey(
                record.clone(),
                0,
                StorageManagerSubkeyLockPurpose::BackgroundSet,
            )
            .await;
        assert!(!is_preempted(flush.until_preempted(pending())).await);
    }
    mock_registry::terminate(registry).await;
}

/// The app's cancel_dht_watch and close_dht_record on a record whose background operations are
/// mid-fanout stop those operations instead of waiting them out
async fn test_api_does_not_wait_for_background_fanouts() {
    let (update_callback, config) = fixture_veilid_core();
    let api = api_startup(update_callback, config)
        .await
        .expect("startup failed");
    let rc = api.routing_context().unwrap();
    let registry = api.core_context().unwrap().registry();
    // Every background round below would run forever, so finishing at all is the result
    let bound_ms = registry.config().internal().network.rpc.timeout_ms;

    let rec = rc
        .create_dht_record(CRYPTO_KIND_VLD0, DHTSchema::dflt(2).unwrap(), None)
        .await
        .unwrap();
    let key = rec.key().clone();
    let opaque_record_key = key.opaque();
    assert!(rc
        .watch_dht_values(key.clone(), None, None, None)
        .await
        .unwrap());

    {
        let storage_manager = registry.storage_manager();
        let table = &storage_manager.record_lock_table;

        // Each holds its lock across a network round that never ends on its own
        let watch_op = table
            .lock_record(
                opaque_record_key.clone(),
                StorageManagerRecordLockPurpose::BackgroundWatch,
            )
            .await;
        let inspect = table
            .peek_lock(opaque_record_key.clone(), RecordLockPriority::Background)
            .await;
        let fetch = table
            .lock_subkey(
                opaque_record_key.clone(),
                0,
                StorageManagerSubkeyLockPurpose::BackgroundGet,
            )
            .await;
        let flush = table
            .lock_subkey(
                opaque_record_key.clone(),
                1,
                StorageManagerSubkeyLockPurpose::BackgroundSet,
            )
            .await;

        let (cancelled, watch_op_stopped) = timeout(
            bound_ms,
            join(rc.cancel_dht_watch(key.clone(), None), async move {
                let stopped = watch_op.until_preempted(pending::<()>()).await.is_none();
                drop(watch_op);
                stopped
            }),
        )
        .await
        .expect("cancel_dht_watch waited out a background fanout");
        assert!(!cancelled.unwrap(), "the whole watch is cancelled");
        assert!(watch_op_stopped);

        // A cancel conflicts with the watch operation alone
        assert!(!is_preempted(inspect.until_preempted(pending())).await);
        assert!(!is_preempted(fetch.until_preempted(pending())).await);
        assert!(!is_preempted(flush.until_preempted(pending())).await);

        let (closed, inspect_stopped, fetch_stopped, flush_stopped) = timeout(
            bound_ms,
            join4(
                rc.close_dht_record(key.clone()),
                async move {
                    let stopped = inspect.until_preempted(pending::<()>()).await.is_none();
                    drop(inspect);
                    stopped
                },
                async move {
                    let stopped = fetch.until_preempted(pending::<()>()).await.is_none();
                    drop(fetch);
                    stopped
                },
                async move {
                    let stopped = flush.until_preempted(pending::<()>()).await.is_none();
                    drop(flush);
                    stopped
                },
            ),
        )
        .await
        .expect("close_dht_record waited out a background fanout");
        closed.unwrap();
        assert!(inspect_stopped && fetch_stopped && flush_stopped);
    }

    rc.delete_dht_record(key).await.unwrap();
    api.shutdown().await;
}

pub async fn test_all() {
    test_hold_conflicts();
    test_foreground_stops_only_conflicting_background_holds().await;
    test_background_behind_foreground_starts_stopped().await;
    test_api_does_not_wait_for_background_fanouts().await;
}
