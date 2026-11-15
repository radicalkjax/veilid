//! Where the host application stands in its platform's lifecycle.
//!
//! A mobile platform freezes a process that is not in front of the user, and may terminate
//! it later without any further notice. State held only in memory has to be written out
//! when the platform says the process is going away, because nothing says so afterwards.
//!
//! iOS suspends a backgrounded app within seconds unless it asks for background time, so there
//! the time is claimed the moment UIKit reports the move, and held by a [BackgroundTimeGuard]
//! until the state is written.
//!
//! A desktop platform throttles a hidden app rather than freezing it (macOS App Nap, Windows
//! power throttling). A [ForegroundGuard] opts the process out; without one, macOS reports a
//! hidden app as in the background.
//!
//! A browser freezes or unloads a hidden page, and stops an idle service worker, without asking;
//! a service worker's background time keeps it running through a save.
//!
//! Platforms that report changes as events call each monitor's callback at once, so a page can
//! write its state before it is frozen rather than on the next poll.

use super::*;

#[cfg(target_os = "android")]
mod android;
#[cfg(target_os = "ios")]
mod ios;
#[cfg(target_os = "macos")]
mod macos;
#[cfg(all(target_arch = "wasm32", target_os = "unknown"))]
mod web;
#[cfg(target_os = "windows")]
mod windows;

/// Runs a callback body handed to a foreign runtime, aborting instead of unwinding into it
#[cfg(any(target_os = "ios", target_os = "macos"))]
fn abort_on_panic(f: impl FnOnce()) {
    if std::panic::catch_unwind(std::panic::AssertUnwindSafe(f)).is_err() {
        std::process::abort();
    }
}

/// Where the host application stands, as far as its platform reports
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AppLifecycleState {
    /// In front of the user, or doing work the platform keeps the process awake for
    Active,
    /// Put away: the platform may freeze or terminate the process without further notice
    Background,
}

/// Whether the platform freezes a process in the background; a desktop platform only throttles it
#[must_use]
pub const fn background_may_freeze() -> bool {
    cfg!(any(
        target_os = "android",
        target_os = "ios",
        all(target_arch = "wasm32", target_os = "unknown")
    ))
}

type ChangeCallback = Arc<dyn Fn() + Send + Sync>;

/// Every live monitor's change callback, by monitor id
static CHANGE_CALLBACKS: Mutex<Vec<(usize, ChangeCallback)>> = Mutex::new(Vec::new());
static NEXT_MONITOR_ID: AtomicUsize = AtomicUsize::new(0);

/// Tells every monitor's holder that the platform reported a change
#[cfg_attr(
    not(any(
        target_os = "ios",
        target_os = "macos",
        all(target_arch = "wasm32", target_os = "unknown")
    )),
    allow(dead_code)
)]
fn notify_changed() {
    let callbacks: Vec<ChangeCallback> = CHANGE_CALLBACKS
        .lock()
        .iter()
        .map(|(_, callback)| callback.clone())
        .collect();
    for callback in callbacks {
        callback();
    }
}

/// Follows the host application's lifecycle while held: the platform reports its state, and
/// background time is claimed whenever the application moves to the background
#[derive(Debug)]
pub struct AppLifecycleMonitor {
    /// Identifies this monitor's change callback
    monitor_id: usize,
}

impl AppLifecycleMonitor {
    /// Starts following the platform's lifecycle reports. `on_change` runs whenever the platform
    /// reports a change as it happens, on whatever thread reported it; platforms that are only
    /// polled never call it.
    #[must_use]
    pub fn new(on_change: impl Fn() + Send + Sync + 'static) -> Self {
        let monitor_id = NEXT_MONITOR_ID.fetch_add(1, Ordering::AcqRel);
        CHANGE_CALLBACKS
            .lock()
            .push((monitor_id, Arc::new(on_change)));
        #[cfg(target_os = "ios")]
        ios::monitor_started();
        #[cfg(target_os = "macos")]
        macos::monitor_started();
        #[cfg(all(target_arch = "wasm32", target_os = "unknown"))]
        web::monitor_started();
        Self { monitor_id }
    }

    /// The host application's current state, or None where the platform reports none.
    /// Cheap enough to poll every second from async code.
    #[must_use]
    pub fn state(&self) -> Option<AppLifecycleState> {
        cfg_if! {
            if #[cfg(target_os = "android")] {
                android::app_lifecycle_state()
            } else if #[cfg(target_os = "ios")] {
                ios::app_lifecycle_state()
            } else if #[cfg(target_os = "macos")] {
                macos::app_lifecycle_state()
            } else if #[cfg(all(target_arch = "wasm32", target_os = "unknown"))] {
                web::app_lifecycle_state()
            } else {
                None
            }
        }
    }

    /// The background time claimed when the application last moved to the background, for
    /// whatever has to be written out before the platform suspends the process
    #[must_use]
    pub fn take_background_entry_time(&self) -> Option<BackgroundTimeGuard> {
        cfg_if! {
            if #[cfg(target_os = "ios")] {
                ios::take_entry_time().map(|task| BackgroundTimeGuard { _task: task })
            } else if #[cfg(all(target_arch = "wasm32", target_os = "unknown"))] {
                web::take_entry_time().map(|time| BackgroundTimeGuard { _time: time })
            } else {
                None
            }
        }
    }
}

impl Drop for AppLifecycleMonitor {
    fn drop(&mut self) {
        CHANGE_CALLBACKS
            .lock()
            .retain(|(monitor_id, _)| *monitor_id != self.monitor_id);
        #[cfg(target_os = "ios")]
        ios::monitor_stopped();
    }
}

/// Keeps the process running in the background until dropped, on a platform that grants
/// background time
#[derive(Debug)]
pub struct BackgroundTimeGuard {
    #[cfg(target_os = "ios")]
    _task: ios::BackgroundTask,
    #[cfg(all(target_arch = "wasm32", target_os = "unknown"))]
    _time: web::BackgroundTime,
}

impl BackgroundTimeGuard {
    /// None where the platform has no background time to grant
    #[must_use]
    pub fn begin(name: &'static str) -> Option<Self> {
        cfg_if! {
            if #[cfg(target_os = "ios")] {
                Some(Self { _task: ios::BackgroundTask::begin(name) })
            } else if #[cfg(all(target_arch = "wasm32", target_os = "unknown"))] {
                let _ = name;
                web::BackgroundTime::begin().map(|time| Self { _time: time })
            } else {
                let _ = name;
                None
            }
        }
    }
}

/// Keeps a hidden desktop app from being throttled while held, on a platform that throttles one
#[derive(Debug)]
pub struct ForegroundGuard {
    _private: (),
}

impl ForegroundGuard {
    /// None where the platform throttles nothing it can be asked not to
    #[must_use]
    pub fn begin(reason: &'static str) -> Option<Self> {
        cfg_if! {
            if #[cfg(target_os = "macos")] {
                macos::begin_foreground(reason);
                Some(Self { _private: () })
            } else if #[cfg(target_os = "windows")] {
                let _ = reason;
                windows::begin_foreground();
                Some(Self { _private: () })
            } else {
                let _ = reason;
                None
            }
        }
    }
}

impl Drop for ForegroundGuard {
    fn drop(&mut self) {
        #[cfg(target_os = "macos")]
        macos::end_foreground();
        #[cfg(target_os = "windows")]
        windows::end_foreground();
    }
}
