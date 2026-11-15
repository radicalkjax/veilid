//! UIKit posts the application's moves between foreground and background on the main thread.
//! Observers installed once per process keep the latest state for the tick to poll, and claim
//! background time the moment the application moves to the background, before the
//! notification handler returns.
//!
//! objc2 types `UIApplication` as main-thread-only, so background time is begun and ended on
//! the main queue: at once when already on the main thread, queued otherwise. The main queue
//! is serial, so an end queued after its begin runs after it.

use super::*;
use block2::RcBlock;
use dispatch2::DispatchQueue;
use objc2::MainThreadMarker;
use objc2_foundation::{NSNotification, NSNotificationCenter, NSString};
use objc2_ui_kit::{
    UIApplication, UIApplicationDidEnterBackgroundNotification, UIApplicationState,
    UIApplicationWillEnterForegroundNotification, UIBackgroundTaskIdentifier,
    UIBackgroundTaskInvalid,
};
use std::ptr::NonNull;
use std::sync::atomic::AtomicU8;
use std::sync::Once;

const STATE_UNKNOWN: u8 = 0;
const STATE_ACTIVE: u8 = 1;
const STATE_BACKGROUND: u8 = 2;

/// Name iOS shows for the time claimed on the move to the background
const ENTRY_TIME_NAME: &str = "veilid background entry";

/// Background time below which no task is begun: iOS has warned of expiry by then, and it
/// terminates a process that keeps beginning tasks past that warning
const MIN_TIME_TO_BEGIN_SECS: f64 = 10.0;

/// The application's state as UIKit last reported it
static STATE: AtomicU8 = AtomicU8::new(STATE_UNKNOWN);
/// Monitors alive; background time is claimed on the move to the background only while one is
static MONITORS: AtomicUsize = AtomicUsize::new(0);
static INSTALL_OBSERVERS: Once = Once::new();
/// Background time claimed on the move to the background, until a monitor's holder takes it
static ENTRY_TIME: Mutex<Option<BackgroundTask>> = Mutex::new(None);

pub(super) fn monitor_started() {
    MONITORS.fetch_add(1, Ordering::AcqRel);
    // Queued even on the main thread: the shared application exists once the main queue runs
    INSTALL_OBSERVERS.call_once(|| {
        DispatchQueue::main().exec_async(|| {
            if let Some(mtm) = MainThreadMarker::new() {
                install_observers(mtm);
            }
        });
    });
}

pub(super) fn monitor_stopped() {
    if MONITORS.fetch_sub(1, Ordering::AcqRel) == 1 {
        let unclaimed = ENTRY_TIME.lock().take();
        drop(unclaimed);
    }
}

pub(super) fn app_lifecycle_state() -> Option<AppLifecycleState> {
    match STATE.load(Ordering::Acquire) {
        STATE_ACTIVE => Some(AppLifecycleState::Active),
        STATE_BACKGROUND => Some(AppLifecycleState::Background),
        _ => None,
    }
}

pub(super) fn take_entry_time() -> Option<BackgroundTask> {
    ENTRY_TIME.lock().take()
}

fn install_observers(mtm: MainThreadMarker) {
    let center = NSNotificationCenter::defaultCenter();
    let entered = RcBlock::new(|_: NonNull<NSNotification>| abort_on_panic(entered_background));
    let entering = RcBlock::new(|_: NonNull<NSNotification>| abort_on_panic(entering_foreground));
    // The center holds each registration until it is removed, and these never are
    // SAFETY: both names are UIKit constants, valid for the life of the process; the blocks
    // capture nothing, so they are sendable; with no queue given they run on the posting
    // thread, which for these notifications is the main thread
    unsafe {
        let _ = center.addObserverForName_object_queue_usingBlock(
            Some(UIApplicationDidEnterBackgroundNotification),
            None,
            None,
            &entered,
        );
        let _ = center.addObserverForName_object_queue_usingBlock(
            Some(UIApplicationWillEnterForegroundNotification),
            None,
            None,
            &entering,
        );
    }
    // An observer that fired before this ran knows better than this first reading
    let state = if UIApplication::sharedApplication(mtm).applicationState()
        == UIApplicationState::Background
    {
        STATE_BACKGROUND
    } else {
        STATE_ACTIVE
    };
    if STATE
        .compare_exchange(STATE_UNKNOWN, state, Ordering::AcqRel, Ordering::Acquire)
        .is_ok()
    {
        notify_changed();
    }
}

