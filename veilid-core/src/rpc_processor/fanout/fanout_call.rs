use super::*;

impl_veilid_log_facility!("fanout");

#[derive(Debug)]
struct FanoutContext<'a> {
    /// Queue of nodes to process
    fanout_queue: FanoutQueue<'a>,
    /// Current accumulated result
    result: FanoutResult,
    /// Termination disposition from `check_done`
    done: FanoutDoneDisposition,
    /// Stops the lanes and the queue pump, dropped on completion
    lanes_stop_source: Option<StopSource>,
    /// Timestamp consensus was first reached
    consensus_reached_ts: Option<Timestamp>,
    /// Last accepting disposition; before consensus the deadline stays a full timeout past it
    last_accepted_ts: Timestamp,
}

#[derive(Debug, Copy, Clone, Default)]
pub enum FanoutResultKind {
    #[default]
    Incomplete,
    Timeout,
    Consensus,
    Exhausted,
}
impl FanoutResultKind {
    pub fn is_incomplete(&self) -> bool {
        matches!(self, Self::Incomplete)
    }
}

#[derive(Clone, Debug, Default)]
pub struct FanoutResult {
    /// How the fanout completed
    pub kind: FanoutResultKind,
    /// The set of nodes that counted toward consensus
    /// (for example, had the most recent value for this subkey)
    pub consensus_nodes: Vec<NodeRef>,
    /// Which nodes accepted the request
    pub value_nodes: Vec<NodeRef>,
}

impl fmt::Display for FanoutResult {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let kc = match self.kind {
            FanoutResultKind::Incomplete => "I",
            FanoutResultKind::Timeout => "T",
            FanoutResultKind::Consensus => "C",
            FanoutResultKind::Exhausted => "E",
        };
        if f.alternate() {
            write!(
                f,
                "{}:{}[{}]",
                kc,
                self.consensus_nodes.len(),
                self.consensus_nodes
                    .iter()
                    .map(|x| x.to_string())
                    .collect::<Vec<_>>()
                    .join(","),
            )
        } else {
            write!(f, "{}:{}", kc, self.consensus_nodes.len())
        }
    }
}

pub fn debug_fanout_results(results: &[FanoutResult]) -> String {
    let mut col = 0;
    let mut out = String::new();
    let mut left = results.len();
    for r in results {
        if col == 0 {
            out += "    ";
        }
        let sr = format!("{}", r);
        out += &sr;
        out += ",";
        col += 1;
        left -= 1;
        if col == 32 && left != 0 {
            col = 0;
            out += "\n"
        }
    }
    out
}

#[derive(Debug)]
pub struct FanoutCallOutput {
    pub peer_info_list: Vec<Arc<UnverifiedPeerInfo>>,
    pub disposition: FanoutCallDisposition,
}

#[derive(Debug, Clone, Copy)]
pub enum FanoutQueueMode {
    ThrottleAtConsensus,
    Unthrottled,
}

/// How long to wait, as a fraction of the RPC timeout, before treating an in-progress node as slow
/// and dispatching another node beyond `consensus_count` to compensate. Pegged to the RPC timeout
/// rather than the per-fanout timeout so that all fanouts (set, get, transact_begin, watch, ...)
/// share the same throttle window regardless of their overall budget. Also the least a straggler
/// that can no longer complete consensus is waited for.
const THROTTLE_DURATION_PERCENT: u64 = 33;

/// The return type of the fanout call routine
#[derive(Debug, Copy, Clone)]
pub enum FanoutCallDisposition {
    /// The call routine timed out
    Timeout,
    /// The call routine returned an invalid result
    Invalid,
    /// The called node rejected the rpc request but may have returned more nodes
    Rejected,
    /// The called node accepted the rpc request and may have returned more nodes,
    /// but we don't count the result toward our consensus
    Stale,
    /// The called node accepted the rpc request and may have returned more nodes,
    /// but holds a result that conflicts with ours and can never count toward our consensus
    Conflict,
    /// The called node accepted the rpc request and may have returned more nodes,
    /// counting the result toward our consensus
    Accepted,
    /// The called node accepted the rpc request and may have returned more nodes,
    /// returning a newer value that indicates we should restart our consensus
    AcceptedNewerRestart,
    /// The called node accepted the rpc request and may have returned more nodes,
    /// returning a newer value that indicates our current consensus is stale and should be ignored,
    /// and counting the result toward a new consensus
    AcceptedNewer,
    /// The called node accepted the rpc request and may have returned more nodes,
    /// holding a conflicting result that now leads ours: our accepters become conflicts,
    /// earlier conflicts are asked again, and the result counts toward the new consensus
    AcceptedSwitch,
}

