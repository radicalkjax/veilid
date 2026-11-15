use crate::routing_table::*;

// 'Now' far enough from zero to subtract spans without underflow.
fn now() -> Timestamp {
    Timestamp::new(1_000_000_000u64) // 1000s, in microseconds
}
// A timestamp `secs` seconds before now().
fn secs_ago(secs: u64) -> Timestamp {
    Timestamp::new(now().as_u64() - secs * 1_000_000u64)
}
// Whole seconds in a span duration.
fn span_secs(d: TimestampDuration) -> u64 {
    d.as_u64() / 1_000_000u64
}
// The liveness spans at the default RPC timeout.
fn spans() -> LivenessSpans {
    LivenessSpans::new(TimestampDuration::new_ms(
        VeilidConfigInternalRPC::default().timeout_ms.into(),
    ))
}
fn compute(
    punishment: Option<PunishmentReason>,
    rpc_stats: &RPCStats,
    per_so: &BTreeMap<SequenceOrdering, RPCStats>,
    per_transport: &BTreeMap<TransportType, RPCStats>,
    cm_viability: Option<&CMViability>,
) -> BucketEntryStateReason {
    BucketEntryInner::compute_state_reason_from_stats(
        now(),
        &spans(),
        punishment,
        rpc_stats,
        per_so,
        per_transport,
        cm_viability,
        |_, _| TimestampDuration::new(0),
    )
}
// As `compute`, but credits `offline_secs` of untestable time to every window.
fn compute_with_offline(
    punishment: Option<PunishmentReason>,
    rpc_stats: &RPCStats,
    per_so: &BTreeMap<SequenceOrdering, RPCStats>,
    per_transport: &BTreeMap<TransportType, RPCStats>,
    cm_viability: Option<&CMViability>,
    offline_secs: u32,
) -> BucketEntryStateReason {
    BucketEntryInner::compute_state_reason_from_stats(
        now(),
        &spans(),
        punishment,
        rpc_stats,
        per_so,
        per_transport,
        cm_viability,
        |_, _| TimestampDuration::new_secs(offline_secs),
    )
}
fn so_map(entries: &[(SequenceOrdering, RPCStats)]) -> BTreeMap<SequenceOrdering, RPCStats> {
    entries.iter().cloned().collect()
}
fn udp4() -> TransportType {
    TransportType::new(ProtocolType::UDP, AddressType::IPV4)
}
fn udp6() -> TransportType {
    TransportType::new(ProtocolType::UDP, AddressType::IPV6)
}
fn tcp4() -> TransportType {
    TransportType::new(ProtocolType::TCP, AddressType::IPV4)
}
// Per-transport stats that judge as Failing for liveness (unanswered questions)
fn failing_stats() -> RPCStats {
    RPCStats {
        recent_lost_questions: TRANSPORT_FAILING_LOST_QUESTIONS,
        ..Default::default()
    }
}
// Send failures alone: Failing for selection, never for liveness
fn send_failing_stats() -> RPCStats {
    RPCStats {
        failed_to_send: TRANSPORT_FAILING_FAILED_TO_SEND,
        ..Default::default()
    }
}
fn transport_map(entries: &[(TransportType, RPCStats)]) -> BTreeMap<TransportType, RPCStats> {
    entries.iter().cloned().collect()
}
// A non-empty per-transport map; contents are irrelevant to [D3]/[D4], which key on
// node-level last_seen_ts (traffic FROM the node), not dial evidence.
fn seen_transports() -> BTreeMap<TransportType, RPCStats> {
    transport_map(&[(udp4(), RPCStats::default())])
}
fn viability(entries: &[(SequenceOrdering, &[TransportType], bool)]) -> CMViability {
    CMViability {
        per_ordering: entries
            .iter()
            .map(|(so, transports, has_indirect)| {
                (
                    *so,
                    OrderingReachability {
                        direct_transports: transports.iter().cloned().collect(),
                        has_indirect: *has_indirect,
                    },
                )
            })
            .collect(),
        basis: BTreeMap::new(),
    }
}

