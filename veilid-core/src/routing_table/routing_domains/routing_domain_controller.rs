use super::*;

impl_veilid_log_facility!("rtab");

pub trait RoutingDomainController:
    component::VeilidComponentRegistryAccessor + core::any::Any + core::fmt::Debug + Send + Sync
{
    fn as_any(&self) -> &dyn core::any::Any;

    /// The routing domain identifier for this routing domain detail
    fn routing_domain(&self) -> RoutingDomain;

    // Routing Domain Detail Accessors
    ///////////////////////////////////////////////////////////////////////////////////////

    /// Read access to the routing domain detail
    fn read_dyn(&self) -> MappedRwLockReadGuard<'_, dyn RoutingDomainDetail>;
    /// Write access to the routing domain detail
    fn write_dyn(&self) -> MappedRwLockWriteGuard<'_, dyn RoutingDomainDetail>;
    /// Editor access to common fields in the routing domain detail
    fn edit_dyn(&self) -> Box<dyn RoutingDomainEditor + '_>;

    /// Start up the routing domain controller
    fn startup(&self) -> PinBoxFuture<'_, EyreResult<()>>;
    /// Shut down the routing domain controller
    fn shutdown(&self) -> PinBoxFuture<'_, ()>;
    /// Run ticker for this domain
    fn tick(&self) -> PinBoxFuture<'_, EyreResult<()>>;
    /// Cancel tasks for this domain
    fn cancel_tasks(&self) -> PinBoxFuture<'_, ()>;

    /// Load persisted state from the routing table TableDB
    fn load<'a>(&'a self, _db: &'a TableDB) -> PinBoxFuture<'a, EyreResult<()>> {
        Box::pin(async move { Ok(()) })
    }
    /// The state this domain persists, as its key and value in the routing table TableDB
    fn persisted_state(&self) -> Option<(Bytes, TableDBSerializedData)> {
        None
    }
    /// True when this domain's persisted state means the saved routing table must not be trusted
    /// Whether this node found itself among its own bootstraps
    fn is_own_bootstrap(&self) -> bool {
        false
    }

    fn persisted_state_requires_reset<'a>(&'a self, _db: &'a TableDB) -> PinBoxFuture<'a, bool> {
        Box::pin(async move { false })
    }

    // Routing Domain Controller Operations
    ///////////////////////////////////////////////////////////////////////////////////////

    /// The state of this routing domain
    fn state(&self) -> RoutingDomainState;
    /// The health status of this domain
    fn get_health(&self) -> RoutingDomainHealth;

    /// Publish current peer info to the world
    fn publish_peer_info(&self) -> bool;
    /// Unpublish peer info from the world
    fn unpublish_peer_info(&self);
    /// Get the published peer info for this routing domain
    fn get_published_peer_info(&self) -> Option<Arc<PeerInfo>>;

    /// Keep the peer info just published, and what it rests on, for a later attach to publish
    /// again. With no platform restore state nothing is kept: an older restore point
    /// would hold a peer info older than the one peers have.
    fn capture_restore_point(&self, peer_info: Arc<PeerInfo>) {
        let opt_platform = self
            .network_manager()
            .platform_restore_state(self.routing_domain());
        let changed = {
            let rdd = self.read_dyn();
            let restore_point = opt_platform.map(|platform| {
                Arc::new(RoutingDomainRestorePoint::new(platform, &*rdd, peer_info))
            });
            let keeps_one = restore_point.is_some();
            let had_one = rdd.replace_restore_point(restore_point).is_some();
            keeps_one || had_one
        };
        if changed {
            self.routing_table().mark_save_urgent();
        }
    }

    /// Forget the restore point: the network no longer vouches for the published state
    fn discard_restore_point(&self) {
        if self.read_dyn().replace_restore_point(None).is_some() {
            veilid_log!(self debug "[{}] discarded restore point", self.routing_domain());
            self.routing_table().mark_save_urgent();
        }
    }

    /// Put back a restore point the platform network found still valid: config, dial info,
    /// relays, and the signed peer info peers already hold if nothing in it changed.
    /// Returns whether that peer info is the current one.
    fn restore(&self, restore_point: &RoutingDomainRestorePoint) -> bool {
        let mut editor = self.edit_dyn();
        editor.set_network_config(restore_point.network_config.clone());
        editor.clear_dial_info_details(None, None);
        for did in &restore_point.dial_info_details {
            editor.add_dial_info_detail(did.clone());
        }
        editor.set_confirmed(true);
        editor.commit();

        // Relay requirements follow the restored dial info
        let relay_requirements = self.read_dyn().relay_requirements();
        let (opt_relay_compilation, relay_states) = compile_restored_relays(
            &self.routing_table(),
            self.routing_domain(),
            relay_requirements,
            &restore_point.relays,
        );
        let mut editor = self.edit_dyn();
        editor.set_relay_compilation(opt_relay_compilation);
        for (relay_id, state) in relay_states {
            editor.set_relay_state(relay_id, state);
        }
        editor.commit();

        self.read_dyn()
            .restore_current_peer_info(restore_point.peer_info.clone())
    }

    /// Get the contact methods that node A can use to reach node B in this routing domain
    /// Sorted by preference order, leveraging recent failure inforation, and the optional dial info sort in the request
    fn get_contact_methods(&self, request: ContactMethodRequest) -> Vec<ContactMethod>;

    /// Get the best/first contact method that node A can use to reach node B in this routing domain
    /// If no contact method exists, returns None
    fn get_best_contact_method(&self, request: ContactMethodRequest) -> Option<ContactMethod> {
        self.get_contact_methods(request).into_iter().next()
    }

    /// Gets the dial info details, in preference order, to attempt a connection between two nodes
    /// The best available ordering mode is selected from the sequencing constraint,
    /// then the dial info filter is applied, and self-dialing is filtered out.
    #[cfg_attr(feature = "instrument", instrument(level = "trace", target = "rtab", skip(self), fields(__VEILID_LOG_KEY = self.log_key()), ret))]
    fn get_dial_info_details_between_nodes(
        &self,
        from_node: &NodeInfo,
        to_node: &dyn HasDialInfoDetailList,
        dial_info_filter: DialInfoFilter,
        sequencing: Sequencing,
    ) -> Vec<DialInfoDetail> {
        // Consider outbound capabilities
        let (ordering, outbound_dial_info_filter) = DialInfoFilter::all()
            .with_address_type_set(from_node.address_types())
            .with_protocol_type_set(from_node.outbound_protocols())
            .apply_sequencing(sequencing);

        // If the filter is dead then we won't be able to connect
        if dial_info_filter.is_dead() {
            return vec![];
        }

        // Get the sort for this ordering mode
        let ordering_sort: Option<Box<DialInfoDetailSort>> =
            DialInfoDetail::get_ordering_sort(ordering);

        // Get node A's dial info socket addresses so we can check for self-dialing
        let node_a_socket_addresses = from_node
            .dial_info_detail_list()
            .iter()
            .map(|did| did.dial_info.socket_address())
            .collect::<BTreeSet<_>>();

        // Get all dial info we could possibly connect to for node B with the selected sequencing
        // and ensure we don't direct-dial one of our own addresses (would loop back)
        let filter = Box::new(|did: &DialInfoDetail| {
            did.matches_filter(&outbound_dial_info_filter)
                && !node_a_socket_addresses.contains(&did.dial_info.socket_address())
        });
        let mut all_reachable_dial_info =
            to_node.filtered_dial_info_details(ordering_sort.as_deref(), &filter);

        // Get the best available ordering mode for the reachable dial info
        let Some(best_sequence_ordering) = all_reachable_dial_info
            .iter()
            .map(|x| x.dial_info.protocol_type().sequence_ordering())
            .reduce(|a, b| a.max(b))
        else {
            return vec![];
        };

        // Retain only the dial info with the best sequence ordering and matching the dial info filter
        all_reachable_dial_info.retain(|x| {
            (matches!(sequencing, Sequencing::PreferUnordered)
                || x.dial_info.protocol_type().sequence_ordering() == best_sequence_ordering)
                && x.matches_filter(&dial_info_filter)
        });

        // Now return the first dial info detail in the list
        all_reachable_dial_info
    }
}

