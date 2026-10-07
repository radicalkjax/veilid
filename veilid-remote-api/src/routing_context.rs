use super::*;

/// A request to invoke an operation on a remote node's routing context.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct RoutingContextRequest {
    /// Id of the routing context to operate on.
    pub rc_id: u32,
    /// The operation to perform and its arguments.
    #[serde(flatten)]
    pub rc_op: RoutingContextRequestOp,
}

/// A response to a [RoutingContextRequest].
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct RoutingContextResponse {
    /// Id of the routing context the operation ran on.
    pub rc_id: u32,
    /// The operation that was performed and its result.
    #[serde(flatten)]
    pub rc_op: RoutingContextResponseOp,
}

/// A routing context operation to perform on a remote node.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "rc_op")]
pub enum RoutingContextRequestOp {
    /// Release the routing context, freeing its id.
    Release,
    /// Turn on sender privacy with default hop count, stability, and sequencing.
    WithDefaultSafety,
    /// Use a custom safety selection, returning a new routing context.
    WithSafety {
        /// Safety routing requirements to apply.
        safety_selection: SafetySelection,
    },
    /// Use a specified sequencing preference, returning a new routing context.
    WithSequencing {
        /// Ordered vs unordered message delivery preference.
        sequencing: Sequencing,
    },
    /// Get the safety selection in use on this routing context.
    Safety,
    /// App-level bidirectional call that expects a response to be returned.
    AppCall {
        /// Destination node id or private route.
        target: Target,
        /// Message blob of up to 32768 bytes.
        #[serde(with = "as_human_base64")]
        #[schemars(with = "String")]
        message: Vec<u8>,
    },
    /// App-level unidirectional message that does not expect a response.
    AppMessage {
        /// Destination node id or private route.
        target: Target,
        /// Message blob of up to 32768 bytes.
        #[serde(with = "as_human_base64")]
        #[schemars(with = "String")]
        message: Vec<u8>,
    },
    /// Create a new DHT record, leaving it open.
    CreateDhtRecord {
        /// Cryptosystem kind to use.
        #[schemars(with = "String")]
        kind: CryptoKind,
        /// Schema to use when creating the record.
        schema: DHTSchema,
        /// Owner keypair to use, or a random one if None.
        #[schemars(with = "Option<String>")]
        owner: Option<KeyPair>,
    },
    /// Open a DHT record at a specific key.
    OpenDhtRecord {
        /// Record key to open.
        #[schemars(with = "String")]
        key: RecordKey,
        /// Default writer keypair to associate for writer capability.
        #[schemars(with = "Option<String>")]
        writer: Option<KeyPair>,
    },
    /// Close an opened DHT record, allowing it to be re-opened.
    CloseDhtRecord {
        /// Record key to close.
        #[schemars(with = "String")]
        key: RecordKey,
    },
    /// Delete a DHT record from local storage.
    DeleteDhtRecord {
        /// Record key to delete.
        #[schemars(with = "String")]
        key: RecordKey,
    },
    /// Get the latest value of a subkey.
    GetDhtValue {
        /// Record key to read.
        #[schemars(with = "String")]
        key: RecordKey,
        /// Subkey to read.
        subkey: ValueSubkey,
        /// Force a network data refresh rather than using the local copy.
        force_refresh: bool,
    },
    /// Push a changed subkey value to the network.
    SetDhtValue {
        /// Record key to write.
        #[schemars(with = "String")]
        key: RecordKey,
        /// Subkey to write.
        subkey: ValueSubkey,
        /// Value data to set.
        #[serde(with = "as_human_base64")]
        #[schemars(with = "String")]
        data: Vec<u8>,
        /// Optional writer override and other set options.
        options: Option<SetDHTValueOptions>,
    },
    /// Add or update a watch on a DHT record's subkeys.
    WatchDhtValues {
        /// Record key to watch.
        #[schemars(with = "String")]
        key: RecordKey,
        /// Subkey range to watch, or the entire range if None.
        subkeys: Option<ValueSubkeyRangeSet>,
        /// Desired expiration timestamp, or no expiration if None.
        expiration: Option<Timestamp>,
        /// Maximum number of change reports, or u32::MAX if None.
        count: Option<u32>,
    },
    /// Cancel a watch on a range of subkeys.
    CancelDhtWatch {
        /// Record key whose watch to modify.
        #[schemars(with = "String")]
        key: RecordKey,
        /// Subkey range to stop watching, or the entire range if None.
        subkeys: Option<ValueSubkeyRangeSet>,
    },
    /// Inspect a DHT record for subkey state.
    InspectDhtRecord {
        /// Record key to inspect.
        #[schemars(with = "String")]
        key: RecordKey,
        /// Subkey range to inspect, or the entire range if None.
        subkeys: Option<ValueSubkeyRangeSet>,
        /// Kind of range the inspection covers.
        #[schemars(default)]
        scope: DHTReportScope,
    },
    /// Wait for pending offline subkey writes to flush to the network.
    FlushDhtRecord {
        /// Record key to flush.
        #[schemars(with = "String")]
        key: RecordKey,
        /// Timeout in milliseconds, or wait indefinitely if None.
        timeout_ms: Option<u64>,
    },
}

