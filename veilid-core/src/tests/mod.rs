pub mod common;
pub mod mocks;

pub use common::*;
pub use mocks::*;

#[cfg(all(target_os = "android", feature = "veilid_core_android_tests"))]
mod android;
#[cfg(all(target_os = "ios", feature = "veilid_core_ios_tests"))]
mod ios;
#[cfg(not(all(target_arch = "wasm32", target_os = "unknown")))]
mod native;
#[cfg(all(test, not(all(target_arch = "wasm32", target_os = "unknown"))))]
pub use native::wait_for_debugger;

use super::*;

/// Crypto op wall times alone; prints, never asserts
#[cfg(any(test, feature = "test-util"))]
pub async fn run_crypto_timing_test() {
    crypto::tests_crypto::test_crypto_timing::test_all().await;
}

/// Main tests entry point for ios, android, and wasm targets
#[cfg(any(test, feature = "test-util"))]
pub async fn run_all_tests() {
    // Mock only: unlike nextest this monolithic harness cannot filter out the live attach, and check_all.sh stays offline
    #[cfg(feature = "mock-api")]
    {
        info!("TEST: veilid_core");
        test_attachment_manager::test_all().await;
    }

    #[cfg(not(feature = "mock-api"))]
    {
        info!("TEST: attachment_manager");
        attachment_manager::tests_attachment_manager::test_all().await;
    }

    #[cfg(all(
        not(feature = "mock-api"),
        not(all(target_arch = "wasm32", target_os = "unknown"))
    ))]
    {
        info!("TEST: test_bootstrap");
        test_bootstrap::test_all().await;
    }

    info!("TEST: logging");
    logging::tests_logging::test_all().await;

    info!("TEST: crypto");
    crypto::tests_crypto::test_all().await;

    #[cfg(not(feature = "mock-api"))]
    {
        info!("TEST: test_table_store");
        table_store::tests_table_store::test_all().await;
    }

    #[cfg(not(feature = "mock-api"))]
    {
        info!("TEST: test_protected_store");
        protected_store::tests_protected_store::test_all().await;
    }

    info!("TEST: veilid_api");
    veilid_api::tests_veilid_api::test_all().await;

    // These need a DHT to talk to, so they run only against the mock
    #[cfg(feature = "mock-api")]
    {
        info!("TEST: test_dht");
        test_dht::test_all().await;

        info!("TEST: test_dht_transactions");
        test_dht_transactions::test_all().await;
    }

    #[cfg(not(feature = "mock-api"))]
    {
        info!("TEST: routing_table");
        routing_table::tests_routing_table::test_all().await;
    }

    #[cfg(not(feature = "mock-api"))]
    {
        info!("TEST: network_manager");
        network_manager::tests_network_manager::test_all().await;
    }

    #[cfg(not(feature = "mock-api"))]
    {
        info!("TEST: storage_manager");
        storage_manager::tests_storage_manager::test_all().await;
    }

    #[cfg(not(feature = "mock-api"))]
    {
        info!("TEST: rpc_processor");
        rpc_processor::tests_rpc_processor::test_all().await;
    }

    info!("Finished unit tests");
}
