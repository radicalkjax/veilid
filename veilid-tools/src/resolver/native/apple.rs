//! Apple system resolver via dns_sd `DNSServiceQueryRecord`. Queries go through mDNSResponder,
//! so encrypted DNS profiles, VPN-pushed and per-interface resolvers all apply. The symbols
//! live in libsystem_dnssd, which libSystem re-exports, so no link flag reaches static consumers.

use super::*;
use dns::{message::parse_txt, DnsError};
use std::ffi::{c_void, CString};
use std::os::raw::{c_char, c_int};

type DNSServiceRef = *mut c_void;
type ReplyFn = unsafe extern "C" fn(
    DNSServiceRef,
    u32,
    u32,
    i32,
    *const c_char,
    u16,
    u16,
    u16,
    *const c_void,
    u32,
    *mut c_void,
);

extern "C" {
    fn DNSServiceQueryRecord(
        sd_ref: *mut DNSServiceRef,
        flags: u32,
        interface_index: u32,
        fullname: *const c_char,
        rrtype: u16,
        rrclass: u16,
        callback: ReplyFn,
        context: *mut c_void,
    ) -> i32;
    fn DNSServiceRefSockFD(sd_ref: DNSServiceRef) -> c_int;
    fn DNSServiceProcessResult(sd_ref: DNSServiceRef) -> i32;
    fn DNSServiceRefDeallocate(sd_ref: DNSServiceRef);
}

const FLAG_MORE_COMING: u32 = 0x1;
const FLAG_ADD: u32 = 0x2;
/// Negative answers are delivered to the callback instead of never calling it
const FLAG_RETURN_INTERMEDIATES: u32 = 0x1000;
/// mDNSResponder reports its own timeout through the callback
const FLAG_TIMEOUT: u32 = 0x10000;
const ERR_NO_ERROR: i32 = 0;
const ERR_NO_SUCH_NAME: i32 = -65538;
const ERR_NO_SUCH_RECORD: i32 = -65554;
const ERR_TIMEOUT: i32 = -65568;
const CLASS_IN: u16 = 1;
const TYPE_TXT: u16 = 16;
const INTERFACE_ANY: u32 = 0;
const QUERY_TIMEOUT_MS: u32 = 10_000;

struct QueryState {
    records: Vec<String>,
    error: Option<DnsError>,
    done: bool,
}

unsafe extern "C" fn reply(
    _sd_ref: DNSServiceRef,
    flags: u32,
    _interface_index: u32,
    error_code: i32,
    _fullname: *const c_char,
    rrtype: u16,
    _rrclass: u16,
    rdlen: u16,
    rdata: *const c_void,
    _ttl: u32,
    context: *mut c_void,
) {
    if context.is_null() {
        return;
    }
    // SAFETY: context is the QueryState the query was started with, alive until deallocation
    let state = unsafe { &mut *(context as *mut QueryState) };
    // ReturnIntermediates also delivers CNAMEs; only a TXT or a failure can finish the answer
    let mut answered = rrtype == TYPE_TXT;
    match error_code {
        ERR_NO_ERROR => {
            if flags & FLAG_ADD != 0 && rrtype == TYPE_TXT && !rdata.is_null() {
                // SAFETY: rdata points at rdlen readable bytes for the duration of the callback
                let bytes =
                    unsafe { std::slice::from_raw_parts(rdata as *const u8, usize::from(rdlen)) };
                match parse_txt(bytes) {
                    Ok(strings) => {
                        if let Ok(s) = String::from_utf8(strings.concat()) {
                            state.records.push(s);
                        }
                    }
                    Err(e) => state.error = Some(e),
                }
            }
        }
        ERR_NO_SUCH_NAME => state.error = Some(DnsError::NoRecords { rcode: 3 }),
        ERR_NO_SUCH_RECORD => state.error = Some(DnsError::NoRecords { rcode: 0 }),
        ERR_TIMEOUT => state.error = Some(DnsError::Timeout),
        code => state.error = Some(DnsError::Platform(code)),
    }
    if error_code != ERR_NO_ERROR {
        answered = true;
    }
    if answered && flags & FLAG_MORE_COMING == 0 {
        state.done = true;
    }
}

