use super::migration::Migration;
use super::*;
use crate::tests::*;

async fn startup() -> VeilidAPI {
    info!("test_table_store: starting");
    let (update_callback, config) = fixture_veilid_core();
    api_startup(update_callback, config)
        .await
        .expect("startup failed")
}

async fn shutdown(api: VeilidAPI) {
    info!("test_table_store: shutting down");
    api.shutdown().await;
    info!("test_table_store: finished");
}

async fn test_delete_open_delete(ts: &TableStore) {
    info!("test_delete_open_delete");

    let _ = ts.delete("test").await;
    let db = ts.open("test", 3).await.expect("should have opened");
    assert!(
        ts.delete("test").await.is_err(),
        "should fail because file is opened"
    );
    drop(db);
    assert!(
        ts.delete("test").await.is_ok(),
        "should succeed because file is closed"
    );
    let db = ts.open("test", 3).await.expect("should have opened");
    assert!(
        ts.delete("test").await.is_err(),
        "should fail because file is opened"
    );
    drop(db);
    let db = ts.open("test", 3).await.expect("should have opened");
    assert!(
        ts.delete("test").await.is_err(),
        "should fail because file is opened"
    );
    drop(db);
    assert!(
        ts.delete("test").await.is_ok(),
        "should succeed because file is closed"
    );
}

async fn test_store_delete_load(ts: &TableStore) {
    info!("test_store_delete_load");

    let _ = ts.delete("test").await;
    let db = ts.open("test", 3).await.expect("should have opened");
    assert!(
        ts.delete("test").await.is_err(),
        "should fail because file is opened"
    );

    assert_eq!(
        db.load(0, Bytes::from_static(b"foo")).await.unwrap(),
        None,
        "should not load missing key"
    );
    assert!(
        db.store(
            1,
            Bytes::from_static(b"foo"),
            Bytes::from_static(b"1234567890")
        )
        .await
        .is_ok(),
        "should store new key"
    );
    assert_eq!(
        db.load(0, Bytes::from_static(b"foo")).await.unwrap(),
        None,
        "should not load missing key"
    );
    assert_eq!(
        db.load(1, Bytes::from_static(b"foo")).await.unwrap(),
        Some(Bytes::copy_from_slice(b"1234567890"))
    );

    assert!(
        db.store(1, Bytes::from_static(b"bar"), Bytes::from_static(b"FNORD"))
            .await
            .is_ok(),
        "should store new key"
    );
    assert!(
        db.store(
            0,
            Bytes::from_static(b"bar"),
            Bytes::from_static(b"ABCDEFGHIJKLMNOPQRSTUVWXYZ")
        )
        .await
        .is_ok(),
        "should store new key"
    );
    assert!(
        db.store(2, Bytes::from_static(b"bar"), Bytes::from_static(b"FNORD"))
            .await
            .is_ok(),
        "should store new key"
    );
    assert!(
        db.store(2, Bytes::from_static(b"baz"), Bytes::from_static(b"QWERTY"))
            .await
            .is_ok(),
        "should store new key"
    );
    assert!(
        db.store(
            2,
            Bytes::from_static(b"bar"),
            Bytes::from_static(b"QWERTYUIOP")
        )
        .await
        .is_ok(),
        "should store new key"
    );

    assert_eq!(
        db.load(1, Bytes::from_static(b"bar")).await.unwrap(),
        Some(Bytes::copy_from_slice(b"FNORD"))
    );
    assert_eq!(
        db.load(0, Bytes::from_static(b"bar")).await.unwrap(),
        Some(Bytes::copy_from_slice(b"ABCDEFGHIJKLMNOPQRSTUVWXYZ"))
    );
    assert_eq!(
        db.load(2, Bytes::from_static(b"bar")).await.unwrap(),
        Some(Bytes::copy_from_slice(b"QWERTYUIOP"))
    );
    assert_eq!(
        db.load(2, Bytes::from_static(b"baz")).await.unwrap(),
        Some(Bytes::copy_from_slice(b"QWERTY"))
    );

    assert_eq!(
        db.delete(1, Bytes::from_static(b"bar")).await.unwrap(),
        Some(Bytes::copy_from_slice(b"FNORD"))
    );
    assert_eq!(
        db.delete(1, Bytes::from_static(b"bar")).await.unwrap(),
        None
    );
    assert!(
        db.delete(4, Bytes::from_static(b"bar")).await.is_err(),
        "can't delete from column that doesn't exist"
    );

    drop(db);
    let db = ts.open("test", 3).await.expect("should have opened");

    assert_eq!(db.load(1, Bytes::from_static(b"bar")).await.unwrap(), None);
    assert_eq!(
        db.load(0, Bytes::from_static(b"bar")).await.unwrap(),
        Some(Bytes::copy_from_slice(b"ABCDEFGHIJKLMNOPQRSTUVWXYZ"))
    );
    assert_eq!(
        db.load(2, Bytes::from_static(b"bar")).await.unwrap(),
        Some(Bytes::copy_from_slice(b"QWERTYUIOP"))
    );
    assert_eq!(
        db.load(2, Bytes::from_static(b"baz")).await.unwrap(),
        Some(Bytes::copy_from_slice(b"QWERTY"))
    );
}

