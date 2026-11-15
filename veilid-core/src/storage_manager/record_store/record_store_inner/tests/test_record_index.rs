use crate::tests::*;

use super::*;

type TestRecordIndex = RecordIndex<LocalRecordDetail>;
type RecordTestCase<'a> =
    Box<dyn FnOnce(TestRecordIndex, &'a CryptoSystemGuard<'a>) -> PinBoxFuture<'a, ()> + Send + 'a>;

// General test constants
const RECORD_COUNT: usize = 4;
const MAX_SUBKEY: u32 = 7;
const SUBKEY_SIZE: usize = MAX_SUBKEY_SIZE;

pub async fn test_record_index() {
    let registry = mock_registry::init("record_index").await;
    let crypto = registry.crypto();
    let config = registry.config();
    let table_store = registry.table_store();

    let local_config = StorageManager::local_limits_from_config(config.clone());
    let remote_config = StorageManager::remote_limits_from_config(config.clone());
    let mut tiny_config = StorageManager::remote_limits_from_config(config.clone());
    tiny_config.subkey_cache_size = 128;
    tiny_config.max_records = Some(10);
    tiny_config.max_subkey_cache_memory_mb = Some(2);
    tiny_config.max_storage_space_mb = Some(4);

    for crypto_kind in VALID_CRYPTO_KINDS {
        let vcrypto = crypto
            .get(crypto_kind)
            .expect("should get valid cryptosystem");
        for parallel in 1usize..=2 {
            for (prefix, limits) in [
                ("local", local_config),
                ("remote", remote_config),
                ("tiny", tiny_config),
            ] {
                let tests: [(&str, RecordTestCase); 3] = [
                    (
                        "test_access_missing",
                        Box::new(|ri, vc| pin_dyn_future!(test_access_missing(ri, vc))),
                    ),
                    (
                        "test_subkey_access_range",
                        Box::new(|ri, vc| pin_dyn_future!(test_subkey_access_range(ri, vc))),
                    ),
                    (
                        "test_create_read_write_delete_loop",
                        Box::new(|ri, vc| {
                            pin_dyn_future!(test_create_read_write_delete_loop(ri, vc))
                        }),
                    ),
                ];

                // test_read_write_bulk(&mut record_index, &vcrypto).await;
                // test_create_read_write_delete_bulk(&mut record_index, &vcrypto).await;
                // test_stress(&mut record_index, &vcrypto).await;
                // test_create_read_write_delete_loop_parallel(&mut record_index, &vcrypto).await;
                // test_read_write_bulk_parallel(&mut record_index, &vcrypto).await;
                // test_create_read_write_delete_bulk_parallel(&mut record_index, &vcrypto).await;
                // test_stress_parallel(&mut record_index, &vcrypto).await;

                for (test_name, test_func) in tests {
                    let context_name = format!("{}_{}_{}", prefix, parallel, vcrypto.kind());

                    let record_table_name = format!("records_{}_{}", context_name, test_name);
                    let subkey_table_name = format!("subkeys_{}_{}", context_name, test_name);

                    let _ = table_store.delete(&record_table_name).await;
                    let _ = table_store.delete(&subkey_table_name).await;

                    let record_table = table_store
                        .open_pooled(&record_table_name, 1, parallel)
                        .await
                        .expect("should open");
                    let subkey_table = table_store
                        .open_pooled(&subkey_table_name, 1, parallel)
                        .await
                        .expect("should open");

                    let unlocked_inner = RecordStoreUnlockedInner {
                        registry: registry.clone(),
                        name: format!("{}_{}", context_name, test_name),
                        limits,
                        record_table,
                        subkey_table,
                        opt_inbound_expiry: None,
                    };
                    let record_index = TestRecordIndex::try_new(Arc::new(unlocked_inner))
                        .await
                        .expect("should make new recordindex");
                    let debug = record_index.debug();
                    assert!(debug.contains("Total Storage Space"), "{debug}");

                    info!("{}: {}", context_name, test_name);
                    test_func(record_index, &vcrypto).await;
                }
            }
        }
    }

    // Reopen-based tests own their table lifecycle: they rebuild an index over
    // the same tables, which the harness above never does
    for crypto_kind in VALID_CRYPTO_KINDS {
        let vcrypto = crypto
            .get(crypto_kind)
            .expect("should get valid cryptosystem");
        for test_name in ["test_reopen_persistence", "test_subkey_seq_heal"] {
            let context_name = format!("reopen_{}", vcrypto.kind());
            let record_table_name = format!("records_{}_{}", context_name, test_name);
            let subkey_table_name = format!("subkeys_{}_{}", context_name, test_name);

            let _ = table_store.delete(&record_table_name).await;
            let _ = table_store.delete(&subkey_table_name).await;

            let record_table = table_store
                .open_pooled(&record_table_name, 1, 1)
                .await
                .expect("should open");
            let subkey_table = table_store
                .open_pooled(&subkey_table_name, 1, 1)
                .await
                .expect("should open");

            let unlocked_inner = Arc::new(RecordStoreUnlockedInner {
                registry: registry.clone(),
                name: format!("{}_{}", context_name, test_name),
                limits: local_config,
                record_table,
                subkey_table,
                opt_inbound_expiry: None,
            });

            info!("{}: {}", context_name, test_name);
            match test_name {
                "test_reopen_persistence" => {
                    test_reopen_persistence(unlocked_inner, &vcrypto).await;
                }
                _ => test_subkey_seq_heal(unlocked_inner, &vcrypto).await,
            }
        }
    }

    // The record count limit with storage to spare: the remote store at its record cap
    for crypto_kind in VALID_CRYPTO_KINDS {
        let vcrypto = crypto
            .get(crypto_kind)
            .expect("should get valid cryptosystem");
        let context_name = format!("count_limit_{}", vcrypto.kind());
        let record_table_name = format!("records_{}", context_name);
        let subkey_table_name = format!("subkeys_{}", context_name);

        let _ = table_store.delete(&record_table_name).await;
        let _ = table_store.delete(&subkey_table_name).await;

        let record_table = table_store
            .open_pooled(&record_table_name, 1, 1)
            .await
            .expect("should open");
        let subkey_table = table_store
            .open_pooled(&subkey_table_name, 1, 1)
            .await
            .expect("should open");

        let mut limits = remote_config;
        limits.max_records = Some(RECORD_COUNT);
        let unlocked_inner = Arc::new(RecordStoreUnlockedInner {
            registry: registry.clone(),
            name: context_name.clone(),
            limits,
            record_table,
            subkey_table,
            opt_inbound_expiry: None,
        });

        info!("{}: test_record_count_limit", context_name);
        test_record_count_limit(unlocked_inner, &vcrypto).await;
    }

    mock_registry::terminate(registry).await;
}

