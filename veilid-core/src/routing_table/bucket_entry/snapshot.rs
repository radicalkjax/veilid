use super::*;

/// An entry as of `cur_ts`: its persisted parts and the live state they leave out
#[derive(Debug)]
pub(crate) struct BucketEntrySnapshotInner {
    /// When the snapshot was taken
    pub cur_ts: Timestamp,
    /// The entry
    pub node_ref: NodeRef,
    /// Stats for connection-oriented protocols
    pub connection_stats: ConnectionStats,
    /// The entry's persisted parts
    pub persisted: BucketEntryPersisted,
}

impl core::ops::Deref for BucketEntrySnapshotInner {
    type Target = BucketEntryPersisted;
    fn deref(&self) -> &Self::Target {
        &self.persisted
    }
}

/// A point-in-time snapshot of mutable BucketEntry fields used for sorting and filtering.
/// Created once before sorting to avoid total-order violations from concurrent
/// updates between comparisons (Rust 1.81+ driftsort validates total ordering).
///
/// Contains a `NodeRef` for creating `FilteredNodeRef` in transforms, and frozen
/// copies of all mutable fields needed by sort/filter closures. `Option<BucketEntrySnapshot>`
/// where `None` represents the self node.
#[derive(Clone, Debug)]
pub(crate) struct BucketEntrySnapshot {
    inner: Arc<BucketEntrySnapshotInner>,
}

impl core::ops::Deref for BucketEntrySnapshot {
    type Target = BucketEntrySnapshotInner;
    fn deref(&self) -> &Self::Target {
        &self.inner
    }
}

impl BucketEntrySnapshot {
    pub(super) fn new(inner: BucketEntrySnapshotInner) -> Self {
        Self {
            inner: Arc::new(inner),
        }
    }

    pub fn crypto_kinds(&self) -> Vec<CryptoKind> {
        self.node_ids.iter().map(|x| x.kind()).collect()
    }

    pub fn routing_domain_set(&self) -> RoutingDomainSet {
        RoutingDomainSet::all()
            .iter()
            .filter(|rd| self.routing_domain(*rd).peer_info.is_some())
            .collect()
    }

    pub fn is_reliable(&self) -> bool {
        self.state == BucketEntryState::Reliable
    }

    // pub fn is_reliable_for(&self, transport: LowLevelTransportType) -> bool {
    //     self.per_transport_stats
    //         .get(&transport)
    //         .map(|t| t.state == LowLevelState::Reliable)
    //         .unwrap_or(false)
    // }

    /// Set of sequence orderings the node supports, filtered by our outbound dial-info filter.
    pub fn supported_sequence_orderings(
        &self,
        routing_domain: RoutingDomain,
        outbound_dif: &DialInfoFilter,
    ) -> SequenceOrderingSet {
        let Some(pi) = self.get_peer_info(routing_domain) else {
            return SequenceOrderingSet::new();
        };
        pi.node_info().supported_sequence_orderings(outbound_dif)
    }

    // /// Set of low-level transports the node can be reached through (directly or
    // /// via any of its relays), filtered by our outbound dial-info filter.
    // pub fn supported_low_level_transports(
    //     &self,
    //     routing_domain: RoutingDomain,
    //     outbound_dif: &DialInfoFilter,
    // ) -> BTreeSet<LowLevelTransportType> {
    //     let Some(pi) = self.get_peer_info(routing_domain) else {
    //         return BTreeSet::new();
    //     };
    //     pi.node_info().low_level_transport_set(outbound_dif)
    // }

    // /// Best high-level TransportType for a given low-level transport on this node,
    // /// considering own and relay dial infos. None if outbound_dif filters everything.
    // pub fn preferred_transport_for(
    //     &self,
    //     routing_domain: RoutingDomain,
    //     low_level: LowLevelTransportType,
    //     outbound_dif: &DialInfoFilter,
    // ) -> Option<TransportType> {
    //     let pi = self.get_peer_info(routing_domain)?;
    //     pi.node_info()
    //         .preferred_transport_for(low_level, outbound_dif)
    // }

    pub fn has_node_info(&self, routing_domain_set: RoutingDomainSet) -> bool {
        routing_domain_set
            .iter()
            .any(|routing_domain| self.routing_domain(routing_domain).peer_info.is_some())
    }

    pub fn best_node_id(&self) -> Option<NodeId> {
        self.node_ids.first().cloned()
    }

    pub fn get_peer_info(&self, routing_domain: RoutingDomain) -> Option<Arc<PeerInfo>> {
        self.routing_domain(routing_domain).peer_info.clone()
    }

    pub fn node_status(&self, routing_domain: RoutingDomain) -> Option<NodeStatus> {
        let entry = self.routing_domain(routing_domain);
        entry.peer_info.as_ref()?;
        entry.node_status.clone()
    }

    pub fn has_all_capabilities(
        &self,
        routing_domain: RoutingDomain,
        capabilities: &[VeilidCapability],
    ) -> bool {
        let Some(pi) = self.get_peer_info(routing_domain) else {
            return false;
        };
        pi.node_info().has_all_capabilities(capabilities)
    }

    pub fn cmp_fastest(
        a: &Self,
        b: &Self,
        metric: impl Fn(&LatencyStats) -> TimestampDuration,
    ) -> std::cmp::Ordering {
        // Lower latency to the front
        if let Some(a_latency) = &a.peer_stats.latency {
            if let Some(b_latency) = &b.peer_stats.latency {
                metric(a_latency).cmp(&metric(b_latency))
            } else {
                std::cmp::Ordering::Less
            }
        } else if b.peer_stats.latency.is_some() {
            std::cmp::Ordering::Greater
        } else {
            std::cmp::Ordering::Equal
        }
    }

