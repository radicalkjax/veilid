//! # veilid-remote-api
//!
//! This crate provides the ability to control a Veilid node remotely.
//!
//! The [JsonRequestProcessor] is a wrapper around the [VeilidAPI] and provides a way to process requests and send responses.

#![warn(missing_docs)]
#![deny(warnings)]
#![recursion_limit = "256"]

// Re-export veilid-core
pub use veilid_core;

use veilid_core::tools::*;
use veilid_core::*;

use parking_lot::Mutex;
use schemars::{schema_for, JsonSchema};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fmt;
use tracing::*;

mod routing_context;
pub use routing_context::*;

mod table_db;
pub use table_db::*;

mod crypto_system;
pub use crypto_system::*;

mod dht_transaction;
pub use dht_transaction::*;

mod process;
pub use process::*;

/// A single remote API request: an operation paired with a correlation id.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct Request {
    /// Operation Id (pairs with Response, or empty if unidirectional).
    #[serde(default)]
    pub id: u32,
    /// The request operation variant.
    #[serde(flatten)]
    pub op: RequestOp,
}

/// A message received from the remote API: either a reply to a request or an unsolicited update.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "type")]
pub enum RecvMessage {
    /// A reply correlated to a prior [Request].
    Response(Response),
    /// An unsolicited node update pushed by the API.
    Update(VeilidUpdate),
}

/// A single remote API response: an operation result paired with a correlation id.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct Response {
    /// Operation Id (pairs with Request, or empty if unidirectional).
    #[serde(default)]
    pub id: u32,
    /// The response operation variant.
    #[serde(flatten)]
    pub op: ResponseOp,
}

