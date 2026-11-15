//! The route spec store content comes back from the table store as it was saved.

pub mod test_remote_routes;

use super::*;
use crate::crypto::tests_crypto::*;
use crate::tests::*;

pub async fn test_persisted_route_spec_store_content() {
    info!("--- test_persisted_route_spec_store_content ---");
    let registry = mock_registry::init("a").await;
    let table_store = registry.table_store();

    let id = AllocatedRouteSetId::from(fake_route_id());
    let key = fake_public_key();
    let secret_key = fake_secret_key();
    let hops = vec![fake_node_id(), fake_node_id()];
    let route_set = BTreeMap::from([(
        key.clone(),
        RouteSpecDetail {
            secret_key: secret_key.clone(),
            hops: hops.clone(),
        },
    )]);
    let detail = RouteSetSpecDetail::try_new(
        route_set,
        DirectionSet::all(),
        Stability::Reliable,
        SequenceOrderingSet::all(),
        true,
    )
    .expect("detail");

    let mut content = RouteSpecStoreContent::new(registry.clone());
    content.add_detail(id.clone(), detail);
    let db = table_store
        .open("RouteSpecStore", 1)
        .await
        .expect("route spec store table");
    content.save(&db).await.expect("save");

    let loaded = RouteSpecStoreContent::load(registry.clone())
        .await
        .expect("load");
    let loaded_detail = loaded.get_detail(&id).expect("detail survives");
    let mut loaded_routes = loaded_detail.iter_route_set();
    let (loaded_key, loaded_route) = loaded_routes.next().expect("one route");
    assert_eq!(loaded_key, &key);
    assert_eq!(loaded_route.secret_key, secret_key);
    assert_eq!(loaded_route.hops, hops);
    assert!(loaded_routes.next().is_none());
    assert_eq!(loaded.get_id_by_key(&key), Some(id));

    mock_registry::terminate(registry).await;
}

/// A passed route test survives a restart; the session's tallies and keepalive clock do not
pub async fn test_route_test_state_survives_restart() {
    info!("--- test_route_test_state_survives_restart ---");
    use crate::routing_table::tests_routing_table::*;

    let identity = fix_identity();
    let configure = |config: &mut VeilidConfig| {
        config.network.routing_table.bootstrap = vec![];
        set_identity(&mut config.network.routing_table, &identity);
    };
    let (registry, storage) = mock_registry::init_with_storage("rss_test_state", configure).await;
    // A route keyed for the crypto kind its hops have
    let (route_keys, route_secrets) = fix_identity();
    let key = route_keys[0].clone();
    let tested_ts = Timestamp::now();
    {
        let routing_table = registry.routing_table();
        let hop_node_refs = seed_nodes(&routing_table, 2);
        let route_set = BTreeMap::from([(
            key.clone(),
            RouteSpecDetail {
                secret_key: route_secrets.get(key.kind()).expect("route secret"),
                hops: hop_node_refs.iter().map(|nr| nr.best_node_id()).collect(),
            },
        )]);
        let detail = RouteSetSpecDetail::try_new(
            route_set,
            DirectionSet::all(),
            Stability::Reliable,
            SequenceOrderingSet::only(SequenceOrdering::Ordered),
            true,
        )
        .expect("detail");
        let id = AllocatedRouteSetId::from(fake_route_id());
        let rss = routing_table.route_spec_store();
        {
            let mut content = rss.content.write();
            let mut cache = rss.cache.write();
            cache
                .add_allocated_route(id.clone(), &detail, hop_node_refs)
                .expect("route");
            content.add_detail(id, detail);
        }
        rss.update_allocated_route_stats(tested_ts, &key, |s| {
            let _ = s.record_loopback_result(SequenceOrdering::Ordered, true, 1.0, tested_ts);
            s.record_loopback_keepalive(tested_ts, SequenceOrdering::Ordered);
            Ok(())
        });
        routing_table.save().await.expect("should save entries");
        rss.save().await.expect("should save routes");
    }
    mock_registry::terminate(registry).await;

    let registry = mock_registry::reopen("rss_test_state", storage, configure).await;
    {
        let routing_table = registry.routing_table();
        let rss = routing_table.route_spec_store();
        let stats = {
            let cache = rss.cache.read();
            let id = cache
                .get_allocated_route_id_by_key(&key)
                .expect("the route should load");
            cache
                .get_allocated_route_by_id(&id)
                .expect("the route should load")
                .with_stats(|s| s.clone())
        };
        let ordered = stats
            .per_ordering
            .get(&SequenceOrdering::Ordered)
            .expect("the ordering's test state should load");
        assert_eq!(ordered.last_known_valid_ts, Some(tested_ts));
        assert_eq!(ordered.loopback_pass, 0, "tallies belong to one session");
        assert_eq!(
            ordered.last_loopback_keepalive_ts, None,
            "no flow survives an attach, so the keepalive is due"
        );
    }
    mock_registry::terminate(registry).await;
}
