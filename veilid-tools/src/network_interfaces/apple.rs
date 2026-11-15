#![cfg(any(target_os = "macos", target_os = "ios"))]
#![allow(non_camel_case_types)]
use super::*;

#[cfg(target_os = "ios")]
use self::ios_route_abi::{
    rt_msghdr, RTAX_DST, RTAX_GATEWAY, RTAX_MAX, RTA_DST, RTA_GATEWAY, RTF_GATEWAY,
};
use libc::{
    freeifaddrs, getifaddrs, ifaddrs, ifreq, ioctl, sockaddr_in6, socket, sysctl, time_t, AF_INET,
    AF_INET6, AF_LINK, CTL_NET, IFF_BROADCAST, IFF_LOOPBACK, IFF_POINTOPOINT, IFF_RUNNING,
    IFNAMSIZ, NET_RT_FLAGS, PF_ROUTE, SIOCGIFMTU, SOCK_DGRAM,
};
#[cfg(target_os = "macos")]
use libc::{rt_msghdr, RTAX_DST, RTAX_GATEWAY, RTAX_MAX, RTA_DST, RTA_GATEWAY, RTF_GATEWAY};
#[cfg(target_os = "ios")]
mod ios_route_abi {
    use std::os::raw::{c_int, c_uchar, c_ushort};

    // Field order and widths match XNU net/route.h and libc's macOS rt_msghdr.
    #[derive(Clone, Copy)]
    #[repr(C)]
    pub(super) struct rt_metrics {
        rmx_locks: u32,
        rmx_mtu: u32,
        rmx_hopcount: u32,
        rmx_expire: i32,
        rmx_recvpipe: u32,
        rmx_sendpipe: u32,
        rmx_ssthresh: u32,
        rmx_rtt: u32,
        rmx_rttvar: u32,
        rmx_pksent: u32,
        rmx_filler: [u32; 4],
    }

    #[derive(Clone, Copy)]
    #[repr(C)]
    pub(super) struct rt_msghdr {
        pub(super) rtm_msglen: c_ushort,
        rtm_version: c_uchar,
        rtm_type: c_uchar,
        pub(super) rtm_index: c_ushort,
        rtm_flags: c_int,
        pub(super) rtm_addrs: c_int,
        rtm_pid: libc::pid_t,
        rtm_seq: c_int,
        rtm_errno: c_int,
        rtm_use: c_int,
        rtm_inits: u32,
        rtm_rmx: rt_metrics,
    }

    pub(super) const RTF_GATEWAY: c_int = 0x2;
    pub(super) const RTA_DST: c_int = 0x1;
    pub(super) const RTA_GATEWAY: c_int = 0x2;
    pub(super) const RTAX_DST: usize = 0;
    pub(super) const RTAX_GATEWAY: usize = 1;
    pub(super) const RTAX_MAX: usize = 8;
    // The parser starts the sockaddr run immediately after this XNU header.
    const _: [(); 92] = [(); core::mem::size_of::<rt_msghdr>()];
}
use sockaddr_tools::SockAddr;
use std::ffi::CStr;
use std::io;
use std::marker::PhantomData;
use std::os::fd::{AsRawFd, FromRawFd, OwnedFd};
use std::os::raw::{c_int, c_uchar, c_ulong, c_ushort};

/// Sysctl reads retried when the table grows between the size probe and the copy
const ROUTE_TABLE_READ_ATTEMPTS: usize = 3;
/// xnu route.c ROUNDUP aligns each sockaddr in a routing message to uint32_t
const ROUTE_SOCKADDR_ALIGN: usize = 4;

// xnu bsd/netinet6/in6_var.h IN6_IFF_* flags and address-info ioctls
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
/// Address was assigned dynamically (e.g. via DHCPv6)
const IN6_IFF_DYNAMIC: c_ushort = 0x0100;
/// RFC 7217 stable secured SLAAC; currently unused — kept for documentation
#[allow(dead_code)]
const IN6_IFF_SECURED: c_ushort = 0x0400;
/// CLAT46 synthesized address for RFC 6877 IPv4-as-a-Service over IPv6 (cellular)
const IN6_IFF_CLAT46: c_ushort = 0x1000;

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
    ifs6_atmfrag_rcvd: u64,
    ifs6_reass_fail: u64,
    ifs6_in_mcast: u64,
    ifs6_out_mcast: u64,
    ifs6_cantfoward_icmp6: u64,
    ifs6_addr_expiry_cnt: u64,
    ifs6_pfx_expiry_cnt: u64,
    ifs6_defrtr_expiry_cnt: u64,
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

