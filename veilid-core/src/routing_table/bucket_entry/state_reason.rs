use super::*;

impl_veilid_log_facility!("rtab");

//////////////////////////////////////////////////////////////////////////////////////////
// Per-transport health

/// Consecutive send failures on one transport before it is judged Failing
pub(crate) const TRANSPORT_FAILING_FAILED_TO_SEND: u32 = 3;
/// Consecutive lost questions on one transport before it is judged Failing
pub(crate) const TRANSPORT_FAILING_LOST_QUESTIONS: u32 = 3;

/// Evidence judgment for one node over one transport.
///
/// Untried is viable: it can neither disqualify a contact method nor contribute
/// to node death, so alternates must be tried before a node dies.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub(crate) enum TransportHealth {
    /// No attempts recorded
    Untried,
    /// No recent evidence of failure
    Healthy,
    /// Recent consecutive failures
    Failing,
}

impl TransportHealth {
    pub fn is_failing(&self) -> bool {
        matches!(self, TransportHealth::Failing)
    }
}

/// Which credit a time span gets when we could not judge the node
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub(crate) enum TimeCredit {
    /// Offline: no test was possible, so the span neither matures nor condemns
    Untestable,
    /// Offline or degraded: the online detector suppressed blame, so the span cannot condemn
    Unblamed,
}

/// Judge one transport's health for contact method selection. Send failures count
/// here: whatever caused them, this edge is not working.
pub(crate) fn transport_health(stats: &RPCStats) -> TransportHealth {
    if stats.failed_to_send >= TRANSPORT_FAILING_FAILED_TO_SEND
        || stats.recent_lost_questions >= TRANSPORT_FAILING_LOST_QUESTIONS
    {
        TransportHealth::Failing
    } else {
        TransportHealth::Healthy
    }
}

/// Judge one transport's health for node liveness. Only unanswered questions count:
/// a failure to send never left our machine and says nothing about the peer.
pub(crate) fn transport_liveness_health(stats: &RPCStats) -> TransportHealth {
    if stats.recent_lost_questions >= TRANSPORT_FAILING_LOST_QUESTIONS {
        TransportHealth::Failing
    } else {
        TransportHealth::Healthy
    }
}

//////////////////////////////////////////////////////////////////////////////////////////
// Reliable state thresholds

/// Reliable pings are done with increased spacing between pings
///
/// - Start secs is the number of seconds between the first two pings
pub(crate) const RELIABLE_PING_INTERVAL_START: TimestampDuration = TimestampDuration::new_secs(10);
/// - Max secs is the maximum number of seconds between consecutive pings.
///   Bounded so a reliable transport's last_seen can't get more than this stale before the
///   next ping, which keeps stopped-node detection timely.
pub(crate) const RELIABLE_PING_INTERVAL_MAX: TimestampDuration = TimestampDuration::new_secs(120);
/// - Multiplier changes the number of seconds between pings over time
///   making it longer as the node becomes more reliable
pub(crate) const RELIABLE_PING_INTERVAL_MULTIPLIER: f64 = 2.0;

//////////////////////////////////////////////////////////////////////////////////////////
// Unreliable state thresholds

/// How long a node must answer, or lose questions, steadily before its state changes; twelve RPC
/// timeouts each, so twelve rounds of `RoutingTableTimings::unreliable_ping_interval`.
///
/// Unreliable pings are done for a span while the node is given a chance to come back online
/// before it is made dead. If a node misses a single ping, it is marked missing and must return
/// reliable pings for the span before being marked reliable again.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct LivenessSpans {
    /// Consecutive answering before a node is reliable
    pub unreliable_answer: TimestampDuration,
    /// Consecutive lost questions before a node heard from before is dead
    pub dead_lost_question: TimestampDuration,
}

impl LivenessSpans {
    pub fn new(rpc_timeout: TimestampDuration) -> Self {
        let span = rpc_timeout.saturating_mul(12);
        Self {
            unreliable_answer: span,
            dead_lost_question: span,
        }
    }
}

//////////////////////////////////////////////////////////////////////////////////////////
// Missing state thresholds

/// Node-level consecutive lost questions (any transport, reset by any answer)
/// at which time we call something missing
pub(crate) const MISSING_LOST_QUESTIONS: u32 = 3;

//////////////////////////////////////////////////////////////////////////////////////////
// Dead state thresholds

/// Failed node-level unreachable attempts (no usable transport) before we call it dead
pub(crate) const DEAD_UNREACHABLE_COUNT: u32 = 1;
/// Node-level lost questions before we call a node dead if we have never seen any traffic from it
pub(crate) const DEAD_NEVER_SEEN_LOST_QUESTIONS: u32 = 3;

//////////////////////////////////////////////////////////////////////////////////////////
// Contact-method viability

