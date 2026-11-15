//
// This file really shouldn't be necessary, but 'ip' isn't a stable feature
//

use super::*;

use core::hash::*;

/// Scope of an IPv6 multicast address (RFC 4291 § 2.7), in increasing reach.
#[derive(Copy, PartialEq, Eq, Clone, Hash, Debug)]
pub enum Ipv6MulticastScope {
    /// Confined to a single interface, for loopback multicast.
    InterfaceLocal,
    /// Confined to a single link.
    LinkLocal,
    /// Confined to the local realm.
    RealmLocal,
    /// Confined to a locally administered scope.
    AdminLocal,
    /// Confined to a single site.
    SiteLocal,
    /// Confined to a single organization.
    OrganizationLocal,
    /// Reaches the whole internet.
    Global,
}

/// Address has the all-zeros bit pattern (IPv4 `0.0.0.0` or IPv6 `::`).
#[must_use]
pub fn ipaddr_is_unspecified(addr: &IpAddr) -> bool {
    match addr {
        IpAddr::V4(ip) => ipv4addr_is_unspecified(ip),
        IpAddr::V6(ip) => ipv6addr_is_unspecified(ip),
    }
}

/// Address points at the host itself (`127.0.0.0/8` or `::1`).
#[must_use]
pub fn ipaddr_is_loopback(addr: &IpAddr) -> bool {
    match addr {
        IpAddr::V4(ip) => ipv4addr_is_loopback(ip),
        IpAddr::V6(ip) => ipv6addr_is_loopback(ip),
    }
}

/// Address is globally routable on the public internet.
#[must_use]
pub fn ipaddr_is_global(addr: &IpAddr) -> bool {
    match addr {
        IpAddr::V4(ip) => ipv4addr_is_global(ip),
        IpAddr::V6(ip) => ipv6addr_is_global(ip),
    }
}

/// Address is in a multicast range (IPv4 `224.0.0.0/4` or IPv6 `ff00::/8`).
#[must_use]
pub fn ipaddr_is_multicast(addr: &IpAddr) -> bool {
    match addr {
        IpAddr::V4(ip) => ipv4addr_is_multicast(ip),
        IpAddr::V6(ip) => ipv6addr_is_multicast(ip),
    }
}

/// Address is reserved for documentation (RFC 5737 / RFC 3849).
#[must_use]
pub fn ipaddr_is_documentation(addr: &IpAddr) -> bool {
    match addr {
        IpAddr::V4(ip) => ipv4addr_is_documentation(ip),
        IpAddr::V6(ip) => ipv6addr_is_documentation(ip),
    }
}

/// IPv4 unspecified address `0.0.0.0`.
#[must_use]
pub fn ipv4addr_is_unspecified(addr: &Ipv4Addr) -> bool {
    addr.octets() == [0u8, 0u8, 0u8, 0u8]
}

/// IPv4 loopback range `127.0.0.0/8` (RFC 1122).
#[must_use]
pub fn ipv4addr_is_loopback(addr: &Ipv4Addr) -> bool {
    addr.octets()[0] == 127
}

/// IPv4 RFC 1918 private ranges: `10.0.0.0/8`, `172.16.0.0/12`, `192.168.0.0/16`.
#[must_use]
pub fn ipv4addr_is_private(addr: &Ipv4Addr) -> bool {
    match addr.octets() {
        [10, ..] => true,
        [172, b, ..] if (16..=31).contains(&b) => true,
        [192, 168, ..] => true,
        _ => false,
    }
}

/// IPv4 link-local / APIPA range `169.254.0.0/16` (RFC 3927) — DHCP-fallback addresses.
#[must_use]
pub fn ipv4addr_is_link_local(addr: &Ipv4Addr) -> bool {
    matches!(addr.octets(), [169, 254, ..])
}

