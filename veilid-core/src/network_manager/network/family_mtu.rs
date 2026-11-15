//! One address family's packet sizes: what its egress interface allows, and where a probe left it

use super::*;

/// Larger interfaces (jumbo frames) still meet the internet at Ethernet's MTU
const MAX_IP_MTU: u32 = 1500;

/// Where a family's packet size stands
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum FamilyMtuState {
    /// Waiting for a probe before the family settles
    Detecting,
    /// Settled at the safe size with no peer to probe; one that can answer starts a probe
    Unprobed,
    /// Full-size packets crossed both ways
    Full,
    /// Full-size packets went missing where small ones got through
    BlackHole,
    /// The interface allows no more than the safe size, or will not say
    Fixed,
}

/// One address family's packet sizes
#[derive(Debug, Clone)]
pub(crate) struct FamilyMtu {
    /// Largest IP packet the egress interface carries
    pub interface_mtu: Option<u32>,
    /// Where the family's size stands
    pub state: FamilyMtuState,
    /// The interface address generation the family started over at
    pub generation: u64,
    /// Peers already probed since then
    pub tried: BTreeSet<NodeId>,
    /// A pass of probes is out for this generation
    pub probing: bool,
}

impl FamilyMtu {
    pub fn new(interface_mtu: Option<u32>, address_type: AddressType, generation: u64) -> Self {
        let state = match full_mtus(interface_mtu, address_type) {
            Some((udp, _)) if udp > SAFE_TRANSPORT_MTU => FamilyMtuState::Detecting,
            _ => FamilyMtuState::Fixed,
        };
        Self {
            interface_mtu,
            state,
            generation,
            tried: BTreeSet::new(),
            probing: false,
        }
    }

    /// The (UDP, TCP) sizes the egress interface allows at most
    pub fn limits(&self, address_type: AddressType) -> (u16, u16) {
        full_mtus(self.interface_mtu, address_type)
            .unwrap_or((SAFE_TRANSPORT_MTU, SAFE_TRANSPORT_MTU))
    }

    /// The (UDP, TCP) sizes the family uses now
    pub fn mtus(&self, address_type: AddressType) -> (u16, u16) {
        match (self.state, full_mtus(self.interface_mtu, address_type)) {
            (FamilyMtuState::Full, Some(full)) => full,
            (_, Some((udp, tcp))) => (udp.min(SAFE_TRANSPORT_MTU), tcp.min(SAFE_TRANSPORT_MTU)),
            (_, None) => (SAFE_TRANSPORT_MTU, SAFE_TRANSPORT_MTU),
        }
    }
}

/// What one peer's probe showed
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum MtuProbeOutcome {
    /// The full-size question and answer both crossed
    Full,
    /// The plain answer came back and the full-size one did not
    Lost,
    /// Nothing either way, or the peer could not answer at our size
    Inconclusive,
}

/// Losses, among the peers one pass probed, that make a black hole when none went full
const MTU_PROBE_BLACK_HOLE_LOSSES: usize = 2;

/// What a pass of probes settles: one full round trip, or enough losses without one
pub(crate) fn probe_verdict(outcomes: &[MtuProbeOutcome]) -> Option<FamilyMtuState> {
    if outcomes.contains(&MtuProbeOutcome::Full) {
        return Some(FamilyMtuState::Full);
    }
    let losses = outcomes
        .iter()
        .filter(|outcome| **outcome == MtuProbeOutcome::Lost)
        .count();
    (losses >= MTU_PROBE_BLACK_HOLE_LOSSES).then_some(FamilyMtuState::BlackHole)
}

/// The (UDP, TCP) payload sizes of an interface mtu, which reaches the internet at Ethernet's
fn full_mtus(interface_mtu: Option<u32>, address_type: AddressType) -> Option<(u16, u16)> {
    let ip_mtu = interface_mtu?.min(MAX_IP_MTU);
    let (udp_headers, tcp_headers) = match address_type {
        AddressType::IPV4 => (28, 40),
        AddressType::IPV6 => (48, 60),
    };
    let udp = u16::try_from(ip_mtu.checked_sub(udp_headers)?).ok()?;
    let tcp = u16::try_from(ip_mtu.checked_sub(tcp_headers)?).ok()?;
    Some((udp, tcp))
}
