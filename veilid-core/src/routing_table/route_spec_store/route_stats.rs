use super::*;
use crate::rpc_processor::Destination;
use hashlink::LruCache;

#[must_use]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RouteStatsDisposition {
    /// Route (or ordering) is still valid
    Valid,
    /// Invalid because we could not send, which names us and the first hop
    InvalidSendFailure,
    /// Invalid because loopbacks failed end to end often enough to blame the route
    InvalidLoopbackFailures,
    /// Invalid because it has lost too many answers
    InvalidLostQuestions,
}

impl RouteStatsDisposition {
    /// Whether the route is dead, rather than merely unusable right now.
    ///
    /// Only evidence spanning the route counts. A send failure is excluded: it
    /// names us and the first hop, whose own liveness covers it.
    pub fn means_route_dead(&self) -> bool {
        matches!(
            self,
            RouteStatsDisposition::InvalidLoopbackFailures
                | RouteStatsDisposition::InvalidLostQuestions
        )
    }

    /// Whether a compiled route built from this should be rebuilt. Send
    /// failures count: a recompile may pick a first hop we can reach.
    pub fn needs_recompile(&self) -> bool {
        !matches!(self, RouteStatsDisposition::Valid)
    }
}

/// Evidence one failure carries at full blame weight
const EVIDENCE_PER_FAILURE: u32 = 1000;

/// Evidence a failure carries at a blame weight from 0 to 1
fn failure_evidence(weight: f64) -> u32 {
    (weight.clamp(0.0, 1.0) * f64::from(EVIDENCE_PER_FAILURE)).round() as u32
}

/// Evidence `failures` failures at full blame weight carry
fn evidence_of(failures: usize) -> u32 {
    u32::try_from(failures)
        .unwrap_or(u32::MAX)
        .saturating_mul(EVIDENCE_PER_FAILURE)
}

/// Per-`SequenceOrdering` validity/failure stats. These are the **single source
/// of truth** for whether a route works at a given ordering; every aggregate
/// view on [RouteStats] is derived from these, never mutated directly. A
/// success or failure always happens on *some* ordering (the transport used),
/// so the recorder always knows which entry to update.
///
/// Failures kill an ordering by evidence, not count: each carries the blame weight it was
/// observed under, so under our own loss more of them are needed, and a recovery does not
/// retroactively condemn a run gathered while degraded.
///
/// Saved with the route set, so a restarted node holds its routes as tested. What belongs to
/// one session is not: the tallies, the evidence, deferrals under blame suppression, loss marks
/// (their state ends with the session) and the keepalive clock (no flow survives an attach).
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct RouteOrderingStats {
    /// Consecutive failed-to-send count (loopback test failures count here too)
    pub failed_to_send: usize,
    /// Consecutive loopback failures deferred under blame suppression
    #[serde(skip)]
    pub deferred_loopback_failures: usize,
    /// Loopback passes this session
    #[serde(skip)]
    pub loopback_pass: u64,
    /// Loopback failures this session
    #[serde(skip)]
    pub loopback_fail: u64,
    /// Current run of loopback failures
    pub consecutive_loopback_failures: usize,
    /// Evidence of the current run of loopback failures, `EVIDENCE_PER_FAILURE` each at full blame
    #[serde(skip)]
    pub loopback_failure_evidence: u32,
    /// Longest run of loopback failures this session
    #[serde(skip)]
    pub max_consecutive_loopback_failures: usize,
    /// Per-destination lost question tracking: destination pubkey -> lost count
    pub lost_question_destinations: HashMap<PublicKey, usize>,
    /// Evidence of the lost questions per destination, each counting at most one failure
    #[serde(skip)]
    pub lost_destination_evidence: HashMap<PublicKey, u32>,
    /// Destinations a question was lost to, until each answers here: pubkey -> state it was lost in
    #[serde(skip)]
    pub lost_destination_marks: HashMap<PublicKey, LossEpoch>,
    /// When this ordering was last known valid (loopback pass / rcvd traffic)
    pub last_known_valid_ts: Option<Timestamp>,
    /// When this ordering was last sent to
    pub last_sent_ts: Option<Timestamp>,
    /// When this ordering last received a question or statement
    pub last_rcvd_question_ts: Option<Timestamp>,
    /// When this ordering last received an answer
    pub last_rcvd_answer_ts: Option<Timestamp>,
    /// Last periodic loopback keepalive enqueue for this ordering
    #[serde(skip)]
    pub last_loopback_keepalive_ts: Option<Timestamp>,
    /// Answer stats for this ordering
    pub answer: AnswerStats,
    /// Accounting mechanism for this ordering's RPC answers
    #[serde(skip)]
    answer_stats_accounting: AnswerStatsAccounting,
}

