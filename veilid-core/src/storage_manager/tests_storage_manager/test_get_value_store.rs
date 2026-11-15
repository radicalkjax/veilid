//! What a get stores of the value its fanout settled on: a newer value always replaces the local
//! copy, a different one at the same sequence number only on a forced refresh with no write of ours
//! waiting to reach the network, and an older one never does.

use super::*;
use crate::tests::*;
use test_value_split::make_value;

/// A sequence number with successors left
const SEQ: u32 = 7;

/// The local copy and pending write a case starts from, and the value its get settled on
struct StoreCase {
    /// The local copy before the get
    local: Option<Arc<SignedValueData>>,
    /// Whether a write of ours to the subkey is still waiting to go out
    pending_write: bool,
    /// How the get treats the local copy
    refresh: GetValueRefresh,
    /// The value the get's fanout settled on
    settled: Arc<SignedValueData>,
}

/// Run `case` through the store rule on `subkey`, returning what the get returned and what is stored
async fn run_case(
    storage_manager: &StorageManager,
    record_key: &RecordKey,
    subkey: ValueSubkey,
    case: StoreCase,
) -> (EncryptedValueData, EncryptedValueData) {
    let opaque_record_key = record_key.opaque();
    let subkey_lock = storage_manager
        .record_lock_table
        .lock_subkey(
            opaque_record_key.clone(),
            subkey,
            StorageManagerSubkeyLockPurpose::Get,
        )
        .await;
    if let Some(local) = case.local {
        storage_manager
            .handle_set_single_local_value_with_subkey_lock(&subkey_lock, local)
            .await
            .expect("local set should succeed");
    }
    if case.pending_write {
        storage_manager.add_offline_subkey_write(
            opaque_record_key.clone(),
            subkey,
            SafetySelection::Unsafe(Sequencing::PreferUnordered),
        );
    }

    let result = get_value::OutboundGetValueResult {
        fanout_result: FanoutResult {
            kind: FanoutResultKind::Consensus,
            consensus_nodes: vec![],
            value_nodes: vec![],
        },
        get_result: GetResult {
            opt_value: Some(case.settled),
            opt_descriptor: None,
        },
    };
    let returned = storage_manager
        .process_outbound_get_value_result_locked(&subkey_lock, case.refresh, result)
        .await
        .expect("get result should process")
        .expect("get should return a value");
    let stored = storage_manager
        .handle_get_single_local_value(&opaque_record_key, subkey, false)
        .await
        .expect("local get should succeed")
        .opt_value
        .expect("subkey should be stored")
        .value_data()
        .clone();

    if case.pending_write {
        let mut inner = storage_manager.inner.lock();
        storage_manager.remove_offline_subkey_write_inner(&mut inner, &opaque_record_key, subkey);
    }
    (returned, stored)
}

pub async fn test_get_result_store_rule() {
    let (update_callback, config) = fixture_veilid_core();
    let api = api_startup(update_callback, config)
        .await
        .expect("startup failed");
    let rc = api.routing_context().unwrap();
    let registry = api.core_context().unwrap().registry();

    let rec = rc
        .create_dht_record(CRYPTO_KIND_VLD0, DHTSchema::dflt(8).unwrap(), None)
        .await
        .unwrap();
    let key = rec.key().clone();

    {
        let storage_manager = registry.storage_manager();
        let a = make_value(b"A", SEQ.into());
        let b = make_value(b"B", SEQ.into());
        let d = make_value(b"D", (SEQ + 1).into());
        let same_seq = |refresh, pending_write| StoreCase {
            local: Some(a.clone()),
            pending_write,
            refresh,
            settled: b.clone(),
        };

        // A different value at the same seq leaves the local copy alone unless the app forced it
        for (subkey, refresh) in [(0, GetValueRefresh::Local), (1, GetValueRefresh::Newer)] {
            let (returned, stored) =
                run_case(&storage_manager, &key, subkey, same_seq(refresh, false)).await;
            assert_eq!(
                &returned,
                a.value_data(),
                "{:?} returns the local copy",
                refresh
            );
            assert_eq!(
                &stored,
                a.value_data(),
                "{:?} keeps the local copy",
                refresh
            );
        }
        let (returned, stored) = run_case(
            &storage_manager,
            &key,
            2,
            same_seq(GetValueRefresh::Force, false),
        )
        .await;
        assert_eq!(&returned, b.value_data());
        assert_eq!(&stored, b.value_data());

        // Not even a forced refresh replaces a write of ours that has not gone out yet
        let (returned, stored) = run_case(
            &storage_manager,
            &key,
            3,
            same_seq(GetValueRefresh::Force, true),
        )
        .await;
        assert_eq!(&returned, a.value_data());
        assert_eq!(&stored, a.value_data());

        // A newer value replaces the local copy in every mode
        for (subkey, refresh) in [
            (4, GetValueRefresh::Local),
            (5, GetValueRefresh::Newer),
            (6, GetValueRefresh::Force),
        ] {
            let case = StoreCase {
                local: Some(a.clone()),
                pending_write: false,
                refresh,
                settled: d.clone(),
            };
            let (returned, stored) = run_case(&storage_manager, &key, subkey, case).await;
            assert_eq!(
                &returned,
                d.value_data(),
                "{:?} returns the newer value",
                refresh
            );
            assert_eq!(
                &stored,
                d.value_data(),
                "{:?} stores the newer value",
                refresh
            );
        }

        // An older value never replaces the local copy, and an empty subkey takes what came back
        let case = StoreCase {
            local: Some(d.clone()),
            pending_write: false,
            refresh: GetValueRefresh::Force,
            settled: a.clone(),
        };
        let (returned, stored) = run_case(&storage_manager, &key, 7, case).await;
        assert_eq!(&returned, d.value_data());
        assert_eq!(&stored, d.value_data());

        let case = StoreCase {
            local: None,
            pending_write: false,
            refresh: GetValueRefresh::Local,
            settled: b.clone(),
        };
        let rec2 = rc
            .create_dht_record(CRYPTO_KIND_VLD0, DHTSchema::dflt(1).unwrap(), None)
            .await
            .unwrap();
        let key2 = rec2.key().clone();
        let (returned, stored) = run_case(&storage_manager, &key2, 0, case).await;
        assert_eq!(&returned, b.value_data());
        assert_eq!(&stored, b.value_data());
        rc.close_dht_record(key2.clone()).await.unwrap();
        rc.delete_dht_record(key2).await.unwrap();
    }

    rc.close_dht_record(key.clone()).await.unwrap();
    rc.delete_dht_record(key).await.unwrap();

    api.shutdown().await;
}

pub async fn test_all() {
    test_get_result_store_rule().await;
}
