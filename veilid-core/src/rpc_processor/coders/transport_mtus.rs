use super::*;

/// Decodes a transport mtu list, stripping transports this build does not know unchecked, as dial
/// info does. A known transport's size outside the wire range, or one listed twice, is a bad list,
/// and the node info holding it is dropped rather than its sender punished: old nodes forward peer
/// info they cannot check.
pub fn decode_transport_mtus(
    reader: &::capnp::struct_list::Reader<'_, veilid_capnp::transport_mtu::Owned>,
) -> Result<TransportMtus, RPCError> {
    rpc_ignore_max_len!(reader, MAX_TRANSPORT_MTUS);
    let mut transport_mtus = TransportMtus::default();
    for tm_reader in reader.iter() {
        let low_level_protocol_type = match tm_reader.get_protocol_type() {
            FOURCC_PROTOCOL_TYPE_UDP => LowLevelProtocolType::UDP,
            FOURCC_PROTOCOL_TYPE_TCP => LowLevelProtocolType::TCP,
            _ => continue,
        };
        let address_type = match tm_reader.get_address_type() {
            FOURCC_ADDRESS_TYPE_IPV4 => AddressType::IPV4,
            FOURCC_ADDRESS_TYPE_IPV6 => AddressType::IPV6,
            _ => continue,
        };
        let mtu = tm_reader.get_mtu();
        if !(MIN_TRANSPORT_MTU..=MAX_TRANSPORT_MTU).contains(&mtu) {
            return Err(RPCError::ignore("transport mtu out of range"));
        }
        let low_level_transport_type = LowLevelTransportType {
            low_level_protocol_type,
            address_type,
        };
        if transport_mtus.get(low_level_transport_type).is_some() {
            return Err(RPCError::ignore("transport mtu listed twice"));
        }
        transport_mtus.set(low_level_transport_type, mtu);
    }
    Ok(transport_mtus)
}

pub fn encode_transport_mtus(
    transport_mtus: &TransportMtus,
    builder: &mut ::capnp::struct_list::Builder<'_, veilid_capnp::transport_mtu::Owned>,
) {
    for (n, (llt, mtu)) in transport_mtus.iter().enumerate() {
        let mut tm_builder = builder.reborrow().get(n as u32);
        tm_builder.set_protocol_type(match llt.low_level_protocol_type {
            LowLevelProtocolType::UDP => FOURCC_PROTOCOL_TYPE_UDP,
            LowLevelProtocolType::TCP => FOURCC_PROTOCOL_TYPE_TCP,
        });
        tm_builder.set_address_type(match llt.address_type {
            AddressType::IPV4 => FOURCC_ADDRESS_TYPE_IPV4,
            AddressType::IPV6 => FOURCC_ADDRESS_TYPE_IPV6,
        });
        tm_builder.set_mtu(mtu);
    }
}
