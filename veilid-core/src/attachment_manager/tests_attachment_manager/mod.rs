#[cfg(feature = "app-lifecycle")]
pub mod test_app_lifecycle;
#[cfg(not(all(target_arch = "wasm32", target_os = "unknown")))]
pub mod test_attach_detach;
pub mod test_attachment_level_calculator;

use super::*;

#[cfg_attr(
    all(target_arch = "wasm32", target_os = "unknown"),
    expect(clippy::unused_async)
)]
pub async fn test_all() {
    test_attachment_level_calculator::test_latency_thresholds_at_default_rpc_timeout();
    test_attachment_level_calculator::test_attaching_when_no_live_peers();
    test_attachment_level_calculator::test_weak_floor_when_live_but_untested();
    test_attachment_level_calculator::test_well_attached_server();
    test_attachment_level_calculator::test_minimal_test_node();
    test_attachment_level_calculator::test_partial_attached_no_latency_penalty();
    test_attachment_level_calculator::test_latency_penalty_drops_one_bar();
    test_attachment_level_calculator::test_severe_latency_drops_three_bars();
    test_attachment_level_calculator::test_no_latency_samples_no_penalty();
    test_attachment_level_calculator::test_bar_count_mapping();
    #[cfg(feature = "app-lifecycle")]
    {
        test_app_lifecycle::test_detection_wins_where_it_reports();
        test_app_lifecycle::test_report_fills_a_gap();
        test_app_lifecycle::test_report_alone_without_detection();
    }
    #[cfg(not(all(target_arch = "wasm32", target_os = "unknown")))]
    test_attach_detach::test_bind_wait_is_attaching().await;
}
