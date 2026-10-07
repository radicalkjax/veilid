use super::*;

/// Run `x`, then yield once to the runtime before returning its result.
pub async fn yielding<R, T: FnOnce() -> R>(x: T) -> R {
    let out = x();
    sleep(0).await;
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

        /// Yielding that chooses the best mechanism using a scaled approach
        pub async fn scaled_yielding<
            N: PartialOrd,
            R: Send + 'static,
            T: FnOnce() -> R + Send + 'static,
        >(
            _n: N,
            _yield_n: N,
            _cpu_n: N,
            x: T,
        ) -> R {
            // Due to speed issues and an inability to spawn in the WASM target, always yield
            yielding(x).await
        }

    } else {

        /// Spawn a named future on the async runtime, returning a join handle that must be awaited or detached.
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
            // run blocking stuff in blocking thread
            cfg_if! {
                if #[cfg(feature="rt-async-std")] {
                    async_std::task::spawn_blocking(blocking_task).await
                } else if #[cfg(feature="rt-tokio")] {
                    tokio::task::spawn_blocking(blocking_task).await.unwrap_or_log()
                } else {
                    #[compile_error("must use an executor")]
                }
            }
        }

        /// For things that use locks or IO operations or cpu tasks that take between 10-100us
        pub async fn blocking_yielding<R: Send + 'static>(x: impl FnOnce() -> R + Send + 'static) -> R
        {
            blocking_wrapper(x).await
        }

        /// For heavy cpu-intensive tasks that do not use locks or IO operations > (100us)
        pub async fn cpu_yielding<R: Send + 'static>(x: impl FnOnce() -> R + Send + 'static) -> R {
            let (tx, rx) = flume::bounded(1);
            cpu_pool_spawn_fifo(Box::new(move || {
                let result = x();
                let _ = tx.send(result);
            }));
            rx.recv_async().await.unwrap_or_log()
        }

        /// Yielding that chooses the best mechanism using a scaled approach
        pub async fn scaled_yielding<
            N: PartialOrd,
            R: Send + 'static,
            T: FnOnce() -> R + Send + 'static,
            >(
            n: N,
            yield_n: N,
            cpu_n: N,
            x: T,
            ) -> R {
            if n < yield_n {
                x()
            } else if n < cpu_n {
                yielding(x).await
            } else {
                cpu_yielding(x).await
            }
        }
    }
}
