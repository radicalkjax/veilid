// Copyright 2018 MaidSafe.net limited.
//
// This SAFE Network Software is licensed to you under the MIT license <LICENSE-MIT
// http://opensource.org/licenses/MIT> or the Modified BSD license <LICENSE-BSD
// https://opensource.org/licenses/BSD-3-Clause>, at your option. This file may not be copied,
// modified, or distributed except according to those terms. Please review the Licences for the
// specific language governing permissions and limitations relating to use of the SAFE Network
// Software.

//! Bounded reads of OS-provided `sockaddr`s.
//!
//! The kernel hands back sockaddrs shorter than their family's struct: BSD netmasks carry only
//! the significant bytes (`sa_len` 5..7) and some point-to-point destinations are empty
//! (`sa_len` 0). Every read here copies at most the stated length into a zeroed struct, so a
//! truncated mask reads as zero-filled and a short address never runs past its allocation.

#[cfg(not(windows))]
use libc::{sockaddr, sockaddr_in, sockaddr_in6, AF_INET, AF_INET6};
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr};
use std::ptr::NonNull;
#[cfg(windows)]
use winapi::{
    shared::ws2def::SOCKET_ADDRESS,
    shared::ws2def::{AF_INET, AF_INET6, SOCKADDR as sockaddr, SOCKADDR_IN as sockaddr_in},
    shared::ws2ipdef::SOCKADDR_IN6 as sockaddr_in6,
};

/// Bytes needed to read the family field
const SA_HEADER_LEN: usize = 2;

#[cfg(not(windows))]
pub fn to_ipaddr(sockaddr: *const sockaddr) -> Option<IpAddr> {
    SockAddr::new(sockaddr)?.as_ipaddr()
}

#[cfg(windows)]
pub fn to_ipaddr(address: &SOCKET_ADDRESS) -> Option<IpAddr> {
    let len = usize::try_from(address.iSockaddrLength).ok()?;
    SockAddr::new(address.lpSockaddr, len)?.as_ipaddr()
}

pub enum SockAddrIn {
    In(sockaddr_in),
    In6(sockaddr_in6),
}

/// A non-null sockaddr pointer with the byte length the OS declared for it
pub struct SockAddr {
    inner: NonNull<sockaddr>,
    len: usize,
}

impl SockAddr {
    /// None for a null pointer or one too short to carry a family
    #[cfg(not(windows))]
    #[allow(clippy::new_ret_no_self)]
    pub fn new(sockaddr: *const sockaddr) -> Option<Self> {
        let inner = NonNull::new(sockaddr.cast_mut())?;
        // SAFETY: an OS sockaddr always has its length byte; nothing past it is read here
        let len =
            usize::from(unsafe { std::ptr::addr_of!((*inner.as_ptr()).sa_len).read_unaligned() });
        (len >= SA_HEADER_LEN).then_some(Self { inner, len })
    }

    /// None for a null pointer or a length too short to carry a family
    #[cfg(windows)]
    #[allow(clippy::new_ret_no_self)]
    pub fn new(sockaddr: *const sockaddr, len: usize) -> Option<Self> {
        let inner = NonNull::new(sockaddr.cast_mut())?;
        (len >= SA_HEADER_LEN).then_some(Self { inner, len })
    }

    #[cfg(not(windows))]
    pub fn as_ipaddr(&self) -> Option<IpAddr> {
        match self.sockaddr_in()? {
            SockAddrIn::In(sa) => {
                Some(IpAddr::V4(Ipv4Addr::from(sa.sin_addr.s_addr.to_ne_bytes())))
            }
            SockAddrIn::In6(sa) => Some(IpAddr::V6(Ipv6Addr::from(sa.sin6_addr.s6_addr))),
        }
    }

    #[cfg(windows)]
    pub fn as_ipaddr(&self) -> Option<IpAddr> {
        match self.sockaddr_in()? {
            SockAddrIn::In(sa) => {
                // SAFETY: the union was copied from an AF_INET sockaddr, so S_addr is initialized
                let s_addr = unsafe { *sa.sin_addr.S_un.S_addr() };
                Some(IpAddr::V4(Ipv4Addr::from(s_addr.to_ne_bytes())))
            }
            SockAddrIn::In6(sa) => {
                // SAFETY: the union was copied from an AF_INET6 sockaddr, so Byte is initialized
                let s6_addr = unsafe { *sa.sin6_addr.u.Byte() };
                Some(IpAddr::V6(Ipv6Addr::from(s6_addr)))
            }
        }
    }

    pub fn sockaddr_in(&self) -> Option<SockAddrIn> {
        const AF_INET_U32: u32 = AF_INET as u32;
        const AF_INET6_U32: u32 = AF_INET6 as u32;

        match self.sa_family() {
            AF_INET_U32 => Some(SockAddrIn::In(self.sa_in())),
            AF_INET6_U32 => Some(SockAddrIn::In6(self.sa_in6())),
            _ => None,
        }
    }

    pub fn sa_family(&self) -> u32 {
        // SAFETY: new() checked that len covers the family field
        u32::from(unsafe { std::ptr::addr_of!((*self.inner.as_ptr()).sa_family).read_unaligned() })
    }

    /// The address as `sockaddr_in`, zero-filled past the declared length
    pub fn sa_in(&self) -> sockaddr_in {
        self.copy_prefix()
    }

    /// The address as `sockaddr_in6`, zero-filled past the declared length
    pub fn sa_in6(&self) -> sockaddr_in6 {
        self.copy_prefix()
    }

    /// The link-level address as `sockaddr_dl`, zero-filled past the declared length
    #[cfg(not(windows))]
    pub fn sa_dl(&self) -> libc::sockaddr_dl {
        self.copy_prefix()
    }

