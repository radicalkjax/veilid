use super::*;

impl_veilid_log_facility!("rtab");

/// Drop a relay whose protected connections survive less than this on average.
pub(super) const RELAY_MIN_PROTECTED_CONNECTION_TM90: TimestampDuration =
    TimestampDuration::new_secs(20);

/// Public Internet routing domain internals
pub struct PublicInternetRoutingDomainDetail {
    /// Registry accessor
    registry: VeilidComponentRegistry,
    /// The interface networks that are in this domain
    interface_addresses: Vec<IfAddr>,
    /// Common implementation for all routing domains
    common: RoutingDomainDetailCommon,
    /// Published peer info for this routing domain
    published_peer_info: Mutex<Option<Arc<PeerInfo>>>,
    /// Last relay requirements for this routing domain
    opt_last_relay_requirements: ArcSwapOption<RelayRequirements>,
}

impl fmt::Debug for PublicInternetRoutingDomainDetail {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("PublicInternetRoutingDomainDetail")
            // .field("registry", &self.registry)
            .field("interface_addresses", &self.interface_addresses)
            .field("common", &self.common)
            .field("published_peer_info", &self.published_peer_info)
            .field(
                "opt_last_relay_requirements",
                &self.opt_last_relay_requirements,
            )
            .finish()
    }
}

impl fmt::Display for PublicInternetRoutingDomainDetail {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "Interface Addresses:\n{}\nPublished Peer Info:\n{}\nLast Relay Requirements:\n{}\n{}",
            indent_all_string(
                f.to_multiline_indexed_string(
                    self.interface_addresses
                        .iter()
                        .map(|intf_addr| f.to_string(intf_addr))
                )
                .string_if_empty("None")
            ),
            indent_all_string(f.to_string_opt(self.published_peer_info.lock().as_ref())),
            indent_all_string(f.to_string_opt(self.opt_last_relay_requirements.load().as_ref())),
            f.to_string(&self.common)
        )
    }
}

impl_veilid_component_accessors!(PublicInternetRoutingDomainDetail);

impl RoutingDomainDetailAccessors for PublicInternetRoutingDomainDetail {
    fn common(&self) -> &RoutingDomainDetailCommon {
        &self.common
    }
    fn common_mut(&mut self) -> &mut RoutingDomainDetailCommon {
        &mut self.common
    }
}

impl PublicInternetRoutingDomainDetail {
    pub fn new(registry: VeilidComponentRegistry) -> Self {
        Self {
            interface_addresses: Default::default(),
            common: RoutingDomainDetailCommon::new(registry.clone(), RoutingDomain::PublicInternet),
            published_peer_info: Default::default(),
            opt_last_relay_requirements: ArcSwapOption::new(None),
            registry,
        }
    }

    #[expect(dead_code)]
    pub fn interface_addresses(&self) -> Vec<IfAddr> {
        self.interface_addresses.clone()
    }

    pub fn set_interface_addresses(&mut self, mut interface_addresses: Vec<IfAddr>) -> bool {
        // Filter out any networks that are only locally routable as the routing domains should not overlap
        interface_addresses.retain(|x| Address::from_ip_addr(x.ip()).is_global());
        interface_addresses.sort_unstable();
        if interface_addresses == self.interface_addresses {
            return false;
        }
        self.interface_addresses = interface_addresses;
        true
    }

    /// Candidate filter: a node must have proven it can carry our traffic before we
    /// pick it as a relay.
    pub fn make_relay_node_filter(&self) -> impl Fn(&BucketEntrySnapshot) -> bool {
        let inner = self.make_relay_node_filter_inner(true);
        move |snap| inner(snap).is_none()
    }

    /// Retention filter for a relay we already chose. Same disqualifiers, except that
    /// missing evidence is not one: we only ever dial a relay over the flow we already
    /// have, so per-transport dial evidence may never appear. Only positive failure
    /// evidence drops an incumbent.
    pub fn make_relay_retention_filter(&self) -> impl Fn(&BucketEntrySnapshot) -> bool {
        let inner = self.make_relay_node_filter_inner(false);
        move |snap| inner(snap).is_none()
    }