impl RouteOrderingStats {
    /// Disposition of this single ordering; `loopback_threshold` is the run of loopback
    /// failures that kills it (route-level Locked handled by caller).
    pub fn disposition(&self, loopback_threshold: usize) -> RouteStatsDisposition {
        // Route-spanning evidence first, or a dead route carrying a send failure reads as alive
        if self.loopback_failures_kill(loopback_threshold) {
            RouteStatsDisposition::InvalidLoopbackFailures
        } else if self.lost_destinations_kill() {
            RouteStatsDisposition::InvalidLostQuestions
        } else if self.failed_to_send >= ROUTE_SEND_FAILURES_INVALIDATE_THRESHOLD {
            RouteStatsDisposition::InvalidSendFailure
        } else {
            RouteStatsDisposition::Valid
        }
    }

    /// Whether the run of loopback failures carries `loopback_threshold` failures of evidence
    fn loopback_failures_kill(&self, loopback_threshold: usize) -> bool {
        self.loopback_failure_evidence >= evidence_of(loopback_threshold)
    }

    /// Whether lost questions to enough distinct destinations carry their threshold of evidence
    fn lost_destinations_kill(&self) -> bool {
        self.lost_destination_evidence
            .values()
            .map(|e| (*e).min(EVIDENCE_PER_FAILURE))
            .sum::<u32>()
            >= evidence_of(ROUTE_LOST_DESTINATIONS_INVALIDATE_THRESHOLD)
    }

    /// Does this ordering need (re)testing?
    pub fn needs_testing(&self, cur_ts: Timestamp) -> bool {
        if !self.lost_question_destinations.is_empty()
            || self.failed_to_send > 0
            || self.consecutive_loopback_failures > 0
        {
            return true;
        }
        if let Some(last_tested_ts) = self.last_known_valid_ts {
            cur_ts.duration_since(last_tested_ts)
                > TimestampDuration::new_ms(ROUTE_MIN_IDLE_TIME_MS as u64)
        } else {
            true
        }
    }
}

/// Loopback tally for one ordering after a result
#[derive(Copy, Clone, Debug, Default, PartialEq, Eq)]
pub struct RouteLoopbackTally {
    /// On a pass, the run of failures the pass ended; on a failure, the current run
    pub streak: usize,
    /// Loopback passes this session
    pub pass: u64,
    /// Loopback failures this session
    pub fail: u64,
    /// The pass was the first time this ordering was known valid
    pub first_valid: bool,
}

/// Loopback and release tallies for one ordering since the last summary line
#[derive(Clone, Copy, Debug, Default)]
pub struct RouteTestSummaryEntry {
    pub pass: u64,
    pub fail: u64,
    pub releases: u64,
}

/// Per-ordering route test tallies, logged and reset on an interval
#[derive(Debug, Default)]
pub struct RouteTestSummary {
    pub by_ordering: BTreeMap<SequenceOrdering, RouteTestSummaryEntry>,
    pub last_log_ts: Option<Timestamp>,
}

#[derive(Clone, Debug)]
pub struct RouteStats {
    /// Timestamp of when the route was created
    pub created_ts: Timestamp,
    /// Per-`SequenceOrdering` stats — single source of truth for validity/failure.
    pub per_ordering: BTreeMap<SequenceOrdering, RouteOrderingStats>,
    /// Transfers up and down (route-level for now; reserve the ability to move
    /// per-ordering later via a rollup/update).
    pub transfer: TransferStatsDownUp,
    /// Latency stats (route-level for now; same reservation as transfer).
    /// Refreshed once per stats interval by [Self::update_latency_stats].
    pub latency: LatencyStats,
    /// Per-local-route stats keyed by local-route pubkey on our side.
    pub routed_stats: LruCache<PublicKey, PerRouteStats>,
    /// Accounting mechanism for this route's RPC latency (route-level)
    latency_stats_accounting: LatencyStatsAccounting,
    /// Accounting mechanism for the bandwidth across this route (route-level)
    transfer_stats_accounting: TransferStatsAccounting,
}

