//! Address checker - keep track of how other nodes are seeing our node's address on a per-protocol basis
//! Used to determine if our address has changed and if we should re-publish new PeerInfo

use super::*;

impl_veilid_log_facility!("net");

/// Number of 'existing dialinfo inconsistent' results in the cache during inbound-capable to trigger detection
pub const ADDRESS_INCONSISTENCY_DETECTION_COUNT: usize = 5;

/// Number of consistent results in the cache during outbound-only to trigger detection
pub const ADDRESS_CONSISTENCY_DETECTION_COUNT: usize = 5;

/// Length of consistent/inconsistent result cache for detection
pub const ADDRESS_CHECK_CACHE_SIZE: usize = 10;

// /// Length of consistent/inconsistent result cache for detection
// pub const ADDRESS_CHECK_PEER_COUNT: usize = 256;
// /// Frequency of address checks
// pub const PUBLIC_ADDRESS_CHECK_TASK_INTERVAL_SECS: u32 = 60;
// /// Duration we leave nodes in the inconsistencies table
// pub const PUBLIC_ADDRESS_INCONSISTENCY_TIMEOUT_US: TimestampDuration =
//     TimestampDuration::new(300_000_000u64); // 5 minutes
// /// How long we punish nodes for lying about our address
// pub const PUBLIC_ADDRESS_INCONSISTENCY_PUNISHMENT_TIMEOUT_US: TimestampDuration =
//     TimestampDuration::new(3_600_000_000_u64); // 60 minutes

/// Address checker config
#[derive(Debug)]
pub struct AddressCheckConfig {
    pub routing_domain_detect_address_changes: BTreeSet<RoutingDomain>,
    pub ip6_prefix_size: usize,
}

#[derive(Copy, Clone, Debug, PartialEq, Eq, Ord, PartialOrd, Hash)]
struct AddressCheckCacheKey(RoutingDomain, TransportType);

/// Address checker - keep track of how other nodes are seeing our node's address on a per-protocol basis
/// Used to determine if our address has changed and if we should re-publish new PeerInfo
pub struct AddressCheck {
    registry: VeilidComponentRegistry,
    config: AddressCheckConfig,
    net: Network,
    last_published_peer_info: BTreeMap<RoutingDomain, Arc<PeerInfo>>,
    port_checked_protocols: BTreeSet<AddressCheckCacheKey>,
    current_addresses: BTreeMap<AddressCheckCacheKey, HashSet<SocketAddress>>,
    // Used by InboundCapable to determine if we have changed our address or re-do our network class
    address_inconsistency_table: BTreeMap<AddressCheckCacheKey, usize>,
    // Last InboundCapable re-confirm trigger; suppresses duplicate re-fires from the same address
    last_inbound_triggered_address: BTreeMap<AddressCheckCacheKey, SocketAddress>,
    // Used by OutboundOnly to determine if we should re-do our network class
    address_consistency_table:
        BTreeMap<AddressCheckCacheKey, hashlink::LruCache<IpAddr, SocketAddress>>,
    // External addresses the last detection saw per family, plus the one a pending OutboundOnly
    // trigger asked about; a report naming one of them re-fires nothing
    detected_external_addresses: BTreeMap<(RoutingDomain, AddressType), BTreeSet<Address>>,
    // Transports the last detection reached no verdict on; unanimous reports over one re-fire
    unexamined_transports: BTreeMap<RoutingDomain, BTreeSet<TransportType>>,
}

impl fmt::Debug for AddressCheck {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("AddressCheck")
            .field("config", &self.config)
            .field(
                "routing_domain_detect_address_changes",
                &self.config.routing_domain_detect_address_changes,
            )
            //.field("net", &self.net)
            .field("last_published_peer_info", &self.last_published_peer_info)
            .field("port_checked_protocols", &self.port_checked_protocols)
            .field("current_addresses", &self.current_addresses)
            .field(
                "address_inconsistency_table",
                &self.address_inconsistency_table,
            )
            .field("address_consistency_table", &self.address_consistency_table)
            .field(
                "detected_external_addresses",
                &self.detected_external_addresses,
            )
            .field("unexamined_transports", &self.unexamined_transports)
            .field(
                "last_inbound_triggered_address",
                &self.last_inbound_triggered_address,
            )
            .finish()
    }
}

impl_veilid_component_accessors!(AddressCheck);

