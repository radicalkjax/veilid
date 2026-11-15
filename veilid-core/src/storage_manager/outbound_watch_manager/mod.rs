mod outbound_watch;
mod outbound_watch_parameters;
mod outbound_watch_per_node_state;
mod outbound_watch_state;
mod per_node_key;

pub(in crate::storage_manager) use outbound_watch::*;
pub(in crate::storage_manager) use outbound_watch_parameters::*;
pub(in crate::storage_manager) use outbound_watch_per_node_state::*;
pub(in crate::storage_manager) use outbound_watch_state::*;
pub(in crate::storage_manager) use per_node_key::*;

use super::*;

impl_veilid_log_facility!("stor");

/// What asked for a change inspection
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(in crate::storage_manager) enum ChangeInspectTrigger {
    /// The watch's own schedule: the fallback timer, a sweep, or a retry of either
    Watch,
    /// A reconcile that changed the watch's nodes, so its routes are already fresh
    Reconcile,
}

impl ChangeInspectTrigger {
    /// Whether a productive inspection may force a reconcile for fresh notification routes
    pub fn may_force_reconcile(self) -> bool {
        self == Self::Watch
    }

    /// Two requests for one record keep the one that may still force a reconcile
    pub fn merged_with(self, other: Self) -> Self {
        if self == Self::Watch || other == Self::Watch {
            Self::Watch
        } else {
            Self::Reconcile
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub(in crate::storage_manager) struct OutboundWatchManager {
    /// Registry for logging
    #[serde(skip)]
    opt_registry: Option<VeilidComponentRegistry>,
    /// Each watch per record key
    #[serde(skip)]
    pub outbound_watches: HashMap<OpaqueRecordKey, OutboundWatch>,
    /// Last known active watch per node+record
    #[serde(with = "serialize_hash_map_as_pairs")]
    pub per_node_states: HashMap<PerNodeKey, OutboundWatchPerNodeState>,
    /// Value changes awaiting inspection before reporting, with the earliest time each may run
    #[serde(skip)]
    pub needs_change_inspection:
        HashMap<RecordKey, (ValueSubkeyRangeSet, Timestamp, ChangeInspectTrigger)>,
    /// Subkeys owed a repeat hint per record, drained by the outbound watch check tick
    #[serde(skip)]
    pub pending_rehints: HashMap<RecordKey, ValueSubkeyRangeSet>,
    /// Records with a change inspection running, so a slow one is not run again concurrently
    #[serde(skip)]
    pub change_inspects_in_flight: HashSet<OpaqueRecordKey>,
    /// Watches to cancel at the node holding them, with the earliest time each may be sent
    #[serde(skip)]
    pub pending_watch_cancels: HashMap<(PerNodeKey, u64), Timestamp>,
    /// Watch cancels being sent, so a stray that pushes again is not cancelled twice at once
    #[serde(skip)]
    pub watch_cancels_in_flight: HashSet<(PerNodeKey, u64)>,
}

/// A watch to cancel at the node holding it
pub(in crate::storage_manager) struct WatchCancel {
    /// The record, and the node holding the watch
    pub pnk: PerNodeKey,
    /// The node's id for the watch
    pub watch_id: u64,
    /// The watch's signing key; None for the anonymous one
    pub opt_watcher: Option<KeyPair>,
    /// How to reach the node
    pub safety_selection: SafetySelection,
}

impl fmt::Display for OutboundWatchManager {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut out = format!("outbound_watches({}): [\n", self.outbound_watches.len());
        {
            let mut keys = self.outbound_watches.keys().cloned().collect::<Vec<_>>();
            keys.sort_unstable();

            for k in keys {
                let v = self.outbound_watches.get(&k).unwrap_or_log();
                out += &format!("  {}:\n{}\n", k, indent_all_by(4, v.to_string()));
            }
        }
        out += "]\n";
        out += &format!("per_node_states({}): [\n", self.per_node_states.len());
        {
            let mut keys = self.per_node_states.keys().cloned().collect::<Vec<_>>();
            keys.sort_unstable();

            for k in keys {
                let v = self.per_node_states.get(&k).unwrap_or_log();
                out += &format!("  {}:\n{}\n", k, indent_all_by(4, v.to_string()));
            }
        }
        out += "]\n";
        out += &format!(
            "needs_change_inspection({}): [\n",
            self.needs_change_inspection.len()
        );
        {
            let mut keys = self
                .needs_change_inspection
                .keys()
                .cloned()
                .collect::<Vec<_>>();
            keys.sort_unstable();

            for k in keys {
                let (subkeys, not_before, trigger) =
                    self.needs_change_inspection.get(&k).unwrap_or_log();
                out += &format!(
                    "  {}: {} not_before={} trigger={:?}\n",
                    k, subkeys, not_before, trigger
                );
            }
        }
        out += "]\n";

        write!(f, "{}", out)
    }
}

impl Default for OutboundWatchManager {
    fn default() -> Self {
        Self::new()
    }
}

impl OutboundWatchManager {
    pub fn new() -> Self {
        Self {
            opt_registry: None,
            outbound_watches: HashMap::new(),
            per_node_states: HashMap::new(),
            needs_change_inspection: HashMap::new(),
            pending_rehints: HashMap::new(),
            change_inspects_in_flight: HashSet::new(),
            pending_watch_cancels: HashMap::new(),
            watch_cancels_in_flight: HashSet::new(),
        }
    }

    /// Queue a cancel for the watch `watch_id` at `pnk`'s node, to send from `not_before`
    pub fn queue_watch_cancel(&mut self, pnk: PerNodeKey, watch_id: u64, not_before: Timestamp) {
        let key = (pnk, watch_id);
        if self.watch_cancels_in_flight.contains(&key) {
            return;
        }
        self.pending_watch_cancels.entry(key).or_insert(not_before);
    }

    /// The queued cancels due by `cur_ts`, now in flight. One whose watch has since become ours is
    /// dropped, as is one for a record no longer watched.
    pub fn take_due_watch_cancels(&mut self, cur_ts: Timestamp) -> Vec<WatchCancel> {
        let due: Vec<(PerNodeKey, u64)> = self
            .pending_watch_cancels
            .iter()
            .filter(|(_, not_before)| **not_before <= cur_ts)
            .map(|(key, _)| key.clone())
            .collect();
        let mut out = Vec::new();
        for (pnk, watch_id) in due {
            self.pending_watch_cancels.remove(&(pnk.clone(), watch_id));
            if self
                .per_node_states
                .get(&pnk)
                .is_some_and(|pns| pns.watch_id == watch_id)
            {
                continue;
            }
            let Some(params) = self
                .outbound_watches
                .get(&pnk.record_key.opaque())
                .and_then(|ow| {
                    ow.desired()
                        .or_else(|| ow.state().map(|s| s.params().clone()))
                })
            else {
                continue;
            };
            self.watch_cancels_in_flight.insert((pnk.clone(), watch_id));
            out.push(WatchCancel {
                pnk,
                watch_id,
                opt_watcher: params.opt_watcher,
                safety_selection: params.safety_selection,
            });
        }
        out
    }

    /// A cancel taken by `take_due_watch_cancels` was answered or failed
    pub fn finish_watch_cancel(&mut self, pnk: &PerNodeKey, watch_id: u64) {
        self.watch_cancels_in_flight
            .remove(&(pnk.clone(), watch_id));
    }

    /// Keep a watch on the `consensus_count` nodes closest to its record and queue the rest for
    /// cancelling: every extra node sends every change again. Returns the nodes dropped.
    pub fn trim_watch_nodes(
        &mut self,
        record_key: &RecordKey,
        consensus_count: usize,
        cur_ts: Timestamp,
    ) -> Vec<PerNodeKey> {
        let Some(state) = self
            .outbound_watches
            .get_mut(&record_key.opaque())
            .and_then(|ow| ow.state_mut())
        else {
            return vec![];
        };
        if state.nodes().len() <= consensus_count {
            return vec![];
        }
        let coordinate = record_key.to_hash_coordinate();
        let mut nodes = state.nodes().clone();
        nodes.sort_by_cached_key(|pnk| coordinate.distance(&pnk.node_id.to_hash_coordinate()));
        let extras: BTreeSet<PerNodeKey> = nodes.split_off(consensus_count).into_iter().collect();
        state.edit(&self.per_node_states, |editor| {
            editor.retain_nodes(|pnk| !extras.contains(pnk));
        });
        for pnk in &extras {
            if let Some(pns) = self.per_node_states.remove(pnk) {
                self.queue_watch_cancel(pnk.clone(), pns.watch_id, cur_ts);
            }
        }
        extras.into_iter().collect()
    }

    pub fn prepare(&mut self, routing_table: &RoutingTable) {
        let registry = routing_table.registry();
        self.opt_registry = Some(registry.clone());

        for (pnk, pns) in &mut self.per_node_states {
            pns.watch_node_ref = match routing_table.lookup_node_id(pnk.node_id.clone()) {
                Ok(v) => v,
                Err(e) => {
                    veilid_log!(registry debug "Error looking up outbound watch node ref: {}", e);
                    None
                }
            };
        }
        self.per_node_states
            .retain(|_, v| v.watch_node_ref.is_some());

        let keys = self.per_node_states.keys().cloned().collect::<HashSet<_>>();

        for v in self.outbound_watches.values_mut() {
            if let Some(state) = v.state_mut() {
                state.edit(&self.per_node_states, |editor| {
                    editor.retain_nodes(|n| keys.contains(n));
                })
            }
        }
    }

    pub fn set_desired_watch(
        &mut self,
        record_key: RecordKey,
        desired_watch: Option<OutboundWatchParameters>,
    ) {
        let opaque_record_key = record_key.opaque();
        match self.outbound_watches.get_mut(&opaque_record_key) {
            Some(w) => {
                // Replace desired watch
                w.set_desired(desired_watch);

                // Remove if the watch is done (shortcut the dead state)
                if w.desired().is_none() && w.state().is_none() {
                    self.outbound_watches.remove(&opaque_record_key);
                }
            }
            None => {
                // Watch does not exist, add one if that's what is desired
                if let Some(desired) = desired_watch {
                    self.outbound_watches
                        .insert(opaque_record_key, OutboundWatch::new(record_key, desired));
                }
            }
        }
    }

    pub fn set_next_reconcile(
        &mut self,
        opaque_record_key: OpaqueRecordKey,
        next_ts: Timestamp,
        reconcile_retries: u32,
    ) {
        if let Some(outbound_watch) = self.outbound_watches.get_mut(&opaque_record_key) {
            if let Some(state) = outbound_watch.state_mut() {
                state.edit(&self.per_node_states, |editor| {
                    editor.set_next_reconcile_ts(next_ts);
                    editor.set_reconcile_retries(reconcile_retries);
                });
            }
        }
    }

    /// Iterate all per-node watches and remove ones with dead nodes from outbound watches
    /// This may trigger reconciliation to increase the number of active per-node watches
    /// for an outbound watch that is still alive
    pub fn update_per_node_states(&mut self, cur_ts: Timestamp) {
        // Node is unreachable
        let mut dead_pnks = HashSet::new();
        // Per-node expiration reached
        let mut expired_pnks = HashSet::new();
        // Count reached
        let mut finished_pnks = HashSet::new();

        for (pnk, pns) in &self.per_node_states {
            if pns.count == 0 {
                // If per-node watch is done, add to finished list
                finished_pnks.insert(pnk.clone());
            } else if !pns
                .watch_node_ref
                .as_ref()
                .unwrap_or_log()
                .state(cur_ts)
                .is_responsive()
            {
                // If node is not responsive add to dead list
                dead_pnks.insert(pnk.clone());
            } else if cur_ts >= pns.expiration {
                // If per-node watch has expired add to expired list
                expired_pnks.insert(pnk.clone());
            }
        }

        // Expired nodes go too: renewal stops at expiry and a kept one still counts for consensus
        for v in self.outbound_watches.values_mut() {
            let Some(current) = v.state_mut() else {
                continue;
            };
            current.edit(&self.per_node_states, |editor| {
                editor.retain_nodes(|x| {
                    !dead_pnks.contains(x)
                        && !finished_pnks.contains(x)
                        && !expired_pnks.contains(x)
                });
            });
        }

        // Drop finished and expired per-node watches
        self.per_node_states
            .retain(|k, _| !finished_pnks.contains(k) && !expired_pnks.contains(k));
    }

    /// Set a record up to be inspected for changed subkeys
    pub fn enqueue_change_inspect(
        &mut self,
        record_key: RecordKey,
        subkeys: ValueSubkeyRangeSet,
        trigger: ChangeInspectTrigger,
    ) {
        // Due now; the enqueue time orders it behind earlier requests
        self.enqueue_change_inspect_after(record_key, subkeys, Timestamp::now(), trigger);
    }

    /// Takes the change inspections due at `cur_ts`, oldest first, up to the free in-flight slots,
    /// and marks their records in flight
    pub fn take_due_change_inspections(
        &mut self,
        cur_ts: Timestamp,
    ) -> Vec<(RecordKey, ValueSubkeyRangeSet, ChangeInspectTrigger)> {
        let free_slots =
            MAX_CHANGE_INSPECTIONS_IN_FLIGHT.saturating_sub(self.change_inspects_in_flight.len());
        let mut due = self
            .needs_change_inspection
            .iter()
            .filter(|(k, (_, not_before, _))| {
                *not_before <= cur_ts && !self.change_inspects_in_flight.contains(&k.opaque())
            })
            .map(|(k, (_, not_before, _))| (*not_before, k.clone()))
            .collect::<Vec<_>>();
        due.sort_by_key(|(not_before, _)| *not_before);
        due.into_iter()
            .take(free_slots)
            .filter_map(|(_, k)| {
                let (subkeys, _, trigger) = self.needs_change_inspection.remove(&k)?;
                self.change_inspects_in_flight.insert(k.opaque());
                Some((k, subkeys, trigger))
            })
            .collect()
    }

    /// Set a record up to be inspected for changed subkeys, no earlier than a given time
    pub fn enqueue_change_inspect_after(
        &mut self,
        record_key: RecordKey,
        subkeys: ValueSubkeyRangeSet,
        not_before: Timestamp,
        trigger: ChangeInspectTrigger,
    ) {
        // No registry until `prepare`, nor after shutdown resets the manager
        if let Some(registry) = &self.opt_registry {
            veilid_log!(registry debug "change inspect: record_key={} subkeys={} not_before={} trigger={:?}", record_key, subkeys, not_before, trigger);
        }

        let opaque_record_key = record_key.opaque();

        // Get the outbound watch
        let Some(outbound_watch) = self.outbound_watches.get(&opaque_record_key) else {
            // No outbound watch means no change inspect
            return;
        };

        if outbound_watch.state().is_none() && outbound_watch.desired().is_none() {
            // Neither established nor wanted, so nothing to report changes to. A
            // wanted-but-unestablished watch IS inspected: the app awaits its changes
            return;
        };

        // Merged entries keep the latest not_before so retry backoff is not defeated
        self.needs_change_inspection
            .entry(record_key)
            .and_modify(|(sk, nb, tr)| {
                *sk = sk.union(&subkeys);
                *nb = (*nb).max(not_before);
                *tr = tr.merged_with(trigger);
            })
            .or_insert((subkeys, not_before, trigger));
    }
}