impl Default for RouteStats {
    fn default() -> Self {
        Self {
            created_ts: Timestamp::default(),
            per_ordering: BTreeMap::new(),
            transfer: TransferStatsDownUp::default(),
            latency: LatencyStats::default(),
            routed_stats: LruCache::new(PER_ROUTE_STATS_LRU_SIZE),
            latency_stats_accounting: LatencyStatsAccounting::default(),
            transfer_stats_accounting: TransferStatsAccounting::default(),
        }
    }
}

impl fmt::Display for RouteStats {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        writeln!(f, "created: {}", self.created_ts)?;
        writeln!(
            f,
            "last_known_valid:   {}",
            f.to_string_opt(self.last_known_valid_ts())
        )?;
        writeln!(
            f,
            "last_sent:          {}",
            f.to_string_opt(self.last_sent_ts())
        )?;
        writeln!(
            f,
            "last_rcvd_answer:   {}",
            f.to_string_opt(self.last_rcvd_answer_ts())
        )?;
        for (ordering, os) in self.per_ordering.iter() {
            writeln!(
                f,
                "  [{}] valid={} sent={} rq={} ra={} lost-dests/failed={}/{} loopback={}/{} streak={} max={} answer={}",
                ordering,
                f.to_string_opt(os.last_known_valid_ts),
                f.to_string_opt(os.last_sent_ts),
                f.to_string_opt(os.last_rcvd_question_ts),
                f.to_string_opt(os.last_rcvd_answer_ts),
                os.lost_question_destinations.len(),
                os.failed_to_send,
                os.loopback_pass,
                os.loopback_fail,
                os.consecutive_loopback_failures,
                os.max_consecutive_loopback_failures,
                f.to_string(&os.answer),
            )?;
        }
        writeln!(f, "transfer:           {}", f.to_string(&self.transfer))?;
        writeln!(f, "latency:            {}", f.to_string(&self.latency))?;
        if !self.routed_stats.is_empty() {
            writeln!(f, "routed_stats:")?;
            for (key, entry) in self.routed_stats.iter() {
                writeln!(f, "  {}:", key)?;
                writeln!(
                    f,
                    "{}",
                    indent_all_string(indent_all_string(f.to_string(entry)))
                )?;
            }
        }

        Ok(())
    }
}

impl RouteStats {
    /// Make new route stats
    pub fn new(created_ts: Timestamp) -> Self {
        Self {
            created_ts,
            ..Default::default()
        }
    }

    /// Make new route stats from a route set spec detail stats
    pub fn new_from_spec_detail_stats(stats: RouteSetSpecDetailStats) -> Self {
        Self {
            created_ts: stats.created_ts,
            transfer: stats.transfer,
            latency: stats.latency,
            per_ordering: stats.per_ordering.into_iter().collect(),
            ..Default::default()
        }
    }

    /// Mutable access to a single ordering's stats, created on demand.
    fn ordering_mut(&mut self, ordering: SequenceOrdering) -> &mut RouteOrderingStats {
        self.per_ordering.entry(ordering).or_default()
    }

    /////////////////////////////////////////////////////////////////
    // Derived aggregate views (computed from per_ordering, never stored)

    /// Most recent "known valid" timestamp across all orderings.
    pub fn last_known_valid_ts(&self) -> Option<Timestamp> {
        self.per_ordering
            .values()
            .filter_map(|o| o.last_known_valid_ts)
            .max()
    }

    /// Most recent "sent" timestamp across all orderings.
    pub fn last_sent_ts(&self) -> Option<Timestamp> {
        self.per_ordering
            .values()
            .filter_map(|o| o.last_sent_ts)
            .max()
    }

    /// Most recent "received answer" timestamp across all orderings.
    pub fn last_rcvd_answer_ts(&self) -> Option<Timestamp> {
        self.per_ordering
            .values()
            .filter_map(|o| o.last_rcvd_answer_ts)
            .max()
    }

    /// Last loopback keepalive enqueue for a given ordering.
    pub fn last_loopback_keepalive_ts(&self, ordering: SequenceOrdering) -> Option<Timestamp> {
        self.per_ordering
            .get(&ordering)
            .and_then(|o| o.last_loopback_keepalive_ts)
    }

    /// Disposition for a single ordering, derived purely from viability stats.
    pub fn ordering_disposition(&self, ordering: SequenceOrdering) -> RouteStatsDisposition {
        // No entry => untested, treated as Valid (optimistic; not dead).
        self.per_ordering
            .get(&ordering)
            .map(|o| o.disposition(loopback_failure_threshold(ordering)))
            .unwrap_or(RouteStatsDisposition::Valid)
    }

