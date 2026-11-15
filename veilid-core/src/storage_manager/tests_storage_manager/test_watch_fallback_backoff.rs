//! The fallback change inspection fires on SILENCE, and silence is the normal
//! condition of a record nobody is writing, so a flat interval inspects a quiet
//! watch forever. The interval has to grow while inspections keep coming back
//! empty, and snap back on evidence that the watch is live, has gained a node, or
//! has changed subkeys.
//!
//! These pin the pure state machine. The network behaviour it drives is covered by
//! veilid-python's watch battery.

use super::*;

/// The ceiling every assertion below is written against
const MAX_INTERVAL: TimestampDuration = TimestampDuration::new_secs(120);
/// First retry of a dead-route renewal: the default RPC timeout
const RETRY_BASE: TimestampDuration = TimestampDuration::new_secs(5);

fn make_params(subkeys: ValueSubkeyRangeSet) -> OutboundWatchParameters {
    OutboundWatchParameters {
        expiration: Timestamp::new(0),
        count: u32::MAX,
        subkeys,
        opt_watcher: None,
        safety_selection: SafetySelection::Unsafe(Sequencing::PreferOrdered),
    }
}

/// The default `get_value_count`
const CONSENSUS_COUNT: usize = 3;
/// The renew window `StorageManagerTimings` derives from the default get timeout
const RENEW_WINDOW: TimestampDuration = TimestampDuration::new_secs(30);

fn make_record_key() -> RecordKey {
    RecordKey::from_opaque(
        OpaqueRecordKey::new(CRYPTO_KIND_VLD0, BareOpaqueRecordKey::new(&[0u8; 32])),
        None,
    )
}

/// Per-node watches on `count` distinct nodes, all accepted until `expiration`
fn make_per_node_states(
    record_key: &RecordKey,
    count: u8,
    expiration: Timestamp,
) -> HashMap<PerNodeKey, OutboundWatchPerNodeState> {
    (1..=count)
        .map(|n| {
            (
                PerNodeKey {
                    record_key: record_key.clone(),
                    node_id: NodeId::new(CRYPTO_KIND_VLD0, BareNodeId::new(&[n; 32])),
                },
                OutboundWatchPerNodeState {
                    watch_id: n.into(),
                    safety_selection: SafetySelection::Unsafe(Sequencing::PreferOrdered),
                    opt_watcher: None,
                    expiration,
                    count: u32::MAX,
                    watch_node_ref: None,
                    opt_value_changed_route: None,
                },
            )
        })
        .collect()
}

/// An inspection answered with nothing newer doubles the wait, up to the ceiling.
fn test_backoff_doubles_until_capped() {
    let mut state = OutboundWatchState::new(make_params(ValueSubkeyRangeSet::single(0)));
    let per_node_states = HashMap::new();

    let base = state.fallback_inspect_interval(MAX_INTERVAL);
    assert_eq!(base, TimestampDuration::new_secs(30));

    // Each empty inspection doubles the interval
    let mut expected = base;
    for _ in 0..2 {
        state.edit(&per_node_states, |editor| {
            editor.back_off_fallback_inspect(MAX_INTERVAL)
        });
        expected = TimestampDuration::new(expected.as_u64() * 2);
        assert_eq!(state.fallback_inspect_interval(MAX_INTERVAL), expected);
    }

    // And stops at the ceiling rather than growing without bound
    for _ in 0..20 {
        state.edit(&per_node_states, |editor| {
            editor.back_off_fallback_inspect(MAX_INTERVAL)
        });
    }
    assert_eq!(state.fallback_inspect_interval(MAX_INTERVAL), MAX_INTERVAL);
}

/// The interval persists with the watch state, so one saved under an older, higher
/// ceiling must read as today's ceiling rather than hold the watch to it.
fn test_saved_interval_above_ceiling_reads_capped() {
    let per_node_states = HashMap::new();
    let mut state = OutboundWatchState::new(make_params(ValueSubkeyRangeSet::single(0)));
    state.edit(&per_node_states, |editor| {
        editor.back_off_fallback_inspect(MAX_INTERVAL)
    });

    let mut saved = serde_json::to_value(&state).unwrap();
    saved["opt_fallback_inspect_interval"] = serde_json::Value::from("600000000");
    let mut state: OutboundWatchState = serde_json::from_value(saved).unwrap();
    assert_eq!(state.fallback_inspect_interval(MAX_INTERVAL), MAX_INTERVAL);

    state.edit(&per_node_states, |editor| {
        editor.back_off_fallback_inspect(MAX_INTERVAL)
    });
    assert_eq!(state.fallback_inspect_interval(MAX_INTERVAL), MAX_INTERVAL);
}

