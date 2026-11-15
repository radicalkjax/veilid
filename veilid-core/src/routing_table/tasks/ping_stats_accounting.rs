use super::*;

use ping_validator::{PingValidationGroup, PingValidationKey};

/// Why a ping was sent, derived from its queue group and key shape
#[derive(Copy, Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum PingPurpose {
    Reliability,
    RelayKeepalive,
    WatchKeepalive,
    RouteLoopback,
    RouteTest,
}

impl PingPurpose {
    /// Relay keepalives are the only Keepalive-group pings addressed to a dial info
    pub fn derive(group: PingValidationGroup, key: &PingValidationKey, dest: &Destination) -> Self {
        match group {
            PingValidationGroup::Reliability => Self::Reliability,
            PingValidationGroup::RouteTest => Self::RouteTest,
            PingValidationGroup::Keepalive => match (key, dest) {
                (PingValidationKey::RouteTest { .. }, _) => Self::RouteLoopback,
                (_, Destination::DialInfo { .. }) => Self::RelayKeepalive,
                _ => Self::WatchKeepalive,
            },
        }
    }
}

/// How a ping concluded
#[derive(Copy, Clone, Debug)]
pub(crate) enum PingOutcome {
    Answered,
    Lost,
    SendFailed,
    NotSent,
    Error,
}

#[derive(Default)]
struct PingOutcomeCounts {
    answered: u64,
    lost: u64,
    send_failed: u64,
    not_sent: u64,
    error: u64,
}

#[derive(Default)]
struct PingGroupAccounting {
    queued: usize,
    in_flight: usize,
    started: u64,
    queue_wait_acc: LatencyStatsAccounting,
    service_acc: LatencyStatsAccounting,
}

/// Cumulative ping outcomes and queue timings, surfaced through RoutingTableStats
#[derive(Default)]
pub(crate) struct PingStatsAccounting {
    by_purpose: BTreeMap<(PingPurpose, Option<TransportType>), PingOutcomeCounts>,
    groups: BTreeMap<PingValidationGroup, PingGroupAccounting>,
}

impl PingStatsAccounting {
    pub fn record_queue_wait(&mut self, group: PingValidationGroup, wait: TimestampDuration) {
        let g = self.groups.entry(group).or_default();
        g.queue_wait_acc.record_latency(wait);
        g.started += 1;
    }

    pub fn record_completion(
        &mut self,
        group: PingValidationGroup,
        purpose: PingPurpose,
        transport: Option<TransportType>,
        outcome: PingOutcome,
        service: TimestampDuration,
    ) {
        let g = self.groups.entry(group).or_default();
        g.service_acc.record_latency(service);

        let c = self.by_purpose.entry((purpose, transport)).or_default();
        match outcome {
            PingOutcome::Answered => c.answered += 1,
            PingOutcome::Lost => c.lost += 1,
            PingOutcome::SendFailed => c.send_failed += 1,
            PingOutcome::NotSent => c.not_sent += 1,
            PingOutcome::Error => c.error += 1,
        }
    }

    pub fn record_queue_gauges(&mut self, gauges: &[(PingValidationGroup, usize, usize)]) {
        for (group, queued, in_flight) in gauges {
            let g = self.groups.entry(*group).or_default();
            g.queued = *queued;
            g.in_flight = *in_flight;
        }
    }

    pub fn snapshot(&self) -> PingStats {
        PingStats {
            purposes: self
                .by_purpose
                .iter()
                .map(|((purpose, transport), c)| PingPurposeStats {
                    kind: match transport {
                        Some(t) => format!("{:?}({})", purpose, t),
                        None => format!("{:?}", purpose),
                    },
                    answered: c.answered,
                    lost: c.lost,
                    send_failed: c.send_failed,
                    not_sent: c.not_sent,
                    error: c.error,
                })
                .collect(),
            groups: self
                .groups
                .iter()
                .map(|(group, g)| PingGroupStats {
                    kind: format!("{:?}", group),
                    queued: g.queued as u32,
                    in_flight: g.in_flight as u32,
                    count: g.started,
                    queue_wait: g.queue_wait_acc.latency_stats().unwrap_or_default(),
                    service: g.service_acc.latency_stats().unwrap_or_default(),
                })
                .collect(),
        }
    }
}

#[derive(Debug, Serialize)]
pub(crate) struct PingStats {
    purposes: Vec<PingPurposeStats>,
    groups: Vec<PingGroupStats>,
}

#[derive(Debug, Serialize)]
struct PingPurposeStats {
    kind: String,
    answered: u64,
    lost: u64,
    send_failed: u64,
    not_sent: u64,
    error: u64,
}

#[derive(Debug, Serialize)]
struct PingGroupStats {
    kind: String,
    queued: u32,
    in_flight: u32,
    count: u64,
    queue_wait: LatencyStats,
    service: LatencyStats,
}
