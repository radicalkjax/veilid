//! Runs a routine when the earliest armed deadline passes.

use super::*;

type DeadlineTimerRoutine =
    dyn Fn(u64, StopToken) -> PinBoxFutureStatic<Option<u64>> + Send + Sync + 'static;

/// Runs a routine when the earliest armed deadline passes.
///
/// The owner arms the timer with each new deadline after recording it; an arm later than the one
/// already armed costs one atomic operation. The routine retires the deadlines that have passed and
/// returns the earliest one left, so the timer never needs to see a deadline that only moved later.
pub struct DeadlineTimer {
    name: String,
    armer: DeadlineArmer,
    wake_receiver: WakeReceiver,
    running: Mutex<Option<DeadlineTimerRunning>>,
}

struct DeadlineTimerRunning {
    stop_source: StopSource,
    join_handle: MustJoinHandle<()>,
}

/// Arms a [DeadlineTimer]; every clone arms the same timer
#[derive(Clone, Debug)]
pub struct DeadlineArmer {
    /// The earliest armed deadline as a raw timestamp, u64::MAX for none
    deadline_us: Arc<AtomicU64>,
    /// Makes the timer read the deadline again
    wake_sender: WakeSender,
}

impl DeadlineArmer {
    /// Run the routine once the raw timestamp `deadline_us` passes, unless an earlier deadline is
    /// armed. 0 runs it at once.
    pub fn arm(&self, deadline_us: u64) {
        if deadline_us < self.deadline_us.fetch_min(deadline_us, Ordering::AcqRel) {
            self.wake_sender.wake();
        }
    }
}

impl fmt::Debug for DeadlineTimer {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("DeadlineTimer")
            .field("name", &self.name)
            .field("running", &self.running.lock().is_some())
            .finish()
    }
}

impl DeadlineTimer {
    /// A stopped timer; its task gets `name`
    #[must_use]
    pub fn new(name: &str) -> Self {
        let (wake_sender, wake_receiver) = wakeup();
        Self {
            name: name.to_string(),
            armer: DeadlineArmer {
                deadline_us: Arc::new(AtomicU64::new(u64::MAX)),
                wake_sender,
            },
            wake_receiver,
            running: Mutex::new(None),
        }
    }

    /// Start the timer: `routine` runs with the current time once the armed deadline passes, and
    /// returns the earliest deadline it left pending. Restarts a timer that is already running.
    pub fn start(
        &self,
        routine: impl Fn(u64, StopToken) -> PinBoxFutureStatic<Option<u64>> + Send + Sync + 'static,
    ) {
        let routine: Box<DeadlineTimerRoutine> = Box::new(routine);
        let deadline_us = self.armer.deadline_us.clone();
        let wake_receiver = self.wake_receiver.clone();
        let stop_source = StopSource::new();
        let stop_token = stop_source.token();
        let join_handle = spawn(&self.name, async move {
            loop {
                let armed_us = deadline_us.load(Ordering::Acquire);
                match wake_receiver
                    .wait(
                        (armed_us != u64::MAX).then_some(armed_us),
                        stop_token.clone(),
                    )
                    .await
                {
                    WakeOutcome::Woken => {}
                    WakeOutcome::Deadline => {
                        // An arm from here on is kept; one before is in the routine's scan
                        deadline_us.store(u64::MAX, Ordering::Release);
                        if let Some(next_us) =
                            routine(get_raw_timestamp(), stop_token.clone()).await
                        {
                            deadline_us.fetch_min(next_us, Ordering::AcqRel);
                        }
                    }
                    WakeOutcome::Stopped => return,
                }
            }
        });
        let previous = self.running.lock().replace(DeadlineTimerRunning {
            stop_source,
            join_handle,
        });
        if let Some(previous) = previous {
            drop(previous.stop_source);
            previous.join_handle.detach();
        }
    }

    /// Run the routine once the raw timestamp `deadline_us` passes, unless an earlier deadline is
    /// armed. 0 runs it at once.
    pub fn arm(&self, deadline_us: u64) {
        self.armer.arm(deadline_us);
    }

    /// An armer for an owner that records deadlines away from the timer
    pub fn armer(&self) -> DeadlineArmer {
        self.armer.clone()
    }

    /// Stop the timer and wait for a routine in flight to return. Arms made while stopped are kept
    /// for the next start.
    pub async fn stop(&self) {
        let Some(running) = self.running.lock().take() else {
            return;
        };
        drop(running.stop_source);
        running.join_handle.await;
    }
}
