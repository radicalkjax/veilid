//! Cumulative transaction-quality counters: per-command fanout outcomes, subkey
//! confirmation coverage at End and Commit, node-set attrition, and the cost of
//! every mitigation (retry pass, keepalives, terminal delivery, TryAgain).
//! The serializable snapshot lives in `mod.rs`.
//! Counters accumulate until `stats reset`; benchmarks report deltas.
use super::*;

/// Outbound transaction command kinds, Begin included
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(in crate::storage_manager) enum TransactionCommandKind {
    Begin,
    Set,
    End,
    Commit,
    Rollback,
    Get,
}

impl TransactionCommandKind {
    pub fn name(self) -> &'static str {
        match self {
            Self::Begin => "begin",
            Self::Set => "set",
            Self::End => "end",
            Self::Commit => "commit",
            Self::Rollback => "rollback",
            Self::Get => "get",
        }
    }
}

impl From<TransactCommand> for TransactionCommandKind {
    fn from(c: TransactCommand) -> Self {
        match c {
            TransactCommand::End => Self::End,
            TransactCommand::Commit => Self::Commit,
            TransactCommand::Rollback => Self::Rollback,
            TransactCommand::Get => Self::Get,
            TransactCommand::Set => Self::Set,
        }
    }
}

/// Why a transaction call returned TryAgain
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(in crate::storage_manager) enum TryAgainReason {
    BeginContended,
    NotOnline,
    OfflineWrites,
    BeginConsensus,
    SetConsensus,
    StageFailed,
    InTransaction,
    Handles,
    BackgroundDeferred,
    EndIncomplete,
    CommitConsensus,
    Other,
}

/// What one outbound command's fanout did, recorded when it returns
#[derive(Debug, Default)]
pub(in crate::storage_manager) struct CommandOutcome {
    /// Per-node RPCs sent
    pub rpcs_sent: u64,
    /// Answers with the transaction still valid
    pub answered_valid: u64,
    /// Answers with the transaction no longer valid
    pub answered_invalid: u64,
    /// Per-node timeouts inside the wait
    pub no_response: u64,
    /// Nodes still in flight when the command returned
    pub abandoned_in_flight: u64,
    /// The command returned with nodes still in flight
    pub post_consensus_window_expired: bool,
    /// Valid answers came up short of required
    pub short_of_required: bool,
    /// Wall time from issue to return
    pub duration: TimestampDuration,
}

/// Late Set answers that were recorded into node state instead of dropped
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(in crate::storage_manager) enum LateAnswer {
    Valid,
    Invalid,
}

/// Per command kind accumulator
#[derive(Debug, Default)]
struct CommandAccounting {
    /// Commands issued
    commands: u64,
    /// Per-node RPCs sent
    rpcs_sent: u64,
    /// Answers with the transaction still valid
    answered_valid: u64,
    /// Answers with the transaction no longer valid
    answered_invalid: u64,
    /// Per-node timeouts inside the wait
    no_response: u64,
    /// Nodes still in flight when a command returned
    abandoned_in_flight: u64,
    /// Commands that returned with nodes still in flight
    post_consensus_window_expired: u64,
    /// Commands whose valid answers came up short of required
    short_of_required: u64,
    /// Answers that landed after the command returned and were still recorded
    collected_after_early_exit: u64,
    /// Rolling command wall times
    duration: LatencyStatsAccounting,
}

/// One kind's command telemetry, read by the storage manager stats snapshot
#[derive(Debug, Clone, Serialize)]
pub(in crate::storage_manager) struct CommandStats {
    /// Command kind name
    pub kind: &'static str,
    /// Commands issued
    pub commands: u64,
    /// Per-node RPCs sent
    pub rpcs_sent: u64,
    /// Answers with the transaction still valid
    pub answered_valid: u64,
    /// Answers with the transaction no longer valid
    pub answered_invalid: u64,
    /// Per-node timeouts inside the wait
    pub no_response: u64,
    /// Nodes still in flight when a command returned
    pub abandoned_in_flight: u64,
    /// Commands that returned with nodes still in flight
    pub post_consensus_window_expired: u64,
    /// Commands whose valid answers came up short of required
    pub short_of_required: u64,
    /// Answers that landed after the command returned and were still recorded
    pub collected_after_early_exit: u64,
    /// Command wall time window
    pub duration: LatencyStats,
}

/// Subkey confirmation coverage measured when End is sent and answered
#[derive(Debug, Default, Clone, Serialize)]
pub(in crate::storage_manager) struct EndCoverageStats {
    /// Nodes sent End
    pub nodes_sent_end: u64,
    /// Nodes sent End with a desired subkey unconfirmed
    pub nodes_sent_end_incomplete: u64,
    /// Unconfirmed desired subkeys summed over those nodes
    pub missing_subkey_confirmations: u64,
    /// Records with a desired subkey confirmed by fewer End voters than required
    pub records_below_required_coverage: u64,
    /// Records whose minimum coverage was measured
    pub coverage_records: u64,
    /// Sum of per-record minimum coverage
    pub coverage_sum: u64,
    /// Lowest per-record minimum coverage seen
    pub coverage_min: u64,
}

