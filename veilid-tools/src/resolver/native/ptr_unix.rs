//! Reverse lookup via libc `getnameinfo`, the system resolver's PTR path. Blocking, so it runs
//! on the blocking pool.

use super::*;
use dns::DnsError;
use std::ffi::CStr;
use std::os::raw::c_char;

const NI_MAXHOST: usize = 1025;

pub(crate) async fn ptr_lookup(ip_addr: IpAddr) -> Result<String, DnsError> {
    blocking_wrapper(move || {
        let addr = socket2::SockAddr::from(SocketAddr::new(ip_addr, 0));
        let mut host = vec![0 as c_char; NI_MAXHOST];
        // SAFETY: addr is a valid sockaddr of the stated length and host is a writable buffer
        // of the stated length; NI_NAMEREQD fails rather than returning the address as text
        let rc = unsafe {
            libc::getnameinfo(
                addr.as_ptr() as *const libc::sockaddr,
                addr.len(),
                host.as_mut_ptr(),
                NI_MAXHOST as _,
                std::ptr::null_mut(),
                0,
                libc::NI_NAMEREQD,
            )
        };
        if rc != 0 {
            return Err(DnsError::Platform(rc));
        }
        // SAFETY: getnameinfo NUL-terminates host on success
        let name = unsafe { CStr::from_ptr(host.as_ptr()) }
            .to_str()
            .map_err(|_| DnsError::Malformed("non-utf8 hostname"))?;
        Ok(name.trim_end_matches('.').to_owned())
    })
    .await
}
