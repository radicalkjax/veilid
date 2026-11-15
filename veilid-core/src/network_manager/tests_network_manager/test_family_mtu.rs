use super::*;

fn family_mtu(interface_mtu: Option<u32>, state: FamilyMtuState) -> FamilyMtu {
    FamilyMtu {
        interface_mtu,
        state,
        generation: 0,
        tried: BTreeSet::new(),
        probing: false,
    }
}

/// A proven family sends what its interface carries, but jumbo frames meet the internet at 1500
pub fn test_full_sizes_follow_the_interface() {
    let ethernet = family_mtu(Some(1500), FamilyMtuState::Full);
    assert_eq!(ethernet.mtus(AddressType::IPV4), (1472, 1460));
    assert_eq!(ethernet.mtus(AddressType::IPV6), (1452, 1440));
    let jumbo = family_mtu(Some(9001), FamilyMtuState::Full);
    assert_eq!(jumbo.mtus(AddressType::IPV4), (1472, 1460));
    assert_eq!(jumbo.limits(AddressType::IPV4), (1472, 1460));
}

/// A family starts at the safe size, and only a probe lifts it
pub fn test_family_starts_at_the_safe_size() {
    let family = FamilyMtu::new(Some(1500), AddressType::IPV4, 1);
    assert_eq!(family.state, FamilyMtuState::Detecting);
    assert_eq!(
        family.mtus(AddressType::IPV4),
        (SAFE_TRANSPORT_MTU, SAFE_TRANSPORT_MTU)
    );
    assert_eq!(family.limits(AddressType::IPV4), (1472, 1460));
    for state in [FamilyMtuState::Unprobed, FamilyMtuState::BlackHole] {
        assert_eq!(
            family_mtu(Some(1500), state).mtus(AddressType::IPV4),
            (SAFE_TRANSPORT_MTU, SAFE_TRANSPORT_MTU)
        );
    }
    // IPv6's minimum MTU still leaves room above the safe size to probe for
    let minimum = FamilyMtu::new(Some(1280), AddressType::IPV6, 1);
    assert_eq!(minimum.state, FamilyMtuState::Detecting);
    assert_eq!(minimum.limits(AddressType::IPV6), (1232, 1220));
}

/// An interface below the safe size is used as it is and never probed; an unknown one stays safe
pub fn test_small_or_unknown_interfaces_are_fixed() {
    let small = FamilyMtu::new(Some(1200), AddressType::IPV4, 1);
    assert_eq!(small.state, FamilyMtuState::Fixed);
    assert_eq!(small.mtus(AddressType::IPV4), (1172, 1160));

    let unknown = FamilyMtu::new(None, AddressType::IPV6, 1);
    assert_eq!(unknown.state, FamilyMtuState::Fixed);
    assert_eq!(
        unknown.mtus(AddressType::IPV6),
        (SAFE_TRANSPORT_MTU, SAFE_TRANSPORT_MTU)
    );
    assert_eq!(
        unknown.limits(AddressType::IPV6),
        (SAFE_TRANSPORT_MTU, SAFE_TRANSPORT_MTU)
    );
}

/// One full round trip settles a family; a black hole takes two losses and no full round trip
pub fn test_probe_verdict_hears_every_peer() {
    use MtuProbeOutcome::*;
    assert_eq!(probe_verdict(&[]), None);
    assert_eq!(probe_verdict(&[Lost]), None);
    assert_eq!(probe_verdict(&[Inconclusive; 4]), None);
    assert_eq!(
        probe_verdict(&[Lost, Inconclusive, Lost]),
        Some(FamilyMtuState::BlackHole)
    );
    assert_eq!(
        probe_verdict(&[Lost, Lost, Full]),
        Some(FamilyMtuState::Full),
        "two colluding losses do not outvote a full round trip"
    );
    assert_eq!(probe_verdict(&[Full]), Some(FamilyMtuState::Full));
}

pub fn test_all() {
    test_full_sizes_follow_the_interface();
    test_family_starts_at_the_safe_size();
    test_small_or_unknown_interfaces_are_fixed();
    test_probe_verdict_hears_every_peer();
}
