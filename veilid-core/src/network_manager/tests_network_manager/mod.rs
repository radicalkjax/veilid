mod mocks;
mod test_address_family_confirm;
mod test_bootstrap;
#[cfg(feature = "debug-api")]
mod test_chaos;
mod test_connection_table;
#[cfg(not(all(target_arch = "wasm32", target_os = "unknown")))]
mod test_egress_change;
mod test_envelope_timestamps;
#[cfg(not(all(target_arch = "wasm32", target_os = "unknown")))]
mod test_family_mtu;
#[cfg(not(all(target_arch = "wasm32", target_os = "unknown")))]
mod test_hole_punch_ttl;
#[cfg(not(all(target_arch = "wasm32", target_os = "unknown")))]
mod test_hop_mtu;
mod test_loss_estimator;
#[cfg(not(all(target_arch = "wasm32", target_os = "unknown")))]
mod test_mtu_probe_processor;
mod test_online_detector;
mod test_protected_reconnect;
mod test_transport_stats;

use super::*;
pub use mocks::*;

pub async fn test_all() {
    test_bootstrap::test_bootstrap_v0().await;
    test_bootstrap::test_bootstrap_v1().await;
    test_bootstrap::test_boot_v1_response().await;
    test_bootstrap::test_bootstrap_record_unresolved().await;
    test_bootstrap::test_short_dial_info_multibyte().await;
    test_bootstrap::test_bootstrap_peer_json_binding().await;
    test_bootstrap::test_config_bootstrap_records().await;
    test_bootstrap::test_bootstrap_cache_cap();
    #[cfg(feature = "debug-api")]
    test_chaos::test_all().await;
    test_connection_table::test_add_get_remove().await;
    test_connection_table::test_eviction().await;
    test_connection_table::test_dead_connection_skip().await;
    test_envelope_timestamps::test_all().await;
    test_loss_estimator::test_all();
    test_online_detector::test_all();
    test_protected_reconnect::test_all();
    #[cfg(not(all(target_arch = "wasm32", target_os = "unknown")))]
    test_egress_change::test_all();
    #[cfg(not(all(target_arch = "wasm32", target_os = "unknown")))]
    test_family_mtu::test_all();
    #[cfg(not(all(target_arch = "wasm32", target_os = "unknown")))]
    test_hop_mtu::test_hop_mtu_sizes_by_the_next_hop().await;
    #[cfg(not(all(target_arch = "wasm32", target_os = "unknown")))]
    test_mtu_probe_processor::test_all().await;
    #[cfg(not(all(target_arch = "wasm32", target_os = "unknown")))]
    test_hole_punch_ttl::test_all();
    test_transport_stats::test_all();
    test_transport_stats::test_ordering_availability_follows_outbound_protocols().await;
    test_address_family_confirm::test_all();
}
