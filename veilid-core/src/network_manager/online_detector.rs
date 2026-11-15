use super::*;

impl_veilid_log_facility!("net");

/// Distinct previously-live targets that must fail on the stream framing type, with no
/// intervening success, for the local node to be considered offline.
pub const OFFLINE_DETECTION_FAILURE_THRESHOLD_CONNECTION: usize = 4;
/// Same, but for the datagram socket class.
pub const OFFLINE_DETECTION_FAILURE_THRESHOLD_MESSAGE: usize = 4;

#[derive(Copy, Clone, Debug, Eq, PartialEq, Default)]
pub enum OnlineState {
    /// Round trips complete and failure evidence is trustworthy
    #[default]
    Online,
    /// Round trips complete but failure evidence is about us; blame writers hold fire
    Degraded,
    /// Nothing gets through; blame and recovery behaviors both gate
    Offline,
}

impl OnlineState {
    /// Whether blame writers should hold fire: in Degraded or Offline the failure
    /// evidence is about us, not the peer it names.
    pub fn suppresses_blame(&self) -> bool {
        !matches!(self, OnlineState::Online)
    }
}

/// The online detector's windows, derived from the RPC timeout; `offline_sustained` outlasts
/// sub-second blips (pings missed under load, sparse idle probes) but not a NAT-flush wedge
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct OnlineDetectorTimings {
    /// How long ping-derived offline must persist to count as a real outage: one RPC timeout
    pub offline_sustained: TimestampDuration,
    /// Sliding window failure breadth accumulates over for degradation detection: two RPC timeouts
    pub degraded_evidence_window: TimestampDuration,
    /// Base clean time after the last degradation evidence before Degraded can end: one RPC timeout
    pub degraded_exit_clean: TimestampDuration,
    /// Ceiling on the escalated clean window: four RPC timeouts
    pub degraded_exit_clean_max: TimestampDuration,
    /// Recovering and degrading again within this is one marginal period, not two: six RPC timeouts
    pub degraded_reentry: TimestampDuration,
    /// Degraded decays after this long without a post-evidence success: twelve RPC timeouts
    pub degraded_max_hold: TimestampDuration,
    /// Local stalls at or above this consume enough of the RPC timeout to void peer evidence: half
    pub local_stall_min: TimestampDuration,
    /// Every question sent this long ago has its outcome: one RPC timeout
    pub outcome_lag: TimestampDuration,
    /// Window the loss estimate's per-path median reads, a few questions each: six RPC timeouts
    pub loss_attribution_window: TimestampDuration,
}

impl OnlineDetectorTimings {
    pub fn new(rpc_timeout: TimestampDuration) -> Self {
        Self {
            offline_sustained: rpc_timeout,
            degraded_evidence_window: rpc_timeout.saturating_mul(2),
            degraded_exit_clean: rpc_timeout,
            degraded_exit_clean_max: rpc_timeout.saturating_mul(4),
            degraded_reentry: rpc_timeout.saturating_mul(6),
            degraded_max_hold: rpc_timeout.saturating_mul(12),
            local_stall_min: rpc_timeout.saturating_mul(50).div(100),
            outcome_lag: rpc_timeout,
            loss_attribution_window: rpc_timeout.saturating_mul(6),
        }
    }
}

/// Credit intervals kept per domain; a streak needs its whole untested history, however long
const CREDIT_INTERVALS_KEPT: usize = 64;

/// Degraded breadth scales with the table: at least this fraction (1/divisor) of live
/// entries must fail fresh in-window per framing type. A big table always carries a few
/// flapping peers (a flat count false-fires on healthy networks), while a lossy uplink
/// fails a large slice of previously-clean peers at once.
const DEGRADED_BREADTH_TABLE_DIVISOR: usize = 20;

/// Failure count threshold for a socket class.
fn failure_threshold(framing_type: FramingType) -> usize {
    match framing_type {
        FramingType::Connection => OFFLINE_DETECTION_FAILURE_THRESHOLD_CONNECTION,
        FramingType::Message => OFFLINE_DETECTION_FAILURE_THRESHOLD_MESSAGE,
    }
}

/// The failing nodes per socket class, for the offline decision's log line
fn describe_failures(pending_failures: &BTreeMap<FramingType, BTreeSet<NodeId>>) -> String {
    pending_failures
        .iter()
        .map(|(ft, nodes)| {
            let ids: Vec<String> = nodes.iter().map(|n| n.to_string()).collect();
            format!("{:?}[{}]: {}", ft, ids.len(), ids.join(" "))
        })
        .collect::<Vec<_>>()
        .join("; ")
}

/// Clean window required to leave Degraded, doubled per consecutive rapid re-entry.
///
/// A marginal period is evidence with gaps in it, and at the base window those gaps open
/// brief Online windows for blame to land in. Escalating closes them on a network that
/// keeps re-degrading; a recovered one stays on the fast path.
pub(crate) fn degraded_exit_clean(
    timings: &OnlineDetectorTimings,
    reentries: u32,
) -> TimestampDuration {
    timings
        .degraded_exit_clean
        .saturating_mul(1u64 << reentries.min(16))
        .min(timings.degraded_exit_clean_max)
}

/// Delay before retry `attempt` (0 for the first) of network work that failed: doubling from
/// `base` up to `ceiling`, stretched by `opt_survival`, the share of round trips our own link lets
/// through, so a lossy link is asked as often per answer as a healthy one. None while offline.
pub(crate) fn failure_backoff(
    base: TimestampDuration,
    ceiling: TimestampDuration,
    attempt: u32,
    opt_survival: Option<f64>,
) -> Option<TimestampDuration> {
    let survival = opt_survival?;
    let doubled = base.saturating_mul(1u64 << attempt.min(16));
    let stretched = if survival > 0.0 {
        doubled.f64_mul(1.0 / survival)
    } else {
        ceiling
    };
    Some(stretched.min(ceiling))
}

/// The blame weight in force through a degraded period, integrated for its mean: zero during
/// stalls and bursts, our link's survival under measured loss
#[derive(Copy, Clone, Debug, PartialEq)]
pub(crate) struct DegradedBlame {
    /// Weight times microseconds up to `since`
    integral: f64,
    /// Weight in force since `since`
    weight: f64,
    /// When `weight` took over
    since: Timestamp,
}

impl DegradedBlame {
    fn new(weight: f64, now: Timestamp) -> Self {
        Self {
            integral: 0.0,
            weight,
            since: now,
        }
    }

    fn advance(&mut self, weight: f64, now: Timestamp) {
        self.integral += self.weight * now.duration_since(self.since).as_u64() as f64;
        self.weight = weight;
        self.since = now;
    }

    /// Mean weight over the period that began at `start`, as of `now`
    fn mean(&self, start: Timestamp, now: Timestamp) -> f64 {
        let span = now.duration_since(start).as_u64() as f64;
        if span <= 0.0 {
            return self.weight;
        }
        let total = self.integral + self.weight * now.duration_since(self.since).as_u64() as f64;
        (total / span).clamp(0.0, 1.0)
    }
}

