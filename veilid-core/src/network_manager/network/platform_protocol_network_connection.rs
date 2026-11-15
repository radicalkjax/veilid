use super::*;

pub type ProtocolNetworkConnection = Box<dyn PlatformProtocolNetworkConnection>;

/// A single low-level network connection over one protocol
pub trait PlatformProtocolNetworkConnection: fmt::Debug + Send + Sync {
    /// The flow this connection carries
    fn flow(&self) -> Flow;
    /// `In` if we accepted this connection, `Out` if we connected it
    fn direction(&self) -> Direction;
    /// Close the connection
    fn close(&self) -> PinBoxFuture<'_, std::io::Result<NetworkResult<()>>>;
    /// Send one message frame in TCP segments of at most `mtu` bytes
    fn send(
        &self,
        message: Bytes,
        mtu: u16,
    ) -> PinBoxFuture<'_, std::io::Result<NetworkResult<()>>>;
    /// Receive one message frame
    fn recv(&self) -> PinBoxFuture<'_, std::io::Result<NetworkResult<Bytes>>>;
}
