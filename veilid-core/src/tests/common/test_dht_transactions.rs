use crate::tests_crypto::*;

use super::*;

// Mirror of WASM VeilidRoutingContext DHT transaction tests
// Simple test parameters
const SIMPLE_DHT_RECORD_COUNT: usize = 4;
const SIMPLE_DHT_SUBKEY_SIZE: usize = 16;
const SIMPLE_DHT_SUBKEY_COUNT: u32 = 2;

// Full records test parameters
const FULL_DHT_RECORD_COUNT: usize = 4;
const FULL_DHT_SUBKEY_SIZE: usize = 32768;
const FULL_DHT_SUBKEY_COUNT: u32 = 16;

fn make_test_data(record_count: usize, subkey_size: usize) -> Vec<Vec<u8>> {
    (0..record_count)
        .map(|rec| vec![rec as u8; subkey_size])
        .collect()
}

async fn create_dht_records(
    rc: &RoutingContext,
    record_count: usize,
    subkey_count: u32,
) -> Vec<RecordKey> {
    let mut keys = Vec::new();
    for _ in 0..record_count {
        let rec = rc
            .create_dht_record(
                TEST_CRYPTO_KIND,
                DHTSchema::dflt(subkey_count as u16).unwrap(),
                None,
            )
            .await
            .expect("should create DHT record");
        keys.push(rec.key());
    }
    keys
}

async fn delete_dht_records(rc: &RoutingContext, keys: &[RecordKey]) {
    for key in keys {
        rc.delete_dht_record(key.clone())
            .await
            .expect("should delete DHT record");
    }
}

async fn test_create_empty_transaction_and_drop(api: VeilidAPI) {
    let rc = api.routing_context().unwrap();
    let keys = create_dht_records(&rc, SIMPLE_DHT_RECORD_COUNT, SIMPLE_DHT_SUBKEY_COUNT).await;

    let tx = api
        .transact_dht_records(keys.clone(), None)
        .await
        .expect("should create transaction");
    drop(tx);

    delete_dht_records(&rc, &keys).await;
}

async fn test_create_empty_transaction_and_rollback(api: VeilidAPI) {
    let rc = api.routing_context().unwrap();
    let keys = create_dht_records(&rc, SIMPLE_DHT_RECORD_COUNT, SIMPLE_DHT_SUBKEY_COUNT).await;

    let tx = api
        .transact_dht_records(keys.clone(), None)
        .await
        .expect("should create transaction");
    tx.rollback().await.expect("should rollback");

    delete_dht_records(&rc, &keys).await;
}

async fn test_create_empty_transaction_and_commit(api: VeilidAPI) {
    let rc = api.routing_context().unwrap();
    let keys = create_dht_records(&rc, SIMPLE_DHT_RECORD_COUNT, SIMPLE_DHT_SUBKEY_COUNT).await;

    let tx = api
        .transact_dht_records(keys.clone(), None)
        .await
        .expect("should create transaction");
    tx.commit().await.expect("should commit");

    delete_dht_records(&rc, &keys).await;
}

async fn test_transaction_sets_and_rollback(api: VeilidAPI) {
    let rc = api.routing_context().unwrap();
    let keys = create_dht_records(&rc, SIMPLE_DHT_RECORD_COUNT, SIMPLE_DHT_SUBKEY_COUNT).await;
    let data = make_test_data(SIMPLE_DHT_RECORD_COUNT, SIMPLE_DHT_SUBKEY_SIZE);

    let tx = api
        .transact_dht_records(keys.clone(), None)
        .await
        .expect("should create transaction");

    for rec in 0..SIMPLE_DHT_RECORD_COUNT {
        let res = tx
            .set(keys[rec].clone(), 0, data[rec].clone(), None)
            .await
            .expect("set should succeed");
        assert!(res.is_none(), "set should return None");
    }

    tx.rollback().await.expect("should rollback");
    delete_dht_records(&rc, &keys).await;
}