/// A change of the committed three-state answer
#[derive(Copy, Clone, Debug, PartialEq)]
pub(crate) struct EffectiveTransition {
    /// State before the change
    pub old_state: OnlineState,
    /// State after the change
    pub new_state: OnlineState,
    /// Clean window the next exit from Degraded needs
    pub clean: TimestampDuration,
    /// On leaving Degraded, whether local stalls alone held it there
    pub local_stall_only: bool,
    /// The loss estimate at the change
    pub loss: LossEstimate,
}

/// The three-state answer, derived from the stored two-state machine plus
/// degradation evidence. Pure so the timing rules are testable without a network.
///
/// Entry is fast, exit is slow: fresh evidence holds Degraded, and leaving requires a
/// success AFTER the evidence (an interleaved success is what keeps the offline detector
/// blind), with the max hold bounding the idle case.
pub(crate) fn effective_online_state(
    timings: &OnlineDetectorTimings,
    stored: OnlineState,
    last_evidence_ts: Option<Timestamp>,
    last_success_ts: Option<Timestamp>,
    clean: TimestampDuration,
    now: Timestamp,
) -> OnlineState {
    if stored == OnlineState::Offline {
        return OnlineState::Offline;
    }
    let Some(ev) = last_evidence_ts else {
        return OnlineState::Online;
    };
    if now < ev.later(clean) {
        return OnlineState::Degraded;
    }
    let recovered = last_success_ts.is_some_and(|s| s > ev);
    if recovered || now >= ev.later(timings.degraded_max_hold) {
        return OnlineState::Online;
    }
    OnlineState::Degraded
}

/// Events that drive the online/offline state machine.
pub(crate) enum OnlineEvent {
    /// A round trip completed, `sent_ts` being when the question left, or something arrived
    /// unasked (`None`). `opt_direct` is a direct question's node and transport.
    Success {
        sent_ts: Option<Timestamp>,
        opt_direct: Option<(NodeId, TransportType)>,
    },
    /// Failures for previously-live nodes on one framing type. `fresh_node_ids` is the
    /// subset whose failure is news (see `record_failure`); only those arm the degraded
    /// window, while offline detection keeps the full live set.
    Failure {
        /// Responsive nodes that failed
        live_node_ids: Vec<NodeId>,
        /// The subset whose failure is news
        fresh_node_ids: Vec<NodeId>,
        /// Live table size at record time, for the fractional degraded breadth
        responsive_count: usize,
        /// When the failed question left
        sent_ts: Timestamp,
        /// Socket class the question went out on
        framing_type: FramingType,
    },
    /// The local network address changed.
    NetworkChange,
    /// The node detached from the network (a deliberate offline period).
    Detached,
}

/// Per-routing-domain online/offline tracking.
#[derive(Debug)]
pub(crate) struct OnlineDetectionPerDomain {
    /// Previously-live nodes that failed on each framing type since the last success
    pub(crate) pending_failures: BTreeMap<FramingType, BTreeSet<NodeId>>,
    /// The same breadth over a time window, which an interleaved success cannot erase
    pub(crate) windowed_failures: BTreeMap<FramingType, BTreeMap<NodeId, Timestamp>>,
    /// Last time the windowed breadth crossed threshold on every enabled framing type
    pub(crate) last_degraded_evidence_ts: Option<Timestamp>,
    /// Last completed round trip; exiting Degraded requires one after the evidence
    pub(crate) last_success_ts: Option<Timestamp>,
    /// Last committed three-state answer, updated on events and lazily on reads
    pub(crate) effective_state: OnlineState,
    /// How many times the committed answer has changed
    effective_transitions: u64,
    /// Consecutive Degraded re-entries within `degraded_reentry`; escalates the exit window
    degraded_reentries: u32,
    /// When Degraded last ended, for the rapid-re-entry test
    degraded_recovered_ts: Option<Timestamp>,
    /// When we last switched online/offline
    last_transition_ts: Timestamp,
    /// Current online/offline state
    pub(crate) online_state: OnlineState,
    /// Flap detector over online_state
    flap_detector: FlapDetector<OnlineState>,
    /// Start of the current offline period, if currently offline
    pub(crate) offline_started_ts: Option<Timestamp>,
    /// Whether this offline period's connection purge has already run
    offline_purged: bool,
    /// Last `CREDIT_INTERVALS_KEPT` offline intervals, oldest first, credited as untestable time
    pub(crate) recent_offline_intervals: VecDeque<(Timestamp, Timestamp)>,
    /// Start of the current degraded period, if effectively degraded
    pub(crate) degraded_started_ts: Option<Timestamp>,
    /// The same for degraded periods, each with the mean blame weight it carried: only the
    /// unblamed share of one is credited
    pub(crate) recent_degraded_intervals: VecDeque<(Timestamp, Timestamp, f64)>,
    /// Blame weight through the current degraded period
    degraded_blame: Option<DegradedBlame>,
    /// Our own share of lost round trips, from direct questions
    pub(crate) loss_estimator: LossEstimator,
    /// Whether loss on our link held the current degraded period at any point
    degraded_by_loss: bool,
    /// When the loss estimate was last logged
    last_loss_log_ts: Option<Timestamp>,
    /// Cumulative online<->offline transition count
    #[cfg(feature = "stats")]
    total_transitions: u64,
    /// Cumulative time spent offline
    #[cfg(feature = "stats")]
    total_time_offline: TimestampDuration,
    /// Cumulative time spent degraded
    #[cfg(feature = "stats")]
    total_time_degraded: TimestampDuration,
}

impl Default for OnlineDetectionPerDomain {
    fn default() -> Self {
        Self {
            pending_failures: BTreeMap::new(),
            windowed_failures: BTreeMap::new(),
            last_degraded_evidence_ts: None,
            last_success_ts: None,
            effective_state: OnlineState::default(),
            effective_transitions: 0,
            degraded_reentries: 0,
            degraded_recovered_ts: None,
            last_transition_ts: Timestamp::new(0),
            online_state: OnlineState::default(),
            // 4 online/offline flips within ~60s = flapping
            flap_detector: FlapDetector::new_secs(4.0, 60),
            offline_started_ts: None,
            offline_purged: false,
            recent_offline_intervals: VecDeque::new(),
            degraded_started_ts: None,
            recent_degraded_intervals: VecDeque::new(),
            degraded_blame: None,
            loss_estimator: LossEstimator::default(),
            degraded_by_loss: false,
            last_loss_log_ts: None,
            #[cfg(feature = "stats")]
            total_transitions: 0,
            #[cfg(feature = "stats")]
            total_time_offline: TimestampDuration::new(0),
            #[cfg(feature = "stats")]
            total_time_degraded: TimestampDuration::new(0),
        }
    }
}