    /// Is the route dead for a given ordering
    pub fn is_dead_for_ordering(&self, ordering: SequenceOrdering) -> bool {
        self.ordering_disposition(ordering).means_route_dead()
    }

    /// Per-ordering counters behind a death, for the release log
    pub fn death_reason(&self, orderings: SequenceOrderingSet) -> String {
        orderings
            .iter()
            .map(|o| match self.per_ordering.get(&o) {
                Some(s) => format!(
                    "{}: send={} loopback_streak={} evidence={:.1} lost_dests={}",
                    o,
                    s.failed_to_send,
                    s.consecutive_loopback_failures,
                    f64::from(s.loopback_failure_evidence) / f64::from(EVIDENCE_PER_FAILURE),
                    s.lost_question_destinations.len()
                ),
                None => format!("{o}: untested"),
            })
            .collect::<Vec<_>>()
            .join(", ")
    }

    /// Dead for *every* ordering it provides. An `EnsureOrdered` route
    /// (orderings = {Ordered}) dies when Ordered dies, never waiting on Unordered.
    pub fn is_dead_for(&self, orderings: SequenceOrderingSet) -> bool {
        !orderings.is_empty() && orderings.iter().all(|o| self.is_dead_for_ordering(o))
    }

    /// Dead on every ordering by evidence that spans the route (a loopback run, or lost questions
    /// to several destinations); a bare send failure names the first hop, which its own liveness
    /// covers, and says nothing about the others
    pub fn death_blames_hops_for(&self, orderings: SequenceOrderingSet) -> bool {
        !orderings.is_empty()
            && orderings.iter().all(|o| {
                self.per_ordering.get(&o).is_some_and(|s| {
                    s.loopback_failures_kill(loopback_failure_threshold(o))
                        || s.lost_destinations_kill()
                })
            })
    }

    /// Disposition aggregated over all orderings (any ordering Valid => Valid).
    pub fn aggregate_disposition(&self) -> RouteStatsDisposition {
        let mut worst = RouteStatsDisposition::Valid;
        for (ordering, o) in self.per_ordering.iter() {
            match o.disposition(loopback_failure_threshold(*ordering)) {
                RouteStatsDisposition::Valid => return RouteStatsDisposition::Valid,
                d => worst = d,
            }
        }
        worst
    }

    /////////////////////////////////////////////////////////////////
    // Per-ordering mutators (the only writers of validity/failure state)

    /// Mark a route as having failed to send at the given ordering
    pub fn record_send_failed(&mut self, ordering: SequenceOrdering) {
        self.ordering_mut(ordering).failed_to_send += 1;
    }

    /// Mark a route as having lost an answer for a destination at the given ordering; a loss
    /// with no epoch, judged while blame is suppressed, marks nothing
    pub fn record_lost_question(
        &mut self,
        ordering: SequenceOrdering,
        destination: &Destination,
        opt_epoch: Option<LossEpoch>,
        weight: f64,
        sent_ts: Timestamp,
    ) {
        let Ok(dest_key) = destination.destination_key() else {
            self.ordering_mut(ordering)
                .answer_stats_accounting
                .record_lost_question(Timestamp::now());
            return;
        };
        self.record_lost_question_to(ordering, dest_key, opt_epoch, weight, sent_ts);
    }

    /// Mark a route as having lost an answer for a destination key at the given ordering, with
    /// the blame weight it was lost under. A question sent before the route's last answer carries
    /// no evidence: the route worked after it left.
    pub fn record_lost_question_to(
        &mut self,
        ordering: SequenceOrdering,
        dest_key: PublicKey,
        opt_epoch: Option<LossEpoch>,
        weight: f64,
        sent_ts: Timestamp,
    ) {
        let os = self.ordering_mut(ordering);
        os.answer_stats_accounting
            .record_lost_question(Timestamp::now());
        *os.lost_question_destinations
            .entry(dest_key.clone())
            .or_insert(0) += 1;
        let weight = if os.last_rcvd_answer_ts.is_some_and(|ts| sent_ts < ts) {
            0.0
        } else {
            weight
        };
        let evidence = os
            .lost_destination_evidence
            .entry(dest_key.clone())
            .or_insert(0);
        *evidence = evidence.saturating_add(failure_evidence(weight));
        if let Some(epoch) = opt_epoch {
            os.lost_destination_marks
                .retain(|_, mark| mark.same_network(&epoch));
            os.lost_destination_marks.insert(dest_key, epoch);
        }
    }

