//! Admission control for outbound DHT network operations: a concurrency cap
//! plus per-direction byte budgets.
//!
//! With the `aimd-dht` feature the budgets adapt via [`AimdController`];
//! without it (the default) they are pinned at `u64::MAX` and never bind, so
//! the concurrency cap is the only throttle.
//!
//! The congestion signal is each gated operation's own outcome, never
//! aggregate RPC statistics: routing-table grooming loses pings to dead
//! nodes on a healthy network, so any global loss rate reads as congestion
//! when there is none. Sets feed the upload budget, gets the download
//! budget. The initial budget is sized so the byte gate cannot bind while
//! the concurrency cap does; only a congestion-driven decrease makes it
//! bind, so a healthy network sees no behavior change at all.
use super::transaction_command::OutboundTransactCommandResult;
#[cfg(any(feature = "aimd-dht", feature = "stats"))]
use super::transaction_command::TransactCommandDisposition;
use super::*;

impl_veilid_log_facility!("stor");

/// Decrease ratio for a congested window: TCP-style halving
#[cfg(feature = "aimd-dht")]
const GATED_OPERATION_BUDGET_MD_SCALE_NUM: u64 = 1;
/// Denominator of the decrease ratio
#[cfg(feature = "aimd-dht")]
const GATED_OPERATION_BUDGET_MD_SCALE_DEN: u64 = 2;
/// Additive increase per demand window: about half a typical transact-set weight
#[cfg(feature = "aimd-dht")]
const GATED_OPERATION_BUDGET_AI_STEP_BPS: u64 = 32 * 1024;
/// A window is congested when at least half its completed ops were
#[cfg(feature = "aimd-dht")]
const GATED_OPERATION_BUDGET_CONGESTED_NUM: u64 = 1;
/// Denominator of the congested-window threshold
#[cfg(feature = "aimd-dht")]
const GATED_OPERATION_BUDGET_CONGESTED_DEN: u64 = 2;
/// Achieved throughput reflects the throttled rate being probed, so the
/// budget must be allowed to lead it by this factor
#[cfg(feature = "aimd-dht")]
const GATED_OPERATION_BUDGET_HEADROOM_FACTOR: u64 = 2;
/// The floor admits this many worst-case ops so collapse stays parallel
#[cfg(feature = "aimd-dht")]
const GATED_OPERATION_BUDGET_FLOOR_OP_MULTIPLE: u64 = 2;
/// Byte-stage waits below this are scheduler noise, not demand
#[cfg(feature = "aimd-dht")]
const GATED_OPERATION_BUDGET_DEMAND_WAIT_MIN_US: u64 = 1_000;

/// An op holds its budget bytes for roughly its round trip, so per-op cost
/// amortizes over the rpc timeout to admit a realistic number in flight
pub(super) fn amortization_secs(config: &VeilidConfig) -> u64 {
    (config.internal().network.rpc.timeout_ms / 1000).max(1) as u64
}

/// Heaviest single admission: a max-size subkey to the set fanout's node estimate
#[cfg(feature = "aimd-dht")]
pub(super) fn max_gated_operation_weight(config: &VeilidConfig) -> u64 {
    let est_node_count = (config.internal().network.dht.set_value_count
        + config.internal().network.dht.set_value_fanout) as u64;
    (MAX_SUBKEY_SIZE as u64).saturating_mul(est_node_count) / amortization_secs(config)
}

/// Non-binding by construction: the concurrency cap admits at most this much
/// amortized demand, so the byte gate only binds after a decrease
#[cfg(feature = "aimd-dht")]
pub(super) fn gated_operation_budget_initial(config: &VeilidConfig) -> u64 {
    (config.network.dht.max_concurrent_operations as u64)
        .max(1)
        .saturating_mul(max_gated_operation_weight(config))
}

#[cfg(feature = "aimd-dht")]
pub(super) fn gated_operation_budget_floor(config: &VeilidConfig) -> u64 {
    GATED_OPERATION_BUDGET_FLOOR_OP_MULTIPLE
        .saturating_mul(max_gated_operation_weight(config))
        .min(gated_operation_budget_initial(config))
}

