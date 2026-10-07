use super::*;

use core::sync::atomic::{AtomicU64, Ordering};
use std::sync::OnceLock;

type TickTaskRoutine<E> =
    dyn Fn(StopToken, u64, u64) -> PinBoxFutureStatic<Result<(), E>> + Send + Sync + 'static;

/// Runs a single-future background processing task, attempting to run it once every 'tick period' microseconds.
/// If the prior tick is still running, it will allow it to finish, and do another tick when the timer comes around again.
/// One should attempt to make tasks short-lived things that run in less than the tick period if you want things to happen with regular periodicity.
pub struct TickTask<E: Send + 'static> {
    name: String,
    last_timestamp_us: AtomicU64,
    tick_period_us: u64,
    routine: OnceLock<Box<TickTaskRoutine<E>>>,
    stop_source: AsyncMutex<Option<StopSource>>,
    single_future: MustJoinSingleFuture<Result<(), E>>,
    running: Arc<AtomicBool>,
}

impl<E: Send + fmt::Debug + 'static> TickTask<E> {
    /// Create a tick task with a period given in microseconds.
    #[must_use]
    pub fn new_us(name: &str, tick_period_us: u64) -> Self {
        Self {
            name: name.to_string(),
            last_timestamp_us: AtomicU64::new(0),
            tick_period_us,
            routine: OnceLock::new(),
            stop_source: AsyncMutex::new(None),
            single_future: MustJoinSingleFuture::new(),
            running: Arc::new(AtomicBool::new(false)),
        }
    }
    /// Create a tick task with a period given in milliseconds.
    #[must_use]
    pub fn new_ms(name: &str, tick_period_ms: u32) -> Self {
        Self {
            name: name.to_string(),
            last_timestamp_us: AtomicU64::new(0),
            tick_period_us: (tick_period_ms as u64) * 1000u64,
            routine: OnceLock::new(),
            stop_source: AsyncMutex::new(None),
            single_future: MustJoinSingleFuture::new(),
            running: Arc::new(AtomicBool::new(false)),
        }
    }
    /// Create a tick task with a period given in seconds.
    #[must_use]
    pub fn new(name: &str, tick_period_sec: u32) -> Self {
        Self {
            name: name.to_string(),
            last_timestamp_us: AtomicU64::new(0),
            tick_period_us: (tick_period_sec as u64) * 1000000u64,
            routine: OnceLock::new(),
            stop_source: AsyncMutex::new(None),
            single_future: MustJoinSingleFuture::new(),
            running: Arc::new(AtomicBool::new(false)),
        }
    }

    /// Set the routine to run on each tick. Must be set before the first tick.
    /// Write-once: a second call after the routine is set is ignored (logged).
    pub fn set_routine(
        &self,
        routine: impl Fn(StopToken, u64, u64) -> PinBoxFutureStatic<Result<(), E>>
            + Send
            + Sync
            + 'static,
    ) {
        self.routine
            .set(Box::new(routine))
            .map_err(drop)
            .unwrap_or_log();
    }

    /// Returns true while the routine for a tick is executing.
    pub fn is_running(&self) -> bool {
        self.running.load(core::sync::atomic::Ordering::Acquire)
    }

    /// The timestamp of the last tick that ran, or None if it has never ticked.
    pub fn last_timestamp_us(&self) -> Option<u64> {
        let ts = self
            .last_timestamp_us
            .load(core::sync::atomic::Ordering::Acquire);
        if ts == 0 {
            None
        } else {
            Some(ts)
        }
    }

    /// Request a stop and wait for the running tick to finish, propagating its error if any.
    /// Idempotent: returns immediately if already stopped. Blocks until the in-flight tick completes.
    pub async fn stop(&self) -> Result<(), E> {
        // drop the stop source if we have one
        {
            let mut stop_source_guard = self.stop_source.lock().await;
            if stop_source_guard.is_none() {
                // already stopped, just return
                return Ok(());
            }
            drop(stop_source_guard.take());
        }

        // wait for completion of the tick task
        match pin_future!(self.single_future.join()).await {
            Ok(Some(Err(err))) => Err(err),
            _ => Ok(()),
        }
    }

    /// Run the routine if at least the tick period has elapsed since the last run, otherwise do nothing.
    /// Returns immediately before the period elapses; if a prior run is still in flight, leaves it
    /// running and starts no second run (does not wait for it).
    pub async fn tick(&self) -> Result<(), E> {
        let now = get_raw_timestamp();
        let last_timestamp_us = self.last_timestamp_us.load(Ordering::Acquire);

        if last_timestamp_us != 0u64 && now.saturating_sub(last_timestamp_us) < self.tick_period_us
        {
            // It's not time yet
            return Ok(());
        }

        let itick = self.internal_tick(now, last_timestamp_us);

        itick.await.map(drop)
    }

    /// Run the routine now regardless of the tick period. Returns true if a new run was started.
    /// Does not wait for an already-running tick: returns false if one is in flight.
    pub async fn try_tick_now(&self) -> Result<bool, E> {
        let now = get_raw_timestamp();
        let last_timestamp_us = self.last_timestamp_us.load(Ordering::Acquire);

        let itick = self.internal_tick(now, last_timestamp_us);

        itick.await
    }

    async fn internal_tick(&self, now: u64, last_timestamp_us: u64) -> Result<bool, E> {
        // Lock the stop source, tells us if we have ever started this future
        let mut stop_source_guard = self.stop_source.lock().await;

        // Run the singlefuture
        let stop_source = StopSource::new();
        let stop_token = stop_source.token();
        let make_singlefuture_closure = || {
            let running = self.running.clone();
            let routine = self.routine.get().unwrap_or_log()(stop_token, last_timestamp_us, now);

            Box::pin(async move {
                running.store(true, core::sync::atomic::Ordering::Release);
                let out = routine.await;
                running.store(false, core::sync::atomic::Ordering::Release);
                out
            })
        };

        match self
            .single_future
            .single_spawn(&self.name, make_singlefuture_closure)
            .await
        {
            // A new singlefuture ran
            Ok((res, ran)) => {
                // If the previous run finished and we started a new one, switch the stopper
                if ran {
                    // Set new timer
                    self.last_timestamp_us.store(now, Ordering::Release);
                    // Save new stopper
                    *stop_source_guard = Some(stop_source);
                }

                match res {
                    Some(Ok(())) => {
                        // Prior run returned successfully
                        Ok(ran)
                    }
                    Some(Err(e)) => {
                        // Prior run returned an error, propagate it
                        Err(e)
                    }
                    None => {
                        // No prior run or nothing completed
                        Ok(ran)
                    }
                }
            }
            Err(()) => {
                // If we get this, it's because we are joining the singlefuture already
                // Don't bother running but this is not an error in this case
                Ok(false)
            }
        }
    }
}
