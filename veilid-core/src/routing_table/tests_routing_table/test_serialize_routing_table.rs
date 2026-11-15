use super::*;
use crate::tests::*;

fn add_mock_data(routing_table: &RoutingTable) {
    let pi = fix_peer_info(
        routing_table,
        fix_crypto_info_list(true),
        fix_crypto_info_list_secrets(),
    )
    .expect("should be valid");
    routing_table
        .register_node_with_peer_info(Arc::new(UnverifiedPeerInfo::from_verified(&pi)), false)
        .expect("should register");

    let _ = fix_peer_info(
        routing_table,
        fix_crypto_info_list(false),
        fix_crypto_info_list_secrets(),
    )
    .expect_err("should be invalid");

    let _ = fix_peer_info(
        routing_table,
        fix_crypto_info_list(true),
        SecretKeyGroup::new(),
    )
    .expect_err("should be missing a secret key");

    let pi3 =
        fix_unsigned_peer_info(routing_table, fix_crypto_info_list(true)).expect("should be valid");
    assert!(pi3.signatures().is_empty(), "should have no signatures");

    let _ = routing_table
        .register_node_with_peer_info(Arc::new(UnverifiedPeerInfo::from_verified(&pi3)), false)
        .expect_err("should fail with only no signatures");

    let _ = routing_table
        .register_node_with_peer_info(Arc::new(UnverifiedPeerInfo::from_verified(&pi3)), true)
        .expect("should succeed with allow_invalid");
}

/// Every bucketed entry as persisted, by crypto kind, bucket and node id
fn saved_contents(routing_table: &RoutingTable) -> Vec<(CryptoKind, usize, BareNodeId, Bytes)> {
    let inner = routing_table.inner.read();
    let mut out = Vec::new();
    for (ck, buckets) in inner.buckets.iter() {
        for (n, bucket) in buckets.iter().enumerate() {
            for (node_id, entry) in bucket.entries() {
                out.push((
                    *ck,
                    n,
                    node_id.clone(),
                    // An entry computes its state rather than holding one
                    entry.with(|e| {
                        serialize_table_value(&e.to_persisted(BucketEntryState::Initial)).0
                    }),
                ));
            }
        }
    }
    out
}

fn mock_config(config: &mut VeilidConfig, identity: &(PublicKeyGroup, SecretKeyGroup)) {
    config.network.routing_table.bootstrap = vec![];
    set_identity(&mut config.network.routing_table, identity);
}

pub async fn test_persisted_entries_round_trip() {
    let identity = fix_identity();
    let (registry, storage) =
        mock_registry::init_with_storage("rt_persisted_round_trip", |config| {
            mock_config(config, &identity)
        })
        .await;
    let saved = {
        let routing_table = registry.routing_table();
        add_mock_data(&routing_table);
        let nodes = seed_nodes(&routing_table, 4);

        // Transport evidence is persisted too
        nodes[0].operate_mut(|e| {
            e.lost_question(
                TransportType::new(ProtocolType::TCP, AddressType::IPV4),
                true,
                1.0,
            );
            e.report_contact_method_result(
                &ContactMethod::Direct {
                    target_di: fix_dial_info_detail().dial_info,
                },
                false,
            );
        });
        drop(nodes);

        // An unsigned peer info, as a bootstrap entry has
        let (crypto_info_list, _) = fix_node_keys();
        let unsigned =
            fix_unsigned_peer_info(&routing_table, crypto_info_list).expect("should be valid");
        routing_table
            .register_node_with_peer_info(
                Arc::new(UnverifiedPeerInfo::from_verified(&unsigned)),
                true,
            )
            .expect("should register with allow_invalid");

        saved_contents(&routing_table)
    };
    assert_eq!(saved.len(), 6, "should have entries to save");
    mock_registry::terminate(registry).await;

    let registry = mock_registry::reopen("rt_persisted_round_trip", storage, |config| {
        mock_config(config, &identity)
    })
    .await;
    let loaded = saved_contents(&registry.routing_table());
    mock_registry::terminate(registry).await;

    assert_eq!(
        loaded, saved,
        "a reload should restore every entry as saved"
    );
}

