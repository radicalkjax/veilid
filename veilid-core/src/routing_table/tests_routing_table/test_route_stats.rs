use super::mocks::fix_typed_node_id;
use crate::routing_table::*;
use crate::rpc_processor::Destination;
use crate::tests::mock_registry;

fn now() -> Timestamp {
    Timestamp::new(1_000_000_000u64)
}

fn only(ordering: SequenceOrdering) -> SequenceOrderingSet {
    SequenceOrderingSet::only(ordering)
}

fn dest_key(n: u8) -> PublicKey {
    PublicKey::new(CRYPTO_KIND_VLD0, BarePublicKey::new(&[n; 32]))
}

/// A destination whose key is `key`, as an answer or a loss names it
fn destination(key: &PublicKey) -> Destination {
    Destination::private_route(
        Arc::new(PrivateRoute::new_stub(
            key.clone(),
            RouteNode::NodeId(fix_typed_node_id(CRYPTO_KIND_VLD0, 1)),
        )),
        SafetySelection::Unsafe(Sequencing::PreferOrdered),
    )
}

pub fn test_loopback_failure_run_kills_ordering() {
    info!("--- test_loopback_failure_run_kills_ordering ---");
    let mut stats = RouteStats::new(now());
    let threshold = loopback_failure_threshold(SequenceOrdering::Unordered);

    for n in 1..threshold {
        let tally = stats.record_loopback_result(SequenceOrdering::Unordered, false, 1.0, now());
        assert_eq!(tally.streak, n);
        assert!(!stats.is_dead_for_ordering(SequenceOrdering::Unordered));
        // A failing ordering wants a retest
        assert!(stats.needs_testing(only(SequenceOrdering::Unordered), now()));
    }
    let tally = stats.record_loopback_result(SequenceOrdering::Unordered, false, 1.0, now());
    assert_eq!(tally.fail, threshold as u64);
    assert!(stats.is_dead_for_ordering(SequenceOrdering::Unordered));
    assert!(stats.is_dead_for(only(SequenceOrdering::Unordered)));

    // The other ordering is untouched and never counted
    assert!(!stats.is_dead_for_ordering(SequenceOrdering::Ordered));
    assert!(!stats.is_dead_for(only(SequenceOrdering::Ordered)));
}

pub fn test_loopback_pass_resets_run() {
    info!("--- test_loopback_pass_resets_run ---");
    let mut stats = RouteStats::new(now());
    let threshold = loopback_failure_threshold(SequenceOrdering::Ordered);

    for _ in 1..threshold {
        stats.record_loopback_result(SequenceOrdering::Ordered, false, 1.0, now());
    }
    let tally = stats.record_loopback_result(SequenceOrdering::Ordered, true, 1.0, now());
    // The pass reports the run it ended
    assert_eq!(tally.streak, threshold - 1);
    assert_eq!(tally.pass, 1);
    assert!(!stats.is_dead_for_ordering(SequenceOrdering::Ordered));
    assert!(!stats.needs_testing(only(SequenceOrdering::Ordered), now()));

    // A fresh run starts from zero
    let tally = stats.record_loopback_result(SequenceOrdering::Ordered, false, 1.0, now());
    assert_eq!(tally.streak, 1);
    assert!(!stats.is_dead_for_ordering(SequenceOrdering::Ordered));
}

pub fn test_send_does_not_clear_loopback_run() {
    info!("--- test_send_does_not_clear_loopback_run ---");
    let mut stats = RouteStats::new(now());
    let threshold = loopback_failure_threshold(SequenceOrdering::Ordered);

    for _ in 1..threshold {
        stats.record_loopback_result(SequenceOrdering::Ordered, false, 1.0, now());
        // Sending over the route is not evidence it works
        stats.record_sent(SequenceOrdering::Ordered, now(), ByteCount::new(100));
    }
    stats.record_loopback_result(SequenceOrdering::Ordered, false, 1.0, now());
    assert!(stats.is_dead_for_ordering(SequenceOrdering::Ordered));
}

