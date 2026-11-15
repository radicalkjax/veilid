//! Fanout Queue
//! Keep a deque of unique nodes
//! Internally the 'front' of the list is the next out, and new nodes are added to the 'back' of the list
//! When passing in a 'cleanup' function, if it sorts the queue, the 'first' items in the queue are the 'next' out.

use super::*;

impl_veilid_log_facility!("fanout");
impl_veilid_component_accessors!(FanoutQueue<'_>);

/// Multiple of the slowest answer that a straggler unable to complete consensus is waited for,
/// when that is longer than the slow-node threshold
const STRAGGLER_RTT_FACTOR: u64 = 3;

/// The stage of a particular node we fanned out to
#[derive(Debug, Copy, Clone)]
pub enum FanoutNodeStage {
    /// Node that needs processing
    Queued,
    /// Node currently being processed
    InProgress,
    /// Node that timed out during processing
    Timeout,
    /// Node that rejected the query
    Rejected,
    /// Node that accepted the query with a current result
    Accepted,
    /// Node that accepted the query but had an older result
    Stale,
    /// Node that accepted the query but holds a result that conflicts with ours
    Conflict,
    /// Node that has been disqualified for being too far away from the key or are acting badly
    Disqualified,
}

/// The state of a particular node we fanned out to including the stage
/// and a linked list of transitions and their timestamps when they transitioned
#[derive(Debug, Clone)]
pub struct FanoutNodeStatus {
    stage: FanoutNodeStage,
    prev_status: Option<Box<FanoutNodeStatus>>,
    transition_ts: Timestamp,
    touch_ts: Timestamp,
}

impl FanoutNodeStatus {
    pub fn stage(&self) -> FanoutNodeStage {
        self.stage
    }

    pub fn is_finished(&self) -> bool {
        match self.stage {
            FanoutNodeStage::Queued | FanoutNodeStage::InProgress => false,
            FanoutNodeStage::Timeout
            | FanoutNodeStage::Rejected
            | FanoutNodeStage::Disqualified
            | FanoutNodeStage::Accepted
            | FanoutNodeStage::Stale
            | FanoutNodeStage::Conflict => true,
        }
    }

    pub fn queued(timestamp: Timestamp) -> Self {
        FanoutNodeStatus {
            stage: FanoutNodeStage::Queued,
            prev_status: None,
            transition_ts: timestamp,
            touch_ts: timestamp,
        }
    }

    pub fn transition(&mut self, stage: FanoutNodeStage, timestamp: Timestamp) {
        self.touch_ts = timestamp;

        let prev_status = Box::new(self.clone());
        self.stage = stage;
        self.prev_status = Some(prev_status);
        self.transition_ts = timestamp;
        self.touch_ts = timestamp;
    }

    /// How long the node took to answer, if its current stage is an answer to its last request
    pub fn answer_duration(&self) -> Option<TimestampDuration> {
        if !matches!(
            self.stage,
            FanoutNodeStage::Accepted
                | FanoutNodeStage::Stale
                | FanoutNodeStage::Conflict
                | FanoutNodeStage::Rejected
        ) {
            return None;
        }
        let prev_status = self.prev_status.as_ref()?;
        matches!(prev_status.stage, FanoutNodeStage::InProgress)
            .then(|| self.transition_ts.duration_since(prev_status.transition_ts))
    }

    pub fn touch(&mut self, timestamp: Timestamp) {
        match self.stage {
            FanoutNodeStage::Queued | FanoutNodeStage::InProgress => {
                self.touch_ts = timestamp;
            }
            FanoutNodeStage::Timeout
            | FanoutNodeStage::Rejected
            | FanoutNodeStage::Accepted
            | FanoutNodeStage::Stale
            | FanoutNodeStage::Conflict
            | FanoutNodeStage::Disqualified => {
                // Don't touch these because they are not considered 'active'
            }
        }
    }
}

impl fmt::Display for FanoutNodeStatus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{:?}{}{}",
            self.stage,
            if self.transition_ts == self.touch_ts {
                "".to_string()
            } else {
                format!("({})", self.touch_ts.duration_since(self.transition_ts))
            },
            if let Some(prev_status) = &self.prev_status {
                format!("<--{}", prev_status)
            } else {
                format!("@{}", self.transition_ts)
            }
        )
    }
}

