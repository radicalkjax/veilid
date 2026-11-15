//! A wakeup signal for one waiter. Wakes coalesce, and a wake sent while nobody waits is kept for
//! the next wait, so a state change between a waiter's check and its wait is never lost.

use super::*;

use futures_util::future::{select, Either};
use stop_token::future::FutureExt as _;

/// Wakes the paired [WakeReceiver]
#[derive(Clone, Debug)]
pub struct WakeSender {
    sender: flume::Sender<()>,
}

impl WakeSender {
    /// Wake the waiter, now or at its next wait
    pub fn wake(&self) {
        // A wake already pending covers this one
        let _ = self.sender.try_send(());
    }
}

/// How a [WakeReceiver::wait] ended
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WakeOutcome {
    /// Woken by the sender
    Woken,
    /// The deadline passed first
    Deadline,
    /// The stop token fired, or every sender is gone
    Stopped,
}

/// Waits for its [WakeSender]; clones share the wakes, so only one should wait at a time
#[derive(Clone, Debug)]
pub struct WakeReceiver {
    receiver: flume::Receiver<()>,
}

impl WakeReceiver {
    /// Wait until woken, until raw timestamp `opt_deadline_us` passes, or until `stop_token` fires
    pub async fn wait(&self, opt_deadline_us: Option<u64>, stop_token: StopToken) -> WakeOutcome {
        let received = match opt_deadline_us {
            Some(deadline_us) => {
                let wait_us = deadline_us.saturating_sub(get_raw_timestamp());
                if wait_us == 0 {
                    return WakeOutcome::Deadline;
                }
                let wait_ms = u32::try_from(wait_us.div_ceil(1000)).unwrap_or(u32::MAX);
                match select(Box::pin(sleep(wait_ms)), self.receiver.recv_async())
                    .timeout_at(stop_token)
                    .await
                {
                    Ok(Either::Left(_)) => return WakeOutcome::Deadline,
                    Ok(Either::Right((res, _))) => res.is_ok(),
                    Err(_) => false,
                }
            }
            None => matches!(
                self.receiver.recv_async().timeout_at(stop_token).await,
                Ok(Ok(()))
            ),
        };
        if received {
            WakeOutcome::Woken
        } else {
            WakeOutcome::Stopped
        }
    }
}

/// A connected [WakeSender] and [WakeReceiver]
#[must_use]
pub fn wakeup() -> (WakeSender, WakeReceiver) {
    let (sender, receiver) = flume::bounded(1);
    (WakeSender { sender }, WakeReceiver { receiver })
}