/// Which peers' fresh failures may arm the degraded window
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum DegradedBreadthStrategy {
    /// Any responsive peer that answered recently and is not judged failing; for comparison runs
    #[expect(dead_code)]
    Responsive,
    /// Only peers in the Reliable state: the ones whose loss is news rather than habit
    ReliableOnly,
}

/// Detects if our node is online or offline.
///
/// To determine if network errors are 'our fault' or 'the other side's fault', we need to
/// generally know if we are online or offline, before assigning blame to another node and
/// changing its state.
pub struct OnlineDetector {
    /// Component registry this detector belongs to
    registry: VeilidComponentRegistry,
    /// Which peers' failures count toward degraded breadth
    breadth_strategy: DegradedBreadthStrategy,
    /// Windows derived from the RPC timeout
    timings: OnlineDetectorTimings,
    /// Detection state per routing domain
    inner: Mutex<BTreeMap<RoutingDomain, OnlineDetectionPerDomain>>,
    /// Framing types the network uses; only these gate offline detection
    enabled_framing_types: Mutex<FramingTypeSet>,
    /// Last local pipeline stall long enough to void timeout evidence; process-wide
    last_local_stall: Mutex<Option<Timestamp>>,
    /// Read-mostly snapshot of the interval data, republished whenever that data changes
    overlap_data: RwLock<BTreeMap<RoutingDomain, Arc<OverlapData>>>,
}

/// The interval data [OnlineDetector::interval_overlap] reads.
///
/// Every bucket entry state computation may consult the overlap of its span with offline or
/// degraded time, from every sweep and peer-selection sort at once, so serving those reads from
/// the detector's own state lock would make it the most contended lock in the process. Readers
/// share this snapshot instead, and the state machine republishes it on every path that can change
/// the interval data.
#[derive(Debug, Default)]
pub(crate) struct OverlapData {
    offline_intervals: VecDeque<(Timestamp, Timestamp)>,
    degraded_intervals: VecDeque<(Timestamp, Timestamp, f64)>,
    offline_started_ts: Option<Timestamp>,
    degraded_started_ts: Option<Timestamp>,
    degraded_blame: Option<DegradedBlame>,
}

impl OverlapData {
    /// A snapshot of the domain's current interval data
    pub(crate) fn of(pd: &OnlineDetectionPerDomain) -> Self {
        Self {
            offline_intervals: pd.recent_offline_intervals.clone(),
            degraded_intervals: pd.recent_degraded_intervals.clone(),
            offline_started_ts: pd.offline_started_ts,
            degraded_started_ts: pd.degraded_started_ts,
            degraded_blame: pd.degraded_blame,
        }
    }

    /// Whether this snapshot already holds the domain's current interval data
    fn matches(&self, pd: &OnlineDetectionPerDomain) -> bool {
        self.offline_started_ts == pd.offline_started_ts
            && self.degraded_started_ts == pd.degraded_started_ts
            && self.degraded_blame == pd.degraded_blame
            && self.offline_intervals == pd.recent_offline_intervals
            && self.degraded_intervals == pd.recent_degraded_intervals
    }

    /// Offline time in `from..to`, plus the unblamed share of degraded time if `include_degraded`
    pub(crate) fn overlap(
        &self,
        from: Timestamp,
        to: Timestamp,
        include_degraded: bool,
    ) -> TimestampDuration {
        let empty: VecDeque<(Timestamp, Timestamp, f64)> = VecDeque::new();
        let (degraded_intervals, opt_degraded_started) = if include_degraded {
            (&self.degraded_intervals, self.degraded_started_ts)
        } else {
            (&empty, None)
        };
        // The open period carries its mean weight so far, so its credit never shrinks
        let open_weight = |s: Timestamp| self.degraded_blame.map_or(0.0, |b| b.mean(s, to));
        let mut overlap = 0u64;
        // Offline and degraded never overlap: entering one closes the other
        for (s, e, weight) in self
            .offline_intervals
            .iter()
            .map(|(s, e)| (*s, *e, 0.0))
            .chain(degraded_intervals.iter().copied())
            .chain(self.offline_started_ts.map(|s| (s, to, 0.0)))
            .chain(opt_degraded_started.map(|s| (s, to, open_weight(s))))
        {
            let lo = s.max(from);
            let hi = e.min(to);
            if hi > lo {
                let span = hi.duration_since(lo).as_u64() as f64;
                overlap += (span * (1.0 - weight.clamp(0.0, 1.0))) as u64;
            }
        }
        TimestampDuration::new(overlap)
    }
}

impl_veilid_component_accessors!(OnlineDetector);

impl fmt::Debug for OnlineDetector {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("OnlineDetector")
            .field("inner", &*self.inner.lock())
            .field("enabled_framing_types", &*self.enabled_framing_types.lock())
            .finish()
    }
}

impl OnlineDetector {
    pub fn new(registry: VeilidComponentRegistry) -> Self {
        let timings = OnlineDetectorTimings::new(registry.config().rpc_timeout());
        Self {
            registry,
            breadth_strategy: DegradedBreadthStrategy::ReliableOnly,
            timings,
            inner: Mutex::new(BTreeMap::new()),
            // Require both classes until the network reports its protocol config.
            enabled_framing_types: Mutex::new(FramingTypeSet::all()),
            last_local_stall: Mutex::new(None),
            overlap_data: RwLock::new(BTreeMap::new()),
        }
    }

    /// Whether this routing domain uses ping-based online detection.
    fn uses_ping_detection(routing_domain: RoutingDomain) -> bool {
        // Other domains will plug in their own signals (LocalNetwork: interface link state).
        matches!(routing_domain, RoutingDomain::PublicInternet)
    }

    /// Republish the overlap snapshot; call before releasing `inner` on any
    /// path that can change interval data (all three mutable-lock functions do)
    fn publish_overlap_data(&self, routing_domain: RoutingDomain, pd: &OnlineDetectionPerDomain) {
        // Most calls change nothing, and readers share this lock
        if self
            .overlap_data
            .read()
            .get(&routing_domain)
            .is_some_and(|od| od.matches(pd))
        {
            return;
        }
        let snap = Arc::new(OverlapData::of(pd));
        self.overlap_data.write().insert(routing_domain, snap);
    }

    /// Set which framing types offline detection requires, from the network protocol config.
    pub fn set_protocol_config(&self, outbound: ProtocolTypeSet, inbound: ProtocolTypeSet) {
        let mut framing_types = FramingTypeSet::new();
        for pt in outbound | inbound {
            framing_types.insert(pt.framing_type());
        }
        *self.enabled_framing_types.lock() = framing_types;
    }