fn random_opaque_record_key(vcrypto: &CryptoSystemGuard<'_>) -> OpaqueRecordKey {
    OpaqueRecordKey::new(
        vcrypto.kind(),
        BareOpaqueRecordKey::new(&vcrypto.random_bytes(vcrypto.hash_digest_length())),
    )
}

fn nth_opaque_record_key(vcrypto: &CryptoSystemGuard<'_>, n: usize) -> OpaqueRecordKey {
    OpaqueRecordKey::new(
        vcrypto.kind(),
        BareOpaqueRecordKey::new(
            vcrypto
                .generate_hash(&n.to_le_bytes())
                .into_value()
                .as_ref(),
        ),
    )
}

fn random_signed_value_data(
    vcrypto: &CryptoSystemGuard<'_>,
    seq: ValueSeqNum,
    size: usize,
) -> Arc<SignedValueData> {
    let random_keypair = vcrypto.generate_keypair();
    let random_nonce = vcrypto.random_nonce();
    let random_signature = Signature::new(
        vcrypto.kind(),
        BareSignature::new(&vcrypto.random_bytes(vcrypto.signature_length())),
    );
    let data = vcrypto.random_bytes(size);
    Arc::new(SignedValueData::new(
        EncryptedValueData::try_new(seq, data.into(), random_keypair.key(), Some(random_nonce))
            .expect("should validate"),
        random_signature,
    ))
}

