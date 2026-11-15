//! Which TTL a hole punch uses: the carrier-grade NAT one only when the family's traffic leaves
//! from a carrier NAT or DS-Lite address.

use super::*;

fn egress(source: Option<&str>) -> Option<EgressIdentity> {
    source.map(|source| EgressIdentity {
        interface: "en0".to_owned(),
        source: Some(source.parse().expect("test address")),
        gateways: vec![],
    })
}

fn state(v4_source: Option<&str>, v6_source: Option<&str>) -> NetworkInterfaceAddressState {
    NetworkInterfaceAddressState {
        egress_v4: egress(v4_source),
        egress_v6: egress(v6_source),
        ..Default::default()
    }
}

fn network(default_ttl: u32, cgnat_ttl: u32) -> VeilidConfigInternalNetwork {
    VeilidConfigInternalNetwork {
        default_hole_punch_ttl: default_ttl,
        cgnat_hole_punch_ttl: cgnat_ttl,
        ..Default::default()
    }
}

/// A home NAT's source punches at the default TTL, and a carrier NAT or DS-Lite source at the
/// carrier one
fn test_ttl_follows_the_egress_source() {
    let network = network(3, 8);
    for (source, expected) in [
        (Some("192.168.1.146"), 3),
        (Some("10.0.0.5"), 3),
        (Some("100.64.0.7"), 8),
        (Some("100.127.255.254"), 8),
        (Some("100.128.0.1"), 3),
        (Some("192.0.0.2"), 8),
        (None, 3),
    ] {
        assert_eq!(
            hole_punch_ttl(&network, &state(source, None), AddressType::IPV4),
            expected,
            "{:?}",
            source
        );
    }
}

/// Carrier-grade NAT is an IPv4 condition; an IPv6 punch keeps the default
fn test_ipv6_keeps_the_default() {
    let network = network(3, 8);
    assert_eq!(
        hole_punch_ttl(
            &network,
            &state(Some("100.64.0.7"), Some("2600:1702:7d80:a0f::1")),
            AddressType::IPV6
        ),
        3
    );
}

/// Both TTLs come from config
fn test_ttls_are_configured() {
    let network = network(4, 9);
    assert_eq!(
        hole_punch_ttl(
            &network,
            &state(Some("192.168.1.146"), None),
            AddressType::IPV4
        ),
        4
    );
    assert_eq!(
        hole_punch_ttl(
            &network,
            &state(Some("100.64.0.7"), None),
            AddressType::IPV4
        ),
        9
    );
}

pub fn test_all() {
    test_ttl_follows_the_egress_source();
    test_ipv6_keeps_the_default();
    test_ttls_are_configured();
}