    /// Record a successful round-trip, or unasked inbound traffic (`sent_ts` None). A direct
    /// question's answer passes its node and transport for the loss estimate.
    pub fn record_success(
        &self,
        routing_domain: RoutingDomain,
        sent_ts: Option<Timestamp>,
        opt_direct: Option<(NodeId, TransportType)>,
    ) {
        self.process_event(
            routing_domain,
            OnlineEvent::Success {
                sent_ts,
                opt_direct,
            },
        );
    }

    /// Record a direct question lost, for the loss estimate. Routed questions stay out: any hop
    /// could have lost them.
    pub fn record_round_trip_lost(
        &self,
        routing_domain: RoutingDomain,
        node_id: NodeId,
        transport: TransportType,
        sent_ts: Timestamp,
    ) {
        if !Self::uses_ping_detection(routing_domain) {
            return;
        }
        let local_stall_ts = *self.last_local_stall.lock();
        let now = Timestamp::now();
        let opt_logged = {
            let mut inner = self.inner.lock();
            let pd = inner.entry(routing_domain).or_default();
            Self::note_round_trip(
                &self.timings,
                pd,
                &node_id,
                transport,
                sent_ts,
                false,
                local_stall_ts,
                now,
            )
        };
        if let Some(estimate) = opt_logged.filter(LossEstimate::shows_loss) {
            veilid_log!(self debug "{:?} {}", routing_domain, estimate);
        }
    }

    /// Feed a direct question's outcome to the loss estimate; returns the estimate when it is due
    /// to be logged
    #[expect(clippy::too_many_arguments)]
    fn note_round_trip(
        timings: &OnlineDetectorTimings,
        pd: &mut OnlineDetectionPerDomain,
        node_id: &NodeId,
        transport: TransportType,
        sent_ts: Timestamp,
        answered: bool,
        local_stall_ts: Option<Timestamp>,
        now: Timestamp,
    ) -> Option<LossEstimate> {
        // Questions from before this online period or a stall say nothing about the link now
        let counted =
            sent_ts >= pd.last_transition_ts && local_stall_ts.is_none_or(|ts| sent_ts >= ts);
        pd.loss_estimator.record_outcome(
            node_id,
            transport.sequence_ordering(),
            sent_ts,
            answered,
            counted,
            now,
        );
        let due = pd
            .last_loss_log_ts
            .is_none_or(|ts| now.duration_since(ts) >= timings.degraded_evidence_window);
        due.then(|| {
            pd.last_loss_log_ts = Some(now);
            let learn = pd.effective_state == OnlineState::Online;
            pd.loss_estimator.evaluate(timings, learn, now)
        })
    }

    /// Record an RPC failure against the directly attempted node. Routed failures are
    /// never fed here: any hop or the route itself may be at fault, and spreading one
    /// failure across hop ids fabricates broad-failure breadth.
    ///
    /// Only currently-responsive nodes count. A probe timing out to a node that is
    /// Initial/Missing/Dead/Punished is evidence about that node, not about us: a stale
    /// routing table loads entries as Initial while their persisted answer history
    /// survives, so a historical-liveness check would count them and flip us offline
    /// during the attach burst.
    ///
    /// The degraded window additionally requires the failure to be news: the ordering
    /// answered recently and is not already judged Failing. Chronic single-transport
    /// failers otherwise re-arm the window forever; a real local storm still counts
    /// each node's first losses before the judge trips.
    pub fn record_failure(
        &self,
        routing_domain: RoutingDomain,
        node_ids: impl IntoIterator<Item = NodeId>,
        sent_ts: Timestamp,
        transport: TransportType,
        remote: Option<SocketAddress>,
    ) {
        if !Self::uses_ping_detection(routing_domain) {
            return;
        }
        // A send that went through our own relay failed on the relay's side of the relay,
        // not on our path to it; our connection to the relay is protected and kept alive
        let via_own_relay = remote.is_some_and(|remote| {
            self.routing_table()
                .relays(routing_domain)
                .iter()
                .filter_map(|r| r.relay_node.unfiltered().get_peer_info(routing_domain))
                .any(|pi| {
                    pi.node_info()
                        .dial_info_detail_list()
                        .iter()
                        .any(|did| did.dial_info.socket_address() == remote)
                })
        });
        // Responsive now, not historically; fresh when the failure is news
        let cur_ts = Timestamp::now();
        let routing_table = self.routing_table();
        let so = transport.sequence_ordering();
        let node_ids: Vec<NodeId> = node_ids.into_iter().collect();
        let mut live_node_ids = Vec::new();
        let mut fresh_node_ids = Vec::new();
        for node_id in node_ids.iter().cloned() {
            let Some(nr) = routing_table.lookup_node_id(node_id.clone()).ok().flatten() else {
                continue;
            };
            let (responsive, fresh) = nr.operate(|e| {
                let state = e.state(cur_ts);
                let responsive = state.is_responsive();
                let eligible = match self.breadth_strategy {
                    DegradedBreadthStrategy::Responsive => responsive,
                    DegradedBreadthStrategy::ReliableOnly => state == BucketEntryState::Reliable,
                };
                let fresh = eligible
                    && !via_own_relay
                    && e.per_sequence_ordering_stats().get(&so).is_some_and(|s| {
                        s.recent_lost_questions
                            < crate::routing_table::TRANSPORT_FAILING_LOST_QUESTIONS
                            && s.last_seen_ts.is_some_and(|ts| {
                                cur_ts.duration_since(ts)
                                    <= crate::routing_table::RELIABLE_PING_INTERVAL_MAX
                            })
                    });
                (responsive, fresh)
            });
            if responsive {
                live_node_ids.push(node_id.clone());
            }
            if fresh {
                fresh_node_ids.push(node_id);
            }
        }
        // Peers that answered recently stay live to the offline count after our loss demotes them
        {
            let inner = self.inner.lock();
            if let Some(pd) = inner.get(&routing_domain) {
                let since = sent_ts.earlier(crate::routing_table::RELIABLE_PING_INTERVAL_MAX);
                for node_id in node_ids {
                    if !live_node_ids.contains(&node_id)
                        && pd.loss_estimator.answered_since(&node_id, since)
                    {
                        live_node_ids.push(node_id);
                    }
                }
            }
        }
        if live_node_ids.is_empty() {
            return;
        }
        let responsive_count = routing_table.get_routing_table_health().live_entry_count();
        self.process_event(
            routing_domain,
            OnlineEvent::Failure {
                live_node_ids,
                fresh_node_ids,
                responsive_count,
                sent_ts,
                framing_type: transport.framing_type(),
            },
        );
    }

    /// Local network addresses changed: presume offline until a new RPC succeeds.
    #[cfg_attr(all(target_arch = "wasm32", target_os = "unknown"), expect(dead_code))]
    pub fn network_address_change(&self, routing_domain: RoutingDomain) {
        self.process_event(routing_domain, OnlineEvent::NetworkChange);
    }

    /// Re-arm for a new attach; online/offline history persists across detach.
    pub fn startup(&self) {
        *self.enabled_framing_types.lock() = FramingTypeSet::all();
    }