    /// Copy min(len, size_of::<T>()) bytes into a zeroed T
    fn copy_prefix<T: Copy>(&self) -> T {
        let n = self.len.min(std::mem::size_of::<T>());
        // SAFETY: T is a plain C sockaddr struct valid when zeroed; n bytes lie inside the
        // OS-declared length of the source, and the destination is a local of size_of::<T>()
        unsafe {
            let mut out = std::mem::MaybeUninit::<T>::zeroed();
            std::ptr::copy_nonoverlapping(
                self.inner.as_ptr().cast::<u8>(),
                out.as_mut_ptr().cast::<u8>(),
                n,
            );
            out.assume_init()
        }
    }
}

#[cfg(all(test, not(windows)))]
mod tests {
    use super::*;
    use libc::{sockaddr_in, sockaddr_in6, AF_LINK};

    /// A BSD sockaddr: sa_len, sa_family, then the body
    fn sa(len: u8, family: i32, body: &[u8]) -> Vec<u8> {
        let mut v = vec![len, family as u8];
        v.extend_from_slice(body);
        v
    }

    fn sa_in_v4(addr: [u8; 4]) -> Vec<u8> {
        let mut body = vec![0u8, 0]; // port
        body.extend_from_slice(&addr);
        body.extend_from_slice(&[0u8; 8]);
        sa(std::mem::size_of::<sockaddr_in>() as u8, AF_INET, &body)
    }

    fn sa_in_v6(addr: [u8; 16]) -> Vec<u8> {
        let mut body = vec![0u8; 6]; // port, flowinfo
        body.extend_from_slice(&addr);
        body.extend_from_slice(&[0u8; 4]);
        sa(std::mem::size_of::<sockaddr_in6>() as u8, AF_INET6, &body)
    }

    #[test]
    fn rejects_null_and_short() {
        assert!(SockAddr::new(std::ptr::null()).is_none());
        assert!(to_ipaddr(std::ptr::null()).is_none());
        let empty = [0u8, 0];
        assert!(SockAddr::new(empty.as_ptr().cast()).is_none(), "sa_len 0");
        let one = [1u8, AF_INET as u8];
        assert!(SockAddr::new(one.as_ptr().cast()).is_none(), "sa_len 1");
    }

    #[test]
    fn reads_full_addresses() {
        let v4 = sa_in_v4([192, 168, 1, 5]);
        assert_eq!(
            to_ipaddr(v4.as_ptr().cast()),
            Some(IpAddr::V4(Ipv4Addr::new(192, 168, 1, 5)))
        );
        let mut raw6 = [0u8; 16];
        raw6[0] = 0x20;
        raw6[1] = 0x01;
        raw6[15] = 1;
        let v6 = sa_in_v6(raw6);
        assert_eq!(
            to_ipaddr(v6.as_ptr().cast()),
            Some(IpAddr::V6(Ipv6Addr::from(raw6)))
        );
        let sa6 = SockAddr::new(v6.as_ptr().cast()).expect("sockaddr");
        assert_eq!(sa6.sa_family(), AF_INET6 as u32);
        assert_eq!(sa6.sa_in6().sin6_addr.s6_addr, raw6);
    }

    #[test]
    fn truncated_netmask_zero_fills() {
        // Exactly how macOS getifaddrs delivers a /8 mask: 5 bytes total
        let short = sa(5, AF_INET, &[0, 0, 255]);
        assert_eq!(
            to_ipaddr(short.as_ptr().cast()),
            Some(IpAddr::V4(Ipv4Addr::new(255, 0, 0, 0)))
        );
        let full = sa(7, AF_INET, &[0, 0, 255, 255, 255]);
        assert_eq!(
            to_ipaddr(full.as_ptr().cast()),
            Some(IpAddr::V4(Ipv4Addr::new(255, 255, 255, 0)))
        );
        // A family with no body reads as the unspecified address, never past the buffer
        let bare6 = sa(2, AF_INET6, &[]);
        assert_eq!(
            to_ipaddr(bare6.as_ptr().cast()),
            Some(IpAddr::V6(Ipv6Addr::UNSPECIFIED))
        );
    }

    #[test]
    fn declared_length_bounds_the_read() {
        // sa_len says 16 but only 6 bytes exist after it in a longer buffer of other data;
        // the copy still honours sa_len, so pad to prove the bytes past it are not used
        let mut buf = sa(6, AF_INET, &[0, 0, 10, 1]);
        buf.extend_from_slice(&[0xEE; 12]);
        assert_eq!(
            to_ipaddr(buf.as_ptr().cast()),
            Some(IpAddr::V4(Ipv4Addr::new(10, 1, 0, 0)))
        );
    }

    #[test]
    fn other_families_are_not_ip() {
        let mut dl = sa(20, AF_LINK, &[0u8; 18]);
        dl[2] = 7; // sdl_index low byte (host order u16)
        let sa_dl = SockAddr::new(dl.as_ptr().cast()).expect("sockaddr");
        assert!(sa_dl.as_ipaddr().is_none());
        assert!(sa_dl.sockaddr_in().is_none());
        assert_eq!(sa_dl.sa_family(), AF_LINK as u32);
        assert_eq!(sa_dl.sa_dl().sdl_index, 7);
        // A truncated link address yields index 0, which callers skip
        let short_dl = sa(2, AF_LINK, &[]);
        assert_eq!(
            SockAddr::new(short_dl.as_ptr().cast())
                .expect("sockaddr")
                .sa_dl()
                .sdl_index,
            0
        );
    }
}
