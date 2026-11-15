use super::*;

/// How long to wait without receiving a ValueChanged before triggering a fallback
/// change inspection to detect missed updates. Silence is not evidence that a watch
/// is broken, so this is the interval after evidence that it is live, and it doubles
/// up to the configured ceiling while inspections keep finding nothing.
const WATCH_FALLBACK_INSPECT_INTERVAL: TimestampDuration = TimestampDuration::new_secs(30);

#[derive(Clone, Debug, Serialize, Deserialize)]
pub(in crate::storage_manager) struct OutboundWatchState {
    /// Requested parameters
    params: OutboundWatchParameters,
    /// Nodes that have an active watch on our behalf
    nodes: Vec<PerNodeKey>,
    /// How many value change updates remain
    remaining_count: u32,
    /// The next earliest time we are willing to try to reconcile and improve the watch
    opt_next_reconcile_ts: Option<Timestamp>,
    /// The number of nodes we got at our last reconciliation
    opt_last_consensus_node_count: Option<usize>,
    /// Calculated field: minimum expiration time for all our nodes
    min_expiration: Timestamp,
    /// Calculated field: the set of value changed routes for this watch from all per node watches
    value_changed_routes: BTreeSet<PublicKey>,
    /// When the watch was established or last renewed/reconciled
    watch_established_ts: Timestamp,
    /// When the last ValueChanged RPC arrived from a remote node
    last_value_changed_ts: Option<Timestamp>,
    /// When the last fallback change inspection ran
    last_fallback_inspect_ts: Option<Timestamp>,
    /// Fallback interval: doubled per empty inspection, reset when the watch proves live or changed
    #[serde(default)]
    opt_fallback_inspect_interval: Option<TimestampDuration>,
    /// Earliest the next fallback inspection may run, after a spread reset or under loss
    #[serde(default, skip)]
    opt_fallback_not_before: Option<Timestamp>,
    /// Consecutive reconciles that ended short of consensus, which size the next retry
    #[serde(default, skip)]
    reconcile_retries: u32,
    /// Renewals sent in the current run of push route deaths
    #[serde(default, skip)]
    route_renewals: u32,
    /// When the last of them went out
    #[serde(default, skip)]
    opt_last_route_renewal_ts: Option<Timestamp>,
    /// Fallback inspection found missed changes; forces a reconcile for fresh notification routes
    needs_forced_reconcile: bool,
    /// Last seq hinted to the app per subkey; hints only rise, so replays and duplicates drop
    #[serde(default)]
    last_reported_subkey_seqs: BTreeMap<ValueSubkey, ValueSeqNum>,
    /// Subkeys reported to the app and not yet read back, with a count of range-only copies
    /// suppressed while unread. Every serving node sends its own ValueChanged per change, and a
    /// seq-less range report has only consumption to dedup by.
    #[serde(default, skip)]
    unread_reported_subkeys: BTreeMap<ValueSubkey, u32>,
}

impl fmt::Display for OutboundWatchState {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut value_changed_routes = self
            .value_changed_routes
            .iter()
            .map(|x| x.to_string())
            .collect::<Vec<_>>();
        value_changed_routes.sort_unstable();

        write!(
            f,
            r#"params: {}
nodes: [{}]
remaining_count: {}
opt_next_reconcile_ts: {}
opt_consensus_node_count: {}
min_expiration: {}
value_changed_routes: [{}]"#,
            self.params,
            self.nodes
                .iter()
                .map(|x| x.node_id.to_string())
                .collect::<Vec<_>>()
                .join(","),
            self.remaining_count,
            if let Some(next_reconcile_ts) = &self.opt_next_reconcile_ts {
                next_reconcile_ts.to_string()
            } else {
                "None".to_owned()
            },
            if let Some(consensus_node_count) = &self.opt_last_consensus_node_count {
                consensus_node_count.to_string()
            } else {
                "None".to_owned()
            },
            self.min_expiration,
            value_changed_routes.join(","),
        )
    }
}

pub(in crate::storage_manager) struct OutboundWatchStateEditor<'a> {
    state: &'a mut OutboundWatchState,
}

