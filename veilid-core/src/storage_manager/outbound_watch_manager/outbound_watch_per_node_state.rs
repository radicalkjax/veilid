use super::*;

impl_veilid_log_facility!("stor");

#[derive(Clone, Debug, Serialize, Deserialize)]
pub(in crate::storage_manager) struct OutboundWatchPerNodeState {
    /// Watch Id
    pub watch_id: u64,
    /// SafetySelection used to contact the node
    pub safety_selection: SafetySelection,
    /// What key was used to perform the watch
    pub opt_watcher: Option<KeyPair>,
    /// The expiration of a successful watch
    pub expiration: Timestamp,
    /// How many value change notifications are left
    pub count: u32,
    /// Resolved watch node reference
    #[serde(skip)]
    pub watch_node_ref: Option<NodeRef>,
    /// Which private route is responsible for receiving ValueChanged notifications
    pub opt_value_changed_route: Option<PublicKey>,
}

impl fmt::Display for OutboundWatchPerNodeState {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{{ watch_id={}, safety_selection={:#}, opt_watcher={}, expiration={:#}, count={}, watch_node_ref={}, opt_value_changed_route={} }}",
            self.watch_id,
            self.safety_selection,
            f.to_string_opt(self.opt_watcher.as_ref()),
            self.expiration,
            self.count,
            f.to_string_opt(self.watch_node_ref.as_ref()),
            f.to_string_opt(self.opt_value_changed_route.as_ref()),
        )
    }
}

impl OutboundWatchPerNodeState {
    /// Whether this per-node watch is inside its own renew window or lost its ValueChanged route
    pub fn needs_renewal(
        &self,
        cur_ts: Timestamp,
        renew_ts: Timestamp,
        route_is_dead: impl Fn(&PublicKey) -> bool,
    ) -> bool {
        (renew_ts >= self.expiration && cur_ts < self.expiration)
            || self
                .opt_value_changed_route
                .as_ref()
                .is_some_and(route_is_dead)
    }
}
