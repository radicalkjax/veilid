//! An inspect nobody answered, on an unattached node the DHT gate is told is online, so the
//! fanout runs with no node to ask.

use super::*;
use crate::tests::*;

/// Every node answers an inspect, holding the record or not, so a fanout that reached no
/// one is our network: it fails rather than reading as nothing newer on the network.
async fn test_unanswered_inspect_is_try_again() {
    let (update_callback, config) = fixture_veilid_core();
    let api = api_startup(update_callback, config)
        .await
        .expect("startup failed");
    let storage_manager = api.core_context().unwrap().registry().storage_manager();

    let rec = storage_manager
        .create_record(
            CRYPTO_KIND_VLD0,
            DHTSchema::dflt(1).unwrap(),
            None,
            SafetySelection::Unsafe(Sequencing::PreferOrdered),
        )
        .await
        .unwrap();
    let key = rec.key().clone();
    storage_manager
        .set_value(key.clone(), 0, b"a".to_vec(), None)
        .await
        .unwrap();

    storage_manager.is_online.store(true, Ordering::Release);
    let res = storage_manager
        .inspect_record(key, ValueSubkeyRangeSet::single(0), DHTReportScope::SyncGet)
        .await;
    assert!(
        matches!(res, Err(VeilidAPIError::TryAgain { .. })),
        "an unanswered inspect must not be a report: {res:?}"
    );

    api.shutdown().await;
}

pub async fn test_all() {
    test_unanswered_inspect_is_try_again().await;
}