async fn test_transaction(ts: &TableStore) {
    info!("test_transaction");

    let _ = ts.delete("test").await;
    let db = ts.open("test", 3).await.expect("should have opened");
    assert!(
        ts.delete("test").await.is_err(),
        "should fail because file is opened"
    );

    let tx = db.transact();
    assert!(tx
        .store(
            0,
            Bytes::from_static(b"aaa"),
            Bytes::from_static(b"a-value")
        )
        .await
        .is_ok());
    let b_value_serialized = serialize_table_value(&"b-value".to_owned());
    assert!(tx
        .store_serialized(1, Bytes::from_static(b"bbb"), b_value_serialized)
        .await
        .is_ok());
    assert!(tx
        .store(
            3,
            Bytes::from_static(b"ddd"),
            Bytes::from_static(b"d-value")
        )
        .await
        .is_err());
    assert!(tx
        .store(
            0,
            Bytes::from_static(b"ddd"),
            Bytes::from_static(b"d-value")
        )
        .await
        .is_ok());
    assert!(tx.delete(0, Bytes::from_static(b"ddd")).await.is_ok());
    assert!(tx.commit().await.is_ok());

    let tx = db.transact();
    assert!(tx.delete(2, Bytes::from_static(b"ccc")).await.is_ok());
    tx.rollback();

    assert_eq!(
        db.load(0, Bytes::from_static(b"aaa")).await,
        Ok(Some(Bytes::copy_from_slice(b"a-value")))
    );
    let b_value_serialized = db.load_serialized(1, Bytes::from_static(b"bbb")).await;
    assert!(matches!(b_value_serialized, Ok(Some(_))));
    let b_value: String = ts
        .try_deserialize(&b_value_serialized.unwrap().unwrap())
        .unwrap();
    assert_eq!(b_value, "b-value".to_owned());
    assert_eq!(db.load(0, Bytes::from_static(b"ddd")).await, Ok(None));
}

async fn test_json(vcrypto: &AsyncCryptoSystemGuard<'_>, ts: &TableStore) {
    info!("test_json");

    let _ = ts.delete("test").await;
    let db = ts.open("test", 3).await.expect("should have opened");
    let keypair = vcrypto.generate_keypair().await;

    let serialized = serialize_table_value(&keypair);
    assert!(db
        .store_serialized(0, Bytes::from_static(b"asdf"), serialized)
        .await
        .is_ok());

    assert!(db
        .load_serialized(0, Bytes::from_static(b"qwer"))
        .await
        .unwrap()
        .is_none());

    let d = match db.load_serialized(0, Bytes::from_static(b"asdf")).await {
        Ok(x) => x.map(|data| ts.try_deserialize::<KeyPair>(&data).unwrap()),
        Err(e) => {
            panic!("couldn't decode: {}", e);
        }
    };
    assert_eq!(d, Some(keypair.clone()), "keys should be equal");

    let d = match db.delete(0, Bytes::from_static(b"asdf")).await {
        Ok(x) => x.map(|data| {
            ts.try_deserialize::<KeyPair>(&TableDBSerializedData(data))
                .unwrap()
        }),
        Err(e) => {
            panic!("couldn't decode: {}", e);
        }
    };
    assert_eq!(d, Some(keypair.clone()), "keys should be equal");

    assert!(
        db.store(
            1,
            Bytes::from_static(b"foo"),
            Bytes::from_static(b"1234567890")
        )
        .await
        .is_ok(),
        "should store new key"
    );

    let serialized = db
        .load_serialized(1, Bytes::from_static(b"foo"))
        .await
        .unwrap()
        .unwrap();

    assert!(
        ts.try_deserialize::<PublicKey>(&serialized).is_err(),
        "should fail to unfreeze"
    );
}

async fn test_protect_unprotect(vcrypto: &AsyncCryptoSystemGuard<'_>, ts: &TableStore) {
    info!("test_protect_unprotect");

    let dek1 = SharedSecret::new(
        vcrypto.kind(),
        BareSharedSecret::new(&[
            0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
            0, 0, 0,
        ]),
    );

    let dek2 = SharedSecret::new(
        vcrypto.kind(),
        BareSharedSecret::new(&[
            1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
            0, 0, 0xFF,
        ]),
    );

    let dek3 = SharedSecret::new(
        vcrypto.kind(),
        BareSharedSecret::new(&[0x80u8; VLD0_SHARED_SECRET_LENGTH]),
    );

    let deks = [dek1, dek2, dek3];
    let passwords = [
        "",
        " ",
        "  ",
        "12345678",
        "|/\\!@#$%^&*()_+",
        "Ⓜ️",
        "🔥🔥♾️",
    ];

    for dek in deks {
        for password in passwords {
            info!("testing dek {} with password {}", dek, password);
            let dek_bytes = ts
                .maybe_protect_device_encryption_key(dek.clone(), password)
                .await
                .unwrap_or_else(|_| panic!("protect: dek: '{}' pw: '{}'", dek, password));

            let unprotected = ts
                .maybe_unprotect_device_encryption_key(&dek_bytes, password)
                .await
                .unwrap_or_else(|_| panic!("unprotect: dek: '{}' pw: '{}'", dek, password));
            assert_eq!(unprotected, dek);
            let invalid_password = format!("{}x", password);
            let _ = ts
                .maybe_unprotect_device_encryption_key(&dek_bytes, &invalid_password)
                .await
                .expect_err(&format!(
                    "invalid_password: dek: '{}' pw: '{}'",
                    dek, invalid_password
                ));
            if !password.is_empty() {
                let _ = ts
                    .maybe_unprotect_device_encryption_key(&dek_bytes, "")
                    .await
                    .expect_err(&format!("empty_password: dek: '{}' pw: ''", dek));
            }
        }
    }
}