fn entered_background() {
    STATE.store(STATE_BACKGROUND, Ordering::Release);
    if MONITORS.load(Ordering::Acquire) > 0 {
        // Claimed before this handler returns: iOS suspends within seconds of it
        let task = BackgroundTask::begin(ENTRY_TIME_NAME);
        let previous = ENTRY_TIME.lock().replace(task);
        drop(previous);
    }
    notify_changed();
}

fn entering_foreground() {
    STATE.store(STATE_ACTIVE, Ordering::Release);
    let unclaimed = ENTRY_TIME.lock().take();
    drop(unclaimed);
    notify_changed();
}

/// Background time from UIKit, ended when dropped or when iOS expires it, whichever is first
#[derive(Debug)]
pub(super) struct BackgroundTask {
    slot: Arc<TaskSlot>,
}

#[derive(Debug)]
struct TaskSlot {
    /// Name iOS shows for this background time
    name: &'static str,
    /// UIKit's identifier while begun, `UIBackgroundTaskInvalid` before and after
    id: AtomicUsize,
    /// Set by whichever of the drop and the expiration ends it first
    ended: AtomicBool,
}

impl BackgroundTask {
    pub(super) fn begin(name: &'static str) -> Self {
        let slot = Arc::new(TaskSlot {
            name,
            id: AtomicUsize::new(invalid_task()),
            ended: AtomicBool::new(false),
        });
        let begun = slot.clone();
        on_main(move |mtm| begun.begin(mtm));
        Self { slot }
    }
}

impl Drop for BackgroundTask {
    fn drop(&mut self) {
        let slot = self.slot.clone();
        on_main(move |mtm| slot.end(mtm, false));
    }
}

impl TaskSlot {
    fn begin(self: Arc<Self>, mtm: MainThreadMarker) {
        // Dropped before its begin reached the main queue
        if self.ended.load(Ordering::Acquire) {
            return;
        }
        let application = UIApplication::sharedApplication(mtm);
        if application.backgroundTimeRemaining() < MIN_TIME_TO_BEGIN_SECS {
            debug!("no background time left to begin {}", self.name);
            return;
        }
        let expiring = self.clone();
        let expiration = RcBlock::new(move || {
            abort_on_panic(|| {
                if let Some(mtm) = MainThreadMarker::new() {
                    expiring.end(mtm, true);
                }
            })
        });
        let id = application.beginBackgroundTaskWithName_expirationHandler(
            Some(&NSString::from_str(self.name)),
            Some(&*expiration),
        );
        self.id.store(id, Ordering::Release);
    }

    fn end(&self, mtm: MainThreadMarker, expired: bool) {
        if self.ended.swap(true, Ordering::AcqRel) {
            return;
        }
        let id = self.id.swap(invalid_task(), Ordering::AcqRel);
        if id == invalid_task() {
            return;
        }
        if expired {
            warn!(
                "background time for {} expired before its work finished",
                self.name
            );
        }
        UIApplication::sharedApplication(mtm).endBackgroundTask(id);
    }
}

fn invalid_task() -> UIBackgroundTaskIdentifier {
    // SAFETY: a UIKit constant, valid for the life of the process
    unsafe { UIBackgroundTaskInvalid }
}

/// Runs `f` on the main thread: at once when already there, queued otherwise
fn on_main(f: impl FnOnce(MainThreadMarker) + Send + 'static) {
    if let Some(mtm) = MainThreadMarker::new() {
        f(mtm);
        return;
    }
    DispatchQueue::main().exec_async(move || {
        // The main queue runs on the main thread
        if let Some(mtm) = MainThreadMarker::new() {
            f(mtm);
        }
    });
}