/// A productive inspection, a gained node or changed subkeys could have hidden a missed update,
/// so the next miss is worth noticing promptly.
fn test_evidence_resets_backoff() {
    let per_node_states = HashMap::new();
    let mut state = OutboundWatchState::new(make_params(ValueSubkeyRangeSet::single(0)));
    state.edit(&per_node_states, |editor| {
        editor.back_off_fallback_inspect(MAX_INTERVAL);
        editor.back_off_fallback_inspect(MAX_INTERVAL);
    });
    assert_ne!(
        state.fallback_inspect_interval(MAX_INTERVAL),
        TimestampDuration::new_secs(30)
    );

    state.edit(&per_node_states, |editor| {
        editor.reset_fallback_inspect_backoff(None)
    });
    assert_eq!(
        state.fallback_inspect_interval(MAX_INTERVAL),
        TimestampDuration::new_secs(30),
        "evidence of a hidden miss should restore the base interval"
    );
}

/// A failure many watches share resets each to the base interval on its own phase, so their
/// inspections spread over the interval instead of all coming due when the failure lands.
fn test_shared_failure_spreads_inspections() {
    let per_node_states = HashMap::new();
    let secs = TimestampDuration::new_secs;
    let t0 = Timestamp::now_non_decreasing();
    let inspected = |last: u32| {
        let mut state = OutboundWatchState::new(make_params(ValueSubkeyRangeSet::single(0)));
        state.edit(&per_node_states, |editor| {
            editor.back_off_fallback_inspect(MAX_INTERVAL);
            editor.back_off_fallback_inspect(MAX_INTERVAL);
            editor.touch_fallback_inspect(t0.later(secs(last)));
        });
        state
    };
    let failed = t0.later(secs(175));
    let mut a = inspected(100);
    let mut b = inspected(110);
    for state in [&mut a, &mut b] {
        state.edit(&per_node_states, |editor| {
            editor.reset_fallback_inspect_backoff(Some(failed))
        });
        assert_eq!(
            state.fallback_inspect_interval(MAX_INTERVAL),
            secs(30),
            "the base interval is restored"
        );
        assert!(!state.needs_fallback_inspect(failed, MAX_INTERVAL));
    }
    // Each waits for the first multiple of the base interval after its last inspection
    assert!(!a.needs_fallback_inspect(t0.later(secs(189)), MAX_INTERVAL));
    assert!(a.needs_fallback_inspect(t0.later(secs(190)), MAX_INTERVAL));
    assert!(!b.needs_fallback_inspect(t0.later(secs(199)), MAX_INTERVAL));
    assert!(b.needs_fallback_inspect(t0.later(secs(200)), MAX_INTERVAL));

    // The inspection itself clears the wait, and a plain reset never sets one
    a.edit(&per_node_states, |editor| {
        editor.touch_fallback_inspect(t0.later(secs(190)))
    });
    assert!(a.needs_fallback_inspect(t0.later(secs(220)), MAX_INTERVAL));
    b.edit(&per_node_states, |editor| {
        editor.reset_fallback_inspect_backoff(None)
    });
    assert!(b.needs_fallback_inspect(failed, MAX_INTERVAL));
}

