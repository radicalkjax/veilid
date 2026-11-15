use super::*;

impl_veilid_log_facility!("rpc");

#[derive(Copy, Clone, Debug, PartialEq, PartialOrd, Ord, Eq, Hash, Default)]
pub struct StatusSenderInfo {
    pub opt_sender_info: Option<SenderInfo>,
    pub opt_previous_sender_info: Option<SenderInfo>,
}

/// Answer to a status RPC plus the transport attempt that carried it at the first hop
#[derive(Debug)]
pub struct StatusAnswer {
    /// The peer's status answer
    pub answer: Answer<StatusSenderInfo>,
    /// The first-hop send attempt that carried the question
    pub send_data_result: SendDataResult,
    /// The datagram a probe's answer arrived whole in, as evidence of the size the path carried
    #[cfg_attr(all(target_arch = "wasm32", target_os = "unknown"), expect(dead_code))]
    pub opt_mtu_probe_datagram_len: Option<u16>,
}

/// Outcome of `rpc_call_status`
pub type StatusResult = RpcOutcome<StatusAnswer>;

impl RPCProcessor {
    // Send StatusQ RPC request, receive StatusA answer
    // Can be sent via relays or routes, but will have less information via routes
    // sender:
    // unsafe -> node status
    // safe -> nothing
    // receiver:
    // direct -> node status + sender info
    // safety -> node status
    // private -> nothing
    // `opt_mtu_probe_size` sends a probe that fills one datagram of that size and asks for an
    // answer of the same size; its loss counts against neither the peer nor our online state
    #[cfg_attr(feature = "instrument", instrument(level = "trace", target = "rpc", skip(self), ret, err(level=Level::DEBUG)))]
    pub async fn rpc_call_status(
        &self,
        dest: Destination,
        opt_mtu_probe_size: Option<u16>,
    ) -> Result<StatusResult, RPCError> {
        let _guard = self
            .startup_context
            .startup_lock
            .enter()
            .map_err(RPCError::map_try_again("not started up"))?;

        // Determine routing domain and node status to send
        let routing_table = self.routing_table();
        let (opt_target_nr, routing_domain, node_status) = if let Some(UnsafeRoutingInfo {
            opt_node,
            opt_routing_domain,
        }) =
            dest.get_unsafe_routing_info(&routing_table)
        {
            let Some(routing_domain) = opt_routing_domain else {
                if let Some(node) = &opt_node {
                    self.record_unreachable(node.clone());
                }
                return Ok(StatusResult::NotSent(NetworkResult::no_connection_other(
                    "no routing domain for target",
                )));
            };

            let node_status = Some(self.network_manager().generate_node_status(routing_domain));
            (opt_node, routing_domain, node_status)
        } else {
            // Safety route means we don't exchange node status and things are all PublicInternet RoutingDomain
            (None, RoutingDomain::PublicInternet, None)
        };

        // Get destination respond_to
        let respond_to = match self.get_destination_respond_to(&dest).await? {
            NetworkResult::Value(v) => v,
            other => return Ok(StatusResult::NotSent(other.map(|_| ()))),
        };

        // Create status rpc question
        let opt_mtu_probe = opt_mtu_probe_size.map(|size| {
            let mut padding = vec![0u8; MAX_MTU_PROBE_PADDING_LEN];
            random_bytes(&mut padding);
            MtuProbe { size, padding }
        });
        let status_q = RPCOperationStatusQ::new(node_status, opt_mtu_probe);
        let question = RPCQuestion::new(respond_to, RPCQuestionDetail::StatusQ(Box::new(status_q)));

        let debug_string = format!("Status => {:#}", dest);

        // Send the info request
        let waitable_reply = match self.question(dest.clone(), question, None, None).await? {
            QuestionOutcome::Sent(wr) => *wr,
            QuestionOutcome::Failed(sdr) => return Ok(StatusResult::Failed(sdr)),
            QuestionOutcome::NotSent(nr) => return Ok(StatusResult::NotSent(nr)),
        };

        // Note what kind of ping this was and to what peer scope
        let send_direct_dial_info = waitable_reply.context.send_data_result.direct_dial_info();
        let Some(send_unique_flow) = waitable_reply.context.send_data_result.unique_flow() else {
            return Err(RPCError::internal(
                "status question sent without a unique flow",
            ));
        };

        // Snapshot send_data_result for the return value (NCM is Clone; success-path NR is
        // Value(UniqueFlow) which is Copy)
        let send_data_result = SendDataResult::new(
            waitable_reply
                .context
                .send_data_result
                .node_contact_method_result()
                .clone(),
            NetworkResult::Value(send_unique_flow),
        );

        // Wait for reply
        let (msg, answer_context) = match self.wait_for_reply(waitable_reply, debug_string).await? {
            NetworkResult::Value(v) => v,
            _ => return Ok(StatusResult::Failed(send_data_result)),
        };
        let opt_answer_datagram_len = match &msg.header.detail {
            RPCMessageHeaderDetail::Direct(detail) => detail.opt_datagram_len,
            _ => None,
        };

        // Get the right answer type
        let (_, _, kind) = msg.operation.destructure();
        let status_a = match kind {
            RPCOperationKind::Answer(a) => match a.destructure() {
                RPCAnswerDetail::StatusA(a) => a,
                _ => return Ok(StatusResult::Failed(send_data_result)),
            },
            _ => return Ok(StatusResult::Failed(send_data_result)),
        };
        let (a_node_status, sender_info, a_mtu_probe) = status_a.destructure();

        // Ensure the returned node status is the kind for the routing domain we asked for
        if let Some(target_nr) = opt_target_nr {
            if let Some(a_node_status) = a_node_status {
                // Update latest node status in routing table
                target_nr.update_node_status(routing_domain, a_node_status.clone());
            }
        }

        // Report sender_info IP addresses to network manager
        // Don't need to validate these addresses for the current routing domain
        // the address itself is irrelevant, and the remote node can lie anyway
        let mut opt_sender_info = None;
        let mut opt_previous_sender_info = None;
        match dest {
            Destination::Direct {
                node: target,
                safety_selection,
            } => {
                if matches!(safety_selection, SafetySelection::Unsafe(_)) {
                    if let Some(sender_info) = sender_info {
                        // A Direct send can ride an existing flow to a relay, whose address
                        // the peer then honestly reports as ours
                        let sent_to_contact_method_target =
                            send_direct_dial_info.as_ref().is_some_and(|di| {
                                di.socket_address() == *send_unique_flow.flow.remote_address()
                            });
                        #[cfg(feature = "verbose-tracing")]
                        if send_direct_dial_info.is_some() && !sent_to_contact_method_target {
                            veilid_log!(self debug
                                "ignoring sender info from {}: Direct to {} but sent via {}",
                                target,
                                send_direct_dial_info.as_ref().map(|di| di.to_string()).unwrap_or_default(),
                                send_unique_flow.flow.remote_address()
                            );
                        }
                        if sent_to_contact_method_target {
                            // Log accepts too, rejections alone can't be told apart from starvation
                            #[cfg(feature = "verbose-tracing")]
                            veilid_log!(self debug
                                "accepting sender info from {}: direct to {}, reports {}",
                                target,
                                send_unique_flow.flow.remote_address(),
                                sender_info.socket_address
                            );
                            // Directly requested status that actually gets sent directly and not over a relay will tell us what our IP address appears as
                            // If this changes, we'd want to know about that to reset the networking stack
                            opt_previous_sender_info = target.report_sender_info(
                                routing_domain,
                                send_unique_flow,
                                sender_info,
                            );
                        };
                        opt_sender_info = Some(sender_info);

                        // Report ping status results to network manager
                        if let Err(e) = self.event_bus().post(SocketAddressChangeEvent {
                            routing_domain,
                            socket_address: sender_info.socket_address,
                            old_socket_address: opt_previous_sender_info.map(|s| s.socket_address),
                            flow: send_unique_flow.flow,
                            reporting_peer: target.unfiltered(),
                        }) {
                            veilid_log!(self debug "Failed to post event: {}", e);
                        }
                    }
                }
            }
            Destination::DialInfo {
                dial_info: _,
                node: _,
            }
            | Destination::PrivateRoute {
                private_route: _,
                safety_selection: _,
            } => {
                // sender info is irrelevant over relays and routes
            }
        };
        Ok(StatusResult::Sent(Box::new(StatusAnswer {
            answer: Answer::new(
                answer_context,
                StatusSenderInfo {
                    opt_sender_info,
                    opt_previous_sender_info,
                },
            ),
            send_data_result,
            // What the answer claims proves nothing; the datagram it came in does
            opt_mtu_probe_datagram_len: a_mtu_probe.and(opt_answer_datagram_len),
        })))
    }