/// Reachability inputs for one SequenceOrdering, derived from the entry's current
/// contact method list by the update_cm_viability task. Direct transports are judged
/// against live per-transport stats at state computation; indirect contact methods
/// (existing flow, signal, relay) veto send-failure death, since their failure modes
/// are covered by unreachable [D1] and steady-lost-question [D4] rules.
#[derive(Clone, Debug, Default)]
pub(crate) struct OrderingReachability {
    /// Transports governing Direct contact methods for this ordering
    pub direct_transports: BTreeSet<TransportType>,
    /// Whether any non-direct contact methods exist
    pub has_indirect: bool,
}

/// Cached per-ordering contact-method viability summary for one entry; an ordering with no
/// contact methods is absent (vacuous: a WS-only environment lives or dies entirely on Ordered)
#[derive(Clone, Debug, Default)]
pub(crate) struct CMViability {
    /// Reachability inputs keyed by ordering
    pub per_ordering: BTreeMap<SequenceOrdering, OrderingReachability>,
    /// Peer info timestamps this was computed against, per routing domain (ours, theirs)
    pub basis: BTreeMap<RoutingDomain, (Timestamp, Timestamp)>,
}

impl CMViability {
    /// An ordering is fully failing when its only avenues are Direct transports
    /// and every one of them is currently judged Failing
    fn ordering_all_failing(
        reachability: &OrderingReachability,
        per_transport_stats: &BTreeMap<TransportType, RPCStats>,
    ) -> bool {
        if reachability.has_indirect {
            return false;
        }
        !reachability.direct_transports.is_empty()
            && reachability.direct_transports.iter().all(|t| {
                per_transport_stats
                    .get(t)
                    .map(transport_liveness_health)
                    .is_some_and(|h| h.is_failing())
            })
    }

    /// True if every ordering that has contact methods is fully failing
    pub fn all_orderings_failing(
        &self,
        per_transport_stats: &BTreeMap<TransportType, RPCStats>,
    ) -> bool {
        !self.per_ordering.is_empty()
            && self
                .per_ordering
                .values()
                .all(|r| Self::ordering_all_failing(r, per_transport_stats))
    }

    /// True if at least one ordering that has contact methods is fully failing
    pub fn any_ordering_failing(
        &self,
        per_transport_stats: &BTreeMap<TransportType, RPCStats>,
    ) -> bool {
        self.per_ordering
            .values()
            .any(|r| Self::ordering_all_failing(r, per_transport_stats))
    }
}

impl BucketEntryInner {
    /// Node-level calculation of state.
    ///
    /// Cold start is Unreliable(Unseen). Pure state-reason computation with no side effects.
    ///
    /// Send-failure death is judged from contact-method viability: one viable ordering
    /// vetoes death, and Untried transports are viable, so alternates must accumulate
    /// failure evidence before a node can die.
    ///
    /// * Punished:
    ///   - [P1] the node id is punished by the address filter
    /// * Dead: (any of the following are true)
    ///   - [D1] the node has per-entry unreachable >= DEAD_UNREACHABLE_COUNT
    ///   - [D2] cm_viability is computed and every ordering with contact methods is
    ///     fully failing (all Direct transports Failing, no indirect avenue)
    ///   - [D3] if the node has never been heard from (last_seen_ts is None), and
    ///     node-level recent_lost_questions >= DEAD_NEVER_SEEN_LOST_QUESTIONS
    ///   - [D4] if the node has been heard from, and node-level
    ///     time_since(first_steady_lost_question_ts) >= the dead lost question span
    ///     (any answer on any transport resets it)
    /// * Missing (any of the following are true):
    ///   - [M1] the node has per-entry unreachable > 0 (only possible if DEAD_UNREACHABLE_COUNT > 1)
    ///   - [M2] cm_viability is computed and at least one ordering with contact methods
    ///     is fully failing
    ///   - [M3] node-level recent_lost_questions >= MISSING_LOST_QUESTIONS
    /// * Initial:
    ///   - [I1] for all sequence-orderings:
    ///     - first_steady_answer_ts is None (cold start)
    /// * Unreliable (any of the following are true):
    ///   - [U1] for any sequence-ordering:
    ///     - time_since(first_steady_answer_ts) < the unreliable answer span
    /// * Reliable:
    ///   - [R1] Any other condition is reliable
    pub(super) fn compute_state_reason(&self, cur_ts: Timestamp) -> BucketEntryStateReason {
        // Zero when there's no network manager (early init / tests)
        let opt_nm = self.network_manager.load_full();
        let credit_since = |from: Timestamp, credit: TimeCredit| {
            opt_nm
                .as_ref()
                .map(|nm| {
                    let od = nm.online_detector();
                    match credit {
                        TimeCredit::Untestable => {
                            od.offline_overlap(RoutingDomain::PublicInternet, from, cur_ts)
                        }
                        TimeCredit::Unblamed => {
                            od.blame_suppressed_overlap(RoutingDomain::PublicInternet, from, cur_ts)
                        }
                    }
                })
                .unwrap_or_else(|| TimestampDuration::new(0))
        };
        Self::compute_state_reason_from_stats(
            cur_ts,
            &self.liveness,
            self.punishment,
            &self.rpc_stats,
            &self.per_sequence_ordering_stats,
            &self.per_transport_stats,
            self.cm_viability.as_ref(),
            credit_since,
        )
    }

