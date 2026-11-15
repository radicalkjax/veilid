use super::*;

/// The table store key an entry is persisted under: its best node id
pub(crate) fn persisted_entry_key(best_node_id: &NodeId) -> Bytes {
    Bytes::from(best_node_id.to_string())
}

/// The entries the last committed save left in the table store, so the next save writes only
/// what changed
#[derive(Debug, Default)]
pub(crate) struct PersistedEntries {
    /// Each persisted entry, by the best node id it is keyed under
    entries: HashMap<NodeId, Arc<BucketEntryPersisted>>,
    /// Keys of entries in the table store that did not load; the next save deletes them
    unloaded: Vec<Bytes>,
}

/// The writes that bring the persisted entries up to date
pub(crate) struct PersistedEntryChanges {
    /// Best node ids of the entries to store
    pub written: Vec<NodeId>,
    /// Keys of the entries to delete
    pub deleted: Vec<Bytes>,
    /// The persisted entries once the changes commit
    pub entries: PersistedEntries,
}

/// What one save wrote
#[derive(Debug, Clone, Copy)]
pub(crate) struct SaveCounts {
    /// Entries the saved table holds
    pub entries: usize,
    /// Entries written
    pub written: usize,
    /// Entries deleted
    pub deleted: usize,
    /// Routing domain states written
    pub routing_domain_states: usize,
}

impl PersistedEntries {
    /// Record an entry read at load under its key
    pub fn insert(&mut self, key: Bytes, persisted: BucketEntryPersisted) {
        let node_id = persisted
            .node_ids
            .iter()
            .find(|nid| VALID_CRYPTO_KINDS.contains(&nid.kind()))
            .cloned();
        match node_id {
            Some(node_id) if persisted_entry_key(&node_id) == key => {
                self.entries.insert(node_id, Arc::new(persisted));
            }
            _ => self.unloaded.push(key),
        }
    }

    /// Record an entry in the table store that did not load
    pub fn insert_unloaded(&mut self, key: Bytes) {
        self.unloaded.push(key);
    }

    /// The persisted entry keyed under `node_id`
    pub fn get(&self, node_id: &NodeId) -> Option<&Arc<BucketEntryPersisted>> {
        self.entries.get(node_id)
    }

    /// Number of persisted entries
    pub fn entry_count(&self) -> usize {
        self.entries.len()
    }

    /// The writes that make the persisted entries match `entries`
    pub fn changes(
        &self,
        entries: &[Arc<BucketEntry>],
        cur_ts: Timestamp,
    ) -> PersistedEntryChanges {
        let mut next = HashMap::with_capacity(entries.len());
        let mut written = Vec::new();
        for entry in entries {
            let Some((node_id, persisted, changed)) = entry.with(|e| {
                if e.public_internet.peer_info.is_none() && e.local_network.peer_info.is_none() {
                    return None;
                }
                let node_id = e.best_node_id();
                let state = e.computed_state(cur_ts);
                Some(match self.entries.get(&node_id) {
                    Some(saved) if e.matches_persisted(saved, state) => {
                        (node_id, saved.clone(), false)
                    }
                    _ => (node_id, Arc::new(e.to_persisted(state)), true),
                })
            }) else {
                continue;
            };
            if changed {
                written.push(node_id.clone());
            }
            next.insert(node_id, persisted);
        }
        let deleted = self
            .entries
            .keys()
            .filter(|node_id| !next.contains_key(*node_id))
            .map(persisted_entry_key)
            .chain(self.unloaded.iter().cloned())
            .collect();
        PersistedEntryChanges {
            written,
            deleted,
            entries: PersistedEntries {
                entries: next,
                unloaded: Vec::new(),
            },
        }
    }
}