/// The stuck watch: at consensus with its fallback at the ceiling, pushes stop while the record
/// keeps changing. It inspects every ceiling until a push arrives, then is served again.
fn test_silent_served_watch_keeps_the_ceiling() {
    let record_key = make_record_key();
    let secs = TimestampDuration::new_secs;
    let mut state = OutboundWatchState::new(make_params(ValueSubkeyRangeSet::single(0)));
    let t0 = Timestamp::now_non_decreasing();
    let expiration = t0.later(secs(600));
    let per_node_states = make_per_node_states(&record_key, 3, expiration);
    let due = |state: &OutboundWatchState, at: Timestamp| {
        state.fallback_inspect_due(at, CONSENSUS_COUNT, RENEW_WINDOW, MAX_INTERVAL, None)
    };
    state.edit(&per_node_states, |editor| {
        editor.add_nodes(per_node_states.keys().cloned());
        for _ in 0..4 {
            editor.back_off_fallback_inspect(MAX_INTERVAL);
        }
        // Pushes worked up to here
        editor.touch_value_changed(t0);
    });
    // Served while its pushes arrive: one ceiling of silence is not enough
    assert!(!due(&state, t0.later(MAX_INTERVAL).later(secs(1))));

    // The renewal window's inspection finds a change nobody pushed
    let inspected = expiration.earlier(RENEW_WINDOW);
    assert!(due(&state, inspected));
    // and the watch renews for another lifetime
    let renewed = make_per_node_states(&record_key, 3, inspected.later(secs(600)));
    state.edit(&renewed, |editor| editor.touch_fallback_inspect(inspected));
    // From then on each ceiling of silence inspects, not each renewal
    let next = inspected.later(MAX_INTERVAL);
    assert!(!due(&state, next.earlier(secs(1))));
    assert!(due(&state, next));
    state.edit(&renewed, |editor| editor.touch_fallback_inspect(next));
    assert!(due(&state, next.later(MAX_INTERVAL)));

    // A push restores the served cadence
    let pushed = next.later(secs(5));
    state.edit(&renewed, |editor| editor.touch_value_changed(pushed));
    assert!(!due(&state, pushed.later(MAX_INTERVAL).later(secs(1))));
}

/// Under our own loss the next fallback inspection waits 1/survival times longer, fixed when it
/// is scheduled, so recovering does not bring every stretched watch due at once
fn test_loss_stretches_the_next_fallback() {
    let per_node_states = HashMap::new();
    let secs = TimestampDuration::new_secs;
    // After every state below is established, so the inspection is the reference
    let t0 = Timestamp::now_non_decreasing().later(secs(1));
    let inspected_at = |survival: f64| {
        let mut state = OutboundWatchState::new(make_params(ValueSubkeyRangeSet::single(0)));
        state.edit(&per_node_states, |editor| {
            editor.touch_fallback_inspect(t0);
            editor.reset_fallback_inspect_backoff(None);
            editor.stretch_fallback_inspect(survival, MAX_INTERVAL);
        });
        state
    };
    // Base interval at full survival
    let state = inspected_at(1.0);
    assert!(state.needs_fallback_inspect(t0.later(secs(30)), MAX_INTERVAL));
    // Doubled at half survival
    let state = inspected_at(0.5);
    assert!(!state.needs_fallback_inspect(t0.later(secs(59)), MAX_INTERVAL));
    assert!(state.needs_fallback_inspect(t0.later(secs(60)), MAX_INTERVAL));
    // Capped at the ceiling, and still waited out after the loss ends
    let state = inspected_at(0.1);
    assert!(!state.needs_fallback_inspect(t0.later(secs(119)), MAX_INTERVAL));
    assert!(state.needs_fallback_inspect(t0.later(MAX_INTERVAL), MAX_INTERVAL));
}

/// A delivery restarts the silence timer at the backed-off interval. Restoring the base interval
/// instead would make every record in a burst of deliveries re-check at the same moment.
fn test_delivery_keeps_the_backed_off_interval() {
    let per_node_states = HashMap::new();
    let mut state = OutboundWatchState::new(make_params(ValueSubkeyRangeSet::single(0)));
    state.edit(&per_node_states, |editor| {
        editor.back_off_fallback_inspect(MAX_INTERVAL);
        editor.back_off_fallback_inspect(MAX_INTERVAL);
    });
    let backed_off = state.fallback_inspect_interval(MAX_INTERVAL);
    assert_eq!(backed_off, MAX_INTERVAL);

    let delivered = Timestamp::now_non_decreasing().later(TimestampDuration::new_secs(100));
    state.edit(&per_node_states, |editor| {
        editor.touch_value_changed(delivered)
    });
    assert_eq!(state.fallback_inspect_interval(MAX_INTERVAL), backed_off);

    let one_sec = TimestampDuration::new_secs(1);
    assert!(
        !state.needs_fallback_inspect(delivered.later(backed_off).earlier(one_sec), MAX_INTERVAL)
    );
    assert!(state.needs_fallback_inspect(delivered.later(backed_off).later(one_sec), MAX_INTERVAL));
}

