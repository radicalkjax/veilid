use super::*;
use std::hash::{DefaultHasher, Hash, Hasher};

impl_veilid_log_facility!("stor");

#[derive(Clone, Debug, Serialize, Deserialize)]
pub(in crate::storage_manager) struct OutboundWatch {
    /// Record key being watched
    record_key: RecordKey,

    /// Current state, None means inactive/cancelled
    state: Option<OutboundWatchState>,

    /// Desired parameters, None means cancelled
    desired: Option<OutboundWatchParameters>,

    /// Last fallback change inspection of a never-established watch; a state's own timer takes over
    #[serde(skip)]
    opt_unestablished_inspect_ts: Option<Timestamp>,

    /// Change inspections in a row that failed, which size the next retry
    #[serde(skip)]
    change_inspect_failures: u32,
}

impl fmt::Display for OutboundWatch {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "Desired: {}\nState:\n{}\n",
            if let Some(desired) = &self.desired {
                desired.to_string()
            } else {
                "None".to_owned()
            },
            if let Some(state) = &self.state {
                indent_all_by(4, state.to_string())
            } else {
                "None".to_owned()
            },
        )
    }
}

impl OutboundWatch {
    /// Create new outbound watch with desired parameters
    pub fn new(record_key: RecordKey, desired: OutboundWatchParameters) -> Self {
        Self {
            record_key,
            state: None,
            desired: Some(desired),
            opt_unestablished_inspect_ts: None,
            change_inspect_failures: 0,
        }
    }

    /// Count a failed change inspection; returns the failures before it
    pub fn note_change_inspect_failure(&mut self) -> u32 {
        let failures = self.change_inspect_failures;
        self.change_inspect_failures = failures.saturating_add(1);
        failures
    }

    /// A change inspection completed
    pub fn clear_change_inspect_failures(&mut self) {
        self.change_inspect_failures = 0;
    }

    /// Whether this watch is due a fallback change inspection, and over which subkeys.
    /// A wanted but never-established watch has no state to keep a timer in, so it is
    /// covered here at the ceiling: reconcile is already retrying establishment, and until
    /// it lands this interval is delivery latency. `opt_loss_ended_ts` is when the latest
    /// Offline or Degraded period ended.
    pub fn needs_fallback_inspect(
        &self,
        cur_ts: Timestamp,
        consensus_count: usize,
        renew_window: TimestampDuration,
        max_interval: TimestampDuration,
        opt_loss_ended_ts: Option<Timestamp>,
    ) -> Option<ValueSubkeyRangeSet> {
        if let Some(state) = self.state.as_ref() {
            // A fixed share of the interval per record spreads the inspections a loss causes
            let opt_loss_end = opt_loss_ended_ts.map(|ts| {
                let mut hasher = DefaultHasher::new();
                self.record_key.hash(&mut hasher);
                let spread = hasher.finish() % max_interval.as_u64().max(1);
                (ts, TimestampDuration::new(spread))
            });
            return state
                .fallback_inspect_due(
                    cur_ts,
                    consensus_count,
                    renew_window,
                    max_interval,
                    opt_loss_end,
                )
                .then(|| state.params().subkeys.clone());
        }
        let desired = self.desired.as_ref()?;
        self.opt_unestablished_inspect_ts
            .map(|last| cur_ts >= last.later(max_interval))
            .unwrap_or(true)
            .then(|| desired.subkeys.clone())
    }

    /// Stamp the fallback inspection time, wherever this watch keeps it
    pub fn touch_fallback_inspect(
        &mut self,
        per_node_states: &HashMap<PerNodeKey, OutboundWatchPerNodeState>,
        cur_ts: Timestamp,
    ) {
        match self.state.as_mut() {
            Some(state) => state.edit(per_node_states, |editor| {
                editor.touch_fallback_inspect(cur_ts);
            }),
            None => self.opt_unestablished_inspect_ts = Some(cur_ts),
        }
    }

    /// Full record key
    pub fn record_key(&self) -> RecordKey {
        self.record_key.clone()
    }

