//! Which watches and per-node watches the outbound watch manager keeps. Dropping a wanted watch
//! loses its changes with nothing reporting it; keeping a dead per-node watch holds its watch at
//! consensus, so it is never reconciled.

use super::*;
use crate::network_manager::{AddressType, ProtocolType, TransportType};
use crate::routing_table::tests_routing_table::mocks::*;
use crate::tests::*;

fn make_record_key() -> RecordKey {
    RecordKey::from_opaque(
        OpaqueRecordKey::new(CRYPTO_KIND_VLD0, BareOpaqueRecordKey::new(&[0u8; 32])),
        None,
    )
}

fn make_params(subkeys: ValueSubkeyRangeSet) -> OutboundWatchParameters {
    OutboundWatchParameters {
        expiration: Timestamp::new(0),
        count: u32::MAX,
        subkeys,
        opt_watcher: None,
        safety_selection: SafetySelection::Unsafe(Sequencing::PreferOrdered),
    }
}

/// Renewal runs only before the earliest per-node expiration, so a stall through the renew
/// window leaves per-node watches expired on nodes that still answer, and they must go
async fn test_expired_per_node_watches_leave_the_watch() {
    let registry = mock_registry::init("watch_bookkeeping").await;
    {
        let routing_table = registry.routing_table();
        let cur_ts = Timestamp::now();
        let udp = TransportType::new(ProtocolType::UDP, AddressType::IPV4);
        let record_key = make_record_key();
        let params = make_params(ValueSubkeyRangeSet::single(0));
        let expired = cur_ts.earlier(TimestampDuration::new_secs(1));
        let live = cur_ts.later(TimestampDuration::new_secs(60));

        let mut owm = OutboundWatchManager::new();
        let mut pnks = Vec::new();
        for (n, node_ref) in seed_nodes(&routing_table, 3).into_iter().enumerate() {
            // An answering node, so its watch is judged by expiration rather than as dead
            node_ref.stats_question_sent(cur_ts, ByteCount::new(0), true, udp, true);
            node_ref.stats_answer_rcvd(cur_ts, cur_ts, ByteCount::new(0), udp, true);
            assert!(node_ref.state(cur_ts).is_responsive());

            let pnk = PerNodeKey {
                record_key: record_key.clone(),
                node_id: node_ref.node_ids().get(CRYPTO_KIND_VLD0).unwrap_or_log(),
            };
            owm.per_node_states.insert(
                pnk.clone(),
                OutboundWatchPerNodeState {
                    watch_id: n as u64 + 1,
                    safety_selection: params.safety_selection.clone(),
                    opt_watcher: None,
                    expiration: if n == 0 { live } else { expired },
                    count: u32::MAX,
                    watch_node_ref: Some(node_ref),
                    opt_value_changed_route: None,
                },
            );
            pnks.push(pnk);
        }

        let mut watch = OutboundWatch::new(record_key.clone(), params.clone());
        watch
            .state_mut_or_create(|| params.clone())
            .edit(&owm.per_node_states, |editor| {
                editor.add_nodes(pnks.clone())
            });
        owm.outbound_watches.insert(record_key.opaque(), watch);

        owm.update_per_node_states(cur_ts);

        let state = owm.outbound_watches[&record_key.opaque()]
            .state()
            .expect("the watch keeps its state");
        assert_eq!(state.nodes(), &pnks[..1], "only the unexpired node stays");
        assert_eq!(state.min_expiration(), live);
        assert_eq!(
            owm.per_node_states.len(),
            1,
            "expired per-node watches are dropped"
        );
    }
    mock_registry::terminate(registry).await;
}

/// A watch call before the record's watch is established replaces what it wants. Only a cancel
/// drops it, and an unestablished one has nothing on the network to cancel.
fn test_rewatch_before_establishment_keeps_the_watch() {
    let record_key = make_record_key();
    let mut owm = OutboundWatchManager::new();
    owm.set_desired_watch(
        record_key.clone(),
        Some(make_params(ValueSubkeyRangeSet::single(0))),
    );

    let wider = make_params(ValueSubkeyRangeSet::single_range(0, 3));
    owm.set_desired_watch(record_key.clone(), Some(wider.clone()));
    let watch = owm
        .outbound_watches
        .get(&record_key.opaque())
        .expect("a re-watch keeps the watch");
    assert_eq!(watch.desired(), Some(wider));

    owm.set_desired_watch(record_key.clone(), None);
    assert!(owm.outbound_watches.is_empty());
}