async fn test_store_load_serialized_many(ts: &TableStore) {
    info!("test_json");

    let _ = ts.delete("test").await;
    let db = ts.open("test", 3).await.expect("should have opened");

    let rows = 16;
    let valuesize = 32768;
    let parallel = 10;

    let value = vec!["ABCD".to_string(); valuesize];

    let mut unord = FuturesUnordered::new();

    let mut r = 0;
    let start_ts = Timestamp::now();
    let mut keys = HashSet::new();
    loop {
        while r < rows && unord.len() < parallel {
            let key = format!("key_{}", r);
            r += 1;

            unord.push(Box::pin(async {
                let key = key;

                let serialized = serialize_table_value(&value);
                db.store_serialized(0, Bytes::from(key.clone()), serialized)
                    .await
                    .expect("should store");
                let value2: Vec<String> = ts
                    .try_deserialize(
                        &db.load_serialized(0, Bytes::from(key.clone()))
                            .await
                            .expect("should load")
                            .expect("should exist"),
                    )
                    .expect("deserialize must succeed");
                assert_eq!(value, value2);

                Bytes::copy_from_slice(key.as_bytes())
            }));
        }
        if let Some(res) = unord.next().await {
            keys.insert(res);
        } else {
            break;
        }
    }

    let stored_keys = db.get_keys(0).await.expect("should get keys");
    let stored_keys_set = stored_keys.into_iter().collect::<HashSet<_>>();
    assert_eq!(stored_keys_set, keys, "should have same keys");

    let end_ts = Timestamp::now();
    trace!(
        "test_store_load_serialized_many duration={}",
        end_ts.duration_since(start_ts)
    );
}

async fn test_open_zero_columns(ts: &TableStore) {
    info!("test_open_zero_columns");

    let _ = ts.delete("zero_cols").await;

    assert!(
        ts.open("zero_cols", 0).await.is_err(),
        "zero columns should be rejected"
    );

    let db = ts.open("zero_cols", 3).await.expect("should have opened");
    assert_eq!(db.get_column_count().unwrap(), 3);
    assert!(db
        .store(2, Bytes::from_static(b"k2"), Bytes::from_static(b"v2"))
        .await
        .is_ok());
    drop(db);

    assert!(
        ts.open("zero_cols", 0).await.is_err(),
        "zero columns should be rejected even when table exists"
    );

    let _ = ts.delete("zero_cols").await;
}