fn random_signed_value_descriptor(
    vcrypto: &CryptoSystemGuard<'_>,
    opt_max_subkey: Option<u32>,
) -> Arc<SignedValueDescriptor> {
    let random_keypair = vcrypto.generate_keypair();
    let random_schema_data = DHTSchema::dflt(
        opt_max_subkey.unwrap_or_else(|| (random::get_random_u32() % MAX_SUBKEY) + 1) as u16,
    )
    .expect("should make dflt schema")
    .compile();
    let random_signature = Signature::new(
        vcrypto.kind(),
        BareSignature::new(&vcrypto.random_bytes(vcrypto.signature_length())),
    );

    Arc::new(SignedValueDescriptor::new(
        random_keypair.key(),
        random_schema_data.into(),
        random_signature,
    ))
}

#[allow(clippy::unused_async)]
pub async fn test_access_missing(
    mut record_index: TestRecordIndex,
    vcrypto: &CryptoSystemGuard<'_>,
) {
    let svd = random_signed_value_data(vcrypto, ValueSeqNum::from(0), SUBKEY_SIZE);

    let mut keys = vec![];
    for _ in 0..RECORD_COUNT {
        let key = random_opaque_record_key(vcrypto);
        keys.push(key.clone());

        assert_eq!(
            record_index.with_record(&key, |_| {
                panic!("should have no record");
            }),
            Ok(None)
        );
        assert_eq!(
            record_index.with_record_detail_mut(&key, |_, _| {
                panic!("should have no record");
            }),
            Ok(None)
        );
        assert!(
            !record_index.contains_record(&key),
            "should not contain record"
        );
        assert_eq!(
            record_index.peek_lru(|k, v| {
                panic!("should have no record {}:{:?}", k, v);
            }),
            None
        );
        assert_eq!(
            record_index.peek_record(&key, |v| {
                panic!("should have no record {}:{:?}", key, v);
            }),
            None
        );

        // Check key not found set and load
        for sk in 0..=MAX_SUBKEY {
            let sk = ValueSubkey::from(sk);
            assert_eq!(
                record_index.set_single_subkey(&key, sk, svd.clone()),
                Err(VeilidAPIError::KeyNotFound { key: key.clone() })
            );

            let lar = record_index.prepare_load_action(key.clone(), sk, false);
            assert!(
                matches!(lar, Ok(LoadActionResult::NoRecord)),
                "should be no record"
            );
        }

        // Check multiple subkeys
        let svl = SubkeyValueList::from_iter((0..=MAX_SUBKEY).map(|x| (x, svd.clone())));
        assert_eq!(
            record_index.set_subkeys_single_record(&key, &svl),
            Err(VeilidAPIError::KeyNotFound { key: key.clone() })
        );
    }

    // Check multiple subkeys on multiple records
    let ksvl = keys
        .into_iter()
        .map(|k| {
            (
                k,
                SubkeyValueList::from_iter((0..=MAX_SUBKEY).map(|x| (x, svd.clone()))),
            )
        })
        .collect();
    assert_eq!(
        record_index.set_subkeys_multiple_records(&ksvl),
        Err(VeilidAPIError::KeyNotFound {
            key: ksvl.first().as_ref().unwrap().0.clone()
        })
    );
}

