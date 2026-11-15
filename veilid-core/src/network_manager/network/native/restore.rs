//! What a routing domain's published state rests on, natively, and putting it back at startup.
//!
//! - PublicInternet: the dial info a detection found holds while the config that shapes it, the
//!   sockets it was found on, the egress our traffic leaves through and our global addresses are
//!   as they were. A WAN address change behind an unchanged egress is invisible here; the
//!   address check finds it from what peers report.
//! - LocalNetwork: everything it publishes comes from our interfaces, so its local addresses
//!   and the dial info they make are the whole requirement.

use super::*;
use igd_manager::{PortMapKey, PortMapValue};

/// What a saved PublicInternet state needs unchanged to be published again
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct PublicInternetValidity {
    /// Protocols, address families, capabilities and the static and interface dial info they make
    network_state: RoutingDomainNetworkState,
    /// The local sockets dial info detection maps from
    listeners: BTreeMap<TransportType, SocketAddr>,
    /// Where IPv4 traffic leaves this host
    egress_v4: Option<EgressIdentity>,
    /// Where IPv6 traffic leaves this host
    egress_v6: Option<EgressIdentity>,
    /// Our globally routable interface addresses
    global_addresses: Vec<IpAddr>,
    /// The protocol settings detected dial info is made with
    protocol_config: VeilidConfigProtocol,
    /// Whether port mappings are requested
    upnp: bool,
    /// Whether only relays carry our inbound traffic
    require_inbound_relay: bool,
}

impl PublicInternetValidity {
    /// What differs from `saved`
    fn changes(&self, saved: &Self) -> Vec<&'static str> {
        let mut changes = Vec::new();
        if self.network_state != saved.network_state {
            changes.push("network config or interface dial info");
        }
        if self.listeners != saved.listeners {
            changes.push("listeners");
        }
        if self.egress_v4 != saved.egress_v4 {
            changes.push("IPv4 egress");
        }
        if self.egress_v6 != saved.egress_v6 {
            changes.push("IPv6 egress");
        }
        if self.global_addresses != saved.global_addresses {
            changes.push("global addresses");
        }
        if self.protocol_config != saved.protocol_config {
            changes.push("protocol config");
        }
        if self.upnp != saved.upnp {
            changes.push("UPnP");
        }
        if self.require_inbound_relay != saved.require_inbound_relay {
            changes.push("inbound relay requirement");
        }
        changes
    }
}

/// What a saved LocalNetwork state needs unchanged to be published again
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct LocalNetworkValidity {
    /// Protocols, address families, capabilities and the interface dial info they make
    network_state: RoutingDomainNetworkState,
    /// Our local interface addresses with their netmasks
    local_addresses: Vec<IfAddr>,
}

impl LocalNetworkValidity {
    /// What differs from `saved`
    fn changes(&self, saved: &Self) -> Vec<&'static str> {
        let mut changes = Vec::new();
        if self.network_state != saved.network_state {
            changes.push("network config or interface dial info");
        }
        if self.local_addresses != saved.local_addresses {
            changes.push("local addresses");
        }
        changes
    }
}

/// Native PublicInternet state put back with its restore point
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
struct PublicInternetRestoreState {
    /// Port mappings and their leases
    port_maps: Vec<(PortMapKey, PortMapValue)>,
    /// External addresses and unexamined transports of the last detection
    last_dial_info_detection: Option<(BTreeSet<Address>, BTreeSet<TransportType>)>,
    /// Validator pool size at the last confirmation
    confirmed_validator_count: Option<usize>,
}

impl NativeNetwork {
    /// What `routing_domain`'s published state rests on now, and the state to keep with it
    pub(super) fn routing_domain_restore_state(
        &self,
        routing_domain: RoutingDomain,
    ) -> Option<PlatformRestoreState> {
        let network_state = self.last_network_state()?;
        match routing_domain {
            RoutingDomain::PublicInternet => {
                let validity = self.public_internet_validity(&network_state)?;
                let port_maps = self
                    .igd_manager()
                    .map(|igd_manager| igd_manager.port_maps())
                    .unwrap_or_default();
                let state = {
                    let inner = self.inner.lock();
                    PublicInternetRestoreState {
                        port_maps,
                        last_dial_info_detection: inner.last_dial_info_detection.clone(),
                        confirmed_validator_count: inner.confirmed_validator_count,
                    }
                };
                Some(PlatformRestoreState {
                    validity: serialize_table_value(&validity).0,
                    state: serialize_table_value(&state).0,
                })
            }
            RoutingDomain::LocalNetwork => {
                let validity = Self::local_network_validity(&network_state)?;
                Some(PlatformRestoreState {
                    validity: serialize_table_value(&validity).0,
                    state: Bytes::new(),
                })
            }
        }
    }