    /// Detached: mark every ping-detected domain offline so the detached period
    /// is credited to node reliability until we reconnect.
    pub fn shutdown(&self) {
        for routing_domain in RoutingDomain::all() {
            self.process_event(routing_domain, OnlineEvent::Detached);
        }
    }

    /// Report a measured local pipeline stall (executor tick lag, envelope build).
    ///
    /// A peer cannot answer inside a timeout we spent ourselves, so long stalls void
    /// timeout evidence. Short ones are filtered here, so call sites report unconditionally
    /// from their slow branches. Returns whether the stall was long enough to record.
    pub fn record_local_stall(&self, duration: TimestampDuration) -> bool {
        if duration < self.timings.local_stall_min {
            return false;
        }
        let now = Timestamp::now();
        *self.last_local_stall.lock() = Some(now);
        // Commit promptly so gates and the transition log fire before the next read
        for routing_domain in RoutingDomain::all() {
            if !Self::uses_ping_detection(routing_domain) {
                continue;
            }
            let (credited, opt_transition) = {
                let mut inner = self.inner.lock();
                let pd = inner.entry(routing_domain).or_default();
                let credited = Self::credit_stall(pd, now, duration);
                let opt_transition = Self::commit_effective(&self.timings, pd, Some(now), now);
                self.publish_overlap_data(routing_domain, pd);
                (credited, opt_transition)
            };
            if let Some((start, end)) = credited {
                veilid_log!(self debug "{:?} stall credited: {:#} untestable, ending {:#} ago", routing_domain, end.duration_since(start), now.duration_since(end));
            }
            self.log_effective_transition(routing_domain, opt_transition, "local stall");
        }

        // Past the inactivity timeout the far end or the NAT has dropped every connection
        let inactivity_timeout = TimestampDuration::new_ms(
            self.config()
                .internal()
                .network
                .connection_inactivity_timeout_ms as u64,
        );
        if duration >= inactivity_timeout {
            if let Some(connection_manager) = self.network_manager().opt_connection_manager() {
                veilid_log!(self debug "purging connections: stalled {:#}", duration);
                connection_manager.close_all_connections();
            }
        }
        true
    }

    /// Credit the stall itself as time nobody could be tested.
    ///
    /// The degraded window that follows starts at the thaw, so on its own it
    /// credits nothing for the stall: a process frozen for minutes came back to
    /// find every entry's silence counted against it and demoted its reliable
    /// peers for its own absence. Recorded as an offline interval, which is what
    /// a stall is to the entries (no test was possible) and which also makes
    /// the thaw a reattach for `online_since`, so a peer last seen before the
    /// freeze keeps its standing until it has been re-tested.
    ///
    /// Overlapping intervals credit twice, so the stall is trimmed to what no
    /// interval covers: one that began before it clips the start, one that began
    /// at the thaw, before this tick ran, clips the end. Degraded time is unblamed
    /// but not untestable, so a degraded window never covers a stall: an open one
    /// is split at it and recorded ones are cut back to their part before it.
    /// Returns what was credited.
    pub(crate) fn credit_stall(
        pd: &mut OnlineDetectionPerDomain,
        now: Timestamp,
        duration: TimestampDuration,
    ) -> Option<(Timestamp, Timestamp)> {
        let stall_start = now.earlier(duration);
        if let Some(s) = pd.degraded_started_ts.filter(|s| *s < stall_start) {
            let weight = pd.degraded_blame.map_or(0.0, |b| b.mean(s, stall_start));
            pd.recent_degraded_intervals
                .push_back((s, stall_start, weight));
            veilid_stats!(
                pd.total_time_degraded = pd
                    .total_time_degraded
                    .saturating_add(stall_start.duration_since(s))
            );
            pd.degraded_started_ts = Some(now);
            pd.degraded_blame = pd.degraded_blame.map(|b| DegradedBlame::new(b.weight, now));
        }
        for iv in pd.recent_degraded_intervals.iter_mut() {
            if iv.0 < stall_start && iv.1 > stall_start {
                iv.1 = stall_start;
            }
        }
        pd.recent_degraded_intervals
            .retain(|(s, _, _)| *s < stall_start);
        let mut covered: Vec<(Timestamp, Timestamp)> = pd
            .recent_offline_intervals
            .iter()
            .copied()
            .chain(
                pd.recent_degraded_intervals
                    .iter()
                    .map(|(s, e, _)| (*s, *e)),
            )
            .chain(
                [pd.offline_started_ts, pd.degraded_started_ts]
                    .into_iter()
                    .flatten()
                    .map(|s| (s, now)),
            )
            .collect();
        covered.sort_unstable();
        let mut start = stall_start;
        let mut end = now;
        for (s, e) in covered {
            if e <= start {
                continue;
            }
            if s <= start {
                start = e;
            } else {
                end = s;
                break;
            }
        }
        if start >= end {
            return None;
        }
        // A blip recorded at the thaw already sits past the stall; keep the order
        let at = pd
            .recent_offline_intervals
            .iter()
            .position(|(s, _)| *s > start)
            .unwrap_or(pd.recent_offline_intervals.len());
        pd.recent_offline_intervals.insert(at, (start, end));
        let excess = pd
            .recent_offline_intervals
            .len()
            .saturating_sub(CREDIT_INTERVALS_KEPT);
        pd.recent_offline_intervals.drain(..excess);
        Some((start, end))
    }

    /// Each detecting domain's committed state and loss estimate, one line each
    pub fn debug_info(&self) -> String {
        RoutingDomain::all()
            .into_iter()
            .filter_map(|rd| {
                self.read_committed(rd, |pd| (pd.effective_state, pd.loss_estimator.estimate()))
                    .map(|(state, loss)| format!("{:?}: {:?}, {}\n", rd, state, loss))
            })
            .collect()
    }

    /// Share of round trips our own link lets through, for `failure_backoff`: 1 while online, the
    /// loss estimate while degraded, None while offline
    pub fn survival(&self, routing_domain: RoutingDomain) -> Option<f64> {
        self.read_committed(routing_domain, |pd| match pd.effective_state {
            OnlineState::Online => Some(1.0),
            OnlineState::Degraded => Some(pd.loss_estimator.estimate().survival),
            OnlineState::Offline => None,
        })
        .unwrap_or(Some(1.0))
    }

    /// The weight a failure observed now carries against a route: full while online, our link's
    /// measured survival while its loss holds the domain degraded. None while offline or degraded
    /// only by stalls and failure bursts, when the failure is about us.
    pub fn blame_weight(&self, routing_domain: RoutingDomain) -> Option<f64> {
        self.read_committed(routing_domain, |pd| {
            let loss = pd.loss_estimator.estimate();
            match pd.effective_state {
                OnlineState::Online => Some(1.0),
                OnlineState::Degraded if loss.degraded => Some(loss.survival),
                OnlineState::Degraded | OnlineState::Offline => None,
            }
        })
        .unwrap_or(Some(1.0))
    }