pub async fn test_subkey_access_range(
    mut record_index: TestRecordIndex,
    vcrypto: &CryptoSystemGuard<'_>,
) {
    let svd = random_signed_value_data(vcrypto, ValueSeqNum::from(0), SUBKEY_SIZE);

    let mut keys = vec![];
    let mut descriptors = vec![];
    for _ in 0..RECORD_COUNT {
        let key = random_opaque_record_key(vcrypto);
        keys.push(key.clone());

        // Create the record to test it for real
        let desc = random_signed_value_descriptor(vcrypto, None);
        descriptors.push(desc.clone());

        let max_subkey = desc.schema().unwrap().max_subkey();

        record_index
            .create_record(
                key.clone(),
                Record::<LocalRecordDetail>::try_new(
                    Timestamp::now(),
                    desc.clone(),
                    LocalRecordDetail::new(SafetySelection::Unsafe(Sequencing::PreferUnordered)),
                )
                .expect("should create record data"),
            )
            .expect("should create record");

        assert!(record_index.contains_record(&key), "should contain record");
        assert_eq!(
            record_index.peek_lru(|_, _| { true }),
            Some(true),
            "should peek lru"
        );
        assert_eq!(
            record_index.peek_record(&key, |_| { true }),
            Some(true),
            "should peek record"
        );

        // Check key set and load
        for sk in 0..=max_subkey {
            let sk = ValueSubkey::from(sk);

            let lar = record_index
                .prepare_load_action(key.clone(), sk, false)
                .expect("should prepare load action");
            assert!(
                matches!(lar, LoadActionResult::NoSubkey { .. }),
                "should get no subkey load action"
            );

            assert_eq!(
                record_index.set_single_subkey(&key, sk, svd.clone()),
                Ok(()),
                "should set subkey"
            );

            let lar = record_index
                .prepare_load_action(key.clone(), sk, false)
                .expect("should prepare load action");
            let LoadActionResult::Subkey {
                mut load_action,
                descriptor: _,
            } = lar
            else {
                panic!("should get a subkey load action");
            };

            let Some(value1) = load_action.load().await.expect("should load") else {
                panic!("should load");
            };

            record_index.finish_load_action(load_action);

            // Commit the subkey
            let mut commit_action = record_index
                .prepare_commit_action()
                .expect("should prepare commit action");

            commit_action.commit().await.expect("should commit");

            record_index.finish_commit_action(commit_action);

            let lar = record_index
                .prepare_load_action(key.clone(), sk, false)
                .expect("should prepare load action");
            let LoadActionResult::Subkey {
                mut load_action,
                descriptor: _,
            } = lar
            else {
                panic!("should get a subkey load action");
            };

            let Some(value2) = load_action.load().await.expect("should load") else {
                panic!("should load");
            };
            record_index.finish_load_action(load_action);

            // Should get same value after commit
            assert_eq!(value1, value2, "should get same value after commit");
        }

        // Check out of range subkey set
        let sk = ValueSubkey::from(max_subkey + 1);
        assert!(
            matches!(
                record_index.set_single_subkey(&key, sk, svd.clone()),
                Err(VeilidAPIError::InvalidArgument { .. })
            ),
            "should be invalid argument"
        );
        assert!(
            matches!(
                record_index.prepare_load_action(key.clone(), sk, false),
                Err(VeilidAPIError::InvalidArgument { .. })
            ),
            "should be invalid argument"
        );

        // Check multiple subkeys
        let svl = SubkeyValueList::from_iter((0..=max_subkey).map(|x| (x, svd.clone())));
        assert_eq!(record_index.set_subkeys_single_record(&key, &svl), Ok(()));

        // Check key set and load
        let mut subkey_values1 = vec![];
        for sk in 0..=max_subkey {
            let sk = ValueSubkey::from(sk);

            let lar = record_index
                .prepare_load_action(key.clone(), sk, false)
                .expect("should prepare load action");
            let LoadActionResult::Subkey {
                mut load_action,
                descriptor: _,
            } = lar
            else {
                panic!("should get a subkey load action");
            };

            let Some(value1) = load_action.load().await.expect("should load") else {
                panic!("should load");
            };
            record_index.finish_load_action(load_action);

            subkey_values1.push(value1);
        }

        // Commit the subkeys
        let mut commit_action = record_index
            .prepare_commit_action()
            .expect("should prepare commit action");

        commit_action.commit().await.expect("should commit");

        record_index.finish_commit_action(commit_action);

        let mut subkey_values2 = vec![];
        for sk in 0..=max_subkey {
            let lar = record_index
                .prepare_load_action(key.clone(), sk, false)
                .expect("should prepare load action");
            let LoadActionResult::Subkey {
                mut load_action,
                descriptor: _,
            } = lar
            else {
                panic!("should get a subkey load action");
            };
            let Some(value2) = load_action.load().await.expect("should load") else {
                panic!("should load");
            };
            record_index.finish_load_action(load_action);

            subkey_values2.push(value2);
        }

        assert_eq!(
            subkey_values1, subkey_values2,
            "should get same values after commit"
        );

        // Check multiple subkeys with one out of range
        let svl = SubkeyValueList::from_iter((0..=max_subkey + 1).map(|x| (x, svd.clone())));
        assert!(
            matches!(
                record_index.set_subkeys_single_record(&key, &svl),
                Err(VeilidAPIError::InvalidArgument { .. })
            ),
            "should be invalid argument"
        );
    }
    // Check multiple subkeys on multiple records
    let ksvl = keys
        .iter()
        .enumerate()
        .filter_map(|(i, k)| {
            if record_index.contains_record(k) {
                let max_subkey = descriptors[i].schema().unwrap().max_subkey();
                Some((
                    k.clone(),
                    SubkeyValueList::from_iter((0..=max_subkey).map(|x| (x, svd.clone()))),
                ))
            } else {
                None
            }
        })
        .collect();
    assert_eq!(record_index.set_subkeys_multiple_records(&ksvl), Ok(()));

    // Check key set and load
    for (i, key) in keys.iter().enumerate() {
        if !record_index.contains_record(key) {
            continue;
        }
        let max_subkey = descriptors[i].schema().unwrap().max_subkey();
        for sk in 0..=max_subkey {
            let sk = ValueSubkey::from(sk);

            let lar = record_index
                .prepare_load_action(key.clone(), sk, false)
                .expect("should prepare load action");
            let LoadActionResult::Subkey {
                mut load_action,
                descriptor: _,
            } = lar
            else {
                panic!("should get a subkey load action: {:?}", lar);
            };

            let Some(value1) = load_action.load().await.expect("should load") else {
                panic!("should load");
            };
            record_index.finish_load_action(load_action);

            assert_eq!(
                value1.signed_value_data(),
                svd.clone(),
                "should get same value"
            );
        }
    }

    // Check multiple subkeys with one out of range on multiple records
    let ksvl = keys
        .iter()
        .enumerate()
        .filter_map(|(i, k)| {
            if record_index.contains_record(k) {
                let max_subkey = descriptors[i].schema().unwrap().max_subkey();
                Some((
                    k.clone(),
                    SubkeyValueList::from_iter((0..=max_subkey + 1).map(|x| (x, svd.clone()))),
                ))
            } else {
                None
            }
        })
        .collect();
    assert!(
        matches!(
            record_index.set_subkeys_multiple_records(&ksvl),
            Err(VeilidAPIError::InvalidArgument { .. })
        ),
        "should be invalid argument"
    );

    for key in keys.into_iter() {
        if record_index.contains_record(&key) {
            assert_eq!(record_index.delete_record(key), Ok(()));
        }
    }
}

