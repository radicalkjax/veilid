use super::*;

/// Run `x`, then yield once to the runtime before returning its result.
pub async fn yielding<R, T: FnOnce() -> R>(x: T) -> R {
    let out = x();
    let yielded_us = get_raw_timestamp();
    sleep(0).await;
    YieldTiming::log_inline_yield_if_slow(yielded_us, get_raw_timestamp());
    out
}

cfg_if! {
    if #[cfg(all(target_arch = "wasm32", target_os = "unknown", feature="rt-wasm-bindgen"))] {
        use async_executors::{Bindgen, LocalSpawnHandleExt, SpawnHandleExt};

        /// Spawn a named future on the async runtime, returning a join handle that must be awaited or detached.
        pub fn spawn<Out>(_name: &str, future: impl Future<Output = Out> + Send + 'static) -> MustJoinHandle<Out>
        where
            Out: Send + 'static,
        {
            MustJoinHandle::new(
                Bindgen
                    .spawn_handle(future)
                    .expect_or_log("wasm-bindgen-futures spawn_handle_local should never error out"),
            )
        }

        /// Spawn a named `!Send` future on the current thread, returning a join handle that must be awaited or detached.
        pub fn spawn_local<Out>(_name: &str, future: impl Future<Output = Out> + 'static) -> MustJoinHandle<Out>
        where
            Out: 'static,
        {
            MustJoinHandle::new(
                Bindgen
                    .spawn_handle_local(future)
                    .expect_or_log("wasm-bindgen-futures spawn_handle_local should never error out"),
            )
        }

        /// Spawn a named future and let it run unattended, with no join handle.
        pub fn spawn_detached<Out>(_name: &str, future: impl Future<Output = Out> + Send + 'static)
        where
            Out: Send + 'static,
        {
            Bindgen
                .spawn_handle_local(future)
                .expect_or_log("wasm-bindgen-futures spawn_handle_local should never error out")
                .detach()
        }
        /// Spawn a named `!Send` future on the current thread and let it run unattended, with no join handle.
        pub fn spawn_detached_local<Out>(_name: &str, future: impl Future<Output = Out> + 'static)
        where
            Out: 'static,
        {
            Bindgen
                .spawn_handle_local(future)
                .expect_or_log("wasm-bindgen-futures spawn_handle_local should never error out")
                .detach()
        }

        /// For things that use locks or IO operations or cpu tasks that take between 10-100us
        /// WASM version is a stub that just uses a yield point
        pub async fn blocking_yielding<R: Send + 'static>(x: impl FnOnce() -> R + Send + 'static) -> R {
            yielding(x).await
        }

        /// For heavy cpu-intensive tasks that do not use locks or IO operations > (100us)
        /// WASM version is a stub that just uses a yield point
        pub async fn cpu_yielding<R: Send + 'static>(x: impl FnOnce() -> R + Send + 'static) -> R {
            yielding(x).await
        }

        /// Run inline when `n` is below `cpu_n`, on the cpu pool otherwise
        pub async fn scaled_yielding<
            N: PartialOrd,
            R: Send + 'static,
            T: FnOnce() -> R + Send + 'static,
        >(
            _n: N,
            _cpu_n: N,
            x: T,
        ) -> R {
            // Due to speed issues and an inability to spawn in the WASM target, always yield
            yielding(x).await
        }

    } else {

        /// Spawn a named future on the async runtime, returning a join handle that must be awaited or detached.
        // allow, not expect: which cfg_if branch compiles depends on feature unification
        // across the workspace, so whether the lint fires here is not fixed.
        #[allow(
            clippy::disallowed_methods,
            reason = "this IS the named-spawn wrapper the lint points callers at"
        )]
        pub fn spawn<Out>(name: &str, future: impl Future<Output = Out> + Send + 'static) -> MustJoinHandle<Out>
        where
            Out: Send + 'static,
        {
            cfg_if! {
                if #[cfg(feature="rt-async-std")] {
                    MustJoinHandle::new(async_std::task::Builder::new().name(name.to_string()).spawn(future).unwrap_or_log())
                } else if #[cfg(all(tokio_unstable, feature="rt-tokio", feature="tracing"))] {
                    MustJoinHandle::new(tokio::task::Builder::new().name(name).spawn(future).unwrap_or_log())
                } else if #[cfg(feature="rt-tokio")] {
                    let _name = name;
                    MustJoinHandle::new(tokio::task::spawn(future))
                }
            }
        }

        /// Spawn a named `!Send` future on the current thread, returning a join handle that must be awaited or detached.
        pub fn spawn_local<Out>(name: &str, future: impl Future<Output = Out> + 'static) -> MustJoinHandle<Out>
        where
            Out: 'static,
        {
            cfg_if! {
                if #[cfg(feature="rt-async-std")] {
                    MustJoinHandle::new(async_std::task::Builder::new().name(name.to_string()).local(future).unwrap_or_log())
                } else if #[cfg(all(tokio_unstable, feature="rt-tokio", feature="tracing"))] {
                    MustJoinHandle::new(tokio::task::Builder::new().name(name).spawn_local(future).unwrap_or_log())
                } else if #[cfg(feature="rt-tokio")] {
                    let _name = name;
                    MustJoinHandle::new(tokio::task::spawn_local(future))
                }
            }
        }

        /// Spawn a named future and let it run unattended, with no join handle.
        // allow, not expect: which cfg_if branch compiles depends on feature unification
        // across the workspace, so whether the lint fires here is not fixed.
        #[allow(
            clippy::disallowed_methods,
            reason = "this IS the named-spawn wrapper the lint points callers at"
        )]
        pub fn spawn_detached<Out>(name: &str, future: impl Future<Output = Out> + Send + 'static)
        where
            Out: Send + 'static,
        {
            cfg_if! {
                if #[cfg(feature="rt-async-std")] {
                    drop(async_std::task::Builder::new().name(name.to_string()).spawn(future).unwrap_or_log());
                } else if #[cfg(all(tokio_unstable, feature="rt-tokio", feature="tracing"))] {
                    drop(tokio::task::Builder::new().name(name).spawn(future).unwrap_or_log());
                } else if #[cfg(feature="rt-tokio")] {
                    let _name = name;
                    drop(tokio::task::spawn(future))
                }
            }
        }

        /// Spawn a named `!Send` future on the current thread and let it run unattended, with no join handle.
        pub fn spawn_detached_local<Out>(name: &str,future: impl Future<Output = Out> + 'static)
        where
            Out: 'static,
        {
            cfg_if! {
                if #[cfg(feature="rt-async-std")] {
                    drop(async_std::task::Builder::new().name(name.to_string()).local(future).unwrap_or_log());
                } else if #[cfg(all(tokio_unstable, feature="rt-tokio", feature="tracing"))] {
                    drop(tokio::task::Builder::new().name(name).spawn_local(future).unwrap_or_log());
                } else if #[cfg(feature="rt-tokio")] {
                    let _name = name;
                    drop(tokio::task::spawn_local(future))
                }
            }
        }

        /// Run a blocking closure on the runtime's dedicated blocking-thread pool and await its result.
        pub async fn blocking_wrapper<F, R>(blocking_task: F) -> R
        where
            F: FnOnce() -> R + Send + 'static,
            R: Send + 'static,
        {
            let enqueued_us = get_raw_timestamp();
            let timed_task = move || {
                let started_us = get_raw_timestamp();
                let result = blocking_task();
                (result, started_us, get_raw_timestamp())
            };
            // run blocking stuff in blocking thread
            let (result, started_us, finished_us);
            cfg_if! {
                if #[cfg(feature="rt-async-std")] {
                    (result, started_us, finished_us) = async_std::task::spawn_blocking(timed_task).await;
                } else if #[cfg(feature="rt-tokio")] {
                    (result, started_us, finished_us) = tokio::task::spawn_blocking(timed_task).await.unwrap_or_log();
                } else {
                    #[compile_error("must use an executor")]
                }
            }
            YieldTiming {
                enqueued_us,
                started_us,
                finished_us,
                woken_us: get_raw_timestamp(),
            }
            .log_if_slow("blocking", None);
            result
        }

        /// For things that use locks or IO operations or cpu tasks that take between 10-100us
        pub async fn blocking_yielding<R: Send + 'static>(x: impl FnOnce() -> R + Send + 'static) -> R
        {
            blocking_wrapper(x).await
        }

        /// For heavy cpu-intensive tasks that do not use locks or IO operations > (100us)
        pub async fn cpu_yielding<R: Send + 'static>(x: impl FnOnce() -> R + Send + 'static) -> R {
            let (tx, rx) = flume::bounded(1);
            let depth = cpu_pool_queue_depth();
            let enqueued_us = get_raw_timestamp();
            cpu_pool_spawn_fifo(Box::new(move || {
                let started_us = get_raw_timestamp();
                let result = x();
                let _ = tx.send((result, started_us, get_raw_timestamp()));
            }));
            let (result, started_us, finished_us) = rx.recv_async().await.unwrap_or_log();
            YieldTiming {
                enqueued_us,
                started_us,
                finished_us,
                woken_us: get_raw_timestamp(),
            }
            .log_if_slow("cpu", Some(depth));
            result
        }

        /// Run inline when `n` is below `cpu_n`, on the cpu pool otherwise
        pub async fn scaled_yielding<
            N: PartialOrd,
            R: Send + 'static,
            T: FnOnce() -> R + Send + 'static,
            >(
            n: N,
            cpu_n: N,
            x: T,
            ) -> R {
            // Inline below the pool threshold: a yield after a short op costs a full queue pass
            if n < cpu_n {
                x()
            } else {
                cpu_yielding(x).await
            }
        }
    }
}