    /// Pure node-level state calculation over the stats it depends on. The wrapper
    /// `compute_state_reason` carries the full condition reference; tags below match it.
    #[expect(clippy::too_many_arguments)]
    pub(crate) fn compute_state_reason_from_stats(
        cur_ts: Timestamp,
        spans: &LivenessSpans,
        punishment: Option<PunishmentReason>,
        rpc_stats: &RPCStats,
        per_sequence_ordering_stats: &BTreeMap<SequenceOrdering, RPCStats>,
        per_transport_stats: &BTreeMap<TransportType, RPCStats>,
        cm_viability: Option<&CMViability>,
        credit_since: impl Fn(Timestamp, TimeCredit) -> TimestampDuration,
    ) -> BucketEntryStateReason {
        // ---===/ Punished /===---

        // [P1]
        if let Some(p) = punishment {
            return BucketEntryStateReason::Punished(p);
        }

        // ---===/ Dead /===---

        // [D1] the node has per-entry unreachable >= DEAD_UNREACHABLE_COUNT
        // Node is dead if it is unreachable even once. That means no contact method could be chosen
        // or no routing domain could reach it. Giving this node more time will not help. If it comes
        // back with a better peerinfo, it will get another chance.
        if rpc_stats.unreachable >= DEAD_UNREACHABLE_COUNT {
            return BucketEntryStateReason::Dead(BucketEntryStateDeadReason::ExcessiveUnreachable);
        }

        // [D2] every ordering with contact methods is fully failing
        if let Some(v) = cm_viability {
            if v.all_orderings_failing(per_transport_stats) {
                return BucketEntryStateReason::Dead(
                    BucketEntryStateDeadReason::ExcessiveSendFailures,
                );
            }
        }

        // [D3] never heard from the node (traffic FROM it, not dial attempts toward it), and
        // node-level lost questions at threshold
        if rpc_stats.last_seen_ts.is_none()
            && rpc_stats.recent_lost_questions >= DEAD_NEVER_SEEN_LOST_QUESTIONS
        {
            return BucketEntryStateReason::Dead(
                BucketEntryStateDeadReason::NeverSeenLostQuestions,
            );
        }

        // [D4] heard from the node before, and node-level steady lost questions (reset by
        // any answer on any transport) span at least the dead span, minus unjudgeable time
        if rpc_stats.last_seen_ts.is_some()
            && rpc_stats
                .first_steady_lost_question_ts
                .map(|ts| {
                    // Credit only shrinks the span, so consult the detector
                    // only once the raw span could cross the threshold
                    let raw = cur_ts.duration_since(ts);
                    raw >= spans.dead_lost_question
                        && raw.saturating_sub(credit_since(ts, TimeCredit::Unblamed))
                            >= spans.dead_lost_question
                })
                .unwrap_or(false)
        {
            return BucketEntryStateReason::Dead(BucketEntryStateDeadReason::SteadyLostQuestions);
        }

        // ---===/ Missing /===---

        // [M1] the node has per-entry unreachable > 0 (only possible if DEAD_UNREACHABLE_COUNT > 1)
        if rpc_stats.unreachable > 0 {
            return BucketEntryStateReason::Missing(BucketEntryStateMissingReason::Unreachable);
        }

        // [M2] at least one ordering with contact methods is fully failing
        if let Some(v) = cm_viability {
            if v.any_ordering_failing(per_transport_stats) {
                return BucketEntryStateReason::Missing(
                    BucketEntryStateMissingReason::FailedToSend,
                );
            }
        }

        // [M3] node-level recent lost questions at threshold
        if rpc_stats.recent_lost_questions >= MISSING_LOST_QUESTIONS {
            return BucketEntryStateReason::Missing(BucketEntryStateMissingReason::LostQuestions);
        }

        // ---===/ Initial /===---

        // [I1] for all sequence-orderings:
        //   - first_steady_answer_ts is None (cold start)
        if per_sequence_ordering_stats
            .values()
            .all(|stats| stats.first_steady_answer_ts.is_none())
        {
            return BucketEntryStateReason::Initial;
        }

        // ---===/ Unreliable /===---

        // [U1] for any sequence-ordering:
        //   - time_since(first_steady_answer_ts), minus offline time, < the unreliable span
        if per_sequence_ordering_stats.values().any(|stats| {
            if let Some(ts) = stats.first_steady_answer_ts {
                let raw = cur_ts.duration_since(ts);
                // Within span by wall-clock → unreliable regardless; otherwise
                // subtract offline time so we don't grant untested reliability.
                raw < spans.unreliable_answer
                    || raw.saturating_sub(credit_since(ts, TimeCredit::Untestable))
                        < spans.unreliable_answer
            } else {
                false
            }
        }) {
            return BucketEntryStateReason::Unreliable;
        }

        // ---===/ Reliable /===---

        // [R1] Any other condition is reliable
        BucketEntryStateReason::Reliable
    }