pub fn test_send_failure_does_not_kill_the_route() {
    info!("--- test_send_failure_does_not_kill_the_route ---");
    // A send failure names us and the first hop, not the route
    let mut stats = RouteStats::new(now());
    stats.record_send_failed(SequenceOrdering::Ordered);
    assert!(!stats.is_dead_for(only(SequenceOrdering::Ordered)));
    assert!(!stats.death_blames_hops_for(only(SequenceOrdering::Ordered)));
    // but it is still a reason to recompile and retest
    assert!(stats
        .ordering_disposition(SequenceOrdering::Ordered)
        .needs_recompile());
    assert!(stats.needs_testing(only(SequenceOrdering::Ordered), now()));

    // A send failure must not pre-empt the loopback threshold
    let mut stats = RouteStats::new(now());
    stats.record_send_failed(SequenceOrdering::Ordered);
    stats.record_loopback_result(SequenceOrdering::Ordered, false, 1.0, now());
    assert!(
        !stats.is_dead_for(only(SequenceOrdering::Ordered)),
        "one bad loopback is not a dead route, whether or not a send failed"
    );

    // Evidence spanning the route kills it, and blames the hops
    let mut stats = RouteStats::new(now());
    for _ in 0..loopback_failure_threshold(SequenceOrdering::Ordered) {
        stats.record_loopback_result(SequenceOrdering::Ordered, false, 1.0, now());
    }
    assert!(stats.is_dead_for(only(SequenceOrdering::Ordered)));
    assert!(stats.death_blames_hops_for(only(SequenceOrdering::Ordered)));
    assert!(!stats.death_blames_hops_for(only(SequenceOrdering::Unordered)));
}

pub fn test_sequencing_narrows_to_route_orderings() {
    info!("--- test_sequencing_narrows_to_route_orderings ---");
    let ordered = only(SequenceOrdering::Ordered);
    let unordered = only(SequenceOrdering::Unordered);

    assert_eq!(
        Sequencing::PreferUnordered.for_route_orderings(ordered),
        Sequencing::PreferOrdered
    );
    assert_eq!(
        Sequencing::PreferUnordered.for_route_orderings(unordered),
        Sequencing::PreferUnordered
    );
    assert_eq!(
        Sequencing::PreferOrdered.for_route_orderings(unordered),
        Sequencing::PreferOrdered
    );
    assert_eq!(
        Sequencing::EnsureOrdered.for_route_orderings(unordered),
        Sequencing::EnsureOrdered
    );

    assert_eq!(
        Sequencing::PreferUnordered.preferred_ordering(),
        SequenceOrdering::Unordered
    );
    assert_eq!(
        Sequencing::PreferOrdered.preferred_ordering(),
        SequenceOrdering::Ordered
    );
    assert_eq!(
        Sequencing::EnsureOrdered.preferred_ordering(),
        SequenceOrdering::Ordered
    );
}

fn epoch(destination: u64, own: u64, online: u64) -> LossEpoch {
    LossEpoch {
        destination: Some(Timestamp::new(destination)),
        own: Some(Timestamp::new(own)),
        online,
    }
}

/// A lost question is remembered per destination and ordering, short of killing the route, while
/// the state it was lost in holds: a peer info change on either side or an online state change
/// leaves the mark behind
pub fn test_lost_question_mark_holds_in_its_epoch() {
    info!("--- test_lost_question_mark_holds_in_its_epoch ---");
    let ordering = SequenceOrdering::Unordered;
    let mut stats = RouteStats::new(now());
    let dest = dest_key(7);
    let other = dest_key(8);
    let lost_in = epoch(1, 1, 0);
    assert!(!stats.lost_question_to(ordering, &dest, &lost_in));

    stats.record_lost_question_to(ordering, dest.clone(), Some(lost_in), 1.0, now());
    assert!(stats.lost_question_to(ordering, &dest, &lost_in));
    assert!(!stats.lost_question_to(ordering, &other, &lost_in));
    assert!(!stats.lost_question_to(SequenceOrdering::Ordered, &dest, &lost_in));
    assert!(!stats.is_dead_for_ordering(ordering));
    assert!(stats.needs_testing(only(ordering), now()));

    assert!(
        !stats.lost_question_to(ordering, &dest, &epoch(2, 1, 0)),
        "the destination republished its peer info"
    );
    assert!(
        !stats.lost_question_to(ordering, &dest, &epoch(1, 2, 0)),
        "we republished our peer info"
    );
    assert!(
        !stats.lost_question_to(ordering, &dest, &epoch(1, 1, 1)),
        "the online state moved"
    );

    // A loss judged while blame is suppressed marks nothing
    stats.record_lost_question_to(ordering, other.clone(), None, 1.0, now());
    assert!(!stats.lost_question_to(ordering, &other, &lost_in));
}