    ////////////////////////////////////////////////////////////////////////////////////////////////

    #[cfg_attr(feature = "instrument", instrument(level = "trace", target = "rpc", skip(self, msg), fields(msg.operation.op_id), ret, err))]
    pub(super) async fn process_status_q(&self, msg: Message) -> RPCNetworkResult<()> {
        // Get the question
        let kind = msg.operation.kind().clone();
        let status_q = match kind {
            RPCOperationKind::Question(q) => match q.destructure() {
                (_, RPCQuestionDetail::StatusQ(q)) => q,
                _ => return Ok(NetworkResult::invalid_message("not a status question")),
            },
            _ => return Ok(NetworkResult::invalid_message("not a question")),
        };
        let (q_node_status, q_mtu_probe) = status_q.destructure();

        // Only a question that arrived whole at its size gets an answer that size, never larger
        let opt_mtu_probe = match (&msg.header.detail, q_mtu_probe) {
            (RPCMessageHeaderDetail::Direct(detail), Some(q_mtu_probe))
                if detail.flow.protocol_type() == ProtocolType::UDP =>
            {
                let limit = self
                    .network_manager()
                    .own_mtus()
                    .limit(LowLevelTransportType {
                        low_level_protocol_type: LowLevelProtocolType::UDP,
                        address_type: detail.flow.address_type(),
                    });
                let filled = q_mtu_probe.size.saturating_sub(MTU_PROBE_FILL_SLACK);
                let question_filled = detail.opt_datagram_len.is_some_and(|len| len >= filled);
                (question_filled && limit >= q_mtu_probe.size).then_some(q_mtu_probe)
            }
            _ => None,
        };

        let (node_status, sender_info) = match &msg.header.detail {
            RPCMessageHeaderDetail::Direct(detail) => {
                let flow = detail.flow;
                let routing_domain = detail.routing_domain;

                // Ensure the node status from the question is the kind for the routing domain we received the request in
                if let Some(q_node_status) = q_node_status {
                    // update node status for the requesting node to our routing table
                    if let Some(sender_nr) = msg.opt_sender_nr.clone() {
                        // Update latest node status in routing table for the statusq sender
                        sender_nr.update_node_status(routing_domain, q_node_status.clone());
                    }
                }

                // Get the peer address in the returned sender info
                let sender_info = SenderInfo {
                    socket_address: *flow.remote_address(),
                };

                // Make status answer
                let node_status = self.network_manager().generate_node_status(routing_domain);
                (Some(node_status), Some(sender_info))
            }
            RPCMessageHeaderDetail::SafetyRouted(_) => {
                // Make status answer
                let node_status = self
                    .network_manager()
                    .generate_node_status(RoutingDomain::PublicInternet);
                (Some(node_status), None)
            }
            RPCMessageHeaderDetail::PrivateRouted(_) => (None, None),
        };

        // Make status answer
        let status_a = RPCOperationStatusA::new(node_status, sender_info, opt_mtu_probe);

        // Send status answer
        self.answer(
            msg,
            RPCAnswer::new(RPCAnswerDetail::StatusA(Box::new(status_a))),
            None,
        )
        .await
    }
}
