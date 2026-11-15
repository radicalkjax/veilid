//! Outbound send health per (RoutingDomain, LowLevelTransportType).
//!
//! Windows record only our own sends; they say nothing about inbound reachability
//! and never change what we publish. Design invariants:
//! - Suppression requires a full window of failures across enough distinct peers,
//!   so a few peers with stale dial info never indict the path
//! - Suppression is sticky: entered on the evidence bar, released only by a send
//!   that works, so window decay cannot un-suppress on thin evidence
//! - While suppressed, one canary send probes for recovery when our side changes
//!   (back online, new interface addresses) and otherwise once per canary period;
//!   it is spent at decision time so concurrent selections cannot all escape with it
//! - Enforcement (skipping sends, rationing the canary) runs only while the
//!   domain's outbound stage is ReadyToOperate. Until then every send is a
//!   canary: with too few live peers the evidence is stale by construction (a
//!   whole-fleet restart looks exactly like a broken local transport), and
//!   bootstrap/dial-info-confirmation sends must flow to make progress. Verdicts
//!   still accrue and still void peer blame; enforcement re-arms on readiness,
//!   and any degradation that costs readiness lifts it again
//! - Debug `transport disable` fails low-level sends with NoConnection, driving
//!   this machinery with real evidence instead of overriding verdicts

use super::*;

impl_veilid_log_facility!("net");

/// Outcomes retained regardless of age, so a quiet period keeps a verdict
pub(crate) const TRANSPORT_STATS_MIN_SAMPLES: usize = 8;
/// Distinct failed peers required before the path is blamed rather than the peers
pub(crate) const TRANSPORT_STATS_MIN_DISTINCT_PEERS: usize = 4;
/// Hard bound on retained outcomes per transport
const MAX_SAMPLES: usize = 64;
/// How often the per-transport health summary is logged
pub const TRANSPORT_STATS_LOG_INTERVAL_SECS: u32 = 10;

/// Peer-diversity key; unbound sends (bootstrap, discovery) have an address but no node
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub(crate) enum TransportPeerKey {
    /// A peer we know by node id
    Node(NodeId),
    /// A target known only by address
    Address(SocketAddress),
}

impl From<NodeId> for TransportPeerKey {
    fn from(node_id: NodeId) -> Self {
        TransportPeerKey::Node(node_id)
    }
}

impl From<SocketAddress> for TransportPeerKey {
    fn from(addr: SocketAddress) -> Self {
        TransportPeerKey::Address(addr)
    }
}

/// What one send attempt says about a transport, from our side only
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub(crate) enum TransportSendOutcome {
    /// The send completed
    Sent,
    /// The send returned an error
    FailedToSend,
    /// The send did not complete in time
    Timeout,
}

impl TransportSendOutcome {
    fn is_sent(&self) -> bool {
        matches!(self, TransportSendOutcome::Sent)
    }

    /// What a send result says about the transport; None when it says nothing
    pub(crate) fn from_network_result<T>(nr: &NetworkResult<T>) -> Option<Self> {
        match nr {
            NetworkResult::Value(_) => Some(TransportSendOutcome::Sent),
            NetworkResult::Timeout => Some(TransportSendOutcome::Timeout),
            // Refused proves the path works; it is no evidence against the transport
            NetworkResult::NoConnection(e) if is_connection_refused(e) => None,
            _ => Some(TransportSendOutcome::FailedToSend),
        }
    }

    pub(crate) fn from_send_data_result(sdr: &SendDataResult) -> Option<Self> {
        Self::from_network_result(sdr.network_result())
    }
}

/// Outbound send verdict for one transport, aggregated across all peers
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub(crate) enum TransportVerdict {
    /// Not enough evidence yet; usable
    Unknown,
    /// Sending works
    Healthy,
    /// Nothing has gone out over this transport for a whole window
    Suppressed {
        /// The canary interval has elapsed since the last attempt
        due_for_canary: bool,
    },
}

impl TransportVerdict {
    /// Whether selection should push this transport to the back of the line
    pub fn is_deprioritized(&self) -> bool {
        matches!(
            self,
            TransportVerdict::Suppressed {
                due_for_canary: false
            }
        )
    }

    /// Whether we cannot send on this transport. Blame writers void per-transport
    /// evidence gathered on it: the aggregate verdict makes the fault ours, not the peer's.
    pub fn is_send_suppressed(&self) -> bool {
        matches!(self, TransportVerdict::Suppressed { .. })
    }
}

