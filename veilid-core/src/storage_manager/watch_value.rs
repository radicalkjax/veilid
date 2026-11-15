use futures_util::StreamExt as _;

use super::*;

impl_veilid_log_facility!("stor");

/// The context of the outbound_watch_value operation
#[derive(Debug, Default)]
pub(super) struct OutboundWatchValueContext {
    /// A successful watch
    pub watch_value_result: OutboundWatchValueResult,
}

/// The record of a node accepting a watch
#[derive(Debug, Clone)]
pub(super) struct AcceptedWatch {
    /// The id the accepting node gave the watch
    pub watch_id: u64,
    /// The node that accepted the watch
    pub node_ref: NodeRef,
    /// The expiration of a successful watch
    pub expiration: Timestamp,
    /// Ref to the route receiving ValueChanged notifications
    pub opt_value_changed_route: Option<AllocatedRouteRef>,
}

/// The result of the outbound_watch_value operation
#[derive(Debug, Clone, Default)]
pub(super) struct OutboundWatchValueResult {
    /// Which nodes accepted the watch
    pub accepted: Vec<AcceptedWatch>,
    /// Which nodes rejected or cancelled the watch
    pub rejected: Vec<NodeRef>,
    /// Which nodes ignored the watch
    pub ignored: Vec<NodeRef>,
}

/// Watch parameters used to configure a watch
#[derive(Debug, Clone)]
pub(crate) struct InboundWatchParameters {
    /// The range of subkeys being watched; empty only to cancel, so it never fires
    pub subkeys: ValueSubkeyRangeSet,
    /// When this watch will expire
    pub expiration: Timestamp,
    /// How many updates are left before forced expiration
    pub count: u32,
    /// The watching schema member key, or an anonymous key
    pub watcher_member_id: MemberId,
    /// The place where updates are sent
    pub dest: Destination,
}

impl InboundWatchParameters {
    /// When an inbound watch with these parameters is dropped; one that can never fire goes at once
    pub fn drop_due(&self) -> Timestamp {
        if self.count == 0 || self.subkeys.is_empty() {
            Timestamp::new(0)
        } else {
            self.expiration
        }
    }
}

/// Watch result to return with answer
/// Default result is cancelled/expired/inactive/rejected
#[derive(Debug, Clone)]
pub(crate) enum InboundWatchValueResult {
    /// A new watch was created
    Created {
        /// The new id of the watch
        id: InboundWatchId,
        /// The expiration timestamp of the watch. This should never be zero.
        expiration: Timestamp,
    },
    /// An existing watch was modified
    Changed {
        /// The new expiration timestamp of the modified watch. This should never be zero.
        expiration: Timestamp,
    },
    /// An existing watch was cancelled
    Cancelled,
    /// The request was rejected due to invalid parameters or a missing watch
    Rejected,
}

impl OutboundWatchValueResult {
    pub fn merge(&mut self, other: OutboundWatchValueResult) {
        self.accepted.extend(other.accepted);
        self.rejected.extend(other.rejected);
        self.ignored.extend(other.ignored);
    }
}

