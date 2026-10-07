use crate::*;

pub fn test_ipv4addr_predicates() {
    // RFC 1918 private
    assert!(ipv4addr_is_private(&"10.0.0.1".parse().unwrap()));
    assert!(ipv4addr_is_private(&"172.16.0.1".parse().unwrap()));
    assert!(ipv4addr_is_private(&"192.168.1.1".parse().unwrap()));
    assert!(!ipv4addr_is_private(&"8.8.8.8".parse().unwrap()));

    // RFC 3927 link-local / APIPA
    assert!(ipv4addr_is_link_local(&"169.254.1.1".parse().unwrap()));
    assert!(!ipv4addr_is_link_local(&"169.255.1.1".parse().unwrap()));

    // RFC 6598 CGNAT shared
    assert!(ipv4addr_is_shared(&"100.64.0.1".parse().unwrap()));
    assert!(ipv4addr_is_shared(&"100.127.255.254".parse().unwrap()));
    assert!(!ipv4addr_is_shared(&"100.63.0.1".parse().unwrap()));
    assert!(!ipv4addr_is_shared(&"100.128.0.1".parse().unwrap()));

    // RFC 6890 IETF protocol assignment (covers RFC 7335 CLAT46 192.0.0.0/29)
    assert!(ipv4addr_is_ietf_protocol_assignment(
        &"192.0.0.2".parse().unwrap()
    ));
    assert!(ipv4addr_is_ietf_protocol_assignment(
        &"192.0.0.7".parse().unwrap()
    ));
    assert!(!ipv4addr_is_ietf_protocol_assignment(
        &"192.0.1.1".parse().unwrap()
    ));

    // is_global
    assert!(ipv4addr_is_global(&"8.8.8.8".parse().unwrap()));
    assert!(!ipv4addr_is_global(&"10.0.0.1".parse().unwrap()));
    assert!(!ipv4addr_is_global(&"192.0.0.2".parse().unwrap()));
    assert!(!ipv4addr_is_global(&"100.64.0.1".parse().unwrap()));
    assert!(!ipv4addr_is_global(&"169.254.1.1".parse().unwrap()));
    // PCP anycast addresses are explicitly global
    assert!(ipv4addr_is_global(&"192.0.0.9".parse().unwrap()));
    assert!(ipv4addr_is_global(&"192.0.0.10".parse().unwrap()));
}

pub fn test_ipv6addr_predicates() {
    // Unique-local fc00::/7
    assert!(ipv6addr_is_unique_local(&"fc00::1".parse().unwrap()));
    assert!(ipv6addr_is_unique_local(&"fd00::1".parse().unwrap()));
    assert!(!ipv6addr_is_unique_local(&"fe00::1".parse().unwrap()));

    // Link-local fe80::/10
    assert!(ipv6addr_is_unicast_link_local(&"fe80::1".parse().unwrap()));
    assert!(ipv6addr_is_unicast_link_local(&"febf::1".parse().unwrap()));
    assert!(!ipv6addr_is_unicast_link_local(&"fec0::1".parse().unwrap()));

    // NAT64 well-known 64:ff9b::/96 (RFC 6052)
    assert!(ipv6addr_is_nat64_well_known(
        &"64:ff9b::8.8.8.8".parse().unwrap()
    ));
    assert!(ipv6addr_is_nat64_well_known(&"64:ff9b::0".parse().unwrap()));
    // Different prefix
    assert!(!ipv6addr_is_nat64_well_known(
        &"64:ff9c::1".parse().unwrap()
    ));
    // Inside the prefix but outside the /96 (high bytes set in the IPv4-mapped portion)
    assert!(ipv6addr_is_nat64_well_known(
        &"64:ff9b::ffff:ffff".parse().unwrap()
    ));

    // Teredo 2001::/32 (RFC 4380)
    assert!(ipv6addr_is_teredo(&"2001:0:1::1".parse().unwrap()));
    assert!(!ipv6addr_is_teredo(&"2001:1::1".parse().unwrap()));
    assert!(!ipv6addr_is_teredo(&"2001:db8::1".parse().unwrap()));

    // 6to4 2002::/16 (RFC 3056)
    assert!(ipv6addr_is_6to4(&"2002:c0a8:101::1".parse().unwrap()));
    assert!(!ipv6addr_is_6to4(&"2003::1".parse().unwrap()));

    // is_global covers NAT64/Teredo/6to4 (they wrap public IPv4)
    assert!(ipv6addr_is_global(&"2001::1".parse().unwrap()));
    assert!(ipv6addr_is_global(&"2002::1".parse().unwrap()));
    assert!(ipv6addr_is_global(&"64:ff9b::8.8.8.8".parse().unwrap()));
    assert!(!ipv6addr_is_global(&"fe80::1".parse().unwrap()));
    assert!(!ipv6addr_is_global(&"fc00::1".parse().unwrap()));
    assert!(!ipv6addr_is_global(&"::1".parse().unwrap()));
    assert!(!ipv6addr_is_global(&"2001:db8::1".parse().unwrap()));
}
