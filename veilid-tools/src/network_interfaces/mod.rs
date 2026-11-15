mod tools;

use crate::*;
use serde::*;

cfg_if::cfg_if! {
    if #[cfg(any(target_os = "linux", target_os = "android"))] {
        mod netlink;
        use self::netlink::PlatformSupportNetlink as PlatformSupport;
    } else if #[cfg(target_os = "windows")] {
        mod windows;
        mod sockaddr_tools;
        use self::windows::PlatformSupportWindows as PlatformSupport;
    } else if #[cfg(any(target_os = "macos", target_os = "ios"))] {
        mod apple;
        mod sockaddr_tools;
        use self::apple::PlatformSupportApple as PlatformSupport;
    } else if #[cfg(target_os = "openbsd")] {
        mod openbsd;
        mod sockaddr_tools;
        use self::openbsd::PlatformSupportOpenBSD as PlatformSupport;
    } else if #[cfg(all(target_arch = "wasm32", target_os = "unknown"))] {
        mod wasm;
        use self::wasm::PlatformSupportWasm as PlatformSupport;
    } else {
        compile_error!("No network interfaces support for this platform!");
    }
}

/// An interface address that is either IPv4 or IPv6, with its netmask and broadcast address.
#[derive(Debug, PartialEq, Eq, Ord, PartialOrd, Hash, Clone, Serialize, Deserialize)]
pub enum IfAddr {
    /// An IPv4 interface address.
    V4(Ifv4Addr),
    /// An IPv6 interface address.
    V6(Ifv6Addr),
}

impl IfAddr {
    /// The IP address.
    #[must_use]
    pub fn ip(&self) -> IpAddr {
        match *self {
            IfAddr::V4(ref ifv4_addr) => IpAddr::V4(ifv4_addr.ip),
            IfAddr::V6(ref ifv6_addr) => IpAddr::V6(ifv6_addr.ip),
        }
    }
    /// The netmask.
    #[must_use]
    pub fn netmask(&self) -> IpAddr {
        match *self {
            IfAddr::V4(ref ifv4_addr) => IpAddr::V4(ifv4_addr.netmask),
            IfAddr::V6(ref ifv6_addr) => IpAddr::V6(ifv6_addr.netmask),
        }
    }
    /// The broadcast address, if one is set.
    pub fn broadcast(&self) -> Option<IpAddr> {
        match *self {
            IfAddr::V4(ref ifv4_addr) => ifv4_addr.broadcast.map(IpAddr::V4),
            IfAddr::V6(ref ifv6_addr) => ifv6_addr.broadcast.map(IpAddr::V6),
        }
    }

    /// The network address, with the host bits masked off by the netmask.
    #[must_use]
    pub fn network(&self) -> IfAddr {
        match *self {
            IfAddr::V4(ref ifv4_addr) => IfAddr::V4(ifv4_addr.network()),
            IfAddr::V6(ref ifv6_addr) => IfAddr::V6(ifv6_addr.network()),
        }
    }

    /// Whether `addr` is in the same subnet as this address. False across address families.
    #[must_use]
    pub fn contains_address(&self, addr: IpAddr) -> bool {
        match *self {
            IfAddr::V4(ref ifv4_addr) => match addr {
                IpAddr::V4(ipv4_addr) => ifv4_addr.contains_address(ipv4_addr),
                IpAddr::V6(_consensus_count) => false,
            },
            IfAddr::V6(ref ifv6_addr) => match addr {
                IpAddr::V4(_) => false,
                IpAddr::V6(ipv6_addr) => ifv6_addr.contains_address(ipv6_addr),
            },
        }
    }
}

impl fmt::Display for IfAddr {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match *self {
            IfAddr::V4(ref ifv4_addr) => write!(f, "{}", f.to_string(ifv4_addr)),
            IfAddr::V6(ref ifv6_addr) => write!(f, "{}", f.to_string(ifv6_addr)),
        }
    }
}

/// Details about the ipv4 address of an interface on this host.
#[derive(Debug, PartialEq, Eq, Ord, PartialOrd, Hash, Clone, Serialize, Deserialize)]
pub struct Ifv4Addr {
    /// The IP address of the interface.
    pub ip: Ipv4Addr,
    /// The netmask of the interface.
    pub netmask: Ipv4Addr,
    /// The broadcast address of the interface.
    pub broadcast: Option<Ipv4Addr>,
}

impl fmt::Display for Ifv4Addr {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{}/{}",
            self.ip,
            if let Some(prefix_len) = self.prefix_len() {
                format!("{}", prefix_len)
            } else {
                format!("(invalid netmask={})", self.netmask)
            },
        )
    }
}

