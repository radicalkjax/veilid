//! Bounded hand-off between the tracing call site and the application's update callback.
//!
//! An `UpdateCallback` invoked inline, on whatever thread emitted the event, makes the embedding
//! application's ability to keep up backpressure on veilid's own network threads: where the
//! callback crosses an FFI boundary (veilid-flutter serializes each update and posts it into the
//! Dart isolate) every event is a JSON encode and a cross-runtime post, and a UDP socket wedged on
//! ENOTCONN produces 30k events/s down that path, enough to stall the host app's UI thread until
//! the OS kills it.
//!
//! Everything here is best effort by construction. Logs are diagnostics: losing some of them is
//! always the right trade against stalling the node or the application embedding it.
//!
//! Order of defence, cheapest first:
//! 1. a bounded queue, so a burst costs memory that is capped rather than the caller's thread;
//! 2. a rate limit on delivery, so a sustained storm cannot outrun the consumer;
//! 3. fuzzy deduplication, applied only once the queue is backed up past what the rate limit will
//!    deliver - a storm is nearly always one message repeating, and collapsing it keeps the
//!    information while dropping the volume;
//! 4. dropping, counted and reported, when even that is not enough.

use super::*;
use core::fmt::Write as _;

/// Queue depth. Roughly a second of a bad storm; past this the drop counter takes over.
const QUEUE_CAPACITY: usize = 4096;

/// How often the drain wakes to deliver a batch.
const DRAIN_INTERVAL_MS: u32 = 100;

/// Deliveries allowed per interval, so 320/s sustained. Far above any human-readable rate and far
/// below what an FFI post can absorb.
const MAX_DELIVERIES_PER_INTERVAL: usize = 32;

/// A batch larger than one interval's budget means the producer is outrunning the consumer, which
/// is the only situation where collapsing distinct-but-similar messages is worth the information
/// it costs.
const DEDUP_WHEN_BATCH_EXCEEDS: usize = MAX_DELIVERIES_PER_INTERVAL;

/// Cap on how much the drain pulls out of the channel in one pass, so dedup work stays bounded.
const MAX_BATCH: usize = QUEUE_CAPACITY;

/// One event on its way to the application.
pub(super) struct QueuedLog {
    /// Empty means the event came from process-wide code and reaches every registered callback
    pub log_key: String,
    /// Severity of the event
    pub log_level: VeilidLogLevel,
    /// Formatted event text
    pub message: String,
    /// Captured for errors when the `backtrace` feature is on
    pub backtrace: Option<String>,
}

/// Where the drain hands events that survived the queue.
///
/// Injected at `start` rather than reached for, so this type knows nothing about update callbacks
/// or what a delivery costs - only how many of them per second it is willing to attempt.
pub(super) type QueuedLogSink = Arc<dyn Fn(QueuedLog) + Send + Sync>;

/// The bounded queue and its drain.
///
/// Holds no global state, so its owner decides how many exist and for how long; `ApiTracingLayer`
/// keeps the one this crate uses. `start` does take `&'static self` - the drain outlives the call
/// that spawns it and reads the drop counter, which is what pins an instance to a `static` for
/// anyone who wants a running drain.
pub(super) struct ApiLogQueue {
    /// Lock-free: every emitting thread touches this on every event, and a mutex would serialize them
    sender: ArcSwapOption<flume::Sender<QueuedLog>>,
    /// Events not queued since the last drain pass reported them
    dropped: AtomicU64,
}

impl ApiLogQueue {
    pub(super) const fn new() -> Self {
        Self {
            sender: ArcSwapOption::const_empty(),
            dropped: AtomicU64::new(0),
        }
    }

    /// Start the drain if it is not already running, delivering through `sink`. Idempotent.
    ///
    /// Callers serialize this against `stop`; the layer does so under its callback map's lock.
    pub(super) fn start(&'static self, sink: QueuedLogSink) {
        if self.sender.load().is_some() {
            return;
        }
        let (tx, rx) = flume::bounded::<QueuedLog>(QUEUE_CAPACITY);
        self.sender.store(Some(Arc::new(tx)));
        self.spawn_drain(rx, sink);
    }

    /// Stop accepting events. Whatever is still queued is discarded rather than delivered: the
    /// callbacks were deregistered before this is reached, and calling back into a node that has
    /// finished shutting down is worse than losing the last few log lines.
    pub(super) fn stop(&self) {
        self.sender.store(None);
    }

    /// Hand an event to the drain, or count it as dropped. Never blocks the caller.
    pub(super) fn enqueue(&self, log: QueuedLog) {
        let Some(sender) = self.sender.load_full() else {
            return;
        };
        if sender.try_send(log).is_err() {
            self.note_dropped();
        }
    }

    /// Count an event the caller chose not to build because the queue was already full.
    pub(super) fn note_dropped(&self) {
        self.dropped.fetch_add(1, Ordering::Relaxed);
    }

    /// Whether the queue is full, so a caller can skip building an event that would be dropped.
    #[must_use]
    pub(super) fn is_full(&self) -> bool {
        self.sender
            .load()
            .as_ref()
            .is_some_and(|sender| sender.is_full())
    }

