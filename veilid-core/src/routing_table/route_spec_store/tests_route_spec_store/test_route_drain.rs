//! A route chosen for use is not removed under its user: a release marks
//! it (no longer selectable) and the last held reference releases it.

use super::*;
use crate::crypto::tests_crypto::*;
use crate::routing_table::tests_routing_table::*;
use crate::tests::*;

pub(super) fn one_hop_params(
    kind: CryptoKind,
    preferred: Option<AllocatedRouteSetId>,
    sequencing: Sequencing,
) -> RouteSelectParams {
    RouteSelectParams {
        crypto_kind: kind,
        preferred_route: preferred,
        hop_count: 1,
        stability: Stability::default(),
        sequencing,
        directions: Direction::In.into(),
        avoid_nodes: vec![],
        is_destination_safe: true,
        opt_destination: None,
    }
}

/// One-hop automatic routes are the default length, so the pool keeps them per cell
fn one_hop_default(config: &mut VeilidConfig) {
    config.network.rpc.default_route_hop_count = 1;
}

/// Insert a one-hop automatic route through `hop`, as allocation does after
/// choosing hops (allocation needs a live network)
async fn insert_route(
    rss: &RouteSpecStore,
    hop: NodeRef,
    orderings: SequenceOrderingSet,
) -> (AllocatedRouteSetId, PublicKey) {
    insert_route_with(rss, hop, orderings, true).await
}

/// Insert a one-hop route through `hop`; `automatic == false` is a route the API allocated
pub(super) async fn insert_route_with(
    rss: &RouteSpecStore,
    hop: NodeRef,
    orderings: SequenceOrderingSet,
    automatic: bool,
) -> (AllocatedRouteSetId, PublicKey) {
    let crypto = rss.crypto();
    let vcrypto = crypto
        .get_async(hop.best_node_id().kind())
        .expect("crypto kind");
    let keypair = vcrypto.generate_keypair().await;
    let route_set = BTreeMap::from([(
        keypair.key(),
        RouteSpecDetail {
            secret_key: keypair.secret(),
            hops: vec![hop.best_node_id()],
        },
    )]);
    let rssd = RouteSetSpecDetail::try_new(
        route_set,
        DirectionSet::all(),
        Stability::default(),
        orderings,
        automatic,
    )
    .expect("route set");
    let id = AllocatedRouteSetId::from(fake_route_id());
    {
        let mut content = rss.content.write();
        let mut cache = rss.cache.write();
        cache
            .add_allocated_route(id.clone(), &rssd, vec![hop])
            .expect("add route");
        content.add_detail(id.clone(), rssd);
    }
    (id, keypair.key())
}

pub(super) fn ordered() -> SequenceOrderingSet {
    SequenceOrderingSet::only(SequenceOrdering::Ordered)
}

fn is_allocated(rss: &RouteSpecStore, id: &AllocatedRouteSetId) -> bool {
    rss.cache.read().get_allocated_route_by_id(id).is_some()
}

fn is_marked_for_release(rss: &RouteSpecStore, id: &AllocatedRouteSetId) -> bool {
    rss.cache
        .read()
        .get_allocated_route_by_id(id)
        .is_some_and(|arce| arce.is_marked_for_release())
}

/// Run the private route management task's test-and-expire pass
fn run_route_expiry(routing_table: &RoutingTable, cur_ts: Timestamp) {
    let rdc = routing_table.get_routing_domain_controller(RoutingDomain::PublicInternet);
    let controller = rdc
        .as_any()
        .downcast_ref::<PublicInternetRoutingDomainController>()
        .expect("public internet controller");
    let _ = controller.get_allocated_routes_to_test(cur_ts);
}