    /// Whether blame writers should hold fire for this routing domain.
    pub fn blame_suppressed(&self, routing_domain: RoutingDomain) -> bool {
        self.online_state(routing_domain).suppresses_blame()
    }

    /// Current online state for a routing domain.
    pub fn online_state(&self, routing_domain: RoutingDomain) -> OnlineState {
        // Domains without ping detection are always considered online.
        self.read_committed(routing_domain, |pd| pd.effective_state)
            .unwrap_or(OnlineState::Online)
    }

    /// How many times a routing domain's online state has changed; evidence gathered under one
    /// count no longer applies once it moves
    pub fn state_epoch(&self, routing_domain: RoutingDomain) -> u64 {
        self.read_committed(routing_domain, |pd| pd.effective_transitions)
            .unwrap_or(0)
    }

    /// Commit the effective state as of now and read from it; None for domains without ping
    /// detection
    fn read_committed<R>(
        &self,
        routing_domain: RoutingDomain,
        read: impl FnOnce(&OnlineDetectionPerDomain) -> R,
    ) -> Option<R> {
        if !Self::uses_ping_detection(routing_domain) {
            return None;
        }
        let local_stall_ts = *self.last_local_stall.lock();
        let now = Timestamp::now();
        let (out, opt_transition) = {
            let mut inner = self.inner.lock();
            let pd = inner.entry(routing_domain).or_default();
            let opt_transition = Self::commit_effective(&self.timings, pd, local_stall_ts, now);
            self.publish_overlap_data(routing_domain, pd);
            (read(pd), opt_transition)
        };
        self.log_effective_transition(routing_domain, opt_transition, "evidence aged");
        Some(out)
    }

    /// When the latest Offline or Degraded period ended; None while one is under way, or if none
    /// is remembered
    pub fn loss_ended_ts(&self, routing_domain: RoutingDomain) -> Option<Timestamp> {
        self.read_committed(routing_domain, |pd| {
            if pd.offline_started_ts.is_some() || pd.degraded_started_ts.is_some() {
                return None;
            }
            let offline_end = pd.recent_offline_intervals.back().map(|(_, end)| *end);
            let degraded_end = pd.recent_degraded_intervals.back().map(|(_, end, _)| *end);
            offline_end.max(degraded_end)
        })
        .flatten()
    }

    /// When this domain last came back online, if within the reliable ping span.
    /// None if currently offline or no recent reattach.
    pub fn online_since(&self, routing_domain: RoutingDomain) -> Option<Timestamp> {
        if !Self::uses_ping_detection(routing_domain) {
            return None;
        }
        let inner = self.inner.lock();
        let pd = inner.get(&routing_domain)?;
        if pd.offline_started_ts.is_some() {
            return None;
        }
        let (_, end) = pd.recent_offline_intervals.back()?;
        // Protection from a reattach only lasts the reliable ping span.
        if Timestamp::now().duration_since(*end) > crate::routing_table::RELIABLE_PING_INTERVAL_MAX
        {
            return None;
        }
        Some(*end)
    }

    /// Total time within `[from, to]` this domain was offline (couldn't ping).
    pub fn offline_overlap(
        &self,
        routing_domain: RoutingDomain,
        from: Timestamp,
        to: Timestamp,
    ) -> TimestampDuration {
        self.interval_overlap(routing_domain, from, to, false)
    }

    /// The same, plus the unblamed share of degraded time: all of it during stalls and bursts,
    /// one minus our link's survival under measured loss.
    ///
    /// Only for spans that condemn. Degraded time still carries positive evidence
    /// (successes are recorded throughout), so it must not be deducted from how long a
    /// node has been answering.
    pub fn blame_suppressed_overlap(
        &self,
        routing_domain: RoutingDomain,
        from: Timestamp,
        to: Timestamp,
    ) -> TimestampDuration {
        self.interval_overlap(routing_domain, from, to, true)
    }

    fn interval_overlap(
        &self,
        routing_domain: RoutingDomain,
        from: Timestamp,
        to: Timestamp,
        include_degraded: bool,
    ) -> TimestampDuration {
        if !Self::uses_ping_detection(routing_domain) || to <= from {
            return TimestampDuration::new(0);
        }
        // Snapshot, so per-entry credit math never contends with the state machine
        let Some(od) = self.overlap_data.read().get(&routing_domain).cloned() else {
            return TimestampDuration::new(0);
        };
        od.overlap(from, to, include_degraded)
    }

    /// Stats snapshot: (online, degraded, transitions, time offline, time degraded).
    #[cfg(feature = "stats")]
    pub(crate) fn online_stats(
        &self,
        routing_domain: RoutingDomain,
        now: Timestamp,
    ) -> (bool, bool, u64, TimestampDuration, TimestampDuration) {
        let inner = self.inner.lock();
        match inner.get(&routing_domain) {
            Some(pd) => {
                let mut time_offline = pd.total_time_offline;
                if let Some(start) = pd.offline_started_ts {
                    time_offline = time_offline.saturating_add(now.duration_since(start));
                }
                let mut time_degraded = pd.total_time_degraded;
                if let Some(start) = pd.degraded_started_ts {
                    time_degraded = time_degraded.saturating_add(now.duration_since(start));
                }
                (
                    matches!(pd.online_state, OnlineState::Online),
                    matches!(pd.effective_state, OnlineState::Degraded),
                    pd.total_transitions,
                    time_offline,
                    time_degraded,
                )
            }
            None => (
                true,
                false,
                0,
                TimestampDuration::new(0),
                TimestampDuration::new(0),
            ),
        }
    }