/// Answers from other destinations and loopback passes say nothing about the hop's path to a
/// destination that lost a question, so only its own answer clears the mark; the route-death
/// tally still forgets on any answer
pub fn test_lost_destination_mark_outlives_other_answers() {
    info!("--- test_lost_destination_mark_outlives_other_answers ---");
    let ordering = SequenceOrdering::Unordered;
    let mut stats = RouteStats::new(now());
    let dest = dest_key(7);
    let other = dest_key(8);

    let lost_in = epoch(1, 1, 0);
    stats.record_lost_question_to(ordering, dest.clone(), Some(lost_in), 1.0, now());
    stats.record_answer_received(ordering, now(), ByteCount::new(100), &destination(&other));
    assert!(!stats.needs_testing(only(ordering), now()));
    assert!(
        stats.lost_question_to(ordering, &dest, &lost_in),
        "another destination's answer cleared the mark"
    );

    stats.record_loopback_result(ordering, true, 1.0, now());
    assert!(
        stats.lost_question_to(ordering, &dest, &lost_in),
        "a loopback pass cleared the mark"
    );

    stats.record_answer_received(ordering, now(), ByteCount::new(100), &destination(&dest));
    assert!(!stats.lost_question_to(ordering, &dest, &lost_in));
}

/// A dead route's hops stay out of its orderings' allocations while the state each was lost in
/// holds; a death on a changed network forgets the older hops, and only the latest deaths are kept
pub async fn test_dead_route_hops_hold_in_their_epoch() {
    info!("--- test_dead_route_hops_hold_in_their_epoch ---");
    let registry = mock_registry::init("dead_route_hops").await;
    let routing_table = registry.routing_table();
    let rss = routing_table.route_spec_store();
    let ordered = SequenceOrdering::Ordered;
    let current = |hops: Vec<(NodeId, LossEpoch)>| {
        move |node_id: &NodeId| {
            hops.iter()
                .find(|(id, _)| id == node_id)
                .map(|(_, lost)| *lost)
        }
    };

    let a = (fix_typed_node_id(CRYPTO_KIND_VLD0, 1), epoch(1, 1, 0));
    let b = (fix_typed_node_id(CRYPTO_KIND_VLD0, 2), epoch(2, 1, 0));
    rss.avoid_dead_route_hops(only(ordered), &[a.clone(), b.clone()]);

    let mut avoided = rss.dead_route_hops_to_avoid(ordered, current(vec![a.clone(), b.clone()]));
    avoided.sort();
    assert_eq!(avoided, vec![a.0.clone(), b.0.clone()]);
    assert!(rss
        .dead_route_hops_to_avoid(
            SequenceOrdering::Unordered,
            current(vec![a.clone(), b.clone()])
        )
        .is_empty());

    // b republished its peer info, and a left the table
    let b_republished = (b.0.clone(), epoch(3, 1, 0));
    assert!(rss
        .dead_route_hops_to_avoid(ordered, current(vec![b_republished]))
        .is_empty());

    // A death on a changed network forgets the hops lost on the old one
    let c = (fix_typed_node_id(CRYPTO_KIND_VLD0, 3), epoch(5, 2, 0));
    rss.avoid_dead_route_hops(only(ordered), std::slice::from_ref(&c));
    assert_eq!(
        rss.dead_route_hops_to_avoid(ordered, current(vec![a.clone(), b.clone(), c.clone()])),
        vec![c.0.clone()]
    );

    // A background pool's worth of hops at the longest hop count, latest first
    let capacity = DESIRED_BACKGROUND_SAFETY_ROUTE_COUNT
        * usize::from(registry.config().internal().network.rpc.max_route_hop_count);
    let many = (0..=capacity)
        .map(|n| {
            let n = u8::try_from(n + 10).expect("small");
            (
                fix_typed_node_id(CRYPTO_KIND_VLD0, n),
                epoch(u64::from(n), 2, 0),
            )
        })
        .collect::<Vec<_>>();
    for hop in &many {
        rss.avoid_dead_route_hops(only(ordered), std::slice::from_ref(hop));
    }
    let mut everything = many.clone();
    everything.push(c.clone());
    let avoided = rss.dead_route_hops_to_avoid(ordered, current(everything));
    assert_eq!(avoided.len(), capacity);
    assert!(
        !avoided.contains(&c.0),
        "the oldest death is forgotten first"
    );
    assert!(!avoided.contains(&many[0].0));
    assert!(avoided.contains(&many[capacity].0));

    mock_registry::terminate(registry).await;
}

