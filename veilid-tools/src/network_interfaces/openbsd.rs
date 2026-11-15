#![cfg(target_os = "openbsd")]
#![allow(non_camel_case_types)]
use super::*;

use libc::{
    freeifaddrs, getifaddrs, ifaddrs, ioctl, pid_t, sockaddr_in6, socket, sysctl, time_t, AF_INET6,
    AF_LINK, CTL_NET, IFF_BROADCAST, IFF_LOOPBACK, IFF_POINTOPOINT, IFF_RUNNING, IFNAMSIZ,
    NET_RT_FLAGS, PF_ROUTE, RTAX_DST, RTAX_GATEWAY, RTAX_MAX, RTA_DST, RTA_GATEWAY, RTF_GATEWAY,
    SOCK_DGRAM,
};
use sockaddr_tools::SockAddr;
use std::ffi::CStr;
use std::io;
use std::marker::PhantomData;
use std::os::fd::{AsRawFd, FromRawFd, OwnedFd};
use std::os::raw::{c_int, c_long, c_short, c_uchar, c_ulong, c_ushort};

/// Sysctl reads retried when the table grows between the size probe and the copy
const ROUTE_TABLE_READ_ATTEMPTS: usize = 3;
/// sys/net/route.c ROUNDUP aligns each sockaddr in a routing message to sizeof(long)
const ROUTE_SOCKADDR_ALIGN: usize = mem::size_of::<c_long>();

// sys/netinet6/in6_var.h IN6_IFF_* flags and address-info ioctls, _IOWR('i', n, in6_ifreq)
/// Get IPv6 address flags
const SIOCGIFAFLAG_IN6: c_ulong = 0xC1206949;
/// Get IPv6 address lifetime (preferred/valid/expire)
const SIOCGIFALIFETIME_IN6: c_ulong = 0xC1206951;
/// Address is undergoing duplicate-address detection
const IN6_IFF_TENTATIVE: c_ushort = 0x0002;
/// Address has been detected as a duplicate
const IN6_IFF_DUPLICATED: c_ushort = 0x0004;
/// Network interface link is detached
const IN6_IFF_DETACHED: c_ushort = 0x0008;
/// Address has reached the end of its preferred lifetime (still valid but discouraged)
const IN6_IFF_DEPRECATED: c_ushort = 0x0010;
/// Address was assigned via IPv6 stateless auto-configuration (SLAAC)
const IN6_IFF_AUTOCONF: c_ushort = 0x0040;
/// RFC 4941 privacy / temporary address (rotates)
const IN6_IFF_TEMPORARY: c_ushort = 0x0080;

macro_rules! set_name {
    ($name_field:expr, $name_str:expr) => {{
        let name_c = &::std::ffi::CString::new($name_str.to_owned()).map_err(|_| {
            ::std::io::Error::new(
                ::std::io::ErrorKind::InvalidInput,
                "malformed interface name",
            )
        })?;
        let name_slice = name_c.as_bytes_with_nul();
        if name_slice.len() > IFNAMSIZ {
            return Err(io::Error::new(::std::io::ErrorKind::InvalidInput, ""));
        }
        $name_field[..name_slice.len()].clone_from_slice(name_slice);

        Ok(())
    }};
}

/// sys/net/route.h
#[derive(Clone, Copy, Debug)]
#[repr(C)]
struct rt_msghdr {
    rtm_msglen: c_ushort,
    rtm_version: c_uchar,
    rtm_type: c_uchar,
    rtm_hdrlen: c_ushort,
    rtm_index: c_ushort,
    rtm_tableid: c_ushort,
    rtm_priority: c_uchar,
    rtm_mpls: c_uchar,
    rtm_addrs: c_int,
    rtm_flags: c_int,
    rtm_fmask: c_int,
    rtm_pid: pid_t,
    rtm_seq: c_int,
    rtm_errno: c_int,
    rtm_inits: u32,
    rtm_rmx: rt_metrics,
}

#[derive(Clone, Copy, Debug)]
#[repr(C)]
struct rt_metrics {
    rmx_pksent: u64,
    rmx_expire: i64,
    rmx_locks: u32,
    rmx_mtu: u32,
    rmx_refcnt: u32,
    rmx_hopcount: u32,
    rmx_recvpipe: u32,
    rmx_sendpipe: u32,
    rmx_ssthresh: u32,
    rmx_rtt: u32,
    rmx_rttvar: u32,
    rmx_pad: u32,
}