#[derive(Debug)]
pub struct FanoutNode {
    pub work_item: FanoutWorkItem,
    pub lane_stop_source: Option<StopSource>,
    pub status: FanoutNodeStatus,
    pub demoted: bool,
}

impl fmt::Display for FanoutNode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut start_status = &self.status;
        while let Some(prev_status) = &start_status.prev_status {
            start_status = prev_status;
        }
        let total_duration = self
            .status
            .touch_ts
            .duration_since(start_status.transition_ts);
        write!(
            f,
            "{}{}: {}",
            self.work_item.node_ref,
            if total_duration.is_zero() {
                "".to_string()
            } else {
                format!(" ({})", total_duration)
            },
            self.status,
        )
    }
}

pub type FanoutQueueSort<'a> = Box<dyn Fn(&NodeId, &NodeId) -> core::cmp::Ordering + Send + 'a>;

#[derive(Debug)]
struct FanoutWorkRequest {
    request_ts: Timestamp,
    lane_name: String,
    work_sender: FanoutWorkSender,
}

impl FanoutWorkRequest {
    fn new(lane_name: String, work_sender: FanoutWorkSender) -> Self {
        Self {
            request_ts: Timestamp::now_non_decreasing(),
            lane_name,
            work_sender,
        }
    }

    pub fn request_ts(&self) -> Timestamp {
        self.request_ts
    }

    pub fn lane_name(&self) -> String {
        self.lane_name.clone()
    }

    pub fn into_work_sender(self) -> FanoutWorkSender {
        self.work_sender
    }
}

#[derive(Debug, Clone)]
pub struct FanoutWorkItem {
    pub node_ref: NodeRef,
    pub work_item_stop_token: StopToken,
}

pub type FanoutWorkReceiver = flume::Receiver<FanoutWorkItem>;
pub type FanoutWorkSender = flume::Sender<FanoutWorkItem>;

pub struct FanoutQueue<'a> {
    /// Name for debugging
    name: String,
    /// Link back to veilid component registry for logging
    registry: VeilidComponentRegistry,
    /// Crypto kind in use for this queue
    crypto_kind: CryptoKind,
    /// The status of all the nodes we have added so far
    nodes: HashMap<NodeId, FanoutNode>,
    /// Closer nodes to the record key are at the front of the list
    sorted_nodes: Vec<NodeId>,
    /// The sort function to use for the nodes
    node_sort: FanoutQueueSort<'a>,
    /// Nodes to ask ahead of the sort, if any (see `FanoutCall::set_prefer_init_nodes`)
    preferred_nodes: HashSet<NodeId>,
    /// Ask nodes unresponsive to routed questions after their window (see `FanoutCall::set_demote_routed_unresponsive`)
    demote_routed_unresponsive: bool,
    /// Lanes' requests for work, queued by `request_work`
    work_request_sender: flume::Sender<FanoutWorkRequest>,
    /// Requests waiting for a node, served by `send_more_work`
    work_request_receiver: flume::Receiver<FanoutWorkRequest>,
    /// Wakes the run loop's pump whenever a lane asks for work, which follows each of its results
    pump_wake_sender: WakeSender,
    /// What the run loop's pump waits on
    pump_wake_receiver: WakeReceiver,
    /// Consensus count to use
    consensus_count: usize,
    /// Accepts to keep dispatching toward once consensus is met (>= consensus_count)
    collection_count: usize,
    /// Number of nodes away from the key that can be considered for fanout operations
    consensus_width: usize,
    /// How long a node may be in flight before a throttled queue counts it as slow
    opt_throttle_duration: Option<TimestampDuration>,
}

impl fmt::Debug for FanoutQueue<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("FanoutQueue")
            .field("crypto_kind", &self.crypto_kind)
            .field("nodes", &self.nodes)
            .field("sorted_nodes", &self.sorted_nodes)
            // .field("node_sort", &self.node_sort)
            .field("work_request_sender", &self.work_request_sender)
            .field("work_request_receiver", &self.work_request_receiver)
            .field("consensus_count", &self.consensus_count)
            .field("collection_count", &self.collection_count)
            .field("consensus_width", &self.consensus_width)
            .field("opt_throttle_duration", &self.opt_throttle_duration)
            .finish()
    }
}

