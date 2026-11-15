use super::*;

/// A heap-pinned box.
pub type PinBox<T> = Pin<Box<T>>;
/// A heap-pinned, `Send`, boxed trait-object future borrowing for `'a`.
pub type PinBoxFuture<'a, T> = PinBox<dyn Future<Output = T> + Send + 'a>;
/// A [`PinBoxFuture`] with a `'static` lifetime.
pub type PinBoxFutureStatic<T> = PinBoxFuture<'static, T>;

/// Pin a future to the heap, returning a concrete boxed future.
///
/// Moves the future to the heap from the caller's stack. May allocate the future on the
/// stack first and then move it.
#[macro_export]
macro_rules! pin_future {
    ($call: expr) => {
        Box::pin($call)
    };
}

/// Pin a future to the heap inside a closure, returning a concrete boxed future.
///
/// Keeps the future off the calling function's stack completely. The closure is still on
/// the stack, but smaller than the future would be.
#[macro_export]
macro_rules! pin_future_closure {
    ($call: expr) => {
        (|| Box::pin($call))()
    };
}

/// Pin a future to the heap, returning a [`PinBoxFuture`] trait object.
///
/// Moves the future to the heap from the caller's stack. May allocate the future on the
/// stack first and then move it.
#[macro_export]
macro_rules! pin_dyn_future {
    ($call: expr) => {
        Box::pin($call) as PinBoxFuture<_>
    };
}

/// Pin a future to the heap inside a closure, returning a [`PinBoxFuture`] trait object.
///
/// Keeps the future off the calling function's stack completely. The closure is still on
/// the stack, but smaller than the future would be.
#[macro_export]
macro_rules! pin_dyn_future_closure {
    ($call: expr) => {
        (|| Box::pin($call) as PinBoxFuture<_>)()
    };
}