#[derive(Clone, Copy, Debug)]
#[repr(C)]
struct in6_addrlifetime {
    ia6t_expire: time_t,
    ia6t_preferred: time_t,
    ia6t_vltime: u32,
    ia6t_pltime: u32,
}

#[derive(Clone, Copy, Debug)]
#[repr(C)]
struct in6_ifstat {
    ifs6_in_receive: u64,
    ifs6_in_hdrerr: u64,
    ifs6_in_toobig: u64,
    ifs6_in_noroute: u64,
    ifs6_in_addrerr: u64,
    ifs6_in_protounknown: u64,
    ifs6_in_truncated: u64,
    ifs6_in_discard: u64,
    ifs6_in_deliver: u64,
    ifs6_out_forward: u64,
    ifs6_out_request: u64,
    ifs6_out_discard: u64,
    ifs6_out_fragok: u64,
    ifs6_out_fragfail: u64,
    ifs6_out_fragcreat: u64,
    ifs6_reass_reqd: u64,
    ifs6_reass_ok: u64,
    ifs6_reass_fail: u64,
    ifs6_in_mcast: u64,
    ifs6_out_mcast: u64,
}

#[derive(Clone, Copy, Debug)]
#[repr(C)]
struct icmp6_ifstat {
    ifs6_in_msg: u64,
    ifs6_in_error: u64,
    ifs6_in_dstunreach: u64,
    ifs6_in_adminprohib: u64,
    ifs6_in_timeexceed: u64,
    ifs6_in_paramprob: u64,
    ifs6_in_pkttoobig: u64,
    ifs6_in_echo: u64,
    ifs6_in_echoreply: u64,
    ifs6_in_routersolicit: u64,
    ifs6_in_routeradvert: u64,
    ifs6_in_neighborsolicit: u64,
    ifs6_in_neighboradvert: u64,
    ifs6_in_redirect: u64,
    ifs6_in_mldquery: u64,
    ifs6_in_mldreport: u64,
    ifs6_in_mlddone: u64,
    ifs6_out_msg: u64,
    ifs6_out_error: u64,
    ifs6_out_dstunreach: u64,
    ifs6_out_adminprohib: u64,
    ifs6_out_timeexceed: u64,
    ifs6_out_paramprob: u64,
    ifs6_out_pkttoobig: u64,
    ifs6_out_echo: u64,
    ifs6_out_echoreply: u64,
    ifs6_out_routersolicit: u64,
    ifs6_out_routeradvert: u64,
    ifs6_out_neighborsolicit: u64,
    ifs6_out_neighboradvert: u64,
    ifs6_out_redirect: u64,
    ifs6_out_mldquery: u64,
    ifs6_out_mldreport: u64,
    ifs6_out_mlddone: u64,
}

/// The union is 272 bytes (icmp6_ifstat), so in6_ifreq is the 288 the ioctls encode
#[derive(Clone, Copy)]
#[repr(C)]
union IfrIfru {
    ifru_addr: sockaddr_in6,
    ifru_dstaddr: sockaddr_in6,
    ifru_flags: c_short,
    ifru_flags6: c_int,
    ifru_metric: c_int,
    ifru_data: *mut c_uchar, // caddr_t
    ifru_lifetime: in6_addrlifetime,
    ifru_stat: in6_ifstat,
    ifru_icmp6stat: icmp6_ifstat,
}

#[derive(Clone)]
#[repr(C)]
struct in6_ifreq {
    ifr_name: [c_uchar; IFNAMSIZ],
    ifr_ifru: IfrIfru,
}

// PANIC-CENSUS: infallible compile-time layout check; the ioctl request numbers encode this size
const _: () = assert!(mem::size_of::<in6_ifreq>() == 288);
// PANIC-CENSUS: infallible compile-time layout check against sys/net/route.h
const _: () = assert!(mem::size_of::<rt_msghdr>() == 96);

impl in6_ifreq {
    pub fn from_name(name: &str) -> io::Result<Self> {
        // SAFETY: all-zero is a valid in6_ifreq: ints, byte arrays, a null pointer and a zeroed union
        let mut req: in6_ifreq = unsafe { mem::zeroed() };
        req.set_name(name)?;
        Ok(req)
    }
    pub fn set_name(&mut self, name: &str) -> io::Result<()> {
        set_name!(self.ifr_name, name)
    }
    pub fn set_addr(&mut self, addr: sockaddr_in6) {
        self.ifr_ifru.ifru_addr = addr;
    }
    pub fn get_flags6(&self) -> c_ushort {
        // SAFETY: read after a successful SIOCGIFAFLAG_IN6, which fills this union member
        unsafe { self.ifr_ifru.ifru_flags6 as c_ushort }
    }
    pub fn get_ia6t_expire(&self) -> time_t {
        // SAFETY: read after a successful SIOCGIFALIFETIME_IN6, which fills this union member
        unsafe { self.ifr_ifru.ifru_lifetime.ia6t_expire as time_t }
    }
}

