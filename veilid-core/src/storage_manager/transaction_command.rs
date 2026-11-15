use super::*;

impl_veilid_log_facility!("stor");

/// A single node entry for an outbound transact command
#[derive(Debug, Clone)]
pub(in crate::storage_manager) struct OutboundTransactCommandNode {
    pub lnxid: LocalNodeTransactionId,
    pub rnxid: RemoteNodeTransactionId,
    pub node_ref: NodeRef,
}

pub(in crate::storage_manager) type OutboundTransactCommandNodes = Vec<OutboundTransactCommandNode>;

/// parameters required to perform a command on a transaction
#[derive(Debug, Clone)]
pub(in crate::storage_manager) struct OutboundTransactCommandParams {
    /// The record key being transacted
    pub opaque_record_key: OpaqueRecordKey,
    /// The safety selection used with the transaction
    pub safety_selection: SafetySelection,
    /// Nodes and transaction ids to use
    pub nodes: OutboundTransactCommandNodes,
    /// The command to execute on each node
    pub command: TransactCommand,
    /// Parameter for the command (sequence numbers)
    pub opt_seqs: Option<Vec<ValueSeqNum>>,
    /// Parameter for the command (subkey number)
    pub opt_subkey: Option<ValueSubkey>,
    /// Parameter for the command (value)
    pub opt_value: Option<Arc<SignedValueData>>,
    /// The number of valid responses required for strict consensus
    pub required_strict_consensus_count: usize,
    /// Nodes already counted as valid before any RPCs run (e.g. Commit's auto-promoted set)
    pub pre_authorized_valid_count: usize,
    /// Latest time an answer can still count, bounding each node's wait
    pub opt_deadline: Option<Timestamp>,
    /// Nodes already holding the value being set, answered as equal without an RPC
    pub pre_satisfied: LocalNodeTransactionIdSet,
}

/// Disposition of a per-node transaction command result
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(in crate::storage_manager) enum TransactCommandDisposition {
    /// The node responded and the transaction is still valid
    Valid,
    /// The node responded but the transaction is no longer valid
    Invalid,
    /// The node did not answer within the RPC timeout
    Skipped,
    /// Still in flight when the command returned; the answer is collected in the background
    Pending,
}

/// The result of the outbound_transact_command operation
#[derive(Debug)]
pub(in crate::storage_manager) struct OutboundTransactCommandPerNodeResult {
    /// The local node transaction id this is for
    pub lnxid: LocalNodeTransactionId,
    /// The remote node transaction id this is for
    pub rnxid: RemoteNodeTransactionId,
    /// The disposition of this node's result
    pub disposition: TransactCommandDisposition,
    /// Return from the command (sequence numbers)
    #[expect(dead_code)]
    pub opt_seqs: Option<Vec<ValueSeqNum>>,
    /// Return from the command (subkey number)
    pub opt_subkey: Option<ValueSubkey>,
    /// Return from the command (value)
    pub opt_value: Option<Arc<SignedValueData>>,
    /// Updated expiration to apply
    pub opt_expiration: Option<Timestamp>,
    /// Measured RTT for this RPC, present only on successful responses
    pub opt_rtt: Option<TimestampDuration>,
}

/// The result of the outbound_transact_command operation
#[derive(Debug)]
pub(in crate::storage_manager) struct OutboundTransactCommandResult {
    /// Copy of the params used to produce these results
    pub params: Arc<OutboundTransactCommandParams>,
    /// The results per node transaction, in closest-to-the-record-key sorted order
    pub per_node_results: Vec<OutboundTransactCommandPerNodeResult>,
}

impl OutboundTransactCommandResult {
    pub fn get_command_lnxids(&self) -> HashSet<LocalNodeTransactionId> {
        self.params
            .nodes
            .iter()
            .map(|x| x.lnxid)
            .collect::<HashSet<_>>()
    }
}

/// The result of the inbound_transact_command operation
#[derive(Clone, Debug)]
pub(crate) enum InboundTransactCommandResult {
    /// Value transacted successfully
    Success(TransactCommandSuccess),
    /// Transaction not valid
    InvalidTransaction,
    /// Invalid arguments
    InvalidArguments,
}

/// The result of a single successful transaction command
#[derive(Default, Debug, Clone)]
pub(crate) struct TransactCommandSuccess {
    /// Expiration timestamp
    pub expiration: Timestamp,
    /// Sequence numbers
    pub opt_seqs: Option<Vec<ValueSeqNum>>,
    /// Subkey
    pub opt_subkey: Option<ValueSubkey>,
    /// Value
    pub opt_value: Option<Arc<SignedValueData>>,
}