/// The return type of the fanout done routine
#[derive(Debug, Copy, Clone)]
pub enum FanoutDoneDisposition {
    /// Finish immediately without completing operations
    DoneEarly,
    /// Finish when all operations are complete
    #[allow(dead_code)]
    Done,
    /// Not done yet
    NotDone,
}

/// The return type of a fanout processor lane, or of the queue pump
enum FanoutProcessorReturn {
    /// The fanout finished early; stop every lane now
    DoneEarly,
    /// This lane or the pump has nothing left to do
    Done,
    /// A lane asked for work or a straggler cut is due
    Pump,
}

/// Exit reason from the inner fanout loop
enum FanoutLoopResult {
    /// Exhausted or consensus reached; return the current result
    Done,
    /// A lane returned an error; propagate it
    LaneFailed(RPCError),
    /// Caller-supplied stop token fired
    Cancelled,
    /// Effective deadline reached; mark unfinished lanes as timed out and re-evaluate
    DeadlineReached,
}

/// Result of a single per-node fanout call
pub type FanoutCallResult = Result<FanoutCallOutput, RPCError>;
/// Filter that decides whether a peer-info entry is eligible for fanout
pub type FanoutPeerInfoFilter = Arc<dyn (Fn(&NodeIdGroup, &NodeInfo) -> bool) + Send + Sync>;
/// Termination check called after each lane response
pub type FanoutCheckDone = Arc<dyn (Fn(&FanoutResult) -> FanoutDoneDisposition) + Send + Sync>;
/// Per-node call routine invoked once per fanout target
pub type FanoutCallRoutine =
    Arc<dyn (Fn(NodeRef) -> PinBoxFutureStatic<FanoutCallResult>) + Send + Sync>;
/// Post-consensus rolling window, recomputed from the result on every further accept
pub type FanoutPostConsensusTimeoutCallback =
    Arc<dyn (Fn(&FanoutResult) -> TimestampDuration) + Send + Sync>;

pub fn empty_fanout_peer_info_filter() -> FanoutPeerInfoFilter {
    Arc::new(|_, _| true)
}

pub fn capability_fanout_peer_info_filter(caps: Vec<VeilidCapability>) -> FanoutPeerInfoFilter {
    Arc::new(move |_, ni| ni.has_all_capabilities(&caps))
}

/// Contains the logic for generically searching the Veilid routing table for a set of nodes and applying an
/// RPC operation that eventually converges on satisfactory result, or times out and returns some
/// unsatisfactory but acceptable result. Or something.
///
/// The algorithm starts by creating a 'closest_nodes' working set of the nodes closest to some node id currently in our routing table
/// If has pluggable callbacks:
///  * 'check_done' - for checking for a termination condition
///  * 'call_routine' - routine to call for each node that performs an operation and may add more nodes to our closest_nodes set
///
/// The algorithm is parameterized by:
///  * 'node_count' - the number of nodes to keep in the closest_nodes set
///  * 'fanout' - the number of concurrent calls being processed at the same time
///  * 'consensus_count' - the number of nodes in the processed queue that need to be in the 'Accepted' state before we terminate the fanout early. 0 means no consensus is required.
///  * 'consensus_width' - the number of nodes away from the key that can be considered for fanout operations. 0 means no consensus width is applied.
///
/// The algorithm returns early if 'check_done' returns some value, or if an error is found during the process.
/// If the algorithm times out, a Timeout result is returned, however operations will still have been performed and a
/// timeout is not necessarily indicative of an algorithmic 'failure', just that no definitive stopping condition was found
/// in the given time
pub(crate) struct FanoutCall<'a> {
    /// Routing table the candidates come from
    routing_table: &'a RoutingTable,
    /// Name for debugging
    name: String,
    /// Hash coordinate for the record we are fanning out for
    hash_coordinate: HashCoordinate,
    /// Number of nodes to keep in the closest nodes set
    node_count: usize,
    /// Number of concurrent fanout lanes to run
    fanout_tasks: usize,
    /// Accepts that make consensus; 0 means none is required
    consensus_count: usize,
    /// Number of nodes away from the key that can be considered; 0 means no width is applied
    consensus_width: usize,
    /// Timeout for the fanout call
    timeout: TimestampDuration,
    /// Filter deciding which peers are eligible
    peer_info_filter: FanoutPeerInfoFilter,
    /// Per-node call routine
    call_routine: FanoutCallRoutine,
    /// Termination check run after each lane response
    check_done: FanoutCheckDone,
    /// Sizes the post-consensus window, when set
    post_consensus_timeout_callback: Option<FanoutPostConsensusTimeoutCallback>,
    /// Accepts this run needs, when it differs from `consensus_count` (see `set_run_target`)
    opt_run_target_count: Option<usize>,
    /// Accepts to keep collecting toward after consensus (see `set_collection_count`)
    collection_count: usize,
    /// Seed from never-contacted nodes too (see `set_seed_untried`)
    seed_untried: bool,
    /// Ask the initial seed nodes before unproven closer ones (see `set_prefer_init_nodes`)
    prefer_init_nodes: bool,
    /// Ask nodes unresponsive to routed questions after their window (see `set_demote_routed_unresponsive`)
    demote_routed_unresponsive: bool,
}