pub fn do_broadcast(ifaddr: &ifaddrs) -> Option<IpAddr> {
    sockaddr_tools::to_ipaddr(ifaddr.ifa_dstaddr)
}

///////////////////////////////////////////////////

pub struct IfAddrs {
    inner: *mut ifaddrs,
}

impl IfAddrs {
    pub fn try_new() -> io::Result<Self> {
        let mut ifaddrs = std::ptr::null_mut();
        // SAFETY: getifaddrs writes a list head (or leaves null) into the out-pointer
        if -1 == unsafe { getifaddrs(&mut ifaddrs) } {
            return Err(io::Error::last_os_error());
        }
        Ok(Self { inner: ifaddrs })
    }

    pub fn iter(&self) -> IfAddrsIterator<'_> {
        IfAddrsIterator {
            next: self.inner,
            _owner: PhantomData,
        }
    }
}

impl Drop for IfAddrs {
    fn drop(&mut self) {
        // SAFETY: inner came from getifaddrs and is freed exactly once; every borrowed
        // item carries this owner's lifetime
        unsafe { freeifaddrs(self.inner) };
    }
}

/// Borrows list entries for as long as the owning `IfAddrs` lives
pub struct IfAddrsIterator<'a> {
    next: *const ifaddrs,
    _owner: PhantomData<&'a IfAddrs>,
}

impl<'a> Iterator for IfAddrsIterator<'a> {
    type Item = &'a ifaddrs;

    fn next(&mut self) -> Option<Self::Item> {
        // SAFETY: next is null or a live entry of the list the owner holds
        let entry = unsafe { self.next.as_ref()? };
        self.next = entry.ifa_next;
        Some(entry)
    }
}

///////////////////////////////////////////////////

pub struct PlatformSupportOpenBSD {
    gateways: BTreeMap<u32, BTreeSet<IpAddr>>,
}

impl PlatformSupportOpenBSD {
    pub fn new() -> Self {
        PlatformSupportOpenBSD {
            gateways: BTreeMap::new(),
        }
    }

    fn refresh_default_route_interfaces(&mut self) {
        let table = match read_route_table() {
            Ok(v) => v,
            Err(e) => {
                error!("Unable to read routing table: {}", e);
                return;
            }
        };
        self.gateways.clear();
        parse_default_gateways(&table, &mut self.gateways);
    }

    fn get_interface_flags(&self, flags: c_int) -> InterfaceFlags {
        InterfaceFlags {
            is_loopback: (flags & IFF_LOOPBACK) != 0,
            is_running: (flags & IFF_RUNNING) != 0,
            is_point_to_point: (flags & IFF_POINTOPOINT) != 0,
            // OpenBSD has no CLAT
            is_clat: false,
        }
    }

    fn get_address_flags(ifname: &str, addr: sockaddr_in6) -> io::Result<AddressFlags> {
        let mut flags_req = in6_ifreq::from_name(ifname)?;
        flags_req.set_addr(addr);
        let mut lifetime_req = flags_req.clone();

        // SAFETY: socket() has no pointer arguments
        let sock = unsafe { socket(AF_INET6, SOCK_DGRAM, 0) };
        if sock < 0 {
            return Err(io::Error::last_os_error());
        }
        // SAFETY: sock is a fresh descriptor this guard now owns and closes on every path
        let sock = unsafe { OwnedFd::from_raw_fd(sock) };

        // SAFETY: flags_req is a full in6_ifreq, the size this ioctl encodes
        if unsafe { ioctl(sock.as_raw_fd(), SIOCGIFAFLAG_IN6, &mut flags_req) } < 0 {
            return Err(io::Error::last_os_error());
        }
        // SAFETY: lifetime_req is a full in6_ifreq, the size this ioctl encodes
        if unsafe { ioctl(sock.as_raw_fd(), SIOCGIFALIFETIME_IN6, &mut lifetime_req) } < 0 {
            return Err(io::Error::last_os_error());
        }
        let flags = flags_req.get_flags6();
        let expire = lifetime_req.get_ia6t_expire();

        let is_temporary = (flags & IN6_IFF_TEMPORARY) != 0;
        let is_dynamic = (flags & IN6_IFF_AUTOCONF) != 0;
        let is_preferred = (flags
            & (IN6_IFF_TENTATIVE | IN6_IFF_DUPLICATED | IN6_IFF_DETACHED | IN6_IFF_DEPRECATED))
            == 0;
        let expiration_secs = if expire == 0 {
            None
        } else {
            Some(expire as u64)
        };

        Ok(AddressFlags {
            is_temporary,
            is_dynamic,
            is_preferred,
            // OpenBSD has no CLAT address flag
            is_clat: false,
            expiration_secs,
        })
    }