async fn test_maybe_encrypt_decrypt(ts: &TableStore) {
    info!("test_maybe_encrypt_decrypt");

    let datas: &[&[u8]] = &[b"", b" ", b"  ", b"12345678", b"|/\\!@#$%^&*()_+",
        b"\xE2\x93\x82\xEF\xB8\x8F\xEF\xB8\x8F", b"\xF0\xEF\xB8\x8F\x9F\x94\xA5\xF0\x9F\x94\xA5\xE2\x99\xBE\xEF\xB8\x8F",
        b"1234567890", b"ABCDEFGHIJKLMNOPQRSTUVWXYZ", b"ABCDEFGHIJKLMNOPQRSTUVWXYZABCDEFGHIJKLMNOPQRSTUVWXYZ", 
        b"\xFF\xFF\xFF\xFF\xFF\xFF\xFF\xFF\xFF\xFF\xFF\xFF\xFF\xFF\xFF\xFF\xFF\xFF\xFF\xFF\xFF\xFF\xFF\xFF\xFF\xFF\xFF\xFF\xFF\xFF\xFF\xFF",
        b"\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00",
        b"\x02\x02\x02\x02\x02\x02\x02\x02\x02\x02\x02\x02\x02\x02\x02\x02\x02\x02\x02\x02\x02\x02\x02\x02\x02\x02\x02\x02\x02\x02\x02\x02",
        b"\x03\x03\x03\x03\x03\x03\x03\x03\x03\x03\x03\x03\x03\x03\x03\x03\x03\x03\x03\x03\x03\x03\x03\x03\x03\x03\x03\x03\x03\x03\x03\x03",
        b"\x04\x04\x04\x04\x04\x04\x04\x04\x04\x04\x04\x04\x04\x04\x04\x04\x04\x04\x04\x04\x04\x04\x04\x04\x04\x04\x04\x04\x04\x04\x04\x04",
        b"\x05\x05\x05\x05\x05\x05\x05\x05\x05\x05\x05\x05\x05\x05\x05\x05\x05\x05\x05\x05\x05\x05\x05\x05\x05\x05\x05\x05\x05\x05\x05\x05",
        b"\x06\x06\x06\x06\x06\x06\x06\x06\x06\x06\x06\x06\x06\x06\x06\x06\x06\x06\x06\x06\x06\x06\x06\x06\x06\x06\x06\x06\x06\x06\x06\x06",
        b"\x07\x07\x07\x07\x07\x07\x07\x07\x07\x07\x07\x07\x07\x07\x07\x07\x07\x07\x07\x07\x07\x07\x07\x07\x07\x07\x07\x07\x07\x07\x07\x07",
        b"\x08\x08\x08\x08\x08\x08\x08\x08\x08\x08\x08\x08\x08\x08\x08\x08\x08\x08\x08\x08\x08\x08\x08\x08\x08\x08\x08\x08\x08\x08\x08\x08",
        b"\x09\x09\x09\x09\x09\x09\x09\x09\x09\x09\x09\x09\x09\x09\x09\x09\x09\x09\x09\x09\x09\x09\x09\x09\x09\x09\x09\x09\x09\x09\x09\x09",
        b"\x0A\x0A\x0A\x0A\x0A\x0A\x0A\x0A\x0A\x0A\x0A\x0A\x0A\x0A\x0A\x0A\x0A\x0A\x0A\x0A\x0A\x0A\x0A\x0A\x0A\x0A\x0A\x0A\x0A\x0A\x0A\x0A\
\x0B\x0B\x0B\x0B\x0B\x0B\x0B\x0B\x0B\x0B\x0B\x0B\x0B\x0B\x0B\x0B\x0B\x0B\x0B\x0B\x0B\x0B\x0B\x0B\x0B\x0B\x0B\x0B\x0B\x0B\x0B\x0B\
\x0C\x0C\x0C\x0C\x0C\x0C\x0C\x0C\x0C\x0C\x0C\x0C\x0C\x0C\x0C\x0C\x0C\x0C\x0C\x0C\x0C\x0C\x0C\x0C\x0C\x0C\x0C\x0C\x0C\x0C\x0C\x0C\
\x0D\x0D\x0D\x0D\x0D\x0D\x0D\x0D\x0D\x0D\x0D\x0D\x0D\x0D\x0D\x0D\x0D\x0D\x0D\x0D\x0D\x0D\x0D\x0D\x0D\x0D\x0D\x0D\x0D\x0D\x0D\x0D\
\x0E\x0E\x0E\x0E\x0E\x0E\x0E\x0E\x0E\x0E\x0E\x0E\x0E\x0E\x0E\x0E\x0E\x0E\x0E\x0E\x0E\x0E\x0E\x0E\x0E\x0E\x0E\x0E\x0E\x0E\x0E\x0E\
\x0F\x0F\x0F\x0F\x0F\x0F\x0F\x0F\x0F\x0F\x0F\x0F\x0F\x0F\x0F\x0F\x0F\x0F\x0F\x0F\x0F\x0F\x0F\x0F\x0F\x0F\x0F\x0F\x0F\x0F\x0F\x0F\
\x10\x10\x10\x10\x10\x10\x10\x10\x10\x10\x10\x10\x10\x10\x10\x10\x10\x10\x10\x10\x10\x10\x10\x10\x10\x10\x10\x10\x10\x10\x10\x10\
",
    ];

    let crypto = ts.crypto();
    let sk = Some(crypto.best().random_shared_secret());

    for data in datas.iter().copied() {
        let encrypted = maybe_encrypt(&crypto, &sk, Bytes::copy_from_slice(data), true).await;
        let decrypted = maybe_decrypt(&crypto, &sk, encrypted).await.unwrap();
        assert_eq!(data, &decrypted);
        let encrypted = maybe_encrypt(&crypto, &sk, Bytes::copy_from_slice(data), false).await;
        let decrypted = maybe_decrypt(&crypto, &sk, encrypted).await.unwrap();
        assert_eq!(data, &decrypted);
    }
}

/// The per-namespace instance lock refuses a second live core on the same
/// namespace (in-process models the two-dylib hot restart case), allows a
/// different namespace in the same directory, and a lock file left behind by
/// a dead process never blocks a fresh start.
/// Native only: the wasm lock is a no-op stub (no advisory file locks).
#[cfg(not(all(target_arch = "wasm32", target_os = "unknown")))]
async fn test_namespace_lock() {
    info!("test_namespace_lock");

    // The fixture isolates table store directories per startup; the lock is
    // per-directory-per-namespace, so pin one shared directory here
    let (update_callback, mut config) = fixture_veilid_core_with_namespace("lock_test");
    let shared_dir = format!("{}_ns_lock_shared", config.table_store.directory);
    config.table_store.directory = shared_dir.clone();
    let api = api_startup(update_callback, config)
        .await
        .expect("first instance should start");

    let (update_callback2, mut config2) = fixture_veilid_core_with_namespace("lock_test");
    config2.table_store.directory = shared_dir.clone();
    assert!(
        api_startup(update_callback2, config2).await.is_err(),
        "second instance on the same namespace should be refused"
    );

    let (update_callback3, mut config3) = fixture_veilid_core_with_namespace("lock_test_other");
    config3.table_store.directory = shared_dir.clone();
    let api3 = api_startup(update_callback3, config3)
        .await
        .expect("a different namespace should share the directory");
    api3.shutdown().await;

    api.shutdown().await;

    // The lock file deliberately survives shutdown (and crashes); only the
    // kernel lock matters, so a stale file must not block a fresh start
    let (update_callback4, mut config4) = fixture_veilid_core_with_namespace("lock_test");
    config4.table_store.directory = shared_dir;
    let api4 = api_startup(update_callback4, config4)
        .await
        .expect("stale lock file should not block startup");
    api4.shutdown().await;
}