/// The result of a [RoutingContextRequestOp].
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "rc_op")]
pub enum RoutingContextResponseOp {
    /// The requested routing context id did not exist.
    InvalidId,
    /// The routing context was released.
    Release,
    /// Id of the new routing context with default safety applied.
    WithDefaultSafety {
        /// New routing context id, or an error.
        #[serde(flatten)]
        result: ApiResult<u32>,
    },
    /// Id of the new routing context with the custom safety selection applied.
    WithSafety {
        /// New routing context id, or an error.
        #[serde(flatten)]
        result: ApiResult<u32>,
    },
    /// Id of the new routing context with the sequencing preference applied.
    WithSequencing {
        /// New routing context id.
        value: u32,
    },
    /// The safety selection in use on this routing context.
    Safety {
        /// Current safety selection.
        value: SafetySelection,
    },
    /// Answer blob of up to 32768 bytes returned by the call.
    AppCall {
        /// Answer blob, or an error.
        #[serde(flatten)]
        #[schemars(with = "ApiResult<String>")]
        result: ApiResultWithVecU8,
    },
    /// Result of sending an app message.
    AppMessage {
        /// Ok if the message was sent, or an error.
        #[serde(flatten)]
        result: ApiResult<()>,
    },
    /// Descriptor of the newly created DHT record.
    CreateDhtRecord {
        /// Record descriptor, or an error.
        #[serde(flatten)]
        result: ApiResult<Box<DHTRecordDescriptor>>,
    },
    /// Descriptor of the opened DHT record.
    OpenDhtRecord {
        /// Record descriptor, or an error.
        #[serde(flatten)]
        result: ApiResult<Box<DHTRecordDescriptor>>,
    },
    /// Result of closing a DHT record.
    CloseDhtRecord {
        /// Ok if the record was closed, or an error.
        #[serde(flatten)]
        result: ApiResult<()>,
    },
    /// Result of deleting a DHT record.
    DeleteDhtRecord {
        /// Ok if the record was deleted, or an error.
        #[serde(flatten)]
        result: ApiResult<()>,
    },
    /// Latest value of the subkey, or None if it has not been set.
    GetDhtValue {
        /// Subkey value, or an error.
        #[serde(flatten)]
        result: ApiResult<Option<ValueData>>,
    },
    /// None if the value was set, or the newer network value if the set was stale.
    SetDhtValue {
        /// Newer network value if the set was stale, or an error.
        #[serde(flatten)]
        result: ApiResult<Option<ValueData>>,
    },
    /// True if a watch is active for the record, false if fully cancelled.
    WatchDhtValues {
        /// Watch-active flag, or an error.
        #[serde(flatten)]
        result: ApiResult<bool>,
    },
    /// True if a watch is still active, false if fully cancelled.
    CancelDhtWatch {
        /// Watch-active flag, or an error.
        #[serde(flatten)]
        result: ApiResult<bool>,
    },
    /// Subkey ranges and sequence numbers reported by the inspection.
    InspectDhtRecord {
        /// Record report, or an error.
        #[serde(flatten)]
        result: ApiResult<Box<DHTRecordReport>>,
    },
    /// True if all pending writes flushed, false if the timeout elapsed.
    FlushDhtRecord {
        /// Flush-complete flag, or an error.
        #[serde(flatten)]
        result: ApiResult<bool>,
    },
}
