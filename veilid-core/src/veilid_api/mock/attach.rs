use super::*;

/// How often a waiting condition rechecks, since nothing wakes it: there is no tick task under mock.
const ATTACHMENT_POLL_INTERVAL_MS: u32 = 100;

/// A simulated node's attachment state.
///
/// Attaching binds nothing and reaches no network: it reports the fully-ready state directly, so
/// readiness polling in existing fixtures returns on its first check.
pub(crate) struct MockAttachment {
    pub state: AttachmentState,
    pub public_internet_ready: bool,
    pub local_network_ready: bool,
    pub attached_ts: Option<Timestamp>,
    pub started_ts: Timestamp,
}

impl Default for MockAttachment {
    fn default() -> Self {
        Self {
            state: AttachmentState::Detached,
            public_internet_ready: false,
            local_network_ready: false,
            attached_ts: None,
            started_ts: Timestamp::now(),
        }
    }
}

impl VeilidAPI {
    /// Whether this node can reach the shared DHT. Detached, it is offline in every sense the real
    /// `dht_is_online()` gate means.
    pub(crate) fn mock_is_attached(&self) -> VeilidAPIResult<bool> {
        MockNetwork::with_node(&self.mock_key()?, |node| {
            node.attachment.public_internet_ready
        })
    }

    /// Refuse an operation that has to reach the network while detached.
    ///
    /// Every transaction command is a network round trip, so each one takes this gate where the
    /// real implementation takes `dht_is_online()`.
    pub(crate) fn mock_require_dht_online(&self) -> VeilidAPIResult<()> {
        if !self.mock_is_attached()? {
            apibail_try_again!("dht is not online");
        }
        Ok(())
    }

    /// Wait until this node's attachment satisfies `condition`, returning the state that did.
    pub(crate) async fn mock_wait_for_attachment_condition(
        &self,
        condition: VeilidStateAttachmentCondition,
        timeout: Option<Duration>,
    ) -> VeilidAPIResult<VeilidStateAttachment> {
        let deadline = timeout.map(|t| {
            Timestamp::now().later(TimestampDuration::new(
                u64::try_from(t.as_micros()).unwrap_or(u64::MAX),
            ))
        });
        loop {
            let state =
                MockNetwork::with_node(&self.mock_key()?, |node| node.attachment.to_state())?;
            if condition.is_satisfied_by(&state) {
                return Ok(state);
            }
            if deadline.is_some_and(|d| Timestamp::now() >= d) {
                apibail_timeout!();
            }
            sleep(ATTACHMENT_POLL_INTERVAL_MS).await;
        }
    }
}

impl MockAttachment {
    pub fn attach(&mut self) -> bool {
        if self.state.is_attached() {
            return false;
        }
        self.state = AttachmentState::AttachedFull;
        self.public_internet_ready = true;
        self.local_network_ready = true;
        self.attached_ts = Some(Timestamp::now());
        true
    }

    pub fn detach(&mut self) -> bool {
        if matches!(self.state, AttachmentState::Detached) {
            return false;
        }
        self.state = AttachmentState::Detached;
        self.public_internet_ready = false;
        self.local_network_ready = false;
        self.attached_ts = None;
        true
    }

    /// Peer counts report a single simulated peer while attached. The mock has no routing table,
    /// but zero live peers would read as "still attaching"
    pub fn to_state(&self) -> VeilidStateAttachment {
        let now = Timestamp::now();
        let attached = self.state.is_attached();
        let peers = NodeCount::new(if attached { 1 } else { 0 });
        VeilidStateAttachment {
            state: self.state,
            public_internet_ready: self.public_internet_ready,
            local_network_ready: self.local_network_ready,
            uptime: now.duration_since(self.started_ts),
            attached_uptime: self.attached_ts.map(|ts| now.duration_since(ts)),
            reliable_peer_count: peers,
            live_peer_count: peers,
            estimated_network_size: peers,
            median_latency: None,
            over_attached_nodes: NodeCount::new(0),
        }
    }
}