#[derive(Clone, Copy)]
#[repr(C)]
union IfrIfru {
    ifru_addr: sockaddr_in6,
    ifru_dstaddr: sockaddr_in6,
    ifru_flags: c_int,
    ifru_flags6: c_int,
    ifru_metric: c_int,
    ifru_intval: c_int,
    ifru_data: *mut c_uchar, // caddr_t
    ifru_lifetime: in6_addrlifetime,
    ifru_stat: in6_ifstat,
    ifru_icmp6stat: icmp6_ifstat,
    ifru_scope_id: [u32; 16],
}

#[derive(Clone)]
#[repr(C)]
struct in6_ifreq {
    ifr_name: [c_uchar; IFNAMSIZ],
    ifr_ifru: IfrIfru,
}

// PANIC-CENSUS: infallible compile-time layout check; the ioctl request numbers encode this size
const _: () = assert!(mem::size_of::<in6_ifreq>() == 288);

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
#[derive(Clone)]
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

pub struct PlatformSupportApple {
    gateways: BTreeMap<u32, BTreeSet<IpAddr>>,
}

impl PlatformSupportApple {
    pub fn new() -> Self {
        PlatformSupportApple {
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
            // macOS/iOS surface CLAT at the address level via IN6_IFF_CLAT46, not as an interface flag
            is_clat: false,
        }
    }

    fn get_interface_mtu(ifname: &str) -> Option<u32> {
        let name = ifname.as_bytes();
        if name.len() >= IFNAMSIZ {
            return None;
        }
        // SAFETY: all-zero is a valid ifreq: a byte array and a zeroed union
        let mut req: ifreq = unsafe { core::mem::zeroed() };
        for (dst, src) in req.ifr_name.iter_mut().zip(name) {
            *dst = *src as libc::c_char;
        }

        // SAFETY: socket() has no pointer arguments
        let sock = unsafe { socket(AF_INET, SOCK_DGRAM, 0) };
        if sock < 0 {
            return None;
        }
        // SAFETY: sock is a fresh descriptor this guard now owns and closes on every path
        let sock = unsafe { OwnedFd::from_raw_fd(sock) };

        // SAFETY: req is a full ifreq, the size this ioctl encodes
        if unsafe { ioctl(sock.as_raw_fd(), SIOCGIFMTU, &mut req) } < 0 {
            return None;
        }
        // SAFETY: read after a successful SIOCGIFMTU, which fills this union member
        let mtu = unsafe { req.ifr_ifru.ifru_mtu };
        u32::try_from(mtu).ok()
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

        // RFC 4941 privacy address only; RFC 7217 secured SLAAC also has a non-zero
        // RA lifetime but is stable, so it must not be conflated
        let is_temporary = (flags & IN6_IFF_TEMPORARY) != 0;
        let is_dynamic = (flags & (IN6_IFF_DYNAMIC | IN6_IFF_AUTOCONF)) != 0;
        let is_preferred = (flags
            & (IN6_IFF_TENTATIVE | IN6_IFF_DUPLICATED | IN6_IFF_DETACHED | IN6_IFF_DEPRECATED))
            == 0;
        let is_clat = (flags & IN6_IFF_CLAT46) != 0;
        let expiration_secs = if expire == 0 {
            None
        } else {
            Some(expire as u64)
        };

        Ok(AddressFlags {
            is_temporary,
            is_dynamic,
            is_preferred,
            is_clat,
            expiration_secs,
        })
    }

    #[expect(clippy::unused_async)]
    pub async fn get_interfaces(
        &mut self,
        interfaces: &mut BTreeMap<String, NetworkInterface>,
    ) -> io::Result<()> {
        self.refresh_default_route_interfaces();
        let ifaddrs = IfAddrs::try_new()?;
        self.collect_interfaces(ifaddrs.iter(), interfaces, Self::get_address_flags);
        Ok(())
    }