    /// Compute and record the node-level state for stats accounting and return the full state reason
    ///
    /// The computation consults the online detector, so it runs outside the stats lock:
    /// holding one across that wait convoys every concurrent state read. The cost is a
    /// rare out-of-order record, which shows up as a zero-length span in debug stats.
    pub fn state_reason(&self, cur_ts: Timestamp) -> BucketEntryStateReason {
        let reason = self.compute_state_reason(cur_ts);
        let change = self
            .state_stats_accounting
            .lock()
            .record_state_reason(cur_ts, reason);
        if let Some(change) = change {
            self.on_state_reason_change(change);
        }
        reason
    }

    /// Get the node-level state without the reason for stats accounting
    pub fn state(&self, cur_ts: Timestamp) -> BucketEntryState {
        self.state_reason(cur_ts).into()
    }

    /// The node-level state, without recording it in the state statistics
    pub(super) fn computed_state(&self, cur_ts: Timestamp) -> BucketEntryState {
        self.compute_state_reason(cur_ts).into()
    }

    /// Utility function to compute and log a state reason change
    fn compute_and_log_state_reason_inner(
        &self,
        cur_ts: Timestamp,
        state_stats_accounting: &mut StateStatsAccounting,
    ) -> BucketEntryStateReason {
        let reason = self.compute_state_reason(cur_ts);
        if let Some(change) = state_stats_accounting.record_state_reason(cur_ts, reason) {
            self.on_state_reason_change(change);
        }
        reason
    }

    /// Log a state reason change, and count a change of state toward the next routing table save
    fn on_state_reason_change(&self, change: StateReasonSpanChange) {
        veilid_log!(self debug target: "rtab::state::node", "{:#}: {:#}", self.best_node_id(), change);
        if change.is_state_change() {
            if let Some(routing_table) = self.registry().lookup::<RoutingTable>() {
                routing_table.mark_entry_state_changed();
            }
        }
    }

    /// If a node is dead or missing, reset it to the 'initial' state to give it another chance.
    /// Called when we get a new peer info for a node so we can try it out.
    ///
    /// Per-transport stats and contact method failures survive: they are learned dial
    /// evidence, pruned only by the update_cm_viability task when the dialinfo changes.
    pub(super) fn revive(&mut self, cur_ts: Timestamp) {
        // Keep the stats lock over this operation so we don't have a race condition
        let mut state_stats_accounting = self.state_stats_accounting.lock();

        // Check if the node is dead or missing
        let dead_or_missing_reason = self.compute_state_reason(cur_ts);
        if matches!(
            dead_or_missing_reason,
            BucketEntryStateReason::Dead(_) | BucketEntryStateReason::Missing(_)
        ) {
            // Drop node-level stats so we can re-validate the node.
            self.rpc_stats.last_question_ts = None;
            self.rpc_stats.last_seen_ts = None;
            self.rpc_stats.first_steady_answer_ts = None;
            self.rpc_stats.first_steady_lost_question_ts = None;
            self.rpc_stats.recent_lost_questions = 0;
            self.rpc_stats.failed_to_send = 0;
            self.rpc_stats.unreachable = 0;

            // Drop per-sequence-ordering stats so each sequence ordering re-validates.
            self.per_sequence_ordering_stats.clear();

            // Initial now, except [M2]/[D2], which derive from retained transport evidence
            let alive_reason =
                self.compute_and_log_state_reason_inner(cur_ts, &mut state_stats_accounting);
            match alive_reason {
                BucketEntryStateReason::Initial => {}
                BucketEntryStateReason::Missing(BucketEntryStateMissingReason::FailedToSend)
                | BucketEntryStateReason::Dead(BucketEntryStateDeadReason::ExcessiveSendFailures) =>
                {
                    veilid_log!(self debug "node revived with retained transport evidence ({:?}): {}", alive_reason, self.best_node_id());
                }
                _ => {
                    veilid_log!(self error "node was ({:?}) but is ({:?}) after reviving: {}", dead_or_missing_reason, alive_reason, self.best_node_id());
                }
            }

            veilid_log!(self debug target: "rtab::state::node", "Node revived: {:#}", self.best_node_id());
        }
    }
}