/// The backed-off interval is what `needs_fallback_inspect` actually waits for -
/// a backoff nothing consults would be decorative.
fn test_needs_fallback_inspect_honours_backoff() {
    let mut state = OutboundWatchState::new(make_params(ValueSubkeyRangeSet::single(0)));
    let per_node_states = HashMap::new();

    // Establishment stamps the reference time, so nothing is due immediately
    assert!(!state.needs_fallback_inspect(Timestamp::new(0), MAX_INTERVAL));

    // Far enough out, the base interval is due
    let base_due = Timestamp::now_non_decreasing().later(TimestampDuration::new_secs(31));
    assert!(state.needs_fallback_inspect(base_due, MAX_INTERVAL));

    // After one empty inspection the same moment is no longer due
    state.edit(&per_node_states, |editor| {
        editor.back_off_fallback_inspect(MAX_INTERVAL)
    });
    assert!(
        !state.needs_fallback_inspect(base_due, MAX_INTERVAL),
        "a backed-off watch should not be due at the base interval"
    );
    assert!(state.needs_fallback_inspect(
        Timestamp::now_non_decreasing().later(TimestampDuration::new_secs(61)),
        MAX_INTERVAL
    ));
}

/// A watch at consensus whose pushes arrive is being served: once its interval reaches the
/// ceiling, silence until its renewal window is expected, so it inspects once per renewal. Short
/// of consensus the ceiling stays, since there the fallback is how changes arrive at all.
fn test_served_watch_inspects_once_per_renewal() {
    let record_key = make_record_key();
    let mut state = OutboundWatchState::new(make_params(ValueSubkeyRangeSet::single(0)));
    let now = Timestamp::now_non_decreasing();
    let expiration = now.later(TimestampDuration::new_secs(600));
    let per_node_states = make_per_node_states(&record_key, 3, expiration);
    state.edit(&per_node_states, |editor| {
        editor.add_nodes(per_node_states.keys().cloned());
        for _ in 0..4 {
            editor.back_off_fallback_inspect(MAX_INTERVAL);
        }
        editor.touch_value_changed(now);
    });
    assert_eq!(state.fallback_inspect_interval(MAX_INTERVAL), MAX_INTERVAL);

    // Past the ceiling the interval alone says due, but a served watch waits for renewal
    let past_ceiling = now.later(MAX_INTERVAL.saturating_add(TimestampDuration::new_secs(1)));
    assert!(state.needs_fallback_inspect(past_ceiling, MAX_INTERVAL));
    assert!(!state.fallback_inspect_due(
        past_ceiling,
        CONSENSUS_COUNT,
        RENEW_WINDOW,
        MAX_INTERVAL,
        None
    ));

    // Its renewal window opens the inspection
    let in_renew_window = expiration.earlier(RENEW_WINDOW);
    assert!(state.fallback_inspect_due(
        in_renew_window,
        CONSENSUS_COUNT,
        RENEW_WINDOW,
        MAX_INTERVAL,
        None
    ));
    assert!(!state.fallback_inspect_due(
        in_renew_window.earlier(TimestampDuration::new_secs(1)),
        CONSENSUS_COUNT,
        RENEW_WINDOW,
        MAX_INTERVAL,
        None
    ));

    // Short of consensus the ceiling applies
    let mut short = state.clone();
    let mut kept = 0;
    short.edit(&per_node_states, |editor| {
        editor.retain_nodes(|_| {
            kept += 1;
            kept < CONSENSUS_COUNT
        })
    });
    assert_eq!(short.nodes().len(), CONSENSUS_COUNT - 1);
    assert!(short.fallback_inspect_due(
        past_ceiling,
        CONSENSUS_COUNT,
        RENEW_WINDOW,
        MAX_INTERVAL,
        None
    ));

    // While still backing off, the interval decides even at consensus
    let mut fresh = OutboundWatchState::new(make_params(ValueSubkeyRangeSet::single(0)));
    fresh.edit(&per_node_states, |editor| {
        editor.add_nodes(per_node_states.keys().cloned())
    });
    assert!(fresh.fallback_inspect_due(
        Timestamp::now_non_decreasing().later(TimestampDuration::new_secs(31)),
        CONSENSUS_COUNT,
        RENEW_WINDOW,
        MAX_INTERVAL,
        None
    ));

    // Without a push since its last inspection the ceiling applies
    let mut silent = state.clone();
    silent.edit(&per_node_states, |editor| {
        editor.touch_fallback_inspect(now.later(TimestampDuration::new_secs(1)))
    });
    let past_silent_ceiling =
        now.later(MAX_INTERVAL.saturating_add(TimestampDuration::new_secs(2)));
    assert!(silent.fallback_inspect_due(
        past_silent_ceiling,
        CONSENSUS_COUNT,
        RENEW_WINDOW,
        MAX_INTERVAL,
        None
    ));

    // A watch whose per-node watches are past expiration is no longer known to be served
    let lapsed_per_node_states = make_per_node_states(&record_key, 3, now);
    let mut lapsed = OutboundWatchState::new(make_params(ValueSubkeyRangeSet::single(0)));
    lapsed.edit(&lapsed_per_node_states, |editor| {
        editor.add_nodes(lapsed_per_node_states.keys().cloned());
        for _ in 0..4 {
            editor.back_off_fallback_inspect(MAX_INTERVAL);
        }
    });
    assert!(lapsed.fallback_inspect_due(
        past_ceiling,
        CONSENSUS_COUNT,
        RENEW_WINDOW,
        MAX_INTERVAL,
        None
    ));
}

