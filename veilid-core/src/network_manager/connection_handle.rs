use super::*;

#[derive(Clone, Debug)]
pub struct ConnectionHandle {
    connection_id: NetworkConnectionId,
    flow: Flow,
    channel: flume::Sender<(Option<Id>, Bytes, u16)>,
    dead: Arc<AtomicBool>,
    dialed: bool,
}

#[derive(Debug)]
pub enum ConnectionHandleSendResult {
    Sent,
    NotSent,
}

impl ConnectionHandle {
    pub(super) fn new(
        connection_id: NetworkConnectionId,
        flow: Flow,
        channel: flume::Sender<(Option<Id>, Bytes, u16)>,
        dead: Arc<AtomicBool>,
        dialed: bool,
    ) -> Self {
        Self {
            connection_id,
            flow,
            channel,
            dead,
            dialed,
        }
    }

    #[expect(dead_code)]
    pub fn connection_id(&self) -> NetworkConnectionId {
        self.connection_id
    }

    #[expect(dead_code)]
    pub fn flow(&self) -> Flow {
        self.flow
    }

    pub fn unique_flow(&self) -> UniqueFlow {
        UniqueFlow {
            flow: self.flow,
            connection_id: Some(self.connection_id),
            dialed: self.dialed,
        }
    }

    // #[cfg_attr(feature = "instrument", instrument(level="trace", target="net", skip(self, message), fields(message.len = message.len())))]
    // pub fn send(&self, message: Bytes) -> ConnectionHandleSendResult {
    //     match self.channel.send((Span::current().id(), message)) {
    //         Ok(()) => ConnectionHandleSendResult::Sent,
    //         Err(_) => ConnectionHandleSendResult::NotSent,
    //     }
    // }

    #[cfg_attr(feature = "instrument", instrument(level="trace", target="net", skip(self, message), fields(message.len = message.len())))]
    /// Queues one message, whose TCP segments carry at most `mtu` bytes
    pub async fn send_async(&self, message: Bytes, mtu: u16) -> ConnectionHandleSendResult {
        // A dead connection's processor no longer drains the channel; queueing would silently drop
        if self.dead.load(Ordering::Relaxed) {
            return ConnectionHandleSendResult::NotSent;
        }
        match self
            .channel
            .send_async((Span::current().id(), message, mtu))
            .await
        {
            Ok(()) => ConnectionHandleSendResult::Sent,
            Err(_) => ConnectionHandleSendResult::NotSent,
        }
    }
}

impl PartialEq for ConnectionHandle {
    fn eq(&self, other: &Self) -> bool {
        self.connection_id == other.connection_id && self.flow == other.flow
    }
}

impl Eq for ConnectionHandle {}