pub async fn test_create_read_write_delete_loop(
    mut record_index: TestRecordIndex,
    vcrypto: &CryptoSystemGuard<'_>,
) {
    let svd = random_signed_value_data(vcrypto, ValueSeqNum::from(0), SUBKEY_SIZE);

    let mut keys = vec![];
    for n in 0..RECORD_COUNT {
        let key = nth_opaque_record_key(vcrypto, n);
        keys.push(key.clone());

        let descriptor = random_signed_value_descriptor(vcrypto, None);
        let max_subkey = descriptor.schema().unwrap().max_subkey();
        let detail = LocalRecordDetail {
            safety_selection: SafetySelection::Unsafe(Sequencing::PreferUnordered),
            nodes: HashMap::new(),
            transaction_membership: None,
        };
        record_index
            .create_record(
                key.clone(),
                Record::<LocalRecordDetail>::try_new(
                    Timestamp::now(),
                    descriptor.clone(),
                    detail.clone(),
                )
                .expect("should create record definition"),
            )
            .expect("should create record");

        assert_eq!(
            record_index.peek_record(&key, |f| {
                assert_eq!(f.descriptor(), descriptor);
                assert_eq!(f.detail(), &detail);
                assert!(f.is_new());
                assert_eq!(f.schema(), descriptor.schema().unwrap());
                assert_eq!(f.max_subkey(), max_subkey);
                assert_eq!(
                    f.subkey_count(),
                    descriptor.schema().unwrap().subkey_count()
                );
                assert_eq!(f.owner(), descriptor.owner());
                assert!(f.stored_subkeys().is_empty());
                true
            }),
            Some(true)
        );

        for subkey in 0..max_subkey {
            record_index
                .set_single_subkey(&key, subkey, svd.clone())
                .expect("should set subkey");

            if let Some(mut commit_action) = record_index.maybe_prepare_commit_action() {
                commit_action.commit().await.expect("should commit");
                record_index.finish_commit_action(commit_action);
            }

            assert_eq!(
                record_index.peek_record(&key, |f| {
                    assert_eq!(
                        f.stored_subkeys(),
                        &ValueSubkeyRangeSet::single_range(0, subkey)
                    );
                    true
                }),
                Some(true)
            );
        }
        for subkey in 0..max_subkey {
            let lar = record_index
                .prepare_load_action(key.clone(), subkey, false)
                .expect("should prepare load action");
            let mut la = match lar {
                LoadActionResult::NoRecord => {
                    panic!("no record");
                }
                LoadActionResult::NoSubkey { descriptor: _ } => {
                    panic!("no subkey");
                }
                LoadActionResult::Subkey {
                    descriptor: _,
                    load_action,
                } => load_action,
            };
            let opt_record_data = la.load().await.expect("should load");
            record_index.finish_load_action(la);

            assert_eq!(opt_record_data.unwrap().signed_value_data(), svd);
        }

        record_index.delete_record(key).expect("should delete");
    }
}

