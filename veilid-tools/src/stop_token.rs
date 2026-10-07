use super::*;

use event_listener::{Event, EventListener};

struct Inner {
    stopped: AtomicBool,
    event: Event,
}

/// Cancellation source; dropping it stops every associated [`StopToken`].
#[must_use]
pub struct StopSource {
    inner: Arc<Inner>,
}

impl StopSource {
    /// Create a new stop source.
    pub fn new() -> Self {
        Self {
            inner: Arc::new(Inner {
                stopped: AtomicBool::new(false),
                event: Event::new(),
            }),
        }
    }

    /// A token that resolves when this source is dropped.
    pub fn token(&self) -> StopToken {
        StopToken {
            inner: self.inner.clone(),
            listener: None,
        }
    }
}

impl Default for StopSource {
    fn default() -> Self {
        Self::new()
    }
}

impl Drop for StopSource {
    fn drop(&mut self) {
        self.inner.stopped.store(true, Ordering::SeqCst);
        self.inner.event.notify(usize::MAX);
    }
}

impl fmt::Debug for StopSource {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "StopSource")
    }
}

/// Future that resolves when its [`StopSource`] is dropped.
#[must_use]
pub struct StopToken {
    inner: Arc<Inner>,
    listener: Option<Pin<Box<EventListener>>>,
}

impl Clone for StopToken {
    fn clone(&self) -> Self {
        Self {
            inner: self.inner.clone(),
            listener: None,
        }
    }
}

impl fmt::Debug for StopToken {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "StopToken")
    }
}

impl Future for StopToken {
    type Output = ();

    fn poll(self: Pin<&mut Self>, cx: &mut task::Context<'_>) -> task::Poll<Self::Output> {
        let this = self.get_mut();
        loop {
            if this.inner.stopped.load(Ordering::SeqCst) {
                this.listener = None;
                return task::Poll::Ready(());
            }
            match &mut this.listener {
                Some(listener) => match listener.as_mut().poll(cx) {
                    task::Poll::Ready(()) => {
                        this.listener = None;
                    }
                    task::Poll::Pending => return task::Poll::Pending,
                },
                None => {
                    this.listener = Some(Box::pin(this.inner.event.listen()));
                }
            }
        }
    }
}

/// Error returned by [`future::FutureExt::timeout_at`] when the deadline fires first.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub struct TimedOutError;

impl fmt::Display for TimedOutError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "timed out")
    }
}

impl std::error::Error for TimedOutError {}

/// Future combinators bounded by a deadline future.
pub mod future {
    use super::*;

    pin_project_lite::pin_project! {
        /// Future that resolves to `Err(TimedOutError)` if the deadline fires before the inner future.
        #[must_use]
        pub struct TimeoutAt<F, D> {
            #[pin]
            future: F,
            #[pin]
            deadline: D,
        }
    }

    impl<F: Future, D: Future> Future for TimeoutAt<F, D> {
        type Output = Result<F::Output, TimedOutError>;

        fn poll(self: Pin<&mut Self>, cx: &mut task::Context<'_>) -> task::Poll<Self::Output> {
            let this = self.project();
            if let task::Poll::Ready(out) = this.future.poll(cx) {
                return task::Poll::Ready(Ok(out));
            }
            match this.deadline.poll(cx) {
                task::Poll::Ready(_) => task::Poll::Ready(Err(TimedOutError)),
                task::Poll::Pending => task::Poll::Pending,
            }
        }
    }

    /// Deadline-bounding extension for futures.
    pub trait FutureExt: Future + Sized {
        /// Run this future until `deadline` resolves, then return `Err(TimedOutError)`.
        fn timeout_at<D: Future>(self, deadline: D) -> TimeoutAt<Self, D> {
            TimeoutAt {
                future: self,
                deadline,
            }
        }
    }

    impl<F: Future + Sized> FutureExt for F {}
}

/// Common imports for stop token users.
pub mod prelude {
    #[doc(no_inline)]
    pub use super::future::FutureExt as _;
    #[doc(no_inline)]
    pub use super::{StopSource, StopToken, TimedOutError};
}
