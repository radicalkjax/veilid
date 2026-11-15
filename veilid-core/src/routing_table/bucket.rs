use super::*;

impl_veilid_log_facility!("rtab");

/// Routing Table Bucket
/// Stores map of public keys to entries, which may be in multiple routing tables per crypto kind
/// Keeps entries at a particular 'hash coordinate distance' from this cryptokind's node id
/// Helps to keep managed lists at particular distances so we can evict nodes by priority
/// where the priority comes from liveness and age of the entry (older is better)
pub struct Bucket {
    /// Component registry accessor
    registry: VeilidComponentRegistry,
    /// Handed to every new entry for lock-free access to the online detector
    network_manager: NetworkManagerHandle,
    /// Map of keys to entries for this bucket
    entries: BTreeMap<BareNodeId, Arc<BucketEntry>>,
    /// The crypto kind in use for the public keys in this bucket
    kind: CryptoKind,
}
pub(super) type EntriesIter<'a> =
    alloc::collections::btree_map::Iter<'a, BareNodeId, Arc<BucketEntry>>;

impl_veilid_component_accessors!(Bucket);

impl Bucket {
    pub fn new(
        registry: VeilidComponentRegistry,
        network_manager: NetworkManagerHandle,
        kind: CryptoKind,
    ) -> Self {
        Self {
            registry,
            network_manager,
            entries: BTreeMap::new(),
            kind,
        }
    }

    /// Create a new entry with a node_id of this crypto kind and return it
    pub(super) fn add_new_entry(&mut self, node_id_key: BareNodeId) -> Arc<BucketEntry> {
        veilid_log!(self trace "Node added: {}:{}", self.kind, node_id_key);

        // Add new entry
        let entry = Arc::new(BucketEntry::new(
            self.registry(),
            self.network_manager.clone(),
            NodeId::new(self.kind, node_id_key.clone()),
        ));
        self.entries.insert(node_id_key, entry.clone());

        // Return the new entry
        entry
    }

    /// Add an existing entry with a new node_id for this crypto kind
    pub(super) fn add_existing_entry(&mut self, node_id_key: BareNodeId, entry: Arc<BucketEntry>) {
        veilid_log!(self trace "Existing node added: {}:{}", self.kind, node_id_key);

        // Add existing entry
        self.entries.insert(node_id_key, entry);
    }

    /// Remove an entry with a node_id for this crypto kind from the bucket
    pub(super) fn remove_entry(&mut self, node_id_key: &BareNodeId) {
        veilid_log!(self trace "Node removed: {}:{}", self.kind, node_id_key);

        // Remove the entry
        self.entries.remove(node_id_key);
    }

    pub(super) fn entry(&self, key: &BareNodeId) -> Option<Arc<BucketEntry>> {
        self.entries.get(key).cloned()
    }

    pub(super) fn entries(&self) -> EntriesIter<'_> {
        self.entries.iter()
    }

    pub(super) fn len(&self) -> usize {
        self.entries.len()
    }

    pub(super) fn kick(
        &mut self,
        bucket_depth: usize,
        exempt_peers: &BTreeSet<BareNodeId>,
    ) -> Option<BTreeSet<BareNodeId>> {
        // Get number of entries to attempt to purge from bucket
        let bucket_len = self.entries.len();

        // Don't bother kicking bucket unless it is full
        if bucket_len <= bucket_depth {
            return None;
        }

        // Try to purge the newest entries that overflow the bucket
        let mut dead_node_ids: BTreeSet<BareNodeId> = BTreeSet::new();
        let mut extra_entries = bucket_len - bucket_depth;

        // Get the sorted list of kickable entries by their kick order
        // Pre-snapshot mutable fields to avoid total-ordering violations from
        // concurrent updates between comparisons (Rust 1.81+ driftsort)
        let cur_ts = Timestamp::now();
        let mut sorted_entries: Vec<(BareNodeId, Arc<BucketEntry>, BucketEntryState, Timestamp)> =
            self.entries
                .iter()
                .filter(|(k, v)| {
                    // Skip entries with active NodeRef references
                    if v.ref_count.load(Ordering::Acquire) > 0 {
                        return false;
                    }
                    // Skip exempt entries
                    if exempt_peers.contains(k) {
                        return false;
                    }
                    true
                })
                .map(|(k, v)| {
                    let (state, time_added) = v.with(|e| (e.state(cur_ts), e.time_added()));
                    (k.clone(), v.clone(), state, time_added)
                })
                .collect();
        sorted_entries.sort_unstable_by(|a, b| -> core::cmp::Ordering {
            // If node ids are equal, they are equal
            if a.0 == b.0 {
                return core::cmp::Ordering::Equal;
            }
            // first kick punished nodes, then dead nodes, then missing nodes,
            // then initial nodes, then unreliable nodes, then reliable nodes
            let st_cmp = a.2.cmp(&b.2);
            if st_cmp != core::cmp::Ordering::Equal {
                return st_cmp;
            }
            // then kick by time added, most recent nodes are kicked first
            b.3.cmp(&a.3)
        });

        for (bare_node_id, _, _, _) in sorted_entries {
            // If we're not evicting more entries, exit
            if extra_entries == 0 {
                break;
            }
            extra_entries -= 1;

            // No references; evict it from this bucket. The entry's node ids are left
            // intact — eviction is by bucket key (remove_entry below), and an entry must
            // always retain a best node id.
            dead_node_ids.insert(bare_node_id);
        }

        // Now purge the dead node ids
        for id in &dead_node_ids {
            // Remove the entry
            // The entry may not be completely gone after this happens
            // because it may still be in another bucket for a different CryptoKind
            self.remove_entry(id);
        }

        if !dead_node_ids.is_empty() {
            Some(dead_node_ids)
        } else {
            None
        }
    }
}