pub struct RoutingDomainControllerGuard<'a> {
    controller: Arc<dyn RoutingDomainController>,
    _phantom: core::marker::PhantomData<&'a ()>,
}

impl<'a> RoutingDomainControllerGuard<'a> {
    pub fn new(controller: Arc<dyn RoutingDomainController>) -> Self {
        Self {
            controller,
            _phantom: Default::default(),
        }
    }
}

impl<'a> core::ops::Deref for RoutingDomainControllerGuard<'a> {
    type Target = dyn RoutingDomainController;

    fn deref(&self) -> &Self::Target {
        self.controller.as_ref()
    }
}

////////////////////////////////////////////////////////////////////////////////////////////////////

pub trait SpecificRoutingDomainController: RoutingDomainController {
    const ROUTING_DOMAIN: RoutingDomain;
    type Detail: RoutingDomainDetail;
    type Editor<'a>: RoutingDomainEditor;

    fn read(&self) -> RwLockReadGuard<'_, Self::Detail>;
    fn write(&self) -> RwLockWriteGuard<'_, Self::Detail>;
    fn edit(&self) -> Self::Editor<'_>;
}

pub struct SpecificRoutingDomainControllerGuard<'a, C: SpecificRoutingDomainController> {
    controller: Arc<C>,
    _phantom: core::marker::PhantomData<&'a ()>,
}

impl<'a, C: SpecificRoutingDomainController> SpecificRoutingDomainControllerGuard<'a, C> {
    pub fn new(controller: Arc<C>) -> Self {
        Self {
            controller,
            _phantom: Default::default(),
        }
    }
}

impl<'a, C: SpecificRoutingDomainController> core::ops::Deref
    for SpecificRoutingDomainControllerGuard<'a, C>
{
    type Target = C;

    fn deref(&self) -> &Self::Target {
        &self.controller
    }
}