fn ipv4_netmask_to_prefix_len(netmask: Ipv4Addr) -> Option<u8> {
    let netmask_u32 = netmask.to_bits();
    let prefix_len = netmask_u32.leading_ones() as u8;
    let suffix_len = netmask_u32.trailing_zeros() as u8;

    if suffix_len != 32 - prefix_len {
        None
    } else {
        Some(prefix_len)
    }
}

impl Ifv4Addr {
    /// Convert ip address to the address of the network itself by applying the netmask
    #[must_use]
    pub fn network(&self) -> Ifv4Addr {
        let v4 = self.ip.octets();
        let v4mask = self.netmask.octets();
        let ip = Ipv4Addr::new(
            v4[0] & v4mask[0],
            v4[1] & v4mask[1],
            v4[2] & v4mask[2],
            v4[3] & v4mask[3],
        );
        Ifv4Addr {
            ip,
            netmask: self.netmask,
            broadcast: self.broadcast,
        }
    }

    /// Whether `addr` is in this address's subnet.
    #[must_use]
    pub fn contains_address(&self, addr: Ipv4Addr) -> bool {
        let v4mask = self.netmask.octets();

        let self_v4 = self.ip.octets();
        let self_ip = Ipv4Addr::new(
            self_v4[0] & v4mask[0],
            self_v4[1] & v4mask[1],
            self_v4[2] & v4mask[2],
            self_v4[3] & v4mask[3],
        );

        let addr_v4 = addr.octets();
        let addr_ip = Ipv4Addr::new(
            addr_v4[0] & v4mask[0],
            addr_v4[1] & v4mask[1],
            addr_v4[2] & v4mask[2],
            addr_v4[3] & v4mask[3],
        );

        self_ip == addr_ip
    }

    /// CIDR prefix length of the netmask, or `None` if the netmask is not contiguous.
    #[must_use]
    pub fn prefix_len(&self) -> Option<u8> {
        ipv4_netmask_to_prefix_len(self.netmask)
    }
}

/// Details about the ipv6 address of an interface on this host.
#[derive(Debug, PartialEq, Eq, Ord, PartialOrd, Hash, Clone, Serialize, Deserialize)]
pub struct Ifv6Addr {
    /// The IP address of the interface.
    pub ip: Ipv6Addr,
    /// The netmask of the interface.
    pub netmask: Ipv6Addr,
    /// The broadcast address of the interface.
    pub broadcast: Option<Ipv6Addr>,
}

impl fmt::Display for Ifv6Addr {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{}/{}",
            self.ip,
            if let Some(prefix_len) = self.prefix_len() {
                format!("{}", prefix_len)
            } else {
                format!("(invalid netmask={})", self.netmask)
            },
        )
    }
}

impl Ifv6Addr {
    /// Convert ip address to the address of the network itself by applying the netmask
    #[must_use]
    pub fn network(&self) -> Ifv6Addr {
        let v6 = self.ip.segments();
        let v6mask = self.netmask.segments();
        let ip = Ipv6Addr::new(
            v6[0] & v6mask[0],
            v6[1] & v6mask[1],
            v6[2] & v6mask[2],
            v6[3] & v6mask[3],
            v6[4] & v6mask[4],
            v6[5] & v6mask[5],
            v6[6] & v6mask[6],
            v6[7] & v6mask[7],
        );
        Ifv6Addr {
            ip,
            netmask: self.netmask,
            broadcast: self.broadcast,
        }
    }

    /// Whether `addr` is in this address's subnet.
    #[must_use]
    pub fn contains_address(&self, addr: Ipv6Addr) -> bool {
        let v6mask = self.netmask.segments();

        let self_v6 = self.ip.segments();
        let self_ip = Ipv6Addr::new(
            self_v6[0] & v6mask[0],
            self_v6[1] & v6mask[1],
            self_v6[2] & v6mask[2],
            self_v6[3] & v6mask[3],
            self_v6[4] & v6mask[4],
            self_v6[5] & v6mask[5],
            self_v6[6] & v6mask[6],
            self_v6[7] & v6mask[7],
        );

        let addr_v6 = addr.segments();
        let addr_ip = Ipv6Addr::new(
            addr_v6[0] & v6mask[0],
            addr_v6[1] & v6mask[1],
            addr_v6[2] & v6mask[2],
            addr_v6[3] & v6mask[3],
            addr_v6[4] & v6mask[4],
            addr_v6[5] & v6mask[5],
            addr_v6[6] & v6mask[6],
            addr_v6[7] & v6mask[7],
        );

        self_ip == addr_ip
    }

    /// CIDR prefix length of the netmask, or `None` if the netmask is not contiguous.
    #[must_use]
    pub fn prefix_len(&self) -> Option<u8> {
        ipv6_netmask_to_prefix_len(self.netmask)
    }
}

