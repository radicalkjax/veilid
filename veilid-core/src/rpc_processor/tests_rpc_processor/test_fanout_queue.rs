use super::*;
use crate::routing_table::tests_routing_table::mocks::*;
use crate::tests::*;

pub const CONSENSUS_COUNT: usize = 5;
const COLLECTION_COUNT: usize = 8;
pub const CONSENSUS_WIDTH: usize = 10;
pub const SEED_COUNT: usize = 20;

fn make_queue<'a>(
    registry: &VeilidComponentRegistry,
    collection_count: usize,
    opt_throttle_duration: Option<TimestampDuration>,
) -> FanoutQueue<'a> {
    let target = fix_typed_node_id(CRYPTO_KIND_VLD0, 0).to_hash_coordinate();
    FanoutQueue::new(
        "test".to_string(),
        registry.clone(),
        CRYPTO_KIND_VLD0,
        Box::new(RoutingTable::make_closest_node_id_sort(target)),
        CONSENSUS_COUNT,
        collection_count,
        CONSENSUS_WIDTH,
        opt_throttle_duration,
    )
}

fn count_stage(queue: &FanoutQueue, stage: FanoutNodeStage) -> usize {
    queue.with_nodes(|nodes, sorted_nodes| {
        sorted_nodes
            .iter()
            .filter(|x| {
                core::mem::discriminant(&nodes.get(*x).unwrap_or_log().status.stage())
                    == core::mem::discriminant(&stage)
            })
            .count()
    })
}

fn node_id(work_item: &FanoutWorkItem) -> NodeId {
    work_item
        .node_ref
        .node_ids()
        .get(CRYPTO_KIND_VLD0)
        .unwrap_or_log()
}

fn take_work(queue: &mut FanoutQueue) -> FanoutWorkItem {
    try_take_work(queue).expect("should have work")
}

fn try_take_work(queue: &mut FanoutQueue) -> Option<FanoutWorkItem> {
    let work_receiver = queue
        .request_work("test-lane".to_string())
        .expect("should request work");
    work_receiver.try_recv().ok()
}

/// A full seed must survive `update()` untouched: with no answers yet, no node may be
/// width-disqualified before a single RPC is attempted
pub async fn test_seed_keeps_full_reserve() {
    let registry = mock_registry::init("fanout_queue").await;
    {
        let routing_table = registry.routing_table();
        let node_refs = seed_nodes(&routing_table, SEED_COUNT);

        let mut queue = make_queue(&registry, CONSENSUS_COUNT, None);
        queue.update(&node_refs, Timestamp::now_non_decreasing());

        assert_eq!(count_stage(&queue, FanoutNodeStage::Queued), SEED_COUNT);
        assert_eq!(count_stage(&queue, FanoutNodeStage::Disqualified), 0);
    }
    mock_registry::terminate(registry).await;
}

/// Once `consensus_width` closer nodes hold terminal answers, the reserve can never
/// matter and is culled
pub async fn test_answered_window_culls_reserve() {
    let registry = mock_registry::init("fanout_queue").await;
    {
        let routing_table = registry.routing_table();
        let node_refs = seed_nodes(&routing_table, SEED_COUNT);

        let mut queue = make_queue(&registry, CONSENSUS_COUNT, None);
        queue.update(&node_refs, Timestamp::now_non_decreasing());

        // Accept the closest CONSENSUS_WIDTH nodes; dispatch order is closest-first
        for _ in 0..CONSENSUS_WIDTH {
            let work_item = take_work(&mut queue);
            queue.accepted(work_item.node_ref, Timestamp::now_non_decreasing());
        }
        queue.update(&[], Timestamp::now_non_decreasing());

        assert_eq!(
            count_stage(&queue, FanoutNodeStage::Accepted),
            CONSENSUS_WIDTH
        );
        assert_eq!(
            count_stage(&queue, FanoutNodeStage::Disqualified),
            SEED_COUNT - CONSENSUS_WIDTH
        );
    }
    mock_registry::terminate(registry).await;
}