// pub async fn test_read_write_bulk(
//     record_index: TestRecordIndex,
//     vcrypto: &CryptoSystemGuard<'_>,
// ) {
//     //
// }
// pub async fn test_read_write_delete_bulk(
//     record_index: TestRecordIndex,
//     vcrypto: &CryptoSystemGuard<'_>,
// ) {
//     //
// }
// pub async fn test_stress(record_index: &mut TestRecordIndex, vcrypto: &CryptoSystemGuard<'_>) {
//     //
// }
// pub async fn test_create_read_write_delete_loop_parallel(
//     record_index: TestRecordIndex,
//     vcrypto: &CryptoSystemGuard<'_>,
// ) {
//     //
// }
// pub async fn test_read_write_bulk_parallel(
//     record_index: TestRecordIndex,
//     vcrypto: &CryptoSystemGuard<'_>,
// ) {
//     //
// }
// pub async fn test_create_read_write_delete_bulk_parallel(
//     record_index: TestRecordIndex,
//     vcrypto: &CryptoSystemGuard<'_>,
// ) {
//     //
// }
// pub async fn test_stress_parallel(
//     record_index: TestRecordIndex,
//     vcrypto: &CryptoSystemGuard<'_>,
// ) {
//     //
// }

fn mirror_seq(
    record_index: &mut TestRecordIndex,
    key: &OpaqueRecordKey,
    subkey: ValueSubkey,
) -> ValueSeqNum {
    record_index
        .peek_record(key, |r| r.subkey_seq(subkey).expect("subkey in range"))
        .expect("record should exist")
}

async fn load_row(
    record_index: &mut TestRecordIndex,
    key: &OpaqueRecordKey,
    subkey: ValueSubkey,
) -> RecordData {
    let lar = record_index
        .prepare_load_action(key.clone(), subkey, false)
        .expect("should prepare load action");
    let LoadActionResult::Subkey {
        mut load_action,
        descriptor: _,
    } = lar
    else {
        panic!("should get a subkey load action");
    };
    let Some(value) = load_action.load().await.expect("should load") else {
        panic!("row should be present");
    };
    record_index.finish_load_action(load_action);
    value
}