    /// At startup, put back `controller`'s last published state if what it rests on is
    /// unchanged; on a PublicInternet that changed, drop the transport evidence gathered on it.
    pub(super) fn restore_routing_domain(&self, controller: &dyn RoutingDomainController) {
        let routing_domain = controller.routing_domain();
        let Some(restore_point) = controller.read_dyn().get_restore_point() else {
            veilid_log!(self debug "[{}] no published state to restore", routing_domain);
            return;
        };
        let Some(network_state) = self.last_network_state() else {
            return;
        };
        let changes = match routing_domain {
            RoutingDomain::PublicInternet => {
                self.public_internet_validity(&network_state)
                    .map(|current| {
                        self.saved_validity::<PublicInternetValidity>(&restore_point)
                            .map_or(vec!["saved validity unreadable"], |saved| {
                                current.changes(&saved)
                            })
                    })
            }
            RoutingDomain::LocalNetwork => {
                Self::local_network_validity(&network_state).map(|current| {
                    self.saved_validity::<LocalNetworkValidity>(&restore_point)
                        .map_or(vec!["saved validity unreadable"], |saved| {
                            current.changes(&saved)
                        })
                })
            }
        };
        let Some(changes) = changes else {
            return;
        };
        if !changes.is_empty() {
            veilid_log!(self info "[{}] not restoring the last published state, changed: {}", routing_domain, changes.join(", "));
            if routing_domain == RoutingDomain::PublicInternet {
                // Transport failures blamed on peers may have been the old network's
                self.routing_table().clear_all_transport_evidence();
            }
            return;
        }

        let same_peer_info = controller.restore(&restore_point);
        if routing_domain == RoutingDomain::PublicInternet {
            self.restore_public_internet_state(&restore_point.platform.state);
        }
        veilid_log!(self info "[{}] restored the last published state, {}", routing_domain,
            if same_peer_info { "with its signed peer info" } else { "signing a new peer info" });
    }

    fn public_internet_validity(
        &self,
        network_state: &NetworkState,
    ) -> Option<PublicInternetValidity> {
        let rd_network_state = network_state
            .routing_domain_network_states
            .get(&RoutingDomain::PublicInternet)?
            .clone();
        let interface_address_state = &network_state.interface_address_state;
        // One guard, taken before the literal: a guard made inside it lives for the
        // whole expression and deadlocks the accessor
        let (listeners, upnp) = {
            let inner = self.inner.lock();
            (
                inner.preferred_local_addresses.clone(),
                inner.igd_manager.is_some(),
            )
        };
        Some(PublicInternetValidity {
            network_state: rd_network_state,
            listeners,
            egress_v4: interface_address_state.egress_v4.clone(),
            egress_v6: interface_address_state.egress_v6.clone(),
            global_addresses: interface_address_state
                .interface_addresses
                .iter()
                .map(|ifaddr| ifaddr.ip())
                .filter(|ip| Address::from_ip_addr(*ip).is_global())
                .collect(),
            protocol_config: self.config().network.protocol.clone(),
            upnp,
            require_inbound_relay: network_state.require_inbound_relay,
        })
    }

    fn local_network_validity(network_state: &NetworkState) -> Option<LocalNetworkValidity> {
        let rd_network_state = network_state
            .routing_domain_network_states
            .get(&RoutingDomain::LocalNetwork)?
            .clone();
        Some(LocalNetworkValidity {
            network_state: rd_network_state,
            local_addresses: network_state
                .interface_address_state
                .interface_addresses
                .iter()
                .filter(|ifaddr| Address::from_ip_addr(ifaddr.ip()).is_local())
                .cloned()
                .collect(),
        })
    }

    fn saved_validity<T: for<'a> Deserialize<'a>>(
        &self,
        restore_point: &RoutingDomainRestorePoint,
    ) -> Option<T> {
        self.table_store()
            .try_deserialize(&TableDBSerializedData(
                restore_point.platform.validity.clone(),
            ))
            .ok()
    }

    /// Put back the port mappings, last detection and validator count saved at publication, and
    /// count the current interface state as confirmed
    fn restore_public_internet_state(&self, state: &Bytes) {
        let state: PublicInternetRestoreState = match self
            .table_store()
            .try_deserialize(&TableDBSerializedData(state.clone()))
        {
            Ok(state) => state,
            Err(e) => {
                veilid_log!(self debug "[PublicInternet] saved network state unreadable: {}", e);
                PublicInternetRestoreState::default()
            }
        };
        if let Some(igd_manager) = self.igd_manager() {
            igd_manager.restore_port_maps(state.port_maps);
        }
        {
            let mut inner = self.inner.lock();
            inner.last_dial_info_detection = state.last_dial_info_detection;
            inner.confirmed_validator_count = state.confirmed_validator_count;
        }
        // The restored dial info stands for a confirmation on this interface state
        self.record_confirmed_interface_generation();
    }
}