pub fn test_state_punished() {
    info!("--- test_state_punished ---");

    // [P1] punished beats everything, even an otherwise-reliable node
    let reliable = so_map(&[(
        SequenceOrdering::Unordered,
        RPCStats {
            first_steady_answer_ts: Some(secs_ago(120)),
            ..Default::default()
        },
    )]);
    assert_eq!(
        compute(
            Some(PunishmentReason::Manual),
            &RPCStats::default(),
            &reliable,
            &seen_transports(),
            None
        ),
        BucketEntryStateReason::Punished(PunishmentReason::Manual)
    );
}

pub fn test_state_dead_excessive_unreachable() {
    info!("--- test_state_dead_excessive_unreachable ---");

    // [D1] a single node-level unreachable is dead. This also shadows [M1]
    // (unreachable > 0) while DEAD_UNREACHABLE_COUNT == 1.
    let rpc = RPCStats {
        unreachable: DEAD_UNREACHABLE_COUNT,
        ..Default::default()
    };
    assert_eq!(
        compute(None, &rpc, &BTreeMap::new(), &BTreeMap::new(), None),
        BucketEntryStateReason::Dead(BucketEntryStateDeadReason::ExcessiveUnreachable)
    );
}

pub fn test_state_dead_excessive_send_failures() {
    info!("--- test_state_dead_excessive_send_failures ---");

    // [D2] every ordering's only avenue failing is dead
    let v = viability(&[
        (SequenceOrdering::Unordered, &[udp6()], false),
        (SequenceOrdering::Ordered, &[tcp4()], false),
    ]);
    let per_transport = transport_map(&[(udp6(), failing_stats()), (tcp4(), failing_stats())]);
    assert_eq!(
        compute(
            None,
            &RPCStats::default(),
            &BTreeMap::new(),
            &per_transport,
            Some(&v)
        ),
        BucketEntryStateReason::Dead(BucketEntryStateDeadReason::ExcessiveSendFailures)
    );

    // a single-ordering environment (WS-only wasm peer) dies on that ordering alone
    let v = viability(&[(SequenceOrdering::Ordered, &[tcp4()], false)]);
    let per_transport = transport_map(&[(tcp4(), failing_stats())]);
    assert_eq!(
        compute(
            None,
            &RPCStats::default(),
            &BTreeMap::new(),
            &per_transport,
            Some(&v)
        ),
        BucketEntryStateReason::Dead(BucketEntryStateDeadReason::ExcessiveSendFailures)
    );
}

pub fn test_state_viable_ordering_vetoes_death() {
    info!("--- test_state_viable_ordering_vetoes_death ---");

    // one healthy ordering keeps the node alive despite the other fully failing;
    // the failing ordering makes it Missing, not Dead
    let v = viability(&[
        (SequenceOrdering::Unordered, &[udp6()], false),
        (SequenceOrdering::Ordered, &[tcp4()], false),
    ]);
    let per_transport = transport_map(&[(udp6(), failing_stats()), (tcp4(), RPCStats::default())]);
    assert_eq!(
        compute(
            None,
            &RPCStats::default(),
            &BTreeMap::new(),
            &per_transport,
            Some(&v)
        ),
        BucketEntryStateReason::Missing(BucketEntryStateMissingReason::FailedToSend)
    );

    // an Untried transport in the same ordering vetoes that ordering's failure:
    // alternates must accumulate evidence before the node can die
    let v = viability(&[(SequenceOrdering::Unordered, &[udp6(), udp4()], false)]);
    let per_transport = transport_map(&[(udp6(), failing_stats())]);
    assert_eq!(
        compute(
            None,
            &RPCStats::default(),
            &BTreeMap::new(),
            &per_transport,
            Some(&v)
        ),
        BucketEntryStateReason::Initial
    );

    // an indirect contact method (relay/signal/existing flow) vetoes send-failure
    // death for its ordering; relay collapse is covered by [D1] unreachable
    let v = viability(&[(SequenceOrdering::Unordered, &[udp6()], true)]);
    let per_transport = transport_map(&[(udp6(), failing_stats())]);
    assert_eq!(
        compute(
            None,
            &RPCStats::default(),
            &BTreeMap::new(),
            &per_transport,
            Some(&v)
        ),
        BucketEntryStateReason::Initial
    );
}

