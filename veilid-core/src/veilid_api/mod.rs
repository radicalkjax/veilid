/// Android-specific JNI setup for registering the host app's JavaVM and Context.
#[cfg(target_os = "android")]
pub mod android;
mod api;
// The real debug commands reach into the subsystems mock-api removes; the mock serves its own few
#[cfg(all(feature = "debug-api", not(feature = "mock-api")))]
mod debug;
#[cfg(all(not(feature = "debug-api"), not(feature = "mock-api")))]
mod debug_stub;
mod dht_transaction;
#[cfg(feature = "mock-api")]
mod mock;
mod routing_context;
mod serialize_helpers;
mod types;

#[cfg(any(test, feature = "test-util"))]
#[doc(hidden)]
#[allow(
    clippy::disallowed_macros,
    clippy::disallowed_methods,
    reason = "test support: printing and raw clocks are the thing under test"
)]
pub mod tests_veilid_api;

pub use api::*;
#[cfg(feature = "unstable-blockstore")]
pub use block_store::*;
pub use crypto::*;
pub use dht_transaction::*;
pub use routing_context::*;
pub use serialize_helpers::*;
pub use types::*;

// The mock supplies the store types, so public API consumers compile unchanged against either build
#[cfg(not(feature = "mock-api"))]
pub(crate) use protected_store::*;
#[cfg(not(feature = "mock-api"))]
pub(crate) use table_store::TableStore;
#[cfg(not(feature = "mock-api"))]
pub use table_store::{TableDB, TableDBTransaction};

#[cfg(feature = "mock-api")]
pub(crate) use mock::{
    check_node_id, check_route_id, generate_node_ids, mock_drop_transaction, MockNetwork,
    MockNodeKey, OutboundTransactionHandle,
};
#[cfg(feature = "mock-api")]
pub use mock::{TableDB, TableDBTransaction};

use crate::*;

use core_context::{api_shutdown, VeilidCoreContext};
#[cfg(not(feature = "mock-api"))]
use routing_table::{AllocateRouteParams, DirectionSet, RouteIdAndKeys, RouteSpecStore};
#[cfg(all(feature = "debug-api", not(feature = "mock-api")))]
use rpc_processor::*;

/////////////////////////////////////////////////////////////////////////////////////////////////////