fn ipv6_netmask_to_prefix_len(netmask: Ipv6Addr) -> Option<u8> {
    let netmask_u128 = netmask.to_bits();
    let prefix_len = netmask_u128.leading_ones() as u8;
    let suffix_len = netmask_u128.trailing_zeros() as u8;

    if suffix_len != 128 - prefix_len {
        None
    } else {
        Some(prefix_len)
    }
}

/// Some of the flags associated with an interface
#[derive(
    Debug, Default, PartialEq, Eq, Ord, PartialOrd, Hash, Clone, Copy, Serialize, Deserialize,
)]
pub struct InterfaceFlags {
    /// Loopback interface (lo/lo0)
    pub is_loopback: bool,
    /// Interface is up and operational
    pub is_running: bool,
    /// Point-to-point link (PPP, tun, etc.)
    pub is_point_to_point: bool,
    /// CLAT (RFC 6877) virtual interface (e.g. Android `clat4*` / `v4-*`); local-only
    pub is_clat: bool,
}

/// Some of the flags associated with an address
#[derive(
    Debug, Default, PartialEq, Eq, Ord, PartialOrd, Hash, Clone, Copy, Serialize, Deserialize,
)]
pub struct AddressFlags {
    /// Auto-configured (DHCP/SLAAC) rather than statically configured
    pub is_dynamic: bool,
    /// RFC 4941 privacy/temporary IPv6 address (rotates periodically)
    pub is_temporary: bool,
    /// Address has finished DAD and is not deprecated/tentative/duplicated
    pub is_preferred: bool,
    /// CLAT46 synthesized IPv6 (RFC 6877); local-only on macOS/iOS
    pub is_clat: bool,
    /// Absolute expiration in seconds since UNIX epoch; `None` if non-expiring (static)
    pub expiration_secs: Option<u64>,
}

/// An address bound to an interface, with its flags. Ordered by reachability preference.
#[derive(PartialEq, Eq, Clone, Debug, Serialize, Deserialize)]
pub struct InterfaceAddress {
    /// The IPv4 or IPv6 address with its netmask and broadcast.
    pub if_addr: IfAddr,
    /// Scope and lifetime flags for the address.
    pub flags: AddressFlags,
}

use core::cmp::Ordering;

// less is less preferable, greater is more preferable
impl Ord for InterfaceAddress {
    fn cmp(&self, other: &Self) -> Ordering {
        match (&self.if_addr, &other.if_addr) {
            (IfAddr::V4(a), IfAddr::V4(b)) => {
                // global scope addresses are better
                let ret = ipv4addr_is_global(&a.ip).cmp(&ipv4addr_is_global(&b.ip));
                if ret != Ordering::Equal {
                    return ret;
                }
                // local scope addresses are better
                let ret = ipv4addr_is_private(&a.ip).cmp(&ipv4addr_is_private(&b.ip));
                if ret != Ordering::Equal {
                    return ret;
                }
                // non-dynamic addresses are better
                let ret = (!self.flags.is_dynamic).cmp(&!other.flags.is_dynamic);
                if ret != Ordering::Equal {
                    return ret;
                }
            }
            (IfAddr::V6(a), IfAddr::V6(b)) => {
                // preferred addresses are better
                let ret = self.flags.is_preferred.cmp(&other.flags.is_preferred);
                if ret != Ordering::Equal {
                    return ret;
                }
                // non-temporary address are better
                let ret = (!self.flags.is_temporary).cmp(&!other.flags.is_temporary);
                if ret != Ordering::Equal {
                    return ret;
                }
                // global scope addresses are better
                let ret = ipv6addr_is_global(&a.ip).cmp(&ipv6addr_is_global(&b.ip));
                if ret != Ordering::Equal {
                    return ret;
                }
                // unique local unicast addresses are better
                let ret = ipv6addr_is_unique_local(&a.ip).cmp(&ipv6addr_is_unique_local(&b.ip));
                if ret != Ordering::Equal {
                    return ret;
                }
                // unicast site local addresses are better
                let ret = ipv6addr_is_unicast_site_local(&a.ip)
                    .cmp(&ipv6addr_is_unicast_site_local(&b.ip));
                if ret != Ordering::Equal {
                    return ret;
                }
                // unicast link local addresses are better
                let ret = ipv6addr_is_unicast_link_local(&a.ip)
                    .cmp(&ipv6addr_is_unicast_link_local(&b.ip));
                if ret != Ordering::Equal {
                    return ret;
                }
                // non-dynamic addresses are better
                let ret = (!self.flags.is_dynamic).cmp(&!other.flags.is_dynamic);
                if ret != Ordering::Equal {
                    return ret;
                }
            }
            (IfAddr::V4(a), IfAddr::V6(b)) => {
                // If the IPv6 address is preferred and not temporary, compare if it is global scope
                if other.flags.is_preferred && !other.flags.is_temporary {
                    let ret = ipv4addr_is_global(&a.ip).cmp(&ipv6addr_is_global(&b.ip));
                    if ret != Ordering::Equal {
                        return ret;
                    }
                }

                // Default, prefer IPv4 because many IPv6 addresses are not actually routed
                return Ordering::Greater;
            }
            (IfAddr::V6(a), IfAddr::V4(b)) => {
                // If the IPv6 address is preferred and not temporary, compare if it is global scope
                if self.flags.is_preferred && !self.flags.is_temporary {
                    let ret = ipv6addr_is_global(&a.ip).cmp(&ipv4addr_is_global(&b.ip));
                    if ret != Ordering::Equal {
                        return ret;
                    }
                }

                // Default, prefer IPv4 because many IPv6 addresses are not actually routed
                return Ordering::Less;
            }
        }
        // stable sort
        let ret = self.if_addr.cmp(&other.if_addr);
        if ret != Ordering::Equal {
            return ret;
        }
        self.flags.cmp(&other.flags)
    }
}
impl PartialOrd for InterfaceAddress {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl InterfaceAddress {
    /// Make an `InterfaceAddress` from an address and its flags.
    #[must_use]
    pub fn new(if_addr: IfAddr, flags: AddressFlags) -> Self {
        Self { if_addr, flags }
    }

