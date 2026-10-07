//! Test suite for Native
#![cfg(not(all(target_arch = "wasm32", target_os = "unknown")))]

mod test_assembly_buffer;
mod test_async_peek_stream;
mod test_igd;
mod test_network_interfaces;

use super::*;

//////////////////////////////////////////////////////////////////////////////////
// Allow access to tests from non cfg(test), as required for android and ios tests

#[allow(dead_code)]
pub async fn run_all_tests() {
    // Timestamp tests
    info!("TEST: test_timestamp");
    test_timestamp::test_get_raw_timestamp();
    test_timestamp::test_sleep().await;

    // IGD parsing tests
    info!("TEST: test_igd");
    test_igd::test_all();

    // Tools basic tests
    info!("TEST: test_tools_basic");
    test_tools_basic::test_log();
    test_tools_basic::test_tools();

    // Random tests
    info!("TEST: test_random");
    test_random::test_get_random_u64();
    test_random::test_get_random_u32();

    // Split URL tests
    info!("TEST: test_split_url");
    test_split_url::test_split_url();

    // Must join tests
    #[cfg(not(all(target_arch = "wasm32", target_os = "unknown")))]
    {
        info!("TEST: test_must_join");
        test_must_join::test_must_join_single_future().await;
    }

    // Eventual tests
    info!("TEST: test_eventual");
    test_eventual::test_eventual().await;
    test_eventual::test_eventual_value().await;
    test_eventual::test_eventual_value_clone().await;

    // Interval and timeout tests
    info!("TEST: test_interval_and_timeout");
    test_interval_and_timeout::test_interval().await;
    test_interval_and_timeout::test_timeout().await;

    // Other module tests
    info!("TEST: exec_test_startup_lock");
    test_startup_lock::test_all().await;
    info!("TEST: exec_test_event_bus");
    test_event_bus::test_all().await;
    info!("TEST: exec_test_network_interfaces");
    test_network_interfaces::test_all().await;
    info!("TEST: exec_test_async_peek_stream");
    test_async_peek_stream::test_all().await;
    info!("TEST: exec_test_async_keyed_cache");
    test_async_keyed_cache::test_all().await;
    info!("TEST: exec_test_async_tag_lock");
    test_async_tag_lock::test_all().await;
    info!("TEST: exec_test_async_weighted_semaphore");
    test_async_weighted_semaphore::test_all().await;
    info!("TEST: exec_test_tag_lock");
    test_tag_lock::test_all();
    info!("TEST: exec_test_assembly_buffer");
    test_assembly_buffer::test_all().await;
    info!("TEST: exec_test_tracked_mutex");
    test_tracked_mutex::test_all();

    info!("Finished unit tests");
}

#[cfg(feature = "rt-tokio")]
static RUNTIME: std::sync::LazyLock<tokio::runtime::Runtime> = std::sync::LazyLock::new(|| {
    tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .expect("Failed to build Tokio runtime")
});

#[cfg(feature = "rt-tokio")]
#[allow(dead_code)]
pub fn block_on<F: Future<Output = T>, T>(f: F) -> T {
    RUNTIME.block_on(f)
}

#[cfg(feature = "rt-async-std")]
#[allow(dead_code)]
pub fn block_on<F: Future<Output = T>, T>(f: F) -> T {
    async_std::task::block_on(f)
}

///////////////////////////////////////////////////////////////////////////
cfg_if! {
    if #[cfg(test)] {

        use std::sync::Once;

        static SETUP_ONCE: Once = Once::new();

        pub fn setup() {
            SETUP_ONCE.call_once(|| {

                cfg_if! {
                    if #[cfg(feature = "tracing")] {
                        use tracing_subscriber::{EnvFilter, fmt, prelude::*};
                        let env_filter = EnvFilter::builder().from_env_lossy();
                        let fmt_layer = fmt::layer().with_filter(env_filter);
                        tracing_subscriber::registry()
                            .with(fmt_layer)
                            .init();
                    } else {
                        env_logger::builder().is_test(true).try_init().expect("Failed to initialize env logger");
                    }
                }

                #[cfg(feature = "debug-locks")]
                block_on(async {
                    crate::deadlock_detector::start_deadlock_detector();
                });

            });
        }

        #[test]
        fn run_test_timestamp() {
            setup();
            test_timestamp::test_get_raw_timestamp();
            block_on(async {
                test_timestamp::test_sleep().await;
            });
        }

        #[test]
        fn run_test_tools_basic() {
            setup();
            test_tools_basic::test_log();
            test_tools_basic::test_tools();
        }

        #[test]
        fn run_test_random() {
            setup();
            test_random::test_get_random_u64();
            test_random::test_get_random_u32();
        }

        #[test]
        fn run_test_ip_extra() {
            setup();
            test_ip_extra::test_ipv4addr_predicates();
            test_ip_extra::test_ipv6addr_predicates();
        }

        #[test]
        fn run_test_split_url() {
            setup();
            test_split_url::test_split_url();
        }

        #[test]
        fn run_test_must_join() {
            setup();
            block_on(async {
                test_must_join::test_must_join_single_future().await;
            });
        }

        #[test]
        fn run_test_eventual() {
            setup();
            block_on(async {
                test_eventual::test_eventual().await;
                test_eventual::test_eventual_value().await;
                test_eventual::test_eventual_value_clone().await;
            });
        }

        #[test]
        fn run_test_interval_and_timeout() {
            setup();
            block_on(async {
                test_interval_and_timeout::test_interval().await;
                test_interval_and_timeout::test_timeout().await;
            });
        }

        #[test]
        fn run_test_startup_lock() {
            setup();
            block_on(async {
                test_startup_lock::test_all().await;
            });
        }

        #[test]
        fn run_test_async_weighted_semaphore() {
            setup();
            block_on(async {
                test_async_weighted_semaphore::test_all().await;
            });
        }

        #[test]
        fn run_test_event_bus() {
            setup();
            block_on(async {
                test_event_bus::test_all().await;
            });
        }

        #[test]
        fn run_test_network_interfaces() {
            setup();
            block_on(async {
                test_network_interfaces::test_all().await;
            });
        }

        #[test]
        fn run_test_async_peek_stream() {
            setup();
            block_on(async {
                test_async_peek_stream::test_all().await;
            });
        }

        #[test]
        fn run_test_async_tag_lock() {
            setup();
            block_on(async {
                test_async_tag_lock::test_all().await;
            });
        }

        #[test]
        fn run_test_tag_lock() {
            setup();
            test_tag_lock::test_all();
        }

        #[test]
        fn run_test_igd() {
            setup();
            test_igd::test_all();
        }

        #[test]
        fn run_test_assembly_buffer() {
            setup();
            block_on(async {
                test_assembly_buffer::test_all().await;
            });
        }

        #[test]
        fn run_test_tracked_mutex() {
            setup();
            test_tracked_mutex::test_all();
        }
    }
}
