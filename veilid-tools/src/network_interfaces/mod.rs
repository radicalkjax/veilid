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

    /// CLAT46 synthesized IPv6 (RFC 6877) — local-only on macOS/iOS
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

    /// IPv4 link-local / APIPA (169.254.0.0/16) — DHCP-failed fallback, transient
    /// and not reachable by peers; always false for IPv6 (link-local IPv6 is
    /// handled by `ipv6addr_is_global`)
    #[must_use]
    pub fn is_ipv4_link_local(&self) -> bool {
        match &self.if_addr {
            IfAddr::V4(v4) => ipv4addr_is_link_local(&v4.ip),
            IfAddr::V6(_) => false,
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
}

impl fmt::Debug for NetworkInterface {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let alt = f.alternate();
        let mut ds = f.debug_struct("NetworkInterface");
        ds.field("name", &self.name)
            .field("flags", &self.flags)
            .field("gateways", &self.gateways)
            .field("addrs", &self.addrs);
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

/// The best routable gateway and interface addresses distilled from all interfaces.
#[derive(Clone, Default, Debug, Hash, Eq, PartialEq, Ord, PartialOrd)]
pub struct NetworkInterfaceAddressState {
    /// Gateway addresses across all interfaces, sorted and deduplicated.
    pub gateway_addresses: Vec<IpAddr>,
    /// Best routable interface addresses, sorted by preference and deduplicated.
    pub interface_addresses: Vec<IfAddr>,
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
    valid: bool,
    interfaces: BTreeMap<String, NetworkInterface>,
    interface_address_state_cache: Arc<NetworkInterfaceAddressState>,
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
        inner.valid = false;
    }
    /// Re-enumerate the platform's interfaces. Returns `true` if the best-address cache changed.
    ///
    /// Errors with an `io::Error` if the platform's interface enumeration fails; the interface set is
    /// left unchanged in that case.
    // returns false if refresh had no changes, true if changes were present
    pub async fn refresh(&self) -> std::io::Result<bool> {
        let mut last_interfaces = {
            let mut last_interfaces = BTreeMap::<String, NetworkInterface>::new();
            let mut platform_support = PlatformSupport::new();
            platform_support
                .get_interfaces(&mut last_interfaces)
                .await?;
            last_interfaces
        };

        // Drop globally-routable addresses from interfaces that aren't the kernel's
        // primary outbound source for that family (e.g. iOS cellular IPv6 while wifi
        // is primary). Private/local addresses are kept so the LocalNetwork routing
        // domain still works. Don't do this on wasm32-unknown-unknown because we don't have
        // access to the network stack.
        #[cfg(not(all(target_arch = "wasm32", target_os = "unknown")))]
        Self::filter_non_primary_global_addresses(&mut last_interfaces);

        let mut inner = self.inner.lock();
        core::mem::swap(&mut inner.interfaces, &mut last_interfaces);
        inner.valid = true;

        if last_interfaces != inner.interfaces {
            // get last address cache
            let old_interface_address_state = inner.interface_address_state_cache.clone();

            // redo the address cache
            Self::cache_interface_address_state(&mut inner);

            // See if our best addresses have changed
            if old_interface_address_state != inner.interface_address_state_cache {
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

    // For each interface, keep only addresses that are either non-global (private/
    // link-local/ULA, needed by LocalNetwork) or match the kernel's primary outbound
    // source for that family. A global address on a non-primary interface is
    // unreachable from peers — publishing it as our dial info just causes timeouts.
    // Interfaces that end up with no addresses are dropped entirely.

    #[cfg(not(all(target_arch = "wasm32", target_os = "unknown")))]
    fn filter_non_primary_global_addresses(interfaces: &mut BTreeMap<String, NetworkInterface>) {
        let primary_v4 = primary_source_ipv4();
        let primary_v6 = primary_source_ipv6();

        interfaces.retain(|_, intf| {
            intf.addrs.retain(|addr| match addr.if_addr() {
                IfAddr::V4(v4) => {
                    !ipv4addr_is_global(&v4.ip) || primary_v4.is_some_and(|p| p == v4.ip)
                }
                IfAddr::V6(v6) => {
                    !ipv6addr_is_unicast_global(&v6.ip) || primary_v6.is_some_and(|p| p == v6.ip)
                }
            });
            !intf.addrs.is_empty()
        });
    }

    fn cache_interface_address_state(inner: &mut NetworkInterfacesInner) {
        // Reduce interfaces to their best routable ip addresses
        let mut intf_addrs = Vec::new();
        let mut gateway_addresses = Vec::new();
        for intf in inner.interfaces.values() {
            // Skip down, loopback, and CLAT virtual interfaces (Android: clat4*/v4-*)
            if !intf.is_running() || intf.is_loopback() || intf.is_clat() {
                continue;
            }

            for addr in intf.addresses() {
                // Skip RFC 4941 privacy addresses; they rotate and churn published peer info
                if addr.is_temporary() {
                    continue;
                }
                // Skip macOS/iOS CLAT46 synthesized IPv6 (local-only translator address)
                if addr.is_clat() {
                    continue;
                }
                // Skip RFC 6598 CGNAT and RFC 6333 DS-Lite IPv4; peers can't reach them
                if addr.is_cgnat_or_ds_lite() {
                    continue;
                }
                // Skip RFC 3927 IPv4 link-local (169.254.0.0/16); transient DHCP-fallback
                // address, never reachable by peers
                if addr.is_ipv4_link_local() {
                    continue;
                }
                intf_addrs.push(addr.clone());
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

        // Now export just the addresses
        inner.interface_address_state_cache = Arc::new(NetworkInterfaceAddressState {
            gateway_addresses,
            interface_addresses,
        });
    }
}