impl OutboundWatchStateEditor<'_> {
    pub fn set_params(&mut self, params: OutboundWatchParameters) {
        self.state.params = params;
    }
    pub fn add_nodes<I: IntoIterator<Item = PerNodeKey>>(&mut self, nodes: I) {
        for node in nodes {
            if !self.state.nodes.contains(&node) {
                self.state.nodes.push(node);
            }
        }
    }
    pub fn retain_nodes<F: FnMut(&PerNodeKey) -> bool>(&mut self, f: F) {
        self.state.nodes.retain(f);
    }
    pub fn set_remaining_count(&mut self, remaining_count: u32) {
        self.state.remaining_count = remaining_count;
    }
    pub fn set_next_reconcile_ts(&mut self, next_reconcile_ts: Timestamp) {
        self.state.opt_next_reconcile_ts = Some(next_reconcile_ts);
    }
    pub fn set_reconcile_retries(&mut self, reconcile_retries: u32) {
        self.state.reconcile_retries = reconcile_retries;
    }
    /// A renewal went out because a push route died; a death after the ceiling of quiet starts a
    /// new run
    pub fn record_route_renewal(&mut self, now: Timestamp, ceiling: TimestampDuration) {
        if self
            .state
            .opt_last_route_renewal_ts
            .is_some_and(|ts| now.duration_since(ts) > ceiling)
        {
            self.state.route_renewals = 0;
        }
        self.state.route_renewals = self.state.route_renewals.saturating_add(1);
        self.state.opt_last_route_renewal_ts = Some(now);
    }
    pub fn update_last_consensus_node_count(&mut self) {
        self.state.opt_last_consensus_node_count = Some(self.state.nodes().len());
    }
    pub fn touch_value_changed(&mut self, ts: Timestamp) {
        // Keeps the backed-off interval so a burst of deliveries cannot line up a re-check wave
        self.state.last_value_changed_ts = Some(ts);
    }
    pub fn touch_fallback_inspect(&mut self, ts: Timestamp) {
        self.state.last_fallback_inspect_ts = Some(ts);
        self.state.opt_fallback_not_before = None;
    }
    /// An inspection was answered with nothing newer: wait longer before asking again,
    /// up to `max_interval`
    pub fn back_off_fallback_inspect(&mut self, max_interval: TimestampDuration) {
        let cur = self.state.fallback_inspect_interval(max_interval);
        self.state.opt_fallback_inspect_interval =
            Some(TimestampDuration::new(cur.as_u64().saturating_mul(2)).min(max_interval));
    }
    /// Something that could have hidden a missed update: a productive inspection, the watch's
    /// node set or parameters moving, or a failure on its delivery path. A failure many watches
    /// share passes `opt_spread_from`, its time: the next inspection then waits for this watch's
    /// own phase of the base interval, so theirs spread over it rather than all coming due at once.
    pub fn reset_fallback_inspect_backoff(&mut self, opt_spread_from: Option<Timestamp>) {
        self.state.opt_fallback_inspect_interval = None;
        self.state.opt_fallback_not_before = opt_spread_from.map(|now| {
            let reference_ts = self.state.fallback_reference_ts();
            let base = WATCH_FALLBACK_INSPECT_INTERVAL.as_u64();
            let periods = now.duration_since(reference_ts).as_u64().div_ceil(base);
            reference_ts.later(TimestampDuration::new(periods * base))
        });
    }
    pub fn set_forced_reconcile(&mut self) {
        self.state.needs_forced_reconcile = true;
    }
    pub fn clear_forced_reconcile(&mut self) {
        self.state.needs_forced_reconcile = false;
    }
    /// Under our own loss the next fallback inspection waits the interval stretched by
    /// 1/`survival`, fixed now so that recovering does not bring every stretched watch due at once
    pub fn stretch_fallback_inspect(&mut self, survival: f64, max_interval: TimestampDuration) {
        if survival >= 1.0 {
            return;
        }
        let interval = self.state.fallback_inspect_interval(max_interval);
        let stretched = if survival > 0.0 {
            interval.f64_mul(1.0 / survival).min(max_interval)
        } else {
            max_interval
        };
        let due = self.state.fallback_reference_ts().later(stretched);
        self.state.opt_fallback_not_before = Some(
            self.state
                .opt_fallback_not_before
                .map_or(due, |nb| nb.max(due)),
        );
    }
    pub fn set_last_reported_subkey_seq(&mut self, subkey: ValueSubkey, seq: ValueSeqNum) {
        self.state.last_reported_subkey_seqs.insert(subkey, seq);
    }
}

