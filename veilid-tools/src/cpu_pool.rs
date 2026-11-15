#![cfg(not(all(target_arch = "wasm32", target_os = "unknown")))]

use super::*;
use std::sync::OnceLock;

// Dedicated FIFO pool so heavy CPU work never starves the executor's blocking pool
static CPU_POOL: OnceLock<flume::Sender<Box<dyn FnOnce() + Send>>> = OnceLock::new();
static CPU_POOL_ENQUEUED: AtomicUsize = AtomicUsize::new(0);
static CPU_POOL_STARTED: AtomicUsize = AtomicUsize::new(0);

/// Tasks handed to the pool that no thread has picked up yet.
#[must_use]
pub fn cpu_pool_queue_depth() -> usize {
    CPU_POOL_ENQUEUED
        .load(Ordering::Relaxed)
        .saturating_sub(CPU_POOL_STARTED.load(Ordering::Relaxed))
}

/// Run a task on the dedicated CPU pool in FIFO order.
pub fn cpu_pool_spawn_fifo(task: Box<dyn FnOnce() + Send>) {
    let tx = CPU_POOL.get_or_init(|| {
        let (tx, rx) = flume::unbounded::<Box<dyn FnOnce() + Send>>();
        let count = std::thread::available_parallelism()
            .map(|n| n.get())
            .unwrap_or(1);
        for n in 0..count {
            let rx = rx.clone();
            std::thread::Builder::new()
                .name(format!("veilid-cpu-{}", n))
                .spawn(move || {
                    while let Ok(task) = rx.recv() {
                        CPU_POOL_STARTED.fetch_add(1, Ordering::Relaxed);
                        task();
                    }
                })
                .expect("failed to spawn cpu pool thread");
        }
        tx
    });
    CPU_POOL_ENQUEUED.fetch_add(1, Ordering::Relaxed);
    let _ = tx.send(task);
}