impl AddressCheck {
    pub fn new(net: Network) -> Self {
        let registry = net.registry();

        let config = registry.config();
        let routing_domain_detect_address_changes = net.routing_domains_detecting_address_changes();
        let ip6_prefix_size = config
            .internal()
            .network
            .max_connections_per_ip6_prefix_size as usize;

        let config = AddressCheckConfig {
            routing_domain_detect_address_changes,
            ip6_prefix_size,
        };

        Self {
            registry,
            config,
            net,
            last_published_peer_info: BTreeMap::new(),
            port_checked_protocols: BTreeSet::new(),
            current_addresses: BTreeMap::new(),
            address_inconsistency_table: BTreeMap::new(),
            last_inbound_triggered_address: BTreeMap::new(),
            address_consistency_table: BTreeMap::new(),
            detected_external_addresses: BTreeMap::new(),
            unexamined_transports: BTreeMap::new(),
        }
    }

    /// Record what a concluded dial info detection saw, so the OutboundOnly trigger re-fires
    /// only for an address it did not see, or over a transport it could not conclude
    pub fn report_dial_info_detection(
        &mut self,
        routing_domain: RoutingDomain,
        external_addresses: BTreeSet<Address>,
        unexamined_transports: BTreeSet<TransportType>,
    ) {
        for address_type in AddressTypeSet::all() {
            let seen: BTreeSet<Address> = external_addresses
                .iter()
                .filter(|a| a.address_type() == address_type)
                .copied()
                .collect();
            if !seen.is_empty() {
                self.detected_external_addresses
                    .insert((routing_domain, address_type), seen);
            }
        }
        self.unexamined_transports
            .insert(routing_domain, unexamined_transports);
    }

    /// Accept a report of any peerinfo that has been -published-
    pub fn report_peer_info_change(
        &mut self,
        routing_domain: RoutingDomain,
        opt_peer_info: Option<Arc<PeerInfo>>,
    ) {
        for protocol_type in ProtocolTypeSet::all() {
            for address_type in AddressTypeSet::all() {
                let acck = AddressCheckCacheKey(
                    routing_domain,
                    TransportType::new(protocol_type, address_type),
                );

                // Clear our current addresses so we can rebuild them for this routing domain
                self.current_addresses.remove(&acck);
                self.port_checked_protocols.remove(&acck);

                // Clear our history as well now so we start fresh when we get a new peer info
                self.address_inconsistency_table.remove(&acck);
                self.address_consistency_table.remove(&acck);

                // Leave the detected addresses and trigger dedups alone: re-arming detection
                // on every publish loops forever on a symmetric NAT's stable address
            }
        }

        if let Some(peer_info) = opt_peer_info {
            self.last_published_peer_info
                .insert(routing_domain, peer_info.clone());

            // Figure out which protocols we need to check the port for
            for did in peer_info.node_info().dial_info_detail_list() {
                // If the this is a dynamically translated dial info, and the protocol is not something a
                // router is going to change the source port for, then we should check the port.
                // Effectively, this ends up checking the port just for UDP protocols when the port would matter for hole punching.
                let acck = AddressCheckCacheKey(routing_domain, (&did.dial_info).into());
                let mut socket_address = did.dial_info.socket_address();
                if did.class.is_dynamically_translated()
                    && did
                        .dial_info
                        .protocol_type()
                        .low_level_protocol_type()
                        .socket_type()
                        == SocketType::Datagram
                {
                    self.port_checked_protocols.insert(acck);
                } else {
                    // If we're not checking the port, strip it before inserting
                    socket_address = socket_address.with_port(0);
                }

                // Keep any of the current dialinfo as expected current addresss with or without the port
                self.current_addresses
                    .entry(acck)
                    .or_default()
                    .insert(socket_address);
            }
        } else {
            self.last_published_peer_info.remove(&routing_domain);
        }
    }

