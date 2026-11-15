use super::*;

/// Frequency to flush record stores to disk
pub const FLUSH_RECORD_STORES_INTERVAL_SECS: u32 = 1;
/// Frequency to check whether a metadata save is due
pub const SAVE_METADATA_CHECK_INTERVAL_SECS: u32 = 1;
/// Frequency to check for offline subkeys writes to send to the network
pub const OFFLINE_SUBKEY_WRITES_INTERVAL_SECS: u32 = 1;
/// Total number of offline subkey write requests to process in parallel
pub const OFFLINE_SUBKEY_WRITES_BATCH_SIZE: usize = 16;
/// Number of subkeys per offline subkey write keys to process in a chunk
pub const OFFLINE_SUBKEY_WRITES_SUBKEY_CHUNK_SIZE: u64 = 4;
/// Frequency to send ValueChanged notifications to the network
pub const SEND_VALUE_CHANGES_INTERVAL_SECS: u32 = 1;
/// Frequency to check for dead nodes and routes for client-side outbound watches
pub const CHECK_OUTBOUND_WATCHES_INTERVAL_SECS: u32 = 1;
/// Frequency to reconcile watches that are at consensus
pub const RECONCILE_OUTBOUND_WATCHES_INTERVAL: TimestampDuration = TimestampDuration::new_secs(60);
/// First retry of a reconcile that ended short of consensus; later ones back off toward the
/// reconcile interval
pub const RECONCILE_OUTBOUND_WATCHES_RETRY_INTERVAL: TimestampDuration =
    TimestampDuration::new_secs(5);
/// Renew per-node watches this many get-value timeouts before expiry, room for the renewal fanout
pub const RENEW_OUTBOUND_WATCHES_GET_TIMEOUT_MULTIPLIER: u64 = 3;
/// Multiplier applied to rpc_timeout_ms to derive the server-side transaction TTL
pub const TRANSACTION_TIMEOUT_RPC_MULTIPLIER: u64 = 2;
/// Multiplier applied to a node's RTT to derive a post-consensus deadline (capped at `rpc.timeout_ms`)
pub const POST_CONSENSUS_RTT_FACTOR: u64 = 3;
/// Cap on the post-consensus window as a percent of the reply timeout; under congested RTTs the
/// RTT factor alone nears the reply timeout and waits on dead stragglers
pub const POST_CONSENSUS_GRACE_PERCENT: u64 = 10;
/// Background begin's record lock wait before TryAgain, % of its records' slowest reply timeout
pub const BACKGROUND_BEGIN_LOCK_TIMEOUT_PERCENT: u64 = 40;
/// Headroom beyond `consensus_count` (= `set_value_count`) that Begin aims for, as a percentage rounded up
pub const CONSENSUS_TRIM_PERCENT: usize = 50;
/// Frequency to process record rehydration requests
pub const REHYDRATE_RECORDS_INTERVAL_SECS: u32 = 1;
/// Number of rehydration requests to process in parallel
pub const REHYDRATE_BATCH_SIZE: usize = 16;
/// Maximum rehydration requests to start per tick
pub const REHYDRATE_REQUESTS_PER_TICK: usize = 4;
/// Number of subkey sets to push in parallel during transactional rehydration
pub const REHYDRATE_SET_CONCURRENCY: usize = 8;
/// First retry of a failed change inspection; later ones back off toward the fallback ceiling
pub const CHANGE_INSPECT_RETRY_INTERVAL: TimestampDuration = TimestampDuration::new_secs(5);
/// Change inspections in flight at once, as many requests as a minimum-core peer serves together
pub const MAX_CHANGE_INSPECTIONS_IN_FLIGHT: usize =
    network_manager::RELAY_WORKERS_PER_CORE as usize;
/// Length of descriptor cache (512 records and 5 nodes per record, roughly 184320 bytes)
pub const DESCRIPTOR_CACHE_SIZE: usize = 2560;
/// Records worth of remembered rehydration checks
pub const REHYDRATION_CHECKED_CACHE_SIZE: usize = 1024;
/// Consecutive hard rehydration failures on one record before it earns a warning
pub const REHYDRATION_FAILURE_WARN_THRESHOLD: u32 = 3;
/// Table store table for storage manager metadata
pub const STORAGE_MANAGER_METADATA: &str = "storage_manager_metadata";
/// Storage manager metadata key name for offline subkey write persistence
pub const OFFLINE_SUBKEY_WRITES: Bytes = Bytes::from_static(b"offline_subkey_writes");
/// Outbound watch manager metadata key name for watch persistence
pub const OUTBOUND_WATCH_MANAGER: Bytes = Bytes::from_static(b"outbound_watch_manager");
/// Rehydration requests metadata key name for rehydration persistence
pub const REHYDRATION_REQUESTS: Bytes = Bytes::from_static(b"rehydration_requests");
/// Descriptor cache metadata key name for persistence
pub const DESCRIPTOR_CACHE: Bytes = Bytes::from_static(b"descriptor_cache");
/// Remote store connection pool divisor
pub const REMOTE_POOL_CONCURRENCY_DIVISOR: usize = 4;

// --- Outbound Transaction Manager Constants ---

/// Floor of the keepalive interval as a fraction (1/N) of the server-side TTL.
pub const KEEPALIVE_INTERVAL_FLOOR_TTL_FRACTION: u64 = 10;
/// Ceiling of the keepalive interval as a fraction (1/N) of the server-side TTL.
pub const KEEPALIVE_INTERVAL_CEILING_TTL_FRACTION: u64 = 3;
/// Number of RTTs of safety the next reply should land before server expiration.
pub const KEEPALIVE_SAFETY_RTT_FACTOR: u64 = 3;