/// Timeouts hold no window slot, so a failing closest set leaves the reserve alive
/// and dispatchable
pub async fn test_timeouts_promote_reserve() {
    let registry = mock_registry::init("fanout_queue").await;
    {
        let routing_table = registry.routing_table();
        let node_refs = seed_nodes(&routing_table, SEED_COUNT);

        let mut queue = make_queue(&registry, CONSENSUS_COUNT, None);
        queue.update(&node_refs, Timestamp::now_non_decreasing());

        // Closest ten: four accept, six time out
        for n in 0..CONSENSUS_WIDTH {
            let work_item = take_work(&mut queue);
            if n < 4 {
                queue.accepted(work_item.node_ref, Timestamp::now_non_decreasing());
            } else {
                queue.timeout(work_item.node_ref, Timestamp::now_non_decreasing());
            }
        }
        queue.update(&[], Timestamp::now_non_decreasing());

        assert_eq!(count_stage(&queue, FanoutNodeStage::Accepted), 4);
        assert_eq!(count_stage(&queue, FanoutNodeStage::Timeout), 6);
        assert_eq!(
            count_stage(&queue, FanoutNodeStage::Queued),
            SEED_COUNT - CONSENSUS_WIDTH
        );
        assert_eq!(count_stage(&queue, FanoutNodeStage::Disqualified), 0);

        // The reserve is dispatchable
        let work_item = take_work(&mut queue);
        queue.accepted(work_item.node_ref, Timestamp::now_non_decreasing());
        assert_eq!(count_stage(&queue, FanoutNodeStage::Accepted), 5);
    }
    mock_registry::terminate(registry).await;
}

/// A throttled queue keeps dispatching reserve candidates after consensus until the
/// collection target, then stops; with no collection target it stops at consensus
pub async fn test_collection_dispatches_reserve_after_consensus() {
    let registry = mock_registry::init("fanout_queue").await;
    {
        let routing_table = registry.routing_table();
        let throttle = Some(TimestampDuration::new_secs(10));

        // Collection target 8: dispatch continues from accept 5 through 8, then stops
        let node_refs = seed_nodes(&routing_table, SEED_COUNT);
        let mut queue = make_queue(&registry, COLLECTION_COUNT, throttle);
        queue.update(&node_refs, Timestamp::now_non_decreasing());
        for _ in 0..COLLECTION_COUNT {
            let work_item = take_work(&mut queue);
            queue.accepted(work_item.node_ref, Timestamp::now_non_decreasing());
        }
        assert!(try_take_work(&mut queue).is_none());
        assert_eq!(
            count_stage(&queue, FanoutNodeStage::Accepted),
            COLLECTION_COUNT
        );
        assert_eq!(count_stage(&queue, FanoutNodeStage::Disqualified), 0);

        // No collection target: dispatch stops at consensus
        let node_refs = seed_nodes(&routing_table, SEED_COUNT);
        let mut queue = make_queue(&registry, CONSENSUS_COUNT, throttle);
        queue.update(&node_refs, Timestamp::now_non_decreasing());
        for _ in 0..CONSENSUS_COUNT {
            let work_item = take_work(&mut queue);
            queue.accepted(work_item.node_ref, Timestamp::now_non_decreasing());
        }
        assert!(try_take_work(&mut queue).is_none());
        assert_eq!(
            count_stage(&queue, FanoutNodeStage::Accepted),
            CONSENSUS_COUNT
        );
    }
    mock_registry::terminate(registry).await;
}

/// Rejection consensus still ends the fanout: `consensus_count` consecutive rejections
/// disqualify everything unfinished (transaction mutual exclusion depends on this)
pub async fn test_rejection_consensus_culls_everything() {
    let registry = mock_registry::init("fanout_queue").await;
    {
        let routing_table = registry.routing_table();
        let node_refs = seed_nodes(&routing_table, SEED_COUNT);

        let mut queue = make_queue(&registry, CONSENSUS_COUNT, None);
        queue.update(&node_refs, Timestamp::now_non_decreasing());

        for _ in 0..CONSENSUS_COUNT {
            let work_item = take_work(&mut queue);
            queue.rejected(work_item.node_ref, Timestamp::now_non_decreasing());
        }

        assert_eq!(
            count_stage(&queue, FanoutNodeStage::Rejected),
            CONSENSUS_COUNT
        );
        assert_eq!(
            count_stage(&queue, FanoutNodeStage::Disqualified),
            SEED_COUNT - CONSENSUS_COUNT
        );
    }
    mock_registry::terminate(registry).await;
}

