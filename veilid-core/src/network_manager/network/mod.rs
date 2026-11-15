// Wasm-only in production, but unit-tested on every platform
#[cfg(any(
    all(target_arch = "wasm32", target_os = "unknown"),
    test,
    feature = "test-util"
))]
mod address_family_confirm_logic;
#[cfg(not(all(target_arch = "wasm32", target_os = "unknown")))]
mod family_mtu;
#[cfg(not(all(target_arch = "wasm32", target_os = "unknown")))]
mod mtu_probe_processor;
mod platform_network;
mod platform_protocol_network_connection;

cfg_if! {
    if #[cfg(not(all(target_arch = "wasm32", target_os = "unknown")))] {
        mod native;
    }
    else if #[cfg(all(target_arch = "wasm32", target_os = "unknown"))] {
        mod wasm;
    }
    else {
        compile_error!("No network implementation for this platform!");
    }
}

use super::*;

#[cfg(any(
    all(target_arch = "wasm32", target_os = "unknown"),
    test,
    feature = "test-util"
))]
pub(crate) use address_family_confirm_logic::*;
#[cfg(not(all(target_arch = "wasm32", target_os = "unknown")))]
pub(crate) use family_mtu::*;
#[cfg(not(all(target_arch = "wasm32", target_os = "unknown")))]
pub(crate) use mtu_probe_processor::*;
pub use platform_network::*;
pub use platform_protocol_network_connection::*;

/// What an interface address change did to the egress PublicInternet traffic leaves from.
/// A tunnel, a secondary interface, a link-local address or an alias coming or going reaches
/// nothing here.
#[cfg(not(all(target_arch = "wasm32", target_os = "unknown")))]
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct EgressChange {
    /// Families whose egress appeared
    pub appeared: AddressTypeSet,
    /// Families in use whose egress interface, source address or gateways changed, or went away
    pub moved: AddressTypeSet,
    /// Families in use whose egress is as it was
    pub kept: AddressTypeSet,
}

#[cfg(not(all(target_arch = "wasm32", target_os = "unknown")))]
impl EgressChange {
    /// Nothing PublicInternet traffic leaves from changed
    pub fn is_none(&self) -> bool {
        self.appeared.is_empty() && self.moved.is_empty()
    }

    /// Every family that carried traffic moved: the old network is gone
    pub fn is_full_move(&self) -> bool {
        !self.moved.is_empty() && self.kept.is_empty()
    }

    /// The moved and appeared families, before and after
    pub fn describe(
        &self,
        last: &NetworkInterfaceAddressState,
        new: &NetworkInterfaceAddressState,
    ) -> String {
        let show = |egress: &Option<EgressIdentity>| {
            egress
                .as_ref()
                .map_or("none".to_owned(), ToString::to_string)
        };
        let mut parts = Vec::new();
        for (family, old, cur) in egress_per_family(last, new) {
            let what = if self.moved.contains(family) {
                if old == cur {
                    "moved with IPv6 (CLAT)"
                } else {
                    "moved"
                }
            } else if self.appeared.contains(family) {
                "appeared"
            } else {
                continue;
            };
            parts.push(format!(
                "{:?} {}: {} -> {}",
                family,
                what,
                show(old),
                show(cur)
            ));
        }
        parts.join("; ")
    }
}

/// Each family's egress before and after a change
#[cfg(not(all(target_arch = "wasm32", target_os = "unknown")))]
fn egress_per_family<'a>(
    last: &'a NetworkInterfaceAddressState,
    new: &'a NetworkInterfaceAddressState,
) -> [(
    AddressType,
    &'a Option<EgressIdentity>,
    &'a Option<EgressIdentity>,
); 2] {
    [
        (AddressType::IPV4, &last.egress_v4, &new.egress_v4),
        (AddressType::IPV6, &last.egress_v6, &new.egress_v6),
    ]
}

/// A family's hole punch TTL: the carrier-grade NAT one when its traffic leaves from a carrier
/// NAT or DS-Lite address, since that NAT sits more hops out than a home router
#[cfg(not(all(target_arch = "wasm32", target_os = "unknown")))]
pub(crate) fn hole_punch_ttl(
    network: &VeilidConfigInternalNetwork,
    interface_address_state: &NetworkInterfaceAddressState,
    address_type: AddressType,
) -> u32 {
    let egress = match address_type {
        AddressType::IPV4 => &interface_address_state.egress_v4,
        AddressType::IPV6 => &interface_address_state.egress_v6,
    };
    match egress
        .as_ref()
        .and_then(|egress| egress.source)
        .map(Address::from_ip_addr)
    {
        Some(source) if source.is_cgnat_indicator() => network.cgnat_hole_punch_ttl,
        _ => network.default_hole_punch_ttl,
    }
}

/// Compare the egress per family. `was_in_use` is what PublicInternet ran on before the
/// change and `now_in_use` what it can run on after it.
#[cfg(not(all(target_arch = "wasm32", target_os = "unknown")))]
pub(crate) fn egress_change(
    last: &NetworkInterfaceAddressState,
    new: &NetworkInterfaceAddressState,
    was_in_use: AddressTypeSet,
    now_in_use: AddressTypeSet,
) -> EgressChange {
    let mut change = EgressChange::default();
    for (family, old, cur) in egress_per_family(last, new) {
        match (old, cur) {
            (None, Some(_)) if now_in_use.contains(family) => {
                change.appeared.insert(family);
            }
            (Some(_), None) if was_in_use.contains(family) => {
                change.moved.insert(family);
            }
            (Some(before), Some(after)) if was_in_use.contains(family) => {
                if before == after {
                    change.kept.insert(family);
                } else {
                    change.moved.insert(family);
                }
            }
            _ => {}
        }
    }
    // A CLAT IPv4 egress (RFC 7335, 192.0.0.0/29) is carried by the IPv6 route: when that route
    // moves or goes, so does IPv4, whatever the route table still lists for it
    if change.kept.contains(AddressType::IPV4)
        && last.egress_v6.is_some()
        && last.egress_v6 != new.egress_v6
        && last.egress_v4.as_ref().is_some_and(is_clat_egress)
    {
        change.kept.remove(AddressType::IPV4);
        change.moved.insert(AddressType::IPV4);
    }
    change
}

/// The IPv4 source is this host's own translator address, so the family has no path of its own
#[cfg(not(all(target_arch = "wasm32", target_os = "unknown")))]
pub(crate) fn is_clat_egress(egress: &EgressIdentity) -> bool {
    matches!(egress.source, Some(IpAddr::V4(v4)) if ipv4addr_is_ietf_protocol_assignment(&v4))
}

/// The set of address types enabled by config; an empty `network.address_types`
/// list means all address types are enabled.
pub(crate) fn configured_address_type_set(config: &VeilidConfig) -> AddressTypeSet {
    if config.network.address_types.is_empty() {
        return AddressTypeSet::all();
    }
    let mut set = AddressTypeSet::empty();
    for at in &config.network.address_types {
        set |= match at {
            VeilidConfigAddressType::Ipv4 => AddressType::IPV4,
            VeilidConfigAddressType::Ipv6 => AddressType::IPV6,
        };
    }
    set
}