impl fmt::Display for FanoutQueue<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "nodes:\n{}",
            self.sorted_nodes
                .iter()
                .map(|x| {
                    let node = self.nodes.get(x).unwrap_or_log();
                    format!(
                        "    {}:{}{} {}",
                        x,
                        if self.preferred_nodes.contains(x) {
                            " [preferred]"
                        } else {
                            ""
                        },
                        if node.demoted { " [demoted]" } else { "" },
                        node.status
                    )
                })
                .collect::<Vec<_>>()
                .join("\n")
        )
    }
}

impl<'a> FanoutQueue<'a> {
    /// Create a queue for fanout candidates that have a crypto-kind compatible node id
    #[expect(clippy::too_many_arguments)]
    pub fn new(
        name: String,
        registry: VeilidComponentRegistry,
        crypto_kind: CryptoKind,
        node_sort: FanoutQueueSort<'a>,
        consensus_count: usize,
        collection_count: usize,
        consensus_width: usize,
        opt_throttle_duration: Option<TimestampDuration>,
    ) -> Self {
        let (sender, receiver) = flume::unbounded();
        let (pump_wake_sender, pump_wake_receiver) = wakeup();
        Self {
            name,
            registry,
            crypto_kind,
            nodes: HashMap::new(),
            sorted_nodes: Vec::new(),
            node_sort,
            preferred_nodes: HashSet::new(),
            demote_routed_unresponsive: false,
            work_request_sender: sender,
            work_request_receiver: receiver,
            pump_wake_sender,
            pump_wake_receiver,
            consensus_count,
            collection_count: collection_count.max(consensus_count),
            consensus_width,
            opt_throttle_duration,
        }
    }

    /// Ask for more work when some is ready
    /// When work is ready it will be sent to work_sender so it can be received
    /// by the worker
    pub fn request_work(&mut self, lane_name: String) -> Result<FanoutWorkReceiver, RPCError> {
        let (work_sender, work_receiver) = flume::bounded(1);

        let work_request = FanoutWorkRequest::new(lane_name, work_sender);
        let request_ts = work_request.request_ts();

        self.work_request_sender
            .send(work_request)
            .map_err(RPCError::internal)?;

        // Send whatever work is available immediately
        self.send_more_work(request_ts);
        self.pump_wake_sender.wake();

        Ok(work_receiver)
    }

    /// What the run loop's pump waits on, besides the next straggler cut
    pub fn pump_wake_receiver(&self) -> WakeReceiver {
        self.pump_wake_receiver.clone()
    }

    /// Ask these nodes ahead of the sort order, for an operation that only a node
    /// already holding the record can satisfy
    pub fn set_preferred_nodes(&mut self, nodes: &[NodeRef]) {
        self.preferred_nodes = nodes
            .iter()
            .filter_map(|node_ref| node_ref.node_ids().get(self.crypto_kind))
            .collect();
    }

    /// Ask nodes that lost a routed question after the rest of their window or reserve, for a
    /// fanout whose questions travel over safety routes; they keep their place in the window
    pub fn set_demote_routed_unresponsive(&mut self, demote_routed_unresponsive: bool) {
        self.demote_routed_unresponsive = demote_routed_unresponsive;
    }

