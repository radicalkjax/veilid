//! A routing domain's last published state, kept so that an attach on a network that has not
//! changed publishes it again instead of detecting it.
//!
//! - Every publish replaces it, so it always holds the last peer info peers were given: peers
//!   take only a newer timestamp, so putting back an older one would leave them on the newer.
//! - The platform network says what the state rests on (`PlatformRestoreState::validity`) and
//!   decides at attach whether it still holds; each routing domain has its own conditions.
//! - It is discarded when the node asks to confirm its dial info again: the network no longer
//!   vouches for what was published.

use super::*;

impl_veilid_log_facility!("rtab");

/// The routing table TableDB key a routing domain's restore point is saved under
pub(crate) fn restore_point_key(routing_domain: RoutingDomain) -> Bytes {
    match routing_domain {
        RoutingDomain::PublicInternet => Bytes::from_static(b"public_internet_restore_point"),
        RoutingDomain::LocalNetwork => Bytes::from_static(b"local_network_restore_point"),
    }
}

/// What the platform network puts into a restore point
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct PlatformRestoreState {
    /// The low-level conditions the published state rests on; a restore needs them unchanged
    #[serde(with = "as_human_base64")]
    pub validity: Bytes,
    /// Platform state put back with the routing domain, such as port mappings
    #[serde(with = "as_human_base64")]
    pub state: Bytes,
}

/// A relay in a restore point
#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct RestorePointRelay {
    /// The relay's best node id
    pub relay_id: NodeId,
    /// What the relay does for us
    pub relay_kind: RelayKind,
    /// The relay's state when the peer info was published
    pub state: RoutingDomainRelayState,
}

/// A routing domain's published state and what it rests on
#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct RoutingDomainRestorePoint {
    /// What the platform network said this state rests on
    pub platform: PlatformRestoreState,
    /// Protocols, address types and capabilities
    pub network_config: RoutingDomainNetworkConfig,
    /// Our dial info
    pub dial_info_details: Vec<DialInfoDetail>,
    /// Our relays, in compilation order
    pub relays: Vec<RestorePointRelay>,
    /// The signed peer info we published
    #[serde(with = "signed_peer_info")]
    pub peer_info: Arc<PeerInfo>,
}

impl RoutingDomainRestorePoint {
    /// The restore point for `peer_info`, published from `rdd`
    pub fn new(
        platform: PlatformRestoreState,
        rdd: &dyn RoutingDomainDetail,
        peer_info: Arc<PeerInfo>,
    ) -> Self {
        let relays = rdd
            .relays()
            .into_iter()
            .filter_map(|relay| {
                rdd.relay_state(relay.relay_id.clone())
                    .map(|state| RestorePointRelay {
                        relay_id: relay.relay_id,
                        relay_kind: relay.relay_kind,
                        state,
                    })
            })
            .collect();
        Self {
            platform,
            network_config: rdd.network_config().clone(),
            dial_info_details: rdd.dial_info_details().clone(),
            relays,
            peer_info,
        }
    }
}

/// The saved relays compiled against the current routing table, with the states to give them.
/// A relay the table no longer holds, or that fails our requirements, is dropped and relay
/// management chooses another.
pub(super) fn compile_restored_relays(
    routing_table: &RoutingTable,
    routing_domain: RoutingDomain,
    relay_requirements: Arc<RelayRequirements>,
    relays: &[RestorePointRelay],
) -> (
    Option<RelayCompilation>,
    Vec<(NodeId, RoutingDomainRelayState)>,
) {
    if relays.is_empty() {
        return (None, vec![]);
    }
    let mut relay_compiler = relay_requirements.make_relay_compiler();
    let mut relay_states = Vec::new();
    for relay in relays {
        let Ok(Some(relay_node)) = routing_table.lookup_node_id(relay.relay_id.clone()) else {
            veilid_log!(routing_table debug "[{}] saved relay {} is not in the routing table", routing_domain, relay.relay_id);
            continue;
        };
        if !relay_compiler.apply_relay(RoutingDomainRelay::new(
            routing_domain,
            relay_node,
            relay.relay_kind,
        )) {
            veilid_log!(routing_table debug "[{}] saved relay {} no longer meets our relay requirements", routing_domain, relay.relay_id);
            continue;
        }
        // No flow to the relay survives an attach, so keep it alive at once
        relay_states.push((
            relay.relay_id.clone(),
            RoutingDomainRelayState {
                last_keepalive: Timestamp::default(),
                ..relay.state
            },
        ));
    }
    (relay_compiler.compile(), relay_states)
}