#[cfg(feature = "aimd-dht")]
pub(super) fn gated_operation_budget_params(config: &VeilidConfig) -> AimdParams {
    AimdParams {
        initial: gated_operation_budget_initial(config),
        floor: gated_operation_budget_floor(config),
        md_scale_num: GATED_OPERATION_BUDGET_MD_SCALE_NUM,
        md_scale_den: GATED_OPERATION_BUDGET_MD_SCALE_DEN,
        ai_step: GATED_OPERATION_BUDGET_AI_STEP_BPS,
        congested_num: GATED_OPERATION_BUDGET_CONGESTED_NUM,
        congested_den: GATED_OPERATION_BUDGET_CONGESTED_DEN,
        headroom_factor: GATED_OPERATION_BUDGET_HEADROOM_FACTOR,
        // One op generation: congestion evidence from ops admitted before a
        // decrease keeps arriving for about this long
        md_cooldown_us: ms_to_us(config.internal().network.rpc.timeout_ms),
    }
}

#[cfg(any(feature = "aimd-dht", feature = "stats"))]
pub(super) fn fanout_result_congested(
    op: GatedOperationKind,
    fanout_result: &FanoutResult,
) -> bool {
    fanout_congested(op, fanout_result.kind, fanout_result.consensus_nodes.len())
}

/// Congestion signals sending slower could fix. A whole-fanout timeout is
/// queueing at our own uplink. A set exhausting with nothing accepted is the
/// same collapse per-RPC (every candidate timed out). Not gets: a get on a
/// nonexistent record legitimately exhausts empty. Exhausted with any
/// consensus is a candidate/routing condition, not congestion.
#[cfg(any(feature = "aimd-dht", feature = "stats"))]
pub(super) fn fanout_congested(
    op: GatedOperationKind,
    kind: FanoutResultKind,
    consensus_count: usize,
) -> bool {
    match kind {
        FanoutResultKind::Timeout => true,
        FanoutResultKind::Exhausted => {
            matches!(
                op,
                GatedOperationKind::Set | GatedOperationKind::TransactSet
            ) && consensus_count == 0
        }
        FanoutResultKind::Incomplete | FanoutResultKind::Consensus => false,
    }
}

/// Consensus failure with a majority of nodes silent. Invalid dispositions
/// are answers (a state problem, not transport trouble) and post-consensus
/// early-exit skips are excluded by the valid-count check
#[cfg(any(feature = "aimd-dht", feature = "stats"))]
pub(super) fn transact_counts_congested(
    valid_total: u64,
    skipped: u64,
    total: u64,
    required: u64,
) -> bool {
    total > 0 && valid_total < required && skipped.saturating_mul(2) >= total
}

#[cfg(any(feature = "aimd-dht", feature = "stats"))]
fn transact_result_congested(result: &OutboundTransactCommandResult) -> bool {
    let mut valid_total = result.params.pre_authorized_valid_count as u64;
    let mut skipped = 0u64;
    for pnr in &result.per_node_results {
        match pnr.disposition {
            TransactCommandDisposition::Valid => valid_total += 1,
            TransactCommandDisposition::Invalid => {}
            TransactCommandDisposition::Skipped | TransactCommandDisposition::Pending => {
                skipped += 1
            }
        }
    }
    transact_counts_congested(
        valid_total,
        skipped,
        result.per_node_results.len() as u64,
        result.params.required_strict_consensus_count as u64,
    )
}

/// DHT network operation kinds admitted through the operation gate
#[derive(Copy, Clone, Debug)]
pub(super) enum GatedOperationKind {
    Set,
    Get,
    TransactSet,
    TransactGet,
}

impl fmt::Display for GatedOperationKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Set => "set",
            Self::Get => "get",
            Self::TransactSet => "tx_set",
            Self::TransactGet => "tx_get",
        })
    }
}

/// One direction's byte budget: the AIMD controller decides the limit and
/// the weighted semaphore enforces it. Only this struct writes the semaphore
/// limit, so the two cannot drift. Without `aimd-dht` there is no controller
/// and the limit stays at `u64::MAX`.
#[derive(Debug)]
pub(super) struct GatedOperationBandwidthBudget {
    #[cfg(feature = "aimd-dht")]
    dir: &'static str,
    #[cfg(feature = "aimd-dht")]
    controller: AimdController,
    #[cfg(all(feature = "aimd-dht", feature = "stats"))]
    aimd_accounting: Mutex<AimdAccounting>,
    gate: Arc<AsyncWeightedSemaphore>,
}