async fn commit_all(record_index: &mut TestRecordIndex) {
    let mut commit_action = record_index
        .prepare_commit_action()
        .expect("should prepare commit action");
    commit_action.commit().await.expect("should commit");
    record_index.finish_commit_action(commit_action);
}

/// A committed record, its subkey rows and its seq mirror survive try_new over
/// the same tables. Nothing else in this file reopens an index, so this is
/// what pins load_db's reconstruction.
pub async fn test_reopen_persistence(
    unlocked_inner: Arc<RecordStoreUnlockedInner>,
    vcrypto: &CryptoSystemGuard<'_>,
) {
    let key = nth_opaque_record_key(vcrypto, 0);
    let desc = random_signed_value_descriptor(vcrypto, Some(MAX_SUBKEY));
    let sk = ValueSubkey::from(0u32);
    let svd = random_signed_value_data(vcrypto, ValueSeqNum::from(5), SUBKEY_SIZE);

    // Session 1: create, set, commit
    {
        let mut record_index = TestRecordIndex::try_new(unlocked_inner.clone())
            .await
            .expect("should make new recordindex");
        record_index
            .create_record(
                key.clone(),
                Record::<LocalRecordDetail>::try_new(
                    Timestamp::now(),
                    desc.clone(),
                    LocalRecordDetail::new(SafetySelection::Unsafe(Sequencing::PreferUnordered)),
                )
                .expect("should create record data"),
            )
            .expect("should create record");
        record_index
            .set_single_subkey(&key, sk, svd.clone())
            .expect("should set subkey");
        commit_all(&mut record_index).await;
    }

    // Session 2: record, row and mirror all survived
    {
        let mut record_index = TestRecordIndex::try_new(unlocked_inner.clone())
            .await
            .expect("should reopen recordindex");
        assert!(
            record_index.contains_record(&key),
            "record should survive reopen"
        );
        assert_eq!(
            mirror_seq(&mut record_index, &key, sk),
            ValueSeqNum::from(5),
            "seq mirror should survive reopen"
        );
        let value = load_row(&mut record_index, &key, sk).await;
        assert_eq!(
            value.signed_value_data().value_data().seq(),
            ValueSeqNum::from(5),
            "row should survive reopen"
        );
    }
}

/// At the record count limit, making room for a new record evicts the least recently used one
/// itself, so the cache never evicts behind the index's back when the record is inserted
pub async fn test_record_count_limit(
    unlocked_inner: Arc<RecordStoreUnlockedInner>,
    vcrypto: &CryptoSystemGuard<'_>,
) {
    let limit = unlocked_inner.limits.max_records.expect("count limit");
    let mut record_index = TestRecordIndex::try_new(unlocked_inner)
        .await
        .expect("should make new recordindex");
    let desc = random_signed_value_descriptor(vcrypto, Some(MAX_SUBKEY));
    let new_record = || {
        Record::<LocalRecordDetail>::try_new(
            Timestamp::now(),
            desc.clone(),
            LocalRecordDetail::new(SafetySelection::Unsafe(Sequencing::PreferUnordered)),
        )
        .expect("should create record data")
    };
    let keys: Vec<_> = (0..=limit)
        .map(|n| nth_opaque_record_key(vcrypto, n))
        .collect();
    for key in &keys[..limit] {
        record_index
            .create_record(key.clone(), new_record())
            .expect("should create record");
    }

    // The slot is the only pressure: no bytes to fit, one record to admit
    record_index
        .make_room_for_record(0, None)
        .expect("should make room");
    assert!(
        !record_index.contains_record(&keys[0]),
        "the least recently used record goes before the new one is inserted"
    );
    for key in &keys[1..limit] {
        assert!(record_index.contains_record(key), "only one record goes");
    }

    record_index
        .create_record(keys[limit].clone(), new_record())
        .expect("should create record at the limit");
    assert!(
        record_index.contains_record(&keys[limit]),
        "the new record is in"
    );
    for key in &keys[1..limit] {
        assert!(
            record_index.contains_record(key),
            "the create itself evicts nothing more"
        );
    }
}

