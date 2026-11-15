use super::*;

impl_veilid_log_facility!("stor");

/// The fully parsed descriptor
struct InspectDescriptorInfo {
    /// The descriptor itself
    descriptor: Arc<SignedValueDescriptor>,

    /// The in-schema subkeys that overlap the inspected range
    subkeys: ValueSubkeyRangeSet,
}

impl InspectDescriptorInfo {
    pub fn try_new(
        descriptor: Arc<SignedValueDescriptor>,
        subkeys: &ValueSubkeyRangeSet,
    ) -> VeilidAPIResult<Self> {
        let schema = descriptor.schema().map_err(RPCError::invalid_format)?;
        let subkeys = schema.truncate_subkeys(subkeys, Some(DHTSchema::MAX_SUBKEY_COUNT));
        Ok(Self {
            descriptor,
            subkeys,
        })
    }
}

/// Info tracked per subkey
struct SubkeySeqCount {
    /// The newest sequence number found for a subkey
    pub seq: ValueSeqNum,
    /// The set of nodes that had the most recent value for this subkey
    pub consensus_nodes: Vec<NodeRef>,
    /// The set of nodes that had any value for this subkey
    pub value_nodes: Vec<NodeRef>,
}

/// The context of the outbound_inspect_value operation
struct OutboundInspectValueContext {
    /// The combined sequence numbers and result counts so far
    pub seqcounts: Vec<SubkeySeqCount>,
    /// The descriptor if we got a fresh one or empty if no descriptor was needed
    pub opt_descriptor_info: Option<InspectDescriptorInfo>,
    /// Bumped whenever a subkey's seq or consensus set advances
    pub convergence: usize,
    /// The convergence counter as of the previous done check
    pub checked_convergence: usize,
    /// Nodes that answered, holding the record or not
    pub answered: usize,
    /// Safety routes that carried an accepted answer
    #[cfg(feature = "verbose-tracing")]
    pub answered_safety_routes: BTreeSet<PublicKey>,
}

/// The result of the outbound_inspect_value operation
#[derive(Debug, Clone)]
pub(in crate::storage_manager) struct OutboundInspectValueResult {
    /// Fanout results for each subkey
    pub subkey_fanout_results: Vec<FanoutResult>,
    /// The inspection that was retrieved
    pub inspect_result: InspectResult,
    /// Nodes that answered, holding the record or not; zero means every call failed
    pub answered: usize,
    /// Safety routes that carried an accepted answer
    #[cfg(feature = "verbose-tracing")]
    pub answered_safety_routes: Vec<PublicKey>,
}

/// The result of the inbound_inspect_value operation
#[derive(Clone, Debug)]
pub(crate) enum InboundInspectValueResult {
    /// Value inspected successfully
    Success(InspectResult),
}

/// What an inspect fanout is waiting to learn, and therefore when it may stop.
#[derive(Debug, Copy, Clone)]
pub(super) enum InspectFanoutStop {
    /// The seqs. Stops on [inspect_fanout_done]
    SubkeyConsensus,
    /// The descriptor. Stops on [descriptor_fanout_done], at the first node that has it
    Descriptor,
}

/// Whether an inspect fanout gathering seqs may stop early.
///
/// An unanswered subkey may still live on nodes further out than we have reached,
/// so the nodes in hand cannot prove absence:
/// - stop only once some subkey has consensus, proving these nodes carry the record
/// - every subkey anyone answered for must have reached full consensus
/// - the last round must have added nothing
///
/// So a range in which NO subkey is ever answered, every subkey unwritten, can never
/// satisfy `any_consensus` and runs to node exhaustion or the full get timeout. An
/// inspect for seqs must always include a subkey known to have been written; one that
/// only wants the descriptor uses [InspectFanoutStop::Descriptor] instead.
pub(super) fn inspect_fanout_done(
    consensus_node_counts: impl IntoIterator<Item = usize>,
    consensus_count: usize,
    has_descriptor: bool,
    stalled: bool,
) -> FanoutDoneDisposition {
    let mut any_subkeys = false;
    let mut any_consensus = false;
    let mut any_converging = false;
    for count in consensus_node_counts {
        any_subkeys = true;
        if count >= consensus_count {
            any_consensus = true;
        } else if count > 0 {
            any_converging = true;
        }
    }

    if any_subkeys && has_descriptor && any_consensus && !any_converging && stalled {
        FanoutDoneDisposition::DoneEarly
    } else {
        FanoutDoneDisposition::NotDone
    }
}