async fn test_transaction_sets_and_commit(api: VeilidAPI) {
    let rc = api.routing_context().unwrap();
    let keys = create_dht_records(&rc, SIMPLE_DHT_RECORD_COUNT, SIMPLE_DHT_SUBKEY_COUNT).await;
    let data = make_test_data(SIMPLE_DHT_RECORD_COUNT, SIMPLE_DHT_SUBKEY_SIZE);

    let tx = api
        .transact_dht_records(keys.clone(), None)
        .await
        .expect("should create transaction");

    for rec in 0..SIMPLE_DHT_RECORD_COUNT {
        let res = tx
            .set(keys[rec].clone(), 0, data[rec].clone(), None)
            .await
            .expect("set should succeed");
        assert!(res.is_none(), "set should return None");
    }

    tx.commit().await.expect("should commit");
    delete_dht_records(&rc, &keys).await;
}

async fn test_transaction_sets_gets_and_commit(api: VeilidAPI) {
    let rc = api.routing_context().unwrap();
    let keys = create_dht_records(&rc, SIMPLE_DHT_RECORD_COUNT, SIMPLE_DHT_SUBKEY_COUNT).await;
    let data = make_test_data(SIMPLE_DHT_RECORD_COUNT, SIMPLE_DHT_SUBKEY_SIZE);

    let tx = api
        .transact_dht_records(keys.clone(), None)
        .await
        .expect("should create transaction");

    let mut set_futs = Vec::new();
    for rec in 0..SIMPLE_DHT_RECORD_COUNT {
        let tx = tx.clone();
        let key = keys[rec].clone();
        let d = data[rec].clone();
        set_futs.push(async move { tx.set(key, 0, d, None).await });
    }
    let set_results = futures_util::future::join_all(set_futs).await;
    for res in set_results {
        assert!(
            res.expect("set should succeed").is_none(),
            "set should return None"
        );
    }

    let mut get_futs = Vec::new();
    for rec in 0..SIMPLE_DHT_RECORD_COUNT {
        let tx = tx.clone();
        let key = keys[rec].clone();
        get_futs.push(async move { tx.get(key, 0).await });
    }
    let get_results = futures_util::future::join_all(get_futs).await;
    for res in get_results {
        assert!(
            res.expect("get should succeed").is_none(),
            "get should return None before commit"
        );
    }

    tx.commit().await.expect("should commit");
    delete_dht_records(&rc, &keys).await;
}

async fn test_transaction_repeated_set_seq(api: VeilidAPI) {
    let rc = api.routing_context().unwrap();
    let keys = create_dht_records(&rc, 1, 1).await;
    let key = keys[0].clone();
    rc.set_dht_value(key.clone(), 0, b"base".to_vec(), None)
        .await
        .unwrap();

    // Every set of a subkey counts from the base, so a repeat replaces the first at its seq
    let tx = api.transact_dht_records(keys.clone(), None).await.unwrap();
    for data in [b"first".as_slice(), b"second"] {
        let res = tx.set(key.clone(), 0, data.to_vec(), None).await.unwrap();
        assert!(res.is_none(), "set should return None");
    }
    tx.commit().await.expect("should commit");

    let value = rc.get_dht_value(key, 0, true).await.unwrap().unwrap();
    assert_eq!(value.seq(), 1.into());
    assert_eq!(value.data(), b"second");
    delete_dht_records(&rc, &keys).await;
}

async fn test_transaction_set_back_to_base(api: VeilidAPI) {
    let rc = api.routing_context().unwrap();
    let keys = create_dht_records(&rc, 1, 1).await;
    let key = keys[0].clone();
    rc.set_dht_value(key.clone(), 0, b"base".to_vec(), None)
        .await
        .unwrap();

    // Restoring the base resends it at its seq, which every node already holds
    let tx = api.transact_dht_records(keys.clone(), None).await.unwrap();
    let res = tx
        .set(key.clone(), 0, b"first".to_vec(), None)
        .await
        .unwrap();
    assert!(res.is_none(), "set should return None");
    let held = tx
        .set(key.clone(), 0, b"base".to_vec(), None)
        .await
        .unwrap()
        .expect("nodes should answer with the base");
    assert_eq!(held.seq(), 0.into());
    assert_eq!(held.data(), b"base");
    tx.commit().await.expect("should commit");

    let value = rc.get_dht_value(key, 0, true).await.unwrap().unwrap();
    assert_eq!(value.seq(), 0.into());
    assert_eq!(value.data(), b"base");
    delete_dht_records(&rc, &keys).await;
}

