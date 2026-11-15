use super::*;

/// The nodes known to hold one value at a sequence number
struct ValueHolders {
    /// The value held
    value: Arc<SignedValueData>,
    /// The nodes holding it
    nodes: HashSet<NodeId>,
    /// The holder closest to the record key
    closest: Option<NodeId>,
}

/// Which nodes hold which value at one sequence number, so a fanout can settle a split between
/// them: the value more nodes hold wins, and as many holders go to the one held closer to the key
pub(super) struct ValueTally {
    /// Holders of each value seen
    holders: Vec<ValueHolders>,
    /// Orders node ids by distance to the record key, closest first
    node_sort: FanoutQueueSort<'static>,
}

impl ValueTally {
    pub fn new(hash_coordinate: HashCoordinate) -> Self {
        Self {
            holders: Vec::new(),
            node_sort: Box::new(RoutingTable::make_closest_node_id_sort(hash_coordinate)),
        }
    }

    /// Record that `node_id` holds `value`
    pub fn add_holder(&mut self, value: &Arc<SignedValueData>, node_id: &NodeId) {
        self.add(value, node_id);
    }

    /// Record that `node_id` holds `rival`, and say whether it now beats `current`: more holders, or
    /// as many with a closer closest holder. A `current` that no node holds loses to any rival.
    pub fn add_rival_holder(
        &mut self,
        rival: &Arc<SignedValueData>,
        node_id: &NodeId,
        current: &SignedValueData,
    ) -> bool {
        let index = self.add(rival, node_id);
        self.beats(index, current)
    }

    /// Forget every holder, for a newer sequence number
    pub fn clear(&mut self) {
        self.holders.clear();
    }

    /// Whether some node holds a value other than `current`
    pub fn conflicted(&self, current: &SignedValueData) -> bool {
        self.holders
            .iter()
            .any(|holders| holders.value.value_data() != current.value_data())
    }

    /// Record that `node_id` holds `value`, returning the index of that value's holders
    fn add(&mut self, value: &Arc<SignedValueData>, node_id: &NodeId) -> usize {
        let index = match self.holders_of(value) {
            Some(index) => index,
            None => {
                self.holders.push(ValueHolders {
                    value: value.clone(),
                    nodes: HashSet::new(),
                    closest: None,
                });
                self.holders.len() - 1
            }
        };
        let holders = &mut self.holders[index];
        if holders.nodes.insert(node_id.clone())
            && holders
                .closest
                .as_ref()
                .is_none_or(|closest| (self.node_sort)(node_id, closest) == cmp::Ordering::Less)
        {
            holders.closest = Some(node_id.clone());
        }
        index
    }

    fn holders_of(&self, value: &SignedValueData) -> Option<usize> {
        self.holders
            .iter()
            .position(|holders| holders.value.value_data() == value.value_data())
    }

    /// Whether the holders at `index` beat `current`'s
    fn beats(&self, index: usize, current: &SignedValueData) -> bool {
        let rival = &self.holders[index];
        let Some(current) = self.holders_of(current).map(|i| &self.holders[i]) else {
            return true;
        };
        match rival.nodes.len().cmp(&current.nodes.len()) {
            cmp::Ordering::Greater => true,
            cmp::Ordering::Less => false,
            cmp::Ordering::Equal => match (&rival.closest, &current.closest) {
                (Some(rival_closest), Some(current_closest)) => {
                    (self.node_sort)(rival_closest, current_closest) == cmp::Ordering::Less
                }
                _ => false,
            },
        }
    }
}
