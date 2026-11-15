use super::*;

/// Options that override defaults for DHTTransaction::set
#[apply(api_data_struct!)]
#[api(default)]
pub struct DHTTransactionSetValueOptions {
    /// Override writer key pair for this operation
    #[cfg_attr(feature = "schemars", schemars(with = "Option<String>"))]
    pub writer: Option<KeyPair>,
    /// Raise the sequence number to at least this, after the automatic increment.
    /// [ValueSeqNum::MAX] minus `n` leaves `n` writes after this one.
    ///
    /// If the subkey already meets the minimum, the write reuses that sequence number instead of
    /// failing, so a write at [ValueSeqNum::MAX] can be retried. The caller must send the same data
    /// and writer on every retry; nothing here enforces that.
    pub min_seqnum: Option<ValueSeqNum>,
}