    /// Whether a question to [dest_key] was lost at this ordering in the state `epoch` describes,
    /// with no answer from it over this route since
    pub fn lost_question_to(
        &self,
        ordering: SequenceOrdering,
        dest_key: &PublicKey,
        epoch: &LossEpoch,
    ) -> bool {
        self.per_ordering
            .get(&ordering)
            .and_then(|os| os.lost_destination_marks.get(dest_key))
            .is_some_and(|mark| mark == epoch)
    }

    /// Mark a route as having received a question or statement at the given ordering
    pub fn record_question_received(
        &mut self,
        ordering: SequenceOrdering,
        cur_ts: Timestamp,
        bytes: ByteCount,
    ) {
        let os = self.ordering_mut(ordering);
        os.last_rcvd_question_ts = Some(cur_ts);
        os.last_known_valid_ts = Some(cur_ts);
        os.answer_stats_accounting.record_question(cur_ts);
        self.transfer_stats_accounting.add_down(bytes);
    }

    /// Mark a route as having received an answer from a destination at the given ordering
    pub fn record_answer_received(
        &mut self,
        ordering: SequenceOrdering,
        cur_ts: Timestamp,
        bytes: ByteCount,
        destination: &Destination,
    ) {
        let os = self.ordering_mut(ordering);
        os.last_rcvd_answer_ts = Some(cur_ts);
        os.last_known_valid_ts = Some(cur_ts);
        os.answer_stats_accounting.record_answer(cur_ts);
        // Receiving any answer at this ordering means the ordering is functional.
        os.lost_question_destinations.clear();
        os.lost_destination_evidence.clear();
        // but only this destination's answer shows the route reaches it
        if let Ok(dest_key) = destination.destination_key() {
            os.lost_destination_marks.remove(&dest_key);
        }
        self.transfer_stats_accounting.add_down(bytes);
    }

    /// Mark a route as having been sent to at the given ordering
    pub fn record_sent(&mut self, ordering: SequenceOrdering, cur_ts: Timestamp, bytes: ByteCount) {
        let os = self.ordering_mut(ordering);
        os.last_sent_ts = Some(cur_ts);
        // If we sent successfully, reset this ordering's 'failed_to_send'
        os.failed_to_send = 0;
        self.transfer_stats_accounting.add_up(bytes);
    }

    /// Record a loopback route test result for an ordering. A pass marks the
    /// ordering valid and clears its failures; failures count consecutively, each with the blame
    /// weight it was observed under, and a run carrying the per-ordering threshold of evidence
    /// kills the ordering. Returns the tally after the result.
    pub fn record_loopback_result(
        &mut self,
        ordering: SequenceOrdering,
        ok: bool,
        weight: f64,
        cur_ts: Timestamp,
    ) -> RouteLoopbackTally {
        let os = self.ordering_mut(ordering);
        os.deferred_loopback_failures = 0;
        let ended_streak = os.consecutive_loopback_failures;
        let first_valid = ok && os.last_known_valid_ts.is_none();
        if ok {
            os.last_known_valid_ts = Some(cur_ts);
            os.failed_to_send = 0;
            os.lost_question_destinations.clear();
            os.lost_destination_evidence.clear();
            os.loopback_pass += 1;
            os.consecutive_loopback_failures = 0;
            os.loopback_failure_evidence = 0;
        } else {
            os.loopback_fail += 1;
            os.consecutive_loopback_failures += 1;
            os.loopback_failure_evidence = os
                .loopback_failure_evidence
                .saturating_add(failure_evidence(weight));
            os.max_consecutive_loopback_failures = os
                .max_consecutive_loopback_failures
                .max(os.consecutive_loopback_failures);
        }
        RouteLoopbackTally {
            streak: if ok {
                ended_streak
            } else {
                os.consecutive_loopback_failures
            },
            pass: os.loopback_pass,
            fail: os.loopback_fail,
            first_valid,
        }
    }

    /// Count a loopback failure observed while blame is suppressed. The caller
    /// caps how long deferral can hide a dead route; any recorded result resets.
    pub fn record_loopback_deferral(&mut self, ordering: SequenceOrdering) -> usize {
        let os = self.ordering_mut(ordering);
        os.deferred_loopback_failures += 1;
        os.deferred_loopback_failures
    }