impl VeilidComponentRegistryAccessor for FanoutCall<'_> {
    fn registry(&self) -> VeilidComponentRegistry {
        self.routing_table.registry()
    }
}

/// Parameters for a fanout call
pub(crate) struct FanoutCallParams {
    /// Name for debugging
    pub name: String,
    /// Hash coordinate for the record we are fanning out for
    pub hash_coordinate: HashCoordinate,
    /// Number of nodes to keep in the closest nodes set
    pub node_count: usize,
    /// Number of concurrent fanout lanes to run
    pub fanout_tasks: usize,
    /// Number of nodes in the processed queue that need to be in the 'Accepted' state before we terminate the fanout. 0 means no consensus is required.
    pub consensus_count: usize,
    /// Number of nodes away from the key that can be considered for fanout operations. 0 means no consensus width is applied.
    pub consensus_width: usize,
    /// Timeout for the fanout call
    pub timeout: TimestampDuration,
}

impl<'a> FanoutCall<'a> {
    pub fn new(
        routing_table: &'a RoutingTable,
        params: FanoutCallParams,
        peer_info_filter: FanoutPeerInfoFilter,
        call_routine: FanoutCallRoutine,
        check_done: FanoutCheckDone,
    ) -> Self {
        Self {
            routing_table,
            name: params.name,
            hash_coordinate: params.hash_coordinate,
            node_count: params.node_count,
            fanout_tasks: params.fanout_tasks,
            consensus_count: params.consensus_count,
            consensus_width: params.consensus_width,
            timeout: params.timeout,
            peer_info_filter,
            call_routine,
            check_done,
            post_consensus_timeout_callback: None,
            opt_run_target_count: None,
            collection_count: params.consensus_count,
            seed_untried: false,
            prefer_init_nodes: false,
            demote_routed_unresponsive: false,
        }
    }

    /// After consensus the callback sizes a window that re-arms on every further
    /// accept. The fanout exits when one elapses with nothing new arriving, or when
    /// the pre-consensus deadline is reached, whichever comes first.
    pub fn with_post_consensus_timeout_callback(
        mut self,
        callback: FanoutPostConsensusTimeoutCallback,
    ) -> Self {
        self.post_consensus_timeout_callback = Some(callback);
        self
    }

    /// Also seed from nodes we have never contacted.
    ///
    /// Off by default: `Initial` entries are unproven and a cold table is full of
    /// them, so a first pass spends its timeout on nodes that may not answer. A
    /// caller that has exhausted every node it HAS reached, without consensus, has
    /// nothing cheaper left to try. Unproven is not failed, so this still excludes
    /// Missing, Dead and Punished.
    pub fn set_seed_untried(&mut self, seed_untried: bool) {
        self.seed_untried = seed_untried;
    }

    /// Ask the initial seed nodes before unproven closer ones. Seeds are the nodes
    /// that previously answered for this record, the only evidence that a node can
    /// serve it, and the default sort merges them into distance order and loses that.
    /// Set it for an operation only a node already holding the record can satisfy.
    pub fn set_prefer_init_nodes(&mut self, prefer_init_nodes: bool) {
        self.prefer_init_nodes = prefer_init_nodes;
    }

    /// Ask nodes that lost a routed question after the rest of their window, and do not wait on
    /// them past the straggler grace.
    ///
    /// Set it when this run's questions travel over safety routes: a node that lost one, often
    /// one reachable only through a relay, still answers direct questions and so stays live, but
    /// a routed question to it is likely to wait out the RPC timeout. It keeps its place in the
    /// window: the closest nodes own the record, and only an answer from it can clear its mark.
    pub fn set_demote_routed_unresponsive(&mut self, demote_routed_unresponsive: bool) {
        self.demote_routed_unresponsive = demote_routed_unresponsive;
    }

