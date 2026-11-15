//! System resolver via libresolv `res_query`, the platform path on Linux and the BSDs. It reads
//! resolv.conf itself and does its own retries and TCP fallback. The call blocks, so it runs on
//! the blocking pool.

use super::*;
use dns::{classify, parse_response, txt_records, DnsError, TYPE_TXT};
use std::ffi::{c_void, CStr, CString};
use std::os::raw::{c_char, c_int};

type ResQueryFn = unsafe extern "C" fn(*const c_char, c_int, c_int, *mut u8, c_int) -> c_int;
type HErrnoLocationFn = unsafe extern "C" fn() -> *mut c_int;

struct ResolvFns {
    res_query: ResQueryFn,
    h_errno_location: Option<HErrnoLocationFn>,
}

static RESOLV: LazyLock<Option<ResolvFns>> = LazyLock::new(load);

const NS_C_IN: c_int = 1;
const MAX_ANSWER_LEN: usize = 65_535;
const HOST_NOT_FOUND: c_int = 1;
const TRY_AGAIN: c_int = 2;
const NO_RECOVERY: c_int = 3;
const NO_DATA: c_int = 4;

pub(crate) async fn txt_lookup(name: &str) -> Result<Vec<String>, DnsError> {
    let fns = RESOLV.as_ref().ok_or(DnsError::Unsupported)?;
    let dname = CString::new(name).map_err(|_| DnsError::InvalidName("interior nul"))?;
    let resp = blocking_wrapper(move || {
        let mut answer = vec![0u8; MAX_ANSWER_LEN];
        // SAFETY: dname is NUL-terminated and answer is a writable buffer of the stated length;
        // res_query was resolved against its declared prototype
        let len = unsafe {
            (fns.res_query)(
                dname.as_ptr(),
                NS_C_IN,
                c_int::from(TYPE_TXT),
                answer.as_mut_ptr(),
                answer.len() as c_int,
            )
        };
        if len < 0 {
            return Err(h_error(fns));
        }
        parse_response(&answer[..len as usize])
    })
    .await?;
    classify(resp).map(|r| txt_records(&r))
}

/// The failure res_query left in h_errno
fn h_error(fns: &ResolvFns) -> DnsError {
    let Some(location) = fns.h_errno_location else {
        return DnsError::NoRecords { rcode: 0 };
    };
    // SAFETY: __h_errno_location returns a pointer to this thread's h_errno
    let code = unsafe { *location() };
    match code {
        HOST_NOT_FOUND => DnsError::NoRecords { rcode: 3 },
        NO_DATA => DnsError::NoRecords { rcode: 0 },
        TRY_AGAIN => DnsError::Timeout,
        NO_RECOVERY => DnsError::Rcode(2),
        code => DnsError::Platform(code),
    }
}

fn load() -> Option<ResolvFns> {
    // glibc 2.34+ and musl export res_query from libc; older glibc keeps it in libresolv.so.2
    let handles = [libc::RTLD_DEFAULT, {
        // SAFETY: a NUL-terminated literal; the handle is checked for null and never closed
        unsafe {
            libc::dlopen(
                c"libresolv.so.2".as_ptr(),
                libc::RTLD_NOW | libc::RTLD_LOCAL,
            )
        }
    }];
    for handle in handles {
        if handle.is_null() && handle != libc::RTLD_DEFAULT {
            continue;
        }
        let Some(sym) = symbol(handle, c"res_query").or_else(|| symbol(handle, c"__res_query"))
        else {
            continue;
        };
        return Some(ResolvFns {
            // SAFETY: sym is a live non-null address for res_query, cast to its prototype
            // from resolv.h
            res_query: unsafe { std::mem::transmute::<*mut c_void, ResQueryFn>(sym) },
            h_errno_location: symbol(handle, c"__h_errno_location").map(|s| {
                // SAFETY: s is a live non-null address for __h_errno_location, cast to its
                // prototype from netdb.h
                unsafe { std::mem::transmute::<*mut c_void, HErrnoLocationFn>(s) }
            }),
        });
    }
    None
}

fn symbol(handle: *mut c_void, name: &CStr) -> Option<*mut c_void> {
    // SAFETY: handle is RTLD_DEFAULT or a live dlopen result and name is a NUL-terminated literal
    let sym = unsafe { libc::dlsym(handle, name.as_ptr()) };
    (!sym.is_null()).then_some(sym)
}