/// A sweep of every watch runs oldest first and only as many at a time as the cap allows; the rest
/// wait for slots to free up
fn test_change_inspections_drain_oldest_first_and_capped() {
    let mut owm = OutboundWatchManager::new();
    let keys: Vec<RecordKey> = (0..40u8)
        .map(|n| {
            RecordKey::from_opaque(
                OpaqueRecordKey::new(CRYPTO_KIND_VLD0, BareOpaqueRecordKey::new(&[n; 32])),
                None,
            )
        })
        .collect();
    // Enqueued in reverse, so oldest-first order is not insertion order
    for (n, k) in keys.iter().enumerate().rev() {
        owm.set_desired_watch(k.clone(), Some(make_params(ValueSubkeyRangeSet::single(0))));
        owm.enqueue_change_inspect_after(
            k.clone(),
            ValueSubkeyRangeSet::single(0),
            Timestamp::new(n as u64 + 1),
            ChangeInspectTrigger::Watch,
        );
    }
    let cur_ts = Timestamp::new(1_000);
    let taken = |batch: &[(RecordKey, ValueSubkeyRangeSet, ChangeInspectTrigger)]| {
        batch.iter().map(|(k, _, _)| k.clone()).collect::<Vec<_>>()
    };

    let first = owm.take_due_change_inspections(cur_ts);
    assert_eq!(taken(&first), keys[..MAX_CHANGE_INSPECTIONS_IN_FLIGHT]);
    assert!(
        owm.take_due_change_inspections(cur_ts).is_empty(),
        "no slot is free"
    );

    for (k, _, _) in &first[..5] {
        owm.change_inspects_in_flight.remove(&k.opaque());
    }
    let next = owm.take_due_change_inspections(cur_ts);
    assert_eq!(
        taken(&next),
        keys[MAX_CHANGE_INSPECTIONS_IN_FLIGHT..MAX_CHANGE_INSPECTIONS_IN_FLIGHT + 5]
    );
}

/// Only a per-node watch inside its own renew window, or bound to a dead route, is renewed. After
/// one renewal round, the node that did not answer is the only one left to retry.
fn test_renewal_targets_only_nodes_that_need_it() {
    let window = TimestampDuration::new_secs(30);
    let cur_ts = Timestamp::new(10_000_000_000);
    let renew_ts = cur_ts.later(window);
    let alive = |_: &PublicKey| false;
    let per_node = |expiration: Timestamp, route: Option<u8>| OutboundWatchPerNodeState {
        watch_id: 1,
        safety_selection: SafetySelection::Unsafe(Sequencing::PreferOrdered),
        opt_watcher: None,
        expiration,
        count: u32::MAX,
        watch_node_ref: None,
        opt_value_changed_route: route
            .map(|n| PublicKey::new(CRYPTO_KIND_VLD0, BarePublicKey::new(&[n; 32]))),
    };

    // The silent node is still due; the two that answered renewed for a full lifetime
    let silent = per_node(cur_ts.later(TimestampDuration::new_secs(10)), None);
    let renewed = per_node(cur_ts.later(TimestampDuration::new_secs(600)), None);
    assert!(silent.needs_renewal(cur_ts, renew_ts, alive));
    assert!(!renewed.needs_renewal(cur_ts, renew_ts, alive));

    // Past its expiration a per-node watch is dropped, not renewed
    let expired = per_node(cur_ts.earlier(TimestampDuration::new_secs(1)), None);
    assert!(!expired.needs_renewal(cur_ts, renew_ts, alive));

    // A dead route renews only the nodes bound to it
    let bound = per_node(cur_ts.later(TimestampDuration::new_secs(600)), Some(7));
    let elsewhere = per_node(cur_ts.later(TimestampDuration::new_secs(600)), Some(8));
    let dead = PublicKey::new(CRYPTO_KIND_VLD0, BarePublicKey::new(&[7u8; 32]));
    let route_is_dead = |route: &PublicKey| *route == dead;
    assert!(bound.needs_renewal(cur_ts, renew_ts, route_is_dead));
    assert!(!elsewhere.needs_renewal(cur_ts, renew_ts, route_is_dead));
}

/// A watch below consensus whose per-node watch is due for renewal reconciles at the paced rate,
/// not on every tick
async fn test_below_consensus_renewal_waits_for_reconcile_pacing() {
    let registry = mock_registry::init("watch_bookkeeping_pacing").await;
    {
        let cur_ts = Timestamp::now();
        let record_key = make_record_key();
        let params = make_params(ValueSubkeyRangeSet::single(0));
        // Two of three nodes, both inside their renew window
        let per_node_states: HashMap<PerNodeKey, OutboundWatchPerNodeState> = (1..=2u8)
            .map(|n| {
                (
                    PerNodeKey {
                        record_key: record_key.clone(),
                        node_id: NodeId::new(CRYPTO_KIND_VLD0, BareNodeId::new(&[n; 32])),
                    },
                    OutboundWatchPerNodeState {
                        watch_id: n.into(),
                        safety_selection: params.safety_selection.clone(),
                        opt_watcher: None,
                        expiration: cur_ts.later(TimestampDuration::new_secs(10)),
                        count: u32::MAX,
                        watch_node_ref: None,
                        opt_value_changed_route: None,
                    },
                )
            })
            .collect();
        let paced_until = cur_ts.later(TimestampDuration::new_secs(5));
        let mut watch = OutboundWatch::new(record_key, params.clone());
        watch
            .state_mut_or_create(|| params.clone())
            .edit(&per_node_states, |editor| {
                editor.add_nodes(per_node_states.keys().cloned());
                editor.set_next_reconcile_ts(paced_until);
            });

        assert!(!watch.needs_renew(&registry, 3, cur_ts), "below consensus");
        assert!(
            !watch.needs_reconcile(&registry, 3, cur_ts),
            "inside the reconcile pacing"
        );
        assert!(watch.needs_reconcile(
            &registry,
            3,
            paced_until.later(TimestampDuration::new_secs(1))
        ));
    }
    mock_registry::terminate(registry).await;
}

pub async fn test_all() {
    test_below_consensus_renewal_waits_for_reconcile_pacing().await;
    test_renewal_targets_only_nodes_that_need_it();
    test_change_inspections_drain_oldest_first_and_capped();
    test_expired_per_node_watches_leave_the_watch().await;
    test_rewatch_before_establishment_keeps_the_watch();
}