    /// Mark when we last enqueued a periodic loopback keepalive for this route at a given ordering
    pub fn record_loopback_keepalive(&mut self, cur_ts: Timestamp, ordering: SequenceOrdering) {
        self.ordering_mut(ordering).last_loopback_keepalive_ts = Some(cur_ts);
    }

    /// Record latency for this route (route-level)
    pub fn record_latency(&mut self, latency: TimestampDuration) {
        self.latency_stats_accounting.record_latency(latency);
    }

    pub fn record_routed_up(&mut self, key: &PublicKey, bytes: ByteCount) {
        self.routed_stats
            .entry(key.clone())
            .or_insert_with(PerRouteStats::default)
            .add_up(bytes);
    }
    pub fn record_routed_down(&mut self, key: &PublicKey, bytes: ByteCount) {
        self.routed_stats
            .entry(key.clone())
            .or_insert_with(PerRouteStats::default)
            .add_down(bytes);
    }
    pub fn record_routed_round_trip(
        &mut self,
        key: &PublicKey,
        sample: TimestampDuration,
        bytes: ByteCount,
    ) {
        let entry = self
            .routed_stats
            .entry(key.clone())
            .or_insert_with(PerRouteStats::default);
        entry.record_round_trip(sample);
        entry.add_down(bytes);
    }

    /// Refresh the cached latency views; recomputes only from accountings that
    /// saw new samples since the last pass
    pub fn update_latency_stats(&mut self) {
        if let Some(l) = self.latency_stats_accounting.take_latency_stats() {
            self.latency = l;
        }
        for (_key, entry) in self.routed_stats.iter_mut() {
            entry.update_latency_stats();
        }
    }

    /// Roll transfers for these route stats (route-level)
    pub fn roll_transfers(&mut self, last_ts: Timestamp, cur_ts: Timestamp) {
        self.transfer_stats_accounting
            .roll_transfers(last_ts, cur_ts, &mut self.transfer);
        for (_key, entry) in self.routed_stats.iter_mut() {
            entry.roll_transfers(last_ts, cur_ts);
        }
    }

    /// Roll answers per ordering
    pub fn roll_answers(&mut self, cur_ts: Timestamp) {
        for os in self.per_ordering.values_mut() {
            os.answer = os.answer_stats_accounting.roll_answers(cur_ts);
        }
    }

    /// Snapshot the per-ordering stats for persistence
    pub fn per_ordering_snapshot(&self) -> Vec<(SequenceOrdering, RouteOrderingStats)> {
        self.per_ordering
            .iter()
            .map(|(o, os)| (*o, os.clone()))
            .collect()
    }

    /// Get the latency stats
    pub fn latency_stats(&self) -> &LatencyStats {
        &self.latency
    }

    /// Get the transfer stats
    pub fn transfer_stats(&self) -> &TransferStatsDownUp {
        &self.transfer
    }

    /// Reset stats when network restarts
    pub fn reset(&mut self) {
        self.per_ordering.clear();
        self.routed_stats.clear();
    }

    /// Check if a route needs testing for any of the orderings it provides.
    pub fn needs_testing(&self, orderings: SequenceOrderingSet, cur_ts: Timestamp) -> bool {
        // Need testing if any provided ordering has never been tested or is stale/failing.
        orderings.iter().any(|o| {
            self.per_ordering
                .get(&o)
                .map(|os| os.needs_testing(cur_ts))
                .unwrap_or(true)
        })
    }

    /// Whether a remote route needs a test. Its orderings are the remote's own, so the route is
    /// one unit: due while a failure stands, or when nothing proved it within the idle time.
    pub fn remote_needs_testing(&self, cur_ts: Timestamp) -> bool {
        let failing = self
            .per_ordering
            .values()
            .any(|os| !os.lost_question_destinations.is_empty() || os.failed_to_send > 0);
        failing
            || self.last_known_valid_ts().is_none_or(|ts| {
                cur_ts.duration_since(ts) > TimestampDuration::new_ms(ROUTE_MIN_IDLE_TIME_MS as u64)
            })
    }

    /// A remote route answered a test: it is proved over `ordering`, and the failures on every
    /// ordering are cleared, since the route is one unit to us
    pub fn record_remote_test_pass(&mut self, ordering: SequenceOrdering, cur_ts: Timestamp) {
        for os in self.per_ordering.values_mut() {
            os.failed_to_send = 0;
            os.lost_question_destinations.clear();
            os.lost_destination_evidence.clear();
        }
        self.ordering_mut(ordering).last_known_valid_ts = Some(cur_ts);
    }
}