pub fn test_failures_under_our_loss_carry_less() {
    info!("--- test_failures_under_our_loss_carry_less ---");
    // At survival 0.25 an Ordered run needs four times the failures
    let ordering = SequenceOrdering::Ordered;
    let threshold = loopback_failure_threshold(ordering);
    let mut stats = RouteStats::new(now());
    for _ in 1..threshold * 4 {
        stats.record_loopback_result(ordering, false, 0.25, now());
        assert!(!stats.is_dead_for_ordering(ordering));
    }
    stats.record_loopback_result(ordering, false, 0.25, now());
    assert!(stats.is_dead_for_ordering(ordering));
    assert!(stats.death_blames_hops_for(only(ordering)));

    // A run gathered while degraded is not condemned at full weight once we recover
    let mut stats = RouteStats::new(now());
    for _ in 0..6 {
        stats.record_loopback_result(ordering, false, 0.25, now());
    }
    stats.record_loopback_result(ordering, false, 1.0, now());
    assert!(!stats.is_dead_for_ordering(ordering));
    stats.record_loopback_result(ordering, false, 1.0, now());
    assert!(stats.is_dead_for_ordering(ordering));

    // Failures nothing could tell from our own loss carry nothing
    let mut stats = RouteStats::new(now());
    for _ in 0..100 {
        stats.record_loopback_result(ordering, false, 0.0, now());
    }
    assert!(!stats.is_dead_for_ordering(ordering));
    assert!(stats.needs_testing(only(ordering), now()));
}

pub fn test_lost_destinations_count_by_evidence() {
    info!("--- test_lost_destinations_count_by_evidence ---");
    let ordering = SequenceOrdering::Ordered;
    let count = ROUTE_LOST_DESTINATIONS_INVALIDATE_THRESHOLD as u8;

    let mut stats = RouteStats::new(now());
    for n in 0..count {
        stats.record_lost_question_to(ordering, dest_key(n), None, 0.5, now());
    }
    assert!(!stats.is_dead_for_ordering(ordering));
    for n in 0..count {
        stats.record_lost_question_to(ordering, dest_key(n), None, 0.5, now());
    }
    assert!(stats.is_dead_for_ordering(ordering));

    // One destination lost many times is still one destination
    let mut stats = RouteStats::new(now());
    for _ in 0..20 {
        stats.record_lost_question_to(ordering, dest_key(1), None, 1.0, now());
    }
    assert!(!stats.is_dead_for_ordering(ordering));

    // A loopback pass clears the evidence with the destinations
    for n in 0..count {
        stats.record_lost_question_to(ordering, dest_key(n), None, 1.0, now());
    }
    assert!(stats.is_dead_for_ordering(ordering));
    stats.record_loopback_result(ordering, true, 1.0, now());
    assert!(!stats.is_dead_for_ordering(ordering));
}

pub fn test_losses_sent_before_an_answer_carry_nothing() {
    info!("--- test_losses_sent_before_an_answer_carry_nothing ---");
    // A fanout's answers arrive in a second and its losses at the timeout; the route answered
    // after those questions left, so their losses say nothing against it
    let ordering = SequenceOrdering::Ordered;
    let count = ROUTE_LOST_DESTINATIONS_INVALIDATE_THRESHOLD as u8;
    let sent = now();
    let answered = sent.later(TimestampDuration::new_secs(1));
    let mut stats = RouteStats::new(now());
    stats.record_answer_received(
        ordering,
        answered,
        ByteCount::new(0),
        &destination(&dest_key(99)),
    );
    for n in 0..count * 2 {
        stats.record_lost_question_to(ordering, dest_key(n), None, 1.0, sent);
    }
    assert!(!stats.is_dead_for_ordering(ordering));
    // Sent after the answer, the same losses kill it
    for n in 0..count {
        stats.record_lost_question_to(
            ordering,
            dest_key(n),
            None,
            1.0,
            answered.later(TimestampDuration::new_secs(1)),
        );
    }
    assert!(stats.is_dead_for_ordering(ordering));
}

pub fn test_all() {
    test_losses_sent_before_an_answer_carry_nothing();
    test_failures_under_our_loss_carry_less();
    test_lost_destinations_count_by_evidence();
    test_lost_question_mark_holds_in_its_epoch();
    test_lost_destination_mark_outlives_other_answers();
    test_send_failure_does_not_kill_the_route();
    test_loopback_failure_run_kills_ordering();
    test_loopback_pass_resets_run();
    test_send_does_not_clear_loopback_run();
    test_sequencing_narrows_to_route_orderings();
}
