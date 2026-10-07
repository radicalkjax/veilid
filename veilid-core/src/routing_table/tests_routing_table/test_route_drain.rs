//! A route chosen for use is not removed under its user: a release marks
//! it (no longer selectable) and the last held reference releases it.

use super::*;
use crate::tests::*;

fn one_hop_params(kind: CryptoKind, preferred: Option<AllocatedRouteSetId>) -> RouteSelectParams {
    RouteSelectParams {
        crypto_kind: kind,
        preferred_route: preferred,
        hop_count: 1,
        stability: Stability::default(),
        sequencing: Sequencing::default(),
        directions: Direction::In.into(),
        avoid_nodes: vec![],
        is_destination_safe: true,
    }
}

async fn store_with_one_route() -> (
    VeilidComponentRegistry,
    AllocatedRouteSetId,
    CryptoKind,
) {
    let registry = mock_registry::init("route-drain").await;
    let routing_table = registry.routing_table();
    let pi = fix_peer_info(
        &routing_table,
        fix_crypto_info_list(true),
        fix_crypto_info_list_secrets(),
    )
    .expect("peer info");
    let hop = routing_table
        .register_node_with_peer_info(Arc::new(pi), false)
        .expect("register")
        .unfiltered();
    let (id, key) = routing_table
        .route_spec_store()
        .test_insert_allocated_route(hop)
        .await;
    (registry, id, key.kind())
}

pub async fn test_held_route_drains_on_release() {
    let (registry, id, kind) = store_with_one_route().await;
    {
        let routing_table = registry.routing_table();
        let rss = routing_table.route_spec_store();
        let selected = rss
            .select_single_route(one_hop_params(kind, None))
            .await
            .expect("selects the route");
        assert_eq!(selected.route_id(), &id);

        // Released while held: still there, marked, out of selection.
        assert!(rss.test_release(&id));
        assert!(rss.test_is_allocated(&id), "a held route is not removed");
        assert!(rss.test_is_marked_for_release(&id));
        let again = rss.select_single_route(one_hop_params(kind, Some(id.clone()))).await;
        assert!(
            !matches!(&again, Ok(s) if s.route_id() == &id),
            "a draining route is not selected again"
        );
        drop(again);

        // The last reference releases it.
        drop(selected);
        assert!(!rss.test_is_allocated(&id), "the last drop releases it");
    }
    mock_registry::terminate(registry).await;
}

pub async fn test_unheld_route_releases_at_once() {
    let (registry, id, _) = store_with_one_route().await;
    {
        let routing_table = registry.routing_table();
        let rss = routing_table.route_spec_store();
        assert!(rss.test_release(&id));
        assert!(!rss.test_is_allocated(&id));
    }
    mock_registry::terminate(registry).await;
}