/// Rolling send outcomes for one transport, plus its suppression latch
#[derive(Debug, Default)]
pub(crate) struct TransportWindow {
    /// Recent send outcomes with their time and peer, oldest first
    outcomes: VecDeque<(Timestamp, TransportSendOutcome, TransportPeerKey)>,
    /// Last send attempt, a spent canary included; None lets the next canary through
    last_attempt_ts: Option<Timestamp>,
    /// Sends skipped while suppressed since the last health log
    skipped: usize,
    /// Sticky suppression latch (see module doc)
    suppressed: bool,
}

impl TransportWindow {
    /// Outcomes currently retained
    pub(crate) fn sample_count(&self) -> usize {
        self.outcomes.len()
    }

    /// Distinct peers represented in the window
    pub(crate) fn distinct_peers(&self) -> usize {
        self.outcomes
            .iter()
            .map(|(_, _, id)| id)
            .collect::<HashSet<_>>()
            .len()
    }

    /// Add one send outcome, then update the suppression latch
    pub(crate) fn record(
        &mut self,
        cur_ts: Timestamp,
        outcome: TransportSendOutcome,
        peer: TransportPeerKey,
        window: TimestampDuration,
    ) {
        self.last_attempt_ts = Some(cur_ts);
        self.outcomes.push_back((cur_ts, outcome, peer));
        while self.outcomes.len() > MAX_SAMPLES {
            self.outcomes.pop_front();
        }
        self.evict_aged(cur_ts, window);

        if outcome.is_sent() {
            self.suppressed = false;
        } else if !self.suppressed {
            self.suppressed = self.meets_entry_bar();
        }
    }

    /// Full window of failures across enough distinct peers
    fn meets_entry_bar(&self) -> bool {
        self.outcomes.len() >= TRANSPORT_STATS_MIN_SAMPLES
            && !self.outcomes.iter().any(|(_, o, _)| o.is_sent())
            && self.distinct_peers() >= TRANSPORT_STATS_MIN_DISTINCT_PEERS
    }

    /// Drop outcomes older than `window`, but never below TRANSPORT_STATS_MIN_SAMPLES
    pub(crate) fn evict_aged(&mut self, cur_ts: Timestamp, window: TimestampDuration) {
        while self.outcomes.len() > TRANSPORT_STATS_MIN_SAMPLES {
            let Some((ts, _, _)) = self.outcomes.front() else {
                break;
            };
            if cur_ts.duration_since(*ts) <= window {
                break;
            }
            self.outcomes.pop_front();
        }
    }

    fn counts(&self) -> (usize, usize, usize) {
        let mut sent = 0;
        let mut failed = 0;
        let mut timeout = 0;
        for (_, o, _) in &self.outcomes {
            match o {
                TransportSendOutcome::Sent => sent += 1,
                TransportSendOutcome::FailedToSend => failed += 1,
                TransportSendOutcome::Timeout => timeout += 1,
            }
        }
        (sent, failed, timeout)
    }

    /// Let the next send through as a canary if suppressed; returns whether one was held back
    pub(crate) fn release_canary(&mut self) -> bool {
        self.suppressed && self.last_attempt_ts.take().is_some()
    }

    /// Verdict from the outcomes currently retained; a suppressed transport is due a canary once
    /// nothing was attempted for `canary_interval`
    pub(crate) fn verdict(
        &self,
        cur_ts: Timestamp,
        canary_interval: TimestampDuration,
    ) -> TransportVerdict {
        if !self.suppressed {
            if self.outcomes.len() < TRANSPORT_STATS_MIN_SAMPLES {
                return TransportVerdict::Unknown;
            }
            return if self.outcomes.iter().any(|(_, o, _)| o.is_sent()) {
                TransportVerdict::Healthy
            } else {
                // Failures below the peer-diversity bar indict the peers, not the path
                TransportVerdict::Unknown
            };
        }
        let due_for_canary = self
            .last_attempt_ts
            .map(|ts| cur_ts.duration_since(ts) > canary_interval)
            .unwrap_or(true);
        TransportVerdict::Suppressed { due_for_canary }
    }
}