    /// Update the queue with changes, adding new nodes to a filtered and sorted
    /// list of fanout candidates and disqualifying nodes no longer needed
    pub fn update(&mut self, new_nodes: &[NodeRef], cur_ts: Timestamp) {
        // The network side of the state losses are judged in, read once
        let opt_epoch = self
            .demote_routed_unresponsive
            .then(|| self.registry().routing_table().loss_epoch(None));

        for node_ref in new_nodes {
            // Ensure the node has a comparable key with our current crypto kind
            let Some(key) = node_ref.node_ids().get(self.crypto_kind) else {
                continue;
            };
            // Check if we have already seen this node before (only one call per node ever)
            if self.nodes.contains_key(&key) {
                continue;
            }

            // Create a cancellation token for the node,
            // so we can stop processing the fanout lane immediately if needed
            let lane_stop_source = StopSource::new();
            let work_item_stop_token = lane_stop_source.token();

            // Judged once, so the dispatch order stays put for the rest of the run
            let demoted = opt_epoch.is_some_and(|epoch| {
                let epoch = LossEpoch {
                    destination: Some(node_ref.node_info_ts(RoutingDomain::PublicInternet)),
                    ..epoch
                };
                node_ref.operate(|e| e.is_routed_unresponsive(&epoch))
            });

            // Add the new node
            self.nodes.insert(
                key.clone(),
                FanoutNode {
                    work_item: FanoutWorkItem {
                        node_ref: node_ref.clone(),
                        work_item_stop_token,
                    },
                    lane_stop_source: Some(lane_stop_source),
                    status: FanoutNodeStatus::queued(cur_ts),
                    demoted,
                },
            );
            self.sorted_nodes.push(key);
        }

        // Sort the node list
        self.sorted_nodes.sort_unstable_by(&self.node_sort);

        // sort_by_key is stable, so each group keeps the node sort's order within it
        if !self.preferred_nodes.is_empty() {
            let preferred_nodes = &self.preferred_nodes;
            self.sorted_nodes
                .sort_by_key(|node_id| !preferred_nodes.contains(node_id));
        }
        // Disqualify any nodes that can be
        self.disqualify(cur_ts);

        // Touch all node status
        for node in &self.sorted_nodes {
            self.nodes
                .get_mut(node)
                .unwrap_or_log()
                .status
                .touch(cur_ts);
        }

        veilid_log!(self debug
            "{}: FanoutQueue::update:\n{}{}\n",
            self.name,
            if new_nodes.is_empty() {
                "".to_string()
            } else {
                format!("new_nodes:{}\n",
                    new_nodes.iter().map(|x| format!("\n    {}", x))
                        .collect::<Vec<String>>()
                        .join(","))
            },
            self.to_string()
        );
    }

    /// The window order with each demoted node still to be asked moved behind the other
    /// candidates of its group, window or reserve: asked last there, never skipped
    fn dispatch_order(&self) -> Vec<NodeId> {
        let mut window_slots = 0usize;
        let mut keyed: Vec<((bool, bool), NodeId)> = Vec::with_capacity(self.sorted_nodes.len());
        for node_id in &self.sorted_nodes {
            let node = self.nodes.get(node_id).unwrap_or_log();
            if !matches!(
                node.status.stage,
                FanoutNodeStage::Timeout | FanoutNodeStage::Disqualified
            ) {
                window_slots += 1;
            }
            let reserve = self.consensus_width > 0 && window_slots > self.consensus_width;
            let deferred = node.demoted && matches!(node.status.stage, FanoutNodeStage::Queued);
            keyed.push(((reserve, deferred), node_id.clone()));
        }
        keyed.sort_by_key(|(key, _)| *key);
        keyed.into_iter().map(|(_, node_id)| node_id).collect()
    }