/// IPv4 address is globally routable on the public internet.
///
/// Excludes private, loopback, link-local, broadcast, documentation, shared (CGNAT),
/// IETF protocol assignment, reserved (future-use), benchmarking, and the `0.0.0.0/8`
/// "this network" range. The two anycast addresses `192.0.0.9` (PCP) and
/// `192.0.0.10` (PCP DHCP server) are special-cased as global despite living in the
/// IETF protocol assignment block.
#[must_use]
pub fn ipv4addr_is_global(addr: &Ipv4Addr) -> bool {
    if u32::from(*addr) == 0xc0000009 || u32::from(*addr) == 0xc000000a {
        return true;
    }
    !ipv4addr_is_private(addr)
        && !ipv4addr_is_loopback(addr)
        && !ipv4addr_is_link_local(addr)
        && !ipv4addr_is_broadcast(addr)
        && !ipv4addr_is_documentation(addr)
        && !ipv4addr_is_shared(addr)
        && !ipv4addr_is_ietf_protocol_assignment(addr)
        && !ipv4addr_is_reserved(addr)
        && !ipv4addr_is_benchmarking(addr)
        && addr.octets()[0] != 0
}

/// IPv4 RFC 6598 shared address space `100.64.0.0/10` — used for carrier-grade NAT (CGNAT).
#[must_use]
pub fn ipv4addr_is_shared(addr: &Ipv4Addr) -> bool {
    addr.octets()[0] == 100 && (addr.octets()[1] & 0b1100_0000 == 0b0100_0000)
}

/// IPv4 IETF protocol assignment range `192.0.0.0/24` (RFC 6890).
///
/// Includes the CLAT46 / 464XLAT translator IPv4 source range `192.0.0.0/29` (RFC 7335)
/// and other IETF-reserved well-known anycast addresses.
#[must_use]
pub fn ipv4addr_is_ietf_protocol_assignment(addr: &Ipv4Addr) -> bool {
    addr.octets()[0] == 192 && addr.octets()[1] == 0 && addr.octets()[2] == 0
}

/// IPv4 RFC 2544 benchmarking range `198.18.0.0/15`.
#[must_use]
pub fn ipv4addr_is_benchmarking(addr: &Ipv4Addr) -> bool {
    addr.octets()[0] == 198 && (addr.octets()[1] & 0xfe) == 18
}

/// IPv4 reserved range `240.0.0.0/4` (RFC 1112), excluding the broadcast address.
#[must_use]
pub fn ipv4addr_is_reserved(addr: &Ipv4Addr) -> bool {
    addr.octets()[0] & 240 == 240 && !addr.is_broadcast()
}

/// IPv4 multicast range `224.0.0.0/4` (RFC 5771).
#[must_use]
pub fn ipv4addr_is_multicast(addr: &Ipv4Addr) -> bool {
    addr.octets()[0] >= 224 && addr.octets()[0] <= 239
}

/// IPv4 limited broadcast address `255.255.255.255`.
#[must_use]
pub fn ipv4addr_is_broadcast(addr: &Ipv4Addr) -> bool {
    addr.octets() == [255u8, 255u8, 255u8, 255u8]
}

/// IPv4 documentation ranges (RFC 5737): `192.0.2.0/24`, `198.51.100.0/24`, `203.0.113.0/24`.
#[must_use]
pub fn ipv4addr_is_documentation(addr: &Ipv4Addr) -> bool {
    matches!(
        addr.octets(),
        [192, 0, 2, _] | [198, 51, 100, _] | [203, 0, 113, _]
    )
}

/// IPv6 unspecified address `::`.
#[must_use]
pub fn ipv6addr_is_unspecified(addr: &Ipv6Addr) -> bool {
    addr.segments() == [0, 0, 0, 0, 0, 0, 0, 0]
}

/// IPv6 loopback `::1`.
#[must_use]
pub fn ipv6addr_is_loopback(addr: &Ipv6Addr) -> bool {
    addr.segments() == [0, 0, 0, 0, 0, 0, 0, 1]
}

