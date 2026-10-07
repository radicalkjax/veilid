//! Test suite for the Web and headless browsers.
#![cfg(all(target_arch = "wasm32", target_os = "unknown"))]

use cfg_if::*;
use parking_lot::Once;
use veilid_tools::tests::*;

use wasm_bindgen_test::*;

wasm_bindgen_test_configure!(run_in_browser);

static SETUP_ONCE: Once = Once::new();
pub fn setup() -> () {
    SETUP_ONCE.call_once(|| {
        console_error_panic_hook::set_once();
        cfg_if! {
            if #[cfg(feature = "tracing")] {
                let config = veilid_tools::wasm_tracing::WasmLayerConfig::new()
                    .remove_timings()
                    .remove_color();
                let _ = veilid_tools::wasm_tracing::set_as_global_default_with_config(config);
            } else {
                wasm_logger::init(wasm_logger::Config::default());
            }
        }
    });
}

#[wasm_bindgen_test]
async fn run_test_timestamp() {
    setup();
    test_timestamp::test_get_raw_timestamp();
    test_timestamp::test_sleep().await;
}

#[wasm_bindgen_test]
async fn run_test_tools_basic() {
    setup();
    test_tools_basic::test_log();
    test_tools_basic::test_tools();
}

#[wasm_bindgen_test]
async fn run_test_random() {
    setup();
    test_random::test_get_random_u64();
    test_random::test_get_random_u32();
}

#[wasm_bindgen_test]
async fn run_test_ip_extra() {
    setup();
    test_ip_extra::test_ipv4addr_predicates();
    test_ip_extra::test_ipv6addr_predicates();
}

#[wasm_bindgen_test]
async fn run_test_split_url() {
    setup();
    test_split_url::test_split_url();
}

#[wasm_bindgen_test]
async fn run_test_eventual() {
    setup();
    test_eventual::test_eventual().await;
    test_eventual::test_eventual_value().await;
    test_eventual::test_eventual_value_clone().await;
}

#[wasm_bindgen_test]
async fn run_test_interval_and_timeout() {
    setup();
    test_interval_and_timeout::test_interval().await;
    test_interval_and_timeout::test_timeout().await;
}

#[wasm_bindgen_test]
async fn run_test_event_bus() {
    setup();

    test_event_bus::test_all().await;
}

#[wasm_bindgen_test]
async fn run_test_async_tag_lock() {
    setup();

    test_async_tag_lock::test_all().await;
}

#[wasm_bindgen_test]
async fn run_test_async_weighted_semaphore() {
    setup();

    test_async_weighted_semaphore::test_all().await;
}

#[wasm_bindgen_test]
async fn run_test_startup_lock() {
    setup();

    test_startup_lock::test_all().await;
}

#[wasm_bindgen_test]
async fn run_test_tracked_mutex() {
    setup();

    test_tracked_mutex::test_all();
}