pub fn test_state_send_failures_never_kill() {
    info!("--- test_state_send_failures_never_kill ---");

    // Failures to send are our own fault: they steer selection but must never
    // make a node Missing or Dead, even as its only transport
    let v = viability(&[(SequenceOrdering::Unordered, &[udp6()], false)]);
    let per_transport = transport_map(&[(udp6(), send_failing_stats())]);
    assert_eq!(
        compute(
            None,
            &RPCStats::default(),
            &BTreeMap::new(),
            &per_transport,
            Some(&v)
        ),
        BucketEntryStateReason::Initial
    );
    // Selection still judges that transport unusable
    assert!(transport_health(&send_failing_stats()).is_failing());
}

pub fn test_state_dead_never_seen_lost_questions() {
    info!("--- test_state_dead_never_seen_lost_questions ---");

    // [D3] never heard from (last_seen_ts None) with enough node-level lost questions is dead
    let rpc = RPCStats {
        recent_lost_questions: DEAD_NEVER_SEEN_LOST_QUESTIONS,
        ..Default::default()
    };
    assert_eq!(
        compute(None, &rpc, &BTreeMap::new(), &BTreeMap::new(), None),
        BucketEntryStateReason::Dead(BucketEntryStateDeadReason::NeverSeenLostQuestions)
    );

    // dial evidence (per-transport buckets exist from sends) does not shield it:
    // a dialed-but-silent node still dies at the [D3] threshold, not the [D4] span
    assert_eq!(
        compute(None, &rpc, &BTreeMap::new(), &seen_transports(), None),
        BucketEntryStateReason::Dead(BucketEntryStateDeadReason::NeverSeenLostQuestions)
    );
}

pub fn test_state_dead_steady_lost_questions() {
    info!("--- test_state_dead_steady_lost_questions ---");

    // [D4] heard from before, node-level steadily losing questions past the span, is dead
    let rpc = RPCStats {
        last_seen_ts: Some(secs_ago(span_secs(spans().dead_lost_question) + 2)),
        first_steady_lost_question_ts: Some(secs_ago(span_secs(spans().dead_lost_question) + 1)),
        ..Default::default()
    };
    assert_eq!(
        compute(None, &rpc, &BTreeMap::new(), &seen_transports(), None),
        BucketEntryStateReason::Dead(BucketEntryStateDeadReason::SteadyLostQuestions)
    );

    // boundary: a steady lost question newer than the span is not yet dead
    let rpc = RPCStats {
        last_seen_ts: Some(secs_ago(span_secs(spans().dead_lost_question) + 2)),
        first_steady_lost_question_ts: Some(secs_ago(span_secs(spans().dead_lost_question) - 1)),
        ..Default::default()
    };
    assert_ne!(
        BucketEntryState::from(compute(
            None,
            &rpc,
            &BTreeMap::new(),
            &seen_transports(),
            None
        )),
        BucketEntryState::Dead,
        "should not be dead under the span"
    );
}

pub fn test_state_missing_failed_to_send() {
    info!("--- test_state_missing_failed_to_send ---");

    // [M2] one fully-failing ordering with another still viable is missing
    let v = viability(&[
        (SequenceOrdering::Unordered, &[udp6()], false),
        (SequenceOrdering::Ordered, &[tcp4()], false),
    ]);
    let per_transport = transport_map(&[(udp6(), failing_stats()), (tcp4(), RPCStats::default())]);
    assert_eq!(
        compute(
            None,
            &RPCStats::default(),
            &BTreeMap::new(),
            &per_transport,
            Some(&v)
        ),
        BucketEntryStateReason::Missing(BucketEntryStateMissingReason::FailedToSend)
    );

    // below the transport-failing threshold no ordering is failing
    let per_transport = transport_map(&[(
        udp6(),
        RPCStats {
            recent_lost_questions: TRANSPORT_FAILING_LOST_QUESTIONS - 1,
            ..Default::default()
        },
    )]);
    assert_eq!(
        compute(
            None,
            &RPCStats::default(),
            &BTreeMap::new(),
            &per_transport,
            Some(&v)
        ),
        BucketEntryStateReason::Initial
    );
}