    /// Pair every address in `entries` with its interface and gateways; `address_flags`
    /// answers the IPv6 flag ioctls for one address
    fn collect_interfaces<'a>(
        &self,
        entries: impl Iterator<Item = &'a ifaddrs> + Clone,
        interfaces: &mut BTreeMap<String, NetworkInterface>,
        address_flags: impl Fn(&str, sockaddr_in6) -> io::Result<AddressFlags>,
    ) {
        // Determine all the name to index mappings
        let mut name_to_index = BTreeMap::<String, u32>::new();
        for ifaddr in entries.clone() {
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
        for ifaddr in entries {
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
                net_intf.mtu = Self::get_interface_mtu(&ifname);
                // Add gateways if we have them
                if let Some(gateways) = self.gateways.get(&ifindex) {
                    for gateway in gateways {
                        net_intf.add_gateway(*gateway);
                    }
                }
                interfaces.insert(ifname.clone(), net_intf);
            }
            let intf = interfaces.get_mut(&ifname).unwrap_or_log();

            let mut flags = AddressFlags::default();

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
                    flags = match address_flags(&ifname, sa.sa_in6()) {
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
            intf.add_address(InterfaceAddress::new(intf_addr, flags));
        }
    }
}

/// The NUL-terminated name getifaddrs attaches to every entry
fn interface_name(ifaddr: &ifaddrs) -> String {
    // SAFETY: getifaddrs always sets ifa_name to a NUL-terminated string owned by the list
    unsafe { CStr::from_ptr(ifaddr.ifa_name) }
        .to_string_lossy()
        .into_owned()
}

/// One sysctl read of the RTF_GATEWAY route mib: a `None` buffer probes the size, `Some`
/// fills it; `len` carries the size in and the bytes written out
fn route_sysctl(buf: Option<&mut [u8]>, len: &mut usize) -> io::Result<()> {
    let mut mib = [CTL_NET, PF_ROUTE, 0, 0, NET_RT_FLAGS, RTF_GATEWAY];
    let oldp = buf.map_or(std::ptr::null_mut(), |b| b.as_mut_ptr().cast());
    // SAFETY: oldp is null or writable for *len bytes; mib and len outlive the call
    if unsafe {
        sysctl(
            mib.as_mut_ptr(),
            mib.len() as u32,
            oldp,
            len,
            std::ptr::null_mut(),
            0,
        )
    } < 0
    {
        return Err(io::Error::last_os_error());
    }
    Ok(())
}

/// The kernel's dump of every RTF_GATEWAY route as a run of `rt_msghdr` messages
fn read_route_table() -> io::Result<Vec<u8>> {
    read_route_table_with(route_sysctl)
}