    /// Accept a report of our address as seen by the other end of a flow, such
    /// as the StatusA response from a StatusQ
    pub fn report_socket_address_change(
        &mut self,
        routing_domain: RoutingDomain, // the routing domain used by this flow
        socket_address: SocketAddress, // the socket address as seen by the remote peer
        old_socket_address: Option<SocketAddress>, // the socket address previously for this peer
        flow: Flow,                    // the flow used
        reporting_peer: NodeRef,       // the peer's noderef reporting the socket address
    ) {
        // Check if this routing domain supports 'socket address change' detection
        // XXX: Only process the PublicInternet RoutingDomain for now, if we expand routing domain support we
        // should revisit this and make it more generic. (It may sufficient to remove this check and just
        // check 'routing_domain_detect_address_changes')
        if !matches!(routing_domain, RoutingDomain::PublicInternet) {
            return;
        }

        // Only process address changes for routing domains configured for it
        if !self
            .config
            .routing_domain_detect_address_changes
            .contains(&routing_domain)
        {
            return;
        }

        // While offline, peer reports may reflect stale or in-flight data; ignore them.
        if self
            .network_manager()
            .online_detector()
            .online_state(routing_domain)
            == OnlineState::Offline
        {
            return;
        }

        // Ignore reports from nodes that have no dial info (probably symmetric NAT)
        let Some(reporting_ni) = reporting_peer.node_info(routing_domain) else {
            return;
        };
        if !reporting_ni.has_any_dial_info() {
            // No dial info, ignore report
            return;
        }

        // Get the routing table and published peer info
        // If the peer info has invalid network class or is unconfirmed or unpublished this will return
        let Some(peer_info) = self.last_published_peer_info.get(&routing_domain).cloned() else {
            return;
        };

        // Ignore flows that do not start from our listening port (unbound connections etc),
        // because a router is going to map these differently
        let Some(pla) = self
            .net
            .get_preferred_local_address_by_key(flow.transport_type())
        else {
            return;
        };
        let Some(local) = flow.local() else {
            return;
        };
        if local.port() != pla.port() {
            veilid_log!(self debug target:"network_result", "ignoring address report because local port did not match listener: {} != {}", local.port(), pla.port());
            return;
        }

        // sender_info is untrusted, and a report naming a different address family
        // than its flow cannot describe our address, so counting it re-detects forever
        if socket_address.address_type() != flow.address_type() {
            veilid_log!(self debug target:"network_result",
                "ignoring address report {} on {} flow from {}: address family mismatch",
                socket_address,
                flow.address_type(),
                reporting_peer
            );
            return;
        }

        // Get the ip(block) this report is coming from
        let reporting_ipblock =
            ip_to_ipblock(self.config.ip6_prefix_size, flow.remote_address().ip_addr());

        // If the socket address reported is the same as the reporter, then this is coming through a relay
        // or it should be ignored due to local proximity (nodes on the same network block should not be trusted as
        // public ip address reporters, only disinterested parties)
        if reporting_ipblock == ip_to_ipblock(self.config.ip6_prefix_size, socket_address.ip_addr())
        {
            return;
        }

        // Process the state of the address checker and see if we need to
        // perform a full address check for this routing domain
        let needs_address_detection = if peer_info.node_info().has_dial_info() {
            self.detect_for_inbound_capable(
                routing_domain,
                socket_address,
                old_socket_address,
                flow,
                reporting_peer,
            )
        } else {
            self.detect_for_outbound_only(routing_domain, socket_address, flow, reporting_ipblock)
        };

        // Only log when the routing domain actually transitioned from confirmed; with multiple
        // acckeys hitting the threshold at once the redundant calls are no-ops
        if needs_address_detection
            && self
                .net
                .routing_domain_request_confirm_dial_info(routing_domain)
        {
            veilid_log!(self info
                "{:?} address has changed, detecting dial info",
                routing_domain
            );
            // Our external address moved, so transport evidence gathered at the old one is stale
            self.routing_table().clear_all_transport_evidence();
        }
    }

    fn matches_current_address(
        &self,
        acckey: AddressCheckCacheKey,
        mut socket_address: SocketAddress,
    ) -> bool {
        let port_checked = self.port_checked_protocols.contains(&acckey);
        if !port_checked {
            socket_address = socket_address.with_port(0);
        }

        self.current_addresses
            .get(&acckey)
            .map(|current_addresses| current_addresses.contains(&socket_address))
            .unwrap_or(false)
    }

