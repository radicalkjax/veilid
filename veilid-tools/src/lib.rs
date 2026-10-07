//! # veilid-tools
//!
//! A collection of baseline tools for Rust development use by Veilid and Veilid-enabled Rust applications
//!
//! These are used by `veilid-core`, `veilid-server`, `veilid-cli` and may be used by any other applications
//! that link in `veilid-core` if a common baseline of functionality is desired. Extending this crate with new
//! utility functions is encouraged rather than adding 'common' functionality to `veilid-core`, allowing it to
//! remain free of boilerplate and utility classes that could be reused elsewhere.
//!
//! Everything added to this crate must be extensively unit-tested (work in progress).
//!
//! ## Cargo features
//!
//! The default `veilid-tools` configurations are:
//!
//! * `default` - Uses `tokio` as the async runtime
//!
//! If you use `--no-default-features`, you can switch to other runtimes:
//!
//! * `rt-async-std` - Uses `async-std` as the async runtime
//! * `rt-wasm-bindgen` - When building for the `wasm32` architecture, use this to enable `wasm-bindgen-futures` as the async runtime
//!

#![warn(missing_docs)]
#![deny(warnings)]

/// Packet fragmentation and reassembly for the sender and receiver ends.
pub mod assembly_buffer;
/// Debuggable async lock primitives (mutex, rwlock).
pub mod async_locks;
/// Stream adapter that allows peeking buffered bytes without consuming them.
pub mod async_peek_stream;
/// `io::Write` adapter that appends into a `BytesMut` buffer.
pub mod bytes_writer;
/// Broadcast stream that hands each subscriber its own clone of every item.
pub mod clone_stream;
/// Dedicated FIFO thread pool for heavy CPU work.
#[cfg(not(all(target_arch = "wasm32", target_os = "unknown")))]
pub mod cpu_pool;
/// parking_lot deadlock-detector watchdog (enabled by the `debug-locks` feature).
#[cfg(feature = "debug-locks")]
pub mod deadlock_detector;
/// Debugger attachment detection and wait-for-attach.
#[cfg(not(all(target_arch = "wasm32", target_os = "unknown")))]
pub mod debugging;
/// Runs futures to completion in the background and routes their results to a stream.
pub mod deferred_stream_processor;
/// Subscribe/publish event bus for type-keyed events.
pub mod event_bus;
/// One-shot resolvable signal with futures that wait on resolution.
pub mod eventual;
/// Shared base machinery behind the `Eventual` family.
pub mod eventual_base;
/// `Eventual` carrying a single by-value payload to one waiter.
pub mod eventual_value;
/// `Eventual` carrying a payload cloned to every waiter.
pub mod eventual_value_clone;
/// Hysteresis detector that debounces a value flapping between states.
pub mod flap_detector;
/// String indentation, human-readable byte counts, and formatter helpers.
pub mod formatting;
/// Batched, concurrency-limited processing of future queues and streams.
pub mod future_queue;
/// Pool that races spawned futures and optionally reports their completion.
pub mod future_race_completion;
/// UPnP gateway discovery and port mapping.
#[cfg(not(all(target_arch = "wasm32", target_os = "unknown")))]
pub mod igd;
/// Periodic interval timer driving an async callback.
pub mod interval;
/// IP address paired with a port.
pub mod ip_addr_port;
/// IP address classification helpers (global, loopback, multicast scope).
pub mod ip_extra;
/// Inter-process communication over named pipes (Windows) or unix sockets.
pub mod ipc;
/// `unwrap`/`expect` variants that log through `tracing` or `log` on failure.
pub mod log_unwrap;
/// Join handle that aborts on drop if not already joined.
pub mod must_join_handle;
/// Single-slot future runner that joins the prior run before starting the next.
pub mod must_join_single_future;
/// Enumeration of host network interfaces and their addresses.
pub mod network_interfaces;
/// Result type distinguishing network-level outcomes from hard errors.
pub mod network_result;
/// Type aliases for pinned, boxed futures.
pub mod pin;
/// Cryptographically secure RNG and random number helpers.
pub mod random;
/// Raw integer timestamps with duration parsing and human formatting.
pub mod raw_timestamp;
/// DNS resolution via the system resolver or hickory-resolver.
pub mod resolver;
/// Single-fire eventual whose resolution returns the prior pending value.
pub mod single_shot_eventual;
/// Async sleep for a number of milliseconds.
pub mod sleep;
/// UDP/TCP socket binding and connection helpers with reuse and linger options.
#[cfg(not(all(target_arch = "wasm32", target_os = "unknown")))]
pub mod socket_tools;
/// Spawn blocking work and yield control on the async runtime.
pub mod spawn;
/// URL parser splitting scheme, host, port, and path into typed parts.
pub mod split_url;
/// Lock gating startup and shutdown against in-flight operations.
pub mod startup_lock;
/// Interning of borrowed strings into `&'static str`.
pub mod static_string_table;
/// Drop-triggered cancellation tokens and deadline-bounded future combinators.
pub mod stop_token;
/// Total system memory query.
#[cfg(not(all(target_arch = "wasm32", target_os = "unknown")))]
pub mod system_info;
/// Per-tag lock table serializing access by key.
pub mod tag_lock;
/// Self-rescheduling periodic task with single-instance run guarantees.
pub mod tick_task;
/// Run a future with a millisecond timeout.
pub mod timeout;
/// Timeout combinator returning either the value, a timeout, or an error.
pub mod timeout_or;
/// Assorted small utilities (time conversion, retry, comparison helpers).
pub mod tools;
/// `parking_lot::Mutex` wrapper that records the holder for debugging.
pub mod tracked_mutex;
/// Backtrace capture that trims runtime frames and shortens paths.
#[cfg(feature = "backtrace")]
pub mod trim_backtrace;
/// In-process simulated network for deterministic testing.
#[cfg(feature = "virtual-network")]
pub mod virtual_network;
/// Browser/WASM environment and address-family detection.
#[cfg(all(target_arch = "wasm32", target_os = "unknown"))]
pub mod wasm_tools;
/// Tracing layer reporting to the browser console and performance timeline.
#[cfg(all(target_arch = "wasm32", target_os = "unknown", feature = "tracing"))]
pub mod wasm_tracing;

