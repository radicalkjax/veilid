/// One-line summary of the async runtime's load, for stall logs. None outside a
/// runtime, or on runtimes that expose no metrics.
#[must_use]
pub fn executor_snapshot() -> Option<String> {
    cfg_if::cfg_if! {
        if #[cfg(all(feature = "rt-tokio", not(all(target_arch = "wasm32", target_os = "unknown"))))] {
            let handle = tokio::runtime::Handle::try_current().ok()?;
            let m = handle.metrics();
            Some(format!(
                "workers {} alive tasks {} global queue {}",
                m.num_workers(),
                m.num_alive_tasks(),
                m.global_queue_depth()
            ))
        } else {
            None
        }
    }
}

/// The snapshot as a bracketed log suffix, empty when unavailable
#[must_use]
pub fn executor_snapshot_suffix() -> String {
    executor_snapshot().map_or_else(String::new, |s| format!(" [{}]", s))
}