/// End collects every voter: Commit targets End-stage nodes, so the plan's spare
/// voters and the commit rounds are the margin a lost Commit answer has
pub(in crate::storage_manager) fn exits_at_consensus(command: TransactCommand) -> bool {
    command != TransactCommand::End
}

impl StorageManager {
    ////////////////////////////////////////////////////////////////////////

    /// Perform transact command queries on the network for a single record.
    ///
    /// Fans out transact commands to all nodes concurrently via FuturesUnordered.
    /// Results are collected as they complete. Early exit once strict consensus is reached,
    /// except for End, which waits for every voter.
    #[cfg_attr(
        feature = "instrument",
        instrument(level = "trace", target = "dht", skip_all, err,
            fields(
                command = ?params.command,
                nodes = params.nodes.len(),
                pre_authorized = params.pre_authorized_valid_count,
            ))
    )]
    pub(in crate::storage_manager) async fn outbound_transact_command(
        &self,
        params: Arc<OutboundTransactCommandParams>,
    ) -> VeilidAPIResult<OutboundTransactCommandResult> {
        let cmd = params.command;
        let key = params.opaque_record_key.clone();
        let nodes_len = params.nodes.len();
        let required = params.required_strict_consensus_count;

        DurationRecorder::new("outbound_transact_command", |name, start| {
            veilid_log!(self debug
                "{}[start={:#}](command: {:?}, key: {}, nodes: {}, required: {})",
                name, start, cmd, key, nodes_len, required);
        })
        .record_fut(
            self.outbound_transact_command_inner(params),
            |name, start, dur, ret| {
                let summary = match &ret {
                    Ok(r) => {
                        let valid = r
                            .per_node_results
                            .iter()
                            .filter(|p| matches!(p.disposition, TransactCommandDisposition::Valid))
                            .count();
                        format!("Ok(valid: {}/{})", valid, required)
                    }
                    Err(e) => format!("Err({})", e),
                };
                veilid_log!(self debug
                "{}[start={:#} dur={:#}](command: {:?}, key: {}, {})",
                name, start, dur, cmd, key, summary);
                ret
            },
        )
        .await
    }

    async fn outbound_transact_command_inner(
        &self,
        params: Arc<OutboundTransactCommandParams>,
    ) -> VeilidAPIResult<OutboundTransactCommandResult> {
        let routing_domain = RoutingDomain::PublicInternet;

        // Pull the descriptor for this record
        let descriptor = {
            let local_record_store = self.get_local_record_store()?;
            local_record_store
                .with_record(&params.opaque_record_key, |record| record.descriptor())?
                .ok_or_else(|| VeilidAPIError::internal("record does not exist in transaction"))?
        };
        let schema = descriptor.schema()?;

        #[cfg(feature = "verbose-tracing")]
        let total_nodes = params.nodes.len();

        #[cfg(feature = "stats")]
        let cmd_start = Timestamp::now();
        #[cfg(feature = "stats")]
        let mut outcome = CommandOutcome::default();

        #[cfg(feature = "verbose-tracing")]
        let diag_start = {
            veilid_log!(self debug "TransactCmd start: cmd={} key={}{} nodes={} required_consensus={}",
                params.command,
                params.opaque_record_key,
                if let Some(subkey) = params.opt_subkey { format!(" #{}", subkey) } else { "".to_string() },
                total_nodes,
                params.required_strict_consensus_count,
            );
            Timestamp::now()
        };

        let mut per_node_results: Vec<OutboundTransactCommandPerNodeResult> = Vec::new();
        // Seed with auto-promoted nodes (e.g. Commit's no-op set); the fanout
        // can then trip consensus on fewer RPC responses.
        let mut valid_response_count = params.pre_authorized_valid_count;
        let nodes_to_send = &params.nodes;
        let mut pending_lnxids: HashSet<LocalNodeTransactionId> = HashSet::new();

        // Wait the full rpc timeout per node, or to the command's deadline if sooner;
        // consensus early-exit below ends the wait sooner, and a shorter bound drops
        // in-flight answers under load
        let rpc_timeout =
            TimestampDuration::new_ms(self.config().internal().network.rpc.timeout_ms.into());
        let per_node_timeout = params.opt_deadline.map_or(rpc_timeout, |deadline| {
            rpc_timeout.min(deadline.duration_since(Timestamp::now()))
        });

        // Best-effort cleanup (rollback, required==0): fire RPCs in the background so
        // the records lock isn't held on a dead node's full timeout.
        let fire_and_forget = params.required_strict_consensus_count == 0;
        let mut unord = FuturesUnordered::new();
        let mut in_flight: HashSet<LocalNodeTransactionId> = nodes_to_send
            .iter()
            .filter(|n| !params.pre_satisfied.contains(&n.lnxid))
            .map(|n| n.lnxid)
            .collect();

        veilid_stats!(outcome.rpcs_sent += in_flight.len() as u64);

        for n in nodes_to_send {
            // A node already at this value answers as an equal-seq holder would
            if params.pre_satisfied.contains(&n.lnxid) {
                per_node_results.push(OutboundTransactCommandPerNodeResult {
                    lnxid: n.lnxid,
                    rnxid: n.rnxid.clone(),
                    disposition: TransactCommandDisposition::Valid,
                    opt_seqs: None,
                    opt_subkey: params.opt_subkey,
                    opt_value: params.opt_value.clone(),
                    opt_expiration: None,
                    opt_rtt: None,
                });
                valid_response_count += 1;
                continue;
            }
            let registry = self.registry();
            let lnxid = n.lnxid;
            let rnxid = n.rnxid.clone();
            let node_ref = n.node_ref.clone();
            let descriptor = descriptor.clone();
            let schema = schema.clone();
            let params = params.clone();
            #[cfg(feature = "instrument")]
            let command = params.command;
            let node_start = Timestamp::now();

            let fut = async move {
                let rpc_processor = registry.rpc_processor();

                let result = rpc_processor
                    .rpc_call_transact_command(
                        Destination::direct(
                            node_ref.routing_domain_filtered(routing_domain),
                            Some(params.safety_selection.clone()),
                        ),
                        params.opaque_record_key.clone(),
                        descriptor.clone(),
                        rnxid.xid(),
                        params.command,
                        params.opt_seqs.clone(),
                        params.opt_subkey,
                        params.opt_value.clone(),
                    )
                    .await
                    .map_err(VeilidAPIError::from)?;

                let node_elapsed = Timestamp::now().duration_since(node_start);

                match result {
                    NetworkResult::Timeout | NetworkResult::NoConnection(_) => {
                        #[cfg(feature = "verbose-tracing")]
                        {
                            let dial_info_str = node_ref
                                .node_info(routing_domain)
                                .map(|ni| {
                                    let dids = ni.dial_info_detail_list();
                                    if dids.is_empty() {
                                        "no_dialinfo".to_string()
                                    } else {
                                        dids.iter()
                                            .map(|d| format!("{}", d))
                                            .collect::<Vec<_>>()
                                            .join(",")
                                    }
                                })
                                .unwrap_or_else(|| "no_nodeinfo".to_string());
                            let caps = node_ref
                                .node_info(routing_domain)
                                .map(|ni| format!("{:?}", ni.capabilities()))
                                .unwrap_or_default();
                            veilid_log!(registry debug "TransactCmd node NO_RESPONSE: cmd={} key={}{} node={} elapsed={} dial_info=[{}] caps={}",
                                params.command, params.opaque_record_key,
                                if let Some(subkey) = params.opt_subkey { format!(" #{}", subkey) } else { "".to_string() },
                                node_ref, node_elapsed,
                                dial_info_str, caps);
                        }
                        VeilidAPIResult::Ok(None)
                    }
                    NetworkResult::ServiceUnavailable(_)
                    | NetworkResult::AlreadyExists(_)
                    | NetworkResult::InvalidMessage(_) => {
                        #[cfg(feature = "verbose-tracing")]
                        veilid_log!(registry debug "TransactCmd node result: cmd={} key={}{} node={} result=SERVICE_ERROR elapsed={}",
                            params.command, params.opaque_record_key,
                            if let Some(subkey) = params.opt_subkey { format!(" #{}", subkey) } else { "".to_string() },
                            node_ref, node_elapsed,
                        );
                        Ok(None)
                    }
                    NetworkResult::Value(tva) => {
                        // A value outside the schema fails this node, not the transaction
                        if let (Some(subkey), Some(value)) =
                            (tva.answer.opt_subkey, &tva.answer.opt_value)
                        {
                            if registry
                                .storage_manager()
                                .check_subkey_value_data(
                                    &schema,
                                    descriptor.ref_owner(),
                                    subkey,
                                    value.value_data(),
                                )
                                .is_err()
                            {
                                veilid_log!(registry debug "TransactCmd node answered a value outside the schema: cmd={} key={} #{} node={}",
                                    params.command, params.opaque_record_key, subkey, node_ref);
                                return Ok(None);
                            }
                        }
                        let disposition = if tva.answer.transaction_valid {
                            #[cfg(feature = "verbose-tracing")]
                            veilid_log!(registry debug "TransactCmd node result: cmd={} key={}{} node={} result=VALID elapsed={}",
                                params.command, params.opaque_record_key,
                                if let Some(subkey) = params.opt_subkey { format!(" #{}", subkey) } else { "".to_string() },
                                node_ref, node_elapsed,
                            );
                            TransactCommandDisposition::Valid
                        } else {
                            #[cfg(feature = "verbose-tracing")]
                            veilid_log!(registry debug "TransactCmd node INVALID (no longer valid): cmd={} key={}{} node={} elapsed={}",
                                params.command, params.opaque_record_key,
                                if let Some(subkey) = params.opt_subkey { format!(" #{}", subkey) } else { "".to_string() },
                                node_ref, node_elapsed,
                            );
                            TransactCommandDisposition::Invalid
                        };

                        // Only record RTT for Valid responses: Invalid means
                        // the server rejected the transaction, which can be a
                        // fast/cheap response that doesn't reflect real RTT.
                        let opt_rtt = matches!(disposition, TransactCommandDisposition::Valid)
                            .then_some(node_elapsed);

                        Ok(Some(OutboundTransactCommandPerNodeResult {
                            lnxid,
                            rnxid,
                            disposition,
                            opt_seqs: tva.answer.opt_seqs,
                            opt_subkey: tva.answer.opt_subkey,
                            opt_value: tva.answer.opt_value,
                            opt_expiration: tva.answer.opt_expiration,
                            opt_rtt,
                        }))
                    }
                }
            };
            #[cfg(feature = "instrument")]
            let fut = fut.instrument(tracing::trace_span!(
                target: "dht",
                "outbound_transact_command per-node",
                command = ?command,
            ));
            // Bounded per-node wait; a timeout reports no response
            let fut = async move {
                let res = match Box::pin(fut.timeout_duration(per_node_timeout)).await {
                    Ok(inner) => inner,
                    Err(_) => VeilidAPIResult::Ok(None),
                };
                (lnxid, res)
            };
            if fire_and_forget {
                self.background_operation_processor.add_future(async move {
                    let _ = fut.await;
                });
            } else {
                unord.push(Box::pin(fut));
            }
        }
        if fire_and_forget {
            // RPCs fired in the background; no consensus to await
            return Ok(self.finish_transact_command(
                params,
                per_node_results,
                pending_lnxids,
                #[cfg(feature = "stats")]
                outcome,
                #[cfg(feature = "stats")]
                cmd_start,
                valid_response_count,
            ));
        }

        // Collect answers until consensus; stragglers settle in the background
        #[cfg(feature = "verbose-tracing")]
        let mut responded_xids = HashSet::new();

        while let Some((lnxid, result)) = unord.next().await {
            in_flight.remove(&lnxid);
            let opt_pnr = result.inspect_err(|e| {
                veilid_log!(self error target:"network_result",
                    "Error performing transaction command: {}", e);
            })?;

            let Some(pnr) = opt_pnr else {
                // No answer within the wait
                veilid_stats!(outcome.no_response += 1);
                continue;
            };

            if pnr.disposition == TransactCommandDisposition::Valid {
                valid_response_count += 1;
                veilid_stats!(outcome.answered_valid += 1);
            } else {
                veilid_stats!(outcome.answered_invalid += 1);
            }
            #[cfg(feature = "verbose-tracing")]
            responded_xids.insert(pnr.lnxid);
            per_node_results.push(pnr);

            if exits_at_consensus(params.command)
                && valid_response_count >= params.required_strict_consensus_count
            {
                #[cfg(feature = "verbose-tracing")]
                veilid_log!(self debug "TransactCmd consensus reached: cmd={} key={}{} valid={}/{} responded={}/{}",
                    params.command,
                    params.opaque_record_key,
                    if let Some(subkey) = params.opt_subkey { format!(" #{}", subkey) } else { "".to_string() },
                    valid_response_count,
                    params.required_strict_consensus_count,
                    responded_xids.len(),
                    total_nodes,
                );
                break;
            }
        }

        veilid_stats!({
            let n = unord.len() as u64;
            outcome.abandoned_in_flight += n;
            if n > 0 {
                outcome.post_consensus_window_expired = true;
            }
        });
        // In-flight Set answers still settle into node state; other commands drop
        // theirs, and the reply pool completes the question either way
        if params.command == TransactCommand::Set && !unord.is_empty() {
            pending_lnxids.extend(in_flight.drain());
            let registry = self.registry();
            let subkey = params.opt_subkey.unwrap_or_log();
            let value = params.opt_value.clone().unwrap_or_log();
            self.background_operation_processor.add_future(async move {
                while let Some((lnxid, res)) = unord.next().await {
                    registry
                        .storage_manager()
                        .record_late_transact_set_result(lnxid, subkey, &value, res);
                }
            });
        } else {
            drop(unord);
        }

        // Log summary
        #[cfg(feature = "verbose-tracing")]
        {
            let invalid_count = per_node_results
                .iter()
                .filter(|r| r.disposition == TransactCommandDisposition::Invalid)
                .count();
            let responded_count = per_node_results.len();
            let no_response_count = total_nodes.saturating_sub(responded_count);
            let diag_elapsed = Timestamp::now().duration_since(diag_start);
            veilid_log!(self debug "TransactCmd done: cmd={} key={}{} valid={} invalid={} no_response={} skipped={} total={} required={} elapsed={} result={}",
                params.command,
                params.opaque_record_key,
                if let Some(subkey) = params.opt_subkey { format!(" #{}", subkey) } else { "".to_string() },
                valid_response_count,
                invalid_count,
                no_response_count,
                total_nodes.saturating_sub(responded_count),
                total_nodes,
                params.required_strict_consensus_count,
                diag_elapsed,
                if valid_response_count >= params.required_strict_consensus_count { "CONSENSUS" } else { "FAILED" },
            );
        }

        Ok(self.finish_transact_command(
            params,
            per_node_results,
            pending_lnxids,
            #[cfg(feature = "stats")]
            outcome,
            #[cfg(feature = "stats")]
            cmd_start,
            valid_response_count,
        ))
    }

    /// Record the command's outcome and fill in the nodes that never answered:
    /// Pending if their answer is still being collected, Skipped if the wait ran out
    fn finish_transact_command(
        &self,
        params: Arc<OutboundTransactCommandParams>,
        mut per_node_results: Vec<OutboundTransactCommandPerNodeResult>,
        pending_lnxids: HashSet<LocalNodeTransactionId>,
        #[cfg(feature = "stats")] mut outcome: CommandOutcome,
        #[cfg(feature = "stats")] cmd_start: Timestamp,
        valid_response_count: usize,
    ) -> OutboundTransactCommandResult {
        #[cfg(not(feature = "stats"))]
        let _ = valid_response_count;
        veilid_stats!({
            outcome.short_of_required = params.required_strict_consensus_count > 0
                && valid_response_count < params.required_strict_consensus_count;
            outcome.duration = Timestamp::now().duration_since(cmd_start);
            self.transaction_quality()
                .lock()
                .record_command(params.command.into(), outcome);
        });

        let responded_lnxids: HashSet<_> = per_node_results.iter().map(|pnr| pnr.lnxid).collect();
        if responded_lnxids.len() < params.nodes.len() {
            let unanswered: Vec<_> = params
                .nodes
                .iter()
                .filter(|n| !responded_lnxids.contains(&n.lnxid))
                .map(|n| OutboundTransactCommandPerNodeResult {
                    lnxid: n.lnxid,
                    rnxid: n.rnxid.clone(),
                    disposition: if pending_lnxids.contains(&n.lnxid) {
                        TransactCommandDisposition::Pending
                    } else {
                        TransactCommandDisposition::Skipped
                    },
                    opt_seqs: None,
                    opt_subkey: None,
                    opt_value: None,
                    opt_expiration: None,
                    opt_rtt: None,
                })
                .collect();
            per_node_results.extend(unanswered);
        }

        // Sort per node results by distance from record to assist with strict consensus checking
        per_node_results.sort_unstable_by(|a, b| {
            let dist_a = params
                .opaque_record_key
                .to_hash_coordinate()
                .distance(&a.rnxid.node_id().to_hash_coordinate());
            let dist_b = params
                .opaque_record_key
                .to_hash_coordinate()
                .distance(&b.rnxid.node_id().to_hash_coordinate());

            dist_a.cmp(&dist_b)
        });

        OutboundTransactCommandResult {
            params,
            per_node_results,
        }
    }

    /// A Set answer that arrived after its command returned
    fn record_late_transact_set_result(
        &self,
        lnxid: LocalNodeTransactionId,
        subkey: ValueSubkey,
        value: &Arc<SignedValueData>,
        res: VeilidAPIResult<Option<OutboundTransactCommandPerNodeResult>>,
    ) {
        let opt_pnr = match res {
            Ok(opt_pnr) => opt_pnr,
            Err(e) => {
                veilid_log!(self debug "late transaction set answer failed: {}", e);
                None
            }
        };
        let mut inner = self.inner.lock();
        inner
            .outbound_transaction_manager
            .record_late_transact_set_result(lnxid, subkey, value, opt_pnr);
    }

    ////////////////////////////////////////////////////////////////////////

    /// Handle a received 'TransactCommand' query
    #[cfg_attr(feature = "instrument", instrument(level = "debug", target = "dht", ret(Display), err, fields(duration, __VEILID_LOG_KEY = self.log_key(), opt_value.len = opt_value.as_ref().map(|x| x.value_data().data_size())), skip(self, opt_value, _opt_seqs)))]
    pub async fn inbound_transact_command(
        &self,
        opaque_record_key: &OpaqueRecordKey,
        transaction_id: u64,
        command: TransactCommand,
        _opt_seqs: Option<Vec<ValueSeqNum>>,
        opt_subkey: Option<ValueSubkey>,
        opt_value: Option<Arc<SignedValueData>>,
    ) -> VeilidAPIResult<NetworkResult<InboundTransactCommandResult>> {
        record_duration_fut(async {
            let remote_record_store = self.get_remote_record_store()?;

            let transaction_id =
                match remote_record_store.lookup_inbound_transaction_id(transaction_id)? {
                    Some(id) => id,
                    None => {
                        return Ok(NetworkResult::value(
                            InboundTransactCommandResult::InvalidTransaction,
                        ));
                    }
                };

            let res = match command {
                TransactCommand::End => {
                    remote_record_store
                        .end_inbound_transaction(opaque_record_key, transaction_id)
                        .await?
                }
                TransactCommand::Commit => {
                    remote_record_store
                        .commit_inbound_transaction(opaque_record_key, transaction_id, || {
                            RemoteRecordDetail {}
                        })
                        .await?
                }
                TransactCommand::Rollback => {
                    remote_record_store
                        .rollback_inbound_transaction(opaque_record_key, transaction_id)
                        .await?
                }
                TransactCommand::Get => {
                    remote_record_store
                        .inbound_transaction_get(opaque_record_key, transaction_id, opt_subkey)
                        .await?
                }
                TransactCommand::Set => {
                    let Some(subkey) = opt_subkey else {
                        return Ok(NetworkResult::invalid_message("missing subkey"));
                    };
                    let Some(value) = opt_value else {
                        return Ok(NetworkResult::invalid_message("missing value"));
                    };

                    // Checked as a plain set value is, against the descriptor the begin fixed
                    let Some(descriptor) = remote_record_store
                        .inbound_transaction_descriptor(opaque_record_key, transaction_id)
                    else {
                        return Ok(NetworkResult::value(
                            InboundTransactCommandResult::InvalidTransaction,
                        ));
                    };
                    network_result_try!(
                        self.verify_inbound_signed_value(
                            opaque_record_key,
                            &descriptor,
                            &value,
                            subkey
                        )
                        .await
                    );
                    let Ok(schema) = descriptor.schema() else {
                        return Ok(NetworkResult::invalid_message("invalid schema"));
                    };
                    if self
                        .check_subkey_value_data(
                            &schema,
                            descriptor.ref_owner(),
                            subkey,
                            value.value_data(),
                        )
                        .is_err()
                    {
                        return Ok(NetworkResult::invalid_message("failed schema validation"));
                    }

                    remote_record_store
                        .inbound_transaction_set(opaque_record_key, transaction_id, subkey, value)
                        .await?
                }
            };

            Ok(NetworkResult::value(res))
        })
        .await
    }
}