    /// Get current watch state if it exists
    pub fn state(&self) -> Option<&OutboundWatchState> {
        self.state.as_ref()
    }

    /// Get mutable current watch state if it exists
    pub fn state_mut(&mut self) -> Option<&mut OutboundWatchState> {
        self.state.as_mut()
    }

    /// Clear current watch state
    pub fn clear_state(&mut self) {
        self.state = None;
    }

    /// Get or create current watch state
    pub fn state_mut_or_create<F: FnOnce() -> OutboundWatchParameters>(
        &mut self,
        make_parameters: F,
    ) -> &mut OutboundWatchState {
        if self.state.is_none() {
            self.state = Some(OutboundWatchState::new(make_parameters()));
        }
        self.state.as_mut().unwrap_or_log()
    }

    /// Get desired watch parameters if it exists
    pub fn desired(&self) -> Option<OutboundWatchParameters> {
        self.desired.clone()
    }

    /// Set desired watch parameters
    pub fn set_desired(&mut self, desired: Option<OutboundWatchParameters>) {
        self.desired = desired;
    }

    /// Check for desired state changes
    pub fn try_expire_desired_state(&mut self, cur_ts: Timestamp) {
        let Some(desired) = self.desired.as_ref() else {
            // No desired parameters means this is already done
            return;
        };

        // Check if desired parameters have expired
        if desired.expiration.as_u64() != 0 && desired.expiration <= cur_ts {
            // Expired
            self.set_desired(None);
            return;
        }

        // Check if the existing state has no remaining count
        if let Some(state) = self.state.as_ref() {
            if state.remaining_count() == 0 {
                // No remaining count
                self.set_desired(None);
            }
        }
    }

    /// Returns true if this outbound watch can be removed from the table
    pub fn is_dead(&self) -> bool {
        self.desired.is_none() && self.state.is_none()
    }

    /// Returns true if this outbound watch needs to be cancelled
    pub fn needs_cancel(&self, registry: &VeilidComponentRegistry) -> bool {
        if self.is_dead() {
            veilid_log!(registry warn "Should have checked for is_dead first");
            return false;
        }

        // If there is no current watch then there is nothing to cancel
        let Some(_state) = self.state.as_ref() else {
            return false;
        };

        // If the desired parameters is None then cancel
        let Some(_desired) = self.desired.as_ref() else {
            veilid_log!(registry debug target: "watch", "OutboundWatch({}): needs_cancel because desired is None", self.record_key);
            return true;
        };

        false
    }

    /// Returns true if this outbound watch can be renewed
    pub fn needs_renew(
        &self,
        registry: &VeilidComponentRegistry,
        consensus_count: usize,
        cur_ts: Timestamp,
    ) -> bool {
        if self.is_dead() || self.needs_cancel(registry) {
            veilid_log!(registry warn "Should have checked for is_dead and needs_cancel first");
            return false;
        }

        // If there is no current watch then there is nothing to renew
        let Some(state) = self.state.as_ref() else {
            return false;
        };

        // Should have desired parameters here
        let Some(desired) = self.desired.as_ref() else {
            veilid_log!(registry warn "needs_cancel should have returned true");
            return false;
        };

        // If we have a consensus, we can avoid fanout by renewing rather than reconciling
        // but if we don't have a consensus, we should defer to fanout to try to improve it
        if state.nodes().len() < consensus_count {
            return false;
        }

        // If we have a consensus but need to renew because some per-node watches
        // either expired or had their routes die, do it
        if self.wants_per_node_watch_update(registry, state, cur_ts) {
            veilid_log!(registry debug target: "watch", "OutboundWatch({}): needs_renew because per_node_watch wants update", self.record_key);
            return true;
        }

        // If the desired parameters have changed, then we should renew with them
        if state.params() != desired {
            veilid_log!(registry debug target: "watch", "OutboundWatch({}): needs_renew because desired params have changed: {} != {}", self.record_key, state.params(), desired);
            return true;
        }

        false
    }