    /// Underlying IPv4 or IPv6 address with netmask/broadcast.
    #[must_use]
    pub fn if_addr(&self) -> &IfAddr {
        &self.if_addr
    }

    /// RFC 4941 privacy/temporary IPv6 (rotates)
    #[must_use]
    pub fn is_temporary(&self) -> bool {
        self.flags.is_temporary
    }

    /// Auto-configured (DHCP/SLAAC) rather than statically configured
    #[must_use]
    pub fn is_dynamic(&self) -> bool {
        self.flags.is_dynamic
    }

    /// Address has finished DAD and is not deprecated/tentative/duplicated
    #[must_use]
    pub fn is_preferred(&self) -> bool {
        self.flags.is_preferred
    }

    /// CLAT46 synthesized IPv6 (RFC 6877), local-only on macOS/iOS
    #[must_use]
    pub fn is_clat(&self) -> bool {
        self.flags.is_clat
    }

    /// IPv4 address in CGNAT (RFC 6598 100.64.0.0/10) or DS-Lite (RFC 6333 192.0.0.0/24);
    /// always false for IPv6
    #[must_use]
    pub fn is_cgnat_or_ds_lite(&self) -> bool {
        match &self.if_addr {
            IfAddr::V4(v4) => {
                ipv4addr_is_shared(&v4.ip) || ipv4addr_is_ietf_protocol_assignment(&v4.ip)
            }
            IfAddr::V6(_) => false,
        }
    }

    /// IPv4 link-local / APIPA (169.254.0.0/16), DHCP-failed fallback, transient
    /// and not reachable by peers; always false for IPv6 (link-local IPv6 is
    /// handled by `ipv6addr_is_global`)
    #[must_use]
    pub fn is_ipv4_link_local(&self) -> bool {
        match &self.if_addr {
            IfAddr::V4(v4) => ipv4addr_is_link_local(&v4.ip),
            IfAddr::V6(_) => false,
        }
    }

    /// IPv6 link-local (RFC 4291 fe80::/10)
    #[must_use]
    pub fn is_ipv6_link_local(&self) -> bool {
        match &self.if_addr {
            IfAddr::V4(_) => false,
            IfAddr::V6(v6) => ipv6addr_is_unicast_link_local(&v6.ip),
        }
    }
}

// #[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
// enum NetworkInterfaceType {
//     Mobile,     // Least preferable, usually metered and slow
//     Unknown,    // Everything else if we can't detect the type
//     Wireless,   // Wifi is usually free or cheap and medium speed
//     Wired,      // Wired is usually free or cheap and high speed
// }

/// A single network interface on this host, with its flags, gateways, and addresses.
#[derive(PartialEq, Eq, Clone, Serialize, Deserialize)]
pub struct NetworkInterface {
    /// OS interface name (e.g. `en0`, `eth0`).
    pub name: String,
    /// Interface-level flags.
    pub flags: InterfaceFlags,
    /// Default-route gateways reachable through this interface, sorted and deduplicated.
    pub gateways: Vec<IpAddr>,
    /// Addresses bound to this interface, sorted by preference and deduplicated.
    pub addrs: Vec<InterfaceAddress>,
    /// Largest IP packet the interface carries, when the OS reports it.
    #[serde(default)]
    pub mtu: Option<u32>,
}

impl fmt::Debug for NetworkInterface {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let alt = f.alternate();
        let mut ds = f.debug_struct("NetworkInterface");
        ds.field("name", &self.name)
            .field("flags", &self.flags)
            .field("gateways", &self.gateways)
            .field("addrs", &self.addrs)
            .field("mtu", &self.mtu);
        if alt {
            ds.field("// primary_ipv4", &self.primary_ipv4());
            ds.field("// primary_ipv6", &self.primary_ipv6());
        }
        ds.finish()
    }
}
impl NetworkInterface {
    /// Make an empty `NetworkInterface` with the given name and flags.
    #[must_use]
    pub fn new(name: String, flags: InterfaceFlags) -> Self {
        Self {
            name,
            flags,
            gateways: Vec::new(),
            addrs: Vec::new(),
            mtu: None,
        }
    }