    /// Narrow which peers this call will consider, for a second run over the same
    /// call routine. A node already holding state from the first run must be
    /// filtered out here rather than skipped inside the routine, or it is asked
    /// again and answers again.
    pub fn set_peer_info_filter(&mut self, peer_info_filter: FanoutPeerInfoFilter) {
        self.peer_info_filter = peer_info_filter;
    }

    /// Retarget a second run at the accepts still missing rather than the original
    /// count, moving the termination check with it.
    ///
    /// Only the result kind moves. `consensus_count` still drives how many nodes the
    /// queue works at once and how many consecutive rejections end the fanout, and
    /// neither got easier because we already hold some accepts.
    pub fn set_run_target(&mut self, target_count: usize, check_done: FanoutCheckDone) {
        self.opt_run_target_count = Some(target_count);
        self.check_done = check_done;
    }

    /// Keep dispatching replacement candidates after consensus until this many
    /// accepts, the queue exhausts, or the post-consensus window closes.
    ///
    /// The default (= `consensus_count`) stops all dispatch at consensus, so only
    /// lanes already in flight can add accepts; a caller that wants headroom
    /// beyond the durability minimum raises this to its collection cap.
    pub fn set_collection_count(&mut self, collection_count: usize) {
        self.collection_count = collection_count.max(self.consensus_count);
    }

    /// Whether this run keeps working the queue after consensus
    fn collecting_past_consensus(&self) -> bool {
        self.collection_count > self.consensus_count
    }

    /// Accepts this run must collect to call it consensus
    fn run_target_count(&self) -> usize {
        self.opt_run_target_count.unwrap_or(self.consensus_count)
    }

