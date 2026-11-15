#[cfg(feature = "stats")]
use super::connection_table::ConnectionTableStats;
use super::*;

// Statistics per address
#[derive(Clone, Debug, Default)]
pub struct PerAddressStats {
    pub last_seen_ts: Timestamp,
    pub transfer_stats_accounting: TransferStatsAccounting,
    pub transfer_stats: TransferStatsDownUp,
}

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct PerAddressStatsKey(IpAddr);

impl Default for PerAddressStatsKey {
    fn default() -> Self {
        Self(IpAddr::V4(Ipv4Addr::UNSPECIFIED))
    }
}

/// Posted each rolling-transfers interval with a snapshot of the node's network accounting
#[derive(Clone, Debug)]
pub struct NetworkManagerAccountingChangeEvent {
    pub accounting: Arc<RwLock<NetworkManagerAccounting>>,
}

// Low-level network transfer and relay accounting
#[derive(Debug, Clone)]
pub struct NetworkManagerAccounting {
    pub self_stats: PerAddressStats,
    pub per_address_stats: hashlink::LruCache<PerAddressStatsKey, PerAddressStats>,
    pub relay_worker_dequeue_latency_accounting: LatencyStatsAccounting,
    pub relay_worker_process_latency_accounting: LatencyStatsAccounting,
}

impl Default for NetworkManagerAccounting {
    fn default() -> Self {
        Self {
            self_stats: PerAddressStats::default(),
            per_address_stats: hashlink::LruCache::new(IPADDR_TABLE_SIZE),
            relay_worker_dequeue_latency_accounting: LatencyStatsAccounting::new(),
            relay_worker_process_latency_accounting: LatencyStatsAccounting::new(),
        }
    }
}

#[cfg(feature = "stats")]
#[derive(Debug, Serialize)]
pub(super) struct NetworkManagerStats {
    transfer: TransferStatsDownUp,
    relay_dequeue_latency: LatencyStats,
    relay_process_latency: LatencyStats,
    connections: ConnectionTableStats,
    public_internet: OnlineDomainStats,
    local_network: OnlineDomainStats,
}

#[cfg(feature = "stats")]
#[derive(Debug, Serialize)]
struct OnlineDomainStats {
    online: bool,
    degraded: bool,
    transitions: u64,
    time_offline: TimestampDuration,
    time_degraded: TimestampDuration,
}

impl NetworkManager {
    // Callbacks from low level network for statistics gathering
    pub fn stats_packet_sent(&self, addr: IpAddr, bytes: ByteCount) {
        let mut stats = self.accounting.write();
        stats.self_stats.transfer_stats_accounting.add_up(bytes);
        #[allow(clippy::unwrap_or_default)]
        stats
            .per_address_stats
            .entry(PerAddressStatsKey(addr))
            .or_insert(PerAddressStats::default())
            .transfer_stats_accounting
            .add_up(bytes);
    }

    pub fn stats_packet_rcvd(&self, addr: IpAddr, bytes: ByteCount) {
        let mut stats = self.accounting.write();
        stats.self_stats.transfer_stats_accounting.add_down(bytes);
        #[allow(clippy::unwrap_or_default)]
        stats
            .per_address_stats
            .entry(PerAddressStatsKey(addr))
            .or_insert(PerAddressStats::default())
            .transfer_stats_accounting
            .add_down(bytes);
    }

    pub fn stats_relay_processed(
        &self,
        dequeue_latency: TimestampDuration,
        process_latency: TimestampDuration,
    ) {
        let mut stats = self.accounting.write();
        stats
            .relay_worker_dequeue_latency_accounting
            .record_latency(dequeue_latency);
        stats
            .relay_worker_process_latency_accounting
            .record_latency(process_latency);
    }

    pub fn get_stats(&self) -> NetworkManagerAccounting {
        self.accounting.read().clone()
    }

    #[cfg(feature = "stats")]
    pub(super) fn stats_impl(&self) -> Option<Box<dyn VeilidComponentStats>> {
        let s = self.get_stats();
        let connections = self
            .opt_connection_manager()
            .map(|cm| cm.connection_table_stats())
            .unwrap_or_default();
        let now = Timestamp::now();
        let domain_stats = |rd| {
            let (online, degraded, transitions, time_offline, time_degraded) =
                self.online_detector().online_stats(rd, now);
            OnlineDomainStats {
                online,
                degraded,
                transitions,
                time_offline,
                time_degraded,
            }
        };
        Some(Box::new(NetworkManagerStats {
            transfer: s.self_stats.transfer_stats,
            relay_dequeue_latency: s
                .relay_worker_dequeue_latency_accounting
                .latency_stats()
                .unwrap_or_default(),
            relay_process_latency: s
                .relay_worker_process_latency_accounting
                .latency_stats()
                .unwrap_or_default(),
            connections,
            public_internet: domain_stats(RoutingDomain::PublicInternet),
            local_network: domain_stats(RoutingDomain::LocalNetwork),
        }))
    }

    pub fn get_veilid_state(&self) -> Box<VeilidStateNetwork> {
        if !self.network_is_started() {
            return Box::new(VeilidStateNetwork {
                started: false,
                bps_down: 0.into(),
                bps_up: 0.into(),
                peers: Vec::new(),
                node_ids: Vec::new(),
            });
        }
        let routing_table = self.routing_table();

        let (bps_down, bps_up) = {
            let stats = self.accounting.read();
            (
                stats.self_stats.transfer_stats.down.average,
                stats.self_stats.transfer_stats.up.average,
            )
        };

        Box::new(VeilidStateNetwork {
            started: true,
            bps_down,
            bps_up,
            peers: {
                let mut out = Vec::new();
                for (k, v) in routing_table.get_recent_peers() {
                    if let Ok(Some(nr)) = routing_table.lookup_node_id(k) {
                        let peer_stats = nr.peer_stats();
                        let peer = PeerTableData {
                            node_ids: nr.node_ids().iter().cloned().collect(),
                            peer_address: v.last_connection.remote().to_string(),
                            peer_stats,
                        };
                        out.push(peer);
                    }
                }
                out
            },
            node_ids: routing_table.node_ids().to_vec(),
        })
    }

    pub fn send_network_update(&self) {
        let update_cb = self.update_callback();

        let state = self.get_veilid_state();
        update_cb(VeilidUpdate::Network(state));
    }
}