    /// OS interface name (e.g. `en0`, `eth0`, `clat4-rmnet_data0`)
    #[must_use]
    pub fn name(&self) -> String {
        self.name.clone()
    }

    /// Loopback interface (lo/lo0)
    #[must_use]
    pub fn is_loopback(&self) -> bool {
        self.flags.is_loopback
    }

    /// Point-to-point link (PPP, tun, etc.)
    #[must_use]
    pub fn is_point_to_point(&self) -> bool {
        self.flags.is_point_to_point
    }

    /// Interface is up and operational
    #[must_use]
    pub fn is_running(&self) -> bool {
        self.flags.is_running
    }

    /// CLAT (RFC 6877) virtual interface (e.g. Android `clat4*` / `v4-*`); local-only
    #[must_use]
    pub fn is_clat(&self) -> bool {
        self.flags.is_clat
    }

    /// Append an address to this interface; addresses are kept sorted and deduplicated
    pub fn add_address(&mut self, addr: InterfaceAddress) {
        self.addrs.push(addr);
        self.addrs.sort();
        self.addrs.dedup();
    }

    /// All addresses bound to this interface.
    #[must_use]
    pub fn addresses(&self) -> &[InterfaceAddress] {
        &self.addrs
    }

    /// Append a gateway to this interface; gateways are kept sorted and deduplicated.
    pub fn add_gateway(&mut self, gateway: IpAddr) {
        self.gateways.push(gateway);
        self.gateways.sort();
        self.gateways.dedup();
    }

    /// Default-route gateways reachable through this interface.
    #[must_use]
    pub fn gateways(&self) -> &[IpAddr] {
        &self.gateways
    }

    /// Largest IP packet the interface carries, when the OS reports it.
    #[must_use]
    pub fn mtu(&self) -> Option<u32> {
        self.mtu
    }

    /// Most-preferred IPv4 address on this interface (per `InterfaceAddress::cmp`).
    #[must_use]
    pub fn primary_ipv4(&self) -> Option<InterfaceAddress> {
        let mut ipv4addrs: Vec<&InterfaceAddress> = self
            .addrs
            .iter()
            .filter(|a| matches!(a.if_addr(), IfAddr::V4(_)))
            .collect();
        ipv4addrs.sort();
        ipv4addrs.last().cloned().cloned()
    }

    /// Most-preferred IPv6 address on this interface (per `InterfaceAddress::cmp`).
    #[must_use]
    pub fn primary_ipv6(&self) -> Option<InterfaceAddress> {
        let mut ipv6addrs: Vec<&InterfaceAddress> = self
            .addrs
            .iter()
            .filter(|a| matches!(a.if_addr(), IfAddr::V6(_)))
            .collect();
        ipv6addrs.sort();
        ipv6addrs.last().cloned().cloned()
    }
}

/// The interface a family's outbound traffic leaves from, the address it leaves from and the
/// gateways it leaves through. A privacy address rotating or an alias coming or going changes
/// nothing here; the source, a gateway or the interface changing does.
#[derive(Clone, Debug, Hash, Eq, PartialEq, Ord, PartialOrd, Serialize, Deserialize)]
pub struct EgressIdentity {
    /// OS interface name.
    pub interface: String,
    /// The address veilid sends from: the kernel's source, or the best published address of
    /// the family when the kernel sends from a privacy address; `None` with nothing published.
    pub source: Option<IpAddr>,
    /// Default-route gateways of the family on this interface, sorted.
    pub gateways: Vec<IpAddr>,
}

impl fmt::Display for EgressIdentity {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.source {
            Some(source) => write!(f, "{} {} via {:?}", self.interface, source, self.gateways),
            None => write!(f, "{} (no source) via {:?}", self.interface, self.gateways),
        }
    }
}

