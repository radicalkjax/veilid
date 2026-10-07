use super::*;

cfg_if! {
    if #[cfg(all(target_arch = "wasm32", target_os = "unknown"))] {

        /// Spawn a named task that invokes `callback` every `freq_ms` milliseconds.
        ///
        /// If `immediate`, the first invocation runs at once; otherwise after one period. The
        /// returned future cancels the task and waits for it to finish when awaited.
        ///
        /// Abstracts over the async runtime (tokio, async-std, wasm).
        ///
        /// The returned future must be driven to completion to stop the task. It holds the
        /// task's join handle, so dropping it before completion panics.
        pub fn interval<F, FUT>(name: &str, freq_ms: u32, immediate: bool, callback: F) -> PinBoxFutureStatic<()>
        where
            F: Fn() -> FUT + Send + Sync + 'static,
            FUT: Future<Output = ()> + Send,
        {
            let e = Eventual::new();

            let ie = e.clone();
            let jh = spawn(name, Box::pin(async move {
                let freq_u64 = (freq_ms as u64) * 1000u64;
                let start_tick_ts = get_raw_timestamp();

                let mut end_tick_ts = if immediate {
                    callback().await;
                    get_raw_timestamp()
                } else {
                    start_tick_ts
                };
                loop {
                    let wait_ms = ((freq_u64 - end_tick_ts.saturating_sub(start_tick_ts) % freq_u64) / 1000) as u32;
                    if timeout(wait_ms, ie.instance_clone(())).await.is_ok() {
                        break;
                    }

                    callback().await;

                    end_tick_ts = get_raw_timestamp();
                }
            }));

            Box::pin(async move {
                e.resolve().await;
                jh.await;
            })
        }

    } else {

        /// Spawn a named task that invokes `callback` every `freq_ms` milliseconds.
        ///
        /// If `immediate`, the first invocation runs at once; otherwise after one period. The
        /// returned future cancels the task and waits for it to finish when awaited.
        ///
        /// Abstracts over the async runtime (tokio, async-std, wasm).
        ///
        /// The returned future must be driven to completion to stop the task. It holds the
        /// task's join handle, so dropping it before completion panics.
        pub fn interval<F, FUT>(name: &str, freq_ms: u32, immediate: bool, callback: F) -> PinBoxFutureStatic<()>
        where
            F: Fn() -> FUT + Send + Sync + 'static,
            FUT: Future<Output = ()> + Send,
        {
            let e = Eventual::new();

            let ie = e.clone();
            let jh = spawn(name, async move {
                let freq_u64 = (freq_ms as u64) * 1000u64;
                let start_tick_ts = get_raw_timestamp();

                let mut end_tick_ts = if immediate {
                    callback().await;
                    get_raw_timestamp()
                } else {
                    start_tick_ts
                };
                loop {
                    let wait_ms = ((freq_u64 - end_tick_ts.saturating_sub(start_tick_ts) % freq_u64) / 1000) as u32;
                    if timeout(wait_ms, ie.instance_clone(())).await.is_ok() {
                        break;
                    }

                    callback().await;

                    end_tick_ts = get_raw_timestamp();
                }
            });

            Box::pin(async move {
                e.resolve().await;
                jh.await;
            })
        }

    }
}