    /// Recompute and commit the effective three-state answer; returns a transition for the
    /// caller to log outside the lock.
    pub(crate) fn commit_effective(
        timings: &OnlineDetectorTimings,
        pd: &mut OnlineDetectionPerDomain,
        local_stall_ts: Option<Timestamp>,
        now: Timestamp,
    ) -> Option<EffectiveTransition> {
        let evidence = pd.last_degraded_evidence_ts.max(local_stall_ts);
        let clean = degraded_exit_clean(timings, pd.degraded_reentries);
        let learn = pd.effective_state == OnlineState::Online;
        let loss = pd.loss_estimator.evaluate(timings, learn, now);
        let mut new_state = effective_online_state(
            timings,
            pd.online_state,
            evidence,
            pd.last_success_ts,
            clean,
            now,
        );
        // Loss on our own link holds Degraded on its own evidence
        if new_state == OnlineState::Online && loss.degraded {
            new_state = OnlineState::Degraded;
        }
        let old_state = pd.effective_state;
        let blame_weight = if loss.degraded { loss.survival } else { 0.0 };
        // Advance only on a new weight: each advance republishes the overlap snapshot
        if let Some(blame) = pd
            .degraded_blame
            .as_mut()
            .filter(|b| b.weight != blame_weight)
        {
            blame.advance(blame_weight, now);
        }
        if new_state == old_state {
            pd.degraded_by_loss |= new_state == OnlineState::Degraded && loss.degraded;
            return None;
        }
        let mut local_stall_only = false;
        // Decided on entry, held for the whole degraded period
        match (old_state, new_state) {
            (OnlineState::Degraded, _) => {
                // Broad failures and loss are the other evidence, and the last exit cleared them
                local_stall_only = pd.last_degraded_evidence_ts.is_none() && !pd.degraded_by_loss;
                pd.degraded_by_loss = false;
                pd.degraded_recovered_ts = Some(now);
                // The window outlives the clean period; without this one more failure
                // re-crosses the threshold on the old entries
                pd.windowed_failures.clear();
                pd.last_degraded_evidence_ts = None;
            }
            (_, OnlineState::Degraded) => {
                pd.degraded_by_loss = loss.degraded;
                let rapid = pd
                    .degraded_recovered_ts
                    .is_some_and(|ts| now.duration_since(ts) <= timings.degraded_reentry);
                pd.degraded_reentries = if rapid {
                    pd.degraded_reentries.saturating_add(1)
                } else {
                    0
                };
            }
            _ => {}
        }
        pd.effective_state = new_state;
        pd.effective_transitions += 1;
        if new_state == OnlineState::Degraded {
            pd.degraded_started_ts = Some(now);
            pd.degraded_blame = Some(DegradedBlame::new(blame_weight, now));
        } else if let Some(start) = pd.degraded_started_ts.take() {
            veilid_stats!(
                pd.total_time_degraded = pd
                    .total_time_degraded
                    .saturating_add(now.duration_since(start))
            );
            let weight = pd.degraded_blame.take().map_or(0.0, |b| b.mean(start, now));
            pd.recent_degraded_intervals.push_back((start, now, weight));
            let excess = pd
                .recent_degraded_intervals
                .len()
                .saturating_sub(CREDIT_INTERVALS_KEPT);
            pd.recent_degraded_intervals.drain(..excess);
        }
        Some(EffectiveTransition {
            old_state,
            new_state,
            clean: degraded_exit_clean(timings, pd.degraded_reentries),
            local_stall_only,
            loss,
        })
    }

    /// Degraded transitions log here; online and offline log from the stored machine.
    fn log_effective_transition(
        &self,
        routing_domain: RoutingDomain,
        opt_transition: Option<EffectiveTransition>,
        why: &str,
    ) {
        let Some(EffectiveTransition {
            old_state,
            new_state,
            clean,
            local_stall_only,
            loss,
        }) = opt_transition
        else {
            return;
        };
        match (old_state, new_state) {
            (_, OnlineState::Degraded) => {
                let why = if loss.degraded {
                    "lost round trips"
                } else {
                    why
                };
                veilid_log!(self info "{:?} degraded detected ({}): blame suppressed, exit needs {} clean, {}{}", routing_domain, why, clean, loss, executor_snapshot_suffix());
            }
            (OnlineState::Degraded, OnlineState::Online) => {
                let cause = if local_stall_only {
                    " (local stalls only)"
                } else {
                    ""
                };
                veilid_log!(self info "{:?} recovered from degraded{}, {}", routing_domain, cause, loss);
            }
            _ => {}
        }
        if new_state == OnlineState::Online {
            self.network_manager()
                .transport_stats()
                .release_canaries(routing_domain.into(), "back online");
        }
    }

    /// Drive the state machine with an event, dispatching to the current state's handler.
    fn process_event(&self, routing_domain: RoutingDomain, event: OnlineEvent) {
        if !Self::uses_ping_detection(routing_domain) {
            return;
        }
        let enabled_framing_types = *self.enabled_framing_types.lock();
        let local_stall_ts = *self.last_local_stall.lock();
        let now = Timestamp::now();

        let (
            opt_new_state,
            opt_offline_evidence,
            opt_flap_penalty,
            do_purge,
            purge_reason,
            do_wipe,
            opt_eff_transition,
            opt_loss_log,
        ) = {
            let mut inner = self.inner.lock();
            let pd = inner.entry(routing_domain).or_default();
            // Exiting Degraded needs a round trip after the evidence, so stamp in every state
            if matches!(event, OnlineEvent::Success { .. }) {
                pd.last_success_ts = Some(now);
            }
            if matches!(event, OnlineEvent::NetworkChange | OnlineEvent::Detached) {
                pd.loss_estimator.reset();
            }
            let opt_new_state = match pd.online_state {
                OnlineState::Offline => Self::process_offline_event(&event, pd.offline_started_ts),
                OnlineState::Online | OnlineState::Degraded => Self::process_online_event(
                    &self.timings,
                    pd,
                    &event,
                    enabled_framing_types,
                    local_stall_ts,
                    now,
                ),
            };
            let mut opt_flap_penalty = None;
            let mut opt_offline_evidence = None;
            let mut do_wipe = false;
            if let Some(new_state) = opt_new_state {
                match new_state {
                    // PANIC-CENSUS: provably-dead the stored machine is two-state; Degraded
                    // is the derived overlay and is never stored
                    OnlineState::Degraded => unreachable!(),
                    OnlineState::Offline => {
                        pd.offline_started_ts = Some(now);
                        pd.offline_purged = false;
                        if matches!(event, OnlineEvent::Failure { .. }) {
                            opt_offline_evidence = Some(describe_failures(&pd.pending_failures));
                        }
                        pd.pending_failures.clear();
                    }
                    OnlineState::Online => {
                        pd.loss_estimator.clear_window();
                        if let Some(start) = pd.offline_started_ts.take() {
                            // Only a sustained outage invalidates transport evidence
                            do_wipe = now.duration_since(start) >= self.timings.offline_sustained;
                            pd.recent_offline_intervals.push_back((start, now));
                            veilid_stats!(
                                pd.total_time_offline = pd
                                    .total_time_offline
                                    .saturating_add(now.duration_since(start))
                            );
                            let excess = pd
                                .recent_offline_intervals
                                .len()
                                .saturating_sub(CREDIT_INTERVALS_KEPT);
                            pd.recent_offline_intervals.drain(..excess);
                        }
                    }
                }
                pd.online_state = new_state;
                pd.last_transition_ts = now;
                veilid_stats!(pd.total_transitions += 1);
                // A deliberate detach is a clean break, not network flapping: reset the
                // flap baseline so pre-detach history doesn't combine with reattach turbulence.
                if matches!(event, OnlineEvent::Detached) {
                    pd.flap_detector.reset();
                } else {
                    opt_flap_penalty = pd.flap_detector.record(now.as_u64(), new_state);
                }
            }

            // Purge policy: a NAT rebind black-holes established flows, so a real outage
            // must close everything and a blip must not. An interface change is a confirmed
            // move (purge at once); ping-derived offline must persist first, rechecked
            // on each event while offline.
            let mut do_purge = false;
            let mut purge_reason = "";
            if pd.online_state == OnlineState::Offline && !matches!(event, OnlineEvent::Detached) {
                // A move purges even mid-outage: recovery attempts may have opened
                // fresh connections on the old interface since the last purge
                let confirmed_move = matches!(event, OnlineEvent::NetworkChange);
                let sustained = !pd.offline_purged
                    && pd.offline_started_ts.is_some_and(|start| {
                        now.duration_since(start) >= self.timings.offline_sustained
                    });
                if confirmed_move || sustained {
                    pd.offline_purged = true;
                    do_purge = true;
                    purge_reason = if confirmed_move {
                        "interface change"
                    } else {
                        "sustained offline"
                    };
                }
            }
            // After the state step, so an answer that ends an outage is not counted in its window
            let opt_loss_log = match &event {
                OnlineEvent::Success {
                    sent_ts: Some(sent_ts),
                    opt_direct: Some((node_id, transport)),
                } => Self::note_round_trip(
                    &self.timings,
                    pd,
                    node_id,
                    *transport,
                    *sent_ts,
                    true,
                    local_stall_ts,
                    now,
                ),
                _ => None,
            };
            let opt_eff_transition = Self::commit_effective(&self.timings, pd, local_stall_ts, now);
            self.publish_overlap_data(routing_domain, pd);
            (
                opt_new_state,
                opt_offline_evidence,
                opt_flap_penalty,
                do_purge,
                purge_reason,
                do_wipe,
                opt_eff_transition,
                opt_loss_log,
            )
        };
        // `debug network stats` shows the estimate on demand
        if let Some(estimate) = opt_loss_log.filter(LossEstimate::shows_loss) {
            veilid_log!(self debug "{:?} {}", routing_domain, estimate);
        }

        // Log the transition (and any flapping) outside the lock.
        if let Some(new_state) = opt_new_state {
            veilid_log!(self info "{:?} {} detected", routing_domain,
                if new_state == OnlineState::Online { "online" } else { "offline" });
        }
        if let Some(evidence) = opt_offline_evidence {
            veilid_log!(self debug "{:?} offline evidence: {}", routing_domain, evidence);
        }
        self.log_effective_transition(routing_domain, opt_eff_transition, "broad failures");
        if do_purge {
            // close_all_connections is silent on an empty table; log the decision
            veilid_log!(self debug "{:?} purging connections: {}", routing_domain, purge_reason);
            if let Some(connection_manager) = self.network_manager().opt_connection_manager() {
                connection_manager.close_all_connections();
            }
        }
        if do_wipe {
            self.network_manager()
                .transport_stats()
                .reset_routing_domain(routing_domain, "sustained offline ended");
        }
        if let Some(penalty) = opt_flap_penalty {
            veilid_log!(self debugwarn "{:?} online state FLAPPING (penalty={:.1})", routing_domain, penalty);
        }
    }

