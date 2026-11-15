use super::*;

/// The state a routed question was lost in. The loss counts against a route or node only while
/// all of it still holds, so a peer info change on either side or an online state change clears
/// it without any timer.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct LossEpoch {
    /// The destination node's PublicInternet node info timestamp, if the destination is a node
    pub destination: Option<Timestamp>,
    /// Our published PublicInternet peer info timestamp
    pub own: Option<Timestamp>,
    /// PublicInternet online state changes so far
    pub online: u64,
}

impl LossEpoch {
    /// Whether both were judged on the same network: our peer info and online state, whose
    /// changes are never undone
    pub fn same_network(&self, other: &LossEpoch) -> bool {
        self.own == other.own && self.online == other.online
    }
}

impl RoutingTable {
    /// The state a routed question to a destination with `destination_ts` is lost in now
    pub(crate) fn loss_epoch(&self, destination_ts: Option<Timestamp>) -> LossEpoch {
        LossEpoch {
            destination: destination_ts,
            own: self
                .get_published_peer_info(RoutingDomain::PublicInternet)
                .map(|peer_info| peer_info.node_info().timestamp()),
            online: self
                .network_manager()
                .online_detector()
                .state_epoch(RoutingDomain::PublicInternet),
        }
    }
}