/// The gateway, publication, identity and egress addresses distilled from all interfaces.
#[derive(Clone, Default, Debug, Hash, Eq, PartialEq, Ord, PartialOrd)]
pub struct NetworkInterfaceAddressState {
    /// Gateway addresses across all interfaces, sorted and deduplicated.
    pub gateway_addresses: Vec<IpAddr>,
    /// PUBLICATION set: best routable interface addresses, sorted by preference and
    /// deduplicated. Privacy, CLAT and CGNAT addresses are excluded from peer info.
    pub interface_addresses: Vec<IfAddr>,
    /// IDENTITY set: every address on a running non-loopback interface, sorted and
    /// deduplicated. Includes privacy addresses, which a peer may report back as our
    /// external address. Not part of change detection.
    pub own_addresses: Vec<IpAddr>,
    /// The interface the kernel sends IPv4 from, the address veilid sends from there and its
    /// gateways; `None` without an IPv4 default route.
    pub egress_v4: Option<EgressIdentity>,
    /// The IPv6 egress, as `egress_v4`.
    pub egress_v6: Option<EgressIdentity>,
}

impl NetworkInterfaceAddressState {
    /// True when the address belongs to one of our interfaces. Identity, not
    /// publishability: matches privacy (RFC 4941) and CLAT addresses that publication excludes.
    #[must_use]
    pub fn is_own_address(&self, addr: IpAddr) -> bool {
        self.own_addresses.binary_search(&addr).is_ok()
    }
}

/// Which address families are present across the running non-loopback interfaces.
#[derive(Clone, Copy, Default, Debug, Hash, Eq, PartialEq, Ord, PartialOrd)]
pub struct RawAddressSummary {
    /// At least one non-loopback IPv4 address is present.
    pub has_non_loopback_ipv4: bool,
    /// At least one non-loopback IPv6 address is present.
    pub has_non_loopback_ipv6: bool,
    /// At least one globally-routable unicast IPv6 address is present.
    pub has_unicast_global_ipv6: bool,
}

/// Mutex-guarded interior state of `NetworkInterfaces`.
pub struct NetworkInterfacesInner {
    /// A successful `refresh` has populated the interface set.
    valid: bool,
    /// Interfaces from the last `refresh`, globals kept only on the primary interfaces.
    interfaces: BTreeMap<String, NetworkInterface>,
    /// Address state distilled from `interfaces` and the primaries.
    interface_address_state_cache: Arc<NetworkInterfaceAddressState>,
    /// The kernel's IPv4 outbound source at the last `refresh`.
    primary_v4: Option<Ipv4Addr>,
    /// The kernel's IPv6 outbound source at the last `refresh`.
    primary_v6: Option<Ipv6Addr>,
}

/// Cross-platform enumeration of this host's network interfaces and their addresses.
#[derive(Clone)]
pub struct NetworkInterfaces {
    inner: Arc<Mutex<NetworkInterfacesInner>>,
}

impl fmt::Debug for NetworkInterfaces {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let inner = self.inner.lock();
        f.debug_struct("NetworkInterfaces")
            .field("valid", &inner.valid)
            .field("interfaces", &inner.interfaces)
            .finish()?;
        if f.alternate() {
            writeln!(f)?;
            writeln!(
                f,
                "// interface_address_state_cache: {:?}",
                inner.interface_address_state_cache
            )?;
        }
        Ok(())
    }
}

impl Default for NetworkInterfaces {
    fn default() -> Self {
        Self::new()
    }
}

impl NetworkInterfaces {
    /// Make an empty `NetworkInterfaces`; call `refresh` to populate it.
    #[must_use]
    pub fn new() -> Self {
        Self {
            inner: Arc::new(Mutex::new(NetworkInterfacesInner {
                valid: false,
                interfaces: BTreeMap::new(),
                interface_address_state_cache: Arc::new(NetworkInterfaceAddressState::default()),
                primary_v4: None,
                primary_v6: None,
            })),
        }
    }