    /// Handle an event while offline.
    fn process_offline_event(
        event: &OnlineEvent,
        offline_started_ts: Option<Timestamp>,
    ) -> Option<OnlineState> {
        match event {
            // Only an answer to a question sent after going offline proves the network we are
            // on now; an answer to an older question, or anything arriving unasked, can be the
            // old network's echo and would flip us online onto a dead one
            OnlineEvent::Success { sent_ts, .. } => match (*sent_ts, offline_started_ts) {
                (Some(sent), Some(started)) if sent > started => Some(OnlineState::Online),
                (Some(_), None) => Some(OnlineState::Online),
                _ => None,
            },
            OnlineEvent::Failure { .. } | OnlineEvent::NetworkChange | OnlineEvent::Detached => {
                None
            }
        }
    }

    /// Handle an event while online.
    pub(crate) fn process_online_event(
        timings: &OnlineDetectorTimings,
        pd: &mut OnlineDetectionPerDomain,
        event: &OnlineEvent,
        enabled_framing_types: FramingTypeSet,
        local_stall_ts: Option<Timestamp>,
        now: Timestamp,
    ) -> Option<OnlineState> {
        match event {
            OnlineEvent::Success { .. } => {
                pd.pending_failures.clear();
                None
            }
            OnlineEvent::Failure {
                live_node_ids,
                fresh_node_ids,
                responsive_count,
                sent_ts,
                framing_type,
            } => {
                // Ignore RPCs sent before this online period began or the last stall ended
                if *sent_ts < pd.last_transition_ts
                    || local_stall_ts.is_some_and(|ts| *sent_ts < ts)
                {
                    return None;
                }
                // A loss for a question sent before the last answer arrived overlaps proof the
                // path worked; only the degraded window takes it
                if pd.last_success_ts.is_none_or(|s| *sent_ts > s) {
                    let bucket = pd.pending_failures.entry(*framing_type).or_default();
                    for node_id in live_node_ids {
                        bucket.insert(node_id.clone());
                    }
                }
                // Only fresh failures arm the degraded window; the offline breadth
                // above keeps the full live set
                let wbucket = pd.windowed_failures.entry(*framing_type).or_default();
                for node_id in fresh_node_ids {
                    wbucket.insert(node_id.clone(), now);
                }
                // Windowed breadth survives interleaved successes: the degraded signature
                for wb in pd.windowed_failures.values_mut() {
                    wb.retain(|_, ts| now.duration_since(*ts) <= timings.degraded_evidence_window);
                }
                // Degraded needs a table-relative slice of previously-clean peers
                // failing at once; the flat offline threshold below stays as-is
                let degraded_threshold = |ft: FramingType| {
                    failure_threshold(ft).max(responsive_count / DEGRADED_BREADTH_TABLE_DIVISOR)
                };
                let all_windowed = !enabled_framing_types.is_empty()
                    && enabled_framing_types.iter().all(|ft| {
                        pd.windowed_failures.get(&ft).map_or(0, BTreeMap::len)
                            >= degraded_threshold(ft)
                    });
                if all_windowed {
                    pd.last_degraded_evidence_ts = Some(now);
                }
                // Offline only when every enabled socket class has crossed its threshold, so
                // localized trouble on one class doesn't drive offline while another works.
                let all_failed = !enabled_framing_types.is_empty()
                    && enabled_framing_types.iter().all(|ft| {
                        pd.pending_failures.get(&ft).map_or(0, BTreeSet::len)
                            >= failure_threshold(ft)
                    });
                if all_failed {
                    return Some(OnlineState::Offline);
                }
                None
            }
            OnlineEvent::NetworkChange | OnlineEvent::Detached => {
                // Degradation evidence belongs to the network we just left
                pd.windowed_failures.clear();
                pd.last_degraded_evidence_ts = None;
                pd.degraded_reentries = 0;
                pd.degraded_recovered_ts = None;
                Some(OnlineState::Offline)
            }
        }
    }
}
