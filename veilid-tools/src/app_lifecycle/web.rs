//! A page's lifecycle arrives as events whose support differs by browser: `visibilitychange` and
//! `pagehide`/`pageshow` are everywhere, `freeze`/`resume` only in Chromium. Each is listened for
//! only where the page has it, as Google Chrome Labs' page-lifecycle library does, and a hidden,
//! frozen or departing page is in the background. `unload` and `beforeunload` are never used:
//! either keeps a page out of the back/forward cache.
//!
//! A service worker has no page and no warning before the browser stops it, so it is in the
//! background from the start. Background time keeps it running through a save: `waitUntil` on
//! each event it handles, and in an extension, an extension API call every 20 seconds, since any
//! call restarts the extension's 30 second idle timer.
//!
//! Dedicated and shared workers see no lifecycle; their host reports it instead.

use super::*;
use gloo_events::{EventListener, EventListenerOptions};
use std::sync::atomic::AtomicU8;
use std::sync::Once;
use wasm_bindgen::{JsCast, JsValue};
use web_sys::{Document, Event, ExtendableEvent, ServiceWorkerGlobalScope, Window};

const STATE_UNKNOWN: u8 = 0;
const STATE_ACTIVE: u8 = 1;
const STATE_BACKGROUND: u8 = 2;

/// Interval between extension API calls while an extension's worker holds background time;
/// Chrome ends a service worker idle for 30 s
const EXTENSION_KEEPALIVE_MS: u32 = 20_000;
/// How often an event kept open checks whether background time is still held
const BACKGROUND_TIME_POLL_MS: u32 = 250;

/// Service worker events that come with work to do, whose lifetime `waitUntil` extends.
/// Install and activate are left out: holding them delays the worker taking over.
const SERVICE_WORKER_EVENTS: [&str; 14] = [
    "fetch",
    "message",
    "push",
    "pushsubscriptionchange",
    "sync",
    "periodicsync",
    "notificationclick",
    "notificationclose",
    "backgroundfetchsuccess",
    "backgroundfetchfail",
    "backgroundfetchabort",
    "backgroundfetchclick",
    "cookiechange",
    "contentdelete",
];

/// The application's state as the page or worker last reported it
static STATE: AtomicU8 = AtomicU8::new(STATE_UNKNOWN);
/// A page after pagehide or freeze stays in the background whatever its visibility says, until
/// pageshow or resume
static PAGE_SUSPENDED: AtomicBool = AtomicBool::new(false);
static INSTALL: Once = Once::new();
/// Background time can only be granted to a service worker
static IS_SERVICE_WORKER: AtomicBool = AtomicBool::new(false);
/// Background time held; events stay open while any is
static BACKGROUND_TIME_HELD: AtomicUsize = AtomicUsize::new(0);
/// Background time for a service worker's start, until a monitor's holder takes it
static ENTRY_TIME_PENDING: AtomicBool = AtomicBool::new(false);

pub(super) fn monitor_started() {
    INSTALL.call_once(install);
}

pub(super) fn app_lifecycle_state() -> Option<AppLifecycleState> {
    match STATE.load(Ordering::Acquire) {
        STATE_ACTIVE => Some(AppLifecycleState::Active),
        STATE_BACKGROUND => Some(AppLifecycleState::Background),
        _ => None,
    }
}

pub(super) fn take_entry_time() -> Option<BackgroundTime> {
    if !ENTRY_TIME_PENDING.swap(false, Ordering::AcqRel) {
        return None;
    }
    BackgroundTime::begin()
}

fn install() {
    let global = js_sys::global();
    if let Some(window) = global.dyn_ref::<Window>() {
        if let Some(document) = window.document() {
            install_page(window, document);
        }
    } else if let Some(scope) = global.dyn_ref::<ServiceWorkerGlobalScope>() {
        install_service_worker(scope);
    }
}

