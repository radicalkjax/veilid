//! The metadata saves periodically, and in the background as soon as it changes.

use super::*;
#[cfg(feature = "app-lifecycle")]
use crate::tests::*;

// `secs` seconds past a base far enough from zero that nothing underflows; a new schedule counts
// as saved at its creation
fn ts(secs: u64) -> Timestamp {
    Timestamp::new(1_000_000_000u64 + secs * 1_000_000u64)
}

const PERIOD: TimestampDuration = TimestampDuration::new_secs(30);

/// In the foreground a change waits for the periodic save.
fn test_foreground_change_waits_for_period() {
    let mut schedule = MetadataSaveSchedule::new(ts(0), PERIOD);
    schedule.changed();
    assert_eq!(schedule.due(ts(29)), None);
    assert_eq!(schedule.due(ts(30)), Some(MetadataSaveReason::Periodic));
}

/// The periodic save runs whether or not anything changed.
fn test_periodic_without_changes() {
    let mut schedule = MetadataSaveSchedule::new(ts(0), PERIOD);
    assert_eq!(schedule.due(ts(30)), Some(MetadataSaveReason::Periodic));
    schedule.saving(ts(30));
    assert_eq!(schedule.due(ts(59)), None);
    assert_eq!(schedule.due(ts(60)), Some(MetadataSaveReason::Periodic));
}

/// In the background a change is due at once, and a save covers it.
#[cfg(feature = "app-lifecycle")]
fn test_background_change_saves_at_once() {
    let mut schedule = MetadataSaveSchedule::new(ts(0), PERIOD);
    schedule.set_background(true);
    assert_eq!(schedule.due(ts(1)), None, "nothing changed");
    schedule.changed();
    assert_eq!(schedule.due(ts(1)), Some(MetadataSaveReason::Background));
    schedule.saving(ts(1));
    assert_eq!(schedule.due(ts(2)), None);
}

/// Back in the foreground, a pending change waits for the periodic save again.
#[cfg(feature = "app-lifecycle")]
fn test_foreground_return_defers_to_period() {
    let mut schedule = MetadataSaveSchedule::new(ts(0), PERIOD);
    schedule.set_background(true);
    schedule.changed();
    schedule.set_background(false);
    assert_eq!(schedule.due(ts(1)), None);
    assert_eq!(schedule.due(ts(30)), Some(MetadataSaveReason::Periodic));
}

/// In the background an offline write reaches disk at the next check, not the periodic save.
#[cfg(feature = "app-lifecycle")]
async fn test_background_offline_write_reaches_disk() {
    let (update_callback, config) = fixture_veilid_core();
    let api = api_startup(update_callback, config)
        .await
        .expect("startup failed");
    let registry = api.core_context().unwrap().registry();
    let storage_manager = registry.storage_manager();
    let metadata_db = storage_manager
        .inner
        .lock()
        .metadata_db
        .clone()
        .expect("metadata db should exist");
    let table_store = registry.table_store();
    let saved_writes = || async {
        metadata_db
            .load_serialized(0, OFFLINE_SUBKEY_WRITES)
            .await
            .expect("load failed")
            .map(|serialized| {
                table_store
                    .try_deserialize::<LinkedHashMap<OpaqueRecordKey, OfflineSubkeyWrite>>(
                        &serialized,
                    )
                    .expect("deserialize failed")
            })
            .unwrap_or_default()
    };

    let key = OpaqueRecordKey::new(CRYPTO_KIND_VLD0, BareOpaqueRecordKey::new(&[3u8; 32]));
    storage_manager.save_metadata().await.expect("save failed");
    assert!(!saved_writes().await.contains_key(&key));

    storage_manager
        .metadata_save_schedule
        .lock()
        .set_background(true);
    storage_manager.add_offline_subkey_write(
        key.clone(),
        0,
        SafetySelection::Unsafe(Sequencing::PreferOrdered),
    );
    storage_manager
        .save_metadata_if_due(Timestamp::now())
        .await
        .expect("save check failed");
    assert!(
        saved_writes().await.contains_key(&key),
        "the background write should be on disk"
    );

    api.shutdown().await;
}

/// In the background the per-tick watch check changes nothing a save must carry, so it leaves
/// the metadata clean rather than saving it every second.
#[cfg(feature = "app-lifecycle")]
async fn test_background_watch_check_leaves_metadata_clean() {
    let (update_callback, config) = fixture_veilid_core();
    let api = api_startup(update_callback, config)
        .await
        .expect("startup failed");
    let registry = api.core_context().unwrap().registry();
    let storage_manager = registry.storage_manager();

    storage_manager.save_metadata().await.expect("save failed");
    storage_manager
        .metadata_save_schedule
        .lock()
        .set_background(true);
    storage_manager
        .check_outbound_watches_task_routine(
            StopSource::new().token(),
            Timestamp::now(),
            Timestamp::now(),
        )
        .expect("check failed");
    assert_eq!(
        storage_manager
            .metadata_save_schedule
            .lock()
            .due(Timestamp::now()),
        None
    );

    api.shutdown().await;
}

#[cfg_attr(not(feature = "app-lifecycle"), allow(clippy::unused_async))]
pub async fn test_all() {
    test_foreground_change_waits_for_period();
    test_periodic_without_changes();
    #[cfg(feature = "app-lifecycle")]
    test_background_change_saves_at_once();
    #[cfg(feature = "app-lifecycle")]
    test_foreground_return_defers_to_period();
    #[cfg(feature = "app-lifecycle")]
    test_background_offline_write_reaches_disk().await;
    #[cfg(feature = "app-lifecycle")]
    test_background_watch_check_leaves_metadata_clean().await;
}