pub fn test_state_missing_lost_questions() {
    info!("--- test_state_missing_lost_questions ---");

    // [M3] heard from before, node-level lost questions at the missing threshold but no
    // steady span yet, is missing; last_seen_ts is set so [D3] does not apply
    let rpc = RPCStats {
        last_seen_ts: Some(secs_ago(30)),
        recent_lost_questions: MISSING_LOST_QUESTIONS,
        first_steady_lost_question_ts: None,
        ..Default::default()
    };
    assert_eq!(
        compute(None, &rpc, &BTreeMap::new(), &seen_transports(), None),
        BucketEntryStateReason::Missing(BucketEntryStateMissingReason::LostQuestions)
    );
}

pub fn test_state_revive_reaches_initial() {
    info!("--- test_state_revive_reaches_initial ---");

    // revive() clears node-level and per-ordering stats but keeps transport
    // evidence; that field state must compute Initial or revival is a no-op
    let rpc = RPCStats {
        last_seen_ts: Some(secs_ago(30)),
        recent_lost_questions: MISSING_LOST_QUESTIONS,
        first_steady_lost_question_ts: None,
        ..Default::default()
    };
    assert_eq!(
        compute(None, &rpc, &BTreeMap::new(), &seen_transports(), None),
        BucketEntryStateReason::Missing(BucketEntryStateMissingReason::LostQuestions)
    );
    let revived = RPCStats {
        recent_lost_questions: 0,
        ..rpc
    };
    assert_eq!(
        compute(None, &revived, &BTreeMap::new(), &seen_transports(), None),
        BucketEntryStateReason::Initial
    );
}

pub fn test_state_initial() {
    info!("--- test_state_initial ---");

    // [I1] cold start (no per-sequence-ordering stats at all) is initial
    assert_eq!(
        compute(
            None,
            &RPCStats::default(),
            &BTreeMap::new(),
            &BTreeMap::new(),
            None
        ),
        BucketEntryStateReason::Initial
    );

    // a sequence ordering tracked but never answered is still initial
    let per_so = so_map(&[(SequenceOrdering::Unordered, RPCStats::default())]);
    assert_eq!(
        compute(
            None,
            &RPCStats::default(),
            &per_so,
            &seen_transports(),
            None
        ),
        BucketEntryStateReason::Initial
    );
}

pub fn test_state_unreliable() {
    info!("--- test_state_unreliable ---");

    // [U1] a recent first steady answer (within the span) is unreliable
    let per_so = so_map(&[(
        SequenceOrdering::Unordered,
        RPCStats {
            first_steady_answer_ts: Some(secs_ago(span_secs(spans().unreliable_answer) - 1)),
            ..Default::default()
        },
    )]);
    assert_eq!(
        compute(
            None,
            &RPCStats::default(),
            &per_so,
            &seen_transports(),
            None
        ),
        BucketEntryStateReason::Unreliable
    );
}

pub fn test_state_reliable() {
    info!("--- test_state_reliable ---");

    // [R1] a first steady answer older than the span, with no failures, is reliable
    let per_so = so_map(&[(
        SequenceOrdering::Unordered,
        RPCStats {
            first_steady_answer_ts: Some(secs_ago(span_secs(spans().unreliable_answer) + 1)),
            ..Default::default()
        },
    )]);
    assert_eq!(
        compute(
            None,
            &RPCStats::default(),
            &per_so,
            &seen_transports(),
            None
        ),
        BucketEntryStateReason::Reliable
    );
}