    /// Send next fanout candidates if available to whatever workers are ready
    pub fn send_more_work(&mut self, cur_ts: Timestamp) {
        // Get the next work and send it along
        let registry = self.registry();
        let mut working_toward_consensus = 0usize;
        let mut counting_consensus = true;

        let mut slow_nodes = Vec::<NodeRef>::new();

        for x in &self.dispatch_order() {
            // If there are no work receivers left then we should stop trying to send
            if self.work_request_receiver.is_empty() {
                break;
            }

            // If we have enough workers to reach the collection target don't send more
            // work beyond that unless we are unthrottled
            let mut throttle_unlock = false;
            if self.opt_throttle_duration.is_some() {
                if working_toward_consensus >= (self.collection_count + slow_nodes.len()) {
                    break;
                } else if !slow_nodes.is_empty() {
                    throttle_unlock = true;
                }
            }

            // Get the queue entry and handle it appropriately
            let node = self.nodes.get_mut(x).unwrap_or_log();
            match node.status.stage {
                FanoutNodeStage::Queued => {
                    // Consensus counting stops at a queued node
                    counting_consensus = false;

                    // Send node to a work request
                    while let Ok(work_request) = self.work_request_receiver.try_recv() {
                        if throttle_unlock {
                            veilid_log!(registry debug "{}: Throttle unlock due to {} slow nodes", self.name, slow_nodes.len());
                        }
                        let lane_name = work_request.lane_name();
                        let request_ts = work_request.request_ts();
                        let work_sender = work_request.into_work_sender();

                        let work_item = node.work_item.clone();
                        if work_sender.send(work_item).is_ok() {
                            // Queued -> InProgress
                            node.status.transition(FanoutNodeStage::InProgress, cur_ts);
                            veilid_log!(registry debug "{}: Queue sent more work {} after request to {} => {}", self.name, cur_ts.duration_since(request_ts), lane_name, node.work_item.node_ref);
                            break;
                        }
                    }
                }
                FanoutNodeStage::InProgress => {
                    // If something has been in progress for longer than 1/3 of the total timeout
                    // then we should send more work to start looking at another node
                    if let Some(throttle_duration) = self.opt_throttle_duration {
                        let stage_duration = cur_ts.duration_since(node.status.transition_ts);
                        if stage_duration > throttle_duration {
                            slow_nodes.push(node.work_item.node_ref.clone());
                        } else {
                            // If we would like this node to finish before we allow consensus
                            // then we have to stop count here until it has reached the throttle duration
                            counting_consensus = false;
                        }
                    }
                    if counting_consensus {
                        working_toward_consensus += 1;
                    }
                }
                FanoutNodeStage::Accepted => {
                    // Accepted nodes are done and count toward the collection target
                    if counting_consensus {
                        working_toward_consensus += 1;
                    }

                    // If this node was a slow node, remove it from the slow node list since we finished it
                    slow_nodes.retain(|x| !x.same_entry(&node.work_item.node_ref));
                }
                FanoutNodeStage::Timeout
                | FanoutNodeStage::Rejected
                | FanoutNodeStage::Stale
                | FanoutNodeStage::Conflict
                | FanoutNodeStage::Disqualified => {
                    // Does not count toward consensus or stop counting it

                    // If this node was a slow node, remove it from the slow node list since we finished it
                    slow_nodes.retain(|x| !x.same_entry(&node.work_item.node_ref));
                }
            }
        }
    }

    /// Moves an answered node from InProgress to `stage`; false if its answer no longer applies
    fn transition_answered(
        &mut self,
        node_ref: NodeRef,
        stage: FanoutNodeStage,
        cur_ts: Timestamp,
    ) -> bool {
        let key = node_ref.node_ids().get(self.crypto_kind).unwrap_or_log();
        match self.nodes.get(&key).unwrap_or_log().status.stage {
            FanoutNodeStage::InProgress => {}
            // A rejection consensus disqualifies in-flight nodes, whose answers can land after it
            FanoutNodeStage::Disqualified => {
                veilid_log!(self debug "fanout: {:?} answer from {} arrived after the node was disqualified", stage, key);
                return false;
            }
            current => {
                veilid_log!(self error "fanout: {:?} answer from {} while the node was {:?}, not in progress", stage, key, current);
                return false;
            }
        }
        self.nodes
            .get_mut(&key)
            .unwrap_or_log()
            .status
            .transition(stage, cur_ts);
        true
    }

    /// Transition node InProgress -> Timeout
    pub fn timeout(&mut self, node_ref: NodeRef, cur_ts: Timestamp) {
        self.transition_answered(node_ref, FanoutNodeStage::Timeout, cur_ts);
    }

    /// Transition node InProgress -> Rejected
    pub fn rejected(&mut self, node_ref: NodeRef, cur_ts: Timestamp) {
        if self.transition_answered(node_ref, FanoutNodeStage::Rejected, cur_ts) {
            self.disqualify(cur_ts);
        }
    }

    /// Transition node InProgress -> Accepted
    pub fn accepted(&mut self, node_ref: NodeRef, cur_ts: Timestamp) {
        self.transition_answered(node_ref, FanoutNodeStage::Accepted, cur_ts);
    }