    /// Returns true if there is work to be done on getting the outbound
    /// watch to its desired state
    pub fn needs_reconcile(
        &self,
        registry: &VeilidComponentRegistry,
        consensus_count: usize,
        cur_ts: Timestamp,
    ) -> bool {
        if self.is_dead()
            || self.needs_cancel(registry)
            || self.needs_renew(registry, consensus_count, cur_ts)
        {
            veilid_log!(registry warn "Should have checked for is_dead, needs_cancel, needs_renew first");
            return false;
        }

        // If desired is none, then is_dead() or needs_cancel() should have been true
        let Some(desired) = self.desired.as_ref() else {
            veilid_log!(registry warn "is_dead() or needs_cancel() should have been true");
            return false;
        };

        // If there is a desired watch but no current state, then reconcile
        let Some(state) = self.state() else {
            veilid_log!(registry debug target: "watch", "OutboundWatch({}): needs_reconcile because state is empty", self.record_key);
            return true;
        };

        // If the desired parameters have changed, then we should reconcile with them
        if state.params() != desired {
            veilid_log!(registry debug target: "watch", "OutboundWatch({}): needs_reconcile because desired params have changed: {} != {}", self.record_key, state.params(), desired);
            return true;
        }

        // If fallback inspection found missed changes, force a reconcile
        // to re-establish watch with fresh notification routes
        if state.needs_forced_reconcile() {
            veilid_log!(registry debug target: "watch", "OutboundWatch({}): needs_reconcile because fallback inspection found missed changes", self.record_key);
            return true;
        }

        // Below-consensus reconciles wait out the reconcile time so they
        // retry at a bounded pace instead of every tick
        if cur_ts < state.next_reconcile_ts().unwrap_or_default() {
            return false;
        }

        // If we are still working on getting the 'current' state to match
        // the 'desired' state, then do the reconcile if we are within the timeframe for it
        if state.nodes().len() < consensus_count {
            veilid_log!(registry debug target: "watch", "OutboundWatch({}): needs_reconcile because consensus count is too low {} < {}", self.record_key, state.nodes().len(), consensus_count);
            return true;
        }

        false
    }

    /// Returns true if we need to update our per-node watches due to expiration,
    /// or if they are all dead because the route died and needs to be updated
    fn wants_per_node_watch_update(
        &self,
        registry: &VeilidComponentRegistry,
        state: &OutboundWatchState,
        cur_ts: Timestamp,
    ) -> bool {
        // Renew in the window before the first per-node expiration; after it the ids are gone
        let renew_ts = cur_ts.later(registry.storage_manager().timings().renew_outbound_watches);
        if renew_ts >= state.min_expiration()
            && cur_ts < state.min_expiration()
            && (state.params().expiration.as_u64() == 0 || renew_ts < state.params().expiration)
        {
            veilid_log!(registry debug target: "watch", "OutboundWatch({}): wants_per_node_watch_update because cur_ts is in expiration renew window", self.record_key);
            return true;
        }

        let routing_table = registry.routing_table();
        let rss = routing_table.route_spec_store();
        let survival = routing_table
            .network_manager()
            .online_detector()
            .survival(RoutingDomain::PublicInternet);
        let max_interval = registry
            .storage_manager()
            .timings()
            .watch_inspect_max_interval;
        // Retries start one RPC timeout out, once the last renewal has answered or timed out
        let retry_base = registry.config().rpc_timeout();
        if !state.route_renewal_due(cur_ts, retry_base, max_interval, survival) {
            return false;
        }

        // See if any of our per node watches have a dead value changed route
        // if so, speculatively renew them
        for vcr in state.value_changed_routes() {
            if rss.get_route_id_for_key(vcr).is_none() {
                // Route we would receive value changes on is dead
                veilid_log!(registry debug target: "watch", "OutboundWatch({}): wants_per_node_watch_update because route is dead: {}", self.record_key, vcr);
                return true;
            }
        }

        false
    }
}
