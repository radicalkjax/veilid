use super::*;

use core::task::{Context, Poll};

/// Wrapper around a task join handle that panics if dropped before the task is joined, detached, or aborted.
#[derive(Debug)]
pub struct MustJoinHandle<T> {
    join_handle: Option<LowLevelJoinHandle<T>>,
    completed: bool,
}

impl<T> MustJoinHandle<T> {
    /// Wrap a low-level join handle so it must be completed before drop.
    /// The caller must resolve it via `.await`, `detach`, or `abort` before drop, or the `Drop` impl panics.
    #[must_use]
    pub fn new(join_handle: LowLevelJoinHandle<T>) -> Self {
        Self {
            join_handle: Some(join_handle),
            completed: false,
        }
    }

    /// Let the task keep running in the background without joining it.
    /// Consumes the handle and satisfies the must-complete contract; non-blocking, does not await the task.
    pub fn detach(mut self) {
        cfg_if! {
            if #[cfg(feature="rt-async-std")] {
                self.join_handle = None;
            } else if #[cfg(feature="rt-tokio")] {
                self.join_handle = None;
            } else if #[cfg(all(target_arch = "wasm32", target_os = "unknown"))] {
                if let Some(jh) = self.join_handle.take() {
                    jh.detach();
                }
            } else {
                compile_error!("needs executor implementation");
            }
        }
        self.completed = true;
    }

    #[allow(unused_mut)]
    #[cfg_attr(
        all(target_arch = "wasm32", target_os = "unknown"),
        expect(clippy::unused_async)
    )]
    /// Cancel the task and wait for it to stop.
    /// Consumes the handle and satisfies the must-complete contract; awaits task teardown except on wasm, where it just drops.
    pub async fn abort(mut self) {
        if !self.completed {
            cfg_if! {
                if #[cfg(feature="rt-async-std")] {
                    if let Some(jh) = self.join_handle.take() {
                        jh.cancel().await;
                        self.completed = true;
                    }
                } else if #[cfg(feature="rt-tokio")] {
                    if let Some(jh) = self.join_handle.take() {
                        jh.abort();
                        let _ = jh.await;
                        self.completed = true;
                    }
                } else if #[cfg(all(target_arch = "wasm32", target_os = "unknown"))] {
                    drop(self.join_handle.take());
                    self.completed = true;
                } else {
                    compile_error!("needs executor implementation");
                }

            }
        }
    }
}

impl<T> Drop for MustJoinHandle<T> {
    fn drop(&mut self) {
        // panic if we haven't completed
        if !self.completed {
            #[cfg(debug_assertions)]
            panic!("MustJoinHandle was not completed upon drop. Add cooperative cancellation where appropriate to ensure this is completed before drop.");
            #[cfg(not(debug_assertions))]
            error!("MustJoinHandle was not completed upon drop. Add cooperative cancellation where appropriate to ensure this is completed before drop.");
        }
    }
}

impl<T: 'static> Future for MustJoinHandle<T> {
    type Output = T;

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        match Pin::new(self.join_handle.as_mut().unwrap_or_log()).poll(cx) {
            Poll::Ready(t) => {
                if self.completed {
                    #[cfg(debug_assertions)]
                    panic!("should not poll completed join handle");
                    #[cfg(not(debug_assertions))]
                    error!("should not poll completed join handle");
                }
                self.completed = true;
                cfg_if! {
                    if #[cfg(feature="rt-async-std")] {
                        Poll::Ready(t)
                    } else if #[cfg(feature="rt-tokio")] {
                        match t {
                            Ok(t) => Poll::Ready(t),
                            Err(e) => {
                                if e.is_panic() {
                                    // Resume the panic on the main task
                                    std::panic::resume_unwind(e.into_panic());
                                } else {
                                    panic!("join error was not a panic, should not poll after abort");
                                }
                            }
                        }
                    } else if #[cfg(all(target_arch = "wasm32", target_os = "unknown"))] {
                        Poll::Ready(t)
                    } else {
                        compile_error!("needs executor implementation");
                    }
                }
            }
            Poll::Pending => Poll::Pending,
        }
    }
}
