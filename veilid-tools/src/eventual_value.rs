use super::*;

use eventual_base::*;

/// A one-shot signal carrying a single owned value. Instance futures complete
/// when `resolve` is called; exactly one consumer may `take_value` the resolved
/// value out.
pub struct EventualValue<T: Unpin> {
    inner: Arc<Mutex<EventualBaseInner<T>>>,
}

impl<T: Unpin> core::fmt::Debug for EventualValue<T> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("EventualValue").finish()
    }
}

impl<T: Unpin> Clone for EventualValue<T> {
    fn clone(&self) -> Self {
        Self {
            inner: self.inner.clone(),
        }
    }
}

impl<T: Unpin> EventualBase for EventualValue<T> {
    type ResolvedType = T;
    fn base_inner(&self) -> MutexGuard<'_, EventualBaseInner<Self::ResolvedType>> {
        self.inner.lock()
    }
}

impl<T: Unpin> Default for EventualValue<T> {
    fn default() -> Self {
        Self::new()
    }
}

impl<T: Unpin> EventualValue<T> {
    /// Create an unresolved `EventualValue`.
    #[must_use]
    pub fn new() -> Self {
        Self {
            inner: Arc::new(Mutex::new(EventualBaseInner::new())),
        }
    }

    /// Attach an instance future that completes when the value is resolved,
    /// yielding a handle from which the value can be taken.
    #[must_use]
    pub fn instance(&self) -> EventualValueFuture<T> {
        EventualValueFuture {
            id: None,
            eventual: self.clone(),
        }
    }

    /// Resolve to `value` and wake every attached instance future. The returned
    /// future completes once all of them have run.
    ///
    /// A second `resolve` after resolution keeps the first value and drops the
    /// new one.
    pub fn resolve(&self, value: T) -> EventualResolvedFuture<Self> {
        self.resolve_to_value(value)
    }

    /// Take the resolved value, leaving the `Eventual` empty. Returns `None` if
    /// unresolved or already taken. Does not block.
    #[must_use]
    pub fn take_value(&self) -> Option<T> {
        let mut inner = self.inner.lock();
        inner.resolved_value_mut().take()
    }
}

/// Instance future from `EventualValue::instance`; resolves to a clone of the
/// owning `EventualValue` so the value can be taken from it.
///
/// Stays `Pending` until the owning `EventualValue` is resolved.
pub struct EventualValueFuture<T: Unpin> {
    id: Option<usize>,
    eventual: EventualValue<T>,
}

impl<T: Unpin> core::fmt::Debug for EventualValueFuture<T> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("EventualValueFuture")
            .field("id", &self.id)
            .finish()
    }
}

impl<T: Unpin> Future for EventualValueFuture<T> {
    type Output = EventualValue<T>;
    fn poll(mut self: Pin<&mut Self>, cx: &mut task::Context<'_>) -> task::Poll<Self::Output> {
        let this = &mut *self;
        let out = {
            let mut inner = this.eventual.base_inner();
            inner.instance_poll(&mut this.id, cx)
        };
        match out {
            None => task::Poll::<Self::Output>::Pending,
            Some(wakers) => {
                // Wake all other instance futures
                for w in wakers {
                    w.wake();
                }
                task::Poll::<Self::Output>::Ready(this.eventual.clone())
            }
        }
    }
}

impl<T> Drop for EventualValueFuture<T>
where
    T: Unpin,
{
    fn drop(&mut self) {
        if let Some(id) = self.id.take() {
            let wakers = {
                let mut inner = self.eventual.base_inner();
                inner.remove_waker(id)
            };
            for w in wakers {
                w.wake();
            }
        }
    }
}
