//! FutureRaceCompletionPool
//!
//! Spawn a future to run to completion in the background and optionally
//! receive its result through a channel. If the receiver is dropped, the
//! spawned future still runs to completion (its result is discarded). The
//! pool tracks lifetime via a read/write lock so a caller can wait for all
//! in-flight handoffs to drain before shutting down.

use super::*;

use futures_util::future::{select, Either};
use std::pin::Pin;

/// Outcome of `race_complete_on`
#[derive(Debug)]
pub enum FutureRaceCompletion<T, R> {
    /// The original future finished first
    Original(T),
    /// The race future finished first; the original is now running in the pool
    Race(R),
}

/// Pool that runs handed-off futures to completion. Cheap to clone.
#[derive(Debug, Clone)]
pub struct FutureRaceCompletionPool {
    /// Read guards held by in-flight futures; shutdown awaits an exclusive write.
    inner: Arc<AsyncRwLock<()>>,
    /// Diagnostic name used when spawning pool tasks
    name: Arc<String>,
}

impl FutureRaceCompletionPool {
    /// Creates an empty pool; `name` labels spawned tasks for diagnostics.
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            inner: Arc::new(AsyncRwLock::new(())),
            name: Arc::new(name.into()),
        }
    }

    /// Run `future` to completion in the background; result is dropped.
    pub fn spawn<F>(&self, future: F)
    where
        F: Future<Output = ()> + Send + 'static,
    {
        let inner = self.inner.clone();
        let name = self.name.clone();
        spawn_detached(name.as_str(), async move {
            let read_guard = inner.read_arc().await;
            future.await;
            drop(read_guard);
        });
    }

    /// Run `future` to completion in the background and return a receiver
    /// that yields its result. Dropping the receiver does not cancel the
    /// future; the value is just discarded when it eventually arrives.
    pub fn channel_spawn<F, T>(&self, future: F) -> flume::Receiver<T>
    where
        F: Future<Output = T> + Send + 'static,
        T: Send + 'static,
    {
        let (tx, rx) = flume::bounded(1);
        let inner = self.inner.clone();
        let name = self.name.clone();
        spawn_detached(name.as_str(), async move {
            let read_guard = inner.read_arc().await;
            let result = future.await;
            let _ = tx.send(result);
            drop(read_guard);
        });
        rx
    }

    /// Wait for every in-flight future to complete. Acquiring the write lock
    /// implies all read guards have been released.
    pub async fn shutdown(&self) {
        let _w = self.inner.write_arc().await;
    }
}

/// Trait that adds `race_complete_on` to any future
pub trait FutureRaceCompletionPoolExt: Future + Sized {
    /// Race `self` against `race_fut`. If `race_fut` finishes first, hand `self`
    /// off to `pool` to run to completion in the background and return `Race(_)`.
    /// Otherwise return `Original(_)` with the value `self` produced.
    fn race_complete_on<R, F>(
        self,
        pool: FutureRaceCompletionPool,
        race_fut: F,
    ) -> impl Future<Output = FutureRaceCompletion<Self::Output, R>> + Send
    where
        Self: Send + 'static,
        Self::Output: Send + 'static,
        F: Future<Output = R> + Send,
        R: Send;
}

impl<Fut> FutureRaceCompletionPoolExt for Fut
where
    Fut: Future,
{
    // async fn would drop the explicit `+ Send` bound from the trait signature
    #[expect(clippy::manual_async_fn)]
    fn race_complete_on<R, F>(
        self,
        pool: FutureRaceCompletionPool,
        race_fut: F,
    ) -> impl Future<Output = FutureRaceCompletion<Fut::Output, R>> + Send
    where
        Self: Send + 'static,
        Self::Output: Send + 'static,
        F: Future<Output = R> + Send,
        R: Send,
    {
        async move {
            let pinned_self: Pin<Box<Fut>> = Box::pin(self);
            let pinned_race = Box::pin(race_fut);
            match select(pinned_self, pinned_race).await {
                Either::Left((output, _race)) => FutureRaceCompletion::Original(output),
                Either::Right((race_output, original)) => {
                    pool.spawn(async move {
                        let _ = original.await;
                    });
                    FutureRaceCompletion::Race(race_output)
                }
            }
        }
    }
}