    // Less is more reliable, then faster
    pub fn cmp_fastest_reliable(
        a: &Self,
        b: &Self,
        metric: impl Fn(&LatencyStats) -> TimestampDuration,
    ) -> std::cmp::Ordering {
        // Reverse compare so most reliable is at front
        let ret = b.state.cmp(&a.state);
        if ret != std::cmp::Ordering::Equal {
            return ret;
        }

        // Lower latency to the front
        Self::cmp_fastest(a, b, metric)
    }

    // Less is more reliable, then older
    pub fn cmp_oldest_reliable(a: &Self, b: &Self) -> std::cmp::Ordering {
        // Reverse compare so most reliable is at front
        let ret = b.state.cmp(&a.state);
        if ret != std::cmp::Ordering::Equal {
            return ret;
        }

        // Lower timestamp to the front, recent or no timestamp is at the end
        // First check steady-ping reliability timestamp
        if let Some(a_ts) = &a.rpc_stats.first_steady_answer_ts {
            if let Some(b_ts) = &b.rpc_stats.first_steady_answer_ts {
                a_ts.cmp(b_ts)
            } else {
                std::cmp::Ordering::Less
            }
        } else if b.rpc_stats.first_steady_answer_ts.is_some() {
            std::cmp::Ordering::Greater
        } else {
            // Then check 'since added to routing table' timestamp
            a.time_added.cmp(&b.time_added)
        }
    }

    pub fn has_seen_our_node_info_ts(
        &self,
        routing_domain: RoutingDomain,
        our_node_info_ts: Timestamp,
    ) -> bool {
        let entry = self.routing_domain(routing_domain);
        entry.peer_info.is_some() && our_node_info_ts == entry.last_seen_our_node_info_ts
    }

    /// Sequence orderings due a proof-of-life ping. A supported ordering with no stats yet (no
    /// dial info for it, or never used) gets a cold-start ping.
    ///
    /// The cadence is driven by the node's overall state, not this ordering's own state: when the
    /// node is unreliable, every ordering is pinged at the fast unreliable interval so a stale
    /// 'reliable' ordering gets re-probed and can degrade, instead of riding the slow reliable
    /// backoff.
    ///
    /// EXCEPT an ordering with consecutive lost questions at the Failing threshold, which
    /// probes on a falloff from its steady-loss start: it is either down (the [D3]/[D4]
    /// spans need no more evidence than a few probes provide) or chronically flaky
    /// (full-rate probing of it is what floods the ping queue and the loss logs).
    pub fn needs_proof_of_life_ping(
        &self,
        routing_domain: RoutingDomain,
        outbound_dif: &DialInfoFilter,
        timings: &RoutingTableTimings,
    ) -> SequenceOrderingSet {
        let unreliable_ping_interval = timings.unreliable_ping_interval;
        let mut out = SequenceOrderingSet::empty();
        for so in self.supported_sequence_orderings(routing_domain, outbound_dif) {
            let Some(stats) = self.per_sequence_ordering_stats.get(&so) else {
                // Supported sequence ordering but no stats yet, cold start ping required
                out |= so;
                continue;
            };

            if stats.recent_lost_questions >= TRANSPORT_FAILING_LOST_QUESTIONS
                && !matches!(
                    self.state,
                    BucketEntryState::Punished | BucketEntryState::Dead
                )
            {
                if let (Some(last_question_ts), Some(first_steady_lost_ts)) =
                    (stats.last_question_ts, stats.first_steady_lost_question_ts)
                {
                    if retry_falloff_log(
                        last_question_ts
                            .duration_since(first_steady_lost_ts)
                            .as_u64(),
                        self.cur_ts.duration_since(first_steady_lost_ts).as_u64(),
                        unreliable_ping_interval.as_u64(),
                        RELIABLE_PING_INTERVAL_MAX.as_u64(),
                        RELIABLE_PING_INTERVAL_MULTIPLIER,
                    ) {
                        out |= so;
                    }
                    continue;
                }
            }

            let needs_ping = match self.state {
                BucketEntryState::Punished | BucketEntryState::Dead => false,
                BucketEntryState::Missing
                | BucketEntryState::Initial
                | BucketEntryState::Unreliable => match stats.last_question_ts {
                    None => true,
                    Some(last_question_ts) => {
                        self.cur_ts.duration_since(last_question_ts) >= unreliable_ping_interval
                    }
                },
                BucketEntryState::Reliable => {
                    match (stats.last_question_ts, stats.first_steady_answer_ts) {
                        // Never asked: ping it
                        (None, _) => true,
                        // Asked but not yet established with this sequence ordering: probe at the unreliable interval
                        (Some(last_question_ts), None) => {
                            self.cur_ts.duration_since(last_question_ts) >= unreliable_ping_interval
                        }
                        // Established reliable with this sequence ordering: use the reliable backoff
                        (Some(last_question_ts), Some(first_steady_answer_ts)) => {
                            let start_of_reliable_time = first_steady_answer_ts.later(
                                timings
                                    .liveness
                                    .unreliable_answer
                                    .saturating_sub(unreliable_ping_interval),
                            );
                            let reliable_cur = self.cur_ts.duration_since(start_of_reliable_time);
                            let reliable_last =
                                last_question_ts.duration_since(start_of_reliable_time);
                            retry_falloff_log(
                                reliable_last.as_u64(),
                                reliable_cur.as_u64(),
                                RELIABLE_PING_INTERVAL_START.as_u64(),
                                RELIABLE_PING_INTERVAL_MAX.as_u64(),
                                RELIABLE_PING_INTERVAL_MULTIPLIER,
                            )
                        }
                    }
                }
            };
            if needs_ping {
                out |= so;
            }
        }

        out
    }
}