pub fn test_state_precedence_dead_beats_reliable() {
    info!("--- test_state_precedence_dead_beats_reliable ---");

    // all orderings failing kills the node even if its answer history is reliable
    let per_so = so_map(&[(
        SequenceOrdering::Unordered,
        RPCStats {
            first_steady_answer_ts: Some(secs_ago(120)),
            ..Default::default()
        },
    )]);
    let v = viability(&[(SequenceOrdering::Unordered, &[udp6()], false)]);
    let per_transport = transport_map(&[(udp6(), failing_stats())]);
    assert_eq!(
        compute(
            None,
            &RPCStats::default(),
            &per_so,
            &per_transport,
            Some(&v)
        ),
        BucketEntryStateReason::Dead(BucketEntryStateDeadReason::ExcessiveSendFailures)
    );
}

pub fn test_state_transition_reliable_degrades_to_missing() {
    info!("--- test_state_transition_reliable_degrades_to_missing ---");

    // a previously-reliable node that starts losing questions degrades to missing
    let per_so = so_map(&[(
        SequenceOrdering::Unordered,
        RPCStats {
            first_steady_answer_ts: Some(secs_ago(120)),
            ..Default::default()
        },
    )]);
    let rpc = RPCStats {
        last_seen_ts: Some(secs_ago(30)),
        recent_lost_questions: MISSING_LOST_QUESTIONS,
        ..Default::default()
    };
    assert_eq!(
        compute(None, &rpc, &per_so, &seen_transports(), None),
        BucketEntryStateReason::Missing(BucketEntryStateMissingReason::LostQuestions)
    );
}

pub fn test_state_offline_credit_steady_lost_questions() {
    info!("--- test_state_offline_credit_steady_lost_questions ---");

    // [D4] a node losing questions past the dead span is Dead by raw time.
    let rpc = RPCStats {
        last_seen_ts: Some(secs_ago(span_secs(spans().dead_lost_question) + 20)),
        first_steady_lost_question_ts: Some(secs_ago(span_secs(spans().dead_lost_question) + 10)),
        recent_lost_questions: MISSING_LOST_QUESTIONS,
        ..Default::default()
    };
    assert_eq!(
        compute(None, &rpc, &BTreeMap::new(), &seen_transports(), None),
        BucketEntryStateReason::Dead(BucketEntryStateDeadReason::SteadyLostQuestions)
    );
    // Crediting offline time pulls the effective span under the threshold: not Dead.
    assert_eq!(
        compute_with_offline(None, &rpc, &BTreeMap::new(), &seen_transports(), None, 20),
        BucketEntryStateReason::Missing(BucketEntryStateMissingReason::LostQuestions)
    );
    // A genuinely long lost span stays Dead despite the same offline credit.
    let long_lost = RPCStats {
        last_seen_ts: Some(secs_ago(span_secs(spans().dead_lost_question) + 220)),
        first_steady_lost_question_ts: Some(secs_ago(span_secs(spans().dead_lost_question) + 200)),
        recent_lost_questions: MISSING_LOST_QUESTIONS,
        ..Default::default()
    };
    assert_eq!(
        compute_with_offline(
            None,
            &long_lost,
            &BTreeMap::new(),
            &seen_transports(),
            None,
            20
        ),
        BucketEntryStateReason::Dead(BucketEntryStateDeadReason::SteadyLostQuestions)
    );
}

pub fn test_state_offline_credit_unreliable_answer() {
    info!("--- test_state_offline_credit_unreliable_answer ---");

    // [R1] a node answering past the unreliable span is Reliable by raw time.
    let per_so = so_map(&[(
        SequenceOrdering::Ordered,
        RPCStats {
            first_steady_answer_ts: Some(secs_ago(span_secs(spans().unreliable_answer) + 10)),
            ..Default::default()
        },
    )]);
    assert_eq!(
        compute(
            None,
            &RPCStats::default(),
            &per_so,
            &seen_transports(),
            None
        ),
        BucketEntryStateReason::Reliable
    );
    // Crediting offline time pulls the effective answer span under the threshold:
    // still Unreliable (we don't grant reliability for time we couldn't test).
    assert_eq!(
        compute_with_offline(
            None,
            &RPCStats::default(),
            &per_so,
            &seen_transports(),
            None,
            20
        ),
        BucketEntryStateReason::Unreliable
    );
}
