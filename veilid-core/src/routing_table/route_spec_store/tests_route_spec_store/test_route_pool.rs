//! The safety route pool is the automatic, unpublished routes. A route the API
//! allocated (`automatic == false`) belongs to its caller: selection uses it only
//! when the caller names it as the preferred route.

use super::super::route_select::SelectedRoute;
use super::test_route_drain::{insert_route_with, one_hop_params, ordered};
use super::*;
use crate::routing_table::tests_routing_table::*;
use crate::tests::*;

fn selects(selected: &VeilidAPIResult<SelectedRoute>, id: &AllocatedRouteSetId) -> bool {
    matches!(selected, Ok(s) if s.route_id() == id)
}

/// An unpublished API route, such as one between allocation and publication, is not
/// handed out as a safety route
pub async fn test_manual_route_is_not_a_pool_route() {
    info!("--- test_manual_route_is_not_a_pool_route ---");
    let registry = mock_registry::init("route-pool-manual").await;
    {
        let routing_table = registry.routing_table();
        let rss = routing_table.route_spec_store();
        let hop = seed_nodes(&routing_table, 1).remove(0);
        let (manual, key) = insert_route_with(rss, hop, ordered(), false).await;
        let selected = rss
            .select_single_route(one_hop_params(key.kind(), None, Sequencing::PreferOrdered))
            .await;
        assert!(
            !selects(&selected, &manual),
            "an API route is not taken from the pool"
        );
    }
    mock_registry::terminate(registry).await;
}

/// A peer info change clears the published flag of an API route; it still does not
/// join the pool
pub async fn test_published_manual_route_stays_out_after_peer_info_change() {
    info!("--- test_published_manual_route_stays_out_after_peer_info_change ---");
    let registry = mock_registry::init("route-pool-peer-info").await;
    {
        let routing_table = registry.routing_table();
        let rss = routing_table.route_spec_store();
        let hop = seed_nodes(&routing_table, 1).remove(0);
        let (manual, key) = insert_route_with(rss, hop, ordered(), false).await;
        rss.mark_route_published(&manual, true).expect("publish");
        rss.report_peer_info_changed();
        let selected = rss
            .select_single_route(one_hop_params(key.kind(), None, Sequencing::PreferOrdered))
            .await;
        assert!(
            !selects(&selected, &manual),
            "an API route is not taken from the pool after a peer info change"
        );
    }
    mock_registry::terminate(registry).await;
}

/// The caller can still use its own route as a safety route by naming it
pub async fn test_preferred_manual_route_is_selected() {
    info!("--- test_preferred_manual_route_is_selected ---");
    let registry = mock_registry::init("route-pool-preferred").await;
    {
        let routing_table = registry.routing_table();
        let rss = routing_table.route_spec_store();
        let hop = seed_nodes(&routing_table, 1).remove(0);
        let (manual, key) = insert_route_with(rss, hop, ordered(), false).await;
        let selected = rss
            .select_single_route(one_hop_params(
                key.kind(),
                Some(manual.clone()),
                Sequencing::PreferOrdered,
            ))
            .await;
        assert!(selects(&selected, &manual), "a named route is used");
    }
    mock_registry::terminate(registry).await;
}

/// With an API route and an automatic route that both match, the automatic one is picked
pub async fn test_automatic_route_is_selected_beside_manual() {
    info!("--- test_automatic_route_is_selected_beside_manual ---");
    let registry = mock_registry::init("route-pool-automatic").await;
    {
        let routing_table = registry.routing_table();
        let rss = routing_table.route_spec_store();
        let hops = seed_nodes(&routing_table, 2);
        let (manual, key) = insert_route_with(rss, hops[0].clone(), ordered(), false).await;
        let (automatic, _) = insert_route_with(rss, hops[1].clone(), ordered(), true).await;
        for _ in 0..2 {
            let selected = rss
                .select_single_route(one_hop_params(key.kind(), None, Sequencing::PreferOrdered))
                .await;
            assert!(selects(&selected, &automatic), "the pool route is used");
            assert!(!selects(&selected, &manual));
        }
    }
    mock_registry::terminate(registry).await;
}
