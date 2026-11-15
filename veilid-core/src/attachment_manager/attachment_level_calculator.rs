//! AttachmentState calculator: routing-table size + latency + network estimate → bars.

use super::*;

const SATURATION_TARGET_RELIABLE_NODES: u64 = 32;

/// Percents of the RPC timeout at which median peer latency takes off one more bar
const LATENCY_PENALTY_PERCENTS: [u64; 3] = [2, 5, 10];

/// Attachment level durations derived from the RPC timeout
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::attachment_manager) struct AttachmentLevelTimings {
    /// Median peer latency at or past each takes off one more bar
    pub latency_penalty_thresholds: [TimestampDuration; 3],
}

impl AttachmentLevelTimings {
    pub fn new(rpc_timeout: TimestampDuration) -> Self {
        Self {
            latency_penalty_thresholds: LATENCY_PENALTY_PERCENTS
                .map(|percent| rpc_timeout.saturating_mul(percent).div(100)),
        }
    }
}

#[derive(Clone, Debug, Default)]
pub(in crate::attachment_manager) struct AttachmentLevelInputs {
    /// Nodes in 'reliable' state
    pub reliable_count: usize,
    /// Nodes in 'reliable' + 'unreliable' state
    pub responsive_count: usize,
    /// Nodes in 'reliable' + 'unreliable' + 'initial' state
    pub live_count: usize,
    /// Smoothed estimate of total reachable network size
    pub estimated_network_size: u64,
    /// Median p75 latency across reliable peers, None when no samples yet.
    pub median_latency: Option<TimestampDuration>,
    /// Nodes in bucket overflow awaiting lazy kick
    pub excess_kickable: usize,
}

pub(in crate::attachment_manager) fn compute_attachment_state(
    inputs: &AttachmentLevelInputs,
    timings: &AttachmentLevelTimings,
) -> AttachmentState {
    // No live nodes at all: still attaching.
    if inputs.live_count == 0 {
        return AttachmentState::Attaching;
    }

    let target = SATURATION_TARGET_RELIABLE_NODES
        .min(inputs.estimated_network_size / 2)
        .max(1) as usize;

    // Bars are driven by nodes we have actually contacted (tested), not by
    // never-contacted live nodes.
    let raw_bars = inputs
        .responsive_count
        .saturating_mul(5)
        .saturating_div(target)
        .min(5) as u8;

    let latency_penalty = inputs.median_latency.map_or(0, |latency| {
        timings
            .latency_penalty_thresholds
            .iter()
            .filter(|threshold| latency >= **threshold)
            .count() as u8
    });

    let bars = raw_bars.saturating_sub(latency_penalty);

    // Floor at AttachedWeak: any live node means we are at least weakly attached.
    match bars {
        0 | 1 => AttachmentState::AttachedWeak,
        2 => AttachmentState::AttachedFair,
        3 => AttachmentState::AttachedGood,
        4 => AttachmentState::AttachedStrong,
        _ => AttachmentState::AttachedFull,
    }
}

#[derive(Clone)]
pub(in crate::attachment_manager) struct AttachmentLevelCalculator {
    registry: VeilidComponentRegistry,
    timings: AttachmentLevelTimings,
    inner: Arc<Mutex<AttachmentLevelCalculatorInner>>,
}

impl fmt::Debug for AttachmentLevelCalculator {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("AttachmentLevelCalculator")
            .field("inner", &*self.inner.lock())
            .finish()
    }
}

#[derive(Debug)]
struct AttachmentLevelCalculatorInner {
    last_state: AttachmentState,
    last_inputs: AttachmentLevelInputs,
}

impl AttachmentLevelCalculator {
    pub fn new(registry: VeilidComponentRegistry) -> Self {
        let timings = AttachmentLevelTimings::new(registry.config().rpc_timeout());
        Self {
            registry,
            timings,
            inner: Arc::new(Mutex::new(AttachmentLevelCalculatorInner {
                last_state: AttachmentState::Detached,
                last_inputs: AttachmentLevelInputs::default(),
            })),
        }
    }

    pub fn recompute(&self) -> AttachmentState {
        let routing_table = self.registry.routing_table();
        let health = routing_table.get_routing_table_health();

        let inputs = AttachmentLevelInputs {
            reliable_count: health.reliable_entry_count(),
            responsive_count: health.responsive_entry_count(),
            live_count: health.live_entry_count(),
            estimated_network_size: routing_table.estimate_network_size_combined(),
            median_latency: median_p75_latency(&routing_table),
            excess_kickable: routing_table.excess_kickable_count(),
        };
        let state = compute_attachment_state(&inputs, &self.timings);

        let mut inner = self.inner.lock();
        inner.last_state = state;
        inner.last_inputs = inputs;
        state
    }

    pub fn last_inputs(&self) -> AttachmentLevelInputs {
        self.inner.lock().last_inputs.clone()
    }
}

fn median_p75_latency(routing_table: &RoutingTable) -> Option<TimestampDuration> {
    let snapshot = routing_table.snapshot_entries(Timestamp::now(), BucketEntryState::Reliable);
    let mut p75s: Vec<TimestampDuration> = snapshot
        .entries()
        .iter()
        .filter_map(|e| e.peer_stats.latency.as_ref().map(|ls| ls.p75))
        .collect();
    if p75s.is_empty() {
        return None;
    }
    p75s.sort_unstable();
    Some(p75s[p75s.len() / 2])
}