/// Subkey coverage over Commit-stage nodes, measured when Commit results land
#[derive(Debug, Default, Clone, Serialize)]
pub(in crate::storage_manager) struct CommitCoverageStats {
    /// Records with a desired subkey held by fewer committed nodes than required
    pub records_below_required_coverage: u64,
    /// Commits that returned Ok below required, with rehydration scheduled
    pub commits_ok_below_required: u64,
    /// Commit rounds sent after the first, inside the End window
    pub commit_retry_rounds: u64,
    /// Commit answers of InvalidTransaction
    pub commit_refusals: u64,
    /// Nodes promoted to Commit without an RPC
    pub pre_authorized_nodes: u64,
}

/// Node-set attrition per record, summed so ratios are sum/sum
#[derive(Debug, Default, Clone, Serialize)]
pub(in crate::storage_manager) struct NodeSetStats {
    /// Records that reached Begin consensus
    pub records: u64,
    /// Node transactions accepted at Begin
    pub begin_nodes: u64,
    /// Nodes sent End
    pub end_sent_nodes: u64,
    /// Nodes that answered End as still valid
    pub end_valid_nodes: u64,
    /// Nodes sent Commit
    pub commit_sent_nodes: u64,
    /// Nodes that answered Commit as still valid
    pub commit_valid_nodes: u64,
    /// Nodes sent Rollback
    pub rollback_sent_nodes: u64,
}

/// TryAgain returns by reason
#[derive(Debug, Default, Clone, Serialize)]
pub(in crate::storage_manager) struct TryAgainStats {
    /// Begin found the records locked
    pub begin_contended: u64,
    /// The DHT was offline
    pub not_online: u64,
    /// A record had pending offline writes
    pub offline_writes: u64,
    /// Begin fell short of consensus
    pub begin_consensus: u64,
    /// A Set fell short of consensus
    pub set_consensus: u64,
    /// The transaction stage was Failed
    pub stage_failed: u64,
    /// A record was already in a transaction
    pub in_transaction: u64,
    /// No free handle or node transaction id
    pub handles: u64,
    /// A background transaction yielded to foreground activity
    pub background_deferred: u64,
    /// End found fewer complete nodes than required
    pub end_incomplete: u64,
    /// Commit fell short of the per-subkey minimum
    pub commit_consensus: u64,
    /// Any other reason
    pub other: u64,
}

/// Cost of each mitigation, plus transaction outcomes
#[derive(Debug, Default, Clone, Serialize)]
pub(in crate::storage_manager) struct MitigationStats {
    /// Targeted Set re-sends before End
    pub completion_pass_rpcs: u64,
    /// End attempts that waited for in-flight Sets
    pub end_waits: u64,
    /// Keepalive trains registered
    pub keepalives_registered: u64,
    /// Keepalive Gets sent
    pub keepalive_gets_sent: u64,
    /// Keepalive deadlines that found the node already refreshed
    pub keepalives_skipped: u64,
    /// Terminal deliveries scheduled at drop
    pub terminal_delivery_scheduled: u64,
    /// Terminal deliveries answered
    pub terminal_delivery_completed: u64,
    /// Terminal deliveries that ran past the server-side expiration
    pub terminal_delivery_expired: u64,
    /// TryAgain returns by reason
    pub try_again: TryAgainStats,
    /// Transactions committed
    pub transactions_committed: u64,
    /// Transactions rolled back by the app
    pub transactions_rolled_back: u64,
    /// Transactions dropped by the rollback guard
    pub transactions_failed: u64,
}

/// Storage-node side refusals, meaningful only on nodes that serve records
#[derive(Debug, Default, Clone, Serialize)]
pub(in crate::storage_manager) struct InboundStats {
    /// Sets refused because the transaction had ended
    pub set_after_end: u64,
    /// Ends refused because the record changed since Begin
    pub end_snapshot_mismatch: u64,
    /// Commits refused because the transaction did not hold the lock
    pub commit_bad_lock_state: u64,
}

#[derive(Debug, Default)]
pub(in crate::storage_manager) struct TransactionQualityAccounting {
    /// Begin commands
    begin: CommandAccounting,
    /// Set commands
    set: CommandAccounting,
    /// End commands
    end: CommandAccounting,
    /// Commit commands
    commit: CommandAccounting,
    /// Rollback commands
    rollback: CommandAccounting,
    /// Get commands
    get: CommandAccounting,
    /// Coverage at End
    pub end_coverage: EndCoverageStats,
    /// Coverage at Commit
    pub commit_coverage: CommitCoverageStats,
    /// Node-set attrition
    pub node_sets: NodeSetStats,
    /// Mitigation costs and outcomes
    pub mitigations: MitigationStats,
    /// Storage-node side refusals
    pub inbound: InboundStats,
    /// Rolling End wait durations
    end_wait: LatencyStatsAccounting,
}