impl RouteSpecStore {
    /// Modify the route statistics for allocated routes
    /// Changes made to the route stats may invalidate the route in the compiled route cache
    /// or lead to the route being tested and eventually removal.
    pub fn update_allocated_route_stats<F>(&self, cur_ts: Timestamp, key: &PublicKey, f: F)
    where
        F: FnOnce(&mut RouteStats) -> VeilidAPIResult<()>,
    {
        // Check for stub route (ignore changes to stubs)
        if self.routing_table().public_keys().contains(key) {
            return;
        }

        let cache = self.cache.read();
        if let Err(e) = cache.update_allocated_route_stats(cur_ts, key, f) {
            veilid_log!(self error "Error updating route stats for allocated route {}: {}", key, e);
        }
    }

    /// After a route-test failure, mark the route for release if it's now dead for all its
    /// orderings. Sticky, so a later send resetting failed_to_send can't un-mark it.
    pub fn mark_allocated_route_for_release_if_dead(&self, key: &PublicKey) {
        let (opt_release_rid, opt_dead_hops) = {
            let cache = self.cache.read();
            let Some(rid) = cache.get_allocated_route_id_by_key(key) else {
                return;
            };
            let Some(arce) = cache.get_allocated_route_by_id(&rid) else {
                return;
            };
            if arce.with_stats(|s| s.is_dead_for(arce.orderings())) && !arce.is_marked_for_release()
            {
                let why = arce.with_stats(|st| st.death_reason(arce.orderings()));
                veilid_log!(self debug "Marking dead route for release: {} ({})", key, why);
                arce.mark_for_release();
                let opt_dead_hops = arce
                    .with_stats(|s| s.death_blames_hops_for(arce.orderings()))
                    .then(|| (arce.orderings(), arce.hop_node_refs()));
                // Nothing holds it right now -> release immediately
                ((!arce.is_locked()).then_some(rid), opt_dead_hops)
            } else {
                (None, None)
            }
        };
        if let Some((orderings, hops)) = opt_dead_hops {
            let routing_table = self.routing_table();
            let hop_epochs = hops
                .iter()
                .map(|hop| {
                    let destination_ts = hop.node_info_ts(RoutingDomain::PublicInternet);
                    (
                        hop.best_node_id(),
                        routing_table.loss_epoch(Some(destination_ts)),
                    )
                })
                .collect::<Vec<_>>();
            self.avoid_dead_route_hops(orderings, &hop_epochs);
        }
        if let Some(rid) = opt_release_rid {
            self.release_allocated_route(rid);
        }
    }

    /// Keep a dead route's hops out of its orderings' allocations while the state each was lost
    /// in holds, otherwise the pool refills with the same dead hop pair. Keeps the hops of the
    /// latest deaths, a background pool's worth of routes at the longest hop count.
    pub(crate) fn avoid_dead_route_hops(
        &self,
        orderings: SequenceOrderingSet,
        hop_epochs: &[(NodeId, LossEpoch)],
    ) {
        let Some((_, network)) = hop_epochs.first() else {
            return;
        };
        let capacity = DESIRED_BACKGROUND_SAFETY_ROUTE_COUNT
            * usize::from(self.config().internal().network.rpc.max_route_hop_count);
        let mut dead_route_hops = self.dead_route_hops.lock();
        for ordering in orderings {
            let avoided = dead_route_hops
                .entry(ordering)
                .or_insert_with(|| LruCache::new(capacity));
            avoided.retain(|_, lost| lost.same_network(network));
            for (node_id, lost) in hop_epochs {
                avoided.insert(node_id.clone(), *lost);
            }
        }
    }

    /// Dead hops to avoid when allocating a route for this ordering: those whose current epoch,
    /// from `current_epoch`, is still the one they were lost in
    pub(crate) fn dead_route_hops_to_avoid(
        &self,
        ordering: SequenceOrdering,
        current_epoch: impl Fn(&NodeId) -> Option<LossEpoch>,
    ) -> Vec<NodeId> {
        let recorded = match self.dead_route_hops.lock().get(&ordering) {
            Some(avoided) => avoided
                .iter()
                .map(|(node_id, lost)| (node_id.clone(), *lost))
                .collect::<Vec<_>>(),
            None => return Vec::new(),
        };
        recorded
            .into_iter()
            .filter(|(node_id, lost)| current_epoch(node_id).as_ref() == Some(lost))
            .map(|(node_id, _)| node_id)
            .collect()
    }