/// A remote operation to perform, one variant per [VeilidAPI] method.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "op")]
pub enum RequestOp {
    /// Host-application control command, handled outside veilid-core.
    Control {
        /// Control command arguments.
        args: Vec<String>,
    },
    /// Get a full copy of the current node state.
    GetState,
    /// Check whether the node has shut down.
    IsShutdown,
    /// Connect to the network.
    Attach,
    /// Disconnect from the network.
    Detach,
    /// Create a new `MemberId` for use in building `DHTSchema`s.
    GenerateMemberId {
        /// Writer public key the member id is derived from.
        #[schemars(with = "String")]
        writer_key: PublicKey,
    },
    /// Deterministically build the record key for a schema and owner.
    GetDhtRecordKey {
        /// Schema the record uses.
        schema: DHTSchema,
        /// Owner public key, whose crypto kind selects the record key kind.
        #[schemars(with = "String")]
        owner: PublicKey,
        /// Optional record encryption key.
        #[schemars(with = "Option<String>")]
        encryption_key: Option<SharedSecret>,
    },
    /// Allocate a new private route with default options.
    NewPrivateRoute,
    /// Allocate a new private route with a specified cryptosystem, stability, and sequencing.
    NewCustomPrivateRoute {
        /// Route specification: crypto kinds, hop count, stability, sequencing.
        #[serde(default)]
        private_spec: PrivateSpec,
    },
    /// Import a private route blob as a remote private route.
    ImportRemotePrivateRoute {
        /// Published route blob to import.
        #[serde(with = "as_human_base64")]
        #[schemars(with = "String")]
        blob: Vec<u8>,
    },
    /// Release a locally allocated or remotely imported private route.
    ReleasePrivateRoute {
        /// Route to release.
        #[schemars(with = "String")]
        route_id: RouteId,
    },
    /// Reply to an `AppCall` received over a [VeilidUpdate::AppCall].
    AppCallReply {
        /// Identifies which call to answer.
        #[schemars(with = "String")]
        call_id: OperationId,
        /// Answer blob returned to the caller.
        #[serde(with = "as_human_base64")]
        #[schemars(with = "String")]
        message: Vec<u8>,
    },
    // Routing Context
    /// Allocate a new routing context with default safety, sequencing, and stability.
    NewRoutingContext,
    /// Operation on an existing routing context.
    RoutingContext(RoutingContextRequest),
    // DHT Transaction
    /// Begin a transaction over a set of opened DHT records.
    TransactDhtRecords {
        /// Records to enroll in the transaction.
        #[schemars(with = "Vec<String>")]
        record_keys: Vec<RecordKey>,
        /// Optional transaction options, including a default signing keypair.
        options: Option<TransactDHTRecordsOptions>,
    },
    /// Operation on an existing DHT transaction.
    DhtTransaction(DhtTransactionRequest),
    // TableDb
    /// Open (creating if absent) a named table store database.
    OpenTableDb {
        /// Database name.
        name: String,
        /// Number of columns.
        column_count: u32,
    },
    /// Delete a named table store database.
    DeleteTableDb {
        /// Database name.
        name: String,
    },
    /// Operation on an open table store database.
    TableDb(TableDbRequest),
    /// Operation on a table store database transaction.
    TableDbTransaction(TableDbTransactionRequest),
    // Crypto
    /// Get the cryptosystem for a crypto kind.
    GetCryptoSystem {
        /// Crypto kind to look up.
        #[schemars(with = "String")]
        kind: CryptoKind,
    },
    /// Operation on a cryptosystem.
    CryptoSystem(CryptoSystemRequest),
    /// Verify a set of signatures against node ids over data.
    VerifySignatures {
        /// Node ids whose signatures to check.
        #[schemars(with = "Vec<String>")]
        node_ids: Vec<PublicKey>,
        /// Signed data.
        #[serde(with = "as_human_base64")]
        #[schemars(with = "String")]
        data: Vec<u8>,
        /// Signatures to verify, parallel to `node_ids`.
        #[schemars(with = "Vec<String>")]
        signatures: Vec<Signature>,
    },
    /// Generate signatures over data with a set of keypairs.
    GenerateSignatures {
        /// Data to sign.
        #[serde(with = "as_human_base64")]
        #[schemars(with = "String")]
        data: Vec<u8>,
        /// Keypairs to sign with.
        #[schemars(with = "Vec<String>")]
        key_pairs: Vec<KeyPair>,
    },
    /// Generate a new keypair of a given crypto kind.
    GenerateKeyPair {
        /// Crypto kind for the keypair.
        #[schemars(with = "String")]
        kind: CryptoKind,
    },
    // Misc
    /// Get the current timestamp.
    Now,
    /// Get a non-decreasing timestamp.
    NowNonDecreasing,
    /// Get a strictly increasing timestamp.
    NowIncreasing,
    /// Run a debug command.
    Debug {
        /// Debug command string.
        command: String,
    },
    /// Get the Veilid version as a string.
    VeilidVersionString,
    /// Get the Veilid version as major/minor/patch numbers.
    VeilidVersion,
    /// Get the list of compiled-in feature flags.
    VeilidFeatures,
    /// Get the default node configuration.
    DefaultVeilidConfig,
    /// Get the set of supported crypto kinds.
    ValidCryptoKinds,
}

