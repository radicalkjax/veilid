//! The storage metadata comes back from the table store as it was saved.

use super::*;
use crate::crypto::tests_crypto::*;
use crate::tests::*;

pub async fn test_metadata_round_trip() {
    info!("--- test_metadata_round_trip ---");
    let registry = mock_registry::init("a").await;
    let storage_manager = registry.storage_manager();

    let record_key = fake_record_key();
    let opaque_record_key = record_key.opaque();
    let node_id = fake_node_id();
    let offline_write = OfflineSubkeyWrite {
        safety_selection: SafetySelection::Unsafe(Sequencing::EnsureOrdered),
        subkeys: ValueSubkeyRangeSet::single_range(0, 3),
        subkeys_in_flight: ValueSubkeyRangeSet::single(1),
    };
    let rehydration = RehydrationRequest {
        subkeys: ValueSubkeyRangeSet::full(),
        consensus_count: 3,
    };
    let per_node_key = PerNodeKey {
        record_key: record_key.clone(),
        node_id: node_id.clone(),
    };
    let per_node_state = OutboundWatchPerNodeState {
        watch_id: 7,
        safety_selection: SafetySelection::Unsafe(Sequencing::PreferUnordered),
        opt_watcher: Some(KeyPair::new_from_parts(
            fake_public_key(),
            fake_bare_secret_key(),
        )),
        expiration: Timestamp::new(1_700_000_000_000_000),
        count: 5,
        watch_node_ref: None,
        opt_value_changed_route: Some(fake_public_key()),
    };
    let descriptor_cache_key = DescriptorCacheKey {
        opaque_record_key: opaque_record_key.clone(),
        node_id,
    };

    {
        let mut inner = storage_manager.inner.lock();
        inner
            .offline_subkey_writes
            .insert(opaque_record_key.clone(), offline_write.clone());
        inner
            .rehydration_requests
            .insert(opaque_record_key.clone(), rehydration.clone());
    }
    storage_manager
        .outbound_watch_manager
        .lock()
        .per_node_states
        .insert(per_node_key.clone(), per_node_state.clone());
    storage_manager
        .descriptor_cache
        .lock()
        .insert(descriptor_cache_key.clone(), ());

    storage_manager.save_metadata().await.expect("save");

    {
        let mut inner = storage_manager.inner.lock();
        inner.offline_subkey_writes.clear();
        inner.rehydration_requests.clear();
    }
    storage_manager
        .outbound_watch_manager
        .lock()
        .per_node_states
        .clear();
    storage_manager.descriptor_cache.lock().clear();

    storage_manager.load_metadata().await.expect("load");

    {
        let inner = storage_manager.inner.lock();
        let loaded = inner
            .offline_subkey_writes
            .get(&opaque_record_key)
            .expect("offline write");
        assert_eq!(loaded.safety_selection, offline_write.safety_selection);
        assert_eq!(loaded.subkeys, offline_write.subkeys);
        assert_eq!(loaded.subkeys_in_flight, offline_write.subkeys_in_flight);
        assert_eq!(
            inner.rehydration_requests.get(&opaque_record_key),
            Some(&rehydration)
        );
    }
    {
        let owm = storage_manager.outbound_watch_manager.lock();
        let loaded = owm
            .per_node_states
            .get(&per_node_key)
            .expect("per node state");
        assert_eq!(loaded.watch_id, per_node_state.watch_id);
        assert_eq!(loaded.safety_selection, per_node_state.safety_selection);
        assert_eq!(loaded.opt_watcher, per_node_state.opt_watcher);
        assert_eq!(loaded.expiration, per_node_state.expiration);
        assert_eq!(loaded.count, per_node_state.count);
        assert_eq!(
            loaded.opt_value_changed_route,
            per_node_state.opt_value_changed_route
        );
    }
    assert!(storage_manager
        .descriptor_cache
        .lock()
        .contains_key(&descriptor_cache_key));

    mock_registry::terminate(registry).await;
}
