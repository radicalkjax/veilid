use cfg_if::*;
use core::future::Future;

cfg_if! {
    if #[cfg(feature="rt-async-std")] {
        pub use async_std::task::JoinHandle;
        pub use async_std::net::TcpStream;
        pub use async_std::sync::Mutex as AsyncMutex;

        pub fn spawn<F: Future<Output = T> + Send + 'static, T: Send + 'static>(f: F) -> JoinHandle<T> {
            async_std::task::spawn(f)
        }

        pub use async_std::task::sleep;
        pub use async_std::future::timeout;
    } else if #[cfg(feature="rt-tokio")] {
        pub use tokio::task::JoinHandle;

        pub fn spawn<F: Future<Output = T> + Send + 'static, T: Send + 'static>(f: F) -> JoinHandle<T> {
            GLOBAL_RUNTIME.spawn(f)
        }

        /// Enter the global runtime context so blocking-style API calls (such
        /// as the OTLP exporter's `build()` which spawns a connection-management
        /// task on the active runtime) succeed when called outside an `await`.
        /// The returned guard installs the runtime as the current thread's
        /// runtime until it is dropped.
        #[cfg(feature = "opentelemetry")]
        pub fn enter_global_runtime() -> tokio::runtime::EnterGuard<'static> {
            GLOBAL_RUNTIME.enter()
        }

        lazy_static::lazy_static! {
            static ref GLOBAL_RUNTIME: tokio::runtime::Runtime = tokio::runtime::Runtime::new().unwrap();
        }
    } else {
        compile_error!("needs executor implementation");
    }
}