async fn test_transaction_set_below_network(api: VeilidAPI) {
    let rc = api.routing_context().unwrap();
    let rec = rc
        .create_dht_record(TEST_CRYPTO_KIND, DHTSchema::dflt(1).unwrap(), None)
        .await
        .unwrap();
    let key = rec.key();
    let owner = rec.owner_keypair().unwrap();
    for data in [b"zero", b"once"] {
        rc.set_dht_value(key.clone(), 0, data.to_vec(), None)
            .await
            .unwrap();
    }

    // Without a local copy the first set counts from nothing, below what the network holds
    api.debug("record purge local".to_owned()).await.unwrap();
    let _ = rc
        .open_dht_record(key.clone(), Some(owner), None)
        .await
        .unwrap();

    let tx = api
        .transact_dht_records(vec![key.clone()], None)
        .await
        .unwrap();
    let held = tx
        .set(key.clone(), 0, b"twice".to_vec(), None)
        .await
        .unwrap()
        .expect("nodes should answer with what they hold");
    assert_eq!(held.seq(), 1.into());
    assert_eq!(held.data(), b"once");
    let res = tx
        .set(key.clone(), 0, b"twice".to_vec(), None)
        .await
        .unwrap();
    assert!(res.is_none(), "set should count from what came back");
    tx.commit().await.expect("should commit");

    let value = rc
        .get_dht_value(key.clone(), 0, true)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(value.seq(), 2.into());
    assert_eq!(value.data(), b"twice");
    rc.close_dht_record(key.clone()).await.unwrap();
    rc.delete_dht_record(key).await.unwrap();
}

async fn test_transaction_failed_commit_releases(api: VeilidAPI) {
    let rc = api.routing_context().unwrap();
    let keys = create_dht_records(&rc, 1, 1).await;

    let tx = api.transact_dht_records(keys.clone(), None).await.unwrap();
    let res = tx
        .set(keys[0].clone(), 0, b"lost".to_vec(), None)
        .await
        .unwrap();
    assert!(res.is_none(), "set should return None");
    api.debug("fault arm commit generic --count 1".to_owned())
        .await
        .unwrap();
    assert!(tx.commit().await.is_err(), "commit should fail");

    // The failed commit ended the transaction, so its record is free and nothing landed
    let tx = api
        .transact_dht_records(keys.clone(), None)
        .await
        .expect("record should be free after a failed commit");
    tx.rollback().await.expect("should rollback");
    let value = rc.get_dht_value(keys[0].clone(), 0, true).await.unwrap();
    assert!(value.is_none(), "failed commit should write nothing");
    delete_dht_records(&rc, &keys).await;
}

async fn test_transaction_fail_non_transactional_sets(api: VeilidAPI) {
    let rc = api.routing_context().unwrap();
    let keys = create_dht_records(&rc, SIMPLE_DHT_RECORD_COUNT, SIMPLE_DHT_SUBKEY_COUNT).await;
    let data = make_test_data(SIMPLE_DHT_RECORD_COUNT, SIMPLE_DHT_SUBKEY_SIZE);

    let tx = api
        .transact_dht_records(keys.clone(), None)
        .await
        .expect("should create transaction");

    let mut set_futs = Vec::new();
    for subkey in 0..SIMPLE_DHT_SUBKEY_COUNT {
        for rec in 0..SIMPLE_DHT_RECORD_COUNT {
            let rc = rc.clone();
            let key = keys[rec].clone();
            let d = data[rec].clone();
            set_futs
                .push(async move { rc.set_dht_value(key, subkey as ValueSubkey, d, None).await });
        }
    }
    let set_results = futures_util::future::join_all(set_futs).await;
    let mut had_try_again = false;
    for res in set_results {
        if let Err(e) = res {
            match e {
                VeilidAPIError::TryAgain { .. } => {
                    had_try_again = true;
                }
                _ => panic!("expected TryAgain error, got: {}", e),
            }
        }
    }
    assert!(
        had_try_again,
        "should have gotten at least one TryAgain error"
    );

    tx.rollback().await.expect("should rollback");
    delete_dht_records(&rc, &keys).await;
}

