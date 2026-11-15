//! Android system resolver via `android_res_nquery`, which honors per-network DNS and private DNS.
//!
//! The symbols arrive in API 29 and minSdk is lower, so they are bound at runtime; without them
//! TXT lookup is unsupported and bootstrap goes direct instead.

use super::*;
use dns::{classify, parse_response, txt_records, DnsError, Response, TYPE_TXT};
use fd_ready::wait_readable;
use std::ffi::{c_void, CStr, CString};
use std::io;
use std::os::fd::RawFd;
use std::os::raw::{c_char, c_int};

type NQueryFn = unsafe extern "C" fn(u64, *const c_char, c_int, c_int, u32) -> c_int;
type NResultFn = unsafe extern "C" fn(c_int, *mut c_int, *mut u8, usize) -> c_int;
type CancelFn = unsafe extern "C" fn(c_int);

struct AndroidResFns {
    nquery: NQueryFn,
    nresult: NResultFn,
    cancel: CancelFn,
}

static ANDROID_RES: LazyLock<Option<AndroidResFns>> = LazyLock::new(load);

const NETWORK_UNSPECIFIED: u64 = 0;
const NS_C_IN: c_int = 1;
/// The OS runs its own retry ladder inside one query
const ANDROID_QUERY_TIMEOUT_MS: u32 = 10_000;
const MAX_ANSWER_LEN: usize = 65_535;

/// True when this device exposes the resolver API (API 29 and up)
#[must_use]
pub(crate) fn available() -> bool {
    ANDROID_RES.is_some()
}

pub(crate) async fn txt_lookup(name: &str) -> Result<Vec<String>, DnsError> {
    let resp = query(name, TYPE_TXT).await?;
    Ok(txt_records(&resp))
}

/// Ask the OS resolver for `name`/`qtype`
async fn query(name: &str, qtype: u16) -> Result<Response, DnsError> {
    let fns = ANDROID_RES.as_ref().ok_or(DnsError::Unsupported)?;
    let dname = CString::new(name).map_err(|_| DnsError::InvalidName("interior nul"))?;

    // SAFETY: dname is a NUL-terminated string that outlives the call, and the pointer was
    // resolved from libandroid.so against its declared prototype
    let fd = unsafe {
        (fns.nquery)(
            NETWORK_UNSPECIFIED,
            dname.as_ptr(),
            NS_C_IN,
            c_int::from(qtype),
            0,
        )
    };
    if fd < 0 {
        return Err(DnsError::Io(io::Error::from_raw_os_error(-fd)));
    }
    let mut query_fd = QueryFd {
        fd,
        fns,
        consumed: false,
    };

    timeout(ANDROID_QUERY_TIMEOUT_MS, wait_readable(fd))
        .await
        .map_err(|_| DnsError::Timeout)??;

    let mut answer = vec![0u8; MAX_ANSWER_LEN];
    let mut rcode: c_int = 0;
    // SAFETY: fd is the live query fd this guard owns and nresult consumes (closes) it exactly
    // once; answer is a writable allocation of the stated length
    let len = unsafe { (fns.nresult)(fd, &mut rcode, answer.as_mut_ptr(), answer.len()) };
    query_fd.consumed = true;
    if len < 0 {
        return Err(DnsError::Io(io::Error::from_raw_os_error(-len)));
    }

    let resp = parse_response(&answer[..len as usize])?;
    // The OS picked the query id, so only the question can be checked
    if !resp.qr
        || resp.questions.len() != 1
        || resp.questions[0].qtype != qtype
        || !resp.questions[0]
            .name
            .trim_end_matches('.')
            .eq_ignore_ascii_case(name.trim_end_matches('.'))
    {
        return Err(DnsError::Malformed("answer does not match the question"));
    }
    classify(resp)
}

/// Cancels the query unless `nresult` already consumed it
struct QueryFd {
    fd: RawFd,
    fns: &'static AndroidResFns,
    consumed: bool,
}

impl Drop for QueryFd {
    fn drop(&mut self) {
        if !self.consumed {
            // SAFETY: fd is live and was not consumed by nresult, so this closes it exactly once
            unsafe { (self.fns.cancel)(self.fd) };
        }
    }
}

fn load() -> Option<AndroidResFns> {
    // SAFETY: a NUL-terminated literal; the handle is checked for null and never closed
    let handle =
        unsafe { libc::dlopen(c"libandroid.so".as_ptr(), libc::RTLD_NOW | libc::RTLD_LOCAL) };
    if handle.is_null() {
        return None;
    }
    Some(AndroidResFns {
        // SAFETY: symbol() yields a live non-null address for android_res_nquery, cast to its
        // prototype from android/multinetwork.h
        nquery: unsafe {
            std::mem::transmute::<*mut c_void, NQueryFn>(symbol(handle, c"android_res_nquery")?)
        },
        // SAFETY: symbol() yields a live non-null address for android_res_nresult, cast to its
        // prototype from android/multinetwork.h
        nresult: unsafe {
            std::mem::transmute::<*mut c_void, NResultFn>(symbol(handle, c"android_res_nresult")?)
        },
        // SAFETY: symbol() yields a live non-null address for android_res_cancel, cast to its
        // prototype from android/multinetwork.h
        cancel: unsafe {
            std::mem::transmute::<*mut c_void, CancelFn>(symbol(handle, c"android_res_cancel")?)
        },
    })
}

fn symbol(handle: *mut c_void, name: &CStr) -> Option<*mut c_void> {
    // SAFETY: handle is a live dlopen result and name is a NUL-terminated literal
    let sym = unsafe { libc::dlsym(handle, name.as_ptr()) };
    (!sym.is_null()).then_some(sym)
}
