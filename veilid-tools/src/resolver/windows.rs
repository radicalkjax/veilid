use super::*;

use ::windows::core::{PCSTR, PSTR};
use ::windows::Win32::NetworkManagement::Dns::{
    DnsFree, DnsFreeRecordList, DnsQuery_UTF8, DNS_QUERY_STANDARD, DNS_RECORDA, DNS_TYPE_PTR,
    DNS_TYPE_TEXT,
};
use core::ffi::c_void;
use std::ffi::{CStr, CString};

/// Cross-platform DNS resolver. This backend uses the Windows system resolver.
pub struct Resolver {}

impl Resolver {
    /// Resolve the TXT records for `host`, returning each record's text.
    ///
    /// The Windows DNS query is synchronous, so this blocks the calling thread despite being `async`.
    ///
    /// Errors with [`ResolverError::Generic`] if `host` contains an interior NUL or the `DnsQuery_UTF8` call fails. Returns `Ok` with an empty vec when the name resolves but has no TXT records.
    #[allow(clippy::unused_async)]
    pub async fn txt_lookup<S: AsRef<str>>(host: S) -> Result<Vec<String>, ResolverError> {
        let mut out = Vec::new();
        unsafe {
            let mut p_query_results: *mut DNS_RECORDA = core::ptr::null_mut();
            let host = CString::new(host.as_ref())
                .map_err(|e| ResolverError::Generic(format!("invalid host string: {}", e)))?;
            DnsQuery_UTF8(
                PCSTR::from_raw(host.as_bytes_with_nul().as_ptr()),
                DNS_TYPE_TEXT,
                DNS_QUERY_STANDARD,
                None,
                &mut p_query_results as *mut *mut DNS_RECORDA,
                None,
            )
            .ok()
            .map_err(|e| ResolverError::Generic(format!("Failed to resolve TXT record: {}", e)))?;

            let mut p_record: *mut DNS_RECORDA = p_query_results;
            while !p_record.is_null() {
                if (*p_record).wType == DNS_TYPE_TEXT.0 {
                    let count: usize = (*p_record)
                        .Data
                        .TXT
                        .dwStringCount
                        .try_into()
                        .unwrap_or_log();
                    let string_array: *const PSTR = &(*p_record).Data.TXT.pStringArray[0];
                    let mut record_out = Vec::<u8>::new();
                    for n in 0..count {
                        let pstr: PSTR = *(string_array.add(n));
                        let c_str: &CStr = CStr::from_ptr(pstr.0 as *const i8);
                        record_out.extend_from_slice(c_str.to_bytes());
                    }
                    if let Ok(s) = String::from_utf8(record_out) {
                        out.push(s);
                    }
                }
                p_record = (*p_record).pNext;
            }
            DnsFree(Some(p_query_results as *const c_void), DnsFreeRecordList);
        }
        Ok(out)
    }

    /// Reverse-resolve `ip_addr` to a hostname via its PTR record.
    ///
    /// The Windows DNS query is synchronous, so this blocks the calling thread despite being `async`.
    ///
    /// Errors with [`ResolverError::Generic`] if the reverse-arpa name contains an interior NUL, the `DnsQuery_UTF8` call fails, or no PTR record is returned.
    #[allow(clippy::unused_async)]
    pub async fn ptr_lookup(ip_addr: IpAddr) -> Result<String, ResolverError> {
        let host = match ip_addr {
            IpAddr::V4(a) => {
                let oct = a.octets();
                format!("{}.{}.{}.{}.in-addr.arpa", oct[3], oct[2], oct[1], oct[0])
            }
            IpAddr::V6(a) => {
                let mut s = String::new();
                for b in a.octets().iter().rev() {
                    s.push_str(&format!("{:x}.{:x}.", b & 0x0F, b >> 4));
                }
                format!("{}ip6.arpa", s)
            }
        };

        unsafe {
            let mut p_query_results: *mut DNS_RECORDA = core::ptr::null_mut();
            let host = CString::new(host)
                .map_err(|e| ResolverError::Generic(format!("invalid host string: {}", e)))?;
            DnsQuery_UTF8(
                PCSTR::from_raw(host.as_bytes_with_nul().as_ptr()),
                DNS_TYPE_PTR,
                DNS_QUERY_STANDARD,
                None,
                &mut p_query_results as *mut *mut DNS_RECORDA,
                None,
            )
            .ok()
            .map_err(|e| ResolverError::Generic(format!("Failed to resolve PTR record: {}", e)))?;

            let mut p_record: *mut DNS_RECORDA = p_query_results;
            while !p_record.is_null() {
                if (*p_record).wType == DNS_TYPE_PTR.0 {
                    let p_name_host: PSTR = (*p_record).Data.PTR.pNameHost;
                    let c_str: &CStr = CStr::from_ptr(p_name_host.0 as *const i8);
                    if let Ok(str_slice) = c_str.to_str() {
                        let str_buf: String = str_slice.to_owned();
                        DnsFree(Some(p_query_results as *const c_void), DnsFreeRecordList);
                        return Ok(str_buf);
                    }
                }
                p_record = (*p_record).pNext;
            }
            DnsFree(Some(p_query_results as *const c_void), DnsFreeRecordList);
        }
        Err(ResolverError::Generic("No records returned".to_owned()))
    }
}
