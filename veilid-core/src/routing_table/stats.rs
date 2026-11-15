use super::*;

use tasks::ping_stats_accounting::PingStats;

#[derive(Debug, Serialize)]
pub(super) struct RoutingTableStats {
    total_entry_count: u32,
    live_entry_count: u32,
    reliable_entry_count: u32,
    public_internet_ready: bool,
    local_network_ready: bool,
    entries_by_state: Vec<StateCount>,
    pings: PingStats,
}

#[derive(Debug, Serialize)]
struct StateCount {
    state: BucketEntryState,
    count: u32,
}

impl RoutingTable {
    pub(super) fn stats_impl(&self) -> Option<Box<dyn VeilidComponentStats>> {
        let health = self.routing_table_health.lock().clone();
        Some(Box::new(RoutingTableStats {
            total_entry_count: health.total_entry_count as u32,
            live_entry_count: health.live_entry_count() as u32,
            reliable_entry_count: health.reliable_entry_count() as u32,
            public_internet_ready: health.routing_domain_ready(RoutingDomain::PublicInternet),
            local_network_ready: health.routing_domain_ready(RoutingDomain::LocalNetwork),
            entries_by_state: health
                .per_state_entry_count
                .iter()
                .map(|(state, count)| StateCount {
                    state: *state,
                    count: *count as u32,
                })
                .collect(),
            pings: self.ping_stats.lock().snapshot(),
        }))
    }
}