fn install_page(window: &Window, document: Document) {
    // Heard on the window in the capture phase, as the polyfill does: document events pass it too
    let listen = |event_type: &'static str, handler: fn(&Document)| {
        let document = document.clone();
        EventListener::new_with_options(
            window,
            event_type,
            EventListenerOptions::run_in_capture_phase(),
            move |_| handler(&document),
        )
        .forget();
    };
    if has_property(&document, "onvisibilitychange") {
        listen("visibilitychange", |document| {
            if !PAGE_SUSPENDED.load(Ordering::Acquire) {
                set_state(visibility_state(document));
            }
        });
    }
    if has_property(window, "onpageshow") {
        listen("pagehide", |_| suspend_page());
        listen("pageshow", resume_page);
    }
    if has_property(&document, "onfreeze") {
        listen("freeze", |_| suspend_page());
        listen("resume", resume_page);
    }
    set_state(visibility_state(&document));
}

fn install_service_worker(scope: &ServiceWorkerGlobalScope) {
    IS_SERVICE_WORKER.store(true, Ordering::Release);
    for event_type in SERVICE_WORKER_EVENTS {
        EventListener::new(scope, event_type, keep_event_open).forget();
    }
    ENTRY_TIME_PENDING.store(true, Ordering::Release);
    set_state(STATE_BACKGROUND);
}

fn has_property(target: &JsValue, name: &str) -> bool {
    js_sys::Reflect::has(target, &JsValue::from_str(name)).unwrap_or(false)
}

fn visibility_state(document: &Document) -> u8 {
    if document.hidden() {
        STATE_BACKGROUND
    } else {
        STATE_ACTIVE
    }
}

fn suspend_page() {
    PAGE_SUSPENDED.store(true, Ordering::Release);
    set_state(STATE_BACKGROUND);
}

fn resume_page(document: &Document) {
    PAGE_SUSPENDED.store(false, Ordering::Release);
    set_state(visibility_state(document));
}

fn set_state(state: u8) {
    if STATE.swap(state, Ordering::AcqRel) != state {
        notify_changed();
    }
}

fn keep_event_open(event: &Event) {
    if BACKGROUND_TIME_HELD.load(Ordering::Acquire) == 0 {
        return;
    }
    let Some(event) = event.dyn_ref::<ExtendableEvent>() else {
        return;
    };
    let released = wasm_bindgen_futures::future_to_promise(async {
        while BACKGROUND_TIME_HELD.load(Ordering::Acquire) > 0 {
            sleep(BACKGROUND_TIME_POLL_MS).await;
        }
        Ok(JsValue::UNDEFINED)
    });
    // Throws only for an event already handled, which is too late to hold open
    let _ = event.wait_until(&released);
}

/// Restarts an extension's idle timer; any extension API call does
fn touch_extension() {
    let Some(runtime) = global_property(&["chrome", "runtime"])
        .or_else(|| global_property(&["browser", "runtime"]))
    else {
        return;
    };
    let Ok(get_platform_info) =
        js_sys::Reflect::get(&runtime, &JsValue::from_str("getPlatformInfo"))
    else {
        return;
    };
    if let Some(get_platform_info) = get_platform_info.dyn_ref::<js_sys::Function>() {
        let _ = get_platform_info.call0(&runtime);
    }
}

/// Background time in a service worker: its events stay open, and an extension's worker stays
/// awake, while any is held
#[derive(Debug)]
pub(super) struct BackgroundTime {
    _private: (),
}

impl BackgroundTime {
    pub(super) fn begin() -> Option<Self> {
        if !IS_SERVICE_WORKER.load(Ordering::Acquire) {
            return None;
        }
        if BACKGROUND_TIME_HELD.fetch_add(1, Ordering::AcqRel) == 0
            && wasm_runtime_context() == WasmRuntimeContext::Extension
        {
            spawn_detached("extension keepalive", async {
                while BACKGROUND_TIME_HELD.load(Ordering::Acquire) > 0 {
                    touch_extension();
                    sleep(EXTENSION_KEEPALIVE_MS).await;
                }
            });
        }
        Some(Self { _private: () })
    }
}

impl Drop for BackgroundTime {
    fn drop(&mut self) {
        BACKGROUND_TIME_HELD.fetch_sub(1, Ordering::AcqRel);
    }
}