/// Probe the size, then read; a table that grew in between is read again a bounded number of times
fn read_route_table_with(
    mut route_sysctl: impl FnMut(Option<&mut [u8]>, &mut usize) -> io::Result<()>,
) -> io::Result<Vec<u8>> {
    for _ in 0..ROUTE_TABLE_READ_ATTEMPTS {
        let mut len = 0usize;
        route_sysctl(None, &mut len)?;
        let mut buf = vec![0u8; len];
        if let Err(err) = route_sysctl(Some(&mut buf), &mut len) {
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
        if msglen < HEADER_LEN || msglen > msg.len() {
            break;
        }
        pos += msglen;

        if hdr.rtm_addrs & (RTA_DST | RTA_GATEWAY) != (RTA_DST | RTA_GATEWAY) {
            continue;
        }
        let Some((dst, gateway)) = dst_and_gateway(&msg[HEADER_LEN..msglen], hdr.rtm_addrs) else {
            continue;
        };
        if dst.is_unspecified() {
            gateways
                .entry(u32::from(hdr.rtm_index))
                .or_default()
                .insert(strip_embedded_scope(gateway));
        }
    }
}

/// The routing socket returns link-local gateways KAME-style, with the interface index in the
/// second segment (`fe80:13::` for `fe80::%utun0`); the index is already the message's rtm_index
fn strip_embedded_scope(addr: IpAddr) -> IpAddr {
    match addr {
        IpAddr::V6(v6) if ipv6addr_is_unicast_link_local(&v6) && v6.segments()[1] != 0 => {
            let mut segments = v6.segments();
            segments[1] = 0;
            IpAddr::V6(Ipv6Addr::from(segments))
        }
        _ => addr,
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

/// xnu route.c ROUNDUP: the stride between sockaddrs in a routing message
fn round_up_sockaddr(len: usize) -> usize {
    if len > 0 {
        1 + ((len - 1) | (ROUTE_SOCKADDR_ALIGN - 1))
    } else {
        ROUTE_SOCKADDR_ALIGN
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use libc::{sockaddr_in, sockaddr_in6, AF_INET};

    fn sa_v4(addr: [u8; 4]) -> Vec<u8> {
        let mut v = vec![mem::size_of::<sockaddr_in>() as u8, AF_INET as u8, 0, 0];
        v.extend_from_slice(&addr);
        v.extend_from_slice(&[0u8; 8]);
        v
    }

    fn sa_v6(addr: [u8; 16]) -> Vec<u8> {
        let mut v = vec![mem::size_of::<sockaddr_in6>() as u8, AF_INET6 as u8];
        v.extend_from_slice(&[0u8; 6]);
        v.extend_from_slice(&addr);
        v.extend_from_slice(&[0u8; 4]);
        v
    }

    fn sa_link(index: u16) -> Vec<u8> {
        let mut v = vec![20u8, AF_LINK as u8];
        v.extend_from_slice(&index.to_ne_bytes());
        v.extend_from_slice(&[0u8; 16]);
        v
    }

    /// One routing message: header with `rtm_addrs` naming the slots, then the sockaddrs
    fn route_msg(
        index: u16,
        rtm_addrs: c_int,
        addrs: &[&[u8]],
        msglen_override: Option<u16>,
    ) -> Vec<u8> {
        let mut body = Vec::new();
        for a in addrs {
            body.extend_from_slice(a);
            body.resize(round_up_sockaddr(body.len()), 0);
        }
        let total = mem::size_of::<rt_msghdr>() + body.len();
        // SAFETY: all-zero is a valid rt_msghdr (plain integers)
        let mut hdr: rt_msghdr = unsafe { mem::zeroed() };
        hdr.rtm_msglen = msglen_override.unwrap_or(total as u16);
        hdr.rtm_index = index;
        hdr.rtm_addrs = rtm_addrs;
        let mut out = Vec::with_capacity(total);
        // SAFETY: rt_msghdr is repr(C) plain data, read as bytes for its full size
        out.extend_from_slice(unsafe {
            std::slice::from_raw_parts(
                (&hdr as *const rt_msghdr).cast::<u8>(),
                mem::size_of::<rt_msghdr>(),
            )
        });
        out.extend_from_slice(&body);
        out
    }

    const DST_GW: c_int = RTA_DST | RTA_GATEWAY;

    #[test]
    fn round_up_matches_xnu() {
        assert_eq!(round_up_sockaddr(0), 4);
        assert_eq!(round_up_sockaddr(1), 4);
        assert_eq!(round_up_sockaddr(4), 4);
        assert_eq!(round_up_sockaddr(5), 8);
        assert_eq!(round_up_sockaddr(16), 16);
        assert_eq!(round_up_sockaddr(28), 28);
    }

    #[test]
    fn default_routes_are_collected_by_interface() {
        let mut table = route_msg(
            4,
            DST_GW,
            &[&sa_v4([0, 0, 0, 0]), &sa_v4([192, 168, 1, 1])],
            None,
        );
        table.extend(route_msg(
            4,
            DST_GW,
            &[&sa_v4([10, 0, 0, 0]), &sa_v4([10, 0, 0, 1])],
            None,
        ));
        table.extend(route_msg(
            9,
            DST_GW,
            &[
                &sa_v6([0; 16]),
                &sa_v6({
                    let mut g = [0u8; 16];
                    g[0] = 0xfe;
                    g[1] = 0x80;
                    g[15] = 1;
                    g
                }),
            ],
            None,
        ));
        let mut gateways = BTreeMap::new();
        parse_default_gateways(&table, &mut gateways);
        assert_eq!(gateways.len(), 2);
        assert_eq!(
            gateways[&4],
            BTreeSet::from([IpAddr::V4(Ipv4Addr::new(192, 168, 1, 1))])
        );
        assert!(gateways[&9].iter().all(|g| g.is_ipv6()));
    }

    #[test]
    fn link_local_gateway_drops_its_embedded_scope() {
        let table = route_msg(
            19,
            DST_GW,
            &[
                &sa_v6([0; 16]),
                &sa_v6({
                    let mut g = [0u8; 16];
                    g[0] = 0xfe;
                    g[1] = 0x80;
                    g[3] = 0x13;
                    g
                }),
            ],
            None,
        );
        let mut gateways = BTreeMap::new();
        parse_default_gateways(&table, &mut gateways);
        assert_eq!(
            gateways[&19],
            BTreeSet::from(["fe80::".parse::<IpAddr>().unwrap()])
        );
    }

    #[test]
    fn non_ip_gateway_does_not_stall_the_walk() {
        // A gateway that is a link address skips the message and the walk continues
        let mut table = route_msg(3, DST_GW, &[&sa_v4([0, 0, 0, 0]), &sa_link(3)], None);
        table.extend(route_msg(
            5,
            DST_GW,
            &[&sa_v4([0, 0, 0, 0]), &sa_v4([172, 16, 0, 1])],
            None,
        ));
        let mut gateways = BTreeMap::new();
        parse_default_gateways(&table, &mut gateways);
        assert_eq!(gateways.len(), 1);
        assert!(gateways[&5].contains(&IpAddr::V4(Ipv4Addr::new(172, 16, 0, 1))));
    }

    #[test]
    fn messages_without_both_slots_are_skipped() {
        let mut table = route_msg(1, RTA_DST, &[&sa_v4([0, 0, 0, 0])], None);
        table.extend(route_msg(2, RTA_GATEWAY, &[&sa_v4([1, 1, 1, 1])], None));
        let mut gateways = BTreeMap::new();
        parse_default_gateways(&table, &mut gateways);
        assert!(gateways.is_empty());
    }

    #[test]
    fn malformed_lengths_stop_without_panicking() {
        let good = route_msg(
            4,
            DST_GW,
            &[&sa_v4([0, 0, 0, 0]), &sa_v4([192, 168, 1, 1])],
            None,
        );
        let mut gateways = BTreeMap::new();

        // rtm_msglen past the end of the buffer
        let overrun = route_msg(
            4,
            DST_GW,
            &[&sa_v4([0, 0, 0, 0]), &sa_v4([192, 168, 1, 1])],
            Some(9000),
        );
        parse_default_gateways(&overrun, &mut gateways);
        assert!(gateways.is_empty());

        // rtm_msglen shorter than the header would loop forever if it advanced by it
        let tiny = route_msg(4, DST_GW, &[&sa_v4([0, 0, 0, 0])], Some(1));
        parse_default_gateways(&tiny, &mut gateways);
        assert!(gateways.is_empty());

        // A header-only tail after a good message
        let mut trailing = good.clone();
        trailing.extend_from_slice(&[0u8; 5]);
        parse_default_gateways(&trailing, &mut gateways);
        assert_eq!(gateways.len(), 1);

        // A sockaddr whose sa_len runs past its message
        gateways.clear();
        let mut bad_sa = sa_v4([0, 0, 0, 0]);
        bad_sa[0] = 200;
        let overlong = route_msg(4, DST_GW, &[&bad_sa, &sa_v4([192, 168, 1, 1])], None);
        parse_default_gateways(&overlong, &mut gateways);
        assert!(gateways.is_empty());

        // Empty and sub-header inputs
        parse_default_gateways(&[], &mut gateways);
        parse_default_gateways(&good[..10], &mut gateways);
        assert!(gateways.is_empty());
    }

    #[test]
    fn dst_and_gateway_walks_slots_in_order() {
        // Slots present: DST, GATEWAY, NETMASK; only the first two matter
        let mut addrs = Vec::new();
        for a in [
            sa_v4([0, 0, 0, 0]),
            sa_v4([10, 0, 0, 1]),
            sa_v4([255, 0, 0, 0]),
        ] {
            addrs.extend_from_slice(&a);
            addrs.resize(round_up_sockaddr(addrs.len()), 0);
        }
        let got = dst_and_gateway(&addrs, RTA_DST | RTA_GATEWAY | libc::RTA_NETMASK);
        assert_eq!(
            got,
            Some((
                IpAddr::V4(Ipv4Addr::UNSPECIFIED),
                IpAddr::V4(Ipv4Addr::new(10, 0, 0, 1))
            ))
        );
        assert_eq!(dst_and_gateway(&[], DST_GW), None);
        assert_eq!(dst_and_gateway(&addrs[..3], DST_GW), None);
        assert_eq!(
            dst_and_gateway(&addrs, RTA_GATEWAY),
            None,
            "gateway without a destination"
        );
        assert_eq!(
            dst_and_gateway(&addrs, RTA_DST),
            None,
            "destination without a gateway"
        );
    }

    #[test]
    fn route_table_read_retries_only_on_enomem() {
        let table = route_msg(
            4,
            DST_GW,
            &[&sa_v4([0, 0, 0, 0]), &sa_v4([192, 168, 1, 1])],
            None,
        );

        // A table that grows once between probe and read is read again
        let mut calls = 0;
        let got = read_route_table_with(|buf, len| {
            calls += 1;
            match buf {
                None => {
                    *len = if calls == 1 { 8 } else { table.len() + 16 };
                    Ok(())
                }
                Some(b) if b.len() < table.len() => Err(io::Error::from_raw_os_error(libc::ENOMEM)),
                Some(b) => {
                    b[..table.len()].copy_from_slice(&table);
                    *len = table.len();
                    Ok(())
                }
            }
        })
        .expect("second read succeeds");
        assert_eq!(got, table, "truncated to what the kernel wrote");
        assert_eq!(calls, 4);

        // Growing forever gives up instead of spinning
        let err = read_route_table_with(|buf, len| {
            if buf.is_none() {
                *len = 8;
                Ok(())
            } else {
                Err(io::Error::from_raw_os_error(libc::ENOMEM))
            }
        })
        .expect_err("gives up");
        assert_eq!(err.kind(), io::ErrorKind::Other);

        // Any other failure is reported at once, from either call
        let err = read_route_table_with(|_, _| Err(io::Error::from_raw_os_error(libc::EPERM)))
            .expect_err("probe failure");
        assert_eq!(err.raw_os_error(), Some(libc::EPERM));
        let err = read_route_table_with(|buf, len| {
            if buf.is_none() {
                *len = 8;
                Ok(())
            } else {
                Err(io::Error::from_raw_os_error(libc::EINVAL))
            }
        })
        .expect_err("read failure");
        assert_eq!(err.raw_os_error(), Some(libc::EINVAL));
    }

    /// An `ifaddrs` entry pointing at test-owned buffers
    struct Entry {
        _name: std::ffi::CString,
        _bufs: Vec<Vec<u8>>,
        raw: ifaddrs,
    }

    fn entry(
        name: &str,
        flags: u32,
        addr: Option<Vec<u8>>,
        netmask: Option<Vec<u8>>,
        dst: Option<Vec<u8>>,
    ) -> Entry {
        let name = std::ffi::CString::new(name).expect("name");
        // SAFETY: all-zero is a valid ifaddrs (null pointers and zero flags)
        let mut raw: ifaddrs = unsafe { mem::zeroed() };
        raw.ifa_name = name.as_ptr().cast_mut();
        raw.ifa_flags = flags;
        let mut bufs = Vec::new();
        let mut keep = |b: Option<Vec<u8>>| -> *mut libc::sockaddr {
            match b {
                Some(b) => {
                    bufs.push(b);
                    bufs.last().expect("pushed").as_ptr().cast_mut().cast()
                }
                None => std::ptr::null_mut(),
            }
        };
        raw.ifa_addr = keep(addr);
        raw.ifa_netmask = keep(netmask);
        raw.ifa_dstaddr = keep(dst);
        Entry {
            _name: name,
            _bufs: bufs,
            raw,
        }
    }

    #[test]
    fn interfaces_are_built_from_entries() {
        let mut platform = PlatformSupportApple::new();
        platform
            .gateways
            .entry(4)
            .or_default()
            .insert(IpAddr::V4(Ipv4Addr::new(192, 168, 1, 1)));
        let broadcast_flag = IFF_BROADCAST as u32;
        let entries = [
            entry("en0", 0, Some(sa_link(4)), None, None),
            entry(
                "en0",
                broadcast_flag,
                Some(sa_v4([192, 168, 1, 5])),
                Some(sa_v4([255, 255, 255, 0])),
                Some(sa_v4([192, 168, 1, 255])),
            ),
            // No netmask and no broadcast flag
            entry(
                "en0",
                0,
                Some(sa_v4([10, 0, 0, 5])),
                None,
                Some(sa_v4([10, 0, 0, 255])),
            ),
            // Broadcast flag but no destination address
            entry(
                "en0",
                broadcast_flag,
                Some(sa_v4([10, 1, 0, 5])),
                Some(sa_v4([255, 0, 0, 0])),
                None,
            ),
            entry(
                "en0",
                0,
                Some(sa_v6({
                    let mut a = [0u8; 16];
                    a[0] = 0xfd;
                    a[15] = 5;
                    a
                })),
                Some(sa_v6([0xff; 16])),
                None,
            ),
            // No netmask on an IPv6 address
            entry(
                "en0",
                0,
                Some(sa_v6({
                    let mut a = [0u8; 16];
                    a[0] = 0xfe;
                    a[1] = 0x80;
                    a[15] = 9;
                    a
                })),
                None,
                None,
            ),
            // Link entry with index 0 is unusable, so its address has no interface
            entry("utun9", 0, Some(sa_link(0)), None, None),
            entry("utun9", 0, Some(sa_v4([10, 9, 9, 9])), None, None),
            // Address without any link entry
            entry("gif0", 0, Some(sa_v4([10, 8, 8, 8])), None, None),
            // Entries with no address at all
            entry("en0", 0, None, None, None),
        ];
        let mut interfaces = BTreeMap::new();
        platform.collect_interfaces(
            entries.iter().map(|e| &e.raw),
            &mut interfaces,
            |name, addr| {
                assert_eq!(name, "en0");
                if addr.sin6_addr.s6_addr[0] == 0xfe {
                    return Err(io::Error::from_raw_os_error(libc::EADDRNOTAVAIL));
                }
                Ok(AddressFlags {
                    is_temporary: true,
                    ..AddressFlags::default()
                })
            },
        );

        assert_eq!(interfaces.keys().collect::<Vec<_>>(), vec!["en0"]);
        let en0 = &interfaces["en0"];
        assert_eq!(en0.gateways(), &[IpAddr::V4(Ipv4Addr::new(192, 168, 1, 1))]);
        let v6_ip = Ipv6Addr::from({
            let mut a = [0u8; 16];
            a[0] = 0xfd;
            a[15] = 5;
            a
        });
        let addrs: BTreeSet<IfAddr> = en0.addrs.iter().map(|a| a.if_addr().clone()).collect();
        assert_eq!(
            addrs,
            BTreeSet::from([
                IfAddr::V4(Ifv4Addr {
                    ip: Ipv4Addr::new(192, 168, 1, 5),
                    netmask: Ipv4Addr::new(255, 255, 255, 0),
                    broadcast: Some(Ipv4Addr::new(192, 168, 1, 255)),
                }),
                IfAddr::V4(Ifv4Addr {
                    ip: Ipv4Addr::new(10, 0, 0, 5),
                    netmask: Ipv4Addr::UNSPECIFIED,
                    broadcast: None,
                }),
                IfAddr::V4(Ifv4Addr {
                    ip: Ipv4Addr::new(10, 1, 0, 5),
                    netmask: Ipv4Addr::new(255, 0, 0, 0),
                    broadcast: None,
                }),
                IfAddr::V6(Ifv6Addr {
                    ip: v6_ip,
                    netmask: Ipv6Addr::from([0xff; 16]),
                    broadcast: None,
                }),
            ])
        );
        let v6 = en0
            .addrs
            .iter()
            .find(|a| a.if_addr().ip() == IpAddr::V6(v6_ip))
            .expect("v6 address kept");
        assert!(v6.is_temporary());
    }

    #[test]
    fn live_route_table_and_interfaces() {
        let table = read_route_table().expect("route table");
        let mut gateways = BTreeMap::new();
        parse_default_gateways(&table, &mut gateways);
        let ifaddrs = IfAddrs::try_new().expect("getifaddrs");
        let names: Vec<String> = ifaddrs.iter().map(interface_name).collect();
        assert!(!names.is_empty());
        assert!(names.iter().all(|n| !n.is_empty()));
    }

    #[test]
    fn address_flags_reject_bad_names() {
        assert!(in6_ifreq::from_name("this-name-is-far-too-long").is_err());
        assert!(in6_ifreq::from_name("en\0zero").is_err());
        assert!(in6_ifreq::from_name("en0").is_ok());
        // SAFETY: all-zero is a valid sockaddr_in6
        let zero: sockaddr_in6 = unsafe { mem::zeroed() };
        assert!(
            PlatformSupportApple::get_address_flags("this-name-is-far-too-long", zero).is_err()
        );
        assert!(PlatformSupportApple::get_address_flags("nosuchif99", zero).is_err());
    }
}