/// Send-health windows keyed by network path (TCP/WS/WSS pool as TCP); per-protocol
/// trouble is the per-node tables' job
#[derive(Debug)]
pub(crate) struct TransportStats {
    /// Registry accessor
    registry: VeilidComponentRegistry,
    /// One send-outcome window per routing domain and transport
    windows: Mutex<BTreeMap<(RoutingDomain, LowLevelTransportType), TransportWindow>>,
    /// How long a suppressed transport goes unattempted before a canary while nothing changes
    canary_interval: TimestampDuration,
    /// Age at which a send outcome leaves a window
    window: TimestampDuration,
    /// Domains with enforcement armed (outbound ReadyToOperate); elsewhere every send is a canary
    enforcing: Mutex<BTreeSet<RoutingDomain>>,
    /// Debug availability overrides by change generation, so a timed revert skips later toggles
    disabled: Mutex<BTreeMap<LowLevelTransportType, u64>>,
    /// Bumped by every availability change
    #[cfg(feature = "debug-api")]
    change_generation: Mutex<u64>,
}

impl_veilid_component_accessors!(TransportStats);

impl TransportStats {
    /// Create an empty send-health tracker
    pub fn new(
        registry: VeilidComponentRegistry,
        canary_interval: TimestampDuration,
        window: TimestampDuration,
    ) -> Self {
        Self {
            registry,
            windows: Mutex::new(BTreeMap::new()),
            canary_interval,
            window,
            enforcing: Mutex::new(BTreeSet::new()),
            disabled: Mutex::new(BTreeMap::new()),
            #[cfg(feature = "debug-api")]
            change_generation: Mutex::new(0),
        }
    }

    /// Whether this transport is administratively disabled (debug `transport disable`)
    pub fn is_transport_disabled(
        &self,
        low_level_protocol_type: LowLevelProtocolType,
        address_type: AddressType,
    ) -> bool {
        self.disabled.lock().contains_key(&LowLevelTransportType {
            low_level_protocol_type,
            address_type,
        })
    }

    /// Set availability; returns the change generation for timed reverts
    #[cfg(feature = "debug-api")]
    pub fn set_transports_disabled(
        &self,
        transports: &[LowLevelTransportType],
        disabled: bool,
    ) -> u64 {
        let mut generation = self.change_generation.lock();
        *generation += 1;
        let gen = *generation;
        let mut map = self.disabled.lock();
        for tt in transports {
            if disabled {
                map.insert(*tt, gen);
            } else {
                map.remove(tt);
            }
        }
        veilid_log!(self debug "transports {}: {}",
            if disabled { "disabled" } else { "enabled" },
            transports.iter().map(|t| t.to_string()).collect::<Vec<_>>().join(","));
        gen
    }

    /// Revert a timed toggle for transports unchanged since `expect_generation`
    #[cfg(feature = "debug-api")]
    pub fn set_transports_disabled_if_generation(
        &self,
        transports: &[LowLevelTransportType],
        disabled: bool,
        expect_generation: u64,
    ) {
        let untouched: Vec<LowLevelTransportType> = {
            let map = self.disabled.lock();
            transports
                .iter()
                .filter(|tt| match map.get(tt) {
                    Some(gen) => *gen == expect_generation,
                    // Enabled entries carry no generation; only revert them toward disabled
                    None => disabled,
                })
                .copied()
                .collect()
        };
        if !untouched.is_empty() {
            self.set_transports_disabled(&untouched, disabled);
        }
    }

    /// Currently disabled transports
    #[cfg(feature = "debug-api")]
    pub fn disabled_transports(&self) -> Vec<LowLevelTransportType> {
        self.disabled.lock().keys().copied().collect()
    }

    /// Purge all windows (debug `transport clear`)
    #[cfg(feature = "debug-api")]
    pub fn clear_all_windows(&self) {
        self.windows.lock().clear();
        veilid_log!(self debug "transport stats cleared (debug)");
    }

    /// Record one send attempt against a peer's routing domain and transport
    pub fn record_send(
        &self,
        routing_domain: RoutingDomain,
        transport: TransportType,
        peer: impl Into<TransportPeerKey>,
        outcome: TransportSendOutcome,
    ) {
        let transport = LowLevelTransportType::from(transport);
        let cur_ts = Timestamp::now();
        self.windows
            .lock()
            .entry((routing_domain, transport))
            .or_default()
            .record(cur_ts, outcome, peer.into(), self.window);
    }

    /// Record a nodeless send (bootstrap, discovery); the address still names the
    /// routing domain and transport
    pub fn record_unbound_send(&self, dial_info: &DialInfo, outcome: TransportSendOutcome) {
        let socket_address = dial_info.socket_address();
        let Some(routing_domain) = self
            .routing_table()
            .routing_domain_for_address(socket_address.address())
        else {
            return;
        };
        self.record_send(
            routing_domain,
            TransportType::from(dial_info),
            socket_address,
            outcome,
        );
    }

