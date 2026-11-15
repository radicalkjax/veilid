use super::*;

/// Request addressed to an opened `TableDB` handle.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct TableDbRequest {
    /// Id of the opened `TableDB` handle to operate on.
    pub db_id: u32,
    /// The operation to perform on the handle.
    #[serde(flatten)]
    pub db_op: TableDbRequestOp,
}

/// Response to a `TableDbRequest`.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct TableDbResponse {
    /// Id of the `TableDB` handle the request addressed.
    pub db_id: u32,
    /// The result of the operation.
    #[serde(flatten)]
    pub db_op: TableDbResponseOp,
}

/// Operations on an opened `TableDB` handle.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "db_op")]
pub enum TableDbRequestOp {
    /// Release the handle, dropping its reference to the table.
    Release,
    /// Get the total number of columns in the table.
    GetColumnCount,
    /// Get the list of keys in a column.
    GetKeys {
        /// Column to enumerate.
        col: u32,
    },
    /// Start a write transaction on the table.
    Transact,
    /// Store a value at a key in a column, as a single immediate transaction.
    Store {
        /// Column to write to.
        col: u32,
        /// Key to write.
        #[serde(with = "as_human_base64")]
        #[schemars(with = "String")]
        key: Vec<u8>,
        /// Value to store.
        #[serde(with = "as_human_base64")]
        #[schemars(with = "String")]
        value: Vec<u8>,
    },
    /// Read the value at a key in a column.
    Load {
        /// Column to read from.
        col: u32,
        /// Key to read.
        #[serde(with = "as_human_base64")]
        #[schemars(with = "String")]
        key: Vec<u8>,
    },
    /// Delete a key from a column, returning its prior value if present.
    Delete {
        /// Column to delete from.
        col: u32,
        /// Key to delete.
        #[serde(with = "as_human_base64")]
        #[schemars(with = "String")]
        key: Vec<u8>,
    },
}
/// Results for `TableDbRequestOp` operations.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "db_op")]
pub enum TableDbResponseOp {
    /// The `db_id` did not name an open handle.
    InvalidId,
    /// The handle was released.
    Release,
    /// The total number of columns in the table.
    GetColumnCount {
        /// Column count, or an error.
        #[serde(flatten)]
        result: ApiResult<u32>,
    },
    /// The keys in the requested column.
    GetKeys {
        /// The keys, or an error.
        #[serde(flatten)]
        #[schemars(with = "ApiResult<Vec<String>>")]
        result: ApiResultWithVecVecU8,
    },
    /// A new transaction was started.
    Transact {
        /// Id of the started transaction.
        value: u32,
    },
    /// The store completed.
    Store {
        /// Success, or an error.
        #[serde(flatten)]
        result: ApiResult<()>,
    },
    /// The value read for the key, or `None` if absent.
    Load {
        /// The value, or an error.
        #[serde(flatten)]
        #[schemars(with = "ApiResult<Option<String>>")]
        result: ApiResult<Option<VecU8>>,
    },
    /// The prior value of the deleted key, or `None` if absent.
    Delete {
        /// The prior value, or an error.
        #[serde(flatten)]
        #[schemars(with = "ApiResult<Option<String>>")]
        result: ApiResult<Option<VecU8>>,
    },
}

//////////////////////////////////////////////////////////////////////////////////////////////////////

/// Request addressed to an open `TableDB` write transaction.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct TableDbTransactionRequest {
    /// Id of the transaction to operate on.
    pub tx_id: u32,
    /// The operation to perform on the transaction.
    #[serde(flatten)]
    pub tx_op: TableDbTransactionRequestOp,
}

/// Response to a `TableDbTransactionRequest`.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct TableDbTransactionResponse {
    /// Id of the transaction the request addressed.
    pub tx_id: u32,
    /// The result of the operation.
    #[serde(flatten)]
    pub tx_op: TableDbTransactionResponseOp,
}

/// Operations on an open `TableDB` write transaction.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "tx_op")]
pub enum TableDbTransactionRequestOp {
    /// Commit the transaction's writes atomically and release it.
    Commit,
    /// Discard the transaction's writes and release it.
    Rollback,
    /// Queue a store of a value at a key in a column.
    Store {
        /// Column to write to.
        col: u32,
        /// Key to write.
        #[serde(with = "as_human_base64")]
        #[schemars(with = "String")]
        key: Vec<u8>,
        /// Value to store.
        #[serde(with = "as_human_base64")]
        #[schemars(with = "String")]
        value: Vec<u8>,
    },
    /// Queue a delete of a key in a column.
    Delete {
        /// Column to delete from.
        col: u32,
        /// Key to delete.
        #[serde(with = "as_human_base64")]
        #[schemars(with = "String")]
        key: Vec<u8>,
    },
}
/// Results for `TableDbTransactionRequestOp` operations.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "tx_op")]
pub enum TableDbTransactionResponseOp {
    /// The `tx_id` did not name an open transaction.
    InvalidId,
    /// The commit completed.
    Commit {
        /// Success, or an error.
        #[serde(flatten)]
        result: ApiResult<()>,
    },
    /// The transaction was rolled back.
    Rollback {},
    /// The store was queued.
    Store {
        /// Success, or an error.
        #[serde(flatten)]
        result: ApiResult<()>,
    },
    /// The delete was queued.
    Delete {
        /// Success, or an error.
        #[serde(flatten)]
        result: ApiResult<()>,
    },
}
