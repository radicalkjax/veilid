use super::*;

/// The parts of a bucket entry that are persisted: what a save writes, a load restores and a
/// snapshot carries
#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct BucketEntryPersisted {
    /// The node ids matching this entry
    pub node_ids: NodeIdGroup,
    /// When the peer was added to the routing table
    pub time_added: Timestamp,
    /// Whether the node updated its peer info since our network last changed
    pub updated_since_last_network_change: bool,
    /// The entry in the PublicInternet routing domain
    pub public_internet: BucketEntryRoutingDomain,
    /// The entry in the LocalNetwork routing domain
    pub local_network: BucketEntryRoutingDomain,
    /// API-visible statistics gathered for the peer
    pub peer_stats: PeerStats,
    /// Node-level information about RPCs used to calculate state
    pub rpc_stats: RPCStats,
    /// Statistics gathered per SequenceOrdering used to calculate state
    pub per_sequence_ordering_stats: BTreeMap<SequenceOrdering, RPCStats>,
    /// Per-transport reliability stats used to calculate state
    pub per_transport_stats: BTreeMap<TransportType, RPCStats>,
    /// Contact method failures cache for assisting with contact method selection
    #[serde(with = "serialize_hash_map_as_pairs")]
    pub contact_method_failures: HashMap<ContactMethod, Timestamp>,
    /// Our address as this peer last reported it, per routing domain and transport
    #[serde(with = "serialize_hash_map_as_pairs")]
    pub last_sender_info: HashMap<LastSenderInfoKey, SenderInfo>,
    /// The node-level state when this was taken
    pub state: BucketEntryState,
}

impl BucketEntryPersisted {
    /// The entry in `routing_domain`
    pub fn routing_domain(&self, routing_domain: RoutingDomain) -> &BucketEntryRoutingDomain {
        match routing_domain {
            RoutingDomain::PublicInternet => &self.public_internet,
            RoutingDomain::LocalNetwork => &self.local_network,
        }
    }
}

impl BucketEntryInner {
    /// The persisted parts of this entry, in `state`
    pub(in crate::routing_table) fn to_persisted(
        &self,
        state: BucketEntryState,
    ) -> BucketEntryPersisted {
        BucketEntryPersisted {
            node_ids: self.node_ids.clone(),
            time_added: self.time_added,
            updated_since_last_network_change: self.updated_since_last_network_change,
            public_internet: self.public_internet.clone(),
            local_network: self.local_network.clone(),
            peer_stats: self.peer_stats.clone(),
            rpc_stats: self.rpc_stats.clone(),
            per_sequence_ordering_stats: self.per_sequence_ordering_stats.clone(),
            per_transport_stats: self.per_transport_stats.clone(),
            contact_method_failures: self.contact_method_failures.clone(),
            last_sender_info: self.last_sender_info.clone(),
            state,
        }
    }

    /// Whether `persisted` holds what a save would write for this entry in `state`; stats, node
    /// status and contact failures alone do not count, a changed report of our address does
    pub(in crate::routing_table) fn matches_persisted(
        &self,
        persisted: &BucketEntryPersisted,
        state: BucketEntryState,
    ) -> bool {
        self.node_ids == persisted.node_ids
            && self.updated_since_last_network_change == persisted.updated_since_last_network_change
            && self
                .public_internet
                .matches_saved(&persisted.public_internet)
            && self.local_network.matches_saved(&persisted.local_network)
            && self.last_sender_info == persisted.last_sender_info
            && state == persisted.state
    }

    /// Rebuild an entry from its persisted parts; errors when they have no valid node id
    pub(in crate::routing_table) fn from_persisted(
        registry: VeilidComponentRegistry,
        network_manager: NetworkManagerHandle,
        persisted: &BucketEntryPersisted,
    ) -> EyreResult<Self> {
        let best_node_id = persisted
            .node_ids
            .iter()
            .find(|nid| VALID_CRYPTO_KINDS.contains(&nid.kind()))
            .cloned()
            .ok_or_else(|| eyre!("bucket entry has no valid node id"))?;

        let mut inner = Self::new(registry, network_manager, best_node_id);
        inner.node_ids = persisted.node_ids.clone();
        inner.time_added = persisted.time_added;
        inner.updated_since_last_network_change = persisted.updated_since_last_network_change;
        inner.public_internet = persisted.public_internet.clone();
        inner.local_network = persisted.local_network.clone();
        inner.set_envelope_support(
            [
                &persisted.public_internet.peer_info,
                &persisted.local_network.peer_info,
            ]
            .into_iter()
            .flatten()
            .flat_map(|pi| pi.node_info().envelope_support().iter().copied())
            .collect(),
        );
        inner.peer_stats = persisted.peer_stats.clone();
        inner.rpc_stats = persisted.rpc_stats.clone();
        inner.per_sequence_ordering_stats = persisted.per_sequence_ordering_stats.clone();
        inner.per_transport_stats = persisted.per_transport_stats.clone();
        inner.contact_method_failures = persisted.contact_method_failures.clone();
        inner.last_sender_info = persisted.last_sender_info.clone();
        Ok(inner)
    }
}
