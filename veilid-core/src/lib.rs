//! # The Veilid Framework
//!
//! This is the core library used to create a Veilid node and operate it as part of an application.
//!
//! `veilid-core` contains all of the core logic for Veilid and can be used in mobile applications as well as desktop
//! and in-browser WebAssembly apps.
//!
//! ## Getting started
//!
//! - [Developer Book](https://veilid.gitlab.io/developer-book/)
//! - [Examples](https://gitlab.com/veilid/veilid/-/tree/main/veilid-core/examples/)
//! - [API Documentation](https://docs.rs/veilid-core)
//!
//! The public API is accessed by getting a [VeilidAPI] object via a call to [api_startup] or [api_startup_json].
//!
//! From there, a [RoutingContext] object can get you access to public and private routed operations.
//!
//! ## Cargo features
//!
//! The default `veilid-core` configurations are:
//!
//! * `default` - Uses `tokio` as the async runtime.
//!
//! If you use `--no-default-features`, you can switch to other runtimes:
//!
//! * `default-async-std` - Uses `async-std` as the async runtime.
//! * `default-wasm` - When building for the `wasm32` architecture, use this to enable `wasm-bindgen-futures` as the async runtime.
//!

#![warn(missing_docs)]
#![deny(warnings)]
// A mock build leaves shared imports unused and async API fns with nothing to await; the default build denies both
#![cfg_attr(
    feature = "mock-api",
    allow(dead_code, unused_imports, clippy::unused_async)
)]
#![recursion_limit = "512"]

cfg_if::cfg_if! {
    if #[cfg(all(target_arch = "wasm32", target_os = "unknown"))] {
        #[cfg(any(feature = "rt-async-std", feature = "rt-tokio"))]
        compile_error!("features \"rt-async-std\" and \"rt-tokio\" can not be specified for WASM");
    } else {
        #[cfg(all(feature = "rt-async-std", feature = "rt-tokio"))]
        compile_error!(
            "feature \"rt-async-std\" and feature \"rt-tokio\" cannot be enabled at the same time"
        );
        #[cfg(not(any(feature = "rt-async-std", feature = "rt-tokio")))]
        compile_error!("exactly one of feature \"rt-async-std\" or feature \"rt-tokio\" must be specified");
    }
}

#[cfg(all(feature = "mock-api", feature = "virtual-network"))]
compile_error!(
    "features \"mock-api\" and \"virtual-network\" are mutually exclusive: mock-api replaces the whole API, leaving no network to virtualize"
);

#[cfg(all(feature = "mock-api", feature = "stats"))]
compile_error!(
    "features \"mock-api\" and \"stats\" are mutually exclusive: no subsystem remains to populate stats, so a mock build would only record garbage."
);

#[macro_use]
extern crate alloc;

mod macros;

mod component;
mod core_context;
mod crypto;
mod logging;
mod stats_accounting;
#[cfg(feature = "stats")]
mod stats_manager;
mod stats_mutex;
mod veilid_api;
mod veilid_config;

// The subsystems mock-api replaces are not compiled, so nothing can bind a socket, open a file or reach a keychain
#[cfg(not(feature = "mock-api"))]
mod attachment_manager;
#[cfg(all(feature = "unstable-blockstore", not(feature = "mock-api")))]
mod block_store;
#[cfg(not(feature = "mock-api"))]
mod network_manager;
#[cfg(not(feature = "mock-api"))]
mod protected_store;
#[cfg(not(feature = "mock-api"))]
mod routing_table;
#[cfg(not(feature = "mock-api"))]
mod rpc_processor;
#[cfg(not(feature = "mock-api"))]
mod storage_manager;
#[cfg(not(feature = "mock-api"))]
mod table_store;

////////////////////////////////////////////////////////////////////////////////////////////
// Public API Exports

pub use self::component::VeilidComponentGuard;
pub use self::core_context::{api_startup, api_startup_json, UpdateCallback};
pub use self::logging::{
    ApiTracingLayer, FmtStripVeilidFields, TryIntoIterVeilidLogDirective, VeilidLayerFilter,
    VeilidLayerFilterConfig, VeilidLayerLogKeyFilter, VeilidLogDirective, VeilidLogKey,
    VeilidLogKeyFilterMode, VEILID_LOG_KEY_FIELD,
};
#[cfg(not(all(target_arch = "wasm32", target_os = "unknown")))]
pub use self::logging::{LogOutput, LogOutputKind, VeilidTracing};
#[cfg(feature = "stats")]
pub use self::stats_manager::{StatsManager, StatsOutput, StatsSettings};
pub use self::veilid_api::*;
pub use self::veilid_config::*;
/// Re-exported from `async-trait` crate
pub use async_trait;
/// Re-exported from `tracing` crate
pub use tracing;
/// Re-exported from `tracing-subscriber` crate
pub use tracing_subscriber;
#[cfg(target_os = "android")]
pub use veilid_api::android::veilid_core_setup_android;
/// Re-exported from `veilid-tools` crate
pub use veilid_tools as tools;
/// Re-exported from `bytes` crate
pub use veilid_tools::bytes;