/// IPv6 address is globally routable on the public internet.
///
/// True for unicast-global addresses (which excludes link-local, unique-local, loopback,
/// unspecified, documentation) and global-scoped multicast.
#[must_use]
pub fn ipv6addr_is_global(addr: &Ipv6Addr) -> bool {
    match ipv6addr_multicast_scope(addr) {
        Some(Ipv6MulticastScope::Global) => true,
        None => ipv6addr_is_unicast_global(addr),
        _ => false,
    }
}

/// IPv6 unique-local range `fc00::/7` (RFC 4193) — organisation-internal addresses.
#[must_use]
pub fn ipv6addr_is_unique_local(addr: &Ipv6Addr) -> bool {
    (addr.segments()[0] & 0xfe00) == 0xfc00
}

/// IPv6 strict link-local `fe80::/64` — the canonical link-local prefix with all-zero subnet ID.
#[must_use]
pub fn ipv6addr_is_unicast_link_local_strict(addr: &Ipv6Addr) -> bool {
    addr.segments()[0] == 0xfe80
        && addr.segments()[1] == 0
        && addr.segments()[2] == 0
        && addr.segments()[3] == 0
}

/// IPv6 link-local range `fe80::/10` (RFC 4291).
#[must_use]
pub fn ipv6addr_is_unicast_link_local(addr: &Ipv6Addr) -> bool {
    (addr.segments()[0] & 0xffc0) == 0xfe80
}

/// IPv6 deprecated site-local range `fec0::/10` (RFC 3879, deprecated by RFC 4291).
#[must_use]
pub fn ipv6addr_is_unicast_site_local(addr: &Ipv6Addr) -> bool {
    (addr.segments()[0] & 0xffc0) == 0xfec0
}

/// IPv6 documentation range `2001:db8::/32` (RFC 3849).
#[must_use]
pub fn ipv6addr_is_documentation(addr: &Ipv6Addr) -> bool {
    (addr.segments()[0] == 0x2001) && (addr.segments()[1] == 0xdb8)
}

/// IPv6 well-known NAT64 prefix `64:ff9b::/96` (RFC 6052) — synthesised IPv6 wrapping public IPv4.
#[must_use]
pub fn ipv6addr_is_nat64_well_known(addr: &Ipv6Addr) -> bool {
    let s = addr.segments();
    s[0] == 0x0064 && s[1] == 0xff9b && s[2] == 0 && s[3] == 0 && s[4] == 0 && s[5] == 0
}

/// IPv6 Teredo prefix `2001::/32` (RFC 4380) — IPv6-in-UDP/IPv4 transitional encapsulation.
#[must_use]
pub fn ipv6addr_is_teredo(addr: &Ipv6Addr) -> bool {
    let s = addr.segments();
    s[0] == 0x2001 && s[1] == 0x0000
}

/// IPv6 6to4 prefix `2002::/16` (RFC 3056) — IPv6-in-IPv4 transitional encapsulation.
#[must_use]
pub fn ipv6addr_is_6to4(addr: &Ipv6Addr) -> bool {
    addr.segments()[0] == 0x2002
}

/// IPv6 unicast address that is globally routable.
///
/// True for any unicast address not falling into multicast, loopback, link-local,
/// unique-local, unspecified, or documentation. NAT64 / Teredo / 6to4 ranges are
/// included here — they wrap public IPv4 endpoints and route on the public internet.
#[must_use]
pub fn ipv6addr_is_unicast_global(addr: &Ipv6Addr) -> bool {
    !ipv6addr_is_multicast(addr)
        && !ipv6addr_is_loopback(addr)
        && !ipv6addr_is_unicast_link_local(addr)
        && !ipv6addr_is_unique_local(addr)
        && !ipv6addr_is_unspecified(addr)
        && !ipv6addr_is_documentation(addr)
}

