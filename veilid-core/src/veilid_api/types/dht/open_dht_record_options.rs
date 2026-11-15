use super::*;

/// Options that override defaults for open_dht_record
#[apply(api_data_struct!)]
#[api(default)]
pub struct OpenDHTRecordOptions {
    /// Defaults to true. If false, opening a record this node already holds does not queue a
    /// check that the network still carries it.
    pub rehydrate: Option<bool>,
}