/// A watch that is wanted but has never established has no state to keep a timer
/// in. It still needs covering, because the app asked for it and core owns
/// delivery, at the ceiling of an established watch short of consensus since
/// reconcile is already retrying.
fn test_unestablished_watch_is_inspected_at_the_ceiling() {
    let record_key = make_record_key();
    let subkeys = ValueSubkeyRangeSet::single(7);
    let mut outbound_watch = OutboundWatch::new(record_key, make_params(subkeys.clone()));
    let per_node_states = HashMap::new();

    // Never inspected, so it is due at once, over the DESIRED subkeys
    assert_eq!(
        outbound_watch.needs_fallback_inspect(
            Timestamp::new(0),
            CONSENSUS_COUNT,
            RENEW_WINDOW,
            MAX_INTERVAL,
            None
        ),
        Some(subkeys.clone())
    );

    let now = Timestamp::now_non_decreasing();
    outbound_watch.touch_fallback_inspect(&per_node_states, now);

    // Not due again at the established-watch cadence
    assert_eq!(
        outbound_watch.needs_fallback_inspect(
            now.later(TimestampDuration::new_secs(60)),
            CONSENSUS_COUNT,
            RENEW_WINDOW,
            MAX_INTERVAL,
            None
        ),
        None,
        "an unestablished watch should not inspect at the 30s cadence"
    );
    assert_eq!(
        outbound_watch.needs_fallback_inspect(
            now.later(TimestampDuration::new_secs(121)),
            CONSENSUS_COUNT,
            RENEW_WINDOW,
            MAX_INTERVAL,
            None
        ),
        Some(subkeys)
    );

    // A cancelled watch - no state and no desired - is inspected by nobody
    outbound_watch.set_desired(None);
    assert_eq!(
        outbound_watch.needs_fallback_inspect(
            now.later(TimestampDuration::new_secs(121)),
            CONSENSUS_COUNT,
            RENEW_WINDOW,
            MAX_INTERVAL,
            None
        ),
        None
    );
}

/// Offline, nothing gets through, so watch network work holds rather than spend
/// itself failing; degraded round trips still complete, so it runs.
fn test_watch_network_work_holds_only_while_offline() {
    assert!(StorageManager::holds_watch_network_work(
        OnlineState::Offline
    ));
    assert!(!StorageManager::holds_watch_network_work(
        OnlineState::Degraded
    ));
    assert!(!StorageManager::holds_watch_network_work(
        OnlineState::Online
    ));
}

/// A reconcile that nodes answered only with refusals (no copy, usually nothing
/// written yet) waits the slow interval; one nobody answered learned nothing about the
/// record and retries, as does any reconcile still short of consensus.
fn test_reconcile_retries_silence_not_refusals() {
    let retry = RECONCILE_OUTBOUND_WATCHES_RETRY_INTERVAL;
    let slow = RECONCILE_OUTBOUND_WATCHES_INTERVAL;
    let online = Some(1.0);
    assert_eq!(
        StorageManager::next_reconcile_interval(true, 0, 0, 0, online),
        (retry, 1)
    );
    assert_eq!(
        StorageManager::next_reconcile_interval(true, 0, 3, 4, online),
        (slow, 0)
    );
    assert_eq!(
        StorageManager::next_reconcile_interval(true, 2, 1, 0, online),
        (retry, 1)
    );
    assert_eq!(
        StorageManager::next_reconcile_interval(false, 5, 0, 4, online),
        (slow, 0)
    );
}

