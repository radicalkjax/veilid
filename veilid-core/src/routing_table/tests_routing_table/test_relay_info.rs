use crate::routing_table::*;

fn relay_info(timestamp: u64, relay_kind: RelayKind, receive_mtus: TransportMtus) -> RelayInfo {
    RelayInfo::new(
        Timestamp::new(timestamp),
        NodeIdGroup::from(NodeId::new(CRYPTO_KIND_VLD0, BareNodeId::default())),
        ProtocolTypeSet::all(),
        AddressTypeSet::all(),
        vec![],
        relay_kind,
        receive_mtus,
    )
}

pub fn test_relay_info_equivalent() {
    info!("--- test_relay_info_equivalent ---");

    let no_mtus = TransportMtus::default;
    let inbound = relay_info(1, RelayKind::Inbound, no_mtus());
    assert!(inbound.equivalent(&relay_info(2, RelayKind::Inbound, no_mtus())));
    assert!(!inbound.equivalent(&relay_info(1, RelayKind::Outbound, no_mtus())));
    assert!(!relay_info(1, RelayKind::Outbound, no_mtus()).equivalent(&inbound));

    // A relay whose mtus changed must be republished
    let mut mtus = TransportMtus::default();
    mtus.set(
        LowLevelTransportType {
            low_level_protocol_type: LowLevelProtocolType::UDP,
            address_type: AddressType::IPV4,
        },
        1472,
    );
    assert!(!inbound.equivalent(&relay_info(1, RelayKind::Inbound, mtus)));
}
