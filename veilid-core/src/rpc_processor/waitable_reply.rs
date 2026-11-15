use super::*;

#[derive(Debug)]
#[must_use]
pub struct WaitableReplyContext {
    pub timeout: TimestampDuration,
    pub send_ts: Timestamp,
    pub send_data_result: SendDataResult,
    pub node_ref: FilteredNodeRef,
    pub opt_safety_route_ref: Option<AllocatedRouteRef>,
    pub opt_remote_private_route_ref: Option<RemoteRouteRef>,
    pub opt_reply_private_route_ref: Option<AllocatedRouteRef>,
    pub destination: Destination,
    pub mtu_probe: bool,
}

impl WaitableReplyContext {
    pub fn is_routed(&self) -> bool {
        self.opt_safety_route_ref.is_some() || self.opt_remote_private_route_ref.is_some()
    }

    /// Whether both legs of the round trip went straight between us and the node
    pub fn is_direct(&self) -> bool {
        !self.is_routed() && self.opt_reply_private_route_ref.is_none()
    }

    /// Take the allocated route refs (safety + reply) held by this context.
    pub fn into_allocated_route_refs(self) -> Vec<AllocatedRouteRef> {
        self.opt_safety_route_ref
            .into_iter()
            .chain(self.opt_reply_private_route_ref)
            .collect()
    }

    #[cfg_attr(not(feature = "verbose-tracing"), expect(dead_code))]
    pub fn debug(&self, routing_table: &RoutingTable) -> String {
        let rss = routing_table.route_spec_store();

        let opt_safety_key = self.opt_safety_route_ref.as_ref().map(|r| r.route_key());
        let opt_srstr = opt_safety_key.map(|key| rss.display_route_by_key(key));
        let opt_remprstr = self
            .opt_remote_private_route_ref
            .as_ref()
            .map(|r| rss.display_route_by_key(r.route_key()));
        let opt_reply_key = self
            .opt_reply_private_route_ref
            .as_ref()
            .map(|r| r.route_key());
        let opt_repprstr = if opt_reply_key != opt_safety_key {
            opt_reply_key.map(|key| rss.display_route_by_key(key))
        } else {
            None
        };

        format!(
            "timeout={} send_ts={} send_data_result={} node={} dest={}{}{}{}",
            self.timeout,
            self.send_ts,
            self.send_data_result,
            self.node_ref,
            self.destination,
            if let Some(srstr) = opt_srstr {
                format!("\nsafety_route={}", srstr)
            } else {
                "".to_string()
            },
            if let Some(remprstr) = opt_remprstr {
                format!("\nremote_private_route={}", remprstr)
            } else {
                "".to_string()
            },
            if let Some(repprstr) = opt_repprstr {
                format!("\nreply_private_route={}", repprstr)
            } else {
                "".to_string()
            },
        )
    }
}

/// What an answer must match to be accepted for the question waiting on it
#[derive(Clone, Debug)]
pub(super) struct WaitingQuestion {
    /// Descriptor of the answer kind the question expects
    pub answer_desc: &'static str,
    /// Context the answer's validation needs, when the question carries one
    pub context: Option<Arc<QuestionContext>>,
}

#[derive(Debug)]
#[must_use]
pub(super) struct WaitableReply {
    pub handle: OperationWaitHandle<Result<Message, RPCError>, WaitingQuestion>,
    _opt_connection_ref_scope: Option<ConnectionRefScope>,
    pub context: WaitableReplyContext,
}

impl WaitableReply {
    pub fn new(
        handle: OperationWaitHandle<Result<Message, RPCError>, WaitingQuestion>,
        opt_connection_ref_scope: Option<ConnectionRefScope>,
        context: WaitableReplyContext,
    ) -> Self {
        Self {
            handle,
            _opt_connection_ref_scope: opt_connection_ref_scope,
            context,
        }
    }
}

/// Outcome of an RPC send
#[derive(Debug)]
#[must_use]
pub(crate) enum RpcOutcome<T> {
    /// Sent; carries what the caller waits on or received (boxed: much larger than the other variants)
    Sent(Box<T>),
    /// Send was attempted but didn't produce a Value; SendDataResult exposes what was tried
    Failed(SendDataResult),
    /// Never made it to send_envelope (render failure, no routing domain, etc.)
    NotSent(NetworkResult<()>),
}

impl<T> RpcOutcome<T> {
    /// Collapse for callers that don't need the SendDataResult
    pub(crate) fn into_network_result(self) -> NetworkResult<T> {
        match self {
            RpcOutcome::Sent(v) => NetworkResult::Value(*v),
            // PANIC-CENSUS: provably-dead the Failed arm carries no value to map
            RpcOutcome::Failed(sdr) => sdr.into_network_result().map(|_| unreachable!()),
            // PANIC-CENSUS: provably-dead the NotSent arm carries no value to map
            RpcOutcome::NotSent(nr) => nr.map(|_| unreachable!()),
        }
    }
}

/// Outcome of `RPCProcessor::question`
pub(super) type QuestionOutcome = RpcOutcome<WaitableReply>;