    /// Transition node InProgress -> Stale
    pub fn stale(&mut self, node_ref: NodeRef, cur_ts: Timestamp) {
        self.transition_answered(node_ref, FanoutNodeStage::Stale, cur_ts);
    }

    /// Transition node InProgress -> Conflict
    pub fn conflict(&mut self, node_ref: NodeRef, cur_ts: Timestamp) {
        self.transition_answered(node_ref, FanoutNodeStage::Conflict, cur_ts);
    }

    /// Transition node InProgress -> Disqualified
    pub fn disqualified(&mut self, node_ref: NodeRef, cur_ts: Timestamp) {
        self.transition_answered(node_ref, FanoutNodeStage::Disqualified, cur_ts);
    }

    /// Transition all Accepted -> Queued, in the event a newer value for consensus is found and we want to try again
    pub fn all_accepted_to_queued(&mut self, cur_ts: Timestamp) {
        for node in &mut self.nodes {
            if matches!(node.1.status.stage, FanoutNodeStage::Accepted) {
                node.1.status.transition(FanoutNodeStage::Queued, cur_ts);
            }
        }
    }

    /// Transition all Accepted -> Conflict, in the event a conflicting result overtakes ours
    pub fn all_accepted_to_conflict(&mut self, cur_ts: Timestamp) {
        for node in &mut self.nodes {
            if matches!(node.1.status.stage, FanoutNodeStage::Accepted) {
                node.1.status.transition(FanoutNodeStage::Conflict, cur_ts);
            }
        }
    }

    /// Transition all Stale -> Queued, so they are asked again with a newer result
    pub fn all_stale_to_queued(&mut self, cur_ts: Timestamp) {
        for node in &mut self.nodes {
            if matches!(node.1.status.stage, FanoutNodeStage::Stale) {
                node.1.status.transition(FanoutNodeStage::Queued, cur_ts);
            }
        }
    }

    /// Transition all Conflict -> Queued, so they are asked again with our current result
    pub fn all_conflict_to_queued(&mut self, cur_ts: Timestamp) {
        for node in &mut self.nodes {
            if matches!(node.1.status.stage, FanoutNodeStage::Conflict) {
                node.1.status.transition(FanoutNodeStage::Queued, cur_ts);
            }
        }
    }

    /// Transition all Accepted -> Stale, in the event a newer value for consensus is found but we don't want to try again
    pub fn all_accepted_to_stale(&mut self, cur_ts: Timestamp) {
        for node in &mut self.nodes {
            if matches!(node.1.status.stage, FanoutNodeStage::Accepted) {
                node.1.status.transition(FanoutNodeStage::Stale, cur_ts);
            }
        }
    }

    /// The window's in-flight nodes and how long each is waited for, when the ones still counted
    /// on could not bring the window to `target` accepts even if they all accepted and nothing in
    /// the window is left to ask: past `floor` and `STRAGGLER_RTT_FACTOR` times the slowest
    /// answer. A demoted node in flight is not counted on
    fn straggler_scan(
        &self,
        target: usize,
        floor: TimestampDuration,
    ) -> Option<(Vec<NodeId>, TimestampDuration)> {
        if target == 0 || self.consensus_width == 0 {
            return None;
        }

        let mut accepted = 0usize;
        let mut in_flight = Vec::<NodeId>::new();
        let mut counted_on = 0usize;
        let mut window_slots = 0usize;
        for node_id in &self.sorted_nodes {
            let node = self.nodes.get(node_id).unwrap_or_log();
            match node.status.stage {
                FanoutNodeStage::Timeout | FanoutNodeStage::Disqualified => continue,
                FanoutNodeStage::Queued => return None,
                FanoutNodeStage::InProgress => {
                    in_flight.push(node_id.clone());
                    if !node.demoted {
                        counted_on += 1;
                    }
                }
                FanoutNodeStage::Accepted => accepted += 1,
                FanoutNodeStage::Rejected | FanoutNodeStage::Stale | FanoutNodeStage::Conflict => {}
            }
            window_slots += 1;
            if window_slots == self.consensus_width {
                break;
            }
        }
        if in_flight.is_empty() || accepted + counted_on >= target {
            return None;
        }

        // With no answer yet there is nothing to call a node slow against
        let slowest_answer = self
            .nodes
            .values()
            .filter_map(|node| node.status.answer_duration())
            .max()?;
        let grace = floor.max(slowest_answer.saturating_mul(STRAGGLER_RTT_FACTOR));
        Some((in_flight, grace))
    }