    #[expect(clippy::unused_async)]
    pub async fn get_interfaces(
        &mut self,
        interfaces: &mut BTreeMap<String, NetworkInterface>,
    ) -> io::Result<()> {
        self.refresh_default_route_interfaces();

        // Ask for all the addresses we have
        let ifaddrs = IfAddrs::try_new()?;

        // Determine all the name to index mappings
        let mut name_to_index = BTreeMap::<String, u32>::new();
        for ifaddr in ifaddrs.iter() {
            let Some(sa) = SockAddr::new(ifaddr.ifa_addr) else {
                continue;
            };
            if sa.sa_family() != AF_LINK as u32 {
                continue;
            }
            let index = u32::from(sa.sa_dl().sdl_index);
            if index == 0 {
                continue;
            }
            name_to_index.insert(interface_name(ifaddr), index);
        }

        // Iterate all interface addresses, pair them with their gateways and add them to the interfaces map
        for ifaddr in ifaddrs.iter() {
            // Ensure we're getting an ip address here otherwise we'll skip it
            let Some(sa) = SockAddr::new(ifaddr.ifa_addr) else {
                continue;
            };
            let Some(ip_addr) = sa.as_ipaddr() else {
                continue;
            };

            let ifname = interface_name(ifaddr);

            // Get the interface index
            let Some(ifindex) = name_to_index.get(&ifname).copied() else {
                continue;
            };

            // Map the name to a NetworkInterface
            if !interfaces.contains_key(&ifname) {
                // If we have no NetworkInterface yet, make one
                let flags = self.get_interface_flags(ifaddr.ifa_flags as c_int);
                let mut net_intf = NetworkInterface::new(ifname.clone(), flags);
                // Add gateways if we have them
                if let Some(gateways) = self.gateways.get(&ifindex) {
                    for gateway in gateways {
                        net_intf.add_gateway(*gateway);
                    }
                }
                interfaces.insert(ifname.clone(), net_intf);
            }
            let intf = interfaces.get_mut(&ifname).unwrap_or_log();

            let mut address_flags = AddressFlags::default();

            let intf_addr = match ip_addr {
                IpAddr::V4(ipv4_addr) => {
                    let netmask = match sockaddr_tools::to_ipaddr(ifaddr.ifa_netmask) {
                        Some(IpAddr::V4(netmask)) => netmask,
                        _ => Ipv4Addr::new(0, 0, 0, 0),
                    };
                    let broadcast = if (ifaddr.ifa_flags & (IFF_BROADCAST as u32)) != 0 {
                        match do_broadcast(ifaddr) {
                            Some(IpAddr::V4(broadcast)) => Some(broadcast),
                            _ => None,
                        }
                    } else {
                        None
                    };

                    IfAddr::V4(Ifv4Addr {
                        ip: ipv4_addr,
                        netmask,
                        broadcast,
                    })
                }
                IpAddr::V6(ipv6_addr) => {
                    let netmask = match sockaddr_tools::to_ipaddr(ifaddr.ifa_netmask) {
                        Some(IpAddr::V6(netmask)) => netmask,
                        _ => Ipv6Addr::new(0, 0, 0, 0, 0, 0, 0, 0),
                    };

                    // Get address flags for ipv6
                    address_flags = match Self::get_address_flags(&ifname, sa.sa_in6()) {
                        Ok(v) => v,
                        Err(e) => {
                            debug!(
                                "failed to get address flags for ifname={}, ifaddr={:?} : {}",
                                ifname, ifaddr.ifa_addr, e
                            );
                            continue;
                        }
                    };

                    IfAddr::V6(Ifv6Addr {
                        ip: ipv6_addr,
                        netmask,
                        broadcast: None,
                    })
                }
            };

            // Add to the list
            intf.add_address(InterfaceAddress::new(intf_addr, address_flags));
        }

        Ok(())
    }
}

