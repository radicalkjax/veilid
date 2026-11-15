//! Windows system resolver via `DnsQuery_UTF8`, which goes through the DNS Client service and
//! so honors its configured servers and DoH. The calls block, so they run on the blocking pool.

use super::*;
use ::windows::core::{PCSTR, PSTR};
use ::windows::Win32::NetworkManagement::Dns::{
    DnsFree, DnsFreeRecordList, DnsQuery_UTF8, DNS_QUERY_STANDARD, DNS_RECORDA, DNS_TYPE_PTR,
    DNS_TYPE_TEXT,
};
use core::ffi::c_void;
use dns::DnsError;
use std::ffi::{CStr, CString};

pub(crate) async fn txt_lookup(name: &str) -> Result<Vec<String>, DnsError> {
    let host = CString::new(name).map_err(|_| DnsError::InvalidName("interior nul"))?;
    blocking_wrapper(move || {
        let mut out = Vec::new();
        // SAFETY: host is NUL-terminated and outlives the call; the record list DnsQuery_UTF8
        // returns is walked through its own pNext links and freed exactly once with DnsFree
        unsafe {
            let mut p_query_results: *mut DNS_RECORDA = core::ptr::null_mut();
            DnsQuery_UTF8(
                PCSTR::from_raw(host.as_bytes_with_nul().as_ptr()),
                DNS_TYPE_TEXT,
                DNS_QUERY_STANDARD,
                None,
                &mut p_query_results as *mut *mut DNS_RECORDA,
                None,
            )
            .ok()
            .map_err(|e| DnsError::Platform(e.code().0))?;

            let mut p_record: *const DNS_RECORDA = p_query_results;
            while let Some(record) = p_record.as_ref() {
                if record.wType == DNS_TYPE_TEXT.0 {
                    let count = record.Data.TXT.dwStringCount as usize;
                    // pStringArray is a flexible array member; address it as a whole, not
                    // through its declared first element
                    let string_array: *const PSTR =
                        core::ptr::addr_of!(record.Data.TXT.pStringArray).cast::<PSTR>();
                    let mut record_out = Vec::<u8>::new();
                    for n in 0..count {
                        let pstr: PSTR = string_array.add(n).read_unaligned();
                        if pstr.is_null() {
                            continue;
                        }
                        record_out
                            .extend_from_slice(CStr::from_ptr(pstr.0 as *const i8).to_bytes());
                    }
                    if let Ok(s) = String::from_utf8(record_out) {
                        out.push(s);
                    }
                }
                p_record = record.pNext;
            }
            DnsFree(Some(p_query_results as *const c_void), DnsFreeRecordList);
        }
        if out.is_empty() {
            return Err(DnsError::NoRecords { rcode: 0 });
        }
        Ok(out)
    })
    .await
}

pub(crate) async fn ptr_lookup(ip_addr: IpAddr) -> Result<String, DnsError> {
    let host =
        CString::new(reverse_name(ip_addr)).map_err(|_| DnsError::InvalidName("interior nul"))?;
    // SAFETY: host is NUL-terminated and outlives the call; the record list DnsQuery_UTF8 returns
    // is walked through its own pNext links and freed exactly once with DnsFree on every path
    blocking_wrapper(move || unsafe {
        let mut p_query_results: *mut DNS_RECORDA = core::ptr::null_mut();
        DnsQuery_UTF8(
            PCSTR::from_raw(host.as_bytes_with_nul().as_ptr()),
            DNS_TYPE_PTR,
            DNS_QUERY_STANDARD,
            None,
            &mut p_query_results as *mut *mut DNS_RECORDA,
            None,
        )
        .ok()
        .map_err(|e| DnsError::Platform(e.code().0))?;

        let mut found = None;
        let mut p_record: *const DNS_RECORDA = p_query_results;
        while let Some(record) = p_record.as_ref() {
            if record.wType == DNS_TYPE_PTR.0 {
                let p_name_host: PSTR = record.Data.PTR.pNameHost;
                if !p_name_host.is_null() {
                    if let Ok(name) = CStr::from_ptr(p_name_host.0 as *const i8).to_str() {
                        found = Some(name.trim_end_matches('.').to_owned());
                        break;
                    }
                }
            }
            p_record = record.pNext;
        }
        DnsFree(Some(p_query_results as *const c_void), DnsFreeRecordList);
        found.ok_or(DnsError::NoRecords { rcode: 0 })
    })
    .await
}

/// Reverse-lookup name for `ip` (`in-addr.arpa` / `ip6.arpa`), no trailing dot
fn reverse_name(ip: IpAddr) -> String {
    match ip {
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
    }
}