/// One in-flight query: the service ref and the state its callback writes
struct Query {
    sd_ref: DNSServiceRef,
    state: *mut QueryState,
}

// SAFETY: only the task driving the query touches the ref and state, one call at a time; the
// callback runs inside DNSServiceProcessResult on that same task
unsafe impl Send for Query {}
unsafe impl Sync for Query {}

impl Query {
    fn start(name: &str) -> Result<Self, DnsError> {
        let fullname = CString::new(name).map_err(|_| DnsError::InvalidName("interior nul"))?;
        let state = Box::into_raw(Box::new(QueryState {
            records: Vec::new(),
            error: None,
            done: false,
        }));
        let mut sd_ref: DNSServiceRef = std::ptr::null_mut();
        // SAFETY: fullname outlives the call, and state outlives the service ref, which Drop
        // deallocates before freeing it
        let err = unsafe {
            DNSServiceQueryRecord(
                &mut sd_ref,
                FLAG_RETURN_INTERMEDIATES | FLAG_TIMEOUT,
                INTERFACE_ANY,
                fullname.as_ptr(),
                TYPE_TXT,
                CLASS_IN,
                reply,
                state as *mut c_void,
            )
        };
        if err != ERR_NO_ERROR {
            // SAFETY: state was just created by Box::into_raw and nothing else holds it
            drop(unsafe { Box::from_raw(state) });
            return Err(DnsError::Platform(err));
        }
        Ok(Self { sd_ref, state })
    }

    /// Pump results until the answer is complete or the resolver reports a failure
    async fn drive(&self) -> Result<(), DnsError> {
        // SAFETY: sd_ref is live
        let fd = unsafe { DNSServiceRefSockFD(self.sd_ref) };
        if fd < 0 {
            return Err(DnsError::Platform(fd));
        }
        loop {
            fd_ready::wait_readable(fd).await?;
            // SAFETY: sd_ref is live; this runs the callback on this thread before returning
            let err = unsafe { DNSServiceProcessResult(self.sd_ref) };
            if err != ERR_NO_ERROR {
                return Err(DnsError::Platform(err));
            }
            // SAFETY: no callback runs outside DNSServiceProcessResult, which has returned
            let state = unsafe { &mut *self.state };
            if let Some(e) = state.error.take() {
                return Err(e);
            }
            if state.done {
                return Ok(());
            }
        }
    }

    fn take_records(&self) -> Vec<String> {
        // SAFETY: no callback is running between DNSServiceProcessResult calls
        std::mem::take(&mut unsafe { &mut *self.state }.records)
    }
}

impl Drop for Query {
    fn drop(&mut self) {
        // SAFETY: deallocating the ref stops callbacks, so the state can be freed after it
        unsafe {
            DNSServiceRefDeallocate(self.sd_ref);
            drop(Box::from_raw(self.state));
        }
    }
}

pub(crate) async fn txt_lookup(name: &str) -> Result<Vec<String>, DnsError> {
    let query = Query::start(name)?;
    timeout(QUERY_TIMEOUT_MS, query.drive())
        .await
        .map_err(|_| DnsError::Timeout)??;
    let records = query.take_records();
    if records.is_empty() {
        return Err(DnsError::NoRecords { rcode: 0 });
    }
    Ok(records)
}

#[cfg(test)]
mod tests {
    use super::*;

    const TYPE_CNAME: u16 = 5;

    /// Run the callback against a fresh state and hand the state back
    fn deliver(flags: u32, error_code: i32, rrtype: u16, rdata: Option<&[u8]>) -> QueryState {
        deliver_into(
            QueryState {
                records: Vec::new(),
                error: None,
                done: false,
            },
            flags,
            error_code,
            rrtype,
            rdata,
        )
    }

    fn deliver_into(
        state: QueryState,
        flags: u32,
        error_code: i32,
        rrtype: u16,
        rdata: Option<&[u8]>,
    ) -> QueryState {
        let state = Box::into_raw(Box::new(state));
        let (ptr, len) = match rdata {
            Some(d) => (d.as_ptr().cast::<c_void>(), d.len() as u16),
            None => (std::ptr::null(), 0),
        };
        // SAFETY: state is a live Box and rdata (if any) outlives the call, as mDNSResponder
        // guarantees for a real callback
        unsafe {
            reply(
                std::ptr::null_mut(),
                flags,
                0,
                error_code,
                std::ptr::null(),
                rrtype,
                CLASS_IN,
                len,
                ptr,
                0,
                state.cast(),
            );
            *Box::from_raw(state)
        }
    }