/// A saved entry with no node ids stays out of the buckets, where the first `best_node_id()` panics
pub async fn test_load_rejects_entries_without_node_ids() {
    let identity = fix_identity();
    let (registry, storage) =
        mock_registry::init_with_storage("rt_persisted_no_node_ids", |config| {
            mock_config(config, &identity)
        })
        .await;
    let tampered_key = {
        let routing_table = registry.routing_table();
        seed_nodes(&routing_table, 2);
        routing_table.save().await.expect("should save");

        // Blank out node_ids on one persisted entry
        let tdb = routing_table.table_db().await.expect("should open");
        let key = tdb
            .get_keys(PERSISTED_ENTRIES_COLUMN)
            .await
            .expect("should list entries")
            .remove(0);
        let persisted = tdb
            .load_serialized(PERSISTED_ENTRIES_COLUMN, key.clone())
            .await
            .expect("should read entry")
            .expect("entry should exist");
        let mut v: ciborium::Value = registry.table_store().try_deserialize(&persisted).unwrap();
        if let ciborium::Value::Map(ref mut m) = v {
            for (k, v) in m.iter_mut() {
                if *k == ciborium::Value::Text("node_ids".into()) {
                    *v = ciborium::Value::Array(vec![]);
                }
            }
        };
        tdb.store_serialized(
            PERSISTED_ENTRIES_COLUMN,
            key.clone(),
            serialize_table_value(&v),
        )
        .await
        .expect("should write entry");
        key
    };
    // The terminate save leaves unchanged entries alone, so the blanked one stays
    mock_registry::terminate(registry).await;

    let registry = mock_registry::reopen("rt_persisted_no_node_ids", storage, |config| {
        mock_config(config, &identity)
    })
    .await;
    {
        let routing_table = registry.routing_table();

        // Startup calls best_node_id() on every bucketed entry
        {
            let inner = &*routing_table.inner.read();
            for buckets in inner.buckets.values() {
                for bucket in buckets {
                    for (_k, entry) in bucket.entries() {
                        let _ = entry.with(|e| e.best_node_id());
                    }
                }
            }
            assert_eq!(
                inner.bucket_entry_count(),
                1,
                "the blanked entry must not load"
            );
        }

        // The entry that did not load is deleted by the next save
        let saved = routing_table.save().await.expect("should save");
        assert_eq!((saved.entries, saved.written, saved.deleted), (1, 0, 1));
        let keys = routing_table
            .table_db()
            .await
            .expect("should open")
            .get_keys(PERSISTED_ENTRIES_COLUMN)
            .await
            .expect("should list entries");
        assert!(
            !keys.contains(&tampered_key),
            "the blanked entry should be gone"
        );
    }
    mock_registry::terminate(registry).await;
}

/// A save writes the entries that changed and deletes the entries that left
pub async fn test_persisted_entries_write_only_changes() {
    let registry = mock_registry::init("rt_persisted_changes").await;
    {
        let routing_table = registry.routing_table();
        let (crypto_info_list, secret_keys) = fix_node_keys();
        let first_peer_info = fix_peer_info_at(
            &routing_table,
            crypto_info_list.clone(),
            secret_keys.clone(),
            Timestamp::new(1),
        )
        .expect("should be valid");
        routing_table
            .register_node_with_peer_info(
                Arc::new(UnverifiedPeerInfo::from_verified(&first_peer_info)),
                false,
            )
            .expect("should register");
        let nodes = seed_nodes(&routing_table, 2);

        let first = routing_table.save().await.expect("should save");
        assert_eq!((first.entries, first.written, first.deleted), (3, 3, 0));

        let unchanged = routing_table.save().await.expect("should save");
        assert_eq!(
            (unchanged.written, unchanged.deleted),
            (0, 0),
            "an unchanged table writes nothing"
        );

        // A newer peer info rewrites that entry
        let newer_peer_info = fix_peer_info_at(
            &routing_table,
            crypto_info_list,
            secret_keys,
            Timestamp::new(2),
        )
        .expect("should be valid");
        routing_table
            .register_node_with_peer_info(
                Arc::new(UnverifiedPeerInfo::from_verified(&newer_peer_info)),
                false,
            )
            .expect("should register");
        let peer_info_changed = routing_table.save().await.expect("should save");
        assert_eq!(
            (peer_info_changed.written, peer_info_changed.deleted),
            (1, 0),
            "a newer peer info rewrites one entry"
        );

        // A state change rewrites that entry
        nodes[0].operate_mut(|e| e.unreachable());
        let state_changed = routing_table.save().await.expect("should save");
        assert_eq!(
            (state_changed.written, state_changed.deleted),
            (1, 0),
            "a state change rewrites one entry"
        );

        // An entry that leaves the buckets is deleted
        let dropped = nodes[1].node_ids();
        drop(nodes);
        {
            let inner = &mut *routing_table.inner.write();
            for node_id in dropped.iter() {
                let bucket_index = routing_table
                    .calculate_bucket_index(node_id)
                    .expect("should have a bucket");
                inner
                    .get_bucket_mut(bucket_index)
                    .remove_entry(node_id.ref_value());
            }
        }
        let removed = routing_table.save().await.expect("should save");
        assert_eq!(
            (removed.entries, removed.written, removed.deleted),
            (2, 0, 1),
            "a dropped entry is deleted"
        );
    }
    mock_registry::terminate(registry).await;
}