impl TransactionQualityAccounting {
    pub fn record_end_wait(&mut self, wait: TimestampDuration) {
        self.mitigations.end_waits += 1;
        self.end_wait.record_latency(wait);
    }

    pub fn end_wait_duration(&self) -> LatencyStats {
        self.end_wait.latency_stats().unwrap_or_default()
    }

    pub fn record_collected(&mut self, kind: TransactionCommandKind, answer: LateAnswer) {
        let c = self.command_mut(kind);
        c.collected_after_early_exit += 1;
        match answer {
            LateAnswer::Valid => c.answered_valid += 1,
            LateAnswer::Invalid => c.answered_invalid += 1,
        }
    }

    fn command_mut(&mut self, kind: TransactionCommandKind) -> &mut CommandAccounting {
        match kind {
            TransactionCommandKind::Begin => &mut self.begin,
            TransactionCommandKind::Set => &mut self.set,
            TransactionCommandKind::End => &mut self.end,
            TransactionCommandKind::Commit => &mut self.commit,
            TransactionCommandKind::Rollback => &mut self.rollback,
            TransactionCommandKind::Get => &mut self.get,
        }
    }

    pub fn record_command(&mut self, kind: TransactionCommandKind, outcome: CommandOutcome) {
        let c = self.command_mut(kind);
        c.commands += 1;
        c.rpcs_sent += outcome.rpcs_sent;
        c.answered_valid += outcome.answered_valid;
        c.answered_invalid += outcome.answered_invalid;
        c.no_response += outcome.no_response;
        c.abandoned_in_flight += outcome.abandoned_in_flight;
        c.post_consensus_window_expired += u64::from(outcome.post_consensus_window_expired);
        c.short_of_required += u64::from(outcome.short_of_required);
        c.duration.record_latency(outcome.duration);
    }

    /// One record's min-over-subkeys coverage at End; None when nothing was desired
    pub fn record_end_coverage(&mut self, opt_min_coverage: Option<usize>, required: usize) {
        let Some(min_coverage) = opt_min_coverage else {
            return;
        };
        let ec = &mut self.end_coverage;
        if min_coverage < required {
            ec.records_below_required_coverage += 1;
        }
        let min_coverage = min_coverage as u64;
        if ec.coverage_records == 0 || min_coverage < ec.coverage_min {
            ec.coverage_min = min_coverage;
        }
        ec.coverage_records += 1;
        ec.coverage_sum += min_coverage;
    }

    pub fn try_again(&mut self, reason: TryAgainReason) {
        let ta = &mut self.mitigations.try_again;
        let counter = match reason {
            TryAgainReason::BeginContended => &mut ta.begin_contended,
            TryAgainReason::NotOnline => &mut ta.not_online,
            TryAgainReason::OfflineWrites => &mut ta.offline_writes,
            TryAgainReason::BeginConsensus => &mut ta.begin_consensus,
            TryAgainReason::SetConsensus => &mut ta.set_consensus,
            TryAgainReason::StageFailed => &mut ta.stage_failed,
            TryAgainReason::InTransaction => &mut ta.in_transaction,
            TryAgainReason::Handles => &mut ta.handles,
            TryAgainReason::BackgroundDeferred => &mut ta.background_deferred,
            TryAgainReason::EndIncomplete => &mut ta.end_incomplete,
            TryAgainReason::CommitConsensus => &mut ta.commit_consensus,
            TryAgainReason::Other => &mut ta.other,
        };
        *counter += 1;
    }

    pub fn commands(&self) -> [CommandStats; 6] {
        let snap = |kind: TransactionCommandKind, c: &CommandAccounting| CommandStats {
            kind: kind.name(),
            commands: c.commands,
            rpcs_sent: c.rpcs_sent,
            answered_valid: c.answered_valid,
            answered_invalid: c.answered_invalid,
            no_response: c.no_response,
            abandoned_in_flight: c.abandoned_in_flight,
            post_consensus_window_expired: c.post_consensus_window_expired,
            short_of_required: c.short_of_required,
            collected_after_early_exit: c.collected_after_early_exit,
            duration: c.duration.latency_stats().unwrap_or_default(),
        };
        [
            snap(TransactionCommandKind::Begin, &self.begin),
            snap(TransactionCommandKind::Set, &self.set),
            snap(TransactionCommandKind::End, &self.end),
            snap(TransactionCommandKind::Commit, &self.commit),
            snap(TransactionCommandKind::Rollback, &self.rollback),
            snap(TransactionCommandKind::Get, &self.get),
        ]
    }
}