    /// When `time_out_stragglers` next has a node to cut
    pub fn next_straggler_cut(&self, target: usize, floor: TimestampDuration) -> Option<Timestamp> {
        let (in_flight, grace) = self.straggler_scan(target, floor)?;
        in_flight
            .iter()
            .map(|node_id| {
                self.nodes
                    .get(node_id)
                    .unwrap_or_log()
                    .status
                    .transition_ts
                    .later(grace)
            })
            .min()
    }

    /// Time out the stragglers `straggler_scan` finds past their grace, returning how many
    pub fn time_out_stragglers(
        &mut self,
        target: usize,
        floor: TimestampDuration,
        cur_ts: Timestamp,
    ) -> usize {
        let Some((in_flight, grace)) = self.straggler_scan(target, floor) else {
            return 0;
        };

        let mut timed_out = 0usize;
        for node_id in in_flight {
            let node = self.nodes.get_mut(&node_id).unwrap_or_log();
            if cur_ts.duration_since(node.status.transition_ts) < grace {
                continue;
            }
            node.status.transition(FanoutNodeStage::Timeout, cur_ts);
            // Drop the stop source to force the lane to stop processing this node
            drop(node.lane_stop_source.take());
            timed_out += 1;
        }
        timed_out
    }

    /// Transition all Queued | InProgress -> Timeout, in the event that the fanout is being cut short by a timeout
    pub fn all_unfinished_to_timeout(&mut self, cur_ts: Timestamp) {
        for node in &mut self.nodes {
            if matches!(
                node.1.status.stage,
                FanoutNodeStage::Queued | FanoutNodeStage::InProgress
            ) {
                node.1.status.transition(FanoutNodeStage::Timeout, cur_ts);
            }
        }
    }

    /// Transition InProgress | Queued -> Disqualified once they can no longer matter to the outcome
    ///
    /// Width counts only terminal answers that hold a slot (Accepted, Stale, Conflict, Rejected):
    /// a candidate is culled only when `consensus_width` closer nodes have answered for
    /// good, or rejections reach consensus. Unanswered nodes hold no slot, so a slow or
    /// unreachable closest node never culls the reserve that could replace it.
    fn disqualify(&mut self, cur_ts: Timestamp) {
        let mut consecutive_rejections = 0usize;
        let mut rejected_consensus = false;
        let mut window_slots = 0usize;
        for node_id in self.sorted_nodes.iter() {
            let node = self.nodes.get_mut(node_id).unwrap_or_log();

            // If the node is finished already we don't have to consider it
            if node.status.is_finished() {
                // Count up consecutive rejections
                if matches!(node.status.stage, FanoutNodeStage::Rejected) {
                    consecutive_rejections += 1;
                    if consecutive_rejections >= self.consensus_count {
                        rejected_consensus = true;
                    }
                    // A rejection holds its slot, it is a live close node's explicit answer
                    window_slots += 1;
                    continue;
                } else {
                    consecutive_rejections = 0;
                }

                if !matches!(
                    node.status.stage,
                    FanoutNodeStage::Timeout | FanoutNodeStage::Disqualified
                ) {
                    window_slots += 1;
                }
                continue;
            }

            // If the window is permanently held by closer answers, or we have had enough
            // consecutive rejections, cancel it immediately and disqualify it
            if (self.consensus_width > 0 && window_slots >= self.consensus_width)
                || rejected_consensus
            {
                node.status
                    .transition(FanoutNodeStage::Disqualified, cur_ts);
                if let Some(css) = node.lane_stop_source.take() {
                    // Drop the stop source to force the lane to stop processing this node
                    drop(css);
                }
            }
        }
    }

    /// Review the nodes in the queue
    pub fn with_nodes<R, F: FnOnce(&HashMap<NodeId, FanoutNode>, &[NodeId]) -> R>(
        &self,
        func: F,
    ) -> R {
        func(&self.nodes, &self.sorted_nodes)
    }
}