impl GatedOperationBandwidthBudget {
    pub(super) fn new(dir: &'static str, config: &VeilidConfig) -> Self {
        cfg_if! {
            if #[cfg(feature = "aimd-dht")] {
                let params = gated_operation_budget_params(config);
                let gate = AsyncWeightedSemaphore::new(params.initial);
                Self {
                    dir,
                    controller: AimdController::new(params),
                    #[cfg(feature = "stats")]
                    aimd_accounting: Mutex::new(AimdAccounting::default()),
                    gate,
                }
            } else {
                let _ = (dir, config);
                Self {
                    gate: AsyncWeightedSemaphore::new(u64::MAX),
                }
            }
        }
    }

    async fn acquire(&self, weight: u64) -> AsyncWeightedSemaphoreGuard {
        self.gate.acquire(weight).await
    }

    #[cfg(feature = "aimd-dht")]
    pub(super) fn limit(&self) -> u64 {
        self.controller.limit()
    }

    /// The limit the semaphore is enforcing; equals `limit()` unless the
    /// mirror is broken
    #[cfg(feature = "aimd-dht")]
    #[cfg_attr(not(any(test, feature = "test-util")), expect(dead_code))]
    pub(super) fn enforced_limit(&self) -> u64 {
        self.gate.limit()
    }

    #[cfg(feature = "aimd-dht")]
    pub(super) fn record_outcome(&self, congested: bool) {
        self.controller.record_outcome(congested);
    }

    #[cfg(feature = "aimd-dht")]
    pub(super) fn record_demand(&self) {
        self.controller.record_demand();
    }

    #[cfg(feature = "aimd-dht")]
    pub(super) fn step(&self, now_us: u64, achieved_bps: u64) -> AimdStepResult {
        let step = self.controller.step(now_us, achieved_bps);
        veilid_stats!(self.aimd_accounting.lock().record(&step));
        if !matches!(step.decision, AimdDecision::Hold) {
            self.gate.set_limit(self.controller.limit());
        }
        step
    }

    #[cfg(all(feature = "aimd-dht", feature = "stats"))]
    pub(super) fn aimd_counters(&self) -> AimdCounters {
        self.aimd_accounting
            .lock()
            .counters(self.controller.limit())
    }

    #[cfg(feature = "aimd-dht")]
    pub(super) fn reset(&self) {
        self.controller.reset();
        self.gate.set_limit(self.controller.limit());
    }
}

/// Holds the concurrency permit + per-direction budget weights acquired for
/// one DHT network operation; releases all on drop.
#[allow(dead_code)]
pub(super) struct GatedOperationGuard {
    registry: VeilidComponentRegistry,
    kind: GatedOperationKind,
    acquired_ts: Timestamp,
    gate_active: Arc<AtomicUsize>,
    #[cfg(feature = "stats")]
    gate_accounting: Arc<Mutex<GatedOperationAccounting>>,
    concurrency: AsyncSemaphoreGuardArc,
    upload: Option<AsyncWeightedSemaphoreGuard>,
    download: Option<AsyncWeightedSemaphoreGuard>,
}

impl Drop for GatedOperationGuard {
    fn drop(&mut self) {
        let active = self.gate_active.fetch_sub(1, Ordering::AcqRel) - 1;
        let held = TimestampDuration::since_non_decreasing(self.acquired_ts);
        veilid_stats!(self
            .gate_accounting
            .lock()
            .record_completion(self.kind, held));
        let registry = &self.registry;
        #[cfg(feature = "verbose-tracing")]
        veilid_log!(registry debug "OpGate release: kind={} held_us={} active={}", self.kind, held.as_u64(), active);
        #[cfg(not(feature = "verbose-tracing"))]
        let _ = (active, held, registry);
    }
}