    cfg_if! {
        if #[cfg(all(target_arch = "wasm32", target_os = "unknown"))] {
            /// No threads in the browser, so the drain is a task on the event loop. It still buys
            /// the thing that matters: the emitting call site does not perform the delivery.
            fn spawn_drain(&'static self, rx: flume::Receiver<QueuedLog>, sink: QueuedLogSink) {
                spawn_detached_local("veilid api log drain", async move {
                    while let Ok(first) = rx.recv_async().await {
                        if self.drain_one_pass(&rx, &sink, first) {
                            sleep(DRAIN_INTERVAL_MS).await;
                        }
                    }
                });
            }
        } else {
            /// A dedicated thread rather than a task on the executor: delivery blocks on whatever
            /// the application does with the update, which must not happen on the runtime's workers.
            fn spawn_drain(&'static self, rx: flume::Receiver<QueuedLog>, sink: QueuedLogSink) {
                if let Err(e) = std::thread::Builder::new()
                    .name("veilid-api-log".to_owned())
                    .spawn(move || {
                        while let Ok(first) = rx.recv() {
                            if !self.drain_one_pass(&rx, &sink, first) {
                                continue;
                            }
                            #[expect(
                                clippy::disallowed_methods,
                                reason = "this thread is not the executor: it exists to block, and an async sleep here would need a runtime it deliberately does not join"
                            )]
                            std::thread::sleep(core::time::Duration::from_millis(
                                u64::from(DRAIN_INTERVAL_MS),
                            ));
                        }
                    })
                {
                    // Cleared first, so the error below skips this layer and reaches the others
                    self.sender.store(None);
                    tracing::error!("failed to spawn veilid api log drain: {e}");
                }
            }
        }
    }

    /// Take everything waiting, collapse it if the producer is outrunning us, deliver up to budget.
    ///
    /// Returns whether the rate limit is what ended the pass, so the caller pauses only when
    /// pausing is what the limit is for. A quiet node should not have 100ms added to every line.
    fn drain_one_pass(
        &self,
        rx: &flume::Receiver<QueuedLog>,
        sink: &QueuedLogSink,
        first: QueuedLog,
    ) -> bool {
        let mut batch = Vec::with_capacity(MAX_BATCH.min(rx.len() + 1));
        batch.push(first);
        while batch.len() < MAX_BATCH {
            match rx.try_recv() {
                Ok(log) => batch.push(log),
                Err(_) => break,
            }
        }

        if batch.len() > DEDUP_WHEN_BATCH_EXCEEDS {
            batch = Self::deduplicate(batch);
        }

        let delivered = batch.len().min(MAX_DELIVERIES_PER_INTERVAL);
        let suppressed = batch.len() - delivered;
        for log in batch.into_iter().take(delivered) {
            sink(log);
        }

        let dropped = self.dropped.swap(0, Ordering::Relaxed) + suppressed as u64;
        if dropped > 0 {
            sink(QueuedLog {
                log_key: String::new(),
                log_level: VeilidLogLevel::Warn,
                message: format!(
                    "api log: {dropped} messages dropped, logging faster than delivery"
                ),
                backtrace: None,
            });
        }

        suppressed > 0 || !rx.is_empty()
    }

    /// Collapse messages of the same shape, keeping the first of each and how many it stands for.
    pub(super) fn deduplicate(batch: Vec<QueuedLog>) -> Vec<QueuedLog> {
        // Keyed by level as well as shape: the same words at error and at debug are different
        let mut seen: HashMap<(u8, String), usize> = HashMap::new();
        let mut out: Vec<QueuedLog> = Vec::new();
        let mut counts: Vec<u64> = Vec::new();

        for log in batch {
            let key = (log.log_level as u8, Self::fuzzy_key(&log.message));
            match seen.get(&key) {
                Some(&idx) => counts[idx] += 1,
                None => {
                    seen.insert(key, out.len());
                    out.push(log);
                    counts.push(1);
                }
            }
        }

        for (log, count) in out.iter_mut().zip(counts) {
            if count > 1 {
                let _ = write!(log.message, " [and {} more like it]", count - 1);
            }
        }
        out
    }

    /// A message with its variable parts collapsed, so two events of the same shape share a key.
    ///
    /// Deliberately crude, and only has to be right often enough to collapse a storm: any
    /// alphanumeric run containing a digit becomes `#`, which folds addresses, ports, counts, byte
    /// lengths and errno values together while leaving prose alone.
    pub(super) fn fuzzy_key(message: &str) -> String {
        let mut out = String::with_capacity(message.len());
        let mut run_start: Option<usize> = None;
        let mut run_has_digit = false;

        let push_run = |out: &mut String, start: usize, end: usize, has_digit: bool| {
            if has_digit {
                out.push('#');
            } else {
                out.push_str(&message[start..end]);
            }
        };

        for (i, c) in message.char_indices() {
            if c.is_alphanumeric() {
                if run_start.is_none() {
                    run_start = Some(i);
                    run_has_digit = false;
                }
                run_has_digit |= c.is_ascii_digit();
            } else {
                if let Some(start) = run_start.take() {
                    push_run(&mut out, start, i, run_has_digit);
                }
                out.push(c);
            }
        }
        if let Some(start) = run_start {
            push_run(&mut out, start, message.len(), run_has_digit);
        }
        out
    }
}