impl OutboundWatchState {
    pub fn new(params: OutboundWatchParameters) -> Self {
        let remaining_count = params.count;
        let min_expiration = params.expiration;

        Self {
            params,
            nodes: vec![],
            remaining_count,
            opt_next_reconcile_ts: None,
            opt_last_consensus_node_count: None,
            min_expiration,
            value_changed_routes: BTreeSet::new(),
            watch_established_ts: Timestamp::now_non_decreasing(),
            last_value_changed_ts: None,
            last_fallback_inspect_ts: None,
            opt_fallback_inspect_interval: None,
            opt_fallback_not_before: None,
            reconcile_retries: 0,
            route_renewals: 0,
            opt_last_route_renewal_ts: None,
            needs_forced_reconcile: false,
            last_reported_subkey_seqs: BTreeMap::new(),
            unread_reported_subkeys: BTreeMap::new(),
        }
    }

    pub fn params(&self) -> &OutboundWatchParameters {
        &self.params
    }
    pub fn nodes(&self) -> &Vec<PerNodeKey> {
        &self.nodes
    }
    pub fn remaining_count(&self) -> u32 {
        self.remaining_count
    }
    pub fn next_reconcile_ts(&self) -> Option<Timestamp> {
        self.opt_next_reconcile_ts
    }
    pub fn min_expiration(&self) -> Timestamp {
        self.min_expiration
    }
    pub fn value_changed_routes(&self) -> &BTreeSet<PublicKey> {
        &self.value_changed_routes
    }
    pub fn last_reported_subkey_seq(&self, subkey: ValueSubkey) -> Option<ValueSeqNum> {
        self.last_reported_subkey_seqs.get(&subkey).copied()
    }

    /// Mark `subkeys` hinted to the app and return the unmarked news; an already-marked subkey
    /// counts the copy instead. Touches no calculated field, so it takes no editor.
    pub fn mark_hinted_subkeys(&mut self, subkeys: &ValueSubkeyRangeSet) -> ValueSubkeyRangeSet {
        let mut news = ValueSubkeyRangeSet::new();
        for subkey in subkeys.iter() {
            match self.unread_reported_subkeys.entry(subkey) {
                std::collections::btree_map::Entry::Vacant(v) => {
                    v.insert(0);
                    news.insert(subkey);
                }
                std::collections::btree_map::Entry::Occupied(mut o) => {
                    *o.get_mut() = o.get().saturating_add(1);
                }
            }
        }
        news
    }

    /// Mark `subkeys` reported with data; fresh data satisfies owed copies, so counts reset.
    pub fn mark_value_reported_subkeys(&mut self, subkeys: &ValueSubkeyRangeSet) {
        for subkey in subkeys.iter() {
            self.unread_reported_subkeys.insert(subkey, 0);
        }
    }

    /// Consume the report marks for subkeys the app read from the network, returning those that
    /// had copies suppressed while unread: a copy racing the read may be a newer change, so the
    /// watch owes the app one re-report.
    pub fn consume_read_subkeys(&mut self, subkeys: &ValueSubkeyRangeSet) -> ValueSubkeyRangeSet {
        let mut owed = ValueSubkeyRangeSet::new();
        for subkey in subkeys.iter() {
            if let Some(copies) = self.unread_reported_subkeys.remove(&subkey) {
                if copies > 0 {
                    owed.insert(subkey);
                }
            }
        }
        owed
    }

    /// Get the parameters we use if we're updating this state's per node watches
    pub fn get_per_node_params(
        &self,
        desired: &OutboundWatchParameters,
    ) -> OutboundWatchParameters {
        // Change the params to update count
        if self.params() != desired {
            // If parameters are changing, just use the desired parameters
            desired.clone()
        } else {
            // If this is a renewal of the same parameters,
            // use the current remaining update count for the rpc
            let mut renew_params = desired.clone();
            renew_params.count = self.remaining_count();
            renew_params
        }
    }

    pub fn edit<R, F: FnOnce(&mut OutboundWatchStateEditor) -> R>(
        &mut self,
        per_node_state: &HashMap<PerNodeKey, OutboundWatchPerNodeState>,
        closure: F,
    ) -> R {
        let mut editor = OutboundWatchStateEditor { state: self };
        let res = closure(&mut editor);

        // Update calculated fields
        self.min_expiration = self
            .nodes
            .iter()
            .map(|x| per_node_state.get(x).unwrap_or_log().expiration)
            .reduce(|a, b| a.min(b))
            .unwrap_or(self.params.expiration);

        self.value_changed_routes = self
            .nodes
            .iter()
            .filter_map(|x| {
                per_node_state
                    .get(x)
                    .cloned()
                    .unwrap_or_log()
                    .opt_value_changed_route
            })
            .collect();

        res
    }