/// protected_store.delete=true, table must be wiped using delete_all_in_namespace()
/// upon opening with wrong key
async fn test_protected_store_delete_wipes_tables() {
    info!("test_protected_store_delete_wipes_tables");

    let (update_callback, config_a) = fixture_veilid_core();
    let api1 = api_startup(update_callback.clone(), config_a.clone())
        .await
        .expect("startup 1 failed");
    {
        let ts = api1.table_store().unwrap();
        let db = ts.open("wipe_test", 1).await.expect("open 1 failed");
        db.store(
            0,
            Bytes::from_static(b"persist_me"),
            Bytes::from_static(b"hello"),
        )
        .await
        .expect("store failed");
        assert_eq!(
            db.load(0, Bytes::from_static(b"persist_me")).await.unwrap(),
            Some(Bytes::copy_from_slice(b"hello"))
        );
        drop(db);
    }
    api1.shutdown().await;

    // Same paths; wipe protected store, keep table store files.
    let config_b = VeilidConfig {
        protected_store: VeilidConfigProtectedStore {
            delete: true,
            ..config_a.protected_store.clone()
        },
        table_store: VeilidConfigTableStore {
            delete: false,
            ..config_a.table_store.clone()
        },
        ..config_a
    };
    let api2 = api_startup(update_callback, config_b)
        .await
        .expect("startup 2 failed");
    {
        let ts = api2.table_store().unwrap();
        let db = ts.open("wipe_test", 1).await.expect("open 2 failed");
        assert_eq!(
            db.load(0, Bytes::from_static(b"persist_me")).await.unwrap(),
            None,
            "table should be wiped after protected_store.delete cascade"
        );
        drop(db);
    }
    api2.shutdown().await;
}

/// With wipe_on_invalid_device_encryption_key=false, api_startup must
/// surface NotInitialized when an internal table fails to decrypt, instead
/// of silently wiping the user's data.
async fn test_protected_store_delete_no_wipe_returns_not_initialized() {
    info!("test_protected_store_delete_no_wipe_returns_not_initialized");

    let (update_callback, config_a) = fixture_veilid_core();
    let api1 = api_startup(update_callback.clone(), config_a.clone())
        .await
        .expect("startup 1 failed");
    {
        let ts = api1.table_store().unwrap();
        let db = ts.open("nowipe_test", 1).await.expect("open 1 failed");
        db.store(
            0,
            Bytes::from_static(b"persist_me"),
            Bytes::from_static(b"hello"),
        )
        .await
        .expect("store failed");
        drop(db);
    }
    api1.shutdown().await;

    let config_b = VeilidConfig {
        protected_store: VeilidConfigProtectedStore {
            delete: true,
            ..config_a.protected_store.clone()
        },
        table_store: VeilidConfigTableStore {
            delete: false,
            wipe_on_invalid_device_encryption_key: false,
            ..config_a.table_store.clone()
        },
        ..config_a
    };
    let err = api_startup(update_callback, config_b)
        .await
        .expect_err("startup 2 should fail with NotInitialized");
    assert!(
        matches!(err, VeilidAPIError::NotInitialized),
        "expected NotInitialized, got {err:?}"
    );
}

async fn test_rename(ts: &TableStore) {
    info!("test_rename");

    let _ = ts.delete("test").await;
    let _ = ts.delete("test2").await;

    let db = ts.open("test", 3).await.expect("should have opened");
    assert!(
        db.store(
            0,
            Bytes::from_static(b"foo"),
            Bytes::from_static(b"1234567890")
        )
        .await
        .is_ok(),
        "should store new key"
    );
    drop(db);

    assert!(
        ts.rename("test", "test2").await.is_ok(),
        "should rename to an unused name"
    );

    let db = ts.open("test2", 3).await.expect("should have opened");
    assert_eq!(
        db.load(0, Bytes::from_static(b"foo")).await.unwrap(),
        Some(b"1234567890".to_vec().into()),
        "contents should follow the rename"
    );
    drop(db);

    assert!(
        ts.rename("test", "test3").await.is_err(),
        "should fail because the old name no longer exists"
    );

    let db = ts.open("test", 3).await.expect("should have opened");
    drop(db);
    assert!(
        ts.rename("test", "test2").await.is_err(),
        "should fail because the new name already exists"
    );

    let _ = ts.delete("test").await;
    let _ = ts.delete("test2").await;
}

/// A row as a v0 store kept it: LZ4 size-prepended, then encrypted under the store's key
async fn v0_row(ts: &TableStore, key: &[u8], value: &[u8]) -> (Bytes, Bytes) {
    let crypto = ts.crypto();
    let dek = ts.inner.lock().encryption_key.clone();
    (
        maybe_encrypt(&crypto, &dek, Bytes::from(compress_prepend_size(key)), true).await,
        maybe_encrypt(
            &crypto,
            &dek,
            Bytes::from(compress_prepend_size(value)),
            false,
        )
        .await,
    )
}

/// A row in the v1 encoding
async fn v1_row(ts: &TableStore, key: &[u8], value: &[u8]) -> (Bytes, Bytes) {
    let crypto = ts.crypto();
    let dek = ts.inner.lock().encryption_key.clone();
    (
        maybe_encrypt(&crypto, &dek, Bytes::copy_from_slice(key), true).await,
        maybe_encrypt(&crypto, &dek, Bytes::copy_from_slice(value), false).await,
    )
}

async fn put_row(db: &Database, col: u32, (key, value): (Bytes, Bytes)) {
    let mut tx = DBTransaction::with_capacity(1);
    tx.put(col, key, value);
    db.write(tx).await.expect("row");
}