    /// Why a relay we already chose should be dropped, or None to keep it
    pub fn make_relay_retention_filter_reason(
        &self,
    ) -> impl Fn(&BucketEntrySnapshot) -> Option<&'static str> {
        self.make_relay_node_filter_inner(false)
    }

    /// Shared body of the relay filters: the disqualifying reason, or None to accept.
    /// Transport health keys off last_seen_ts because it is monotone; a streak measure
    /// would reset on a single lost question and churn relays under congestion.
    fn make_relay_node_filter_inner(
        &self,
        require_evidence: bool,
    ) -> impl Fn(&BucketEntrySnapshot) -> Option<&'static str> {
        let ip6_prefix_size = self
            .config()
            .internal()
            .network
            .max_connections_per_ip6_prefix_size as usize;

        // Get all our outbound protocol/address types
        let outbound_dif = self.outbound_dial_info_filter();

        // Get our own peer info
        let own_peer_info = self.get_peer_info();

        move |snap: &BucketEntrySnapshot| {
            // Defensively exclude nodes that aren't responsive
            if !snap.state.is_responsive() {
                return Some("not responsive");
            }

            // Ensure this node is not on the local network
            if snap
                .routing_domain_set()
                .contains(RoutingDomain::LocalNetwork)
            {
                return Some("local network");
            }

            // Exclude any nodes that don't have a 'best node id' for our enabled cryptosystems
            if snap.best_node_id().is_none() {
                return Some("no best node id");
            }

            // Exclude nodes whose past protected connections died too fast.
            if let Some(drop_stats) = &snap.connection_stats.protected_drop_span {
                if drop_stats.tm90 < RELAY_MIN_PROTECTED_CONNECTION_TM90 {
                    return Some("protected connections die too fast");
                }
            }

            // Get the public internet peer info so we can validate it
            let Some(peer_info) = snap.get_peer_info(RoutingDomain::PublicInternet) else {
                return Some("no public internet peer info");
            };

            // Exclude any nodes that are relaying directly through us
            if own_peer_info
                .node_ids()
                .contains_any_from_iter(peer_info.node_info().relay_ids().iter())
            {
                return Some("relays through us");
            }

            // Disqualify nodes that don't have relay capability
            if !peer_info
                .node_info()
                .has_capability(VEILID_CAPABILITY_RELAY)
            {
                return Some("no relay capability");
            }

            // Disqualify any nodes that don't speak all of the envelope versions we do
            let peer_envelope_support = peer_info.node_info().envelope_support();
            if own_peer_info
                .node_info()
                .envelope_support()
                .iter()
                .copied()
                .any(|x| !peer_envelope_support.contains(&x))
            {
                return Some("envelope version mismatch");
            }

            // Ensure a transport we can reach this relay through is currently healthy.
            // Counters record why each candidate transport was rejected, so churn is attributable.
            let mut has_reachable_transport = false;
            let mut considered = 0usize;
            let mut no_evidence = 0usize;
            let mut unreachable = 0usize;
            let mut failing = 0usize;
            for did in peer_info.node_info().dial_info_detail_list() {
                if did.class.requires_signal() {
                    continue;
                }
                if !did.dial_info.matches_filter(&outbound_dif) {
                    continue;
                }
                considered += 1;
                let t: TransportType = (&did.dial_info).into();
                let opt_stats = snap.per_transport_stats.get(&t);
                match opt_stats {
                    None => no_evidence += 1,
                    Some(stats) if stats.last_seen_ts.is_none() => no_evidence += 1,
                    Some(stats) if stats.unreachable != 0 => unreachable += 1,
                    Some(stats) if transport_health(stats).is_failing() => failing += 1,
                    _ => {}
                }
                // Answered a dialed question here at some point and is not now Failing
                let healthy = match opt_stats {
                    // Absent evidence disqualifies a candidate but never an incumbent
                    None => !require_evidence,
                    Some(stats) => {
                        stats.unreachable == 0
                            && !transport_health(stats).is_failing()
                            && (!require_evidence || stats.last_seen_ts.is_some())
                    }
                };
                if healthy {
                    has_reachable_transport = true;
                    break;
                }
            }

            if !has_reachable_transport {
                return Some(if considered == 0 {
                    "no dial info matching our outbound capabilities"
                } else if require_evidence && no_evidence == considered {
                    "never dialed on any usable transport"
                } else if failing > 0 {
                    "usable transports are failing"
                } else if unreachable > 0 {
                    "usable transports unreachable"
                } else {
                    "no healthy reachable transport"
                });
            }

            // Exclude any nodes that have our same network block
            if own_peer_info
                .node_info()
                .is_on_same_ipblock(peer_info.node_info(), ip6_prefix_size)
            {
                return Some("same ip block as us");
            }

            None
        }
    }
}

impl RoutingDomainDetail for PublicInternetRoutingDomainDetail {
    fn as_any(&self) -> &dyn core::any::Any {
        self
    }
    fn as_any_mut(&mut self) -> &mut dyn core::any::Any {
        self
    }

    fn routing_domain(&self) -> RoutingDomain {
        RoutingDomain::PublicInternet
    }

    fn relay_requirements(&self) -> Arc<RelayRequirements> {
        {
            let opt_relay_requirements_guard = self.opt_last_relay_requirements.load();
            if let Some(relay_requirements) =
                opt_relay_requirements_guard.as_ref().map(|x| x.clone())
            {
                return relay_requirements;
            }
        }

        let relay_requirements = RelayRequirements::new(self);
        self.opt_last_relay_requirements
            .store(Some(relay_requirements.clone()));
        relay_requirements
    }

    fn network_config(&self) -> &RoutingDomainNetworkConfig {
        self.common.network_config()
    }

