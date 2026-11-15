use super::*;

cfg_if! {
    if #[cfg(all(target_arch = "wasm32", target_os = "unknown"))] {
        use futures_util::future::{select, Either};

        /// Await `f`, failing with [`TimeoutError`] if it does not complete within `dur_ms` milliseconds.
        ///
        /// Abstracts over the async runtime (tokio, async-std, wasm).
        pub async fn timeout<F, T>(dur_ms: u32, f: F) -> Result<T, TimeoutError>
        where
            F: Future<Output = T>,
        {
            let tout = select(Box::pin(sleep(dur_ms)), Box::pin(f));

            match tout.await {
                Either::Left((_x, _b)) => Err(TimeoutError()),
                Either::Right((y, _a)) => Ok(y),
            }
        }

    } else {

        /// Await `f`, failing with [`TimeoutError`] if it does not complete within `dur_ms` milliseconds.
        ///
        /// Abstracts over the async runtime (tokio, async-std, wasm).
        pub async fn timeout<F, T>(dur_ms: u32, f: F) -> Result<T, TimeoutError>
        where
            F: Future<Output = T>,
        {
            cfg_if! {
                if #[cfg(feature="rt-async-std")] {
                    let tout = async_std::future::timeout(Duration::from_millis(dur_ms as u64), f);
                } else if #[cfg(feature="rt-tokio")] {
                    let tout = tokio::time::timeout(Duration::from_millis(dur_ms as u64), f);
                }
            }

            tout.await.map_err(|e| e.into())
        }

    }
}
