use super::*;

#[derive(Debug, Serialize)]
pub(super) struct RPCProcessorStats {
    /// Time a message waits for a worker
    worker_dequeue_latency: LatencyStats,
    /// Time a worker spends on a message
    worker_process_latency: LatencyStats,
    /// Messages waiting for a worker
    queue_depth: u32,
    /// Processing count and latency per operation kind
    process_by_kind: Vec<RPCKindStats>,
    /// Answers that landed after the caller stopped waiting, by kind, since `stats reset`
    answers_after_waiter_dropped: Vec<RPCKindCount>,
}

#[derive(Debug, Serialize)]
pub(super) struct RPCKindCount {
    /// Operation kind name
    kind: String,
    /// Occurrences
    count: u64,
}

#[derive(Debug, Serialize)]
pub(super) struct RPCKindStats {
    kind: String,
    count: u64,
    latency: LatencyStats,
}

impl RPCProcessor {
    pub(super) fn reset_stats_impl(&self) {
        self.answers_after_waiter_dropped.lock().clear();
    }

    pub(super) fn stats_impl(&self) -> Option<Box<dyn VeilidComponentStats>> {
        let inner = self.inner.lock();
        Some(Box::new(RPCProcessorStats {
            worker_dequeue_latency: inner
                .rpc_worker_dequeue_latency_accounting
                .latency_stats()
                .unwrap_or_default(),
            worker_process_latency: inner
                .rpc_worker_process_latency_accounting
                .latency_stats()
                .unwrap_or_default(),
            queue_depth: inner
                .rpc_send_channel
                .as_ref()
                .map(|s| s.len() as u32)
                .unwrap_or(0),
            process_by_kind: inner
                .rpc_worker_process_latency_and_accounting_by_operation_kind
                .iter()
                .map(|(kind, (acc, count))| RPCKindStats {
                    kind: (*kind).to_string(),
                    count: *count,
                    latency: acc.latency_stats().unwrap_or_default(),
                })
                .collect(),
            answers_after_waiter_dropped: self
                .answers_after_waiter_dropped
                .lock()
                .iter()
                .map(|(kind, count)| RPCKindCount {
                    kind: (*kind).to_string(),
                    count: *count,
                })
                .collect(),
        }))
    }
}