    fn outbound_protocols(&self) -> ProtocolTypeSet {
        self.common.outbound_protocols()
    }
    fn inbound_protocols(&self) -> ProtocolTypeSet {
        self.common.inbound_protocols()
    }
    fn address_types(&self) -> AddressTypeSet {
        self.common.address_types()
    }
    fn origin_routing_domains(&self) -> RoutingDomainSet {
        RoutingDomain::LocalNetwork | RoutingDomain::PublicInternet
    }
    fn confirmed(&self) -> bool {
        self.common.confirmed()
    }
    fn receive_mtus(&self) -> Option<TransportMtus> {
        self.common.receive_mtus()
    }
    fn capabilities(&self) -> BTreeSet<VeilidCapability> {
        self.common.capabilities()
    }

    fn relays(&self) -> Vec<RoutingDomainRelay> {
        self.common.relays()
    }
    fn relay_compilation(&self) -> Option<RelayCompilation> {
        self.common.relay_compilation()
    }
    fn relay_state(&self, relay_id: NodeId) -> Option<RoutingDomainRelayState> {
        self.common.relay_state(relay_id)
    }

    fn dial_info_details(&self) -> &Vec<DialInfoDetail> {
        self.common.dial_info_details()
    }

    fn translated_address_types(&self) -> AddressTypeSet {
        let mut inbound_address_types = HashMap::new();
        for did in self.dial_info_details() {
            inbound_address_types.insert(did.dial_info.ip_addr(), did.dial_info.address_type());
        }
        for intf_addr in &self.interface_addresses {
            inbound_address_types.remove(&intf_addr.ip());
        }
        inbound_address_types
            .into_values()
            .fold(AddressTypeSet::new(), |acc, at| acc | at)
    }

    fn inbound_dial_info_filter(&self) -> DialInfoFilter {
        self.common.inbound_dial_info_filter()
    }
    fn outbound_dial_info_filter(&self) -> DialInfoFilter {
        self.common.outbound_dial_info_filter()
    }

    fn get_peer_info(&self) -> Arc<PeerInfo> {
        self.common.get_current_peer_info()
    }

    fn get_bootstrap_peers(&self) -> Vec<NodeRef> {
        self.common.get_bootstrap_peers()
    }
    fn clear_bootstrap_peers(&self) {
        self.common.clear_bootstrap_peers();
    }
    fn add_bootstrap_peer(&self, bootstrap_peer: NodeRef) {
        self.common.add_bootstrap_peer(bootstrap_peer);
    }
    fn get_bootstrap_cache(&self) -> Option<BootstrapCache> {
        self.common.get_bootstrap_cache()
    }
    fn set_bootstrap_cache(&self, bootstrap_cache: Option<BootstrapCache>) {
        self.common.set_bootstrap_cache(bootstrap_cache);
    }
    fn record_bootstrap_cache(&self, bootstrap_cache: BootstrapCache) {
        self.common.record_bootstrap_cache(bootstrap_cache);
    }
    fn set_config_bootstrap_cache(&self, bootstrap_cache: BootstrapCache) {
        self.common.set_config_bootstrap_cache(bootstrap_cache);
    }
    fn get_served_bootstrap_cache(&self) -> BootstrapCache {
        self.common.get_served_bootstrap_cache()
    }

    fn update_low_water_mark(&self, low_water_mark: Arc<LowWaterMark>) {
        self.common.update_low_water_mark(low_water_mark);
    }

    fn reset_low_water_mark(&self) {
        self.common.reset_low_water_mark();
    }

    fn get_low_water_mark(&self) -> Arc<LowWaterMark> {
        self.common.get_low_water_mark()
    }

    fn get_entry_summary(&self) -> Arc<EntrySummary> {
        self.common.get_entry_summary()
    }
    fn set_entry_summary(&self, live_entry_counts: Arc<EntrySummary>) {
        self.common.set_entry_summary(live_entry_counts);
    }

    fn get_restore_point(&self) -> Option<Arc<RoutingDomainRestorePoint>> {
        self.common.get_restore_point()
    }
    fn replace_restore_point(
        &self,
        restore_point: Option<Arc<RoutingDomainRestorePoint>>,
    ) -> Option<Arc<RoutingDomainRestorePoint>> {
        self.common.replace_restore_point(restore_point)
    }
    fn restore_current_peer_info(&self, peer_info: Arc<PeerInfo>) -> bool {
        self.common.restore_current_peer_info(peer_info)
    }

    fn can_contain_address(&self, address: Address) -> bool {
        address.is_global()
    }

    fn ensure_dial_info_is_valid(&self, dial_info: &DialInfo) -> bool {
        let address = dial_info.socket_address().address();
        let can_contain_address = self.can_contain_address(address);

        if !can_contain_address {
            return false;
        }
        if !dial_info.is_valid() {
            veilid_log!(self debug
                "shouldn't be registering invalid addresses: {:?}",
                dial_info
            );
            return false;
        }
        true
    }

    ////////////////////////////////////////////////

    fn invalidate(&self) {
        self.opt_last_relay_requirements.store(None);
        self.common.clear_current_peer_info_cache();
    }

    fn debug(&self, alt: bool) -> String {
        if alt {
            format!("{:#}", self)
        } else {
            format!("{}", self)
        }
    }
}