    /// Whether a successful `refresh` has populated the interface set.
    #[must_use]
    pub fn is_valid(&self) -> bool {
        let inner = self.inner.lock();
        inner.valid
    }
    /// Drop all interfaces and the cached address state, marking the set invalid.
    pub fn clear(&self) {
        let mut inner = self.inner.lock();

        inner.interfaces.clear();
        inner.interface_address_state_cache = Arc::new(NetworkInterfaceAddressState::default());
        inner.primary_v4 = None;
        inner.primary_v6 = None;
        inner.valid = false;
    }
    /// Re-enumerate the platform's interfaces. Returns `true` when the publication set, the
    /// gateways or a family's egress changed; identity-only churn, a privacy address rotating,
    /// refreshes the cache without reporting a change.
    ///
    /// Errors with an `io::Error` if the platform's interface enumeration fails; the interface set is
    /// left unchanged in that case.
    pub async fn refresh(&self) -> std::io::Result<bool> {
        let mut last_interfaces = {
            let mut last_interfaces = BTreeMap::<String, NetworkInterface>::new();
            let mut platform_support = PlatformSupport::new();
            platform_support
                .get_interfaces(&mut last_interfaces)
                .await?;
            last_interfaces
        };

        // The kernel's outbound source per family names the egress interface; one UDP connect
        // per family, nothing is sent
        #[cfg(not(all(target_arch = "wasm32", target_os = "unknown")))]
        let (primary_v4, primary_v6) = (primary_source_ipv4(), primary_source_ipv6());
        #[cfg(all(target_arch = "wasm32", target_os = "unknown"))]
        let (primary_v4, primary_v6): (Option<Ipv4Addr>, Option<Ipv6Addr>) = (None, None);

        // wasm has no network stack to filter
        #[cfg(not(all(target_arch = "wasm32", target_os = "unknown")))]
        Self::filter_non_primary_global_addresses(&mut last_interfaces, primary_v4, primary_v6);

        let mut inner = self.inner.lock();
        core::mem::swap(&mut inner.interfaces, &mut last_interfaces);
        inner.valid = true;
        // A default route moving between interfaces changes the egress with every address in place
        let primaries_changed = (inner.primary_v4, inner.primary_v6) != (primary_v4, primary_v6);
        inner.primary_v4 = primary_v4;
        inner.primary_v6 = primary_v6;

        if last_interfaces != inner.interfaces || primaries_changed {
            let old = inner.interface_address_state_cache.clone();
            Self::cache_interface_address_state(&mut inner);
            let new = &inner.interface_address_state_cache;
            if old.gateway_addresses != new.gateway_addresses
                || old.interface_addresses != new.interface_addresses
                || old.egress_v4 != new.egress_v4
                || old.egress_v6 != new.egress_v6
            {
                return Ok(true);
            }
        }
        Ok(false)
    }
    /// Call `f` with the interface map held under the lock, returning its result.
    pub fn with_interfaces<F, R>(&self, f: F) -> R
    where
        F: FnOnce(&BTreeMap<String, NetworkInterface>) -> R,
    {
        let inner = self.inner.lock();
        f(&inner.interfaces)
    }

    /// The cached best-address state from the last `refresh`.
    #[must_use]
    pub fn interface_address_state(&self) -> Arc<NetworkInterfaceAddressState> {
        let inner = self.inner.lock();
        inner.interface_address_state_cache.clone()
    }

    /// Address presence across running, non-loopback interfaces, NOT filtered by
    /// `is_temporary()`, since RFC 7217 secured SLAAC globals get marked temporary on macOS.
    #[must_use]
    pub fn raw_address_summary(&self) -> RawAddressSummary {
        let inner = self.inner.lock();
        let mut summary = RawAddressSummary::default();
        for intf in inner.interfaces.values() {
            if !intf.is_running() || intf.is_loopback() {
                continue;
            }
            for addr in intf.addresses() {
                match addr.if_addr() {
                    IfAddr::V4(v4) => {
                        if !v4.ip.is_loopback() && !v4.ip.is_unspecified() {
                            summary.has_non_loopback_ipv4 = true;
                        }
                    }
                    IfAddr::V6(v6) => {
                        if !v6.ip.is_loopback() && !v6.ip.is_unspecified() {
                            summary.has_non_loopback_ipv6 = true;
                            if ipv6addr_is_unicast_global(&v6.ip) {
                                summary.has_unicast_global_ipv6 = true;
                            }
                        }
                    }
                }
            }
        }
        summary
    }

    /////////////////////////////////////////////

    /// Keep global addresses only on the interface carrying the kernel's primary outbound
    /// source for that family; a global on any other interface (iOS cellular beside wifi) is
    /// unreachable from peers, and publishing it causes timeouts. The interface decides, not
    /// the exact address: the kernel sends from a privacy address, which must not choose the
    /// stable global that gets published. Private, link-local and ULA addresses stay for
    /// LocalNetwork. Interfaces left with no addresses are dropped.
    #[cfg(not(all(target_arch = "wasm32", target_os = "unknown")))]
    pub(crate) fn filter_non_primary_global_addresses(
        interfaces: &mut BTreeMap<String, NetworkInterface>,
        primary_v4: Option<Ipv4Addr>,
        primary_v6: Option<Ipv6Addr>,
    ) {
        let primary_v4_intf =
            primary_v4.and_then(|p| owner_of(interfaces, IpAddr::V4(p)).map(str::to_owned));
        let primary_v6_intf =
            primary_v6.and_then(|p| owner_of(interfaces, IpAddr::V6(p)).map(str::to_owned));

        interfaces.retain(|name, intf| {
            intf.addrs.retain(|addr| match addr.if_addr() {
                IfAddr::V4(v4) => {
                    !ipv4addr_is_global(&v4.ip) || primary_v4_intf.as_deref() == Some(name.as_str())
                }
                IfAddr::V6(v6) => {
                    !ipv6addr_is_unicast_global(&v6.ip)
                        || primary_v6_intf.as_deref() == Some(name.as_str())
                }
            });
            !intf.addrs.is_empty()
        });
    }

