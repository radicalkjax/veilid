use super::*;

#[derive(
    Copy, Clone, Debug, PartialEq, PartialOrd, Ord, Eq, Hash, Default, Serialize, Deserialize,
)]
pub(crate) struct SenderInfo {
    pub socket_address: SocketAddress,
}
