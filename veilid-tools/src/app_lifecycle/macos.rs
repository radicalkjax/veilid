//! macOS naps an application none of whose windows are visible: its timers coalesce, its
//! priority drops and its I/O is throttled until a window shows again. A node that prefers the
//! foreground holds an activity that exempts the process; otherwise AppKit's occlusion reports
//! stand for the background, as a mobile platform's lifecycle does.
//!
//! No state is read up front: asking AppKit for the application creates one in a process that
//! has none, so the state stays unknown until AppKit reports a change.

use super::*;
use block2::RcBlock;
use objc2::rc::Retained;
use objc2::runtime::ProtocolObject;
use objc2::MainThreadMarker;
use objc2_app_kit::{
    NSApplication, NSApplicationDidChangeOcclusionStateNotification,
    NSApplicationDidHideNotification, NSApplicationDidUnhideNotification,
    NSApplicationOcclusionState,
};
use objc2_foundation::{
    NSActivityOptions, NSNotification, NSNotificationCenter, NSObjectProtocol, NSProcessInfo,
    NSString,
};
use std::ptr::NonNull;
use std::sync::atomic::AtomicU8;
use std::sync::Once;

const STATE_UNKNOWN: u8 = 0;
const STATE_ACTIVE: u8 = 1;
const STATE_BACKGROUND: u8 = 2;

/// Whether a window of the application is visible, as AppKit last reported it
static STATE: AtomicU8 = AtomicU8::new(STATE_UNKNOWN);
static INSTALL_OBSERVER: Once = Once::new();
/// Guards that want the process kept out of App Nap, and the activity that does it
static FOREGROUND: Mutex<(usize, Option<ActivityToken>)> = Mutex::new((0, None));

/// An activity from NSProcessInfo, ended by handing it back
struct ActivityToken(Retained<ProtocolObject<dyn NSObjectProtocol>>);

// SAFETY: the token is only handed back to NSProcessInfo, which objc2 declares Send and Sync,
// or released, and an objc2 Retained release is thread-safe
unsafe impl Send for ActivityToken {}

pub(super) fn monitor_started() {
    INSTALL_OBSERVER.call_once(install_observer);
}

pub(super) fn app_lifecycle_state() -> Option<AppLifecycleState> {
    let state = match STATE.load(Ordering::Acquire) {
        STATE_ACTIVE => AppLifecycleState::Active,
        STATE_BACKGROUND => AppLifecycleState::Background,
        _ => return None,
    };
    // A hidden process holding the activity runs at full speed
    if FOREGROUND.lock().0 > 0 {
        return Some(AppLifecycleState::Active);
    }
    Some(state)
}

pub(super) fn begin_foreground(reason: &'static str) {
    let mut foreground = FOREGROUND.lock();
    foreground.0 += 1;
    if foreground.1.is_none() {
        let token = NSProcessInfo::processInfo().beginActivityWithOptions_reason(
            NSActivityOptions::UserInitiatedAllowingIdleSystemSleep,
            &NSString::from_str(reason),
        );
        foreground.1 = Some(ActivityToken(token));
    }
}

pub(super) fn end_foreground() {
    let mut foreground = FOREGROUND.lock();
    foreground.0 = foreground.0.saturating_sub(1);
    if foreground.0 > 0 {
        return;
    }
    if let Some(ActivityToken(token)) = foreground.1.take() {
        // SAFETY: the token came from beginActivity on this process's NSProcessInfo, and is
        // ended once
        unsafe { NSProcessInfo::processInfo().endActivity(&token) };
    }
}

fn install_observer() {
    let center = NSNotificationCenter::defaultCenter();
    let changed = RcBlock::new(|_: NonNull<NSNotification>| abort_on_panic(visibility_changed));
    // The center holds each registration until it is removed, and these never are
    // SAFETY: the names are AppKit constants, valid for the life of the process; the block
    // captures nothing, so it is sendable; with no queue given it runs on the posting thread,
    // which for AppKit's notifications is the main thread
    unsafe {
        for name in [
            NSApplicationDidChangeOcclusionStateNotification,
            NSApplicationDidHideNotification,
            NSApplicationDidUnhideNotification,
        ] {
            let _ =
                center.addObserverForName_object_queue_usingBlock(Some(name), None, None, &changed);
        }
    }
}

fn visibility_changed() {
    // AppKit posts these on the main thread, from an application that exists
    let Some(mtm) = MainThreadMarker::new() else {
        return;
    };
    let app = NSApplication::sharedApplication(mtm);
    // Hiding the application reports no occlusion change, only the hiding
    let visible = !app.isHidden()
        && app
            .occlusionState()
            .contains(NSApplicationOcclusionState::Visible);
    let state = if visible {
        STATE_ACTIVE
    } else {
        STATE_BACKGROUND
    };
    if STATE.swap(state, Ordering::AcqRel) != state {
        notify_changed();
    }
}