/// Whether an inspect fanout wanting only the descriptor may stop early.
///
/// A descriptor is immutable and self-authenticating: the record key is a hash over the
/// owner and schema, and `SignedValueDescriptor::validate` recomputes it before the
/// answer reaches the fanout. So the first descriptor to arrive is the only one that
/// could arrive, and further nodes cannot improve on it. No consensus, no seq, and no
/// dependence on any subkey ever having been written.
///
/// `has_seqs` guards the result frame rather than the descriptor: a node can hand over a
/// descriptor and no seqs, and stopping there would return a fanout result list that
/// does not line up with the subkeys the descriptor names.
pub(super) fn descriptor_fanout_done(
    has_descriptor: bool,
    has_seqs: bool,
) -> FanoutDoneDisposition {
    if has_descriptor && has_seqs {
        FanoutDoneDisposition::DoneEarly
    } else {
        FanoutDoneDisposition::NotDone
    }
}

impl StorageManager {
    /// Inspect an opened DHT record for its subkey sequence numbers
    #[cfg_attr(
        feature = "instrument",
        instrument(level = "trace", target = "stor", skip_all, fields(__VEILID_LOG_KEY = self.log_key()))
    )]
    pub async fn inspect_record(
        &self,
        record_key: RecordKey,
        subkeys: ValueSubkeyRangeSet,
        scope: DHTReportScope,
    ) -> VeilidAPIResult<DHTRecordReport> {
        self.inspect_record_with_fanout(
            record_key,
            subkeys,
            scope,
            FanoutConfig::default(),
            RecordLockPriority::Foreground,
        )
        .await
    }

    /// `inspect_record` with a say in the fanout: a watch at consensus asks the nodes it already
    /// knows and wants no peers back. The API path above has no such nodes, so it is the default.
    /// A background inspection gives way to foreground work on the record with TryAgain.
    pub(super) async fn inspect_record_with_fanout(
        &self,
        record_key: RecordKey,
        subkeys: ValueSubkeyRangeSet,
        scope: DHTReportScope,
        fanout_config: FanoutConfig,
        priority: RecordLockPriority,
    ) -> VeilidAPIResult<DHTRecordReport> {
        let Ok(_guard) = self.startup_lock.enter() else {
            apibail_not_initialized!();
        };

        let opaque_record_key = record_key.opaque();

        let subkeys = if subkeys.is_empty() {
            ValueSubkeyRangeSet::full()
        } else {
            subkeys
        };

        // Local scope skips peek_lock; inner RecordStore lock keeps local reads consistent
        let is_local_scope = match scope {
            DHTReportScope::Local => true,
            DHTReportScope::SyncGet
            | DHTReportScope::SyncSet
            | DHTReportScope::UpdateGet
            | DHTReportScope::UpdateSet => false,
        };

        let opt_peek_lock = if is_local_scope {
            None
        } else {
            Some(
                self.record_lock_table
                    .peek_lock(opaque_record_key.clone(), priority)
                    .measure_debug(
                        TimestampDuration::new_ms(200),
                        veilid_log_dbg!(self, "StorageManager::inspect_record lock"),
                    )
                    .await,
            )
        };

        let (safety_selection, offline_subkey_writes) = {
            let inner = self.inner.lock();
            let Some(opened_record) = inner.opened_records.get(&opaque_record_key) else {
                apibail_invalid_argument!("record not open", "record_key", opaque_record_key);
            };
            let safety_selection = opened_record.safety_selection();

            let in_flight_transaction = inner
                .outbound_transaction_manager
                .get_transaction_by_record(&opaque_record_key)
                .is_some();

            let purpose_state = self
                .record_lock_table
                .get_record_lock_purpose_state(&opaque_record_key);

            let mut active_subkey_writes = ValueSubkeyRangeSet::new();
            let mut transaction_record_lock = false;
            if let Some(p) = &purpose_state.whole_record_lock_purpose {
                match p {
                    StorageManagerRecordLockPurpose::TransactBegin
                    | StorageManagerRecordLockPurpose::TransactExtend
                    | StorageManagerRecordLockPurpose::TransactEndAndCommit
                    | StorageManagerRecordLockPurpose::TransactRollback
                    | StorageManagerRecordLockPurpose::TransactDrop => {
                        transaction_record_lock = true;
                        if !in_flight_transaction {
                            veilid_log!(self debug
                                "whole-record lock purpose {:?} on {} without an in-flight transaction",
                                p, opaque_record_key);
                        }
                    }
                    StorageManagerRecordLockPurpose::Create
                    | StorageManagerRecordLockPurpose::Open
                    | StorageManagerRecordLockPurpose::Close
                    | StorageManagerRecordLockPurpose::Delete
                    | StorageManagerRecordLockPurpose::Watch
                    | StorageManagerRecordLockPurpose::WatchUpdate
                    | StorageManagerRecordLockPurpose::BackgroundWatch => {}
                }
            }

            if in_flight_transaction || transaction_record_lock {
                active_subkey_writes = ValueSubkeyRangeSet::full();
            } else if purpose_state.whole_record_lock_purpose.is_none() {
                for (subkey, purpose) in purpose_state.subkey_lock_purpose.into_iter() {
                    match purpose {
                        StorageManagerSubkeyLockPurpose::TransactSet => {
                            if !in_flight_transaction {
                                veilid_log!(self debug
                                    "subkey lock purpose TransactSet on {}/{} without an in-flight transaction",
                                    opaque_record_key, subkey);
                            }
                            active_subkey_writes.insert(subkey);
                        }
                        StorageManagerSubkeyLockPurpose::TransactGet => {
                            if !in_flight_transaction {
                                veilid_log!(self debug
                                    "subkey lock purpose TransactGet on {}/{} without an in-flight transaction",
                                    opaque_record_key, subkey);
                            }
                        }
                        StorageManagerSubkeyLockPurpose::Set
                        | StorageManagerSubkeyLockPurpose::BackgroundSet => {
                            active_subkey_writes.insert(subkey);
                        }
                        StorageManagerSubkeyLockPurpose::Get
                        | StorageManagerSubkeyLockPurpose::BackgroundGet => {}
                    }
                }
            }

            let offline_subkey_writes = inner
                .offline_subkey_writes
                .get(&opaque_record_key)
                .map(|o| o.subkeys.union(&o.subkeys_in_flight))
                .unwrap_or_default()
                .union(&active_subkey_writes)
                .intersect(&subkeys);
            (safety_selection, offline_subkey_writes)
        };

        let mut local_inspect_result = self
            .handle_inspect_local_values(opaque_record_key.clone(), subkeys.clone(), true)
            .await?;

        // If this is the maximum scope we're interested in, return the report
        if matches!(scope, DHTReportScope::Local) {
            return DHTRecordReport::try_new(
                local_inspect_result.subkeys().clone(),
                offline_subkey_writes,
                local_inspect_result.seqs().to_vec(),
                vec![ValueSeqNum::NONE; local_inspect_result.seqs().len()],
            )
            .inspect_err(|e| {
                veilid_log!(self error "invalid record report generated: {}", e);
            });
        }

        // Get rpc processor and drop mutex so we don't block while getting the value from the network
        if !self.dht_is_online() {
            apibail_try_again!("offline, try again later");
        };

        // If we're simulating a set, increase the previous sequence number we have by 1
        if matches!(scope, DHTReportScope::UpdateSet) {
            for seq in local_inspect_result.seqs_mut() {
                *seq = seq.saturating_next();
            }
        }

        // Get the inspect record report from the network
        let outbound_inspect = self.outbound_inspect_value(
            &opaque_record_key,
            subkeys,
            safety_selection,
            if matches!(scope, DHTReportScope::SyncGet | DHTReportScope::SyncSet) {
                InspectResult::default()
            } else {
                local_inspect_result.clone()
            },
            matches!(scope, DHTReportScope::UpdateSet | DHTReportScope::SyncSet),
            InspectFanoutStop::SubkeyConsensus,
            fanout_config,
        );
        let opt_outbound_inspect_result = match &opt_peek_lock {
            Some(peek_lock) => peek_lock.until_preempted(outbound_inspect).await,
            None => Some(outbound_inspect.await),
        };
        let Some(outbound_inspect_result) = opt_outbound_inspect_result else {
            apibail_try_again!("inspect pre-empted by foreground work");
        };
        let outbound_inspect_result = outbound_inspect_result?;

        // Every node answers an inspect, holding the record or not, so silence is our network
        if outbound_inspect_result.answered == 0 {
            apibail_try_again!("no node answered the inspect");
        }

        // Pairs with the watch route diagnosis on a productive change inspect
        #[cfg(feature = "verbose-tracing")]
        if !outbound_inspect_result.answered_safety_routes.is_empty() {
            let srs = outbound_inspect_result
                .answered_safety_routes
                .iter()
                .map(ToString::to_string)
                .collect::<Vec<_>>()
                .join(",");
            veilid_log!(self debug "InspectValue answered via safety routes [{}] ({})", srs, record_key);
        }

        // Keep the list of nodes that returned a value for later reference
        let results_iter = outbound_inspect_result
            .inspect_result
            .subkeys()
            .iter()
            .map(ValueSubkeyRangeSet::single)
            .zip(outbound_inspect_result.subkey_fanout_results);

        let existed = self.process_fanout_results(
            opaque_record_key.clone(),
            results_iter,
            false,
            self.config().internal().network.dht.consensus_width as usize,
        )?;

        if !existed {
            apibail_internal!(
                "record was locked for inspect but is now missing: {}",
                opaque_record_key
            );
        }

        if outbound_inspect_result.inspect_result.subkeys().is_empty() {
            DHTRecordReport::try_new(
                local_inspect_result.subkeys().clone(),
                offline_subkey_writes,
                local_inspect_result.seqs().to_vec(),
                vec![ValueSeqNum::NONE; local_inspect_result.seqs().len()],
            )
        } else {
            DHTRecordReport::try_new(
                outbound_inspect_result.inspect_result.subkeys().clone(),
                offline_subkey_writes,
                local_inspect_result.seqs().to_vec(),
                outbound_inspect_result.inspect_result.seqs().to_vec(),
            )
        }
    }

    ////////////////////////////////////////////////////////////////////////

    /// Perform a 'inspect value' query on the network
    /// Performs the work without a transaction
    #[cfg_attr(
        feature = "instrument",
        instrument(level = "trace", target = "dht", skip_all, err, fields(__VEILID_LOG_KEY = self.log_key()))
    )]
    #[expect(clippy::too_many_arguments)]
    pub(super) async fn outbound_inspect_value(
        &self,
        opaque_record_key: &OpaqueRecordKey,
        subkeys: ValueSubkeyRangeSet,
        safety_selection: SafetySelection,
        local_inspect_result: InspectResult,
        use_set_scope: bool,
        stop: InspectFanoutStop,
        fanout_config: FanoutConfig,
    ) -> VeilidAPIResult<OutboundInspectValueResult> {
        let routing_domain = RoutingDomain::PublicInternet;
        let requested_subkeys = subkeys.clone();

        // Get the DHT parameters for 'InspectValue'
        // Can use either 'get scope' or 'set scope' depending on the purpose of the inspection
        let (node_count, consensus_count, consensus_width, fanout_tasks, timeout) = if use_set_scope
        {
            let config = self.config();
            (
                config.internal().network.dht.max_find_node_count as usize,
                config.internal().network.dht.set_value_count as usize,
                config.internal().network.dht.consensus_width as usize,
                config.internal().network.dht.set_value_fanout as usize,
                TimestampDuration::from(ms_to_us(
                    config.internal().network.dht.set_value_timeout_ms,
                )),
            )
        } else {
            let config = self.config();
            (
                config.internal().network.dht.max_find_node_count as usize,
                config.internal().network.dht.get_value_count as usize,
                config.internal().network.dht.consensus_width as usize,
                config.internal().network.dht.get_value_fanout as usize,
                TimestampDuration::from(ms_to_us(
                    config.internal().network.dht.get_value_timeout_ms,
                )),
            )
        };

        // Seed the fanout with the value's holders, and a peerless one with our watch's nodes too
        let mut value_nodes = self.get_value_nodes(opaque_record_key)?.unwrap_or_default();
        if fanout_config.max_peers == 0 {
            for node_ref in self.get_outbound_watch_node_refs(opaque_record_key) {
                if !value_nodes.contains(&node_ref) {
                    value_nodes.push(node_ref);
                }
            }
        }
        let init_fanout_queue = value_nodes
            .into_iter()
            .filter(|x| {
                x.node_info(routing_domain)
                    .map(|ni| ni.has_all_capabilities(&[VEILID_CAPABILITY_DHT]))
                    .unwrap_or_default()
            })
            .collect();

        // Make operation context
        let opt_descriptor_info = if let Some(descriptor) = local_inspect_result.opt_descriptor() {
            // Get the descriptor info. This also truncates the subkeys list to what can be returned from the network.
            Some(InspectDescriptorInfo::try_new(descriptor, &subkeys)?)
        } else {
            None
        };

        let context = Arc::new(Mutex::new(OutboundInspectValueContext {
            seqcounts: local_inspect_result
                .seqs()
                .iter()
                .map(|s| SubkeySeqCount {
                    seq: *s,
                    consensus_nodes: vec![],
                    value_nodes: vec![],
                })
                .collect(),
            opt_descriptor_info,
            convergence: 0,
            checked_convergence: 0,
            answered: 0,
            #[cfg(feature = "verbose-tracing")]
            answered_safety_routes: BTreeSet::new(),
        }));

        // Routine to call to generate fanout
        let call_routine = {
            let context = context.clone();
            let registry = self.registry();
            let opaque_record_key = opaque_record_key.clone();
            let safety_selection = safety_selection.clone();
            Arc::new(
                move |next_node: NodeRef| -> PinBoxFutureStatic<FanoutCallResult> {
                    let context = context.clone();
                    let registry = registry.clone();
                    let subkeys = subkeys.clone();
                    let opaque_record_key = opaque_record_key.clone();
                    let safety_selection = safety_selection.clone();
                    let fut = async move {
                        let rpc_processor = registry.rpc_processor();

                        let descriptor_mode = GetDescriptorMode::new(
                            context
                                .lock()
                                .opt_descriptor_info
                                .as_ref()
                                .map(|x| x.descriptor.clone()),
                        );

                        let iva = match rpc_processor
                            .rpc_call_inspect_value(
                                Destination::direct(
                                    next_node.routing_domain_filtered(routing_domain),
                                    Some(safety_selection),
                                ),
                                opaque_record_key.clone(),
                                subkeys.clone(),
                                descriptor_mode,
                                fanout_config,
                            )
                            .await?
                        {
                            NetworkResult::Timeout => {
                                return Ok(FanoutCallOutput {
                                    peer_info_list: vec![],
                                    disposition: FanoutCallDisposition::Timeout,
                                });
                            }
                            NetworkResult::ServiceUnavailable(_)
                            | NetworkResult::NoConnection(_)
                            | NetworkResult::AlreadyExists(_)
                            | NetworkResult::InvalidMessage(_) => {
                                return Ok(FanoutCallOutput {
                                    peer_info_list: vec![],
                                    disposition: FanoutCallDisposition::Invalid,
                                });
                            }
                            NetworkResult::Value(v) => v,
                        };

                        let answer = iva.answer;

                        // Check if we got an accepted result
                        if !answer.accepted {
                            context.lock().answered += 1;
                            // Return peers if we have some
                            veilid_log!(registry debug target:"network_result", "InspectValue missed, fanout call returned peers {}", answer.peers.len());
                            return Ok(FanoutCallOutput {
                                peer_info_list: answer.peers,
                                disposition: FanoutCallDisposition::Rejected,
                            });
                        }

                        #[cfg(feature = "verbose-tracing")]
                        if let Some(sr) = &iva
                            .answer_context
                            .waitable_reply_context
                            .opt_safety_route_ref
                        {
                            context
                                .lock()
                                .answered_safety_routes
                                .insert(sr.route_key().clone());
                        }

                        // Keep the descriptor if we got one. If we had a last_descriptor it will
                        // already be validated by rpc_call_inspect_value
                        if let Some(descriptor) = answer.descriptor {
                            let mut ctx = context.lock();
                            if ctx.opt_descriptor_info.is_none() {
                                // Get the descriptor info. This also truncates the subkeys list to what can be returned from the network.
                                let descriptor_info = match InspectDescriptorInfo::try_new(
                                    Arc::new(descriptor.clone()),
                                    &subkeys,
                                ) {
                                    Ok(v) => v,
                                    Err(e) => {
                                        veilid_log!(registry debug target:"network_result", "InspectValue returned an invalid descriptor: {}", e);
                                        return Ok(FanoutCallOutput {
                                            peer_info_list: vec![],
                                            disposition: FanoutCallDisposition::Invalid,
                                        });
                                    }
                                };
                                ctx.opt_descriptor_info = Some(descriptor_info);
                            }
                        }

                        // Keep the value if we got one and it is newer and it passes schema validation
                        if answer.seqs.is_empty() {
                            context.lock().answered += 1;
                            veilid_log!(registry debug target:"network_result", "InspectValue returned no seq, fanout call returned peers {}", answer.peers.len());
                            return Ok(FanoutCallOutput {
                                peer_info_list: answer.peers,
                                disposition: FanoutCallDisposition::Rejected,
                            });
                        }

                        veilid_log!(registry debug target:"network_result", "Got seqs back: len={}", answer.seqs.len());
                        let mut ctx = context.lock();

                        // Ensure we have a schema and descriptor etc
                        let Some(descriptor_info) = &ctx.opt_descriptor_info else {
                            // Got a value but no descriptor for it
                            // Move to the next node
                            veilid_log!(registry debug target:"network_result", "InspectValue returned a value with no descriptor invalid descriptor");
                            return Ok(FanoutCallOutput {
                                peer_info_list: vec![],
                                disposition: FanoutCallDisposition::Invalid,
                            });
                        };

                        // Get number of subkeys from schema and ensure we are getting the
                        // right number of sequence numbers betwen that and what we asked for
                        #[allow(clippy::unnecessary_cast)]
                        if answer.seqs.len() as u64 != descriptor_info.subkeys.len() as u64 {
                            // Not the right number of sequence numbers
                            // Move to the next node
                            veilid_log!(registry debug target:"network_result", "wrong number of seqs returned {} (wanted {})",
                                answer.seqs.len(),
                                descriptor_info.subkeys.len());
                            return Ok(FanoutCallOutput {
                                peer_info_list: vec![],
                                disposition: FanoutCallDisposition::Invalid,
                            });
                        }

                        // If we have a prior seqs list, merge in the new seqs
                        if ctx.seqcounts.is_empty() {
                            ctx.seqcounts = answer
                                .seqs
                                .iter()
                                .map(|s| SubkeySeqCount {
                                    seq: *s,
                                    // Only a returned seq is evidence; a subkey this node
                                    // had nothing for must not count toward its consensus
                                    consensus_nodes: if s.is_some() {
                                        vec![next_node.clone()]
                                    } else {
                                        vec![]
                                    },
                                    value_nodes: vec![next_node.clone()],
                                })
                                .collect();
                            ctx.convergence += 1;
                        } else {
                            if ctx.seqcounts.len() != answer.seqs.len() {
                                veilid_log!(registry debug target:"network_result", "seqs list length should always be equal by now: {} (wanted {})",
                                    answer.seqs.len(),
                                    ctx.seqcounts.len());
                                return Ok(FanoutCallOutput {
                                    peer_info_list: vec![],
                                    disposition: FanoutCallDisposition::Invalid,
                                });
                            }
                            let mut convergence = 0usize;
                            for (subkey_index, pair) in
                                ctx.seqcounts.iter_mut().zip(answer.seqs.iter()).enumerate()
                            {
                                let ctx_seqcnt = pair.0;
                                let answer_seq = *pair.1;

                                // If we already have consensus for this subkey, don't bother updating it any more
                                // While we may find a better sequence number if we keep looking, this does not mimic the behavior
                                // of get and set unless we stop here
                                if ctx_seqcnt.consensus_nodes.len() >= consensus_count {
                                    // Unlike get, inspect has no AcceptedNewer restart, so a newer
                                    // seq dropped here is a change the watch fallback never sees
                                    if answer_seq > ctx_seqcnt.seq {
                                        veilid_log!(registry debug
                                            "InspectValue discarded a newer seq after consensus: {} subkey_index={} consensus_seq={} discarded_seq={} node={}",
                                            opaque_record_key,
                                            subkey_index,
                                            ctx_seqcnt.seq,
                                            answer_seq,
                                            next_node
                                        );
                                    }
                                    continue;
                                }

                                // If the new seq isn't undefined and is better than the old seq (either greater or old is undefined)
                                // Then take that sequence number and note that we have gotten newer sequence numbers so we keep
                                // looking for consensus
                                // If the sequence number matches the old sequence number, then we keep the value node for reference later
                                if answer_seq.is_some() {
                                    if answer_seq > ctx_seqcnt.seq {
                                        // One node has shown us the latest sequence numbers so far
                                        ctx_seqcnt.seq = answer_seq;
                                        ctx_seqcnt.consensus_nodes = vec![next_node.clone()];
                                        convergence += 1;
                                    } else if answer_seq == ctx_seqcnt.seq {
                                        // Keep the nodes that showed us the latest values
                                        ctx_seqcnt.consensus_nodes.push(next_node.clone());
                                        convergence += 1;
                                    }
                                }
                                ctx_seqcnt.value_nodes.push(next_node.clone());
                            }
                            ctx.convergence += convergence;
                        }

                        ctx.answered += 1;

                        // Return peers if we have some
                        veilid_log!(registry debug target:"network_result", "InspectValue fanout call returned peers {}", answer.peers.len());

                        // Inspect doesn't actually use the fanout queue consensus tracker
                        Ok(FanoutCallOutput {
                            peer_info_list: answer.peers,
                            disposition: FanoutCallDisposition::Accepted,
                        })
                    };
                    #[cfg(feature = "instrument")]
                    let fut = fut.instrument(tracing::trace_span!(
                        target: "dht",
                        "outbound_inspect_value fanout call"
                    ));
                    Box::pin(fut) as PinBoxFuture<FanoutCallResult>
                },
            )
        };

        // Routine to call to check if we're done at each step.
        // Seqs need consensus per subkey, which the FanoutCall does not track, so that
        // case uses our own; the descriptor needs no consensus at all
        let check_done = {
            let context = context.clone();
            Arc::new(move |_: &FanoutResult| {
                let mut ctx = context.lock();

                match stop {
                    InspectFanoutStop::Descriptor => descriptor_fanout_done(
                        ctx.opt_descriptor_info.is_some(),
                        !ctx.seqcounts.is_empty(),
                    ),
                    InspectFanoutStop::SubkeyConsensus => {
                        let stalled = ctx.convergence == ctx.checked_convergence;
                        ctx.checked_convergence = ctx.convergence;

                        inspect_fanout_done(
                            ctx.seqcounts.iter().map(|cs| cs.consensus_nodes.len()),
                            consensus_count,
                            ctx.opt_descriptor_info.is_some(),
                            stalled,
                        )
                    }
                }
            })
        };

        // Call the fanout
        let routing_table = self.routing_table();
        let hash_coordinate = opaque_record_key.to_hash_coordinate();
        let mut fanout_call = FanoutCall::new(
            &routing_table,
            FanoutCallParams {
                name: format!("outbound_inspect_value({})", Timestamp::now_increasing()),
                hash_coordinate,
                node_count,
                fanout_tasks,
                consensus_count,
                consensus_width,
                timeout,
            },
            capability_fanout_peer_info_filter(vec![VEILID_CAPABILITY_DHT]),
            call_routine,
            check_done,
        );
        fanout_call
            .set_demote_routed_unresponsive(matches!(safety_selection, SafetySelection::Safe(_)));
        // With no peers coming back, only the nodes we seeded can answer, so ask them first
        fanout_call.set_prefer_init_nodes(fanout_config.max_peers == 0);

        let shutdown_stop_source = self
            .startup_lock
            .stop_token()
            .ok_or_else(VeilidAPIError::not_initialized)?;
        let fanout_result = fanout_call
            .run(
                init_fanout_queue,
                FanoutQueueMode::Unthrottled,
                shutdown_stop_source,
            )
            .await?;

        let ctx = context.lock();
        let mut subkey_fanout_results = vec![];
        for cs in &ctx.seqcounts {
            let has_consensus = cs.consensus_nodes.len() >= consensus_count;
            let kind = if has_consensus {
                FanoutResultKind::Consensus
            } else if cs.consensus_nodes.is_empty()
                && matches!(fanout_result.kind, FanoutResultKind::Consensus)
            {
                // No node answered for this subkey, so its absence is unconfirmed
                FanoutResultKind::Exhausted
            } else {
                fanout_result.kind
            };
            let subkey_fanout_result = FanoutResult {
                kind,
                consensus_nodes: cs.consensus_nodes.clone(),
                value_nodes: cs.value_nodes.clone(),
            };
            subkey_fanout_results.push(subkey_fanout_result);
        }

        if subkey_fanout_results.len() == 1 {
            veilid_log!(self debug "InspectValue Fanout: {:#}\n{:#}", fanout_result, subkey_fanout_results.first().unwrap_or_log());
        } else {
            veilid_log!(self debug "InspectValue Fanout: {:#}:\n{}", fanout_result, debug_fanout_results(&subkey_fanout_results));
        }

        let result = OutboundInspectValueResult {
            subkey_fanout_results,
            answered: ctx.answered,
            #[cfg(feature = "verbose-tracing")]
            answered_safety_routes: ctx.answered_safety_routes.iter().cloned().collect(),
            inspect_result: InspectResult::try_new(
                self,
                requested_subkeys,
                "outbound_inspect_value",
                ctx.opt_descriptor_info
                    .as_ref()
                    .map(|d| d.subkeys.clone())
                    .unwrap_or_default(),
                ctx.seqcounts.iter().map(|cs| cs.seq).collect(),
                ctx.opt_descriptor_info
                    .as_ref()
                    .map(|d| d.descriptor.clone()),
            )?,
        };

        #[allow(clippy::unnecessary_cast)]
        {
            if result.inspect_result.subkeys().len() as u64
                != result.subkey_fanout_results.len() as u64
            {
                veilid_log!(self error "mismatch between subkeys returned and fanout results returned: {}!={}", result.inspect_result.subkeys().len(), result.subkey_fanout_results.len());
                apibail_internal!("subkey and fanout list length mismatched");
            }
        }

        Ok(result)
    }

    /// Handle a received 'Inspect Value' query
    #[cfg_attr(
        feature = "instrument",
        instrument(level = "trace", target = "dht", skip_all, fields(__VEILID_LOG_KEY = self.log_key()))
    )]
    pub async fn inbound_inspect_value(
        &self,
        opaque_record_key: &OpaqueRecordKey,
        subkeys: ValueSubkeyRangeSet,
        want_descriptor: bool,
    ) -> VeilidAPIResult<NetworkResult<InboundInspectValueResult>> {
        let subkeys = if subkeys.is_empty() {
            ValueSubkeyRangeSet::full()
        } else {
            subkeys
        };

        // See if the subkey we are getting has a last known remote value
        let remote_record_store = self.get_remote_record_store()?;
        let inspect_result = remote_record_store
            .inspect_record(opaque_record_key, &subkeys, want_descriptor)
            .await?
            .unwrap_or_default();

        Ok(NetworkResult::value(InboundInspectValueResult::Success(
            inspect_result,
        )))
    }
}