/// The NUL-terminated name getifaddrs attaches to every entry
fn interface_name(ifaddr: &ifaddrs) -> String {
    // SAFETY: getifaddrs always sets ifa_name to a NUL-terminated string owned by the list
    unsafe { CStr::from_ptr(ifaddr.ifa_name) }
        .to_string_lossy()
        .into_owned()
}

/// The kernel's dump of every RTF_GATEWAY route as a run of `rt_msghdr` messages
fn read_route_table() -> io::Result<Vec<u8>> {
    let mut mib = [CTL_NET, PF_ROUTE, 0, 0, NET_RT_FLAGS, RTF_GATEWAY];
    for _ in 0..ROUTE_TABLE_READ_ATTEMPTS {
        let mut len = 0usize;
        // SAFETY: a null oldp asks only for the needed size; mib and len outlive the call
        if unsafe {
            sysctl(
                mib.as_mut_ptr(),
                mib.len() as u32,
                std::ptr::null_mut(),
                &mut len,
                std::ptr::null_mut(),
                0,
            )
        } < 0
        {
            return Err(io::Error::last_os_error());
        }
        let mut buf = vec![0u8; len];
        // SAFETY: buf is writable for len bytes and the kernel lowers len to what it wrote
        if unsafe {
            sysctl(
                mib.as_mut_ptr(),
                mib.len() as u32,
                buf.as_mut_ptr().cast(),
                &mut len,
                std::ptr::null_mut(),
                0,
            )
        } < 0
        {
            let err = io::Error::last_os_error();
            // Only a table that grew since the probe is worth another attempt
            if err.raw_os_error() == Some(libc::ENOMEM) {
                continue;
            }
            return Err(err);
        }
        buf.truncate(len);
        return Ok(buf);
    }
    Err(io_error_other!("routing table kept changing size"))
}

/// Collect the gateway of every default route, keyed by interface index
fn parse_default_gateways(table: &[u8], gateways: &mut BTreeMap<u32, BTreeSet<IpAddr>>) {
    const HEADER_LEN: usize = mem::size_of::<rt_msghdr>();
    let mut pos = 0;
    while let Some(msg) = table.get(pos..) {
        if msg.len() < HEADER_LEN {
            break;
        }
        // SAFETY: msg holds a full header; read_unaligned tolerates the byte buffer's alignment
        let hdr = unsafe { msg.as_ptr().cast::<rt_msghdr>().read_unaligned() };
        let msglen = usize::from(hdr.rtm_msglen);
        let hdrlen = usize::from(hdr.rtm_hdrlen);
        if msglen < HEADER_LEN || msglen > msg.len() || hdrlen < HEADER_LEN || hdrlen > msglen {
            break;
        }
        pos += msglen;

        if hdr.rtm_addrs & (RTA_DST | RTA_GATEWAY) != (RTA_DST | RTA_GATEWAY) {
            continue;
        }
        let Some((dst, gateway)) = dst_and_gateway(&msg[hdrlen..msglen], hdr.rtm_addrs) else {
            continue;
        };
        if dst.is_unspecified() {
            gateways
                .entry(u32::from(hdr.rtm_index))
                .or_default()
                .insert(gateway);
        }
    }
}

/// The DST and GATEWAY addresses from a message's sockaddr run, if both are IP addresses
fn dst_and_gateway(mut addrs: &[u8], rtm_addrs: c_int) -> Option<(IpAddr, IpAddr)> {
    let mut dst = None;
    for slot in 0..RTAX_MAX {
        if rtm_addrs & (1 << slot) == 0 {
            continue;
        }
        let sa_len = usize::from(*addrs.first()?);
        if sa_len > addrs.len() {
            return None;
        }
        // The sockaddr lies within addrs, so SockAddr's sa_len-bounded reads stay in the message
        let ip = sockaddr_tools::to_ipaddr(addrs.as_ptr().cast());
        if slot == RTAX_DST {
            dst = Some(ip?);
        } else if slot == RTAX_GATEWAY {
            return Some((dst?, ip?));
        }
        addrs = addrs.get(round_up_sockaddr(sa_len)..).unwrap_or(&[]);
    }
    None
}

/// OpenBSD route.c ROUNDUP: the stride between sockaddrs in a routing message
fn round_up_sockaddr(len: usize) -> usize {
    if len > 0 {
        1 + ((len - 1) | (ROUTE_SOCKADDR_ALIGN - 1))
    } else {
        ROUTE_SOCKADDR_ALIGN
    }
}