impl StorageManager {
    /// Create, update or cancel an outbound watch to a DHT value
    #[cfg_attr(
        feature = "instrument",
        instrument(level = "trace", target = "stor", skip_all)
    )]
    pub async fn watch_values(
        &self,
        record_key: RecordKey,
        subkeys: ValueSubkeyRangeSet,
        expiration: Timestamp,
        count: u32,
    ) -> VeilidAPIResult<bool> {
        let Ok(_guard) = self.startup_lock.enter() else {
            apibail_not_initialized!();
        };

        // Watch mode: excludes close/delete and other watch ops on this record
        let _record_lock = self
            .record_lock_table
            .lock_record(record_key.opaque(), StorageManagerRecordLockPurpose::Watch)
            .await;

        let mut inner = self.inner.lock();
        let owm = &mut *self.outbound_watch_manager_mut();
        self.watch_values_inner(&mut inner, owm, record_key, subkeys, expiration, count)
    }

    #[cfg_attr(
        feature = "instrument",
        instrument(level = "trace", target = "stor", skip_all)
    )]
    pub async fn cancel_watch_values(
        &self,
        record_key: RecordKey,
        subkeys: ValueSubkeyRangeSet,
    ) -> VeilidAPIResult<bool> {
        let Ok(_guard) = self.startup_lock.enter() else {
            apibail_not_initialized!();
        };
        let opaque_record_key = record_key.opaque();

        // Watch mode: excludes close/delete and other watch ops on this record
        let _record_lock = self
            .record_lock_table
            .lock_record(
                opaque_record_key.clone(),
                StorageManagerRecordLockPurpose::Watch,
            )
            .await;

        // Calculate change to existing watch
        let mut inner = self.inner.lock();
        let owm = &mut *self.outbound_watch_manager_mut();

        let Some(_opened_record) = inner.opened_records.get(&opaque_record_key) else {
            apibail_invalid_argument!("record not open", "record_key", opaque_record_key);
        };

        // See what watch we have currently if any
        let Some(outbound_watch) = owm.outbound_watches.get(&opaque_record_key) else {
            // If we didn't have an active watch, then we can just return false because there's nothing to do here
            return Ok(false);
        };

        // Ensure we have a 'desired' watch state
        let Some(desired) = outbound_watch.desired() else {
            // If we didn't have a desired watch, then we're already cancelling
            let still_active = outbound_watch.state().is_some();
            return Ok(still_active);
        };

        // Rewrite subkey range if empty to full
        let subkeys = if subkeys.is_empty() {
            ValueSubkeyRangeSet::full()
        } else {
            subkeys
        };

        // Reduce the subkey range
        let new_subkeys = desired.subkeys.difference(&subkeys);

        // If no change is happening return false
        if new_subkeys == desired.subkeys {
            return Ok(false);
        }

        // If we have no subkeys left, then set the count to zero to indicate a full cancellation
        let count = if new_subkeys.is_empty() {
            0
        } else if let Some(state) = outbound_watch.state() {
            state.remaining_count()
        } else {
            desired.count
        };

        // Update the watch. This just calls through to the above watch_values_inner() function
        // This will update the active_watch so we don't need to do that in this routine.
        self.watch_values_inner(
            &mut inner,
            owm,
            record_key,
            new_subkeys,
            desired.expiration,
            count,
        )
    }

    /// Pin a safety selection to the route the remote pushes ValueChanged to, so a
    /// watch node keeps hearing from us over the path its notifications already use.
    /// Route selection otherwise round-robins, and `wants_per_node_watch_update` does
    /// not notice, since the recorded route stays allocated. Returns the selection
    /// unchanged once the route is gone, leaving reconcile to move the watch.
    pub(super) fn pin_value_changed_route(
        &self,
        safety_selection: &SafetySelection,
        opt_value_changed_route: Option<&PublicKey>,
    ) -> SafetySelection {
        let SafetySelection::Safe(safety_spec) = safety_selection else {
            return safety_selection.clone();
        };
        let Some(value_changed_route) = opt_value_changed_route else {
            return safety_selection.clone();
        };
        let Some(route_id) = self
            .routing_table()
            .route_spec_store()
            .get_route_id_for_key(value_changed_route)
        else {
            return safety_selection.clone();
        };
        let mut safety_spec = safety_spec.clone();
        safety_spec.preferred_route = Some(route_id);
        SafetySelection::Safe(safety_spec)
    }

    /// Get the set of nodes in our active watches
    /// The nodes holding our watch on a record
    pub(super) fn get_outbound_watch_node_refs(
        &self,
        opaque_record_key: &OpaqueRecordKey,
    ) -> Vec<NodeRef> {
        let owm = self.outbound_watch_manager.lock();
        let Some(state) = owm
            .outbound_watches
            .get(opaque_record_key)
            .and_then(|w| w.state())
        else {
            return vec![];
        };
        state
            .nodes()
            .iter()
            .filter_map(|pnk| owm.per_node_states.get(pnk)?.watch_node_ref.clone())
            .collect()
    }

    pub fn get_outbound_watch_nodes(&self) -> Vec<Destination> {
        let owm = self.outbound_watch_manager.lock();

        let mut out = vec![];
        let mut node_set: HashSet<NodeRef> = HashSet::new();
        for v in owm.outbound_watches.values() {
            if let Some(current) = v.state() {
                for pnk in current.nodes() {
                    let Some(pns) = owm.per_node_states.get(pnk) else {
                        continue;
                    };
                    let Some(node_ref) = pns.watch_node_ref.as_ref() else {
                        continue;
                    };
                    if node_set.contains(node_ref) {
                        continue;
                    }

                    node_set.insert(node_ref.clone());
                    out.push(Destination::direct(
                        node_ref.routing_domain_filtered(RoutingDomain::PublicInternet),
                        Some(self.pin_value_changed_route(
                            &current.params().safety_selection,
                            pns.opt_value_changed_route.as_ref(),
                        )),
                    ))
                }
            }
        }

        out
    }

    ////////////////////////////////////////////////////////////////////////

    #[cfg_attr(
        feature = "instrument",
        instrument(level = "trace", target = "stor", skip_all)
    )]
    fn watch_values_inner(
        &self,
        inner: &mut StorageManagerInner,
        owm: &mut OutboundWatchManager,
        record_key: RecordKey,
        subkeys: ValueSubkeyRangeSet,
        expiration: Timestamp,
        count: u32,
    ) -> VeilidAPIResult<bool> {
        let opaque_record_key = record_key.opaque();

        // Get the safety selection and the writer we opened this record
        let (safety_selection, opt_watcher) = {
            let Some(opened_record) = inner.opened_records.get(&opaque_record_key) else {
                // Record must be opened already to change watch
                apibail_invalid_argument!("record not open", "record_key", opaque_record_key);
            };
            (
                opened_record.safety_selection(),
                opened_record.writer().cloned(),
            )
        };

        // Rewrite subkey range if empty to full
        let subkeys = if subkeys.is_empty() {
            ValueSubkeyRangeSet::full()
        } else {
            subkeys
        };

        // Get the schema so we can truncate the watch to the number of subkeys
        let schema = if let Some(lrs) = inner.local_record_store.as_ref() {
            let Some(schema) = lrs.peek_record(&opaque_record_key, |r| r.schema()) else {
                apibail_generic!("no local record found");
            };
            schema
        } else {
            apibail_not_initialized!();
        };
        let subkeys = schema.truncate_subkeys(&subkeys, None);

        // Calculate desired watch parameters
        let desired_params = if count == 0 {
            // Cancel
            None
        } else {
            // Get the minimum expiration timestamp we will accept
            let rpc_timeout =
                TimestampDuration::new_ms(self.config().internal().network.rpc.timeout_ms.into());
            let min_expiration = Timestamp::now().later(rpc_timeout);
            let expiration = if expiration.as_u64() == 0 {
                expiration
            } else if expiration < min_expiration {
                apibail_invalid_argument!("expiration is too soon", "expiration", expiration);
            } else {
                expiration
            };

            // Create or modify
            Some(OutboundWatchParameters {
                expiration,
                count,
                subkeys,
                opt_watcher,
                safety_selection,
            })
        };

        // Modify the 'desired' state of the watch or add one if it does not exist
        let active = desired_params.is_some();
        owm.set_desired_watch(record_key, desired_params);

        Ok(active)
    }

    /// Perform a 'watch value cancel' on the network without fanout
    #[cfg_attr(
        feature = "instrument",
        instrument(target = "watch", level = "debug", skip_all, err)
    )]
    pub(super) async fn outbound_watch_value_cancel(
        &self,
        opaque_record_key: OpaqueRecordKey,
        opt_watcher: Option<KeyPair>,
        safety_selection: SafetySelection,
        watch_node: NodeRef,
        watch_id: u64,
    ) -> VeilidAPIResult<bool> {
        let routing_domain = RoutingDomain::PublicInternet;

        // Get the appropriate watcher key, if anonymous use a static anonymous watch key
        // which lives for the duration of the app's runtime
        let watcher = opt_watcher.unwrap_or_else(|| {
            self.anonymous_signing_keys
                .get(opaque_record_key.kind())
                .unwrap_or_log()
        });

        let wva = VeilidAPIError::from_network_result(
            self.rpc_processor()
                .rpc_call_watch_value(
                    Destination::direct(
                        watch_node.routing_domain_filtered(routing_domain),
                        Some(safety_selection),
                    ),
                    opaque_record_key,
                    ValueSubkeyRangeSet::new(),
                    Timestamp::default(),
                    0,
                    watcher,
                    Some(watch_id),
                    FanoutConfig::default(),
                )
                .await?,
        )?;

        if wva.answer.accepted {
            veilid_log!(self debug "Outbound watch cancelled: id={} ({})", wva.answer.watch_id, watch_node);
            Ok(true)
        } else {
            veilid_log!(self debug "Outbound watch id did not exist: id={} ({})", watch_id, watch_node);
            Ok(false)
        }
    }

    /// Perform a 'watch value change' on the network without fanout
    #[cfg_attr(
        feature = "instrument",
        instrument(target = "watch", level = "debug", skip_all, err)
    )]
    pub(super) async fn outbound_watch_value_change(
        &self,
        opaque_record_key: OpaqueRecordKey,
        params: OutboundWatchParameters,
        watch_node: NodeRef,
        watch_id: u64,
    ) -> VeilidAPIResult<OutboundWatchValueResult> {
        let routing_domain = RoutingDomain::PublicInternet;

        if params.count == 0 {
            apibail_internal!("cancel should be done with outbound_watch_value_cancel");
        }

        // Get the appropriate watcher key, if anonymous use a static anonymous watch key
        // which lives for the duration of the app's runtime
        let watcher = params.opt_watcher.unwrap_or_else(|| {
            self.anonymous_signing_keys
                .get(opaque_record_key.kind())
                .unwrap_or_log()
        });

        let wva = VeilidAPIError::from_network_result(
            pin_future!(self.rpc_processor().rpc_call_watch_value(
                Destination::direct(
                    watch_node.routing_domain_filtered(routing_domain),
                    Some(params.safety_selection)
                ),
                opaque_record_key,
                params.subkeys,
                params.expiration,
                params.count,
                watcher,
                Some(watch_id),
                FanoutConfig::default(),
            ))
            .await?,
        )?;

        if wva.answer.accepted {
            if wva.answer.expiration.is_zero() {
                veilid_log!(self debug "WatchValue not renewed: id={} ({})", watch_id, watch_node);
            } else if watch_id != wva.answer.watch_id {
                veilid_log!(self debug "WatchValue changed: id={}->{} expiration={:#} ({})", watch_id, wva.answer.watch_id, wva.answer.expiration, watch_node);
            } else {
                veilid_log!(self debug "WatchValue renewed: id={} expiration={:#} ({})", watch_id, wva.answer.expiration, watch_node);
            }

            Ok(OutboundWatchValueResult {
                accepted: vec![AcceptedWatch {
                    watch_id: wva.answer.watch_id,
                    node_ref: watch_node,
                    expiration: wva.answer.expiration,
                    opt_value_changed_route: wva
                        .answer_context
                        .waitable_reply_context
                        .opt_reply_private_route_ref
                        .clone(),
                }],
                rejected: vec![],
                ignored: vec![],
            })
        } else {
            veilid_log!(self debug "WatchValue change failed: id={} ({})", wva.answer.watch_id, watch_node);
            Ok(OutboundWatchValueResult {
                accepted: vec![],
                rejected: vec![watch_node],
                ignored: vec![],
            })
        }
    }

    /// Perform a 'watch value' query on the network using fanout
    ///
    /// Accepted watches land in `context` as they arrive, so a caller that stops the fanout early
    /// still knows which nodes hold one
    #[cfg_attr(
        feature = "instrument",
        instrument(target = "watch", level = "debug", skip_all, err)
    )]
    pub(super) async fn outbound_watch_value(
        &self,
        record_key: RecordKey,
        params: OutboundWatchParameters,
        per_node_state: HashMap<PerNodeKey, OutboundWatchPerNodeState>,
        context: Arc<Mutex<OutboundWatchValueContext>>,
    ) -> VeilidAPIResult<OutboundWatchValueResult> {
        let opaque_record_key = record_key.opaque();
        let routing_domain = RoutingDomain::PublicInternet;

        // Get the DHT parameters for 'WatchValue', some of which are the same for 'GetValue' operations
        let config = self.config();
        let node_count = config.internal().network.dht.max_find_node_count as usize;
        let consensus_count = config.internal().network.dht.get_value_count as usize;
        let consensus_width = config.internal().network.dht.consensus_width as usize;
        let fanout_tasks = config.internal().network.dht.get_value_fanout as usize;
        let timeout =
            TimestampDuration::from(ms_to_us(config.internal().network.dht.get_value_timeout_ms));

        // Get the appropriate watcher key, if anonymous use a static anonymous watch key
        // which lives for the duration of the app's runtime
        let watcher = params.opt_watcher.clone().unwrap_or_else(|| {
            self.anonymous_signing_keys
                .get(record_key.kind())
                .unwrap_or_log()
        });

        // Get the nodes we know are caching this value to seed the fanout
        let init_fanout_queue = self
            .get_value_nodes(&opaque_record_key)?
            .unwrap_or_default()
            .into_iter()
            .filter(|x| {
                x.node_info(routing_domain)
                    .map(|ni| ni.has_all_capabilities(&[VEILID_CAPABILITY_DHT]))
                    .unwrap_or_default()
            })
            .collect();

        // Routine to call to generate fanout
        let call_routine = {
            let context = context.clone();
            let registry = self.registry();
            let params = params.clone();
            let record_key = record_key.clone();
            let watcher = watcher.clone();
            Arc::new(
                move |next_node: NodeRef| -> PinBoxFutureStatic<FanoutCallResult> {
                    let context = context.clone();
                    let registry = registry.clone();
                    let record_key = record_key.clone();
                    let watcher = watcher.clone();
                    let params = params.clone();

                    // Only an id the node can still know about: an expired one is refused
                    // before the record is checked, and a restart makes every id stale
                    let node_id = next_node.node_ids().get(record_key.kind()).unwrap_or_log();
                    let pnk = PerNodeKey {
                        record_key: record_key.clone(),
                        node_id,
                    };
                    let watch_id = per_node_state.get(&pnk).and_then(|pns| {
                        (Timestamp::now() < pns.expiration).then_some(pns.watch_id)
                    });

                    let fut = async move {
                        let rpc_processor = registry.rpc_processor();

                        let wva = match rpc_processor
                            .rpc_call_watch_value(
                                Destination::direct(
                                    next_node.routing_domain_filtered(routing_domain),
                                    Some(params.safety_selection),
                                ),
                                record_key.opaque(),
                                params.subkeys,
                                params.expiration,
                                params.count,
                                watcher,
                                watch_id,
                                FanoutConfig::default(),
                            )
                            .await?
                        {
                            NetworkResult::Timeout => {
                                let mut ctx = context.lock();
                                ctx.watch_value_result.ignored.push(next_node.clone());
                                return Ok(FanoutCallOutput {
                                    peer_info_list: vec![],
                                    disposition: FanoutCallDisposition::Timeout,
                                });
                            }
                            NetworkResult::ServiceUnavailable(_)
                            | NetworkResult::NoConnection(_)
                            | NetworkResult::AlreadyExists(_)
                            | NetworkResult::InvalidMessage(_) => {
                                let mut ctx = context.lock();
                                ctx.watch_value_result.ignored.push(next_node.clone());
                                return Ok(FanoutCallOutput {
                                    peer_info_list: vec![],
                                    disposition: FanoutCallDisposition::Invalid,
                                });
                            }
                            NetworkResult::Value(v) => v,
                        };

                        // Keep answer if we got one
                        // (accepted means the node could provide an answer, not that the watch is active)
                        let disposition = if wva.answer.accepted {
                            if wva.answer.expiration.as_u64() > 0 {
                                // If the expiration time is greater than zero this watch is active
                                veilid_log!(registry debug target:"watch", "WatchValue accepted for {}: id={} expiration={:#} ({})", record_key, wva.answer.watch_id, wva.answer.expiration, next_node);

                                // Add to accepted watches
                                let mut ctx = context.lock();
                                ctx.watch_value_result.accepted.push(AcceptedWatch {
                                    watch_id: wva.answer.watch_id,
                                    node_ref: next_node.clone(),
                                    expiration: wva.answer.expiration,
                                    opt_value_changed_route: wva
                                        .answer_context
                                        .waitable_reply_context
                                        .opt_reply_private_route_ref
                                        .clone(),
                                });

                                FanoutCallDisposition::Accepted
                            } else {
                                // Zero expiration is a refusal with no reason on the wire,
                                // so log whether we sent an id
                                veilid_log!(registry debug target:"watch", "WatchValue refused for {}: sent_id={} id={} expiration={:#} ({})",
                                    record_key,
                                    watch_id.map(|x| x.to_string()).unwrap_or_else(|| "none".to_owned()),
                                    wva.answer.watch_id, wva.answer.expiration, next_node);

                                // Add to rejected watches
                                let mut ctx = context.lock();
                                ctx.watch_value_result.rejected.push(next_node.clone());

                                // Treat as accepted but do not add to consensus
                                FanoutCallDisposition::Stale
                            }
                        } else {
                            // Logged apart from a refusal: the node put itself outside the
                            // consensus width, so this is node selection, not the holder
                            veilid_log!(registry debug target:"watch", "WatchValue not accepted for {}: too far from key ({})", record_key, next_node);

                            // Add to rejected watches
                            let mut ctx = context.lock();
                            ctx.watch_value_result.rejected.push(next_node.clone());

                            // Treat as rejected and do not add to consensus
                            FanoutCallDisposition::Rejected
                        };

                        // Return peers if we have some
                        veilid_log!(registry debug target:"network_result", "WatchValue fanout call returned peers {} ({})", wva.answer.peers.len(), next_node);

                        Ok(FanoutCallOutput {
                            peer_info_list: wva.answer.peers,
                            disposition,
                        })
                    };
                    #[cfg(feature = "instrument")]
                    let fut = fut.instrument(tracing::trace_span!(
                        target: "dht",
                        "outbound_watch_value call routine"
                    ));
                    Box::pin(fut) as PinBoxFuture<FanoutCallResult>
                },
            )
        };

        // Routine to call to check if we're done at each step
        let check_done = {
            Arc::new(
                move |fanout_result: &FanoutResult| -> FanoutDoneDisposition {
                    match fanout_result.kind {
                        FanoutResultKind::Incomplete => {
                            // Keep going
                            FanoutDoneDisposition::NotDone
                        }
                        FanoutResultKind::Timeout | FanoutResultKind::Exhausted => {
                            // Signal we're done
                            FanoutDoneDisposition::DoneEarly
                        }
                        FanoutResultKind::Consensus => {
                            // Let the questions in flight answer: a node that accepts after
                            // consensus holds a watch too, and pushes until it is cancelled
                            FanoutDoneDisposition::Done
                        }
                    }
                },
            )
        };

        // Call the fanout
        // Use the same fanout parameters as a get
        // and each one might take timeout_us time.
        let routing_table = self.routing_table();
        let hash_coordinate = record_key.to_hash_coordinate();
        let mut fanout_call = FanoutCall::new(
            &routing_table,
            FanoutCallParams {
                name: format!("outbound_watch_value({:#})", Timestamp::now_increasing()),
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

        // A node with no copy refuses the watch, and only the seeds have answered before
        fanout_call.set_prefer_init_nodes(true);
        fanout_call.set_demote_routed_unresponsive(matches!(
            params.safety_selection,
            SafetySelection::Safe(_)
        ));

        let shutdown_stop_source = self
            .startup_lock
            .stop_token()
            .ok_or_else(VeilidAPIError::not_initialized)?;

        let fanout_result = fanout_call
            .run(
                init_fanout_queue,
                FanoutQueueMode::ThrottleAtConsensus,
                shutdown_stop_source,
            )
            .await
            .inspect_err(|e| {
                // If we finished with an error, return that
                veilid_log!(self debug target:"watch", "WatchValue fanout error: {}", e);
            })?;

        veilid_log!(self debug target:"dht", "WatchValue Fanout: {:#}", fanout_result);

        // Keep the list of nodes that responded for later reference
        let existed = self.process_fanout_results(
            record_key.opaque(),
            core::iter::once((ValueSubkeyRangeSet::new(), fanout_result)),
            false,
            self.config().internal().network.dht.consensus_width as usize,
        )?;
        if !existed {
            apibail_internal!(
                "Record went missing during watch operation despite lock: {}",
                record_key.opaque(),
            )
        }

        let owvresult = context.lock().watch_value_result.clone();
        Ok(owvresult)
    }

    /// Remove dead watches from the table
    pub(super) fn process_outbound_watch_dead(&self, record_lock: StorageManagerRecordLockGuard) {
        let opaque_record_key = record_lock.record();

        let Some(outbound_watch) = self
            .outbound_watch_manager
            .lock()
            .outbound_watches
            .remove(&opaque_record_key)
        else {
            veilid_log!(self warn "dead watch should have still been in the table");
            return;
        };

        if outbound_watch.state().is_some() {
            veilid_log!(self warn "dead watch still had current state");
        }
        if outbound_watch.desired().is_some() {
            veilid_log!(self warn "dead watch still had desired params");
        }

        // Send valuechange with dead count and no subkeys to inform the api that this watch is now gone completely
        self.update_callback_value_change(
            record_lock,
            outbound_watch.record_key(),
            ValueSubkeyRangeSet::new(),
            0,
            None,
        );
    }

    /// Get the list of remaining active watch ids
    /// and call their nodes to cancel the watch
    pub(super) async fn process_outbound_watch_cancel(
        &self,
        record_lock: StorageManagerRecordLockGuard,
    ) {
        let opaque_record_key = record_lock.record();

        // If we can't do this operation right now, don't try
        if !self.dht_is_online() {
            return;
        }

        let per_node_states = {
            let owm = &mut *self.outbound_watch_manager_mut();

            let Some(outbound_watch) = owm.outbound_watches.get_mut(&opaque_record_key) else {
                veilid_log!(self warn "watch being cancelled should have still been in the table");
                return;
            };
            let Some(state) = &mut outbound_watch.state_mut() else {
                veilid_log!(self warn "watch being cancelled should have current state");
                return;
            };
            let mut per_node_states = vec![];
            let mut missing_pnks = BTreeSet::new();
            for pnk in state.nodes() {
                let Some(per_node_state) = owm.per_node_states.get(pnk).cloned() else {
                    veilid_log!(self warn "missing per-node state for watch");
                    missing_pnks.insert(pnk.clone());
                    continue;
                };
                per_node_states.push((pnk.clone(), per_node_state));
            }

            state.edit(&owm.per_node_states, |editor| {
                editor.retain_nodes(|x| !missing_pnks.contains(x));
            });

            per_node_states
        };

        // Now reach out to each node and cancel their watch ids
        let mut unord = FuturesUnordered::new();
        for (pnk, pns) in per_node_states {
            let opaque_record_key = opaque_record_key.clone();
            unord.push(async move {
                let safety_selection = self.pin_value_changed_route(
                    &pns.safety_selection,
                    pns.opt_value_changed_route.as_ref(),
                );
                let res = Box::pin(self.outbound_watch_value_cancel(
                    opaque_record_key,
                    pns.opt_watcher,
                    safety_selection,
                    pns.watch_node_ref.unwrap_or_log(),
                    pns.watch_id,
                ))
                .await;
                (pnk, res)
            });
        }

        let mut cancelled = vec![];
        let mut preempted = false;
        loop {
            let Some(next) = record_lock.until_preempted(unord.next()).await else {
                preempted = true;
                break;
            };
            let Some((pnk, res)) = next else {
                break;
            };
            match res {
                Ok(_) => {
                    // Remove from 'per node states' because we got some response
                    cancelled.push(pnk);
                }
                Err(e) => {
                    veilid_log!(self debug "Outbound watch cancel error: {}", e);

                    // xxx should do something different for network unreachable vs host unreachable
                    // Leave in the 'per node states' for now because we couldn't contact the node
                    // but remove from this watch. We'll try the cancel again if we reach this node again during fanout.
                }
            }
        }

        // Update state
        {
            let owm = &mut *self.outbound_watch_manager_mut();

            // Remove per node watches we cancelled
            for pnk in &cancelled {
                if owm.per_node_states.remove(pnk).is_none() {
                    veilid_log!(self warn "per-node watch being cancelled should have still been in the table");
                };
            }

            // Remove outbound watch we've cancelled
            let Some(outbound_watch) = owm.outbound_watches.get_mut(&opaque_record_key) else {
                veilid_log!(self warn "watch being cancelled should have still been in the table");
                return;
            };

            // The next cancel operation takes the nodes this one did not reach
            if preempted {
                veilid_log!(self debug target: "watch", "OutboundWatch({}): cancel pre-empted after {} nodes", opaque_record_key, cancelled.len());
                if let Some(state) = outbound_watch.state_mut() {
                    state.edit(&owm.per_node_states, |editor| {
                        editor.retain_nodes(|x| !cancelled.contains(x));
                    });
                }
                return;
            }

            // Mark as dead now that we cancelled
            outbound_watch.clear_state();
        }
    }

    /// See which existing per-node watches can be renewed
    /// and drop the ones that can't be or are dead
    pub(super) async fn process_outbound_watch_renew(
        &self,
        record_lock: StorageManagerRecordLockGuard,
    ) {
        let opaque_record_key = record_lock.record();

        // If we can't do this operation right now, don't try
        if !self.dht_is_online() {
            return;
        }

        let (record_key, per_node_states, per_node_params) = {
            let owm = &mut *self.outbound_watch_manager_mut();
            let Some(outbound_watch) = owm.outbound_watches.get_mut(&opaque_record_key) else {
                veilid_log!(self warn "watch being renewed should have still been in the table");
                return;
            };

            let Some(desired) = outbound_watch.desired() else {
                veilid_log!(self warn "watch being renewed should have desired parameters");
                return;
            };

            let Some(state) = outbound_watch.state_mut() else {
                veilid_log!(self warn "watch being renewed should have current state");
                return;
            };

            // Only the per-node watches that need it, so a silent node retries alone
            let cur_ts = Timestamp::now();
            let renew_ts = cur_ts.later(self.timings().renew_outbound_watches);
            let routing_table = self.routing_table();
            let rss = routing_table.route_spec_store();
            let route_is_dead = |route: &PublicKey| rss.get_route_id_for_key(route).is_none();
            let max_interval = self.timings().watch_inspect_max_interval;
            let mut per_node_states = vec![];
            let mut missing_pnks = BTreeSet::new();
            for pnk in state.nodes() {
                let Some(per_node_state) = owm.per_node_states.get(pnk).cloned() else {
                    veilid_log!(self warn "missing per-node state for watch");
                    missing_pnks.insert(pnk.clone());
                    continue;
                };
                if per_node_state.needs_renewal(cur_ts, renew_ts, route_is_dead) {
                    per_node_states.push((pnk.clone(), per_node_state));
                }
            }
            let lost_a_route = per_node_states.iter().any(|(_, pns)| {
                pns.opt_value_changed_route
                    .as_ref()
                    .is_some_and(route_is_dead)
            });
            state.edit(&owm.per_node_states, |editor| {
                editor.retain_nodes(|x| !missing_pnks.contains(x));
                // Pushes sent over a route that died may never have arrived
                if lost_a_route {
                    editor.reset_fallback_inspect_backoff(Some(cur_ts));
                    editor.record_route_renewal(cur_ts, max_interval);
                }
            });

            let per_node_params = state.get_per_node_params(&desired);

            (
                outbound_watch.record_key(),
                per_node_states,
                per_node_params,
            )
        };

        // Now reach out to each node and renew their watches
        let mut unord = FuturesUnordered::new();
        for (_pnk, pns) in per_node_states {
            let mut params = per_node_params.clone();
            let opaque_record_key = opaque_record_key.clone();
            // A renewal is not moving the watch, so confirm it over the route its
            // ValueChanged already uses
            params.safety_selection = self.pin_value_changed_route(
                &params.safety_selection,
                pns.opt_value_changed_route.as_ref(),
            );
            unord.push(async move {
                self.outbound_watch_value_change(
                    opaque_record_key,
                    params,
                    pns.watch_node_ref.unwrap_or_log(),
                    pns.watch_id,
                )
                .await
            });
        }

        // Process and merge all results since we're not fanning out; a pre-empted renewal keeps
        // the answers it has
        let mut opt_owvresult: Option<OutboundWatchValueResult> = None;
        loop {
            let Some(next) = record_lock.until_preempted(unord.next()).await else {
                veilid_log!(self debug target: "watch", "OutboundWatch({}): renew pre-empted", opaque_record_key);
                break;
            };
            let Some(res) = next else {
                break;
            };
            match res {
                Ok(r) => {
                    opt_owvresult = match opt_owvresult {
                        Some(mut owvresult) => {
                            owvresult.merge(r);
                            Some(owvresult)
                        }
                        None => Some(r),
                    };
                }
                Err(e) => {
                    veilid_log!(self debug "Outbound watch change error: {}", e);
                }
            }
        }

        // Update state with merged results if we have them
        if let Some(owvresult) = opt_owvresult {
            let owm = &mut *self.outbound_watch_manager_mut();
            self.process_outbound_watch_value_result_inner(owm, record_key, owvresult);
        }
    }

    /// Perform fanout to add or update per-node watches to an outbound watch
    pub(super) async fn process_outbound_watch_reconcile(
        &self,
        record_lock: StorageManagerRecordLockGuard,
    ) {
        let opaque_record_key = record_lock.record();

        // If we can't do this operation right now, don't try
        if !self.dht_is_online() {
            return;
        }

        // Get the nodes already active on this watch,
        // and the parameters to fanout with for the rest
        let (record_key, per_node_state, per_node_params) = {
            let owm = &mut *self.outbound_watch_manager_mut();
            let Some(outbound_watch) = owm.outbound_watches.get_mut(&opaque_record_key) else {
                veilid_log!(self warn "watch being reconciled should have still been in the table");
                return;
            };

            let record_key = outbound_watch.record_key();

            // Get params to reconcile
            let Some(desired) = outbound_watch.desired() else {
                veilid_log!(self warn "watch being reconciled should have had desired parameters");
                return;
            };

            // Get active per node states
            let (mut per_node_state, per_node_params) = if let Some(state) = outbound_watch.state()
            {
                let per_node_state = state
                    .nodes()
                    .iter()
                    .map(|pnk| {
                        (
                            pnk.clone(),
                            owm.per_node_states.get(pnk).cloned().unwrap_or_log(),
                        )
                    })
                    .collect();
                let per_node_params = state.get_per_node_params(&desired);

                (per_node_state, per_node_params)
            } else {
                (HashMap::new(), desired)
            };

            // Add in any inactive per node states
            for (pnk, pns) in &owm.per_node_states {
                // Skip any we have already
                if per_node_state.contains_key(pnk) {
                    continue;
                }
                // Add inactive per node state if the record key matches
                if pnk.record_key == record_key {
                    per_node_state.insert(pnk.clone(), pns.clone());
                }
            }

            (record_key, per_node_state, per_node_params)
        };

        // Now fan out with parameters and get new per node watches
        let cur_ts = Timestamp::now_non_decreasing();
        let context = Arc::new(Mutex::new(OutboundWatchValueContext::default()));
        let Some(res) = record_lock
            .until_preempted(self.outbound_watch_value(
                record_key.clone(),
                per_node_params,
                per_node_state,
                context.clone(),
            ))
            .await
        else {
            // Keep the watches already accepted so a cancel can reach them; leave the
            // reconcile due, since being pre-empted is not a failed attempt
            let partial = context.lock().watch_value_result.clone();
            veilid_log!(self debug target: "watch", "OutboundWatch({}): reconcile pre-empted after {} accepted", record_key, partial.accepted.len());
            if !partial.accepted.is_empty() {
                let owm = &mut *self.outbound_watch_manager_mut();
                self.process_outbound_watch_value_result_inner(owm, record_key, partial);
            }
            return;
        };

        {
            let owm = &mut *self.outbound_watch_manager_mut();
            let (accepted, rejected) = match &res {
                Ok(owvresult) => (owvresult.accepted.len(), owvresult.rejected.len()),
                Err(_) => (0, 0),
            };
            match res {
                Ok(owvresult) => {
                    // Update state
                    self.process_outbound_watch_value_result_inner(
                        owm,
                        record_key.clone(),
                        owvresult,
                    );

                    // If we succeeded update the last consensus node count
                    let Some(outbound_watch) = owm.outbound_watches.get_mut(&opaque_record_key)
                    else {
                        veilid_log!(self warn "watch being reconciled should have still been in the table");
                        return;
                    };
                    let Some(state) = outbound_watch.state_mut() else {
                        veilid_log!(self warn "watch being reconciled should have had a state");
                        return;
                    };
                    state.edit(&owm.per_node_states, |editor| {
                        editor.update_last_consensus_node_count();
                    });
                }
                Err(e) => {
                    veilid_log!(self debug "Outbound watch fanout error: {}", e);
                }
            }

            // Regardless of result, set our next possible reconciliation time
            let consensus_count = self.config().internal().network.dht.get_value_count as usize;
            let opt_state = owm
                .outbound_watches
                .get(&opaque_record_key)
                .and_then(|ow| ow.state());
            let below_consensus = opt_state.is_none_or(|s| s.nodes().len() < consensus_count);
            let retries = opt_state.map_or(0, |s| s.reconcile_retries());
            let survival = self
                .network_manager()
                .online_detector()
                .survival(RoutingDomain::PublicInternet);
            let (interval, next_retries) = Self::next_reconcile_interval(
                below_consensus,
                accepted,
                rejected,
                retries,
                survival,
            );
            owm.set_next_reconcile(opaque_record_key, cur_ts.later(interval), next_retries);
        }
    }

    /// How long until a watch reconciles again, and the retry count after this one: soon while
    /// below consensus, backing off through `failure_backoff` as retries repeat, unless every
    /// answer was a refusal, a node with no copy, usually because nothing is written yet.
    /// A fanout nobody answered learned nothing about the record and retries.
    pub(super) fn next_reconcile_interval(
        below_consensus: bool,
        accepted: usize,
        rejected: usize,
        retries: u32,
        opt_survival: Option<f64>,
    ) -> (TimestampDuration, u32) {
        let all_refused = accepted == 0 && rejected > 0;
        if below_consensus && !all_refused {
            let interval = failure_backoff(
                RECONCILE_OUTBOUND_WATCHES_RETRY_INTERVAL,
                RECONCILE_OUTBOUND_WATCHES_INTERVAL,
                retries,
                opt_survival,
            )
            .unwrap_or(RECONCILE_OUTBOUND_WATCHES_INTERVAL);
            (interval, retries.saturating_add(1))
        } else {
            (RECONCILE_OUTBOUND_WATCHES_INTERVAL, 0)
        }
    }

    fn process_outbound_watch_value_result_inner(
        &self,
        owm: &mut OutboundWatchManager,
        record_key: RecordKey,
        owvresult: OutboundWatchValueResult,
    ) {
        let kind = record_key.kind();
        let opaque_record_key = record_key.opaque();

        let Some(outbound_watch) = owm.outbound_watches.get_mut(&opaque_record_key) else {
            veilid_log!(self warn "Outbound watch should have still been in the table");
            return;
        };
        let Some(desired) = outbound_watch.desired() else {
            veilid_log!(self warn "Watch with result should have desired params");
            return;
        };

        let watch_subkeys = desired.subkeys.clone();
        let opt_old_state_params = outbound_watch.state().map(|s| s.params().clone());

        let state = outbound_watch.state_mut_or_create(|| desired.clone());

        let mut added_nodes = Vec::new();
        let mut remove_nodes = BTreeSet::new();
        let mut did_add_new_nodes = false;

        // Handle accepted
        for accepted_watch in owvresult.accepted {
            let node_id = accepted_watch.node_ref.node_ids().get(kind).unwrap_or_log();
            let pnk = PerNodeKey {
                record_key: record_key.clone(),
                node_id,
            };

            let expiration = accepted_watch.expiration;
            let count = state.remaining_count();

            // Check for accepted watch that came back with a dead watch
            // (non renewal, watch id didn't exist, didn't renew in time)
            if expiration.as_u64() != 0 && count > 0 {
                // Insert state, possibly overwriting an existing one
                let watch_id = accepted_watch.watch_id;
                let opt_watcher = desired.opt_watcher.clone();
                let safety_selection = desired.safety_selection.clone();
                let watch_node_ref = Some(accepted_watch.node_ref);
                // Persisted state stores the key, not the lock ref (not serializable)
                let opt_value_changed_route = accepted_watch
                    .opt_value_changed_route
                    .map(|r| r.route_key().clone());

                // Renewals overwrite existing per-node state; only first registrations count as new
                did_add_new_nodes |= !owm.per_node_states.contains_key(&pnk);
                owm.per_node_states.insert(
                    pnk.clone(),
                    OutboundWatchPerNodeState {
                        watch_id,
                        safety_selection,
                        opt_watcher,
                        expiration,
                        count,
                        watch_node_ref,
                        opt_value_changed_route,
                    },
                );
                added_nodes.push(pnk);
            } else {
                // Remove per node state because this watch id was not renewed
                owm.per_node_states.remove(&pnk);
                remove_nodes.insert(pnk);
            }
        }
        // Eliminate rejected
        for rejected_node_ref in owvresult.rejected {
            let node_id = rejected_node_ref.node_ids().get(kind).unwrap_or_log();
            let pnk = PerNodeKey {
                record_key: record_key.clone(),
                node_id,
            };
            owm.per_node_states.remove(&pnk);
            remove_nodes.insert(pnk);
        }
        // Drop unanswered but leave in per node state
        for ignored_node_ref in owvresult.ignored {
            let node_id = ignored_node_ref.node_ids().get(kind).unwrap_or_log();
            let pnk = PerNodeKey {
                record_key: record_key.clone(),
                node_id,
            };
            remove_nodes.insert(pnk);
        }

        // Update watch state
        let params_changed = opt_old_state_params.as_ref() != Some(&desired);
        state.edit(&owm.per_node_states, |editor| {
            editor.set_params(desired.clone());
            editor.retain_nodes(|x| !remove_nodes.contains(x));
            editor.add_nodes(added_nodes);
            editor.clear_forced_reconcile();
            // New nodes or a new subkey range can hide a change made while unwatched.
            // Losing nodes cannot: it costs future coverage, which is what the fallback
            // interval is for, and a watch nobody will accept loses nodes on every
            // reconcile, so resetting on that pins it to the floor forever
            if params_changed || did_add_new_nodes {
                editor.reset_fallback_inspect_backoff(None);
            }
        });

        // Inspect on params change (includes first establishment) or a first-time
        // node registration, so changes made while unwatched are reported promptly
        if opt_old_state_params != Some(desired) || did_add_new_nodes {
            owm.enqueue_change_inspect(record_key, watch_subkeys, ChangeInspectTrigger::Reconcile);
        }
    }

    /// Get the next operation for a particular watch's state machine
    /// Can be processed in the foreground, or by the background operation queue
    pub(super) fn get_next_outbound_watch_operation_inner(
        registry: VeilidComponentRegistry,
        record_lock: StorageManagerRecordLockGuard,
        cur_ts: Timestamp,
        outbound_watch: &mut OutboundWatch,
        hold_network_work: bool,
    ) -> Option<PinBoxFutureStatic<()>> {
        let consensus_count = registry.config().internal().network.dht.get_value_count as usize;

        // Terminate the 'desired' params for watches
        // that have no remaining count or have expired
        outbound_watch.try_expire_desired_state(cur_ts);

        // Check states
        if outbound_watch.is_dead() {
            // Outbound watch is dead
            let fut = {
                let registry = registry.clone();
                async move {
                    registry
                        .storage_manager()
                        .process_outbound_watch_dead(record_lock)
                }
            };
            return Some(pin_dyn_future!(fut));
        } else if hold_network_work {
            return None;
        } else if outbound_watch.needs_cancel(&registry) {
            // Outbound watch needs to be cancelled
            let fut = {
                let registry = registry.clone();
                async move {
                    registry
                        .storage_manager()
                        .process_outbound_watch_cancel(record_lock)
                        .await
                }
            };
            return Some(pin_dyn_future!(fut));
        } else if outbound_watch.needs_renew(&registry, consensus_count, cur_ts) {
            // Outbound watch expired but can be renewed
            let fut = {
                let registry = registry.clone();
                async move {
                    registry
                        .storage_manager()
                        .process_outbound_watch_renew(record_lock)
                        .await
                }
            };
            return Some(pin_dyn_future!(fut));
        } else if outbound_watch.needs_reconcile(&registry, consensus_count, cur_ts) {
            // Outbound watch parameters have changed or it needs more nodes
            let fut = {
                let registry = registry.clone();
                async move {
                    registry
                        .storage_manager()
                        .process_outbound_watch_reconcile(record_lock)
                        .await
                }
            };
            return Some(pin_dyn_future!(fut));
        }
        None
    }

    /// Build the change-inspection operation for a record: an inspection of its subkeys for
    /// anything newer online. One changed subkey is fetched and reported with its value; a wider
    /// range is announced for the app to read what it wants of it. Routes to the transactional
    /// variant when the record currently participates in a transaction set. The caller, which
    /// holds the watch manager, says whether the watch's own nodes can answer with no peer list.
    pub(super) fn get_change_inspection_operation_inner(
        registry: VeilidComponentRegistry,
        local_record_store: RecordStore<LocalRecordDetail>,
        record_key: RecordKey,
        subkeys: ValueSubkeyRangeSet,
        trigger: ChangeInspectTrigger,
        fanout_config: FanoutConfig,
    ) -> PinBoxFutureStatic<()> {
        let opaque_record_key = record_key.opaque();
        let opt_membership = match local_record_store.with_record(&opaque_record_key, |r| {
            r.detail().transaction_membership.clone()
        }) {
            Ok(Some(membership)) => membership,
            _ => None,
        };
        match opt_membership {
            Some(membership) => Self::get_change_inspection_operation_transactional_inner(
                registry,
                record_key,
                subkeys,
                membership,
                trigger,
                fanout_config,
            ),
            None => Self::get_change_inspection_operation_non_transactional_inner(
                registry,
                record_key,
                subkeys,
                trigger,
                fanout_config,
            ),
        }
    }

    fn get_change_inspection_operation_non_transactional_inner(
        registry: VeilidComponentRegistry,
        record_key: RecordKey,
        subkeys: ValueSubkeyRangeSet,
        trigger: ChangeInspectTrigger,
        fanout_config: FanoutConfig,
    ) -> PinBoxFutureStatic<()> {
        let fut = {
            let opaque_record_key = record_key.opaque();
            async move {
                let this = registry.storage_manager();

                let res = async {
                    let report = this
                        .inspect_record_with_fanout(
                            record_key.clone(),
                            subkeys.clone(),
                            DHTReportScope::SyncGet,
                            fanout_config,
                            RecordLockPriority::Background,
                        )
                        .await?;
                    let newer_online_subkeys = report.newer_online_subkeys();

                    // Telemetry to decide whether the periodic inspection earns its cost
                    if newer_online_subkeys.is_empty() {
                        veilid_log!(this debug "change inspect: nothing newer online for {}", record_key);
                    } else {
                        veilid_log!(this debug "change inspect: PRODUCTIVE: missed value change on {} subkeys {}", record_key, newer_online_subkeys);
                        #[cfg(feature = "verbose-tracing")]
                        this.diagnose_watch_notification_routes(&opaque_record_key);
                    }
                    this.note_change_inspect_outcome(
                        &opaque_record_key,
                        !newer_online_subkeys.is_empty(),
                    );

                    if newer_online_subkeys.len() == 1 {
                        let changed_subkey = newer_online_subkeys.first().unwrap_or_log();
                        // By subkey: the seq lists follow the report's range, not the newer subset
                        let local_seq = report
                            .local_seq_for(changed_subkey)
                            .unwrap_or(ValueSeqNum::NONE);
                        let opt_value = this
                            .get_value(record_key.clone(), changed_subkey, GetValueRefresh::Newer)
                            .await?
                            .filter(|value| value.seq() > local_seq);
                        if let Some(value) = opt_value {
                            // get_value already wrote the local store, so no reported seq to track
                            this.report_change_inspect(
                                record_key.clone(),
                                &opaque_record_key,
                                newer_online_subkeys,
                                Some((changed_subkey, value)),
                                false,
                                trigger,
                            )
                            .await;
                        }
                    } else if !newer_online_subkeys.is_empty() {
                        this.report_change_inspect(
                            record_key.clone(),
                            &opaque_record_key,
                            newer_online_subkeys,
                            None,
                            false,
                            trigger,
                        )
                        .await;
                    }
                    VeilidAPIResult::Ok(())
                }
                .await;

                if let Err(e) = res {
                    // A failed inspection leaves missed value changes missed forever;
                    // requeue while the watch still exists
                    veilid_log!(this debug "Failed to inspect record for changes, will retry: {}", e);
                    this.requeue_failed_change_inspect(record_key, subkeys, trigger);
                }
            }
        };
        pin_dyn_future!(fut)
    }

    /// Report what a change inspection found, one subkey with its value or a range the app reads
    /// for itself: decrement the watch count, force a reconcile for fresh routes if pushes were
    /// missed, unless a reconcile is what asked for the inspection, and (for the non-committing
    /// hint path) record the reported sequence number. Holds the watch record lock across the
    /// update.
    async fn report_change_inspect(
        &self,
        record_key: RecordKey,
        opaque_record_key: &OpaqueRecordKey,
        changed_subkeys: ValueSubkeyRangeSet,
        opt_value: Option<(ValueSubkey, ValueData)>,
        record_reported_seq: bool,
        trigger: ChangeInspectTrigger,
    ) {
        let record_lock = self
            .record_lock_table
            .lock_record(
                opaque_record_key.clone(),
                StorageManagerRecordLockPurpose::WatchUpdate,
            )
            .await;

        let remaining_count = {
            let owm = &mut *self.outbound_watch_manager_mut();
            let Some(outbound_watch) = owm.outbound_watches.get_mut(opaque_record_key) else {
                veilid_log!(self debug "change inspect report skipped: no outbound watch for {}", opaque_record_key);
                return;
            };
            let Some(state) = outbound_watch.state_mut() else {
                veilid_log!(self debug "change inspect report skipped: no outbound watch state for {}", opaque_record_key);
                return;
            };
            let remaining_count = state.remaining_count().saturating_sub(1);
            let cur_ts = Timestamp::now_non_decreasing();
            let missing_value_changes = state.unpushed_since_fallback()
                || state.needs_fallback_inspect(cur_ts, self.timings().watch_inspect_max_interval);
            state.edit(&owm.per_node_states, |editor| {
                editor.set_remaining_count(remaining_count);
                if let (Some((subkey, value)), true) = (&opt_value, record_reported_seq) {
                    editor.set_last_reported_subkey_seq(*subkey, value.seq());
                }
                if missing_value_changes && trigger.may_force_reconcile() {
                    editor.set_forced_reconcile();
                }
            });
            remaining_count
        };

        self.update_callback_value_change(
            record_lock,
            record_key,
            changed_subkeys,
            remaining_count,
            opt_value.map(|(_, value)| value),
        );
    }

    /// Diagnose a watch's notification routes after a productive change inspect: the
    /// inspect answered, so our receive path works and a missed push implicates the
    /// bound routes. Logs whether each is still allocated (present-but-broken vs
    /// missing-but-unnoticed) and requests a cooldown-bypassed test of the present ones.
    #[cfg(feature = "verbose-tracing")]
    fn diagnose_watch_notification_routes(&self, opaque_record_key: &OpaqueRecordKey) {
        let value_changed_routes: Vec<PublicKey> = {
            let owm = self.outbound_watch_manager.lock();
            let Some(outbound_watch) = owm.outbound_watches.get(opaque_record_key) else {
                return;
            };
            let Some(state) = outbound_watch.state() else {
                return;
            };
            state.value_changed_routes().iter().cloned().collect()
        };
        if value_changed_routes.is_empty() {
            veilid_log!(self debug "watch route diagnosis: no notification routes bound ({})", opaque_record_key);
            return;
        }

        let routing_table = self.routing_table();
        let rss = routing_table.route_spec_store();
        for vcr in value_changed_routes {
            match rss.get_route_id_for_key(&vcr) {
                Some(route_id) => {
                    veilid_log!(self debug "watch route diagnosis: notification route {} allocated as {:#}, requesting test ({})", vcr, route_id, opaque_record_key);
                    routing_table.request_route_test(route_id);
                }
                None => {
                    let allocated = rss
                        .list_allocated_routes(|k, _v| Some(k.to_string()))
                        .join(",");
                    veilid_log!(self warn "watch route diagnosis: notification route {} MISSING but watch not renewed; allocated routes [{}] ({})", vcr, allocated, opaque_record_key);
                }
            }
        }
    }

    fn get_change_inspection_operation_transactional_inner(
        registry: VeilidComponentRegistry,
        record_key: RecordKey,
        subkeys: ValueSubkeyRangeSet,
        transaction_membership: TransactionMembership,
        trigger: ChangeInspectTrigger,
        fanout_config: FanoutConfig,
    ) -> PinBoxFutureStatic<()> {
        let fut = {
            let opaque_record_key = record_key.opaque();
            async move {
                let this = registry.storage_manager();

                // Run inside a background transaction over the whole stored set so nothing the
                // notification path can't see is committed. The closure only inspects/gets and
                // reports a non-committing hint; the helper rolls the transaction back on teardown.
                let closure_registry = registry.clone();
                let closure_record_key = record_key.clone();
                let closure_subkeys = subkeys.clone();
                let res = async {
                    // Non-transactional pre-check: skip the transaction when nothing is newer online
                    let report = this
                        .inspect_record_with_fanout(
                            record_key.clone(),
                            subkeys.clone(),
                            DHTReportScope::SyncGet,
                            fanout_config,
                            RecordLockPriority::Background,
                        )
                        .await?;
                    if report.newer_online_subkeys().is_empty() {
                        veilid_log!(this debug "change inspect: nothing newer online for {}", record_key);
                        this.note_change_inspect_outcome(&opaque_record_key, false);
                        return VeilidAPIResult::Ok(());
                    }
                    veilid_log!(this debug "change inspect: PRODUCTIVE: missed value change on {} subkeys {}", record_key, report.newer_online_subkeys());
                    this.note_change_inspect_outcome(&opaque_record_key, true);
                    #[cfg(feature = "verbose-tracing")]
                    this.diagnose_watch_notification_routes(&opaque_record_key);

                    this.with_background_transaction(
                        record_key.clone(),
                        transaction_membership,
                        move |transaction_handle, _members| async move {
                            let this = closure_registry.storage_manager();
                            this.transactional_change_inspect_and_report(
                                transaction_handle,
                                closure_record_key,
                                opaque_record_key,
                                closure_subkeys,
                                trigger,
                            )
                            .await;
                            VeilidAPIResult::Ok(())
                        },
                    )
                    .await
                }
                .await;

                match res {
                    Ok(()) => {}
                    // Contended, unanswered, or the begin did not complete
                    Err(VeilidAPIError::TryAgain { .. } | VeilidAPIError::Timeout) => {
                        this.requeue_failed_change_inspect(record_key, subkeys, trigger);
                    }
                    Err(e) => {
                        veilid_log!(this debug "transactional change inspection failed: {}", e);
                    }
                }
            }
        };
        pin_dyn_future!(fut)
    }

    /// Inspect a transactional record's changes and report them as a non-committing hint: one
    /// changed subkey with its value, a wider range as it stands. Runs inside the caller's
    /// transaction (which is rolled back), so the local store is never modified.
    async fn transactional_change_inspect_and_report(
        &self,
        transaction_handle: OutboundTransactionHandle,
        record_key: RecordKey,
        opaque_record_key: OpaqueRecordKey,
        subkeys: ValueSubkeyRangeSet,
        trigger: ChangeInspectTrigger,
    ) {
        let report = match self.transaction_inspect(
            transaction_handle,
            record_key.clone(),
            Some(subkeys),
            DHTReportScope::SyncGet,
        ) {
            Ok(v) => v,
            Err(e) => {
                veilid_log!(self debug "Failed to inspect record for changes (transactional): {}", e);
                return;
            }
        };
        let newer_online_subkeys = report.newer_online_subkeys();

        if newer_online_subkeys.len() == 1 {
            let changed_subkey = newer_online_subkeys.first().unwrap_or_log();
            let value = match self
                .transaction_get(transaction_handle, record_key.clone(), changed_subkey)
                .await
            {
                Ok(v) => v,
                Err(e) => {
                    veilid_log!(self debug "Failed to get changed record (transactional): {}", e);
                    return;
                }
            };
            let local_seq = report
                .local_seq_for(changed_subkey)
                .unwrap_or(ValueSeqNum::NONE);
            if let Some(value) = value.filter(|value| value.seq() > local_seq) {
                // Non-committing hint: track the reported seq since nothing is written locally
                self.report_change_inspect(
                    record_key,
                    &opaque_record_key,
                    newer_online_subkeys,
                    Some((changed_subkey, value)),
                    true,
                    trigger,
                )
                .await;
            }
        } else if !newer_online_subkeys.is_empty() {
            self.report_change_inspect(
                record_key,
                &opaque_record_key,
                newer_online_subkeys,
                None,
                false,
                trigger,
            )
            .await;
        }
    }

    /// Handle a received 'Watch Value' query
    #[cfg_attr(
        feature = "instrument",
        instrument(level = "trace", target = "dht", skip_all)
    )]
    pub async fn inbound_watch_value(
        &self,
        opaque_record_key: OpaqueRecordKey,
        params: InboundWatchParameters,
        opt_raw_watch_id: Option<u64>,
    ) -> VeilidAPIResult<NetworkResult<InboundWatchValueResult>> {
        // Validate input
        if params.count == 0 && (opt_raw_watch_id.unwrap_or_default() == 0) {
            // Can't cancel a watch without a watch id
            return VeilidAPIResult::Ok(NetworkResult::invalid_message(
                "can't cancel watch without id",
            ));
        }

        let remote_record_store = self.get_remote_record_store()?;

        let opt_watch_id = match opt_raw_watch_id {
            Some(raw_watch_id) => {
                match remote_record_store.lookup_inbound_watch_id(raw_watch_id)? {
                    Some(id) => Some(id),
                    None => {
                        return Ok(NetworkResult::value(InboundWatchValueResult::Rejected));
                    }
                }
            }
            None => None,
        };

        if remote_record_store.contains_record(&opaque_record_key) {
            return remote_record_store
                .watch_record(opaque_record_key, params, opt_watch_id)
                .await
                .map(NetworkResult::value);
        }

        // No record found
        Ok(NetworkResult::value(InboundWatchValueResult::Rejected))
    }

    fn get_watched_subkeys_inner(
        &self,
        owm: &OutboundWatchManager,
        record_key: &RecordKey,
    ) -> VeilidAPIResult<Option<ValueSubkeyRangeSet>> {
        let opaque_record_key = record_key.opaque();

        // Get the outbound watch
        let Some(outbound_watch) = owm.outbound_watches.get(&opaque_record_key) else {
            // No outbound watch means no callback
            return Ok(None);
        };

        let Some(desired) = outbound_watch.desired() else {
            // No outbound watch desired state means no callback (we are cancelling)
            return Ok(None);
        };

        Ok(Some(desired.subkeys))
    }

    fn update_watch_per_node_state_inner(
        &self,
        owm: &mut OutboundWatchManager,
        record_key: &RecordKey,
        count: u32,
        inbound_node_id: &NodeId,
        watch_id: u64,
    ) -> VeilidAPIResult<bool> {
        let opaque_record_key = record_key.opaque();

        // Get the outbound watch
        let Some(outbound_watch) = owm.outbound_watches.get_mut(&opaque_record_key) else {
            // No outbound watch means no callback
            return Ok(false);
        };

        let Some(state) = outbound_watch.state_mut() else {
            // No outbound watch current state means no callback (we haven't reconciled yet)
            return Ok(false);
        };

        // If the reporting node is not part of our current watch, don't process the value change
        let pnk = PerNodeKey {
            record_key: record_key.clone(),
            node_id: inbound_node_id.clone(),
        };
        // A push on a watch we do not hold is a stray: one whose acceptance never reached us, or
        // one replaced by a newer watch at its node. Cancel it once a fanout still answering has
        // had time to claim it.
        let stray_cancel_ts = Timestamp::now().later(TimestampDuration::new_ms(
            self.config()
                .internal()
                .network
                .dht
                .get_value_timeout_ms
                .into(),
        ));
        if !state.nodes().contains(&pnk) {
            // A node left out for an unanswered renewal keeps its per-node state, and a push on
            // its watch id shows that watch is alive
            if owm
                .per_node_states
                .get(&pnk)
                .is_none_or(|pns| pns.watch_id != watch_id)
            {
                owm.queue_watch_cancel(pnk, watch_id, stray_cancel_ts);
                return Ok(false);
            }
            veilid_log!(self debug target: "watch",
                "OutboundWatch({}): {} rejoins on a push to watch id {}",
                record_key, inbound_node_id, watch_id
            );
            state.edit(&owm.per_node_states, |editor| {
                editor.add_nodes([pnk.clone()]);
            });
        }

        // Get per node state
        let Some(per_node_state) = owm.per_node_states.get_mut(&pnk) else {
            // No per node state means no callback
            veilid_log!(self warn "Missing per node state in outbound watch: {:?}", pnk);
            return Ok(false);
        };

        // If watch id doesn't match it's for an older watch and should be ignored
        if per_node_state.watch_id != watch_id {
            veilid_log!(self debug "Incorrect watch id for per node state in outbound watch: {:?} {} != {}", pnk, per_node_state.watch_id, watch_id);
            owm.queue_watch_cancel(pnk, watch_id, stray_cancel_ts);
            return Ok(false);
        }

        // Update per node state
        if count > per_node_state.count {
            // If count is greater than our requested count then this is invalid, cancel the watch
            // XXX: Should this be a punishment?
            veilid_log!(self debug
                "Watch count went backward: {} @ {} id={}: {} > {}",
                record_key,
                inbound_node_id,
                watch_id,
                count,
                per_node_state.count
            );

            // Force count to zero for this node id so it gets cancelled out by the background process
            per_node_state.count = 0;
            return Ok(false);
        } else if count == per_node_state.count {
            // If a packet gets delivered twice or something else is wrong, report a non-decremented watch count
            // Log this because watch counts should always be decrementing non a per-node basis.
            // XXX: Should this be a punishment?
            veilid_log!(self debug
                "Watch count duplicate: {} @ {} id={}: {} == {}",
                record_key,
                inbound_node_id,
                watch_id,
                count,
                per_node_state.count
            );
        } else {
            // Reduce the per-node watch count
            veilid_log!(self debug
                "Watch count decremented: {} @ {} id={}: {} < {}",
                record_key,
                inbound_node_id,
                watch_id,
                count,
                per_node_state.count
            );
            per_node_state.count = count;
        }

        // A valid ValueChanged resets the fallback inspection timer
        // NOTE: It is possible this should be a per-node state, but for now let's try it on the whole watch
        // Done after per_node_state usage to avoid borrow conflicts with owm.per_node_states
        state.edit(&owm.per_node_states, |editor| {
            editor.touch_value_changed(Timestamp::now_non_decreasing());
        });

        Ok(true)
    }

    async fn accept_single_value_change_and_report(
        &self,
        record_lock: StorageManagerRecordLockGuard,
        record_key: RecordKey,
        reportable_subkeys: ValueSubkeyRangeSet,
        value: Arc<SignedValueData>,
    ) -> VeilidAPIResult<()> {
        let opaque_record_key = record_lock.record();

        // Set the local value
        self.handle_set_single_local_value_with_single_record_lock(
            &record_lock,
            reportable_subkeys.first().unwrap_or_log(),
            value.clone(),
        )
        .await?;

        // Mark the report and take the count under one guard
        let remaining_count = {
            let owm = &mut *self.outbound_watch_manager_mut();
            if let Some(state) = Self::watch_state_mut_inner(owm, &opaque_record_key) {
                // Data in hand: later range-only copies of this change must not re-notify
                state.mark_value_reported_subkeys(&reportable_subkeys);
            }
            Self::note_watch_report_inner(owm, &opaque_record_key, None)
        };

        // Announce ValueChanged VeilidUpdate
        // Cancellations (count=0) are sent by process_outbound_watch_dead(), not here
        let value = self
            .maybe_decrypt_value_data(&record_key, value.value_data())
            .await?;

        // We have a single value with a newer sequence number to report
        self.update_callback_value_change(
            record_lock,
            record_key,
            reportable_subkeys,
            remaining_count,
            Some(value),
        );

        Ok(())
    }

    /// Report a value-change to the app as a non-committing hint (no local-store write) and
    /// decrement the watch count. When a value is present its decrypted form is reported and its
    /// sequence number recorded so replays and duplicate reports are gated out.
    ///
    /// One guard covers the whole decision: re-trim to the currently watched range, mark the
    /// reported subkeys (news-filtering range-only reports), and take the count. Sends nothing
    /// when no watched subkey remains, the range-only news is empty, or the watch has died.
    async fn report_value_change_hint(
        &self,
        record_lock: StorageManagerRecordLockGuard,
        record_key: RecordKey,
        reportable_subkeys: ValueSubkeyRangeSet,
        opt_value: Option<Arc<SignedValueData>>,
    ) -> VeilidAPIResult<()> {
        let opaque_record_key = record_lock.record();

        let (subkeys, remaining_count) = {
            let owm = &mut *self.outbound_watch_manager_mut();
            let Some(watched) = self.get_watched_subkeys_inner(owm, &record_key)? else {
                return Ok(());
            };
            let reportable = reportable_subkeys.intersect(&watched);
            if reportable.is_empty() {
                return Ok(());
            }
            let Some(state) = Self::watch_state_mut_inner(owm, &opaque_record_key) else {
                return Ok(());
            };
            let subkeys = if opt_value.is_some() {
                state.mark_value_reported_subkeys(&reportable);
                reportable
            } else {
                state.mark_hinted_subkeys(&reportable)
            };
            if subkeys.is_empty() {
                return Ok(());
            }
            let opt_reported = opt_value
                .as_ref()
                .map(|v| (subkeys.first().unwrap_or_log(), v.value_data().seq()));
            (
                subkeys,
                Self::note_watch_report_inner(owm, &opaque_record_key, opt_reported),
            )
        };

        let opt_value_data = if let Some(value) = &opt_value {
            Some(
                self.maybe_decrypt_value_data(&record_key, value.value_data())
                    .await?,
            )
        } else {
            None
        };

        self.update_callback_value_change(
            record_lock,
            record_key,
            subkeys,
            remaining_count,
            opt_value_data,
        );

        Ok(())
    }

    /// Decrement and return the watch's remaining change count for a report,
    /// recording the reported seq that gates transactional hints.
    fn note_watch_report_inner(
        owm: &mut OutboundWatchManager,
        opaque_record_key: &OpaqueRecordKey,
        opt_reported: Option<(ValueSubkey, ValueSeqNum)>,
    ) -> u32 {
        if let Some(outbound_watch) = owm.outbound_watches.get_mut(opaque_record_key) {
            if let Some(state) = outbound_watch.state_mut() {
                let rc = state.remaining_count().saturating_sub(1);
                state.edit(&owm.per_node_states, |editor| {
                    editor.set_remaining_count(rc);
                    if let Some((subkey, seq)) = opt_reported {
                        editor.set_last_reported_subkey_seq(subkey, seq);
                    }
                });
                return rc;
            }
        }
        // No outbound watch or state means the watch died (no remaining count)
        0
    }

    /// Handle a received 'Value Changed' statement
    #[cfg_attr(
        feature = "instrument",
        instrument(level = "debug", target = "watch", skip_all)
    )]
    pub async fn inbound_value_changed(
        &self,
        opaque_record_key: OpaqueRecordKey,
        subkeys: ValueSubkeyRangeSet,
        count: u32,
        mut opt_value: Option<Arc<SignedValueData>>,
        inbound_node_id: NodeId,
        watch_id: u64,
    ) -> VeilidAPIResult<NetworkResult<()>> {
        let Ok(record_key) = self.get_record_key_for_opaque_record_key(&opaque_record_key) else {
            // value change received for unopened key
            return Ok(NetworkResult::value(()));
        };

        // Operate on the watch for this record
        let record_lock = self
            .record_lock_table
            .lock_record(
                record_key.opaque(),
                StorageManagerRecordLockPurpose::WatchUpdate,
            )
            .await;

        // A record currently in a transaction set is reported as a non-committing hint
        // rather than being committed to the local store.
        let record_is_transactional = self
            .get_local_record_store()?
            .with_record(&opaque_record_key, |r| {
                r.detail().transaction_membership.is_some()
            })?
            .unwrap_or(false);

        // Determine if we have reportable subkeys
        // Also clears the value if the value should not be reported
        let mut reportable_subkeys = {
            let owm = &mut *self.outbound_watch_manager_mut();

            // No subkeys means remote node cancelled, but we already captured that with the
            // assignment of 'count' to the per_node_state above, so we can just jump out here
            let Some(first_subkey) = subkeys.first() else {
                return Ok(NetworkResult::value(()));
            };

            // If more than one subkey was reported then ignore any value because we want
            // to do all our watch updates transactionally and we'll just kick off a change inspection
            if subkeys.len() > 1 {
                opt_value = None;
            }

            // Get what subkeys are being watched
            let Some(watched_subkeys) = self.get_watched_subkeys_inner(owm, &record_key)? else {
                // Nothing watched, nothing to report
                return Ok(NetworkResult::value(()));
            };

            // Update the per-node watch state; skip if rejected
            let accepted = self.update_watch_per_node_state_inner(
                owm,
                &record_key,
                count,
                &inbound_node_id,
                watch_id,
            )?;
            if !accepted {
                return Ok(NetworkResult::value(()));
            }

            // If our watched subkey range differs from the reported change's range
            // we should only report changes that we care about
            let reportable_subkeys = subkeys.intersect(&watched_subkeys);
            if let Some(first_reportable_subkey) = reportable_subkeys.first() {
                if first_reportable_subkey != first_subkey {
                    opt_value = None;
                }
            } else {
                opt_value = None;
            }

            reportable_subkeys
        };

        // See if the value we received can be reported as-is
        if let Some(value) = opt_value.clone() {
            // Safe to unwrap here because if we have reportable subkeys, there must be
            // a first reportable subkey, and if not, the value is set to None
            let first_subkey = reportable_subkeys.first().unwrap_or_log();

            let last_get_result = self
                .handle_get_single_local_value(&opaque_record_key, first_subkey, true)
                .await?;

            let descriptor = last_get_result.opt_descriptor.unwrap_or_log();

            // Verify signatures before reporting
            network_result_try!(
                self.verify_inbound_signed_value(
                    &opaque_record_key,
                    &descriptor,
                    &value,
                    first_subkey
                )
                .await
            );

            let schema = descriptor.schema()?;

            // Validate with schema
            if self
                .check_subkey_value_data(
                    &schema,
                    descriptor.ref_owner(),
                    first_subkey,
                    value.value_data(),
                )
                .is_err()
            {
                // Validation failed, ignore this value
                // Move to the next node
                return Ok(NetworkResult::invalid_message(format!(
                    "Schema validation failed on subkey {}",
                    first_subkey
                )));
            }

            // Make sure this value would actually be newer than what we last stored (commit path)
            // or last reported (transactional hint path, which doesn't touch the local store)
            let opt_last_seq = if record_is_transactional {
                self.outbound_watch_manager
                    .lock()
                    .outbound_watches
                    .get(&opaque_record_key)
                    .and_then(|ow| ow.state())
                    .and_then(|s| s.last_reported_subkey_seq(first_subkey))
            } else {
                last_get_result
                    .opt_value
                    .as_ref()
                    .map(|v| v.value_data().seq())
            };
            if let Some(last_seq) = opt_last_seq {
                if value.value_data().seq() <= last_seq {
                    // Not newer than what we last stored/reported, so we're not going to report this
                    opt_value = None;

                    // Shrink up the subkey range because we're removing the first value from the things we'd possibly report on
                    reportable_subkeys.pop_first().unwrap_or_log();
                    if reportable_subkeys.is_empty() {
                        // If there's nothing left to report, just return no
                        return Ok(NetworkResult::value(()));
                    }
                }
            }
        }

        // Report the change to the app
        if let Some(value) = opt_value.clone() {
            if record_is_transactional {
                // Non-committing hint: report the validated value but do NOT write the local store
                self.report_value_change_hint(
                    record_lock,
                    record_key,
                    reportable_subkeys,
                    Some(value),
                )
                .await?;
            } else {
                // Commit the value locally and report it
                self.accept_single_value_change_and_report(
                    record_lock,
                    record_key,
                    reportable_subkeys,
                    value,
                )
                .await?;
            }
        } else if !reportable_subkeys.is_empty() {
            // No single reportable value (e.g. multiple subkeys changed): report the range so the app fetches it
            self.report_value_change_hint(record_lock, record_key, reportable_subkeys, None)
                .await?;
        }

        Ok(NetworkResult::value(()))
    }

    /// This record's outbound watch state, if the watch and its state exist.
    fn watch_state_mut_inner<'a>(
        owm: &'a mut OutboundWatchManager,
        opaque_record_key: &OpaqueRecordKey,
    ) -> Option<&'a mut OutboundWatchState> {
        owm.outbound_watches.get_mut(opaque_record_key)?.state_mut()
    }

    /// Consume the report marks for subkeys the app read from the network, so a later change is
    /// news again. A subkey whose copies were suppressed mid-read queues one repeat hint (the
    /// racing copy may be a newer change), which the outbound watch check tick sends.
    pub(super) fn consume_watch_read_subkeys(
        &self,
        opaque_record_key: &OpaqueRecordKey,
        subkeys: &ValueSubkeyRangeSet,
    ) {
        let Ok(record_key) = self.get_record_key_for_opaque_record_key(opaque_record_key) else {
            return;
        };
        let owm = &mut *self.outbound_watch_manager_mut();
        let Some(owed) = Self::watch_state_mut_inner(owm, opaque_record_key)
            .map(|state| state.consume_read_subkeys(subkeys))
        else {
            return;
        };
        if !owed.is_empty() {
            owm.pending_rehints
                .entry(record_key)
                .and_modify(|s| *s = s.union(&owed))
                .or_insert(owed);
        }
    }

    /// Send any owed repeat hints. Runs from the outbound watch check tick.
    pub(super) async fn send_pending_rehints(&self) {
        let rehints: Vec<(RecordKey, ValueSubkeyRangeSet)> = {
            let owm = &mut *self.outbound_watch_manager_mut();
            owm.pending_rehints.drain().collect()
        };
        for (record_key, subkeys) in rehints {
            // Record lock before inner, as everywhere
            let record_lock = self
                .record_lock_table
                .lock_record(
                    record_key.opaque(),
                    StorageManagerRecordLockPurpose::WatchUpdate,
                )
                .await;
            if let Err(e) = self
                .report_value_change_hint(record_lock, record_key, subkeys, None)
                .await
            {
                veilid_log!(self debug "failed to send repeat hint: {}", e);
            }
        }
    }

    /// Send the watch cancels that are due: watches beyond consensus, and strays
    pub(super) async fn send_pending_watch_cancels(&self) {
        let cancels = self
            .outbound_watch_manager_mut()
            .take_due_watch_cancels(Timestamp::now());
        let routing_table = self.routing_table();
        let mut unord = FuturesUnordered::new();
        for cancel in cancels {
            let node_ref = match routing_table.lookup_node_id(cancel.pnk.node_id.clone()) {
                Ok(Some(node_ref)) => node_ref,
                Ok(None) => {
                    self.outbound_watch_manager_mut()
                        .finish_watch_cancel(&cancel.pnk, cancel.watch_id);
                    continue;
                }
                Err(e) => {
                    veilid_log!(self debug "Error looking up watch cancel node: {}", e);
                    self.outbound_watch_manager_mut()
                        .finish_watch_cancel(&cancel.pnk, cancel.watch_id);
                    continue;
                }
            };
            unord.push(async move {
                let res = self
                    .outbound_watch_value_cancel(
                        cancel.pnk.record_key.opaque(),
                        cancel.opt_watcher,
                        cancel.safety_selection,
                        node_ref,
                        cancel.watch_id,
                    )
                    .await;
                (cancel.pnk, cancel.watch_id, res)
            });
        }
        while let Some((pnk, watch_id, res)) = unord.next().await {
            if let Err(e) = res {
                veilid_log!(self debug "Watch cancel failed: id={} ({}): {}", watch_id, pnk.node_id, e);
            }
            self.outbound_watch_manager_mut()
                .finish_watch_cancel(&pnk, watch_id);
        }
    }

    /// Feed a completed change inspection back into its watch's fallback interval.
    /// Finding nothing is the common case and earns a longer wait; finding a missed
    /// change means the watch is not delivering and we want to look again promptly.
    pub(super) fn note_change_inspect_outcome(
        &self,
        opaque_record_key: &OpaqueRecordKey,
        productive: bool,
    ) {
        let survival = self
            .network_manager()
            .online_detector()
            .survival(RoutingDomain::PublicInternet);
        let owm = &mut *self.outbound_watch_manager_mut();
        let Some(outbound_watch) = owm.outbound_watches.get_mut(opaque_record_key) else {
            return;
        };
        outbound_watch.clear_change_inspect_failures();
        let Some(state) = outbound_watch.state_mut() else {
            return;
        };
        let max_interval = self.timings().watch_inspect_max_interval;
        state.edit(&owm.per_node_states, |editor| {
            if productive {
                editor.reset_fallback_inspect_backoff(None);
            } else {
                editor.back_off_fallback_inspect(max_interval);
            }
            if let Some(survival) = survival {
                editor.stretch_fallback_inspect(survival, max_interval);
            }
        });
    }

    /// Requeue a change inspection that failed or was deferred for foreground activity.
    ///
    /// A failure learned nothing about the record: its nodes did not answer, or we could not
    /// reach them. It retries through `failure_backoff`, or when a running quiet window ends if
    /// that is sooner, and while we are offline the queue holds it rather than spending it.
    pub(super) fn requeue_failed_change_inspect(
        &self,
        record_key: RecordKey,
        subkeys: ValueSubkeyRangeSet,
        trigger: ChangeInspectTrigger,
    ) {
        let cur_ts = Timestamp::now_non_decreasing();
        let survival = self
            .network_manager()
            .online_detector()
            .survival(RoutingDomain::PublicInternet);
        let max_interval = self.timings().watch_inspect_max_interval;
        let quiet_until = self.background_quiet_until();
        let owm = &mut *self.outbound_watch_manager_mut();
        let Some(outbound_watch) = owm.outbound_watches.get_mut(&record_key.opaque()) else {
            return;
        };
        let failures = outbound_watch.note_change_inspect_failure();
        let delay = failure_backoff(
            CHANGE_INSPECT_RETRY_INTERVAL,
            max_interval,
            failures,
            survival,
        )
        .unwrap_or(max_interval);
        let retry_ts = cur_ts.later(delay);
        let not_before = if quiet_until > cur_ts {
            quiet_until.min(retry_ts)
        } else {
            retry_ts
        };
        owm.enqueue_change_inspect_after(record_key, subkeys, not_before, trigger);
    }

    /// Check all watches for changes
    /// Used when we come back online from being offline and may have
    /// missed some ValueChanged notifications
    #[cfg_attr(
        feature = "instrument",
        instrument(level = "trace", target = "watch", skip_all)
    )]
    pub fn change_inspect_all_watches(&self) {
        let owm = &mut *self.outbound_watch_manager_mut();

        let mut change_inspects = vec![];
        for outbound_watch in owm.outbound_watches.values() {
            if let Some(state) = outbound_watch.state() {
                let reportable_subkeys = state.params().subkeys.clone();
                change_inspects.push((outbound_watch.record_key(), reportable_subkeys));
            }
        }

        if change_inspects.is_empty() {
            return;
        }

        veilid_log!(self debug "change inspecting {} watches", change_inspects.len());

        for change_inspect in change_inspects {
            owm.enqueue_change_inspect(
                change_inspect.0,
                change_inspect.1,
                ChangeInspectTrigger::Watch,
            );
        }
    }
}
