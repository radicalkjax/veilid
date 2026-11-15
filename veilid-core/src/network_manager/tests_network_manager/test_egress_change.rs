//! Which interface changes PublicInternet reacts to: only ones to the egress its traffic leaves from.

use super::*;

fn egress(interface: &str, source: &str, gateway: &str) -> Option<EgressIdentity> {
    Some(EgressIdentity {
        interface: interface.to_owned(),
        source: Some(source.parse().expect("test address")),
        gateways: vec![gateway.parse().expect("test gateway")],
    })
}

fn state(
    egress_v4: Option<EgressIdentity>,
    egress_v6: Option<EgressIdentity>,
) -> NetworkInterfaceAddressState {
    NetworkInterfaceAddressState {
        egress_v4,
        egress_v6,
        ..Default::default()
    }
}

fn wired() -> Option<EgressIdentity> {
    egress("en19", "192.168.1.146", "192.168.1.1")
}

fn global_v6() -> Option<EgressIdentity> {
    egress("en19", "2600:1702:7d80:a0f:ac:47d2:bae1:c87", "fe80::1")
}

fn v4() -> AddressTypeSet {
    AddressTypeSet::only(AddressType::IPV4)
}

fn both() -> AddressTypeSet {
    AddressTypeSet::all()
}

/// Whatever else in the state changed, the same egress is no change
fn test_same_egress_is_no_change() {
    let change = egress_change(&state(wired(), None), &state(wired(), None), v4(), v4());
    assert!(change.is_none());
    assert!(!change.is_full_move());
    assert_eq!(change.kept, v4());
}

/// The source, gateway or interface changing, or the egress going away, moves every family
/// in use
fn test_egress_change_is_a_full_move() {
    let before = state(wired(), None);
    for after in [
        state(egress("en19", "192.168.1.200", "192.168.1.1"), None),
        state(egress("en19", "192.168.1.146", "192.168.1.254"), None),
        state(egress("en0", "192.168.1.69", "192.168.1.1"), None),
        state(egress("utun9", "10.8.0.2", "10.8.0.1"), None),
        state(None, None),
    ] {
        let change = egress_change(&before, &after, v4(), v4());
        assert_eq!(change.moved, v4(), "{after:?}");
        assert!(change.appeared.is_empty());
        assert!(change.is_full_move());
    }
}

/// An egress appearing retires nothing
fn test_egress_appearing_is_not_a_move() {
    let change = egress_change(&state(None, None), &state(wired(), None), v4(), v4());
    assert_eq!(change.appeared, v4());
    assert!(change.moved.is_empty());
    assert!(!change.is_none());
    assert!(!change.is_full_move());
}

/// One family moving while the other stays is not the network going away
fn test_other_family_keeps_the_network() {
    let change = egress_change(
        &state(wired(), global_v6()),
        &state(egress("en0", "192.168.1.69", "192.168.1.1"), global_v6()),
        both(),
        both(),
    );
    assert_eq!(change.moved, v4());
    assert_eq!(change.kept, AddressTypeSet::only(AddressType::IPV6));
    assert!(!change.is_full_move());
}

/// IPv4 from this host's own translator has no path without the IPv6 route; a native IPv4
/// egress keeps the network when IPv6 goes
fn test_clat_v4_rides_the_v6_egress() {
    let clat_v4 = egress("en0", "192.0.0.2", "192.0.0.1");
    let change = egress_change(
        &state(clat_v4.clone(), global_v6()),
        &state(clat_v4, None),
        both(),
        both(),
    );
    assert_eq!(change.moved, both());
    assert!(change.kept.is_empty());
    assert!(change.is_full_move());

    let change = egress_change(
        &state(wired(), global_v6()),
        &state(wired(), None),
        both(),
        both(),
    );
    assert_eq!(change.moved, AddressTypeSet::only(AddressType::IPV6));
    assert_eq!(change.kept, v4());
    assert!(!change.is_full_move());
}

/// A family PublicInternet does not run on can come and go
fn test_unused_family_is_ignored() {
    let ula_v6 = egress("en19", "fd5c:1f15:d332:47c5::1", "fd5c:1f15:d332:47c5::");
    assert!(egress_change(
        &state(wired(), ula_v6.clone()),
        &state(wired(), None),
        v4(),
        v4()
    )
    .is_none());
    assert!(egress_change(&state(wired(), None), &state(wired(), ula_v6), v4(), v4()).is_none());
}

pub fn test_all() {
    test_same_egress_is_no_change();
    test_egress_change_is_a_full_move();
    test_egress_appearing_is_not_a_move();
    test_other_family_keeps_the_network();
    test_clat_v4_rides_the_v6_egress();
    test_unused_family_is_ignored();
}