impl StorageManager {
    #[cfg(feature = "aimd-dht")]
    fn budget_for_kind(&self, kind: GatedOperationKind) -> &GatedOperationBandwidthBudget {
        match kind {
            GatedOperationKind::Set | GatedOperationKind::TransactSet => &self.upload_budget,
            GatedOperationKind::Get | GatedOperationKind::TransactGet => &self.download_budget,
        }
    }

    /// Feed the budget from a completed set/get fanout. Ops racing an
    /// offline transition are dropped; reset wipes the window on the next
    /// online edge anyway.
    pub(super) fn record_gated_fanout_outcome(
        &self,
        kind: GatedOperationKind,
        fanout_result: &FanoutResult,
    ) {
        #[cfg(any(feature = "aimd-dht", feature = "stats"))]
        let congested = fanout_result_congested(kind, fanout_result);
        #[cfg(feature = "stats")]
        if congested {
            self.gate_accounting.lock().record_retryable(kind);
        }
        #[cfg(feature = "aimd-dht")]
        if self.dht_is_online() {
            self.budget_for_kind(kind).record_outcome(congested);
        }
        #[cfg(not(any(feature = "aimd-dht", feature = "stats")))]
        let _ = (kind, fanout_result);
    }

    /// Feed the budget from a completed transact command
    pub(super) fn record_gated_transact_outcome(
        &self,
        kind: GatedOperationKind,
        result: &OutboundTransactCommandResult,
    ) {
        #[cfg(any(feature = "aimd-dht", feature = "stats"))]
        let congested = transact_result_congested(result);
        #[cfg(feature = "stats")]
        if congested {
            self.gate_accounting.lock().record_retryable(kind);
        }
        #[cfg(feature = "aimd-dht")]
        if self.dht_is_online() {
            self.budget_for_kind(kind).record_outcome(congested);
        }
        #[cfg(not(any(feature = "aimd-dht", feature = "stats")))]
        let _ = (kind, result);
    }