    /// Decision point for nodeless sends, whose targets come from configuration
    /// rather than contact method selection; spends the canary when due
    pub fn unbound_send_deprioritized(&self, dial_info: &DialInfo) -> bool {
        let Some(routing_domain) = self
            .routing_table()
            .routing_domain_for_address(dial_info.socket_address().address())
        else {
            return false;
        };
        self.deprioritized_consuming_canary(routing_domain, TransportType::from(dial_info))
    }

    /// Whether any outbound protocol carries this ordering, suppressed or not; WS-only WASM
    /// has no Unordered carrier
    pub fn ordering_is_available(
        &self,
        routing_domain: RoutingDomain,
        ordering: SequenceOrdering,
    ) -> bool {
        self.routing_table()
            .get_current_peer_info(routing_domain)
            .node_info()
            .outbound_protocols()
            .iter()
            .any(|protocol| protocol.sequence_ordering() == ordering)
    }

    /// Whether any non-suppressed transport carries this ordering. Only suppression
    /// counts as degraded (Unknown is usable); WS-only WASM has no Unordered carrier
    pub fn ordering_is_viable(
        &self,
        routing_domain: RoutingDomain,
        ordering: SequenceOrdering,
    ) -> bool {
        let routing_table = self.routing_table();
        let peer_info = routing_table.get_current_peer_info(routing_domain);
        let node_info = peer_info.node_info();
        let outbound = node_info.outbound_protocols();
        let address_types = node_info.address_types();

        for protocol in outbound.iter() {
            // XXX: WebTransport will offer both orderings over UDP; derive ordering
            // support from dialinfo rather than protocol when it lands
            if protocol.sequence_ordering() != ordering {
                continue;
            }
            for address_type in address_types.iter() {
                let transport = TransportType::new(protocol, address_type);
                if !self.verdict(routing_domain, transport).is_deprioritized() {
                    return true;
                }
            }
        }
        false
    }

    /// Arm or stand down suppression enforcement for a routing domain; armed only
    /// while the domain's outbound stage is ReadyToOperate
    pub fn set_enforcing(&self, routing_domain: RoutingDomain, enforcing: bool) {
        let changed = {
            let mut set = self.enforcing.lock();
            if enforcing {
                set.insert(routing_domain)
            } else {
                set.remove(&routing_domain)
            }
        };
        if changed {
            veilid_log!(self debug "transport suppression {} for {:?}",
                if enforcing { "armed: outbound ready" } else { "standing down: outbound not ready" },
                routing_domain);
        }
    }

    /// Whether suppression enforcement is armed for this domain
    pub fn is_enforcing(&self, routing_domain: RoutingDomain) -> bool {
        self.enforcing.lock().contains(&routing_domain)
    }

    /// Decision-point verdict check that spends the canary at selection time, so
    /// sends picked during the in-flight window cannot all escape with it.
    /// Never use for read-only verdict queries.
    pub fn deprioritized_consuming_canary(
        &self,
        routing_domain: RoutingDomain,
        transport: TransportType,
    ) -> bool {
        // Until enforcement arms, sends flow freely and none may spend the canary
        if !self.is_enforcing(routing_domain) {
            return false;
        }
        let transport = LowLevelTransportType::from(transport);
        let cur_ts = Timestamp::now();
        let mut windows = self.windows.lock();
        let Some(window) = windows.get_mut(&(routing_domain, transport)) else {
            return false;
        };
        window.evict_aged(cur_ts, self.window);
        match window.verdict(cur_ts, self.canary_interval) {
            TransportVerdict::Suppressed {
                due_for_canary: false,
            } => true,
            TransportVerdict::Suppressed {
                due_for_canary: true,
            } => {
                window.last_attempt_ts = Some(cur_ts);
                false
            }
            _ => false,
        }
    }

    /// Count a send skipped because the transport is suppressed
    pub fn record_skip(&self, routing_domain: RoutingDomain, transport: TransportType) {
        let transport = LowLevelTransportType::from(transport);
        if let Some(window) = self.windows.lock().get_mut(&(routing_domain, transport)) {
            window.skipped += 1;
        }
    }