    /// Whether a fallback inspection has fired with no change pushed since. The fallback stamps
    /// its time as it fires, so `needs_fallback_inspect` cannot answer this for its own report
    pub fn unpushed_since_fallback(&self) -> bool {
        self.last_fallback_inspect_ts
            .is_some_and(|f| self.last_value_changed_ts.is_none_or(|v| v < f))
    }

    /// Whether the current backed-off interval has passed in silence since the latest of
    /// establishment, the last fallback inspection and the last delivered change
    pub fn needs_fallback_inspect(
        &self,
        cur_ts: Timestamp,
        max_interval: TimestampDuration,
    ) -> bool {
        if self.opt_fallback_not_before.is_some_and(|ts| cur_ts < ts) {
            return false;
        }
        cur_ts
            >= self
                .fallback_reference_ts()
                .later(self.fallback_inspect_interval(max_interval))
    }

    /// The latest of establishment, the last fallback inspection and the last delivered change
    fn fallback_reference_ts(&self) -> Timestamp {
        [self.last_fallback_inspect_ts, self.last_value_changed_ts]
            .into_iter()
            .flatten()
            .fold(self.watch_established_ts, Timestamp::max)
    }

    /// Whether the fallback change inspection is due. Once the interval reaches its ceiling, a
    /// watch at consensus that has had a push since its last inspection waits for its renewal
    /// window: every node accepted it within one per-node lifetime and its pushes arrive, so one
    /// inspection per renewal covers it. Without a push it keeps the ceiling, which bounds how
    /// long pushes failing silently can hide changes, as it does for a watch short of consensus.
    /// `opt_loss_end` is when the latest Offline or Degraded period ended, with this watch's
    /// share of an interval: pushes from before it may have lost their successors, so the
    /// watch inspects again from that point on, spread so watches do not all go at once.
    pub fn fallback_inspect_due(
        &self,
        cur_ts: Timestamp,
        consensus_count: usize,
        renew_window: TimestampDuration,
        max_interval: TimestampDuration,
        opt_loss_end: Option<(Timestamp, TimestampDuration)>,
    ) -> bool {
        if !self.needs_fallback_inspect(cur_ts, max_interval) {
            return false;
        }
        if self.nodes.len() < consensus_count
            || self.fallback_inspect_interval(max_interval) < max_interval
            || !self.pushed_since_fallback()
        {
            return true;
        }
        if let Some((loss_end_ts, spread)) = opt_loss_end {
            if self.last_value_changed_ts.is_none_or(|ts| ts < loss_end_ts) {
                return cur_ts >= loss_end_ts.later(spread);
            }
        }
        cur_ts.later(renew_window) >= self.min_expiration
    }

    /// Whether a change was pushed since the last fallback inspection, or since establishment
    /// before the first
    fn pushed_since_fallback(&self) -> bool {
        let since = self
            .last_fallback_inspect_ts
            .unwrap_or(self.watch_established_ts);
        self.last_value_changed_ts.is_some_and(|ts| ts >= since)
    }

    /// The interval currently in force for this watch's fallback inspection
    pub fn fallback_inspect_interval(&self, max_interval: TimestampDuration) -> TimestampDuration {
        // Capped on read too: an interval saved under an older, higher ceiling persists
        self.opt_fallback_inspect_interval
            .unwrap_or(WATCH_FALLBACK_INSPECT_INTERVAL)
            .min(max_interval)
    }

    /// Consecutive reconciles that ended short of consensus
    pub fn reconcile_retries(&self) -> u32 {
        self.reconcile_retries
    }

    /// Whether a renewal for a dead push route may go out now: the first of a run at once, the
    /// rest through `failure_backoff` from `retry_base` up to the fallback ceiling
    pub fn route_renewal_due(
        &self,
        cur_ts: Timestamp,
        retry_base: TimestampDuration,
        max_interval: TimestampDuration,
        opt_survival: Option<f64>,
    ) -> bool {
        let Some(last_ts) = self.opt_last_route_renewal_ts else {
            return true;
        };
        let since = cur_ts.duration_since(last_ts);
        if self.route_renewals == 0 || since > max_interval {
            return true;
        }
        failure_backoff(
            retry_base,
            max_interval,
            self.route_renewals - 1,
            opt_survival,
        )
        .is_some_and(|delay| since >= delay)
    }

    /// Whether a forced reconcile is needed (fallback inspection found missed changes).
    pub fn needs_forced_reconcile(&self) -> bool {
        self.needs_forced_reconcile
    }
}