    /// Tally a loopback outcome for the periodic summary
    pub fn record_route_test_summary(&self, ordering: SequenceOrdering, ok: bool) {
        let mut summary = self.route_test_summary.lock();
        let entry = summary.by_ordering.entry(ordering).or_default();
        if ok {
            entry.pass += 1;
        } else {
            entry.fail += 1;
        }
    }

    /// Tally a route release under each ordering it carried
    pub fn record_route_release_summary(&self, orderings: SequenceOrderingSet) {
        let mut summary = self.route_test_summary.lock();
        for ordering in orderings {
            summary.by_ordering.entry(ordering).or_default().releases += 1;
        }
    }

    /// Log the per-ordering loopback tallies for an interval that saw a failure or a release, then reset them
    pub fn log_route_test_summary(&self, cur_ts: Timestamp) {
        let taken = {
            let mut summary = self.route_test_summary.lock();
            if summary
                .last_log_ts
                .is_some_and(|ts| cur_ts.duration_since(ts) < ROUTE_TEST_SUMMARY_INTERVAL)
            {
                return;
            }
            summary.last_log_ts = Some(cur_ts);
            mem::take(&mut summary.by_ordering)
        };
        // A clean interval stays quiet
        if taken.values().all(|e| e.fail == 0 && e.releases == 0) {
            return;
        }
        let mut route_counts = BTreeMap::<SequenceOrdering, usize>::new();
        self.list_allocated_routes(|_k, v| {
            for ordering in v.orderings() {
                *route_counts.entry(ordering).or_default() += 1;
            }
            Option::<()>::None
        });
        let parts = SequenceOrderingSet::all()
            .iter()
            .map(|ordering| {
                let entry = taken.get(&ordering).copied().unwrap_or_default();
                format!(
                    "{:#} pass/fail={}/{} (routes {}, releases {})",
                    ordering,
                    entry.pass,
                    entry.fail,
                    route_counts.get(&ordering).copied().unwrap_or(0),
                    entry.releases
                )
            })
            .collect::<Vec<_>>();
        veilid_log!(self debug target: "rtab::route", "route test summary: {}", parts.join(" "));
    }

    /// A remote route answered a test: proved over the ordering its question left on
    pub fn record_remote_test_pass(&self, key: &PublicKey, opt_transport: Option<TransportType>) {
        let ordering = opt_transport.map_or(SequenceOrdering::Ordered, |t| t.sequence_ordering());
        let cur_ts = Timestamp::now();
        self.update_remote_route_stats(cur_ts, key, |s| {
            s.record_remote_test_pass(ordering, cur_ts);
            Ok(())
        });
    }

    /// Modify the route statistics for remote routes
    /// Changes made to the route stats may invalidate the route in the compiled route cache
    pub fn update_remote_route_stats<F>(&self, cur_ts: Timestamp, key: &PublicKey, f: F)
    where
        F: FnOnce(&mut RouteStats) -> VeilidAPIResult<()>,
    {
        // Check for stub route (ignore changes to stubs)
        if self.routing_table().public_keys().contains(key) {
            return;
        }

        let cache = self.cache.read();
        if let Err(e) = cache.update_remote_route_stats(cur_ts, key, f) {
            veilid_log!(self error "Error updating route stats for remote route {}: {}", key, e);
        }
    }

    /// Process transfer statistics to get averages
    pub fn roll_transfers(&self, last_ts: Timestamp, cur_ts: Timestamp) {
        // Careful with locking order here, we need to lock the content before the cache
        let content = self.content.read();
        let cache = self.cache.read();

        // Roll transfers for allocated route cache and remote private routes
        cache.roll_transfers(last_ts, cur_ts);

        // Update transfers from cache into content to save them
        content.update_transfers(&cache);

        // Also update latency here because we don't use this in realtime from the content like we do from the cache
        content.update_latency(&cache);
    }

    /// Process answer statistics
    pub fn roll_answers(&self, cur_ts: Timestamp) {
        // Careful with locking order here, we need to lock the content before the cache
        let content = self.content.read();
        let cache = self.cache.read();

        // Roll transfers for the cache
        cache.roll_answers(cur_ts);

        // Update answers from cache into content to save them
        content.update_per_ordering(&cache);
    }
}