/// The result of a remote operation, one variant per [RequestOp].
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "op")]
pub enum ResponseOp {
    /// Result of a `Control` command.
    Control {
        /// Control output, or error.
        #[serde(flatten)]
        result: ApiResult<String>,
    },
    /// Result of `GetState`.
    GetState {
        /// Current node state, or error.
        #[serde(flatten)]
        result: ApiResult<Box<VeilidState>>,
    },
    /// Result of `IsShutdown`.
    IsShutdown {
        /// True if the node has shut down.
        value: bool,
    },
    /// Result of `Attach`.
    Attach {
        /// Success, or error.
        #[serde(flatten)]
        result: ApiResult<()>,
    },
    /// Result of `Detach`.
    Detach {
        /// Success, or error.
        #[serde(flatten)]
        result: ApiResult<()>,
    },
    /// Result of `GenerateMemberId`.
    GenerateMemberId {
        /// Generated member id, or error.
        #[serde(flatten)]
        #[schemars(with = "ApiResult<String>")]
        result: ApiResultWithString<MemberId>,
    },
    /// Result of `GetDhtRecordKey`.
    GetDhtRecordKey {
        /// Derived record key, or error.
        #[serde(flatten)]
        #[schemars(with = "ApiResult<String>")]
        result: ApiResultWithString<RecordKey>,
    },
    /// Result of `NewPrivateRoute`.
    NewPrivateRoute {
        /// Route id and publishable blob, or error.
        #[serde(flatten)]
        result: ApiResult<RouteBlob>,
    },
    /// Result of `NewCustomPrivateRoute`.
    NewCustomPrivateRoute {
        /// Route id and publishable blob, or error.
        #[serde(flatten)]
        result: ApiResult<RouteBlob>,
    },
    /// Result of `ImportRemotePrivateRoute`.
    ImportRemotePrivateRoute {
        /// Imported route id, or error.
        #[serde(flatten)]
        #[schemars(with = "ApiResult<String>")]
        result: ApiResultWithString<RouteId>,
    },
    /// Result of `ReleasePrivateRoute`.
    ReleasePrivateRoute {
        /// Success, or error.
        #[serde(flatten)]
        result: ApiResult<()>,
    },
    /// Result of `AppCallReply`.
    AppCallReply {
        /// Success, or error.
        #[serde(flatten)]
        result: ApiResult<()>,
    },
    // Routing Context
    /// Result of `NewRoutingContext`: a handle to the new routing context.
    NewRoutingContext {
        /// Routing context handle, or error.
        #[serde(flatten)]
        result: ApiResult<u32>,
    },
    /// Result of a routing context operation.
    RoutingContext(Box<RoutingContextResponse>),
    // DHT Transaction
    /// Result of `TransactDhtRecords`: a handle to the new transaction.
    TransactDhtRecords {
        /// DHT transaction handle, or error.
        #[serde(flatten)]
        result: ApiResult<u32>,
    },
    /// Result of a DHT transaction operation.
    DhtTransaction(Box<DhtTransactionResponse>),
    // TableDb
    /// Result of `OpenTableDb`: a handle to the open database.
    OpenTableDb {
        /// Table database handle, or error.
        #[serde(flatten)]
        result: ApiResult<u32>,
    },
    /// Result of `DeleteTableDb`.
    DeleteTableDb {
        /// True if a database was deleted, or error.
        #[serde(flatten)]
        result: ApiResult<bool>,
    },
    /// Result of a table database operation.
    TableDb(TableDbResponse),
    /// Result of a table database transaction operation.
    TableDbTransaction(TableDbTransactionResponse),
    // Crypto
    /// Result of `GetCryptoSystem`: a handle to the cryptosystem.
    GetCryptoSystem {
        /// Cryptosystem handle, or error.
        #[serde(flatten)]
        result: ApiResult<u32>,
    },
    /// Result of selecting the best available cryptosystem: a handle to it.
    BestCryptoSystem {
        /// Cryptosystem handle, or error.
        #[serde(flatten)]
        result: ApiResult<u32>,
    },
    /// Result of a cryptosystem operation.
    CryptoSystem(CryptoSystemResponse),
    /// Result of `VerifySignatures`: the verified subset of keys, or `None` if any failed.
    VerifySignatures {
        /// Verified public key group, or error.
        #[serde(flatten)]
        #[schemars(with = "ApiResult<Option<Vec<String>>>")]
        result: ApiResultWithOptVecString<Option<PublicKeyGroup>>,
    },
    /// Result of `GenerateSignatures`.
    GenerateSignatures {
        /// Generated signatures, or error.
        #[serde(flatten)]
        #[schemars(with = "ApiResult<Vec<String>>")]
        result: ApiResultWithVecString<Vec<Signature>>,
    },
    /// Result of `GenerateKeyPair`.
    GenerateKeyPair {
        /// Generated keypair, or error.
        #[serde(flatten)]
        #[schemars(with = "ApiResult<String>")]
        result: ApiResultWithString<KeyPair>,
    },
    // Misc
    /// Result of `Now`.
    Now {
        /// Current timestamp.
        #[schemars(with = "String")]
        value: Timestamp,
    },
    /// Result of `NowNonDecreasing`.
    NowNonDecreasing {
        /// Non-decreasing timestamp.
        #[schemars(with = "String")]
        value: Timestamp,
    },
    /// Result of `NowIncreasing`.
    NowIncreasing {
        /// Strictly increasing timestamp.
        #[schemars(with = "String")]
        value: Timestamp,
    },
    /// Result of `Debug`.
    Debug {
        /// Debug command output, or error.
        #[serde(flatten)]
        result: ApiResult<String>,
    },
    /// Result of `VeilidVersionString`.
    VeilidVersionString {
        /// Version string.
        value: String,
    },
    /// Result of `VeilidVersion`.
    VeilidVersion {
        /// Major version.
        major: u32,
        /// Minor version.
        minor: u32,
        /// Patch version.
        patch: u32,
    },
    /// Result of `DefaultVeilidConfig`.
    DefaultVeilidConfig {
        /// Default configuration as a string.
        value: String,
    },
    /// Result of `VeilidFeatures`.
    VeilidFeatures {
        /// Compiled-in feature flags.
        value: Vec<String>,
    },
    /// Result of `ValidCryptoKinds`.
    ValidCryptoKinds {
        /// Supported crypto kinds.
        #[schemars(with = "Vec<String>")]
        value: Vec<CryptoKind>,
    },
}

