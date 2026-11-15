use super::*;

/// Request to operate on a DHT transaction held by the remote node.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct DhtTransactionRequest {
    /// Id of the DHT transaction the operation applies to.
    pub dhttx_id: u32,
    /// The operation to perform on the transaction.
    #[serde(flatten)]
    pub dhttx_op: DhtTransactionRequestOp,
}

/// Response to a DHT transaction request.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct DhtTransactionResponse {
    /// Id of the DHT transaction the operation applied to.
    pub dhttx_id: u32,
    /// The result of the operation.
    #[serde(flatten)]
    pub dhttx_op: DhtTransactionResponseOp,
}

/// An operation to perform on a DHT transaction.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "dhttx_op")]
pub enum DhtTransactionRequestOp {
    /// Release the transaction handle, rolling back if not yet committed.
    Release,
    /// Commit the transaction, applying all write operations atomically.
    Commit,
    /// Roll back the transaction, performing no write operations.
    Rollback,
    /// Extend the transaction with additional record keys.
    Extend {
        /// Record keys to add to the transaction.
        #[schemars(with = "Vec<String>")]
        record_keys: Vec<RecordKey>,
        /// Options for extending the transaction over the records.
        options: Option<TransactDHTRecordsOptions>,
    },
    /// Get a record subkey value inside the transaction, pulling the latest from the network.
    Get {
        /// Record key to read.
        #[schemars(with = "String")]
        key: RecordKey,
        /// Subkey to read.
        subkey: ValueSubkey,
    },
    /// Set a record subkey value inside the transaction.
    Set {
        /// Record key to write.
        #[schemars(with = "String")]
        key: RecordKey,
        /// Subkey to write.
        subkey: ValueSubkey,
        /// Data to write to the subkey.
        #[serde(with = "as_human_base64")]
        #[schemars(with = "String")]
        data: Vec<u8>,
        /// Options for setting the value, including an overriding writer.
        options: Option<DHTTransactionSetValueOptions>,
    },
    /// Inspect record subkey state inside the transaction, without network activity.
    Inspect {
        /// Record key to inspect.
        #[schemars(with = "String")]
        key: RecordKey,
        /// Subkey range to inspect, or all subkeys if omitted.
        subkeys: Option<ValueSubkeyRangeSet>,
        /// Scope of the subkey state to report.
        #[schemars(default)]
        scope: DHTReportScope,
    },
}

/// The result of a DHT transaction operation.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "dhttx_op")]
pub enum DhtTransactionResponseOp {
    /// The transaction id did not refer to an open transaction.
    InvalidId,
    /// The transaction handle was released.
    Release,
    /// Result of committing the transaction.
    Commit {
        /// Ok if the transaction committed.
        #[serde(flatten)]
        result: ApiResult<()>,
    },
    /// Result of rolling back the transaction.
    Rollback {
        /// Ok if the transaction rolled back.
        #[serde(flatten)]
        result: ApiResult<()>,
    },
    /// Result of extending the transaction with additional record keys.
    Extend {
        /// Ok if the records were added to the transaction.
        #[serde(flatten)]
        result: ApiResult<()>,
    },
    /// Result of a get operation inside the transaction.
    Get {
        /// `Some(data)` if the subkey has data, `None` if unset.
        #[serde(flatten)]
        result: ApiResult<Option<ValueData>>,
    },
    /// Result of a set operation inside the transaction.
    Set {
        /// `None` if the value was set, `Some(data)` if the network value was newer.
        #[serde(flatten)]
        result: ApiResult<Option<ValueData>>,
    },
    /// Result of an inspect operation inside the transaction.
    Inspect {
        /// Report of subkey ranges and sequence numbers overlapping the schema.
        #[serde(flatten)]
        result: ApiResult<Box<DHTRecordReport>>,
    },
}