/// A row newer than the record's seq mirror (the on-disk shape of a mirror
/// left behind) is healed by one read, the heal persists, and a STALE row
/// never regresses the mirror.
pub async fn test_subkey_seq_heal(
    unlocked_inner: Arc<RecordStoreUnlockedInner>,
    vcrypto: &CryptoSystemGuard<'_>,
) {
    let key = nth_opaque_record_key(vcrypto, 1);
    let desc = random_signed_value_descriptor(vcrypto, Some(MAX_SUBKEY));
    let sk = ValueSubkey::from(0u32);
    let stk = SubkeyTableKey {
        record_key: key.clone(),
        subkey: sk,
    };

    // Session 1: the row and mirror agree at seq 7
    {
        let mut record_index = TestRecordIndex::try_new(unlocked_inner.clone())
            .await
            .expect("should make new recordindex");
        record_index
            .create_record(
                key.clone(),
                Record::<LocalRecordDetail>::try_new(
                    Timestamp::now(),
                    desc.clone(),
                    LocalRecordDetail::new(SafetySelection::Unsafe(Sequencing::PreferUnordered)),
                )
                .expect("should create record data"),
            )
            .expect("should create record");
        record_index
            .set_single_subkey(
                &key,
                sk,
                random_signed_value_data(vcrypto, ValueSeqNum::from(7), SUBKEY_SIZE),
            )
            .expect("should set subkey");
        commit_all(&mut record_index).await;
    }

    // Forge a newer row directly in the subkey table, bypassing the index
    let newer = RecordData::new(random_signed_value_data(
        vcrypto,
        ValueSeqNum::from(9),
        SUBKEY_SIZE,
    ));
    unlocked_inner
        .subkey_table
        .store_serialized(0, Bytes::from(stk.bytes()), serialize_table_value(&newer))
        .await
        .expect("should forge newer row");

    // Session 2: the mirror is behind; one read heals it
    {
        let mut record_index = TestRecordIndex::try_new(unlocked_inner.clone())
            .await
            .expect("should reopen recordindex");
        assert_eq!(
            mirror_seq(&mut record_index, &key, sk),
            ValueSeqNum::from(7),
            "mirror should still be behind before any read"
        );
        let value = load_row(&mut record_index, &key, sk).await;
        assert_eq!(
            value.signed_value_data().value_data().seq(),
            ValueSeqNum::from(9),
            "read should return the newer row"
        );
        assert_eq!(
            mirror_seq(&mut record_index, &key, sk),
            ValueSeqNum::from(9),
            "read should heal the mirror"
        );
        commit_all(&mut record_index).await;
    }

    // Session 3: the heal persisted
    {
        let mut record_index = TestRecordIndex::try_new(unlocked_inner.clone())
            .await
            .expect("should reopen recordindex");
        assert_eq!(
            mirror_seq(&mut record_index, &key, sk),
            ValueSeqNum::from(9),
            "healed mirror should survive reopen"
        );
    }

    // Forge a STALE row; forward-only means the mirror must not regress
    let stale = RecordData::new(random_signed_value_data(
        vcrypto,
        ValueSeqNum::from(3),
        SUBKEY_SIZE,
    ));
    unlocked_inner
        .subkey_table
        .store_serialized(0, Bytes::from(stk.bytes()), serialize_table_value(&stale))
        .await
        .expect("should forge stale row");
    {
        let mut record_index = TestRecordIndex::try_new(unlocked_inner.clone())
            .await
            .expect("should reopen recordindex");
        let value = load_row(&mut record_index, &key, sk).await;
        assert_eq!(
            value.signed_value_data().value_data().seq(),
            ValueSeqNum::from(3),
            "read returns the row as stored"
        );
        assert_eq!(
            mirror_seq(&mut record_index, &key, sk),
            ValueSeqNum::from(9),
            "a stale row must not regress the mirror"
        );
    }
}
