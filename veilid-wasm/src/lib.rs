//! Veilid WASM bindings (TypeScript / JavaScript)
//!
//! Exposes the veilid-core API through `wasm_bindgen` for use from web and Node.js.
#![cfg(all(target_arch = "wasm32", target_os = "unknown"))]
#![no_std]
#![recursion_limit = "256"]
#![warn(missing_docs)]

extern crate alloc;
use alloc::string::String;
use alloc::sync::Arc;
use alloc::*;
use core::cell::RefCell;
use core::fmt::Debug;
use core::sync::atomic::{AtomicBool, Ordering};
use js_sys::*;
use lazy_static::*;
use send_wrapper::*;
use serde::*;
use tracing_subscriber::prelude::*;
use tracing_subscriber::*;
use tsify::*;
use veilid_core::tools::wasm_tracing::*;
use veilid_core::*;
use veilid_core::{tools::*, VeilidAPIError};

/// API singleton storage and accessors.
pub mod veilid_api_js;
/// `veilidClient` bindings: startup, shutdown, state, routing, and node-level operations.
pub mod veilid_client_js;
/// `VeilidCrypto` bindings: per-cryptosystem hashing, signing, and encryption.
pub mod veilid_crypto_js;
/// `VeilidRoutingContext` bindings: messaging and DHT record operations.
pub mod veilid_routing_context_js;
/// `VeilidTableDB` bindings: persistent key-value table storage.
pub mod veilid_table_db_js;
/// veilid-core version and build feature accessors.
pub mod veilid_version;
/// WASM platform and logging configuration types.
pub mod veilid_wasm_config;
/// wasm-bindgen array conversion helpers used by the bindings.
mod wasm_helpers;
pub use wasm_helpers::*;

pub use veilid_api_js::*;
pub use veilid_version::*;
pub use veilid_wasm_config::*;
