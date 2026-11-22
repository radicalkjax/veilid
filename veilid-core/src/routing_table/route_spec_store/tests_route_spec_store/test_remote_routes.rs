//! A remote route the app imported is tested as one unit: until it is proved, then once per idle
//! time, and again while a failure stands. An import of our own route's blob shares the
//! allocated route's id; releasing or failing the import never touches the allocated route.

use super::test_route_drain::{insert_route_with, is_allocated, ordered};
use super::*;
use crate::routing_table::tasks::ping_validator::PingValidationEntry;
use crate::routing_table::tests_routing_table::*;
use crate::tests::*;

pub fn test_remote_route_testing_cadence() {
    let t0 = Timestamp::new(1_000_000_000);
    let idle = TimestampDuration::new_ms(ROUTE_MIN_IDLE_TIME_MS as u64);
    let mut stats = RouteStats::new(t0);
    assert!(stats.remote_needs_testing(t0), "never proved");

    stats.record_remote_test_pass(SequenceOrdering::Ordered, t0);
    assert!(
        !stats.remote_needs_testing(t0),
        "an ordering the route was never seen on does not make it stale"
    );
    assert!(!stats.remote_needs_testing(t0.later(idle)));
    assert!(stats.remote_needs_testing(t0.later(idle).later(TimestampDuration::new(1))));

    // A failure on another ordering asks for a test, and a pass over any ordering clears it
    stats.record_send_failed(SequenceOrdering::Unordered);
    assert!(stats.remote_needs_testing(t0));
    stats.record_remote_test_pass(SequenceOrdering::Ordered, t0);
    assert!(!stats.remote_needs_testing(t0));
}

/// Import our own route's blob, as an app testing its route does: the import's id is the
/// allocated route's id
fn import_own_route(
    rss: &RouteSpecStore,
    allocated: &AllocatedRouteSetId,
    key: PublicKey,
    hop: &NodeRef,
    imported_ts: Timestamp,
) -> RemoteRouteSetId {
    let import = RemoteRouteSetId::from_route_id(allocated.clone().into());
    let private_route = PrivateRoute::new_stub(key, RouteNode::NodeId(hop.best_node_id()));
    rss.cache
        .write()
        .add_remote_route(imported_ts, import.clone(), vec![Arc::new(private_route)]);
    import
}

/// Complete a remote route test entry with a failure (no answer)
fn fail_remote_test(registry: &VeilidComponentRegistry, entry: PingValidationEntry) {
    let on_complete = entry.on_complete.expect("a remote test has a callback");
    let sdr = SendDataResult::new(
        NodeContactMethodResult::Resolved(ContactMethod::Existing),
        NetworkResult::timeout(),
    );
    let _ = on_complete(registry.clone(), Ok(StatusResult::Failed(sdr)));
}

/// Allocated routes the next route change reports dead
fn dead_allocated(rss: &RouteSpecStore) -> Vec<AllocatedRouteSetId> {
    rss.cache
        .write()
        .take_dead_routes()
        .map(|(allocated, _)| allocated)
        .unwrap_or_default()
}

/// A failed test of an import releases that import; the allocated route with the same id stays
pub async fn test_failed_remote_test_releases_only_the_import() {
    info!("--- test_failed_remote_test_releases_only_the_import ---");
    let registry = mock_registry::init("remote-route-test-failed").await;
    {
        let routing_table = registry.routing_table();
        let rss = routing_table.route_spec_store();
        let hop = seed_nodes(&routing_table, 1).remove(0);
        let (allocated, key) = insert_route_with(rss, hop.clone(), ordered(), false).await;
        let import = import_own_route(rss, &allocated, key, &hop, Timestamp::now());
        let entry =
            RoutingTable::build_remote_route_test_entry(rss, &import, 0).expect("test entry");
        fail_remote_test(&registry, entry);
        assert!(
            !rss.is_route_id_remote(&import.clone().into()),
            "the failed import is released"
        );
        assert!(is_allocated(rss, &allocated), "the allocated route stays");
        assert!(
            !dead_allocated(rss).contains(&allocated),
            "the allocated route is not reported dead"
        );
    }
    mock_registry::terminate(registry).await;
}

/// The app releases its import while a test of it is in flight; the test's failure then finds no
/// import and releases nothing, not the allocated route that shares the id
pub async fn test_failed_remote_test_after_release_keeps_allocated_route() {
    info!("--- test_failed_remote_test_after_release_keeps_allocated_route ---");
    let registry = mock_registry::init("remote-route-test-released").await;
    {
        let routing_table = registry.routing_table();
        let rss = routing_table.route_spec_store();
        let hop = seed_nodes(&routing_table, 1).remove(0);
        let (allocated, key) = insert_route_with(rss, hop.clone(), ordered(), false).await;
        let import = import_own_route(rss, &allocated, key, &hop, Timestamp::now());
        let entry =
            RoutingTable::build_remote_route_test_entry(rss, &import, 0).expect("test entry");
        assert!(
            rss.release_route(import.clone().into()),
            "the import is released"
        );
        assert!(
            is_allocated(rss, &allocated),
            "releasing the import keeps the route"
        );
        let _ = dead_allocated(rss);

        fail_remote_test(&registry, entry);
        assert!(is_allocated(rss, &allocated), "the allocated route stays");
        assert!(
            !dead_allocated(rss).contains(&allocated),
            "the allocated route is not reported dead"
        );
        assert!(
            rss.release_route(allocated.clone().into()),
            "the caller still releases its own route"
        );
        assert!(!is_allocated(rss, &allocated));
        assert!(dead_allocated(rss).contains(&allocated));
    }
    mock_registry::terminate(registry).await;
}