/// A watch that keeps ending short of consensus retries less often each time, and less often
/// still while our own link loses round trips
fn test_reconcile_retries_back_off() {
    let secs = TimestampDuration::new_secs;
    let mut retries = 0;
    let mut seen = Vec::new();
    for _ in 0..6 {
        let (interval, next) =
            StorageManager::next_reconcile_interval(true, 1, 0, retries, Some(1.0));
        seen.push(interval);
        retries = next;
    }
    assert_eq!(
        seen,
        vec![secs(5), secs(10), secs(20), secs(40), secs(60), secs(60)]
    );
    assert_eq!(
        StorageManager::next_reconcile_interval(true, 1, 0, 0, Some(0.25)),
        (secs(20), 1)
    );
    // Offline holds the work anyway; the retry waits the full interval
    assert_eq!(
        StorageManager::next_reconcile_interval(true, 1, 0, 0, None),
        (secs(60), 1)
    );
}

/// Renewals for a dead push route go out at once, then back off from one RPC timeout to the
/// fallback ceiling, and a death after a quiet ceiling starts over
fn test_route_renewals_are_paced() {
    let per_node_states = HashMap::new();
    let secs = TimestampDuration::new_secs;
    let mut state = OutboundWatchState::new(make_params(ValueSubkeyRangeSet::single(0)));
    let t0 = Timestamp::now_non_decreasing();
    let due = |state: &OutboundWatchState, at: Timestamp, survival: Option<f64>| {
        state.route_renewal_due(at, RETRY_BASE, MAX_INTERVAL, survival)
    };
    assert!(due(&state, t0, Some(1.0)));

    let renew = |state: &mut OutboundWatchState, at: Timestamp| {
        state.edit(&per_node_states, |editor| {
            editor.record_route_renewal(at, MAX_INTERVAL)
        });
    };
    renew(&mut state, t0);
    assert!(!due(&state, t0.later(secs(4)), Some(1.0)));
    assert!(due(&state, t0.later(secs(5)), Some(1.0)));
    // Under our own loss the wait stretches
    assert!(!due(&state, t0.later(secs(5)), Some(0.5)));
    assert!(due(&state, t0.later(secs(10)), Some(0.5)));
    // Offline sends nothing
    assert!(!due(&state, t0.later(secs(60)), None));

    // Each further renewal in the run doubles the wait
    renew(&mut state, t0.later(secs(5)));
    assert!(!due(&state, t0.later(secs(14)), Some(1.0)));
    assert!(due(&state, t0.later(secs(15)), Some(1.0)));

    // A quiet ceiling ends the run: the next death renews at once
    renew(&mut state, t0.later(secs(15)));
    let quiet = t0.later(secs(15)).later(MAX_INTERVAL).later(secs(1));
    assert!(due(&state, quiet, Some(1.0)));
    renew(&mut state, quiet);
    assert!(due(&state, quiet.later(secs(5)), Some(1.0)));
}

/// The fallback is a silence timer: a delivered change restarts it from the delivery, so a
/// record that went quiet after delivering is inspected one interval later, not never.
fn test_delivery_restarts_the_silence_timer() {
    let mut state = OutboundWatchState::new(make_params(ValueSubkeyRangeSet::single(0)));
    let per_node_states = HashMap::new();
    let base = TimestampDuration::new_secs(30);
    let one_sec = TimestampDuration::new_secs(1);

    // A change delivered well after establishment
    let delivered = Timestamp::now_non_decreasing().later(TimestampDuration::new_secs(100));
    state.edit(&per_node_states, |editor| {
        editor.touch_value_changed(delivered)
    });
    assert!(
        !state.needs_fallback_inspect(delivered.later(base).earlier(one_sec), MAX_INTERVAL),
        "silence shorter than the interval is not due"
    );
    assert!(
        state.needs_fallback_inspect(delivered.later(base).later(one_sec), MAX_INTERVAL),
        "one interval of silence after a delivery is due"
    );

    // A later fallback inspection moves the reference forward
    let inspected = delivered.later(TimestampDuration::new_secs(40));
    state.edit(&per_node_states, |editor| {
        editor.touch_fallback_inspect(inspected)
    });
    assert!(!state.needs_fallback_inspect(inspected.later(base).earlier(one_sec), MAX_INTERVAL));
    assert!(state.needs_fallback_inspect(inspected.later(base).later(one_sec), MAX_INTERVAL));

    // And so does a delivery after that inspection
    let delivered_again = inspected.later(TimestampDuration::new_secs(10));
    state.edit(&per_node_states, |editor| {
        editor.touch_value_changed(delivered_again)
    });
    assert!(
        !state.needs_fallback_inspect(delivered_again.later(base).earlier(one_sec), MAX_INTERVAL)
    );
    assert!(state.needs_fallback_inspect(delivered_again.later(base).later(one_sec), MAX_INTERVAL));
}

