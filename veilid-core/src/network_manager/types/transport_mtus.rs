use super::*;
use std::sync::atomic::AtomicU16;

/// What every path carries: inside IPv6's 1280-byte minimum MTU on UDP and TCP alike
pub(crate) const SAFE_TRANSPORT_MTU: u16 = 1200;
/// Smallest size a node may advertise or probe for: IPv4's minimum reassembly size
pub(crate) const MIN_TRANSPORT_MTU: u16 = veilid_capnp::MIN_TRANSPORT_MTU as u16;
/// Largest size a node may advertise or probe for: Ethernet's MTU
pub(crate) const MAX_TRANSPORT_MTU: u16 = veilid_capnp::MAX_TRANSPORT_MTU as u16;
/// How far under its size a filled probe may land, as padding shifts lz4's overhead
pub(crate) const MTU_PROBE_FILL_SLACK: u16 = 16;

/// Largest packet a node takes on each low-level transport without fragmentation. UDP sizes count
/// datagram payload bytes and TCP sizes count segment payload bytes.
#[derive(
    Debug, Clone, Copy, Default, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize,
)]
#[serde(default)]
pub(crate) struct TransportMtus {
    udp_v4: Option<u16>,
    udp_v6: Option<u16>,
    tcp_v4: Option<u16>,
    tcp_v6: Option<u16>,
}

impl TransportMtus {
    const ALL: [LowLevelTransportType; 4] = [
        LowLevelTransportType {
            low_level_protocol_type: LowLevelProtocolType::UDP,
            address_type: AddressType::IPV4,
        },
        LowLevelTransportType {
            low_level_protocol_type: LowLevelProtocolType::UDP,
            address_type: AddressType::IPV6,
        },
        LowLevelTransportType {
            low_level_protocol_type: LowLevelProtocolType::TCP,
            address_type: AddressType::IPV4,
        },
        LowLevelTransportType {
            low_level_protocol_type: LowLevelProtocolType::TCP,
            address_type: AddressType::IPV6,
        },
    ];

    fn slot_mut(&mut self, low_level_transport_type: LowLevelTransportType) -> &mut Option<u16> {
        match (
            low_level_transport_type.low_level_protocol_type,
            low_level_transport_type.address_type,
        ) {
            (LowLevelProtocolType::UDP, AddressType::IPV4) => &mut self.udp_v4,
            (LowLevelProtocolType::UDP, AddressType::IPV6) => &mut self.udp_v6,
            (LowLevelProtocolType::TCP, AddressType::IPV4) => &mut self.tcp_v4,
            (LowLevelProtocolType::TCP, AddressType::IPV6) => &mut self.tcp_v6,
        }
    }

    /// The mtu for a transport, if the node gave one
    pub fn get(&self, low_level_transport_type: LowLevelTransportType) -> Option<u16> {
        match (
            low_level_transport_type.low_level_protocol_type,
            low_level_transport_type.address_type,
        ) {
            (LowLevelProtocolType::UDP, AddressType::IPV4) => self.udp_v4,
            (LowLevelProtocolType::UDP, AddressType::IPV6) => self.udp_v6,
            (LowLevelProtocolType::TCP, AddressType::IPV4) => self.tcp_v4,
            (LowLevelProtocolType::TCP, AddressType::IPV6) => self.tcp_v6,
        }
    }

    pub fn set(&mut self, low_level_transport_type: LowLevelTransportType, mtu: u16) {
        *self.slot_mut(low_level_transport_type) = Some(mtu);
    }

    pub fn iter(&self) -> impl Iterator<Item = (LowLevelTransportType, u16)> + '_ {
        Self::ALL
            .into_iter()
            .filter_map(|llt| self.get(llt).map(|mtu| (llt, mtu)))
    }

    pub fn len(&self) -> usize {
        self.iter().count()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

impl fmt::Display for TransportMtus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let entries = self
            .iter()
            .map(|(llt, mtu)| format!("{llt}:{mtu}"))
            .collect::<Vec<_>>()
            .join(",");
        write!(f, "{}", entries.string_if_empty("None"))
    }
}

/// Our own largest packets per low-level transport
#[derive(Debug)]
pub(crate) struct OwnMtus {
    /// What we send at most, and advertise we take
    mtus: [AtomicU16; 4],
    /// What our interfaces allow at most
    limits: [AtomicU16; 4],
}

impl OwnMtus {
    pub fn new() -> Self {
        Self {
            mtus: core::array::from_fn(|_| AtomicU16::new(SAFE_TRANSPORT_MTU)),
            limits: core::array::from_fn(|_| AtomicU16::new(SAFE_TRANSPORT_MTU)),
        }
    }

    fn slot(low_level_transport_type: LowLevelTransportType) -> usize {
        match (
            low_level_transport_type.low_level_protocol_type,
            low_level_transport_type.address_type,
        ) {
            (LowLevelProtocolType::UDP, AddressType::IPV4) => 0,
            (LowLevelProtocolType::UDP, AddressType::IPV6) => 1,
            (LowLevelProtocolType::TCP, AddressType::IPV4) => 2,
            (LowLevelProtocolType::TCP, AddressType::IPV6) => 3,
        }
    }

    pub fn get(&self, low_level_transport_type: LowLevelTransportType) -> u16 {
        self.mtus[Self::slot(low_level_transport_type)].load(Ordering::Relaxed)
    }

    pub fn limit(&self, low_level_transport_type: LowLevelTransportType) -> u16 {
        self.limits[Self::slot(low_level_transport_type)].load(Ordering::Relaxed)
    }

    #[cfg_attr(all(target_arch = "wasm32", target_os = "unknown"), expect(dead_code))]
    pub fn set(&self, low_level_transport_type: LowLevelTransportType, mtu: u16, limit: u16) {
        let slot = Self::slot(low_level_transport_type);
        self.mtus[slot].store(mtu, Ordering::Relaxed);
        self.limits[slot].store(limit, Ordering::Relaxed);
    }
}