///////////////////////////////////////////////////////////////
// Veilid-tools Prelude

#[doc(no_inline)]
pub use std::borrow::{Cow, ToOwned};
#[doc(no_inline)]
pub use std::boxed::Box;
#[doc(no_inline)]
pub use std::cell::RefCell;
#[doc(no_inline)]
pub use std::cmp;
#[doc(no_inline)]
pub use std::collections::btree_map::BTreeMap;
#[doc(no_inline)]
pub use std::collections::btree_set::BTreeSet;
#[doc(no_inline)]
pub use std::collections::hash_map::HashMap;
#[doc(no_inline)]
pub use std::collections::hash_set::HashSet;
#[doc(no_inline)]
pub use std::collections::LinkedList;
#[doc(no_inline)]
pub use std::collections::VecDeque;
#[doc(no_inline)]
pub use std::convert::{TryFrom, TryInto};
#[doc(no_inline)]
pub use std::fmt;
#[doc(no_inline)]
pub use std::future::Future;
#[doc(no_inline)]
pub use std::mem;
#[doc(no_inline)]
pub use std::net::{
    IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr, SocketAddrV4, SocketAddrV6, ToSocketAddrs,
};
#[doc(no_inline)]
pub use std::ops::{Fn, FnMut, FnOnce};
#[doc(no_inline)]
pub use std::pin::Pin;
#[doc(no_inline)]
pub use std::rc::Rc;
#[doc(no_inline)]
pub use std::str::FromStr;
#[doc(no_inline)]
pub use std::string::{String, ToString};
#[doc(no_inline)]
pub use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
#[doc(no_inline)]
pub use std::sync::{Arc, LazyLock, Weak};
#[doc(no_inline)]
pub use std::task;
#[doc(no_inline)]
pub use std::time::Duration;
#[doc(no_inline)]
pub use std::vec::Vec;

cfg_if! {
    if #[cfg(all(target_arch = "wasm32", target_os = "unknown"))] {

        #[doc(no_inline)]
        pub use async_executors::JoinHandle as LowLevelJoinHandle;

    } else {
        cfg_if! {
            if #[cfg(feature="rt-async-std")] {
                #[doc(no_inline)]
                pub use async_std::task::JoinHandle as LowLevelJoinHandle;

            } else if #[cfg(feature="rt-tokio")] {

                #[doc(no_inline)]
                pub use tokio::task::JoinHandle as LowLevelJoinHandle;
            } else {
                compile_error!("needs executor implementation");
            }
        }
    }
}