    /// Verdict for a transport, leaving the canary unspent
    pub fn verdict(
        &self,
        routing_domain: RoutingDomain,
        transport: TransportType,
    ) -> TransportVerdict {
        let transport = LowLevelTransportType::from(transport);
        let cur_ts = Timestamp::now();
        let mut windows = self.windows.lock();
        let Some(window) = windows.get_mut(&(routing_domain, transport)) else {
            return TransportVerdict::Unknown;
        };
        window.evict_aged(cur_ts, self.window);
        window.verdict(cur_ts, self.canary_interval)
    }

    /// Let one canary through on every suppressed transport of these domains, because our side
    /// of the path changed and old failures may no longer apply
    pub fn release_canaries(&self, routing_domains: RoutingDomainSet, reason: &str) {
        let mut released = 0usize;
        for ((rd, _), window) in self.windows.lock().iter_mut() {
            if routing_domains.contains(*rd) && window.release_canary() {
                released += 1;
            }
        }
        if released > 0 {
            veilid_log!(self debug "released {} transport canaries: {}", released, reason);
        }
    }

    /// Drop all evidence for a routing domain. Our network position changed, so
    /// what we learned from the old one no longer applies.
    pub fn reset_routing_domain(&self, routing_domain: RoutingDomain, reason: &str) {
        let mut windows = self.windows.lock();
        let before = windows.len();
        windows.retain(|(rd, _), _| *rd != routing_domain);
        if before != windows.len() {
            veilid_log!(self debug "transport stats reset for {:?}: {}", routing_domain, reason);
        }
        drop(windows);
        // Stand down until the domain controller re-arms on readiness
        self.set_enforcing(routing_domain, false);
    }

    /// Per-domain health lines. `take_skipped` resets the skip counters, which only
    /// the periodic log may do; the debug dump must not steal them from it.
    fn health_report(&self, take_skipped: bool) -> Vec<(RoutingDomain, String)> {
        let cur_ts = Timestamp::now();
        let mut per_domain = BTreeMap::<RoutingDomain, Vec<String>>::new();
        {
            let mut windows = self.windows.lock();
            for ((rd, tt), window) in windows.iter_mut() {
                window.evict_aged(cur_ts, self.window);
                let (sent, failed, timeout) = window.counts();
                let peers = window.distinct_peers();
                let verdict = match window.verdict(cur_ts, self.canary_interval) {
                    TransportVerdict::Unknown => "unknown".to_owned(),
                    TransportVerdict::Healthy => "healthy".to_owned(),
                    TransportVerdict::Suppressed { due_for_canary } => {
                        if due_for_canary {
                            "suppressed(canary due)".to_owned()
                        } else {
                            "suppressed".to_owned()
                        }
                    }
                };
                let since_attempt = window
                    .last_attempt_ts
                    .map(|ts| format!("{:#}", cur_ts.duration_since(ts)))
                    .unwrap_or_else(|| "never".to_owned());
                let skipped = if take_skipped {
                    std::mem::take(&mut window.skipped)
                } else {
                    window.skipped
                };
                per_domain.entry(*rd).or_default().push(format!(
                    "{}={} n:{} peers:{} sent:{} failed:{} timeout:{} skipped:{} last:{}",
                    tt,
                    verdict,
                    window.sample_count(),
                    peers,
                    sent,
                    failed,
                    timeout,
                    skipped,
                    since_attempt
                ));
            }
        }
        per_domain
            .into_iter()
            .map(|(rd, mut entries)| {
                entries.sort();
                let mut line = entries.join(" | ");
                if !self.is_enforcing(rd) {
                    line += " [advisory: enforcement not armed]";
                }
                (rd, line)
            })
            .collect()
    }

    /// Log one line per routing domain with a transport
    pub fn log_health(&self) {
        for (rd, line) in self.health_report(true) {
            veilid_log!(self debug target: "rtab::stats", "transport health [{:?}]: {}", rd, line);
        }
    }

    /// Full dump for the debug `transport stats` command
    #[cfg(feature = "debug-api")]
    pub fn debug_stats(&self) -> String {
        let mut out = String::new();
        let report = self.health_report(false);
        if report.is_empty() {
            out += "no transport windows yet\n";
        }
        for (rd, line) in report {
            out += &format!("[{:?}]\n", rd);
            for entry in line.split(" | ") {
                out += &format!("  {}\n", entry);
            }
        }
        let disabled = self.disabled_transports();
        if disabled.is_empty() {
            out += "disabled: (none)\n";
        } else {
            out += &format!(
                "disabled: {}\n",
                disabled
                    .iter()
                    .map(|t| t.to_string())
                    .collect::<Vec<_>>()
                    .join(", ")
            );
        }
        out
    }
}
