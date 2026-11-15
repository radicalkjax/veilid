use super::*;

impl NetworkManager {
    pub fn debug_info_nodeinfo(&self) -> String {
        let mut out = String::new();
        let stats = self.accounting.read();
        let dequeue_latency = stats
            .relay_worker_dequeue_latency_accounting
            .latency_stats()
            .unwrap_or_default();
        let process_latency = stats
            .relay_worker_process_latency_accounting
            .latency_stats()
            .unwrap_or_default();
        out += &format!(
            "Relay Worker Dequeue Latency:\n{}",
            indent_all_string(format!("{:#}", dequeue_latency))
        );
        out += "\n";
        out += &format!(
            "Relay Worker Process Latency:\n{}",
            indent_all_string(format!("{:#}", process_latency))
        );
        out += "\n";
        out += &format!(
            "Online State:\n{}",
            indent_all_string(self.online_detector().debug_info())
        );
        out
    }

    #[cfg_attr(not(feature = "debug-api"), expect(dead_code))]
    pub fn debug(&self) -> String {
        let stats = self.get_stats();

        let mut out = String::new();
        out += "Network Manager\n";
        out += "---------------\n";
        let mut out = format!(
            "Transfer stats:\n{}\n",
            indent_all_string(format!("{:#}", stats.self_stats.transfer_stats))
        );
        out += &self.debug_info_nodeinfo();
        out += "\n";
        out += "Node Contact Method Cache\n";
        out += "-------------------------\n";
        out += &self.inner.lock().node_contact_method_cache.debug();

        out
    }
}
