use super::*;

impl RPCProcessor {
    pub fn debug_info_nodeinfo(&self) -> String {
        let mut out = String::new();
        let inner = self.inner.lock();
        let dequeue_latency = inner
            .rpc_worker_dequeue_latency_accounting
            .latency_stats()
            .unwrap_or_default();
        let process_latency = inner
            .rpc_worker_process_latency_accounting
            .latency_stats()
            .unwrap_or_default();
        out += &format!(
            "RPC Worker Dequeue Latency:\n{}",
            indent_all_string(format!("{:#}", dequeue_latency))
        );
        out += "\n";
        out += &format!(
            "RPC Worker Process Latency:\n{}",
            indent_all_string(format!("{:#}", process_latency))
        );

        out += "\n";
        let rpc_message_processing_latency_string = inner
            .rpc_worker_process_latency_and_accounting_by_operation_kind
            .iter()
            .map(|(k, (acc, n))| {
                format!(
                    "{:>16}: {:#} (n={})",
                    k,
                    acc.latency_stats().unwrap_or_default(),
                    n
                )
            })
            .collect::<Vec<_>>()
            .join("\n");

        out += &format!(
            "RPC Message Processing Latency:\n{}",
            indent_all_string(&rpc_message_processing_latency_string)
        );

        out
    }
}