    // If we are inbound capable, but start to see places where our sender info used to match our dial info
    // but no longer matches our dial info (count up the number of changes -away- from our dial info)
    // then trigger a detection of dial info and network class
    fn detect_for_inbound_capable(
        &mut self,
        routing_domain: RoutingDomain, // the routing domain used by this flow
        socket_address: SocketAddress, // the socket address as seen by the remote peer
        old_socket_address: Option<SocketAddress>, // the socket address previously for this peer
        flow: Flow,                    // the flow used
        reporting_peer: NodeRef,       // the peer's noderef reporting the socket address
    ) -> bool {
        // Get registry for logging because we have &mut self
        let registry = self.registry();

        let acckey = AddressCheckCacheKey(routing_domain, flow.transport_type());

        // Check the current socket address and see if it matches our current dial info
        let new_matches_current = self.matches_current_address(acckey, socket_address);

        // If we have something that matches our current dial info at all, consider it a validation
        if new_matches_current {
            self.address_inconsistency_table
                .entry(acckey)
                .and_modify(|ait| {
                    if *ait != 0 {
                        veilid_log!(registry debug "Resetting address inconsistency for {:?} due to match on flow {:?} from {}: current=[{}], old={}, new={}",
                            acckey,
                            flow,
                            reporting_peer,
                            self.current_addresses
                                .get(&acckey)
                                .map(|cas| cas.iter().map(|ca| ca.to_string()).collect::<Vec<String>>().join(", "))
                                .unwrap_or("None".to_string()),
                            old_socket_address.map(|osa| osa.to_string()).unwrap_or("None".to_string()),
                            socket_address.to_string(),
                        );
                    }
                    *ait = 0;
                })
                .or_insert(0);
            // Reset dedup so a future genuine inconsistency can re-fire detection
            self.last_inbound_triggered_address.remove(&acckey);
            return false;
        }

        // See if we have a case of switching away from our dial info
        let old_matches_current = old_socket_address
            .map(|osa| self.matches_current_address(acckey, osa))
            .unwrap_or(false);

        if old_matches_current {
            let val = *self
                .address_inconsistency_table
                .entry(acckey)
                .and_modify(|ait| {
                    *ait += 1;
                })
                .or_insert(1);
            veilid_log!(registry debug "Adding address inconsistency ({}) for {:?} due to address {} on flow {:?} from {}: current=[{}], old={}, new={}",
                val,
                acckey,
                socket_address,
                flow,
                reporting_peer,
                self.current_addresses
                    .get(&acckey)
                    .map(|cas| cas.iter().map(|ca| ca.to_string()).collect::<Vec<String>>().join(", "))
                    .unwrap_or("None".to_string()),
                old_socket_address.map(|osa| osa.to_string()).unwrap_or("None".to_string()),
                socket_address.to_string(),
            );
            if val < ADDRESS_INCONSISTENCY_DETECTION_COUNT {
                return false;
            }
            // Only fire when the off-current address actually changes from the last fire
            return self
                .last_inbound_triggered_address
                .insert(acckey, socket_address)
                != Some(socket_address);
        }

        false
    }

    // Outbound only, so no public dial info. Only when every recent reporter, from enough
    // distinct networks, names one external address has the translator settled enough that a
    // detection could find us inbound capable; a NAT that varies the address with the source
    // port never gets there, and a stable one is detected once
    fn detect_for_outbound_only(
        &mut self,
        routing_domain: RoutingDomain, // the routing domain used by this flow
        socket_address: SocketAddress, // the socket address as seen by the remote peer
        flow: Flow,                    // the flow used
        reporting_ipblock: IpAddr,     // the IP block this report came from
    ) -> bool {
        // Get registry for logging because we have &mut self
        let registry = self.registry();

        // Add the currently seen socket address into the consistency table
        let acckey = AddressCheckCacheKey(routing_domain, flow.transport_type());
        let cache = self
            .address_consistency_table
            .entry(acckey)
            .and_modify(|act| {
                act.insert(reporting_ipblock, socket_address);
            })
            .or_insert_with(|| {
                let mut lruc = hashlink::LruCache::new(ADDRESS_CHECK_CACHE_SIZE);
                lruc.insert(reporting_ipblock, socket_address);
                lruc
            });

        if cache.len() < ADDRESS_CONSISTENCY_DETECTION_COUNT {
            return false;
        }
        // Ports are the translator's to vary; the address is what must hold still
        let mut addresses = cache.iter().map(|(_, sa)| sa.address());
        let Some(address) = addresses.next() else {
            return false;
        };
        if !addresses.all(|a| a == address) {
            return false;
        }
        // One detection covers every transport, so an address it saw re-fires only over a
        // transport it reached no verdict on
        let family_key = (routing_domain, address.address_type());
        let seen = self
            .detected_external_addresses
            .get(&family_key)
            .is_some_and(|addresses| addresses.contains(&address));
        let unexamined = self
            .unexamined_transports
            .get_mut(&routing_domain)
            .is_some_and(|transports| transports.remove(&flow.transport_type()));
        if seen && !unexamined {
            return false;
        }
        self.detected_external_addresses
            .entry(family_key)
            .or_default()
            .insert(address);
        veilid_log!(registry debug "Address consistency detected for {:?}: {}", acckey, address);
        true
    }
}
