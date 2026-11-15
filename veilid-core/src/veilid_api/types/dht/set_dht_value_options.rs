use super::*;

/// Options that override defaults for set_dht_value
#[apply(api_data_struct!)]
pub struct SetDHTValueOptions {
    /// Override writer key pair for this operation
    #[cfg_attr(feature = "schemars", schemars(with = "Option<String>"))]
    pub writer: Option<KeyPair>,
    /// Defaults to true. If false, the value will not be written if the node is offline,
    /// and a TryAgain error will be returned.
    pub allow_offline: Option<AllowOffline>,
    /// Raise the sequence number to at least this, after the automatic increment.
    /// [ValueSeqNum::MAX] minus `n` leaves `n` writes after this one.
    ///
    /// If the subkey already meets the minimum, the write reuses that sequence number instead of
    /// failing, so a write at [ValueSeqNum::MAX] can be retried until it reaches consensus. The
    /// caller must send the same data and writer on every retry; nothing here enforces that.
    pub min_seqnum: Option<ValueSeqNum>,
}

impl Default for SetDHTValueOptions {
    fn default() -> Self {
        Self {
            writer: None,
            allow_offline: Some(AllowOffline(true)),
            min_seqnum: None,
        }
    }
}
