mod app_messaging;
mod attach;
mod debug;
mod faults;
mod protected_store;
mod records;
mod table_store;
mod transactions;
mod watches;

pub use table_store::{TableDB, TableDBTransaction};

pub(crate) use app_messaging::*;
pub(crate) use attach::*;
pub(crate) use faults::*;
pub(crate) use protected_store::*;
pub(crate) use records::*;
pub(crate) use table_store::MockTableStore;
pub(crate) use transactions::{mock_drop_transaction, OutboundTransactionHandle};
pub(crate) use watches::*;

use super::*;

impl_veilid_log_facility!("mock");

/// Identifies one simulated node within the process-global mock network.
///
/// The log key is already derived from program name plus namespace, which `api_startup` requires
/// to be unique per concurrent instance, so it identifies an instance without inventing a second
/// identity scheme.
#[derive(Copy, Clone, PartialEq, Eq, Hash, Debug)]
pub(crate) struct MockNodeKey(VeilidLogKey);

impl MockNodeKey {
    pub fn from_registry(registry: &VeilidComponentRegistry) -> Self {
        Self(registry.log_key())
    }
}

impl fmt::Display for MockNodeKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// Derive a simulated node's identity, one id per supported crypto kind.
///
/// Uses the same derivation as the routing table: a node id is the leading bytes of the hash of a
/// public key, so ids are indistinguishable in shape from real ones.
pub(crate) fn generate_node_ids(crypto: &Crypto) -> Vec<NodeId> {
    let mut node_ids = Vec::new();
    for kind in VALID_CRYPTO_KINDS {
        let Some(vcrypto) = crypto.get(kind) else {
            continue;
        };
        let keypair = vcrypto.generate_keypair();
        let idhash = vcrypto.generate_hash(keypair.key().ref_value());
        if idhash.ref_value().len() < HASH_COORDINATE_LENGTH {
            continue;
        }
        node_ids.push(NodeId::new(
            kind,
            BareNodeId::new(&idhash.ref_value()[0..HASH_COORDINATE_LENGTH]),
        ));
    }
    node_ids
}

/// Validate a route id, matching the routing table's checks.
pub(crate) fn check_route_id(crypto: &Crypto, route_id: &RouteId) -> VeilidAPIResult<()> {
    let Some(vcrypto) = crypto.get(route_id.kind()) else {
        apibail_invalid_argument!("unsupported crypto kind", "route_id", route_id);
    };
    if route_id.ref_value().len() != vcrypto.hash_digest_length() {
        apibail_invalid_argument!("invalid route id length", "route_id", route_id);
    }
    Ok(())
}

/// Validate a node id, matching the routing table's checks.
pub(crate) fn check_node_id(crypto: &Crypto, node_id: &NodeId) -> VeilidAPIResult<()> {
    if crypto.get(node_id.kind()).is_none() {
        apibail_invalid_argument!("unsupported crypto kind", "node_id", node_id);
    }
    if node_id.ref_value().len() != HASH_COORDINATE_LENGTH {
        apibail_invalid_argument!("invalid node id length", "node_id", node_id);
    }
    Ok(())
}

/// One simulated node's private state.
///
/// The local store is kept apart from the shared DHT so a report's local and network sequence
/// numbers can differ; it survives close and reopen as the real store does, and only delete or a
/// purge drops it.
pub(crate) struct MockNode {
    /// This node's ids, one per crypto kind
    pub node_ids: Vec<NodeId>,
    /// Records this node has open
    pub opened_records: HashMap<OpaqueRecordKey, MockOpenedRecord>,
    /// Local copy of each record's values, lagging the shared DHT until a get or set refreshes it
    pub local_records: HashMap<OpaqueRecordKey, HashMap<ValueSubkey, ValueData>>,
    /// Owner and schema of every record in the local store, so a reopen needs no network
    pub local_descriptors: HashMap<OpaqueRecordKey, (PublicKey, DHTSchema)>,
    /// Writes queued while detached, outside [MockOpenedRecord] because the queue outlives a close
    pub offline_writes: HashMap<OpaqueRecordKey, HashMap<ValueSubkey, ValueData>>,
    /// Simulated attachment state
    pub attachment: MockAttachment,
    /// In-memory table store
    pub tables: MockTableStore,
    /// In-memory protected store
    pub secrets: MockProtectedStore,
    /// Where this node's updates are delivered
    pub update_callback: UpdateCallback,
}

/// State shared by every simulated node in the process.
#[derive(Default)]
struct MockNetworkInner {
    nodes: HashMap<MockNodeKey, Arc<Mutex<MockNode>>>,
}

static MOCK_NETWORK: LazyLock<Mutex<MockNetworkInner>> =
    LazyLock::new(|| Mutex::new(MockNetworkInner::default()));

/// The process-global simulated network.
///
/// Every operation takes the global lock only long enough to clone out the node handle it needs;
/// nothing is held across an await, and no update callback is ever invoked under a lock.
pub(crate) struct MockNetwork;

impl MockNetwork {
    /// Add a node for a starting API instance.
    pub fn register(registry: &VeilidComponentRegistry, node_ids: Vec<NodeId>) -> MockNodeKey {
        let key = MockNodeKey::from_registry(registry);
        let node = MockNode {
            node_ids,
            opened_records: HashMap::new(),
            local_records: HashMap::new(),
            local_descriptors: HashMap::new(),
            offline_writes: HashMap::new(),
            attachment: MockAttachment::default(),
            tables: MockTableStore::default(),
            secrets: MockProtectedStore::default(),
            update_callback: registry.update_callback(),
        };
        MOCK_NETWORK
            .lock()
            .nodes
            .insert(key, Arc::new(Mutex::new(node)));
        key
    }

    /// Drop a node when its API instance shuts down.
    pub fn unregister(registry: &VeilidComponentRegistry) {
        MOCK_NETWORK
            .lock()
            .nodes
            .remove(&MockNodeKey::from_registry(registry));
    }

    /// Get a node handle, or `NotInitialized` if its instance is gone.
    pub fn node(key: &MockNodeKey) -> VeilidAPIResult<Arc<Mutex<MockNode>>> {
        MOCK_NETWORK
            .lock()
            .nodes
            .get(key)
            .cloned()
            .ok_or(VeilidAPIError::NotInitialized)
    }

    /// Run `f` against a node's state, releasing both locks before returning.
    pub fn with_node<R>(
        key: &MockNodeKey,
        f: impl FnOnce(&mut MockNode) -> R,
    ) -> VeilidAPIResult<R> {
        let node = Self::node(key)?;
        let mut node = node.lock();
        Ok(f(&mut node))
    }

    /// Find the node holding a given node id.
    pub fn find_node_by_id(node_id: &NodeId) -> Option<MockNodeKey> {
        let nodes: Vec<(MockNodeKey, Arc<Mutex<MockNode>>)> = MOCK_NETWORK
            .lock()
            .nodes
            .iter()
            .map(|(k, v)| (*k, v.clone()))
            .collect();
        nodes
            .into_iter()
            .find(|(_, n)| n.lock().node_ids.contains(node_id))
            .map(|(k, _)| k)
    }

    /// Deliver an update to a node's application.
    ///
    /// The callback runs with no lock held: it re-enters the API on some paths, and an application
    /// is free to block in it.
    pub fn send_update(key: &MockNodeKey, update: VeilidUpdate) -> VeilidAPIResult<()> {
        let update_callback = Self::with_node(key, |node| node.update_callback.clone())?;
        update_callback(update);
        Ok(())
    }
}