async fn test_transaction_inspect_set_commit_then_get(api: VeilidAPI) {
    let rc = api.routing_context().unwrap();
    let keys = create_dht_records(&rc, SIMPLE_DHT_RECORD_COUNT, SIMPLE_DHT_SUBKEY_COUNT).await;
    let data = make_test_data(SIMPLE_DHT_RECORD_COUNT, SIMPLE_DHT_SUBKEY_SIZE);

    let tx = api
        .transact_dht_records(keys.clone(), None)
        .await
        .expect("should create transaction");

    let mut inspect_futs = Vec::new();
    for rec in 0..SIMPLE_DHT_RECORD_COUNT {
        let tx = tx.clone();
        let key = keys[rec].clone();
        inspect_futs.push(async move { tx.inspect(key, None, DHTReportScope::SyncGet).await });
    }
    let inspect_results = futures_util::future::join_all(inspect_futs).await;
    for report in inspect_results {
        let report = report.expect("inspect should succeed");
        assert_eq!(
            report.local_seqs(),
            &vec![ValueSeqNum::NONE; SIMPLE_DHT_SUBKEY_COUNT as usize],
            "local seqs should all be None"
        );
        assert_eq!(
            report.network_seqs(),
            &vec![ValueSeqNum::NONE; SIMPLE_DHT_SUBKEY_COUNT as usize],
            "network seqs should all be None"
        );
    }

    let mut set_futs = Vec::new();
    for rec in 0..SIMPLE_DHT_RECORD_COUNT {
        let tx = tx.clone();
        let key = keys[rec].clone();
        let d = data[rec].clone();
        set_futs.push(async move { tx.set(key, 1, d, None).await });
    }
    let set_results = futures_util::future::join_all(set_futs).await;
    for res in set_results {
        assert!(
            res.expect("set should succeed").is_none(),
            "set should return None"
        );
    }

    let mut inspect_futs = Vec::new();
    for rec in 0..SIMPLE_DHT_RECORD_COUNT {
        let tx = tx.clone();
        let key = keys[rec].clone();
        inspect_futs.push(async move { tx.inspect(key, None, DHTReportScope::SyncGet).await });
    }
    let inspect_results = futures_util::future::join_all(inspect_futs).await;
    for report in inspect_results {
        let report = report.expect("inspect should succeed");
        assert_eq!(
            report.local_seqs(),
            &vec![ValueSeqNum::NONE; SIMPLE_DHT_SUBKEY_COUNT as usize],
            "local seqs should still all be None before commit"
        );
        assert_eq!(
            report.network_seqs(),
            &vec![ValueSeqNum::NONE; SIMPLE_DHT_SUBKEY_COUNT as usize],
            "network seqs should still all be None before commit"
        );
    }

    tx.commit().await.expect("should commit");

    let tx2 = api
        .transact_dht_records(keys.clone(), None)
        .await
        .expect("should create transaction 2");

    let mut inspect_futs = Vec::new();
    for rec in 0..SIMPLE_DHT_RECORD_COUNT {
        let tx2 = tx2.clone();
        let key = keys[rec].clone();
        inspect_futs.push(async move { tx2.inspect(key, None, DHTReportScope::SyncGet).await });
    }
    let inspect_results = futures_util::future::join_all(inspect_futs).await;
    let expected_local_seqs: Vec<ValueSeqNum> = vec![ValueSeqNum::NONE, ValueSeqNum::ZERO];
    let expected_network_seqs: Vec<ValueSeqNum> = vec![ValueSeqNum::NONE, ValueSeqNum::ZERO];
    for report in inspect_results {
        let report = report.expect("inspect should succeed");
        assert_eq!(
            report.local_seqs(),
            &expected_local_seqs,
            "local seqs should show subkey 1 as seq 0"
        );
        assert_eq!(
            report.network_seqs(),
            &expected_network_seqs,
            "network seqs should show subkey 1 as seq 0"
        );
    }

    let expected = [None, Some(0u32)];
    for subkey in 0..SIMPLE_DHT_SUBKEY_COUNT {
        let mut get_futs = Vec::new();
        for rec in 0..SIMPLE_DHT_RECORD_COUNT {
            let tx2 = tx2.clone();
            let key = keys[rec].clone();
            get_futs.push(async move { (rec, tx2.get(key, subkey as ValueSubkey).await) });
        }
        let get_results = futures_util::future::join_all(get_futs).await;
        for (rec, res) in get_results {
            let val = res.expect("get should succeed");
            if expected[subkey as usize].is_none() {
                assert!(val.is_none(), "subkey {} should be None", subkey);
            } else {
                let val = val.unwrap_or_else(|| {
                    panic!("subkey {} should have data for rec {}", subkey, rec)
                });
                assert_eq!(val.data(), data[rec], "data should match");
                assert_eq!(val.seq(), 0.into(), "seq should be 0");
            }
        }
    }

    tx2.commit().await.expect("should commit tx2");
    delete_dht_records(&rc, &keys).await;
}