pub async fn test_held_route_drains_on_release() {
    info!("--- test_held_route_drains_on_release ---");
    let registry = mock_registry::init("route-drain").await;
    {
        let routing_table = registry.routing_table();
        let rss = routing_table.route_spec_store();
        let hop = seed_nodes(&routing_table, 1).remove(0);
        let (id, key) = insert_route(rss, hop, ordered()).await;
        let kind = key.kind();
        let selected = rss
            .select_single_route(one_hop_params(kind, None, Sequencing::PreferOrdered))
            .await
            .expect("selects the route");
        assert_eq!(selected.route_id(), &id);

        // Released while held: still there, marked, out of selection.
        assert!(rss.release_allocated_route(id.clone()));
        assert!(is_allocated(rss, &id), "a held route is not removed");
        assert!(is_marked_for_release(rss, &id));
        let again = rss
            .select_single_route(one_hop_params(
                kind,
                Some(id.clone()),
                Sequencing::PreferOrdered,
            ))
            .await;
        assert!(
            !matches!(&again, Ok(s) if s.route_id() == &id),
            "a draining route is not selected again"
        );
        drop(again);

        // The last reference releases it.
        drop(selected);
        assert!(!is_allocated(rss, &id), "the last drop releases it");
    }
    mock_registry::terminate(registry).await;
}

pub async fn test_unheld_route_releases_at_once() {
    info!("--- test_unheld_route_releases_at_once ---");
    let registry = mock_registry::init("route-drain-unheld").await;
    {
        let routing_table = registry.routing_table();
        let rss = routing_table.route_spec_store();
        let hop = seed_nodes(&routing_table, 1).remove(0);
        let (id, _) = insert_route(rss, hop, ordered()).await;
        assert!(rss.release_allocated_route(id.clone()));
        assert!(!is_allocated(rss, &id));
    }
    mock_registry::terminate(registry).await;
}

/// Route management releases an automatic route carrying both orderings; one in use drains
pub async fn test_held_route_drains_on_legacy_expiry() {
    info!("--- test_held_route_drains_on_legacy_expiry ---");
    let registry = mock_registry::init("route-drain-legacy").await;
    {
        let routing_table = registry.routing_table();
        let rss = routing_table.route_spec_store();
        let hop = seed_nodes(&routing_table, 1).remove(0);
        let (id, _) = insert_route(rss, hop, SequenceOrderingSet::all()).await;
        // Selection never hands out a legacy route; a test or a send that took it before
        // the upgrade's first tick still holds it
        let held = rss.lock_allocated_route_set_by_id(&id).expect("route");

        run_route_expiry(&routing_table, Timestamp::now());
        assert!(
            is_allocated(rss, &id),
            "a held legacy route is not removed"
        );
        assert!(is_marked_for_release(rss, &id));
        // The next tick leaves it to its holder
        run_route_expiry(&routing_table, Timestamp::now());
        assert!(is_allocated(rss, &id));

        drop(held);
        assert!(!is_allocated(rss, &id), "the last drop releases it");
    }
    mock_registry::terminate(registry).await;
}

/// Route management keeps the fastest N of a (hop count, ordering) cell; a held extra drains
pub async fn test_held_route_drains_on_cell_expiry() {
    info!("--- test_held_route_drains_on_cell_expiry ---");
    let registry = mock_registry::init_with_config("route-drain-cell", one_hop_default).await;
    {
        let routing_table = registry.routing_table();
        let rss = routing_table.route_spec_store();
        let hops = seed_nodes(&routing_table, DESIRED_BACKGROUND_SAFETY_ROUTE_COUNT + 1);
        let mut routes = Vec::new();
        for hop in hops {
            routes.push(insert_route(rss, hop, ordered()).await);
        }
        // Tested once, long enough ago that each needs testing again
        let idle = TimestampDuration::new_ms(ROUTE_MIN_IDLE_TIME_MS as u64 + 1_000);
        let tested_ts = Timestamp::new(Timestamp::now().as_u64() - idle.as_u64());
        for (_, key) in &routes {
            rss.update_allocated_route_stats(tested_ts, key, |s| {
                let _ = s.record_loopback_result(SequenceOrdering::Ordered, true, 1.0, tested_ts);
                Ok(())
            });
        }
        // Equal (unmeasured) latency sorts by route id; the last one is the extra
        let extra = routes
            .iter()
            .map(|(id, _)| id.clone())
            .max()
            .expect("routes");
        let held = rss.lock_allocated_route_set_by_id(&extra).expect("route");

        run_route_expiry(&routing_table, Timestamp::now());
        assert!(is_allocated(rss, &extra), "a held extra is not removed");
        assert!(is_marked_for_release(rss, &extra));
        for (id, _) in routes.iter().filter(|(id, _)| id != &extra) {
            assert!(!is_marked_for_release(rss, id), "the fastest N are kept");
        }

        drop(held);
        assert!(!is_allocated(rss, &extra), "the last drop releases it");
    }
    mock_registry::terminate(registry).await;
}