    #[cfg_attr(
        feature = "instrument",
        instrument(level = "trace", target = "fanout", skip_all, fields(__VEILID_LOG_KEY = self.log_key()))
    )]
    fn evaluate_done(&self, ctx: &mut FanoutContext) -> FanoutDoneDisposition {
        // DoneEarly is terminal — once set, no further re-evaluation. Done can
        // still escalate to DoneEarly as more accepts arrive (e.g. when a
        // safety-margin threshold is reached).
        if matches!(ctx.done, FanoutDoneDisposition::DoneEarly) {
            return ctx.done;
        }

        // Consensus = `consensus_count` accepts anywhere within the `consensus_width`
        // closest viable nodes:
        // - closeness ordering inside the window doesn't matter, so one slow closer
        //   node can't block faster peers from reaching consensus
        // - Timeout and Disqualified take no window slot, so an unreachable closest
        //   node widens the window to the next candidate instead of capping consensus
        //   below its count
        // - a rejection holds its slot, it is a live close node's explicit answer
        // - `disqualify` windows the same way
        let fanout_result = ctx.fanout_queue.with_nodes(|nodes, sorted_nodes| {
            let mut consensus_nodes: Vec<NodeRef> = vec![];
            let mut value_nodes: Vec<NodeRef> = vec![];
            let mut pending_in_window = false;
            let mut window_slots = 0usize;
            for sn in sorted_nodes.iter() {
                let node = nodes.get(sn).unwrap_or_log();
                let stage = node.status.stage();
                let in_window = if matches!(
                    stage,
                    FanoutNodeStage::Timeout | FanoutNodeStage::Disqualified
                ) {
                    false
                } else {
                    window_slots += 1;
                    window_slots <= self.consensus_width
                };
                match stage {
                    FanoutNodeStage::Queued | FanoutNodeStage::InProgress => {
                        if in_window {
                            pending_in_window = true;
                        }
                    }
                    FanoutNodeStage::Timeout
                    | FanoutNodeStage::Rejected
                    | FanoutNodeStage::Disqualified => {}
                    FanoutNodeStage::Stale | FanoutNodeStage::Conflict => {
                        value_nodes.push(node.work_item.node_ref.clone());
                    }
                    FanoutNodeStage::Accepted => {
                        value_nodes.push(node.work_item.node_ref.clone());
                        if in_window {
                            consensus_nodes.push(node.work_item.node_ref.clone());
                        }
                    }
                }
            }

            let kind = if consensus_nodes.len() >= self.run_target_count() {
                FanoutResultKind::Consensus
            } else if pending_in_window {
                FanoutResultKind::Incomplete
            } else {
                FanoutResultKind::Exhausted
            };
            FanoutResult {
                kind,
                consensus_nodes,
                value_nodes,
            }
        });

        let done = (self.check_done)(&fanout_result);
        ctx.result = fanout_result;
        ctx.done = done;
        if !matches!(done, FanoutDoneDisposition::NotDone) {
            if ctx.consensus_reached_ts.is_none() {
                ctx.consensus_reached_ts = Some(Timestamp::now_non_decreasing());
            }
            // A collecting run keeps its lanes on Done; the post-consensus window bounds how long
            if matches!(done, FanoutDoneDisposition::DoneEarly) || !self.collecting_past_consensus()
            {
                drop(ctx.lanes_stop_source.take())
            }
        }
        done
    }

    #[cfg_attr(
        feature = "instrument",
        instrument(level = "trace", target = "fanout", skip_all, fields(__VEILID_LOG_KEY = self.log_key()))
    )]
    async fn fanout_processor(
        &self,
        lane_name: String,
        context: &Mutex<FanoutContext<'_>>,
    ) -> Result<FanoutProcessorReturn, RPCError> {
        // Make a stop token to break out when we're done; a fanout that finished
        // before this lane started has nothing left for it to do
        let Some(stop_token) = context
            .lock()
            .lanes_stop_source
            .as_ref()
            .map(|ss| ss.token())
        else {
            veilid_log!(self debug "{}[{}]: Fanout finished before lane started", self.name, lane_name);
            return Ok(FanoutProcessorReturn::Done);
        };

        // Loop until we have a result or are done
        loop {
            // Put in a work request
            let work_receiver = {
                let mut context_locked = context.lock();
                veilid_log!(self debug "{}[{}]: Requesting work", self.name, lane_name);
                context_locked
                    .fanout_queue
                    .request_work(lane_name.clone())?
            };

            // Wait around for some work to do
            let Ok(Ok(work_item)) = work_receiver
                .recv_async()
                .timeout_at(stop_token.clone())
                .await
            else {
                // If we don't have a node to process, or we are being told to stop, stop fanning out
                veilid_log!(self debug "{}[{}]: Lane done", self.name, lane_name);
                break Ok(FanoutProcessorReturn::Done);
            };

            let work_node = work_item.node_ref.clone();
            let cancel_stop_token = work_item.work_item_stop_token.clone();

            // Do the call for this node
            match (self.call_routine)(work_node.clone())
                .timeout_at(cancel_stop_token)
                .await
            {
                Ok(Ok(output)) => {
                    // Filter returned nodes
                    let filtered_v: Vec<Arc<UnverifiedPeerInfo>> = output
                        .peer_info_list
                        .into_iter()
                        .filter(|pi| {
                            if !(self.peer_info_filter)(pi.node_ids(), pi.node_info()) {
                                return false;
                            }
                            true
                        })
                        .collect();

                    // Call succeeded
                    // Register the returned nodes and add them to the fanout queue in sorted order
                    let new_nodes = self
                        .routing_table
                        .register_nodes_with_peer_info_list(filtered_v);

                    // Update queue
                    {
                        let mut context_locked = context.lock();
                        let cur_ts = Timestamp::now_non_decreasing();

                        // Process disposition of the output of the fanout call routine.
                        // Any accepting disposition stamps last_accepted_ts, which resets the
                        // pre-consensus deadline in run(): the accepted node's server-side state
                        // is protected by a background keepalive, so it's safe to keep converging
                        // toward closer nodes as long as we're still making progress.
                        match output.disposition {
                            FanoutCallDisposition::Timeout => {
                                context_locked.fanout_queue.timeout(work_node, cur_ts);
                            }
                            FanoutCallDisposition::Rejected => {
                                context_locked.fanout_queue.rejected(work_node, cur_ts);
                            }
                            FanoutCallDisposition::Accepted => {
                                context_locked.fanout_queue.accepted(work_node, cur_ts);
                                context_locked.last_accepted_ts = cur_ts;
                            }
                            FanoutCallDisposition::AcceptedNewerRestart => {
                                context_locked.fanout_queue.all_accepted_to_queued(cur_ts);
                                context_locked.fanout_queue.all_conflict_to_queued(cur_ts);
                                context_locked.fanout_queue.all_stale_to_queued(cur_ts);
                                context_locked.fanout_queue.accepted(work_node, cur_ts);
                                context_locked.last_accepted_ts = cur_ts;
                            }
                            FanoutCallDisposition::AcceptedNewer => {
                                context_locked.fanout_queue.all_accepted_to_stale(cur_ts);
                                context_locked.fanout_queue.accepted(work_node, cur_ts);
                                context_locked.last_accepted_ts = cur_ts;
                            }
                            FanoutCallDisposition::AcceptedSwitch => {
                                // Earlier conflicts are queued before our accepters join them
                                context_locked.fanout_queue.all_conflict_to_queued(cur_ts);
                                context_locked.fanout_queue.all_accepted_to_conflict(cur_ts);
                                context_locked.fanout_queue.accepted(work_node, cur_ts);
                                context_locked.last_accepted_ts = cur_ts;
                            }
                            FanoutCallDisposition::Invalid => {
                                context_locked.fanout_queue.disqualified(work_node, cur_ts);
                            }
                            FanoutCallDisposition::Stale => {
                                context_locked.fanout_queue.stale(work_node, cur_ts);
                            }
                            FanoutCallDisposition::Conflict => {
                                context_locked.fanout_queue.conflict(work_node, cur_ts);
                            }
                        }

                        // Add any new nodes
                        context_locked.fanout_queue.update(&new_nodes, cur_ts);

                        // See if we're done before going back for more processing
                        match self.evaluate_done(&mut context_locked) {
                            FanoutDoneDisposition::DoneEarly => {
                                veilid_log!(self debug "{}[{}]: Fanout done, terminating all other lanes", self.name, lane_name);
                                break Ok(FanoutProcessorReturn::DoneEarly);
                            }
                            FanoutDoneDisposition::Done => {
                                if self.collecting_past_consensus() {
                                    veilid_log!(self debug "{}[{}]: Consensus met, lane continuing toward collection target", self.name, lane_name);
                                } else {
                                    veilid_log!(self debug "{}[{}]: Fanout done, letting other lanes complete", self.name, lane_name);
                                    break Ok(FanoutProcessorReturn::Done);
                                }
                            }
                            FanoutDoneDisposition::NotDone => {
                                veilid_log!(self debug "{}[{}]: Work done, lane checking for more work", self.name, lane_name);
                            }
                        }
                    }
                }
                Ok(Err(e)) => {
                    veilid_log!(self debug "{}[{}]: Error occurred, terminating fanout: {}", self.name, lane_name, e);
                    break Err(e);
                }
                Err(_) => {
                    // Cancelled
                    veilid_log!(self debug "{}[{}]: Work cancelled, lane checking for more work", self.name, lane_name);
                }
            };
        }
    }

    #[cfg_attr(
        feature = "instrument",
        instrument(level = "trace", target = "fanout", skip_all, fields(__VEILID_LOG_KEY = self.log_key()))
    )]
    fn init_closest_nodes(
        &self,
        context: &mut FanoutContext,
        cur_ts: Timestamp,
    ) -> Result<(), RPCError> {
        // Get the 'node_count' closest nodes to the key out of our routing table
        let closest_nodes = {
            let peer_info_filter = self.peer_info_filter.clone();
            // Never-contacted nodes are often dead in a cold table, so only a re-plan seeds from them
            let seed_floor = if self.seed_untried {
                BucketEntryState::Initial
            } else {
                BucketEntryState::Unreliable
            };
            let filter = Box::new(
                move |opt_snap: &Option<BucketEntrySnapshot>, _cur_ts: Timestamp| {
                    // Exclude our own node
                    let Some(snap) = opt_snap else {
                        return false;
                    };

                    if snap.state < seed_floor {
                        return false;
                    }

                    // Filter entries
                    let Some(peer_info) = snap.get_peer_info(RoutingDomain::PublicInternet) else {
                        return false;
                    };
                    // Ensure only things that are valid/signed in the PublicInternet domain are returned
                    if peer_info.signatures().is_empty() {
                        return false;
                    }

                    // Check our node info filter
                    if !(peer_info_filter)(peer_info.node_ids(), peer_info.node_info()) {
                        return false;
                    }

                    true
                },
            ) as RoutingTableEntryFilter;
            let filters = VecDeque::from([filter]);

            let transform =
                |opt_snap: Option<BucketEntrySnapshot>| opt_snap.unwrap_or_log().node_ref.clone();

            self.routing_table.get_preferred_closest_nodes(
                self.node_count,
                self.hash_coordinate.clone(),
                filters,
                transform,
            )
        };

        context.fanout_queue.update(&closest_nodes, cur_ts);

        Ok(())
    }

    #[cfg_attr(
        feature = "instrument",
        instrument(level = "trace", target = "fanout", skip_all, fields(__VEILID_LOG_KEY = self.log_key()))
    )]
    pub async fn run(
        &self,
        init_fanout_queue: Vec<NodeRef>,
        fanout_queue_mode: FanoutQueueMode,
        fanout_stop_token: StopToken,
    ) -> Result<FanoutResult, RPCError> {
        // Create context for this run
        let node_sort = Box::new(RoutingTable::make_closest_node_id_sort(
            self.hash_coordinate.clone(),
        ));
        let rpc_timeout_ms = self.config().internal().network.rpc.timeout_ms;
        let slow_node_duration = TimestampDuration::new_ms(rpc_timeout_ms.into())
            .saturating_mul(THROTTLE_DURATION_PERCENT)
            .div(100);
        let context = Arc::new(Mutex::new(FanoutContext {
            fanout_queue: FanoutQueue::new(
                self.name.clone(),
                self.routing_table.registry(),
                self.hash_coordinate.kind(),
                node_sort,
                self.consensus_count,
                self.collection_count,
                self.consensus_width,
                match fanout_queue_mode {
                    FanoutQueueMode::ThrottleAtConsensus => Some(slow_node_duration),
                    FanoutQueueMode::Unthrottled => None,
                },
            ),
            result: FanoutResult {
                kind: FanoutResultKind::Incomplete,
                consensus_nodes: vec![],
                value_nodes: vec![],
            },
            done: FanoutDoneDisposition::NotDone,
            lanes_stop_source: Some(StopSource::new()),
            consensus_reached_ts: None,
            last_accepted_ts: Timestamp::now_non_decreasing(),
        }));

        // Initialize closest nodes list
        {
            let context_locked = &mut *context.lock();
            let cur_ts = Timestamp::now_non_decreasing();

            if self.prefer_init_nodes {
                context_locked
                    .fanout_queue
                    .set_preferred_nodes(&init_fanout_queue);
            }
            context_locked
                .fanout_queue
                .set_demote_routed_unresponsive(self.demote_routed_unresponsive);

            self.init_closest_nodes(context_locked, cur_ts)?;

            // Ensure we include the most recent nodes
            context_locked
                .fanout_queue
                .update(&init_fanout_queue, cur_ts);

            // Do a quick check to see if we're already done
            if !matches!(
                self.evaluate_done(context_locked),
                FanoutDoneDisposition::NotDone
            ) {
                return Ok(core::mem::take(&mut context_locked.result));
            }
        }

        // Pump to cut stragglers and refresh the deadline as the queue changes
        let (lanes_stop_token, pump_wake_receiver) = {
            let context_locked = context.lock();
            let lanes_stop_token = context_locked
                .lanes_stop_source
                .as_ref()
                .ok_or_else(|| RPCError::internal("should have stop source"))?
                .token();
            (
                lanes_stop_token,
                context_locked.fanout_queue.pump_wake_receiver(),
            )
        };
        let make_pump_future = |opt_straggler_cut: Option<Timestamp>| {
            let lanes_stop_token = lanes_stop_token.clone();
            let pump_wake_receiver = pump_wake_receiver.clone();
            pin_dyn_future!(async move {
                match pump_wake_receiver
                    .wait(opt_straggler_cut.map(|ts| ts.as_u64()), lanes_stop_token)
                    .await
                {
                    WakeOutcome::Woken | WakeOutcome::Deadline => Ok(FanoutProcessorReturn::Pump),
                    WakeOutcome::Stopped => Ok(FanoutProcessorReturn::Done),
                }
            })
        };

        // If not, do the fanout
        let mut unord = FuturesUnordered::new();
        {
            // Spin up 'fanout' tasks to process the fanout
            for n in 0..self.fanout_tasks {
                unord.push(pin_dyn_future!(
                    self.fanout_processor(format!("lane#{}", n), &context)
                ));
            }
            // Lanes wake the pump as they ask for work
            unord.push(make_pump_future(None));
        }

        // Effective deadline:
        // - before consensus, a full timeout past the last accepting disposition, so the
        //   fanout keeps converging on closer nodes while it makes progress (accepted
        //   nodes are kept alive server-side by a background keepalive)
        // - after consensus, a rolling window (post_consensus_timeout_callback) re-armed
        //   per accept and capped by what the pre-consensus deadline had left
        // - with no callback, the pre-consensus deadline stands
        let mut effective_deadline = Timestamp::now_non_decreasing().later(self.timeout);
        let mut post_consensus_cap: Option<Timestamp> = None;
        let inner_result: FanoutLoopResult = loop {
            let now = Timestamp::now_non_decreasing();
            let opt_consensus_ts = context.lock().consensus_reached_ts;
            match opt_consensus_ts {
                None => {
                    let progress_deadline = context.lock().last_accepted_ts.later(self.timeout);
                    if progress_deadline > effective_deadline {
                        effective_deadline = progress_deadline;
                    }
                }
                Some(consensus_ts) => {
                    if let Some(callback) = &self.post_consensus_timeout_callback {
                        let (anchor, result_snapshot) = {
                            let ctx = context.lock();
                            (ctx.last_accepted_ts.max(consensus_ts), ctx.result.clone())
                        };
                        let cap = *post_consensus_cap.get_or_insert(effective_deadline);
                        effective_deadline = cap.min(anchor.later((callback)(&result_snapshot)));
                    }
                }
            }
            if now >= effective_deadline {
                break FanoutLoopResult::DeadlineReached;
            }
            let remaining = effective_deadline
                .duration_since(now)
                .max(TimestampDuration::new_ms(1));

            let fanout_stop_token = fanout_stop_token.clone();
            let next = unord
                .next()
                .in_current_span()
                .timeout_at(fanout_stop_token)
                .timeout_duration(remaining)
                .await;
            // Outer layer: deadline elapsed
            let Ok(next) = next else {
                break FanoutLoopResult::DeadlineReached;
            };
            // Inner layer: caller-supplied stop token fired
            let Ok(next) = next else {
                break FanoutLoopResult::Cancelled;
            };

            match next {
                // No more lanes to process
                None => break FanoutLoopResult::Done,
                // Lane errored
                Some(Err(e)) => break FanoutLoopResult::LaneFailed(e),
                // Stop all lanes immediately
                Some(Ok(FanoutProcessorReturn::DoneEarly)) => break FanoutLoopResult::Done,
                // Lane finished; the deadline is recomputed at the top of the loop
                Some(Ok(FanoutProcessorReturn::Done)) => {}
                // Queue changed or a cut is due: cut stragglers, push more work, re-arm the pump
                Some(Ok(FanoutProcessorReturn::Pump)) => {
                    let context_locked = &mut *context.lock();
                    let cur_ts = Timestamp::now_non_decreasing();
                    let stragglers = context_locked.fanout_queue.time_out_stragglers(
                        self.run_target_count(),
                        slow_node_duration,
                        cur_ts,
                    );
                    if stragglers > 0 {
                        veilid_log!(self debug "{}: Timed out {} stragglers that cannot complete consensus", self.name, stragglers);
                        if matches!(
                            self.evaluate_done(context_locked),
                            FanoutDoneDisposition::DoneEarly
                        ) {
                            break FanoutLoopResult::Done;
                        }
                    }
                    context_locked.fanout_queue.send_more_work(cur_ts);
                    if !unord.is_empty() {
                        let opt_straggler_cut = context_locked
                            .fanout_queue
                            .next_straggler_cut(self.run_target_count(), slow_node_duration);
                        unord.push(make_pump_future(opt_straggler_cut));
                    }
                }
            }
        };

        match inner_result {
            FanoutLoopResult::Done => {
                let context_locked = &mut *context.lock();
                veilid_log!(self debug "{}: Finished FanoutQueue:\n{}", self.name, context_locked.fanout_queue);
                Ok(core::mem::take(&mut context_locked.result))
            }
            FanoutLoopResult::LaneFailed(e) => Err(e),
            FanoutLoopResult::Cancelled => Err(RPCError::ignore("fanout cancelled")),
            FanoutLoopResult::DeadlineReached => {
                let context_locked = &mut *context.lock();
                let cur_ts = Timestamp::now_non_decreasing();
                context_locked
                    .fanout_queue
                    .all_unfinished_to_timeout(cur_ts);
                veilid_log!(self debug "{}: Timeout FanoutQueue:\n{}", self.name, context_locked.fanout_queue);
                if !matches!(
                    self.evaluate_done(context_locked),
                    FanoutDoneDisposition::NotDone,
                ) {
                    // Last-chance value returned at timeout
                    return Ok(core::mem::take(&mut context_locked.result));
                }

                // We definitely weren't done, so this is just a plain timeout
                let mut result = core::mem::take(&mut context_locked.result);
                result.kind = FanoutResultKind::Timeout;
                Ok(result)
            }
        }
    }
}