async fn test_full_records_inspect_and_gets(api: VeilidAPI) {
    let rc = api.routing_context().unwrap();
    let keys = create_dht_records(&rc, FULL_DHT_RECORD_COUNT, FULL_DHT_SUBKEY_COUNT).await;

    let tx = api
        .transact_dht_records(keys.clone(), None)
        .await
        .expect("should create transaction");

    let mut inspect_futs = Vec::new();
    for rec in 0..FULL_DHT_RECORD_COUNT {
        let tx = tx.clone();
        let key = keys[rec].clone();
        inspect_futs.push(async move { tx.inspect(key, None, DHTReportScope::SyncGet).await });
    }
    let inspect_results = futures_util::future::join_all(inspect_futs).await;
    let expected_seqs = vec![ValueSeqNum::NONE; FULL_DHT_SUBKEY_COUNT as usize];
    for report in inspect_results {
        let report = report.expect("inspect should succeed");
        assert_eq!(report.local_seqs(), &expected_seqs);
        assert_eq!(report.network_seqs(), &expected_seqs);
    }

    for subkey in 0..FULL_DHT_SUBKEY_COUNT {
        let mut get_futs = Vec::new();
        for rec in 0..FULL_DHT_RECORD_COUNT {
            let tx = tx.clone();
            let key = keys[rec].clone();
            get_futs.push(async move { tx.get(key, subkey as ValueSubkey).await });
        }
        let get_results = futures_util::future::join_all(get_futs).await;
        for res in get_results {
            assert!(
                res.expect("get should succeed").is_none(),
                "should be None for empty record"
            );
        }
    }

    tx.commit().await.expect("should commit");
    delete_dht_records(&rc, &keys).await;
}