    /// Run one control interval per direction from the rolling transfer
    /// stats. Called from the network stats event handler; must not block.
    pub(super) fn step_gated_operation_budgets(&self, up_bps: u64, down_bps: u64) {
        cfg_if! {
            if #[cfg(feature = "aimd-dht")] {
                let now_us = Timestamp::now_non_decreasing().as_u64();
                for (budget, achieved_bps) in [
                    (&self.upload_budget, up_bps),
                    (&self.download_budget, down_bps),
                ] {
                    let step = budget.step(now_us, achieved_bps);
                    let reason = match step.decision {
                        AimdDecision::Hold => continue,
                        AimdDecision::Lower(_) => "lower",
                        AimdDecision::Clamp(_) => "clamp",
                        AimdDecision::Raise(_) => "raise",
                    };
                    veilid_log!(self debug "OpGate budget: dir={} limit={} reason={} congested={}/{} achieved_bps={}",
                        budget.dir, budget.limit(), reason, step.congested, step.completed, achieved_bps);
                }
            } else {
                let _ = (up_bps, down_bps);
            }
        }
    }

    /// Restore both budgets to their initial limits. Called on the
    /// offline-to-online edge: a new network invalidates learned budgets.
    pub(super) fn reset_gated_operation_budgets(&self) {
        #[cfg(feature = "aimd-dht")]
        for budget in [&self.upload_budget, &self.download_budget] {
            budget.reset();
            veilid_log!(self debug "OpGate budget: dir={} limit={} reason=reset", budget.dir, budget.limit());
        }
    }

    /// Admit an outbound set fanout: `value_bytes` up to the estimated fanout
    /// width. Node count is unknown before the fanout; estimate consensus plus
    /// a lane-width of overshoot.
    pub(super) async fn acquire_gated_set(&self, value_bytes: u64) -> GatedOperationGuard {
        let config = self.config();
        let dht = &config.internal().network.dht;
        let est_node_count = (dht.set_value_count + dht.set_value_fanout) as u64;
        self.acquire_gated_operation(
            GatedOperationKind::Set,
            Some(value_bytes.saturating_mul(est_node_count)),
            None,
        )
        .await
    }

    /// Admit an outbound get fanout: a full value down from the estimated
    /// fanout width. Node count is unknown before the fanout; estimate
    /// consensus plus a lane-width of overshoot.
    pub(super) async fn acquire_gated_get(&self) -> GatedOperationGuard {
        let config = self.config();
        let dht = &config.internal().network.dht;
        let est_node_count = (dht.get_value_count + dht.get_value_fanout) as u64;
        self.acquire_gated_operation(
            GatedOperationKind::Get,
            None,
            Some((MAX_SUBKEY_SIZE as u64).saturating_mul(est_node_count)),
        )
        .await
    }

    /// Admit a transact set: `value_bytes` up to each of the transaction's
    /// actual nodes
    pub(super) async fn acquire_gated_transact_set(
        &self,
        value_bytes: u64,
        node_count: usize,
    ) -> GatedOperationGuard {
        self.acquire_gated_operation(
            GatedOperationKind::TransactSet,
            Some(value_bytes.saturating_mul(node_count as u64)),
            None,
        )
        .await
    }

    /// Admit a transact get: a full value down from each of the transaction's
    /// actual nodes
    pub(super) async fn acquire_gated_transact_get(
        &self,
        node_count: usize,
    ) -> GatedOperationGuard {
        self.acquire_gated_operation(
            GatedOperationKind::TransactGet,
            None,
            Some((MAX_SUBKEY_SIZE as u64).saturating_mul(node_count as u64)),
        )
        .await
    }

    /// Acquire the DHT-operation concurrency permit plus the per-direction
    /// budget weights this op needs (upload for sets, download for gets);
    /// the returned guard releases all on drop. Waits if the gates are full.
    async fn acquire_gated_operation(
        &self,
        kind: GatedOperationKind,
        upload_cost: Option<u64>,
        download_cost: Option<u64>,
    ) -> GatedOperationGuard {
        let secs = amortization_secs(&self.config());
        let amortize = |c: Option<u64>| c.map(|b| (b / secs).max(1));
        let upload_cost = amortize(upload_cost);
        let download_cost = amortize(download_cost);

        let wait_start_ts = Timestamp::now_non_decreasing();
        let concurrency = self.gated_operation_concurrency.acquire_arc().await;
        let mut waited_bw_us = 0u64;
        let upload = match upload_cost {
            Some(cost) => {
                let bw_start_ts = Timestamp::now_non_decreasing();
                let guard = self.upload_budget.acquire(cost).await;
                let waited = TimestampDuration::since_non_decreasing(bw_start_ts).as_u64();
                #[cfg(feature = "aimd-dht")]
                if waited >= GATED_OPERATION_BUDGET_DEMAND_WAIT_MIN_US {
                    self.upload_budget.record_demand();
                }
                waited_bw_us += waited;
                Some(guard)
            }
            None => None,
        };
        let download = match download_cost {
            Some(cost) => {
                let bw_start_ts = Timestamp::now_non_decreasing();
                let guard = self.download_budget.acquire(cost).await;
                let waited = TimestampDuration::since_non_decreasing(bw_start_ts).as_u64();
                #[cfg(feature = "aimd-dht")]
                if waited >= GATED_OPERATION_BUDGET_DEMAND_WAIT_MIN_US {
                    self.download_budget.record_demand();
                }
                waited_bw_us += waited;
                Some(guard)
            }
            None => None,
        };
        let acquired_ts = Timestamp::now_non_decreasing();
        let waited = acquired_ts.duration_since(wait_start_ts);
        veilid_stats!(self.gate_accounting.lock().record_wait(kind, waited));
        let active = self.gate_active.fetch_add(1, Ordering::AcqRel) + 1;
        #[cfg(feature = "verbose-tracing")]
        veilid_log!(self debug "OpGate acquire: kind={} waited_us={} waited_bw_us={} active={}", kind, waited.as_u64(), waited_bw_us, active);
        #[cfg(not(feature = "verbose-tracing"))]
        let _ = (waited, waited_bw_us, active);

        GatedOperationGuard {
            registry: self.registry(),
            kind,
            acquired_ts,
            gate_active: self.gate_active.clone(),
            #[cfg(feature = "stats")]
            gate_accounting: self.gate_accounting.clone(),
            concurrency,
            upload,
            download,
        }
    }
}