/// Register a table name and open its file directly, bypassing the schema bookkeeping
async fn open_raw_table(ts: &TableStore, name: &str, column_count: u32) -> Database {
    let file_name = ts.name_get_or_create(name).expect("name");
    ts.flush().await;
    ts.table_store_driver
        .open(&file_name, column_count, 1)
        .await
        .expect("raw open")
}

/// Remove the schema keys so a table looks like a v0 store wrote it
async fn strip_schema_keys(db: &Database) {
    let mut tx = DBTransaction::with_capacity(2);
    tx.delete(0, VERSION_KEY);
    tx.delete(0, ENCRYPTION_KEY_HASH_KEY);
    db.write(tx).await.expect("strip schema keys");
}

/// Remove the store-level version so the next start migrates; the core must be shut down
async fn unversion_store(driver: &TableStoreDriver) {
    let all_tables_db = driver
        .open("__veilid_all_tables", 1, 1)
        .await
        .expect("all tables db");
    let mut tx = DBTransaction::with_capacity(1);
    tx.delete(0, VERSION_KEY);
    all_tables_db.write(tx).await.expect("unversion store");
}

/// A second start on the same store, keeping its files and keys
fn restart_config(config: &VeilidConfig) -> VeilidConfig {
    VeilidConfig {
        protected_store: VeilidConfigProtectedStore {
            delete: false,
            ..config.protected_store.clone()
        },
        table_store: VeilidConfigTableStore {
            delete: false,
            ..config.table_store.clone()
        },
        ..config.clone()
    }
}