/// A node with no copy of the record refuses a watch outright, and `consensus_width`
/// (10) nodes consider themselves close enough while only `set_value_count` (5) ever
/// receive a write. The seeds are the nodes that answered for the record before, the
/// only evidence available, and the distance sort loses it. Preferred nodes must lead
/// while each group keeps distance order within itself.
pub async fn test_preferred_nodes_lead_the_sort() {
    let registry = mock_registry::init("fanout_queue").await;
    {
        let routing_table = registry.routing_table();
        let node_refs = seed_nodes(&routing_table, SEED_COUNT);

        // The nodes the plain distance sort puts LAST, so preference is visible
        let mut unpreferred = make_queue(&registry, CONSENSUS_COUNT, None);
        unpreferred.update(&node_refs, Timestamp::now_non_decreasing());
        let distance_order = unpreferred.with_nodes(|_, sorted_nodes| sorted_nodes.to_vec());
        let farthest: Vec<NodeId> = distance_order.iter().rev().take(3).cloned().collect();
        let preferred_refs: Vec<NodeRef> = node_refs
            .iter()
            .filter(|nr| {
                nr.node_ids()
                    .get(CRYPTO_KIND_VLD0)
                    .is_some_and(|id| farthest.contains(&id))
            })
            .cloned()
            .collect();
        assert_eq!(preferred_refs.len(), farthest.len());

        let mut queue = make_queue(&registry, CONSENSUS_COUNT, None);
        queue.set_preferred_nodes(&preferred_refs);
        queue.update(&node_refs, Timestamp::now_non_decreasing());

        let order = queue.with_nodes(|_, sorted_nodes| sorted_nodes.to_vec());
        assert_eq!(order.len(), SEED_COUNT);

        // The preferred nodes are asked first, despite being the farthest away
        let leading: HashSet<NodeId> = order.iter().take(farthest.len()).cloned().collect();
        assert_eq!(
            leading,
            farthest.iter().cloned().collect::<HashSet<_>>(),
            "preferred nodes should lead the queue"
        );

        // The rest follow in the relative order the unpreferred queue gave them
        let rest: Vec<NodeId> = order.into_iter().skip(farthest.len()).collect();
        let expected_rest: Vec<NodeId> = distance_order
            .into_iter()
            .filter(|id| !farthest.contains(id))
            .collect();
        assert_eq!(
            rest, expected_rest,
            "the unpreferred remainder should keep distance order"
        );
    }
    mock_registry::terminate(registry).await;
}

/// With no preference set the ordering is pure distance, which is what get, set and
/// inspect rely on.
pub async fn test_no_preference_leaves_order_untouched() {
    let registry = mock_registry::init("fanout_queue").await;
    {
        let routing_table = registry.routing_table();
        let node_refs = seed_nodes(&routing_table, SEED_COUNT);

        let mut plain = make_queue(&registry, CONSENSUS_COUNT, None);
        plain.update(&node_refs, Timestamp::now_non_decreasing());

        let mut empty_pref = make_queue(&registry, CONSENSUS_COUNT, None);
        empty_pref.set_preferred_nodes(&[]);
        empty_pref.update(&node_refs, Timestamp::now_non_decreasing());

        assert_eq!(
            plain.with_nodes(|_, s| s.to_vec()),
            empty_pref.with_nodes(|_, s| s.to_vec())
        );
    }
    mock_registry::terminate(registry).await;
}

/// A node that lost a routed question keeps its window slot, so it is still asked and its answer
/// still counts, but a demoting fanout asks it after the window's other candidates; the mark lifts
/// when it answers a routed question or the state the loss was judged in moves on
pub async fn test_routed_unresponsive_nodes_dispatch_after_their_window() {
    let registry = mock_registry::init("fanout_queue").await;
    {
        let routing_table = registry.routing_table();
        let node_refs = seed_nodes(&routing_table, SEED_COUNT);
        let make = |demote: bool| {
            let mut queue = make_queue(&registry, CONSENSUS_COUNT, None);
            queue.set_demote_routed_unresponsive(demote);
            queue.update(&node_refs, Timestamp::now_non_decreasing());
            queue
        };
        let first_dispatched = |demote: bool| node_id(&take_work(&mut make(demote)));

        let distance_order = make(true).with_nodes(|_, sorted_nodes| sorted_nodes.to_vec());
        let closest_id = distance_order.first().cloned().unwrap_or_log();
        let closest = node_refs
            .iter()
            .find(|nr| nr.node_ids().get(CRYPTO_KIND_VLD0) == Some(closest_id.clone()))
            .unwrap_or_log();
        let lost_in =
            routing_table.loss_epoch(Some(closest.node_info_ts(RoutingDomain::PublicInternet)));

        closest.stats_routed_lost_question(lost_in);
        let mut queue = make(true);
        assert_eq!(
            queue.with_nodes(|_, sorted_nodes| sorted_nodes.to_vec()),
            distance_order,
            "the window keeps distance order"
        );
        // The window's other members are asked first and answer
        for expected in &distance_order[1..CONSENSUS_WIDTH] {
            let work_item = take_work(&mut queue);
            assert_eq!(node_id(&work_item), *expected);
            queue.accepted(work_item.node_ref, Timestamp::now_non_decreasing());
        }
        // Then the demoted node, still holding its slot, before any reserve node
        let work_item = take_work(&mut queue);
        assert_eq!(node_id(&work_item), closest_id);
        queue.accepted(work_item.node_ref, Timestamp::now_non_decreasing());
        queue.update(&[], Timestamp::now_non_decreasing());
        assert_eq!(
            count_stage(&queue, FanoutNodeStage::Accepted),
            CONSENSUS_WIDTH
        );
        assert_eq!(
            count_stage(&queue, FanoutNodeStage::Disqualified),
            SEED_COUNT - CONSENSUS_WIDTH
        );
        assert_eq!(first_dispatched(false), closest_id);

        closest.stats_routed_answer();
        assert_eq!(first_dispatched(true), closest_id);

        // A loss judged in another online state counts only there
        let moved = LossEpoch {
            online: lost_in.online + 1,
            ..lost_in
        };
        closest.stats_routed_lost_question(moved);
        assert!(closest.operate(|e| e.is_routed_unresponsive(&moved)));
        assert_eq!(first_dispatched(true), closest_id);
    }
    mock_registry::terminate(registry).await;
}