/// IPv6 multicast scope (RFC 4291 § 2.7) when the address is multicast; `None` otherwise.
#[must_use]
pub fn ipv6addr_multicast_scope(addr: &Ipv6Addr) -> Option<Ipv6MulticastScope> {
    if ipv6addr_is_multicast(addr) {
        match addr.segments()[0] & 0x000f {
            1 => Some(Ipv6MulticastScope::InterfaceLocal),
            2 => Some(Ipv6MulticastScope::LinkLocal),
            3 => Some(Ipv6MulticastScope::RealmLocal),
            4 => Some(Ipv6MulticastScope::AdminLocal),
            5 => Some(Ipv6MulticastScope::SiteLocal),
            8 => Some(Ipv6MulticastScope::OrganizationLocal),
            14 => Some(Ipv6MulticastScope::Global),
            _ => None,
        }
    } else {
        None
    }
}

/// IPv6 multicast range `ff00::/8` (RFC 4291).
#[must_use]
pub fn ipv6addr_is_multicast(addr: &Ipv6Addr) -> bool {
    (addr.segments()[0] & 0xff00) == 0xff00
}

/// Collapses an address to its allocation block by masking the host bits.
///
/// IPv4 addresses are returned unchanged (treated as single hosts). IPv6 addresses
/// keep the top `ip6_prefix_size` bits and set the remaining host bits, yielding the
/// prefix-allocated block the address belongs to.
// Converts an ip to a ip block by applying a netmask
// to the host part of the ip address
// ipv4 addresses are treated as single hosts
// ipv6 addresses are treated as prefix allocated blocks
#[must_use]
pub fn ip_to_ipblock(ip6_prefix_size: usize, addr: IpAddr) -> IpAddr {
    match addr {
        IpAddr::V4(_) => addr,
        IpAddr::V6(v6) => {
            let mut hostlen = 128usize.saturating_sub(ip6_prefix_size);
            let mut out = v6.octets();
            for i in (0..16).rev() {
                if hostlen >= 8 {
                    out[i] = 0xFF;
                    hostlen -= 8;
                } else {
                    out[i] |= !(0xFFu8 << hostlen);
                    break;
                }
            }
            IpAddr::V6(Ipv6Addr::from(out))
        }
    }
}

/// Applies `netmask` to `addr`, zeroing the host bits. Returns `None` if the address
/// and netmask are different IP families.
#[must_use]
pub fn ipaddr_apply_netmask(addr: IpAddr, netmask: IpAddr) -> Option<IpAddr> {
    match addr {
        IpAddr::V4(v4) => {
            let IpAddr::V4(v4mask) = netmask else {
                return None;
            };
            let v4 = v4.octets();
            let v4mask = v4mask.octets();
            Some(IpAddr::V4(Ipv4Addr::new(
                v4[0] & v4mask[0],
                v4[1] & v4mask[1],
                v4[2] & v4mask[2],
                v4[3] & v4mask[3],
            )))
        }
        IpAddr::V6(v6) => {
            let IpAddr::V6(v6mask) = netmask else {
                return None;
            };
            let v6 = v6.segments();
            let v6mask = v6mask.segments();
            Some(IpAddr::V6(Ipv6Addr::new(
                v6[0] & v6mask[0],
                v6[1] & v6mask[1],
                v6[2] & v6mask[2],
                v6[3] & v6mask[3],
                v6[4] & v6mask[4],
                v6[5] & v6mask[5],
                v6[6] & v6mask[6],
                v6[7] & v6mask[7],
            )))
        }
    }
}

/// True if `addr` falls within the network `netaddr`/`netmask`. False if `addr` and
/// `netaddr` are different IP families.
#[must_use]
pub fn ipaddr_in_network(addr: IpAddr, netaddr: IpAddr, netmask: IpAddr) -> bool {
    if addr.is_ipv4() && !netaddr.is_ipv4() {
        return false;
    }
    if addr.is_ipv6() && !netaddr.is_ipv6() {
        return false;
    }
    match (
        ipaddr_apply_netmask(netaddr, netmask),
        ipaddr_apply_netmask(addr, netmask),
    ) {
        (Some(net), Some(masked)) => net == masked,
        _ => false,
    }
}