// REPRO: the lookup/kick race. `lookup_node_id` clones an entry's Arc under the inner
// read lock then bumps ref_count AFTER releasing it; a kick in that window observes
// ref_count==0 and evicts the entry. This reproduces the resulting state directly: an
// entry kicked while unreferenced, then handed to a NodeRef. Pre-fix, kick empties the
// entry and `best_node_id()` panics.
pub async fn test_kick_preserves_best_node_id() {
    let registry = mock_registry::init("a").await;
    {
        let rt = registry.routing_table();
        add_mock_data(&rt);

        // Clone an entry's Arc the way lookup_node_id does (ref_count stays 0), then
        // drop the lock.
        let (kind, bucket_idx, entry) = {
            let inner = rt.inner.read();
            let mut found = None;
            'outer: for (ck, buckets) in inner.buckets.iter() {
                for (i, bucket) in buckets.iter().enumerate() {
                    if let Some((_k, e)) = bucket.entries().next() {
                        found = Some((*ck, i, e.clone()));
                        break 'outer;
                    }
                }
            }
            found.expect("should have a registered entry")
        };

        // The race window: a kick evicts this entry while it is still unreferenced.
        {
            let mut inner = rt.inner.write();
            inner.buckets.get_mut(&kind).unwrap()[bucket_idx]
                .kick(0, &BTreeSet::<BareNodeId>::new());
        }

        // The deferred NodeRef::new now bumps ref_count and the caller uses the node.
        // best_node_id() routes to bucket_entry/mod.rs:370's expect_or_log; pre-fix the
        // kick emptied the entry and this panics there ("all entries must have one
        // valid node id").
        let nr = NodeRef::new(rt.registry(), entry.clone());
        let _ = nr.best_node_id();
    }
    mock_registry::terminate(registry).await;
}

// replace_node_ids must be atomic: a rejected replacement (no valid node id) leaves the
// entry's node ids and best node id unchanged. This is what lets create_node_ref reject a
// peer info update in its entirety when its node ids can't be applied.
pub async fn test_replace_node_ids_rejects_without_valid_id() {
    let registry = mock_registry::init("a").await;
    {
        let rt = registry.routing_table();
        add_mock_data(&rt);

        let entry = {
            let inner = rt.inner.read();
            let mut found = None;
            'outer: for buckets in inner.buckets.values() {
                for bucket in buckets {
                    if let Some((_k, e)) = bucket.entries().next() {
                        found = Some(e.clone());
                        break 'outer;
                    }
                }
            }
            found.expect("should have a registered entry")
        };

        let before_ids = entry.with(|e| e.node_ids());
        let before_best = entry.with(|e| e.best_node_id());

        // Replacing with a set that has no valid node id must be rejected, atomically.
        let result = entry.with_mut(|e| e.replace_node_ids(&[]));
        assert!(result.is_err(), "replace with no valid node id must fail");

        assert_eq!(
            entry.with(|e| e.node_ids()),
            before_ids,
            "node ids must be unchanged on rejection"
        );
        assert_eq!(
            entry.with(|e| e.best_node_id()),
            before_best,
            "best node id must be unchanged on rejection"
        );
    }
    mock_registry::terminate(registry).await;
}

// A node already in the table registers again under the read lock: the same entry with the
// same node ids, while a peer info update still lands on it.
pub async fn test_register_known_node_keeps_entry() {
    let registry = mock_registry::init("a").await;
    {
        let rt = registry.routing_table();
        let nr = seed_nodes(&rt, 1).remove(0);
        let node_ids = nr.node_ids();
        let entry_count = rt.inner.read().bucket_entry_count();
        let pi = nr
            .get_peer_info(RoutingDomain::PublicInternet)
            .expect("should have peer info");

        let fnr = rt
            .register_node_with_peer_info(Arc::new(UnverifiedPeerInfo::from_verified(&pi)), false)
            .expect("should register");
        assert!(
            fnr.same_entry(&nr),
            "a known node must resolve to its entry"
        );
        assert_eq!(
            nr.get_peer_info(RoutingDomain::PublicInternet),
            Some(pi.clone())
        );

        // A sender seen by a single id, which is all of its ids with one crypto kind
        if node_ids.len() == 1 {
            let fnr = rt
                .register_node_without_peer_info(RoutingDomain::PublicInternet, node_ids[0].clone())
                .expect("should register");
            assert!(
                fnr.same_entry(&nr),
                "a known sender must resolve to its entry"
            );
        }

        assert_eq!(nr.node_ids(), node_ids, "node ids must be unchanged");
        assert_eq!(
            rt.inner.read().bucket_entry_count(),
            entry_count,
            "no entry may be added"
        );
    }
    mock_registry::terminate(registry).await;
}

pub async fn test_round_trip_peerinfo() {
    let registry = mock_registry::init("a").await;
    let routing_table = registry.routing_table();

    let pi = fix_peer_info(
        &routing_table,
        fix_crypto_info_list(true),
        fix_crypto_info_list_secrets(),
    )
    .expect("should be valid");

    let s = serialize_json(&pi);
    let pi2 = deserialize_json(&s).expect("Should deserialize");
    let s2 = serialize_json(&pi2);

    assert_eq!(pi, pi2);
    assert_eq!(s, s2);
}