#[cfg(any(test, feature = "test-util"))]
#[doc(hidden)]
#[allow(
    clippy::disallowed_macros,
    clippy::disallowed_methods,
    reason = "test support: printing and raw clocks are the thing under test"
)]
pub mod tests;

cfg_if::cfg_if! {
    if #[cfg(all(target_arch = "wasm32", target_os = "unknown"))] {
        pub use wasm_bindgen::prelude::*;
        pub use tsify::*;
    }
}

/// Return the cargo package version of veilid-core in string format.
#[must_use]
pub fn veilid_version_string() -> String {
    env!("CARGO_PKG_VERSION").to_owned()
}

/// Return the cargo package version of veilid-core in tuple format.
#[must_use]
pub fn veilid_version() -> (u32, u32, u32) {
    (
        u32::from_str(env!("CARGO_PKG_VERSION_MAJOR")).unwrap_or_log(),
        u32::from_str(env!("CARGO_PKG_VERSION_MINOR")).unwrap_or_log(),
        u32::from_str(env!("CARGO_PKG_VERSION_PATCH")).unwrap_or_log(),
    )
}

#[cfg(not(build_docs))]
include!(env!("BOSION_PATH"));

/// Return the features that were enabled when veilid-core was built.
#[must_use]
pub fn veilid_features() -> Vec<String> {
    cfg_if! {
        if #[cfg(build_docs)] {
            vec!["default".to_string()]
        } else {
            let features = Bosion::CRATE_FEATURES.to_vec();
            features.into_iter().map(String::from).collect()
        }
    }
}

////////////////////////////////////////////////////////////////////////////////////////////
// Crate-only Exports

mod veilid_capnp {
    #![allow(
        clippy::all,
        clippy::must_use_candidate,
        clippy::large_futures,
        clippy::large_stack_arrays,
        clippy::large_stack_frames,
        clippy::large_types_passed_by_value,
        clippy::unused_async,
        clippy::ptr_cast_constness
    )]
    // capnp emits `pub` types in this crate-only module, and its trait impls make them reachable
    #![allow(unnameable_types)]
    // The schema declares limits whose only readers are feature-gated coders
    #![cfg_attr(not(feature = "unstable-blockstore"), allow(dead_code))]
    include!("../proto/veilid_capnp.rs");
}

#[cfg(all(not(feature = "mock-api"), feature = "app-lifecycle"))]
use self::attachment_manager::AppLifecycleEvent;
#[cfg(not(feature = "mock-api"))]
use self::attachment_manager::{ProcessStallEvent, TickEvent};
use self::component::*;
use self::core_context::RegisteredComponents;
#[allow(unused_imports)]
use self::logging::{
    debug_duration, impl_veilid_log_facility, log_veilid_api_error, network_result_value_or_log,
    record_duration, record_duration_fut, veilid_log, veilid_log_dbg, veilid_log_err,
    veilid_log_event, veilid_log_throttled_event, DurationRecorder, MeasureDebugFuture,
    MeasureFuture, VeilidComponentLogFacilities, VeilidComponentLogFacility, DEBUGWARN,
    PROTOCOL_LOG_THROTTLE_MS,
};
use self::stats_accounting::*;
use self::stats_mutex::*;

use core::future::IntoFuture as _;

use arc_swap::{ArcSwap, ArcSwapOption};
use async_trait::*;
use cfg_if::*;
use enumset::*;
use eyre::{bail, eyre, Report as EyreReport, Result as EyreResult, WrapErr};
use futures_util::stream::{FuturesOrdered, FuturesUnordered, StreamExt as _};
use indent::*;
use macro_rules_attribute::apply;
use macros::*;
#[cfg(feature = "schemars")]
use schemars::JsonSchema;
use serde::*;
use stop_token::future::FutureExt as _;
use thiserror::Error as ThisError;
use tracing::*;
use veilid_tools::bytes::*;
use veilid_tools::parking_lot::*;
use veilid_tools::*;

/////////////////////////////////////////////////////////////////////////
