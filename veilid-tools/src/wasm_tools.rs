use super::*;
use core::sync::atomic::{AtomicI8, AtomicU32, Ordering};
use js_sys::{global, Reflect};
use std::io;
use wasm_bindgen::JsValue;
use ws_stream_wasm::WsErr;

/// The JavaScript global scope this wasm module is running in.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
#[repr(i8)]
pub enum WasmRuntimeContext {
    /// Page main thread (`Window`).
    Window = 0,
    /// Dedicated worker (`DedicatedWorkerGlobalScope`).
    DedicatedWorker = 1,
    /// Shared worker (`SharedWorkerGlobalScope`).
    SharedWorker = 2,
    /// Service worker (`ServiceWorkerGlobalScope`).
    ServiceWorker = 3,
    /// WebExtension context (exposes `chrome.runtime.id`/`browser.runtime.id`).
    Extension = 4,
    /// Unrecognized scope (e.g. Node.js).
    Unknown = 5,
}

impl WasmRuntimeContext {
    fn from_i8(v: i8) -> Option<Self> {
        match v {
            0 => Some(Self::Window),
            1 => Some(Self::DedicatedWorker),
            2 => Some(Self::SharedWorker),
            3 => Some(Self::ServiceWorker),
            4 => Some(Self::Extension),
            5 => Some(Self::Unknown),
            _ => None,
        }
    }
}

pub(crate) fn global_property(path: &[&str]) -> Option<JsValue> {
    let mut cur: JsValue = global().into();
    for key in path {
        cur = Reflect::get(&cur, &JsValue::from_str(key)).ok()?;
        if cur.is_undefined() || cur.is_null() {
            return None;
        }
    }
    Some(cur)
}

fn has_extension_runtime() -> bool {
    ["chrome", "browser"]
        .iter()
        .any(|ns| global_property(&[ns, "runtime", "id"]).is_some_and(|v| v.is_truthy()))
}

/// The JavaScript global scope this wasm module is running in. Cached after the first call.
pub fn wasm_runtime_context() -> WasmRuntimeContext {
    static CACHE: AtomicI8 = AtomicI8::new(-1);
    if let Some(ctx) = WasmRuntimeContext::from_i8(CACHE.load(Ordering::Acquire)) {
        return ctx;
    }

    // Extension wins over the base scope: an extension service worker also reports as
    // ServiceWorkerGlobalScope but must use extension storage.
    let ctx = if has_extension_runtime() {
        WasmRuntimeContext::Extension
    } else {
        match global_property(&["constructor", "name"])
            .and_then(|v| v.as_string())
            .as_deref()
        {
            Some("Window") => WasmRuntimeContext::Window,
            Some("DedicatedWorkerGlobalScope") => WasmRuntimeContext::DedicatedWorker,
            Some("SharedWorkerGlobalScope") => WasmRuntimeContext::SharedWorker,
            Some("ServiceWorkerGlobalScope") => WasmRuntimeContext::ServiceWorker,
            _ => WasmRuntimeContext::Unknown,
        }
    };

    CACHE.store(ctx as i8, Ordering::Release);
    ctx
}

/// Whether the host JS environment is a browser scope or exposes a `navigator` global.
///
/// Result is cached after the first call.
#[must_use]
pub fn is_browser() -> bool {
    // Any recognized web scope, or a JS env exposing navigator (e.g. Node.js).
    wasm_runtime_context() != WasmRuntimeContext::Unknown
        || Reflect::has(global().as_ref(), &"navigator".into()).unwrap_or_default()
}

/// Whether the page was loaded over HTTPS.
///
/// Result is cached after the first call.
pub fn is_browser_https() -> bool {
    static CACHE: AtomicI8 = AtomicI8::new(-1);
    let cache = CACHE.load(Ordering::Acquire);
    if cache != -1 {
        return cache != 0;
    }

    let res = global_property(&["location", "protocol"])
        .and_then(|v| v.as_string())
        .is_some_and(|p| p == "https:");

    CACHE.store(res as i8, Ordering::Release);

    res
}

static IPV6_IS_SUPPORTED: Mutex<Option<bool>> = Mutex::new(None);

/// Whether IPv6 is usable in this wasm environment. Cached after the first call.
pub fn is_ipv6_supported() -> bool {
    let mut opt_supp = IPV6_IS_SUPPORTED.lock();
    if let Some(supp) = *opt_supp {
        return supp;
    }

    // XXX: See issue #92
    let supp = false;

    *opt_supp = Some(supp);
    supp
}

static IPV4_IS_SUPPORTED: Mutex<Option<bool>> = Mutex::new(None);

/// Whether IPv4 is usable in this wasm environment. Cached after the first call.
pub fn is_ipv4_supported() -> bool {
    let mut opt_supp = IPV4_IS_SUPPORTED.lock();
    if let Some(supp) = *opt_supp {
        return supp;
    }

    // XXX: See issue #92
    let supp = true;

    *opt_supp = Some(supp);
    supp
}

/// Number of logical cores reported by `navigator.hardwareConcurrency`, defaulting to 1.
///
/// Result is cached after the first call.
pub fn get_concurrency() -> u32 {
    static CACHE: AtomicU32 = AtomicU32::new(0);
    let cache = CACHE.load(Ordering::Acquire);
    if cache != 0 {
        return cache;
    }

    let res = global_property(&["navigator", "hardwareConcurrency"])
        .and_then(|v| v.as_f64())
        .map_or(1, |n| n as u32)
        .max(1);

    CACHE.store(res, Ordering::Release);

    res
}

/// Map a `ws_stream_wasm` [`WsErr`] onto the closest [`io::Error`] kind.
#[must_use]
pub fn ws_err_to_io_error(err: WsErr) -> io::Error {
    match err {
        WsErr::InvalidWsState { supplied: _ } => {
            io::Error::new(io::ErrorKind::InvalidInput, err.to_string())
        }
        WsErr::ConnectionNotOpen => io::Error::new(io::ErrorKind::NotConnected, err.to_string()),
        WsErr::InvalidUrl { supplied: _ } => {
            io::Error::new(io::ErrorKind::InvalidInput, err.to_string())
        }
        WsErr::InvalidCloseCode { supplied: _ } => {
            io::Error::new(io::ErrorKind::InvalidInput, err.to_string())
        }
        WsErr::ReasonStringToLong => io::Error::new(io::ErrorKind::InvalidInput, err.to_string()),
        WsErr::ConnectionFailed { event: _ } => {
            io::Error::new(io::ErrorKind::ConnectionRefused, err.to_string())
        }
        WsErr::InvalidEncoding => io::Error::new(io::ErrorKind::InvalidInput, err.to_string()),
        WsErr::CantDecodeBlob => io::Error::new(io::ErrorKind::InvalidInput, err.to_string()),
        WsErr::UnknownDataType => io::Error::new(io::ErrorKind::InvalidInput, err.to_string()),
        _ => io::Error::other(err.to_string()),
    }
}