    fn cache_interface_address_state(inner: &mut NetworkInterfacesInner) {
        inner.interface_address_state_cache = Arc::new(Self::interface_address_state_for(
            &inner.interfaces,
            inner.primary_v4,
            inner.primary_v6,
        ));
    }

    /// The gateway, publication, identity and egress sets distilled from `interfaces`; the
    /// primaries are the kernel's outbound sources, whose owning interfaces are the egress
    pub(crate) fn interface_address_state_for(
        interfaces: &BTreeMap<String, NetworkInterface>,
        primary_v4: Option<Ipv4Addr>,
        primary_v6: Option<Ipv6Addr>,
    ) -> NetworkInterfaceAddressState {
        // Reduce interfaces to their best routable ip addresses
        let mut intf_addrs = Vec::new();
        let mut gateway_addresses = Vec::new();
        let mut own_addresses = Vec::new();
        for intf in interfaces.values() {
            // Skip down, loopback, and CLAT virtual interfaces (Android: clat4*/v4-*)
            if !intf.is_running() || intf.is_loopback() || intf.is_clat() {
                continue;
            }

            for addr in intf.addresses() {
                // Identity set keeps what publication filters out; peers report those back
                // to us as our external address
                own_addresses.push(addr.if_addr().ip());
                if is_published(addr) {
                    intf_addrs.push(addr.clone());
                }
            }

            for gateway in intf.gateways() {
                gateway_addresses.push(*gateway);
            }
        }

        // Sort one more time to get the best interface addresses overall
        let mut interface_addresses = intf_addrs
            .iter()
            .map(|x| x.if_addr().clone())
            .collect::<Vec<_>>();

        interface_addresses.sort();
        interface_addresses.dedup();

        gateway_addresses.sort();
        gateway_addresses.dedup();

        own_addresses.sort();
        own_addresses.dedup();

        let egress_v4 = primary_v4.and_then(|p| egress_identity(interfaces, IpAddr::V4(p)));
        let egress_v6 = primary_v6.and_then(|p| egress_identity(interfaces, IpAddr::V6(p)));

        NetworkInterfaceAddressState {
            gateway_addresses,
            interface_addresses,
            own_addresses,
            egress_v4,
            egress_v6,
        }
    }
}

/// The interface holding `ip`, if any
fn owner_of(interfaces: &BTreeMap<String, NetworkInterface>, ip: IpAddr) -> Option<&str> {
    interfaces
        .iter()
        .find(|(_, intf)| intf.addrs.iter().any(|a| a.if_addr().ip() == ip))
        .map(|(name, _)| name.as_str())
}

/// Publication keeps what peers can reach: RFC 4941 privacy addresses rotate and churn peer
/// info, CLAT46 and CGNAT/DS-Lite addresses are not reachable, IPv4 link-local is a transient
/// DHCP fallback, and IPv6 link-local needs a scope id no dial info carries, so no peer can
/// dial it and every tunnel and AirDrop interface would churn peer info as its address rotates
fn is_published(addr: &InterfaceAddress) -> bool {
    !addr.is_temporary()
        && !addr.is_clat()
        && !addr.is_cgnat_or_ds_lite()
        && !addr.is_ipv4_link_local()
        && !addr.is_ipv6_link_local()
}

/// The interface the kernel sends `primary` from, the address veilid sends from there and the
/// family's gateways. veilid never binds a privacy address, so when the kernel picked one the
/// source is the best published address of the family instead, stable across rotations
fn egress_identity(
    interfaces: &BTreeMap<String, NetworkInterface>,
    primary: IpAddr,
) -> Option<EgressIdentity> {
    let name = owner_of(interfaces, primary)?;
    let intf = &interfaces[name];
    let same_family = |ip: IpAddr| ip.is_ipv4() == primary.is_ipv4();
    let primary_is_temporary = intf
        .addrs
        .iter()
        .any(|a| a.if_addr().ip() == primary && a.is_temporary());
    let source = if primary_is_temporary {
        intf.addrs
            .iter()
            .filter(|a| {
                let ip = a.if_addr().ip();
                is_published(a)
                    && same_family(ip)
                    && !matches!(a.if_addr(), IfAddr::V6(v6) if ipv6addr_is_unicast_link_local(&v6.ip))
            })
            .max()
            .map(|a| a.if_addr().ip())
    } else {
        Some(primary)
    };
    let gateways = intf
        .gateways()
        .iter()
        .copied()
        .filter(|g| same_family(*g))
        .collect();
    Some(EgressIdentity {
        interface: name.to_owned(),
        source,
        gateways,
    })
}