/// A productive fallback inspection must be able to force a reconcile. It stamps its time as it
/// fires, so the silence timer has restarted by the time it reports; only a push since then
/// says the watch still delivers.
fn test_unpushed_since_fallback() {
    let mut state = OutboundWatchState::new(make_params(ValueSubkeyRangeSet::single(0)));
    let per_node_states = HashMap::new();
    let base = Timestamp::now_non_decreasing();
    let one_sec = TimestampDuration::new_secs(1);
    assert!(!state.unpushed_since_fallback(), "no fallback has fired");

    state.edit(&per_node_states, |editor| editor.touch_value_changed(base));
    state.edit(&per_node_states, |editor| {
        editor.touch_fallback_inspect(base.later(one_sec))
    });
    assert!(
        !state.needs_fallback_inspect(base.later(one_sec).later(one_sec), MAX_INTERVAL),
        "the timer it just restarted cannot say a push was missed"
    );
    assert!(
        state.unpushed_since_fallback(),
        "nothing pushed since it fired"
    );

    state.edit(&per_node_states, |editor| {
        editor.touch_value_changed(base.later(one_sec).later(one_sec))
    });
    assert!(!state.unpushed_since_fallback(), "a push since it fired");
}

/// A loss period that ended after a served watch's last push leaves it unserved, since that push
/// may have lost its successors: it inspects from the loss end plus its spread, not at renewal.
/// A push after the loss serves it again.
fn test_loss_ends_served() {
    let secs = TimestampDuration::new_secs;
    let record_key = make_record_key();
    let mut state = OutboundWatchState::new(make_params(ValueSubkeyRangeSet::single(0)));
    let now = Timestamp::now_non_decreasing();
    let expiration = now.later(secs(600));
    let per_node_states = make_per_node_states(&record_key, 3, expiration);
    state.edit(&per_node_states, |editor| {
        editor.add_nodes(per_node_states.keys().cloned());
        for _ in 0..4 {
            editor.back_off_fallback_inspect(MAX_INTERVAL);
        }
        editor.touch_value_changed(now);
    });
    let due = |state: &OutboundWatchState,
               at: Timestamp,
               loss: Option<(Timestamp, TimestampDuration)>| {
        state.fallback_inspect_due(at, CONSENSUS_COUNT, RENEW_WINDOW, MAX_INTERVAL, loss)
    };

    let loss = Some((now.later(secs(100)), secs(60)));
    assert!(
        !due(&state, now.later(secs(121)), None),
        "served: waits for renewal"
    );
    assert!(
        !due(&state, now.later(secs(121)), loss),
        "not before its share of the interval"
    );
    assert!(due(&state, now.later(secs(161)), loss), "then inspects");

    state.edit(&per_node_states, |editor| {
        editor.touch_value_changed(now.later(secs(170)));
    });
    assert!(
        !due(&state, now.later(secs(292)), loss),
        "a push after the loss serves it again"
    );
}

pub fn test_all() {
    test_loss_ends_served();
    test_silent_served_watch_keeps_the_ceiling();
    test_loss_stretches_the_next_fallback();
    test_reconcile_retries_back_off();
    test_route_renewals_are_paced();
    test_shared_failure_spreads_inspections();
    test_unpushed_since_fallback();
    test_backoff_doubles_until_capped();
    test_saved_interval_above_ceiling_reads_capped();
    test_evidence_resets_backoff();
    test_delivery_keeps_the_backed_off_interval();
    test_needs_fallback_inspect_honours_backoff();
    test_delivery_restarts_the_silence_timer();
    test_served_watch_inspects_once_per_renewal();
    test_unestablished_watch_is_inspected_at_the_ceiling();
    test_watch_network_work_holds_only_while_offline();
    test_reconcile_retries_silence_not_refusals();
}