#[doc(inline)]
pub use assembly_buffer::*;
#[doc(inline)]
pub use async_locks::*;
#[doc(inline)]
pub use async_peek_stream::*;
#[doc(inline)]
pub use bytes_writer::*;
#[doc(inline)]
pub use clone_stream::*;
#[doc(inline)]
#[cfg(not(all(target_arch = "wasm32", target_os = "unknown")))]
pub use cpu_pool::*;
#[cfg(feature = "debug-locks")]
#[doc(inline)]
pub use deadlock_detector::*;
#[doc(inline)]
#[cfg(not(all(target_arch = "wasm32", target_os = "unknown")))]
pub use debugging::*;
#[doc(inline)]
pub use deferred_stream_processor::*;
#[doc(inline)]
pub use event_bus::*;
#[doc(inline)]
pub use eventual::*;
#[doc(inline)]
pub use eventual_base::{EventualCommon, EventualResolvedFuture};
#[doc(inline)]
pub use eventual_value::*;
#[doc(inline)]
pub use eventual_value_clone::*;
#[doc(inline)]
pub use flap_detector::*;
#[doc(inline)]
pub use formatting::*;
#[doc(inline)]
pub use future_queue::*;
#[doc(inline)]
pub use future_race_completion::*;
#[doc(inline)]
pub use interval::*;
#[doc(inline)]
pub use ip_addr_port::*;
#[doc(inline)]
pub use ip_extra::*;
#[doc(inline)]
pub use ipc::*;
#[doc(inline)]
pub use log_unwrap::*;
#[doc(inline)]
pub use must_join_handle::*;
#[doc(inline)]
pub use must_join_single_future::*;
#[doc(inline)]
pub use network_interfaces::*;
#[doc(inline)]
pub use network_result::*;
#[doc(inline)]
pub use pin::*;
#[doc(inline)]
pub use random::*;
#[doc(inline)]
pub use raw_timestamp::*;
#[doc(inline)]
pub use resolver::*;
#[doc(inline)]
pub use single_shot_eventual::*;
#[doc(inline)]
pub use sleep::*;
#[doc(inline)]
#[cfg(not(all(target_arch = "wasm32", target_os = "unknown")))]
pub use socket_tools::*;
#[doc(inline)]
pub use spawn::*;
#[doc(inline)]
pub use split_url::*;
#[doc(inline)]
pub use startup_lock::*;
#[doc(inline)]
pub use static_string_table::*;
#[doc(inline)]
pub use stop_token::{StopSource, StopToken, TimedOutError};
#[doc(inline)]
#[cfg(not(all(target_arch = "wasm32", target_os = "unknown")))]
pub use system_info::*;
#[doc(inline)]
pub use tag_lock::*;
#[doc(inline)]
pub use tick_task::*;
#[doc(inline)]
pub use timeout::*;
#[doc(inline)]
pub use timeout_or::*;
#[doc(inline)]
pub use tools::*;
#[doc(inline)]
pub use tracked_mutex::*;
#[doc(inline)]
#[cfg(feature = "backtrace")]
pub use trim_backtrace::*;
#[cfg(all(target_arch = "wasm32", target_os = "unknown"))]
pub use wasm_tools::*;

#[cfg(any(test, feature = "test-util"))]
#[doc(hidden)]
pub mod tests;

cfg_if! {
    if #[cfg(feature = "tracing")] {
        use tracing::*;

        /// True if DEBUG-level logging is enabled for the given target.
        #[macro_export]
        macro_rules! debug_target_enabled {
            ($target:expr) => { enabled!(target: $target, Level::DEBUG) }
        }
    } else {
        use log::*;
        /// True if DEBUG-level logging is enabled for the given target.
        #[macro_export]
        macro_rules! debug_target_enabled {
            ($target:expr) => { log_enabled!(target: $target, Level::Debug) }
        }
    }
}
use bytes::*;
use cfg_if::*;
use futures_util::{AsyncRead, AsyncWrite};
use parking_lot::*;
use thiserror::Error as ThisError;

// Re-exports for other crates
pub use bytes;
pub use fn_name;
pub use hex;
pub use parking_lot;
pub use shell_words;