    #[test]
    fn txt_records_accumulate_until_the_last_one() {
        let first = deliver(
            FLAG_ADD | FLAG_MORE_COMING,
            ERR_NO_ERROR,
            TYPE_TXT,
            Some(b"\x05hello"),
        );
        assert_eq!(first.records, vec!["hello".to_owned()]);
        assert!(!first.done);
        assert!(first.error.is_none());

        let last = deliver_into(
            first,
            FLAG_ADD,
            ERR_NO_ERROR,
            TYPE_TXT,
            Some(b"\x03abc\x02de"),
        );
        assert_eq!(last.records, vec!["hello".to_owned(), "abcde".to_owned()]);
        assert!(last.done);
    }

    #[test]
    fn cname_intermediate_does_not_finish_the_answer() {
        let state = deliver(FLAG_ADD, ERR_NO_ERROR, TYPE_CNAME, Some(b"\x03foo\x00"));
        assert!(state.records.is_empty());
        assert!(!state.done);
        assert!(state.error.is_none());
    }

    #[test]
    fn negative_and_error_replies_finish_with_an_error() {
        let s = deliver(0, ERR_NO_SUCH_NAME, TYPE_TXT, None);
        assert!(matches!(s.error, Some(DnsError::NoRecords { rcode: 3 })));
        assert!(s.done);
        let s = deliver(0, ERR_NO_SUCH_RECORD, TYPE_CNAME, None);
        assert!(matches!(s.error, Some(DnsError::NoRecords { rcode: 0 })));
        assert!(s.done);
        let s = deliver(0, ERR_TIMEOUT, TYPE_TXT, None);
        assert!(matches!(s.error, Some(DnsError::Timeout)));
        let s = deliver(FLAG_MORE_COMING, -65537, TYPE_TXT, None);
        assert!(matches!(s.error, Some(DnsError::Platform(-65537))));
        assert!(!s.done, "MoreComing keeps even a failed query open");
    }

    #[test]
    fn ignores_removals_empty_and_malformed_rdata() {
        let s = deliver(0, ERR_NO_ERROR, TYPE_TXT, Some(b"\x05hello"));
        assert!(s.records.is_empty(), "no FLAG_ADD");
        assert!(s.done);
        let s = deliver(FLAG_ADD, ERR_NO_ERROR, TYPE_TXT, None);
        assert!(s.records.is_empty(), "null rdata");
        let s = deliver(FLAG_ADD, ERR_NO_ERROR, TYPE_TXT, Some(b"\x09short"));
        assert!(s.records.is_empty());
        assert!(matches!(s.error, Some(DnsError::Malformed(_))));
        let s = deliver(FLAG_ADD, ERR_NO_ERROR, TYPE_TXT, Some(b"\x02\xff\xfe"));
        assert!(s.records.is_empty(), "non-utf8 record dropped");
        assert!(s.error.is_none());
    }

    #[test]
    fn null_context_is_ignored() {
        // SAFETY: every pointer argument is null and the callback checks context first
        unsafe {
            reply(
                std::ptr::null_mut(),
                FLAG_ADD,
                0,
                ERR_NO_ERROR,
                std::ptr::null(),
                TYPE_TXT,
                CLASS_IN,
                0,
                std::ptr::null(),
                0,
                std::ptr::null_mut(),
            );
        }
    }

    #[test]
    fn interior_nul_names_are_rejected_before_any_query() {
        assert!(matches!(
            Query::start("bad\0name"),
            Err(DnsError::InvalidName(_))
        ));
    }

    #[test]
    fn oversized_names_fail_at_start() {
        // Longer than kDNSServiceMaxDomainName, so mDNSResponder refuses the query itself
        let name = "a".repeat(2000);
        assert!(matches!(Query::start(&name), Err(DnsError::Platform(_))));
    }
}