/// An operation result: an `Ok` value or an `Err` error.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(untagged)]
pub enum ApiResult<T>
where
    T: Clone + fmt::Debug + JsonSchema,
{
    /// Success value.
    Ok {
        /// The returned value.
        value: T,
    },
    /// Failure.
    Err {
        /// The error.
        error: VeilidAPIError,
    },
}

/// An operation result whose `Ok` value serializes as a string.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(untagged)]
pub enum ApiResultWithString<T>
where
    T: Clone + fmt::Debug,
{
    /// Success value, serialized as a string.
    Ok {
        /// The returned value.
        #[schemars(with = "String")]
        value: T,
    },
    /// Failure.
    Err {
        /// The error.
        error: VeilidAPIError,
    },
}

/// An operation result whose `Ok` value is a byte buffer.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(untagged)]
pub enum ApiResultWithVecU8 {
    /// Success value, serialized as base64.
    Ok {
        /// The returned bytes.
        #[serde(with = "as_human_base64")]
        #[schemars(with = "String")]
        value: Vec<u8>,
    },
    /// Failure.
    Err {
        /// The error.
        error: VeilidAPIError,
    },
}
/// A base64-serialized byte buffer.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(transparent)]
pub struct VecU8 {
    #[serde(with = "as_human_base64")]
    #[schemars(with = "String")]
    value: Vec<u8>,
}

/// An operation result whose `Ok` value is a list of byte buffers.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(untagged)]
pub enum ApiResultWithVecVecU8 {
    /// Success value, a list of base64-serialized buffers.
    Ok {
        /// The returned buffers.
        #[schemars(with = "Vec<String>")]
        value: Vec<VecU8>,
    },
    /// Failure.
    Err {
        /// The error.
        error: VeilidAPIError,
    },
}

/// An operation result whose `Ok` value serializes as a list of strings.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(untagged)]
pub enum ApiResultWithVecString<T>
where
    T: Clone + fmt::Debug,
{
    /// Success value, serialized as a list of strings.
    Ok {
        /// The returned value.
        #[schemars(with = "Vec<String>")]
        value: T,
    },
    /// Failure.
    Err {
        /// The error.
        error: VeilidAPIError,
    },
}

/// An operation result whose `Ok` value serializes as an optional list of strings.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(untagged)]
pub enum ApiResultWithOptVecString<T>
where
    T: Clone + fmt::Debug,
{
    /// Success value, serialized as an optional list of strings.
    Ok {
        /// The returned value.
        #[schemars(with = "Option<Vec<String>>")]
        value: T,
    },
    /// Failure.
    Err {
        /// The error.
        error: VeilidAPIError,
    },
}

/// Insert the JSON schemas for [Request] and [RecvMessage] into `out`, keyed by type name.
pub fn emit_schemas(out: &mut HashMap<String, String>) {
    let schema_request = schema_for!(Request);
    let schema_recv_message = schema_for!(RecvMessage);

    out.insert(
        "Request".to_owned(),
        serde_json::to_string_pretty(&schema_request).unwrap_or_log(),
    );

    out.insert(
        "RecvMessage".to_owned(),
        serde_json::to_string_pretty(&schema_recv_message).unwrap_or_log(),
    );
}