/// A selection that waited on the allocate lock takes the route the allocation behind the
/// lock made, held, falling back from a datagram preference to an ordered route
pub async fn test_route_found_after_allocate_wait_is_held() {
    info!("--- test_route_found_after_allocate_wait_is_held ---");
    let registry = mock_registry::init("route-drain-allocate").await;
    {
        let routing_table = registry.routing_table();
        let rss = routing_table.route_spec_store();
        let hop = seed_nodes(&routing_table, 1).remove(0);
        let kind = hop.best_node_id().kind();

        let allocating = rss.allocate_route_lock.lock().await;
        let select =
            rss.select_single_route(one_hop_params(kind, None, Sequencing::PreferUnordered));
        let allocate = async {
            // The selection found nothing and is waiting on the lock
            sleep(100).await;
            let route = insert_route(rss, hop, ordered()).await;
            drop(allocating);
            route
        };
        let (selected, (id, _)) = futures_util::join!(select, allocate);
        let selected = selected.expect("selects the allocated route");
        assert_eq!(selected.route_id(), &id);

        assert!(rss.release_allocated_route(id.clone()));
        assert!(is_allocated(rss, &id), "a held route is not removed");
        drop(selected);
        assert!(!is_allocated(rss, &id), "the last drop releases it");
    }
    mock_registry::terminate(registry).await;
}

/// A route marked for release while held is not saved, so a restart never reloads it live
pub async fn test_marked_route_is_not_saved() {
    info!("--- test_marked_route_is_not_saved ---");
    let identity = fix_identity();
    let configure = |config: &mut VeilidConfig| {
        config.network.routing_table.bootstrap = vec![];
        set_identity(&mut config.network.routing_table, &identity);
    };
    let (registry, storage) = mock_registry::init_with_storage("route-drain-save", configure).await;
    let (kept, marked) = {
        let routing_table = registry.routing_table();
        let rss = routing_table.route_spec_store();
        let hops = seed_nodes(&routing_table, 2);
        let (kept, _) = insert_route(rss, hops[0].clone(), ordered()).await;
        let (marked, _) = insert_route(rss, hops[1].clone(), ordered()).await;
        let held = rss.lock_allocated_route_set_by_id(&marked).expect("route");
        assert!(rss.release_allocated_route(marked.clone()));
        assert!(is_marked_for_release(rss, &marked));

        routing_table.save().await.expect("should save entries");
        rss.save().await.expect("should save routes");
        // The process exits before the holder finishes
        core::mem::forget(held);
        (kept, marked)
    };
    mock_registry::terminate(registry).await;

    let registry = mock_registry::reopen("route-drain-save", storage, configure).await;
    {
        let routing_table = registry.routing_table();
        let rss = routing_table.route_spec_store();
        assert!(is_allocated(rss, &kept), "an unmarked route reloads");
        assert!(!is_allocated(rss, &marked), "a marked route does not");
        assert!(rss.content.read().get_detail(&marked).is_none());
    }
    mock_registry::terminate(registry).await;
}