/// App rows survive the v1 migration byte for byte in every column, a core table's bare JSON
/// still reads back, the routing table is dropped, and a migrated store is left alone
async fn test_migration_v1() {
    info!("test_migration_v1");

    // Values an app could have stored, including ones that look like JSON or like a header
    let app_rows: &[(u32, &[u8], &[u8])] = &[
        (0, b"json", br#"{"a":1,"b":[true,null]}"#),
        (0, b"number", b"42"),
        (0, b"string", b"\"quoted\""),
        (0, b"empty", b""),
        (0, b"binary", &[0, 1, 2, 255, 254, 0]),
        (0, b"lz4c-prefixed", b"LZ4C not compressed at all"),
        (0, b"cbor-prefixed", b"CBOR not cbor at all"),
        (1, b"col1", b"second column"),
        (2, b"col2", b"third column"),
    ];
    let big = vec![7u8; 300_000];
    let core_value = serialize_json_bytes(vec!["one".to_string(), "two".to_string()]);

    let (update_callback, config_a) = fixture_veilid_core();
    let api1 = api_startup(update_callback.clone(), config_a.clone())
        .await
        .expect("startup 1 failed");
    let (registry, routing_table_file, routing_table_row) = {
        let ts = api1.table_store().unwrap();
        let app_db = open_raw_table(&ts, "migrate_app", 3).await;
        for (col, key, value) in app_rows {
            put_row(&app_db, *col, v0_row(&ts, key, value).await).await;
        }
        put_row(&app_db, 0, v0_row(&ts, b"big", &big).await).await;

        let core_db = open_raw_table(&ts, "migrate_core", 1).await;
        put_row(&core_db, 0, v0_row(&ts, b"content", &core_value).await).await;

        // The routing table is written by the core until it shuts down
        (
            ts.registry(),
            ts.name_get_or_create("routing_table").expect("name"),
            v0_row(&ts, b"v0_row", b"[[1,2,3]]").await,
        )
    };
    api1.shutdown().await;
    {
        let driver = TableStoreDriver::new(registry);
        let rt_db = driver.open(&routing_table_file, 1, 1).await.expect("open");
        strip_schema_keys(&rt_db).await;
        put_row(&rt_db, 0, routing_table_row).await;
        drop(rt_db);
        unversion_store(&driver).await;
    }

    let config_b = restart_config(&config_a);
    let api2 = api_startup(update_callback.clone(), config_b.clone())
        .await
        .expect("startup 2 failed");
    {
        let ts = api2.table_store().unwrap();
        let app_db = ts.open("migrate_app", 3).await.expect("open app");
        for (col, key, value) in app_rows {
            assert_eq!(
                app_db
                    .load(*col, Bytes::copy_from_slice(key))
                    .await
                    .unwrap()
                    .as_deref(),
                Some(*value),
                "{}",
                String::from_utf8_lossy(key)
            );
        }
        assert_eq!(
            app_db
                .load(0, Bytes::from_static(b"big"))
                .await
                .unwrap()
                .as_deref(),
            Some(big.as_slice())
        );
        let mut keys = app_db.get_keys(0).await.unwrap();
        keys.sort();
        let mut expected: Vec<Bytes> = app_rows
            .iter()
            .filter(|(col, _, _)| *col == 0)
            .map(|(_, key, _)| Bytes::copy_from_slice(key))
            .chain([Bytes::copy_from_slice(b"big")])
            .collect();
        expected.sort();
        assert_eq!(keys, expected);
        assert_eq!(
            app_db.get_key_count(0).await.unwrap(),
            expected.len() as u64
        );
        assert_eq!(app_db.get_key_count(1).await.unwrap(), 1);
        assert_eq!(app_db.get_key_count(2).await.unwrap(), 1);

        let core_db = ts.open("migrate_core", 1).await.expect("open core");
        let serialized = core_db
            .load_serialized(0, Bytes::from_static(b"content"))
            .await
            .unwrap()
            .expect("core row");
        let value: Vec<String> = ts.try_deserialize(&serialized).expect("bare json reads");
        assert_eq!(value, ["one", "two"]);

        let rt_db = ts
            .open(
                "routing_table",
                crate::routing_table::ROUTING_TABLE_COLUMN_COUNT,
            )
            .await
            .expect("open routing table");
        assert_eq!(
            rt_db.load(0, Bytes::from_static(b"v0_row")).await.unwrap(),
            None
        );
    }
    api2.shutdown().await;

    let api3 = api_startup(update_callback, config_b)
        .await
        .expect("startup 3 failed");
    {
        let ts = api3.table_store().unwrap();
        let dek = ts.inner.lock().encryption_key.clone();
        assert!(
            !migration::v1::V1::is_needed(&ts, &dek).await.unwrap(),
            "a migrated store needs no migration"
        );
        let app_db = ts.open("migrate_app", 3).await.expect("open app");
        for (col, key, value) in app_rows {
            assert_eq!(
                app_db
                    .load(*col, Bytes::copy_from_slice(key))
                    .await
                    .unwrap()
                    .as_deref(),
                Some(*value)
            );
        }
    }
    api3.shutdown().await;
}

/// An interrupted migration finishes each table from its progress record, whether the record
/// names only the rows left or every row the table began with, and a table already at v1 is not
/// touched
async fn test_migration_v1_resumes() {
    info!("test_migration_v1_resumes");

    let rows: &[(&[u8], &[u8])] = &[
        (b"k1", b"v1 value"),
        (b"k2", b"v2 value"),
        (b"k3", b"v3 value"),
        (b"k4", b"v4 value"),
    ];
    let looks_compressed = compress_prepend_size(b"x");

    let (update_callback, config_a) = fixture_veilid_core();
    let api1 = api_startup(update_callback.clone(), config_a.clone())
        .await
        .expect("startup 1 failed");
    let registry = {
        let ts = api1.table_store().unwrap();

        // Half migrated: k1 and k2 already at v1, k3 and k4 named in the checkpoint
        let db = open_raw_table(&ts, "migrate_resume", 1).await;
        for (key, value) in &rows[..2] {
            put_row(&db, 0, v1_row(&ts, key, value).await).await;
        }
        let mut remaining = Vec::new();
        for (key, value) in &rows[2..] {
            let row = v0_row(&ts, key, value).await;
            remaining.push((0u32, row.0.to_vec()));
            put_row(&db, 0, row).await;
        }
        let progress = serde_json::to_vec(&serde_json::json!({ "remaining": remaining })).unwrap();
        put_row(
            &db,
            0,
            (
                Bytes::from_static(MIGRATION_PROGRESS_KEY),
                Bytes::from(progress),
            ),
        )
        .await;

        // The same, with a record that still names k1 and k2, whose v0 rows are gone
        let named_db = open_raw_table(&ts, "migrate_resume_named", 1).await;
        let mut began_with = Vec::new();
        for (i, (key, value)) in rows.iter().enumerate() {
            let row = v0_row(&ts, key, value).await;
            began_with.push((0u32, row.0.to_vec()));
            if i < 2 {
                put_row(&named_db, 0, v1_row(&ts, key, value).await).await;
            } else {
                put_row(&named_db, 0, row).await;
            }
        }
        let progress = serde_json::to_vec(&serde_json::json!({ "remaining": began_with })).unwrap();
        put_row(
            &named_db,
            0,
            (
                Bytes::from_static(MIGRATION_PROGRESS_KEY),
                Bytes::from(progress),
            ),
        )
        .await;

        // Finished: carries the version, so a value that would decompress stays as it is
        let done_db = open_raw_table(&ts, "migrate_done", 1).await;
        put_row(&done_db, 0, v1_row(&ts, b"k", &looks_compressed).await).await;
        let crypto = ts.crypto();
        let dek = ts.inner.lock().encryption_key.clone().expect("dek");
        let mut tx = DBTransaction::with_capacity(2);
        tx.put(0, VERSION_KEY, CURRENT_SCHEMA_VERSION);
        tx.put(
            0,
            ENCRYPTION_KEY_HASH_KEY,
            hash_table_encryption_key(&crypto, &dek).await,
        );
        done_db.write(tx).await.expect("version");

        ts.registry()
    };
    api1.shutdown().await;
    unversion_store(&TableStoreDriver::new(registry)).await;

    let api2 = api_startup(update_callback, restart_config(&config_a))
        .await
        .expect("startup 2 failed");
    {
        let ts = api2.table_store().unwrap();
        for table in ["migrate_resume", "migrate_resume_named"] {
            let db = ts.open(table, 1).await.expect("open resume");
            for (key, value) in rows {
                assert_eq!(
                    db.load(0, Bytes::copy_from_slice(key))
                        .await
                        .unwrap()
                        .as_deref(),
                    Some(*value),
                    "{} {}",
                    table,
                    String::from_utf8_lossy(key)
                );
            }
            assert_eq!(db.get_key_count(0).await.unwrap(), rows.len() as u64);
        }

        let done_db = ts.open("migrate_done", 1).await.expect("open done");
        assert_eq!(
            done_db
                .load(0, Bytes::from_static(b"k"))
                .await
                .unwrap()
                .as_deref(),
            Some(looks_compressed.as_slice())
        );
    }
    api2.shutdown().await;
}

/// A v0 store the device encryption key cannot read follows the wipe policy
async fn test_migration_v1_wrong_key() {
    info!("test_migration_v1_wrong_key");

    for wipe in [true, false] {
        let (update_callback, config_a) = fixture_veilid_core();
        let api1 = api_startup(update_callback.clone(), config_a.clone())
            .await
            .expect("startup 1 failed");
        let registry = {
            let ts = api1.table_store().unwrap();
            let db = open_raw_table(&ts, "migrate_wrong_key", 1).await;
            put_row(&db, 0, v0_row(&ts, b"k", b"v").await).await;
            ts.registry()
        };
        api1.shutdown().await;
        unversion_store(&TableStoreDriver::new(registry)).await;

        // Wipe the protected store: a new device encryption key is minted
        let config_b = VeilidConfig {
            protected_store: VeilidConfigProtectedStore {
                delete: true,
                ..config_a.protected_store.clone()
            },
            table_store: VeilidConfigTableStore {
                delete: false,
                wipe_on_invalid_device_encryption_key: wipe,
                ..config_a.table_store.clone()
            },
            ..config_a
        };
        if wipe {
            let api2 = api_startup(update_callback, config_b)
                .await
                .expect("startup 2 should wipe and start");
            {
                let ts = api2.table_store().unwrap();
                let db = ts.open("migrate_wrong_key", 1).await.expect("open");
                assert_eq!(
                    db.load(0, Bytes::from_static(b"k")).await.unwrap(),
                    None,
                    "store was wiped"
                );
            }
            api2.shutdown().await;
        } else {
            let err = api_startup(update_callback, config_b)
                .await
                .expect_err("startup 2 should refuse");
            assert!(
                matches!(err, VeilidAPIError::NotInitialized),
                "expected NotInitialized, got {err:?}"
            );
        }
    }
}

/// Internal keys are not counted, in any column
async fn test_key_count(ts: &TableStore) {
    info!("test_key_count");

    let _ = ts.delete("test").await;
    let db = ts.open("test", 3).await.expect("should have opened");
    assert_eq!(db.get_key_count(0).await.unwrap(), 0);
    assert_eq!(db.get_key_count(1).await.unwrap(), 0);
    db.store(0, Bytes::from_static(b"a"), Bytes::from_static(b"1"))
        .await
        .unwrap();
    db.store(0, Bytes::from_static(b"b"), Bytes::from_static(b"2"))
        .await
        .unwrap();
    db.store(1, Bytes::from_static(b"c"), Bytes::from_static(b"3"))
        .await
        .unwrap();
    assert_eq!(db.get_key_count(0).await.unwrap(), 2);
    assert_eq!(db.get_key_count(1).await.unwrap(), 1);
    assert_eq!(db.get_key_count(2).await.unwrap(), 0);
    drop(db);
    let _ = ts.delete("test").await;
}

/// Every form `try_deserialize` reads, and the forms it refuses
fn test_serialized_forms(ts: &TableStore) {
    info!("test_serialized_forms");

    let small = vec!["a".to_string()];
    let serialized = serialize_table_value(&small);
    assert!(serialized.0.starts_with(b"CBOR"));
    assert_eq!(
        ts.try_deserialize::<Vec<String>>(&serialized).unwrap(),
        small
    );

    let big = vec!["abcdefgh".to_string(); 4096];
    let serialized = serialize_table_value(&big);
    assert!(serialized.0.starts_with(b"LZ4C"));
    assert_eq!(ts.try_deserialize::<Vec<String>>(&serialized).unwrap(), big);

    let bare_json = TableDBSerializedData(Bytes::from(serialize_json_bytes(&small)));
    assert_eq!(
        ts.try_deserialize::<Vec<String>>(&bare_json).unwrap(),
        small
    );

    for garbage in [
        &b""[..],
        b"LZ4C",
        b"LZ4Cjunk",
        b"CBOR\xff",
        b"\x00\x01",
        b"{not json",
    ] {
        assert!(
            ts.try_deserialize::<Vec<String>>(&TableDBSerializedData(Bytes::copy_from_slice(
                garbage
            )))
            .is_err(),
            "{garbage:?}"
        );
    }
}

pub async fn test_all() {
    let api = startup().await;
    let crypto = api.crypto().unwrap();
    let ts = api.table_store().unwrap();

    test_store_load_serialized_many(&ts).await;
    test_maybe_encrypt_decrypt(&ts).await;
    test_key_count(&ts).await;
    test_serialized_forms(&ts);

    for ck in VALID_CRYPTO_KINDS {
        let vcrypto = crypto.get_async(ck).unwrap();
        test_protect_unprotect(&vcrypto, &ts).await;
        test_delete_open_delete(&ts).await;
        test_open_zero_columns(&ts).await;
        test_store_delete_load(&ts).await;
        test_transaction(&ts).await;
        test_json(&vcrypto, &ts).await;
        test_rename(&ts).await;
        let _ = ts.delete("test").await;
    }

    shutdown(api).await;

    #[cfg(not(all(target_arch = "wasm32", target_os = "unknown")))]
    test_namespace_lock().await;
    test_protected_store_delete_wipes_tables().await;
    test_protected_store_delete_no_wipe_returns_not_initialized().await;
    test_migration_v1().await;
    test_migration_v1_resumes().await;
    test_migration_v1_wrong_key().await;
}
