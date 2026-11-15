use super::*;

/// The peers a fanout question gets by default, the most any node sends; the schema's default
/// for the field, so a question written without one asks for the same
const FANOUT_CONFIG_DEFAULT_MAX_PEERS: u32 = veilid_capnp::DEFAULT_FANOUT_CONFIG_MAX_PEERS;

/// What a fanout question asks of the peer list in its answer. Absent on the wire it reads as the
/// default, so old questions ask for what they always got.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FanoutConfig {
    /// Most peers wanted, closest to the key first; the answering node caps it at its own limit
    pub max_peers: u32,
}

impl Default for FanoutConfig {
    fn default() -> Self {
        Self {
            max_peers: FANOUT_CONFIG_DEFAULT_MAX_PEERS,
        }
    }
}

impl FanoutConfig {
    /// A question whose asker already knows the nodes to ask
    pub const NO_PEERS: Self = Self { max_peers: 0 };

    /// How many of an answer's peers to send: what was asked, never more than `limit`
    pub fn peers_limit(&self, limit: usize) -> usize {
        usize::try_from(self.max_peers)
            .unwrap_or(usize::MAX)
            .min(limit)
    }

    pub fn decode(reader: &veilid_capnp::fanout_config::Reader) -> Self {
        Self {
            max_peers: reader.get_max_peers(),
        }
    }

    pub fn encode(&self, builder: &mut veilid_capnp::fanout_config::Builder) {
        builder.set_max_peers(self.max_peers);
    }
}