/// A demoted node in flight is not counted on: with the rest of the window answered short of the
/// target, the straggler cut is due even though its accept alone would reach the target
pub async fn test_demoted_straggler_does_not_hold_the_cut() {
    let registry = mock_registry::init("fanout_queue").await;
    {
        let routing_table = registry.routing_table();
        let node_refs = seed_nodes(&routing_table, SEED_COUNT);
        let floor = TimestampDuration::new_secs(1);

        let mut plain = make_queue(&registry, CONSENSUS_COUNT, None);
        plain.update(&node_refs, Timestamp::now_non_decreasing());
        let closest_id = plain.with_nodes(|_, sorted_nodes| sorted_nodes[0].clone());
        let closest = node_refs
            .iter()
            .find(|nr| nr.node_ids().get(CRYPTO_KIND_VLD0) == Some(closest_id.clone()))
            .unwrap_or_log();
        let lost_in =
            routing_table.loss_epoch(Some(closest.node_info_ts(RoutingDomain::PublicInternet)));

        // The closest stays in flight; four others accept, the rest answer stale
        let next_cut = |demote_closest: bool| {
            if demote_closest {
                closest.stats_routed_lost_question(lost_in);
            } else {
                closest.stats_routed_answer();
            }
            let mut queue = make_queue(&registry, CONSENSUS_COUNT, None);
            queue.set_demote_routed_unresponsive(true);
            queue.update(&node_refs, Timestamp::now_non_decreasing());
            let work_items = (0..CONSENSUS_WIDTH)
                .map(|_| take_work(&mut queue))
                .collect::<Vec<_>>();
            let mut accepted = 0usize;
            for work_item in work_items {
                if node_id(&work_item) == closest_id {
                    continue;
                }
                if accepted < CONSENSUS_COUNT - 1 {
                    queue.accepted(work_item.node_ref, Timestamp::now_non_decreasing());
                    accepted += 1;
                } else {
                    queue.stale(work_item.node_ref, Timestamp::now_non_decreasing());
                }
            }
            queue.next_straggler_cut(CONSENSUS_COUNT, floor)
        };
        assert!(
            next_cut(false).is_none(),
            "a node in flight that could complete consensus is waited for"
        );
        assert!(
            next_cut(true).is_some(),
            "a demoted node in flight does not hold the cut"
        );
    }
    mock_registry::terminate(registry).await;
}

/// The pump's straggler deadline is the first moment the cut has a node to take, and there is
/// none while the nodes in flight could still bring consensus
pub async fn test_next_straggler_cut_matches_the_cut() {
    let registry = mock_registry::init("fanout_queue").await;
    {
        let routing_table = registry.routing_table();
        let node_refs = seed_nodes(&routing_table, SEED_COUNT);
        let floor = TimestampDuration::new_secs(1);

        let mut queue = make_queue(&registry, CONSENSUS_COUNT, None);
        queue.update(&node_refs, Timestamp::now_non_decreasing());

        // The closest two stay in flight, the rest of the window answers stale
        let before_dispatch = Timestamp::now_non_decreasing();
        let work_items = (0..CONSENSUS_WIDTH)
            .map(|_| take_work(&mut queue))
            .collect::<Vec<_>>();
        let after_dispatch = Timestamp::now_non_decreasing();
        for work_item in &work_items[2..] {
            queue.stale(work_item.node_ref.clone(), Timestamp::now_non_decreasing());
        }

        assert_eq!(queue.next_straggler_cut(2, floor), None);

        let cut = queue
            .next_straggler_cut(CONSENSUS_COUNT, floor)
            .expect("the stragglers are due a cut");
        assert!(
            cut >= before_dispatch.later(floor) && cut <= after_dispatch.later(floor),
            "cut {} outside the dispatch window",
            cut
        );
        assert_eq!(
            queue.time_out_stragglers(CONSENSUS_COUNT, floor, Timestamp::new(cut.as_u64() - 1)),
            0
        );
        assert!(queue.time_out_stragglers(CONSENSUS_COUNT, floor, cut) >= 1);

        // The cut lets a queued reserve node into the window, which is asked before another cut
        assert_eq!(queue.next_straggler_cut(CONSENSUS_COUNT, floor), None);
    }
    mock_registry::terminate(registry).await;
}
