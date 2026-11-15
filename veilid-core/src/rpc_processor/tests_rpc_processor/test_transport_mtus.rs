use super::*;

fn llt(
    low_level_protocol_type: LowLevelProtocolType,
    address_type: AddressType,
) -> LowLevelTransportType {
    LowLevelTransportType {
        low_level_protocol_type,
        address_type,
    }
}

fn decode_from(entries: &[(u32, u32, u16)]) -> Result<TransportMtus, RPCError> {
    let mut message = capnp::message::Builder::new_default();
    let node_info = message.init_root::<veilid_capnp::node_info::Builder>();
    let mut list = node_info.init_receive_mtus(entries.len() as u32);
    for (n, (protocol_type, address_type, mtu)) in entries.iter().enumerate() {
        let mut tm = list.reborrow().get(n as u32);
        tm.set_protocol_type(*protocol_type);
        tm.set_address_type(*address_type);
        tm.set_mtu(*mtu);
    }
    let reader = message
        .get_root_as_reader::<veilid_capnp::node_info::Reader>()
        .unwrap();
    decode_transport_mtus(&reader.get_receive_mtus().unwrap())
}

fn decode_probe(size: u16, padding_len: usize) -> Result<MtuProbe, RPCError> {
    let mut message = capnp::message::Builder::new_default();
    let mut mtu_probe = message.init_root::<veilid_capnp::mtu_probe::Builder>();
    mtu_probe.set_size(size);
    mtu_probe.set_padding(&vec![0u8; padding_len]);
    let reader = message
        .get_root_as_reader::<veilid_capnp::mtu_probe::Reader>()
        .unwrap();
    decode_mtu_probe(&reader)
}

/// Unknown transports and WS are stripped whatever size they carry, and what is kept round-trips
pub fn test_transport_mtus_skip_what_they_cannot_use() {
    let unknown = u32::from_be_bytes(*b"zzzz");
    let mtus = decode_from(&[
        (FOURCC_PROTOCOL_TYPE_UDP, FOURCC_ADDRESS_TYPE_IPV4, 1472),
        (FOURCC_PROTOCOL_TYPE_TCP, FOURCC_ADDRESS_TYPE_IPV6, 1440),
        (unknown, FOURCC_ADDRESS_TYPE_IPV4, 1400),
        (unknown, FOURCC_ADDRESS_TYPE_IPV4, 0),
        (unknown, FOURCC_ADDRESS_TYPE_IPV6, 9000),
        (FOURCC_PROTOCOL_TYPE_UDP, unknown, u16::MAX),
        (FOURCC_PROTOCOL_TYPE_WS, FOURCC_ADDRESS_TYPE_IPV4, 1400),
    ])
    .unwrap();
    assert_eq!(mtus.len(), 2);
    assert_eq!(
        mtus.get(llt(LowLevelProtocolType::UDP, AddressType::IPV4)),
        Some(1472)
    );
    assert_eq!(
        mtus.get(llt(LowLevelProtocolType::TCP, AddressType::IPV6)),
        Some(1440)
    );
    assert_eq!(
        mtus.get(llt(LowLevelProtocolType::UDP, AddressType::IPV6)),
        None
    );

    let mut message = capnp::message::Builder::new_default();
    let node_info = message.init_root::<veilid_capnp::node_info::Builder>();
    let mut list = node_info.init_receive_mtus(mtus.len() as u32);
    encode_transport_mtus(&mtus, &mut list);
    let reader = message
        .get_root_as_reader::<veilid_capnp::node_info::Reader>()
        .unwrap();
    assert_eq!(
        decode_transport_mtus(&reader.get_receive_mtus().unwrap()).unwrap(),
        mtus
    );
}

/// A known transport's size outside the wire range, or a transport listed twice, drops the list
/// without punishing whoever forwarded it
pub fn test_transport_mtus_reject_bad_lists() {
    let udp4 = |mtu| (FOURCC_PROTOCOL_TYPE_UDP, FOURCC_ADDRESS_TYPE_IPV4, mtu);
    assert!(decode_from(&[udp4(MIN_TRANSPORT_MTU)]).is_ok());
    assert!(decode_from(&[udp4(MAX_TRANSPORT_MTU)]).is_ok());
    for bad in [0, 1, MIN_TRANSPORT_MTU - 1, MAX_TRANSPORT_MTU + 1, u16::MAX] {
        assert!(
            matches!(decode_from(&[udp4(bad)]), Err(RPCError::Ignore(_))),
            "size {bad}"
        );
    }
    assert!(matches!(
        decode_from(&[udp4(1472), udp4(1200)]),
        Err(RPCError::Ignore(_))
    ));
}

/// A list longer than the wire limit is refused, whatever its transports
pub fn test_transport_mtus_bound_the_list() {
    let unknown = u32::from_be_bytes(*b"zzzz");
    let entry = (unknown, FOURCC_ADDRESS_TYPE_IPV4, 1472);
    assert!(decode_from(&[entry; MAX_TRANSPORT_MTUS]).is_ok());
    assert!(matches!(
        decode_from(&[entry; MAX_TRANSPORT_MTUS + 1]),
        Err(RPCError::Ignore(_))
    ));
}

/// Node info from an older node has no list, which decodes as no sizes
pub fn test_transport_mtus_absent_from_older_node_info() {
    let mut message = capnp::message::Builder::new_default();
    message.init_root::<veilid_capnp::node_info::Builder>();
    let reader = message
        .get_root_as_reader::<veilid_capnp::node_info::Reader>()
        .unwrap();
    assert!(!reader.has_receive_mtus());
}

/// A probe's size stays within the wire range and its padding fits the datagram it claims; a
/// probe comes from its sender, so a bad one is a protocol error
pub fn test_mtu_probe_bounds() {
    let fits = usize::from(MAX_TRANSPORT_MTU) - FRAME_HEADER_LEN - ENV0_MIN_ENVELOPE_SIZE;
    assert!(decode_probe(MAX_TRANSPORT_MTU, fits).is_ok());
    assert!(decode_probe(MIN_TRANSPORT_MTU, 0).is_ok());
    assert!(matches!(
        decode_probe(MAX_TRANSPORT_MTU, fits + 1),
        Err(RPCError::Protocol(_))
    ));
    for bad in [0, 1, MIN_TRANSPORT_MTU - 1, MAX_TRANSPORT_MTU + 1, u16::MAX] {
        assert!(
            matches!(decode_probe(bad, 0), Err(RPCError::Protocol(_))),
            "size {bad}"
        );
    }
    assert!(matches!(
        decode_probe(MAX_TRANSPORT_MTU, MAX_MTU_PROBE_PADDING_LEN + 1),
        Err(RPCError::Ignore(_))
    ));
}