async fn test_full_records_fill_commit_then_get(api: VeilidAPI) {
    let rc = api.routing_context().unwrap();
    let keys = create_dht_records(&rc, FULL_DHT_RECORD_COUNT, FULL_DHT_SUBKEY_COUNT).await;
    let data = make_test_data(FULL_DHT_RECORD_COUNT, FULL_DHT_SUBKEY_SIZE);

    let start = Timestamp::now();
    let tx = api
        .transact_dht_records(keys.clone(), None)
        .await
        .expect("should create transaction");
    info!("begin transaction: {:?}", TimestampDuration::since(start));

    for subkey in 0..FULL_DHT_SUBKEY_COUNT {
        let start_set = Timestamp::now();
        let mut set_futs = Vec::new();
        for rec in 0..FULL_DHT_RECORD_COUNT {
            let tx = tx.clone();
            let key = keys[rec].clone();
            let d = data[rec].clone();
            set_futs.push(async move { tx.set(key, subkey as ValueSubkey, d, None).await });
        }
        let set_results = futures_util::future::join_all(set_futs).await;
        for res in set_results {
            assert!(
                res.expect("set should succeed").is_none(),
                "set should return None"
            );
        }
        info!(
            "set subkey {}: {:?}",
            subkey,
            TimestampDuration::since(start_set)
        );
    }

    let start_commit = Timestamp::now();
    tx.commit().await.expect("should commit");
    info!(
        "commit transaction: {:?}",
        TimestampDuration::since(start_commit)
    );

    let start_begin2 = Timestamp::now();
    let tx2 = api
        .transact_dht_records(keys.clone(), None)
        .await
        .expect("should create transaction 2");
    info!(
        "begin transaction 2: {:?}",
        TimestampDuration::since(start_begin2)
    );

    for subkey in 0..FULL_DHT_SUBKEY_COUNT {
        let start_get = Timestamp::now();
        let mut get_futs = Vec::new();
        for rec in 0..FULL_DHT_RECORD_COUNT {
            let tx2 = tx2.clone();
            let key = keys[rec].clone();
            let d = data[rec].clone();
            get_futs.push(async move {
                let val = tx2.get(key, subkey as ValueSubkey).await;
                (rec, d, val)
            });
        }
        let get_results = futures_util::future::join_all(get_futs).await;
        for (rec, expected_data, res) in get_results {
            let val = res
                .unwrap_or_else(|e| panic!("get rec={} subkey={} failed: {}", rec, subkey, e))
                .unwrap_or_else(|| panic!("get rec={} subkey={} returned None", rec, subkey));
            assert_eq!(
                val.data(),
                expected_data,
                "data should match for rec={} subkey={}",
                rec,
                subkey
            );
            assert_eq!(
                val.seq(),
                0.into(),
                "seq should be 0 for rec={} subkey={}",
                rec,
                subkey
            );
        }
        info!(
            "get subkey {}: {:?}",
            subkey,
            TimestampDuration::since(start_get)
        );
    }

    let start_rollback = Timestamp::now();
    tx2.rollback().await.expect("should rollback tx2");
    info!(
        "rollback transaction 2: {:?}",
        TimestampDuration::since(start_rollback)
    );

    delete_dht_records(&rc, &keys).await;
}

pub async fn test_all() {
    let (update_callback, config) = fixture_veilid_core();
    let api = api_startup(update_callback, config)
        .await
        .expect("startup failed");

    let _ = api.attach().await;
    let _ = api.wait_for_attachment_condition(None, None).await.unwrap();

    info!("=== Simple transaction tests ===");

    info!("--- test_create_empty_transaction_and_drop ---");
    test_create_empty_transaction_and_drop(api.clone()).await;

    info!("--- test_create_empty_transaction_and_rollback ---");
    test_create_empty_transaction_and_rollback(api.clone()).await;

    info!("--- test_create_empty_transaction_and_commit ---");
    test_create_empty_transaction_and_commit(api.clone()).await;

    info!("--- test_transaction_sets_and_rollback ---");
    test_transaction_sets_and_rollback(api.clone()).await;

    info!("--- test_transaction_sets_and_commit ---");
    test_transaction_sets_and_commit(api.clone()).await;

    info!("--- test_transaction_sets_gets_and_commit ---");
    test_transaction_sets_gets_and_commit(api.clone()).await;

    info!("--- test_transaction_repeated_set_seq ---");
    test_transaction_repeated_set_seq(api.clone()).await;

    info!("--- test_transaction_set_back_to_base ---");
    test_transaction_set_back_to_base(api.clone()).await;

    info!("--- test_transaction_set_below_network ---");
    test_transaction_set_below_network(api.clone()).await;

    info!("--- test_transaction_failed_commit_releases ---");
    test_transaction_failed_commit_releases(api.clone()).await;

    info!("--- test_transaction_fail_non_transactional_sets ---");
    test_transaction_fail_non_transactional_sets(api.clone()).await;

    info!("--- test_transaction_inspect_set_commit_then_get ---");
    test_transaction_inspect_set_commit_then_get(api.clone()).await;

    info!("=== Full records transaction tests ===");

    info!("--- test_full_records_inspect_and_gets ---");
    test_full_records_inspect_and_gets(api.clone()).await;

    info!("--- test_full_records_fill_commit_then_get ---");
    test_full_records_fill_commit_then_get(api.clone()).await;

    api.shutdown().await;
}
