mod bucket;
mod bucket_entry;
#[cfg_attr(not(feature = "debug-api"), expect(dead_code))]
mod debug;
mod find_nodes;
#[cfg(feature = "geolocation")]
mod geolocation;
mod get_nodes;
mod health;
mod network_estimator;
mod node_ref;
mod privacy;
mod route_spec_store;
mod routing_domains;
mod routing_table_inner;
mod routing_table_snapshot;
mod save_schedule;
#[cfg(feature = "stats")]
mod stats;
mod stats_accounting;
mod tasks;
mod types;

#[cfg(any(test, feature = "test-util"))]
#[doc(hidden)]
#[allow(
    clippy::disallowed_macros,
    clippy::disallowed_methods,
    reason = "test support: printing and raw clocks are the thing under test"
)]
pub mod tests_routing_table;

pub(crate) use bucket_entry::*;
pub(crate) use node_ref::*;
pub(crate) use privacy::*;
pub(crate) use route_spec_store::*;
pub(crate) use routing_domains::*;
pub(crate) use routing_table_inner::*;
pub(crate) use routing_table_snapshot::*;
pub(crate) use save_schedule::*;
pub(crate) use stats_accounting::*;
pub use types::*;

use super::*;

use crate::crypto::*;
use crate::network_manager::*;
use crate::rpc_processor::*;

use bucket::*;
use hashlink::LruCache;
use network_estimator::*;
use tasks::ping_validator::PingValidationBatch;
pub(crate) use tasks::ping_validator::PingValidationGroup;

impl_veilid_log_facility!("rtab");

//////////////////////////////////////////////////////////////////////////

/// Routing table bucket count (one per bit per 32 byte node id)
const BUCKET_COUNT: usize = HASH_COORDINATE_LENGTH * 8;

/// Contact-method viability recompute is event-driven (cm_viability_dirty); this
/// only rate-limits how often the drain can fire
const UPDATE_CM_VIABILITY_INTERVAL_SECS: u32 = 1;

/// Routing table durations derived from the RPC timeout
#[derive(Clone, Copy, Debug)]
pub(crate) struct RoutingTableTimings {
    /// Ping period until a node answers steadily, and base of the ping falloffs: one RPC timeout
    pub unreliable_ping_interval: TimestampDuration,
    /// TXT lookup time for a bootstrap name before direct bootstrap: two RPC timeouts
    pub bootstrap_txt_timeout: TimestampDuration,
    /// Zero responsive nodes this long falls back to bootstrap: one ping round and its answer
    pub no_responsive_bootstrap_delay: TimestampDuration,
    /// How long nodes must answer or lose questions steadily to become reliable or dead
    pub liveness: LivenessSpans,
    /// Loopback tests of one route come no closer than this: three RPC timeouts
    pub route_test_cooldown: TimestampDuration,
}

impl RoutingTableTimings {
    pub fn new(rpc_timeout: TimestampDuration) -> Self {
        Self {
            unreliable_ping_interval: rpc_timeout,
            bootstrap_txt_timeout: rpc_timeout.saturating_mul(2),
            no_responsive_bootstrap_delay: rpc_timeout.saturating_mul(2),
            liveness: LivenessSpans::new(rpc_timeout),
            route_test_cooldown: rpc_timeout.saturating_mul(3),
        }
    }
}

// Table store keys
const ROUTING_TABLE: &str = "routing_table";
/// Column 0 holds the table's own state, column 1 the persisted entries
pub(crate) const ROUTING_TABLE_COLUMN_COUNT: u32 = 2;
/// Column holding the persisted entries, one per key
const PERSISTED_ENTRIES_COLUMN: u32 = 1;
const CACHE_VALIDITY_KEY: Bytes = Bytes::from_static(b"cache_validity_key");
const NETWORK_ESTIMATOR_HISTORY: Bytes = Bytes::from_static(b"network_estimator_history");
/// Layout of the saved routing table
const ROUTING_TABLE_FORMAT_VERSION: u32 = 1;

/// What a saved routing table was built under; a table saved under anything else is emptied at load
#[derive(Serialize)]
struct RoutingTableCacheValidity<'a> {
    /// Layout of the saved table
    format_version: u32,
    /// This node's ids
    node_ids: &'a NodeIdGroup,
    /// Configured bootstrap hosts
    bootstrap: &'a [String],
    /// Keys trusted to sign bootstrap records
    bootstrap_keys: &'a [PublicKey],
    /// Configured bootstrap records
    bootstrap_records: &'a [String],
    /// Key separating this network from others
    network_key_password: Option<&'a str>,
}

pub type RoutingTableEntryFilter<'t> =
    Box<dyn FnMut(&Option<BucketEntrySnapshot>, Timestamp) -> bool + Send + 't>;
pub type RoutingTableEntryPreSortFilter<'t> =
    Box<dyn FnMut(&mut Vec<Option<BucketEntrySnapshot>>, Timestamp) + Send + 't>;
pub type RoutingTableEntrySort<'t> = Box<
    dyn FnMut(
            &Option<BucketEntrySnapshot>,
            &Option<BucketEntrySnapshot>,
            Timestamp,
        ) -> core::cmp::Ordering
        + Send
        + 't,
>;
pub type CreateNodeRefUpdateFunc<'t> = Box<dyn FnOnce(&mut BucketEntryInner) + 't>;

type RoutingDomainControllerMap = HashMap<RoutingDomain, Arc<dyn RoutingDomainController>>;

pub type BucketIndex = (CryptoKind, usize);

/// Old/new peer info pairs produced by a node id change, for subscriber notification.
type PeerInfoUpdatePairs = Vec<(Arc<PeerInfo>, Arc<PeerInfo>)>;

#[derive(Debug, Clone, Copy)]
#[must_use]
pub struct RecentPeersEntry {
    pub last_connection: Flow,
}

#[derive(Debug, Clone)]
pub struct RoutingTableStartupContext {
    pub startup_lock: Arc<StartupLock>,
}
impl RoutingTableStartupContext {
    pub fn new() -> Self {
        Self {
            startup_lock: Arc::new(StartupLock::new()),
        }
    }
}
impl Default for RoutingTableStartupContext {
    fn default() -> Self {
        Self::new()
    }
}

/// Peer infos received more than once are the norm; this many verifications are remembered
const VERIFIED_PEER_INFO_CACHE_SIZE: usize = 1024;

/// The network manager, held by the routing table while it runs so per-entry state
/// computation reaches the online detector without a registry lookup. Stored once at
/// post-init and cleared at pre-terminate; loaded on every entry state read.
pub(crate) type NetworkManagerHandle = Arc<ArcSwapOption<VeilidComponentGuardArc<NetworkManager>>>;

#[must_use]
pub(crate) struct RoutingTable {
    /// Component registry this table belongs to
    registry: VeilidComponentRegistry,
    /// The network manager, held from post-init until termination
    network_manager: NetworkManagerHandle,

    /// Startup context
    startup_context: RoutingTableStartupContext,

    /// Routing table inner state
    inner: RwLock<RoutingTableInner>,
    /// Routing domains, replaced whole at init and terminate so readers take no lock
    routing_domains: ArcSwap<RoutingDomainControllerMap>,

    /// Ping validation channel sender (recreated on each startup/shutdown cycle)
    ping_validation_sender: Mutex<flume::Sender<PingValidationBatch>>,
    /// Ping validation channel receiver (taken during startup, recreated during shutdown)
    ping_validation_receiver: Mutex<Option<flume::Receiver<PingValidationBatch>>>,
    /// Ping validation processor join handle
    ping_validation_processor_jh: Mutex<Option<MustJoinHandle<()>>>,
    /// Ping validation processor stop source
    ping_validation_stop_source: Mutex<Option<StopSource>>,

    /// Node Ids
    node_ids: RwLock<NodeIdGroup>,
    /// Public Keys
    public_keys: RwLock<PublicKeyGroup>,
    /// Secret Keys
    secret_keys: RwLock<SecretKeyGroup>,
    /// Route spec store
    route_spec_store: RouteSpecStore,
    /// Durations derived from the RPC timeout
    timings: RoutingTableTimings,
    /// Buckets to kick on our next kick task
    kick_queue: Mutex<BTreeSet<BucketIndex>>,
    /// Set when peerinfo changed on either side; the cm viability task fires only when set
    cm_viability_dirty: AtomicBool,
    /// Last route-test enqueue time per route, so a lossy network cannot retest it continuously
    route_test_cooldown: Mutex<BTreeMap<RouteId, Timestamp>>,
    /// Routes whose test failures waited out blame suppression, retested past the cooldown after
    deferred_route_tests: Mutex<BTreeSet<RouteId>>,
    /// Ping outcome and queue accounting surfaced through stats
    #[cfg(feature = "stats")]
    ping_stats: Mutex<tasks::ping_stats_accounting::PingStatsAccounting>,
    /// Interim accounting mechanism for this node's RPC latency to any other node
    self_latency_stats_accounting: Mutex<LatencyStatsAccounting>,
    /// Interim accounting mechanism for the total bandwidth to/from this node
    self_transfer_stats_accounting: Mutex<(TransferStatsAccounting, TransferStatsDownUp)>,
    /// Peers we have recently communicated with
    recent_peers: Mutex<LruCache<NodeId, RecentPeersEntry>>,
    /// Byte-identical peer infos whose signatures already checked out
    verified_peer_infos: Mutex<LruCache<(Bytes, SignatureGroup), ()>>,
    /// Last routing table health summary
    routing_table_health: Mutex<Arc<RoutingTableHealth>>,
    /// Network size estimator (24h rolling history)
    network_estimator: Mutex<NetworkEstimator>,
    /// When the table next goes to disk
    save_schedule: Mutex<SaveSchedule>,
    /// Serializes saves so an older snapshot never lands after a newer one
    save_lock: AsyncMutex<()>,
    /// The entries the last committed save left in the table store
    persisted_entries: Mutex<PersistedEntries>,
    /// The network estimator history as the table store holds it
    saved_network_estimator: Mutex<Option<NetworkEstimator>>,
    /// Each routing domain's states as the table store holds them, by key
    saved_routing_domain_states: Mutex<HashMap<Bytes, Bytes>>,
    /// The routing table's table, opened on the first save and held open after
    table_db: Mutex<Option<TableDB>>,

    /// Background process for saving the table to disk when its schedule says
    flush_task: TickTask<EyreReport>,
    /// Background process for computing statistics
    rolling_transfers_task: TickTask<EyreReport>,
    /// Background process for computing statistics
    update_state_stats_task: TickTask<EyreReport>,
    /// Background process for computing statistics
    rolling_answers_task: TickTask<EyreReport>,
    /// Background process to purge dead routing table entries when necessary
    kick_buckets_task: TickTask<EyreReport>,
    /// Background process to recompute per-entry contact-method viability
    update_cm_viability_task: TickTask<EyreReport>,

    /// Tick subscription
    tick_subscription: Mutex<Option<EventBusSubscription>>,
    /// App lifecycle subscription
    #[cfg(feature = "app-lifecycle")]
    app_lifecycle_subscription: Mutex<Option<EventBusSubscription>>,
    /// Background time held until the next save lands
    #[cfg(feature = "app-lifecycle")]
    background_save_time: Mutex<Option<Arc<BackgroundTimeGuard>>>,
}

impl fmt::Debug for RoutingTable {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("RoutingTable")
            // .field("inner", &self.inner)
            // .field("unlocked_inner", &self.unlocked_inner)
            .finish()
    }
}

impl_veilid_component!(RoutingTable, stats);

impl RoutingTable {
    pub fn new(
        registry: VeilidComponentRegistry,
        startup_context: RoutingTableStartupContext,
    ) -> Self {
        let network_manager: NetworkManagerHandle = Arc::new(ArcSwapOption::empty());
        let inner = RwLock::new(RoutingTableInner::new(
            registry.clone(),
            network_manager.clone(),
        ));
        let route_spec_store = RouteSpecStore::new(registry.clone());
        let timings = RoutingTableTimings::new(registry.config().rpc_timeout());
        let save_timings =
            SaveTimings::from(&registry.config().internal().network.routing_table.save);

        // Create ping validation channel
        let (ping_validation_sender, ping_validation_receiver) = flume::unbounded();

        let this = Self {
            registry,
            network_manager,
            inner,
            startup_context,
            route_spec_store,
            timings,

            routing_domains: Default::default(),
            node_ids: Default::default(),
            public_keys: Default::default(),
            secret_keys: Default::default(),
            kick_queue: Default::default(),
            cm_viability_dirty: AtomicBool::new(true),
            route_test_cooldown: Default::default(),
            deferred_route_tests: Default::default(),
            #[cfg(feature = "stats")]
            ping_stats: Default::default(),
            ping_validation_sender: Mutex::new(ping_validation_sender),
            ping_validation_receiver: Mutex::new(Some(ping_validation_receiver)),
            ping_validation_processor_jh: Mutex::new(None),
            ping_validation_stop_source: Mutex::new(None),
            self_latency_stats_accounting: Default::default(),
            self_transfer_stats_accounting: Default::default(),
            recent_peers: Mutex::new(LruCache::new(RECENT_PEERS_TABLE_SIZE)),
            verified_peer_infos: Mutex::new(LruCache::new(VERIFIED_PEER_INFO_CACHE_SIZE)),
            routing_table_health: Mutex::new(Arc::new(RoutingTableHealth::default())),
            network_estimator: Mutex::new(NetworkEstimator::new()),
            save_schedule: Mutex::new(SaveSchedule::new(Timestamp::now(), save_timings)),
            save_lock: AsyncMutex::new(()),
            persisted_entries: Default::default(),
            saved_network_estimator: Default::default(),
            saved_routing_domain_states: Default::default(),
            table_db: Mutex::new(None),

            flush_task: TickTask::new("flush_task", 1),
            rolling_transfers_task: TickTask::new(
                "rolling_transfers_task",
                ROLLING_TRANSFERS_INTERVAL_SECS,
            ),
            update_state_stats_task: TickTask::new(
                "update_state_stats_task",
                UPDATE_STATE_STATS_INTERVAL_SECS,
            ),
            rolling_answers_task: TickTask::new(
                "rolling_answers_task",
                ROLLING_ANSWER_INTERVAL_SECS,
            ),
            kick_buckets_task: TickTask::new("kick_buckets_task", 1),
            update_cm_viability_task: TickTask::new(
                "update_cm_viability_task",
                UPDATE_CM_VIABILITY_INTERVAL_SECS,
            ),

            tick_subscription: Default::default(),
            #[cfg(feature = "app-lifecycle")]
            app_lifecycle_subscription: Default::default(),
            #[cfg(feature = "app-lifecycle")]
            background_save_time: Default::default(),
        };

        this.setup_tasks();

        this
    }

    /////////////////////////////////////
    // Initialization

    fn log_facilities_impl(&self) -> VeilidComponentLogFacilities {
        let mut facilities = VeilidComponentLogFacilities::new();

        facilities = facilities.with_facility(
            VeilidComponentLogFacility::try_new_with_tags("rtab", ["#common"]).unwrap(),
        );

        facilities = facilities.with_facility(
            VeilidComponentLogFacility::try_new_with_tags("rtab::route", ["#common"]).unwrap(),
        );

        facilities = facilities.with_facility(
            VeilidComponentLogFacility::try_new_with_tags("rtab::state", ["#verbose", "#state"])
                .unwrap(),
        );

        facilities = facilities.with_facility(
            VeilidComponentLogFacility::try_new_with_tags(
                "rtab::state::node",
                ["#verbose", "#state"],
            )
            .unwrap(),
        );

        facilities = facilities.with_facility(
            VeilidComponentLogFacility::try_new_with_tags("rtab::health", ["#verbose"]).unwrap(),
        );

        facilities = facilities.with_facility(
            VeilidComponentLogFacility::try_new_with_tags(
                "rtab::state::ping",
                ["#verbose", "#state"],
            )
            .unwrap(),
        );

        facilities = facilities.with_facility(
            VeilidComponentLogFacility::try_new_with_tags("network_result", ["#verbose"]).unwrap(),
        );

        #[cfg(feature = "geolocation")]
        {
            facilities = facilities.with_facility(
                VeilidComponentLogFacility::try_new_with_tags("geolocation", ["#verbose"]).unwrap(),
            );
        }

        facilities
    }

    /// Called to initialize the routing table after it is created
    async fn init_async(&self) -> EyreResult<()> {
        veilid_log!(self debug "starting routing table init");

        // Set up routing domains (eventually make this pluggable or use features)
        {
            let mut routing_domains =
                RoutingDomainControllerMap::clone(&self.routing_domains.load());
            routing_domains.insert(
                RoutingDomain::PublicInternet,
                Arc::new(PublicInternetRoutingDomainController::new(self.registry())),
            );
            routing_domains.insert(
                RoutingDomain::LocalNetwork,
                Arc::new(LocalNetworkRoutingDomainController::new(self.registry())),
            );
            self.routing_domains.store(Arc::new(routing_domains));
        }

        // Set up initial keys and node ids
        self.setup_public_keys().await?;

        // Set up routing buckets
        {
            let mut inner = self.inner.write();
            inner.init_buckets();
        }

        // Clear out routing table health
        *self.routing_table_health.lock() = Arc::new(RoutingTableHealth::default());

        // Load persisted routing table state from table db if possible
        veilid_log!(self debug "loading routing table state");
        if let Err(e) = self.load().await {
            veilid_log!(self debug "Error loading routing table state from storage: {:#?}. Resetting.", e);
            let mut inner = self.inner.write();
            inner.init_buckets();
        }

        // Set up routespecstore
        veilid_log!(self debug "starting route spec store init");
        if let Err(e) = self.route_spec_store().load().await {
            veilid_log!(self debug "Error loading route spec store: {:#?}. Resetting.", e);
            self.route_spec_store().reset();
        };
        veilid_log!(self debug "finished route spec store init");

        veilid_log!(self debug "finished routing table init");
        Ok(())
    }

    #[expect(clippy::unused_async)]
    async fn post_init_async(&self) -> EyreResult<()> {
        self.network_manager
            .store(self.registry().lookup_arc::<NetworkManager>().map(Arc::new));

        // Held while detached too, so the schedule knows where the app stands on reattach
        #[cfg(feature = "app-lifecycle")]
        {
            let app_lifecycle_subscription =
                impl_subscribe_event_bus!(self, Self, app_lifecycle_event_handler);
            *self.app_lifecycle_subscription.lock() = Some(app_lifecycle_subscription);
        }
        Ok(())
    }

    pub(crate) async fn startup(&self) -> EyreResult<()> {
        let guard = self.startup_context.startup_lock.startup()?;

        // Start all routing domain controllers
        veilid_log!(self debug "starting routing domain controllers");
        let mut started_rdcs: Vec<RoutingDomainControllerGuard<'_>> = Vec::new();
        for rdc in self.get_routing_domain_controllers(RoutingDomainSet::all()) {
            veilid_log!(self debug "  starting routing domain controller: {}", rdc.routing_domain());
            if let Err(e) = rdc.startup().await {
                veilid_log!(self debug "error starting routing domain controller: {}", e);
                for started_rdc in started_rdcs {
                    veilid_log!(self debug "  shutting down routing domain controller: {}", started_rdc.routing_domain());
                    started_rdc.shutdown().await;
                }

                return Err(e);
            }
            started_rdcs.push(rdc);
        }
        veilid_log!(self debug "finished starting routing domain controllers");

        // Don't reset low water marks: preserve persisted state via min_assign merge
        self.refresh_summaries(RoutingDomainSet::empty());

        // Start ping validation processor
        veilid_log!(self debug "starting ping validation processor");
        let rx = self
            .ping_validation_receiver
            .lock()
            .take()
            .expect("ping validation receiver already taken");
        let stop_source = StopSource::new();
        let stop_token = stop_source.token();
        let jh = spawn(
            "ping validation processor",
            Self::ping_validation_processor(self.registry(), stop_token, rx),
        );
        *self.ping_validation_processor_jh.lock() = Some(jh);
        *self.ping_validation_stop_source.lock() = Some(stop_source);

        // Register event handlers
        let tick_subscription = impl_subscribe_event_bus_async!(self, Self, tick_event_handler);
        *self.tick_subscription.lock() = Some(tick_subscription);

        guard.success();
        Ok(())
    }

    pub(crate) async fn shutdown(&self) {
        // Stop tasks
        veilid_log!(self debug "stopping routing table tasks");

        let guard = self
            .startup_context
            .startup_lock
            .shutdown()
            .await
            .expect_or_log("should be started up");

        if let Some(sub) = self.tick_subscription.lock().take() {
            self.event_bus().unsubscribe(sub);
        }

        veilid_log!(self debug "cancelling routing table tasks");
        self.cancel_tasks().await;

        // Before the routing domains unpublish; terminate saves only what changes after
        self.flush(SaveReason::Detach).await;

        // Stop ping validation processor
        veilid_log!(self debug "stopping ping validation processor");
        drop(self.ping_validation_stop_source.lock().take());
        let opt_jh = self.ping_validation_processor_jh.lock().take();
        if let Some(jh) = opt_jh {
            jh.await;
        }

        // Recreate ping validation channel for potential re-attach
        let (tx, rx) = flume::unbounded();
        *self.ping_validation_sender.lock() = tx;
        *self.ping_validation_receiver.lock() = Some(rx);

        // Shutdown all routing domain controllers
        veilid_log!(self debug "shutting down routing domain controllers");
        for rdc in self.get_routing_domain_controllers(RoutingDomainSet::all()) {
            veilid_log!(self debug "  shutting down routing domain controller: {}", rdc.routing_domain());
            rdc.shutdown().await;
        }
        veilid_log!(self debug "finished shutting down routing domain controllers");

        // Update routing table health now that routing domains are shut down
        //   - Don't change the low water marks at all
        self.refresh_summaries(RoutingDomainSet::empty());

        guard.success();
    }

    #[expect(clippy::unused_async)]
    async fn pre_terminate_async(&self) {
        // Ensure things have shut down
        debug_assert!(
            self.startup_context.startup_lock.is_shut_down(),
            "should have shut down by now"
        );
        // Released here or the network manager outlives its own termination
        self.network_manager.store(None);

        #[cfg(feature = "app-lifecycle")]
        if let Some(sub) = self.app_lifecycle_subscription.lock().take() {
            self.event_bus().unsubscribe(sub);
        }
    }

    /// Called to shut down the routing table
    async fn terminate_async(&self) {
        veilid_log!(self debug "starting routing table terminate");

        // A detach saved everything before it
        if self.save_schedule.lock().is_dirty() {
            veilid_log!(self debug "routing table termination flush");
            self.flush(SaveReason::Terminate).await;
        }
        self.table_db.lock().take();
        self.route_spec_store().close_table();

        veilid_log!(self debug "shutting down routing table");

        {
            let mut inner = self.inner.write();
            *inner = RoutingTableInner::new(self.registry(), self.network_manager.clone());
        }
        *self.routing_table_health.lock() = Arc::new(RoutingTableHealth::default());

        self.node_ids.write().clear();

        self.routing_domains.store(Default::default());

        veilid_log!(self debug "finished routing table terminate");
    }

    /// Save the routing table and route spec store
    pub async fn flush(&self, reason: SaveReason) {
        let _save_guard = self.save_lock.lock().await;
        let start_ts = Timestamp::now();
        // Held until this save lands; a change during it claims time for the next one
        #[cfg(feature = "app-lifecycle")]
        let _background_time = {
            let mut schedule = self.save_schedule.lock();
            schedule.saving(start_ts);
            self.background_save_time.lock().take()
        };
        #[cfg(not(feature = "app-lifecycle"))]
        self.save_schedule.lock().saving(start_ts);

        let saved = self
            .save()
            .await
            .inspect_err(|e| error!("failed to save routing table state: {}", e))
            .ok();

        if let Err(e) = self.route_spec_store().save().await {
            error!("couldn't save route spec store: {}", e);
        }

        if let Some(saved) = saved {
            veilid_log!(self debug "saved routing table ({:?}): {} entries, {} written, {} deleted, {} routing domain states written in {:#}",
                reason, saved.entries, saved.written, saved.deleted, saved.routing_domain_states, TimestampDuration::since(start_ts));
        }
    }

    #[cfg(feature = "app-lifecycle")]
    fn app_lifecycle_event_handler(&self, evt: Arc<AppLifecycleEvent>) {
        // A hidden desktop app keeps running, so its saves keep the foreground schedule
        self.save_schedule.lock().set_background(
            evt.state == VeilidAppLifecycleState::Background && background_may_freeze(),
            Timestamp::now(),
        );
        if let Some(background_time) = evt.background_time.clone() {
            *self.background_save_time.lock() = Some(background_time);
        }
        self.hold_background_time_for_save();
    }

    /// In the background the platform may suspend the process before a save it owes; claim
    /// time until that save lands
    #[cfg(feature = "app-lifecycle")]
    fn hold_background_time_for_save(&self) {
        // Under the schedule lock, as in flush, so no save starts between the check and the claim
        let schedule = self.save_schedule.lock();
        if !(schedule.is_background() && schedule.is_dirty()) {
            return;
        }
        let mut held = self.background_save_time.lock();
        if held.is_none() {
            *held = BackgroundTimeGuard::begin("veilid routing table save").map(Arc::new);
        }
    }

    ///////////////////////////////////////////////////////////////////

    /// Create a point-in-time snapshot of all routing table entries matching the minimum state
    pub fn snapshot_entries(
        &self,
        cur_ts: Timestamp,
        min_state: BucketEntryState,
    ) -> RoutingTableSnapshot {
        // State computation stays outside the lock: a waiting writer blocks every reader
        let entry_refs = self.inner.read().all_entry_refs();
        let registry = self.registry();
        let entries = entry_refs
            .into_iter()
            .filter_map(|entry| entry.snapshot_if_at_least(registry.clone(), cur_ts, min_state))
            .collect();
        RoutingTableSnapshot::new(registry, cur_ts, entries)
    }

    /// Max entries kept in the bucket at position `pos`.
    #[cfg(any(test, feature = "test-util"))]
    pub fn bucket_depth(pos: usize) -> usize {
        RoutingTableInner::bucket_depth(pos)
    }

    /// Expected routing-table fill size for a network of `n` reachable nodes.
    #[cfg(any(test, feature = "test-util"))]
    pub fn practical_max_size(n: NodeCount) -> usize {
        let n = n.as_u64();
        let mut total: usize = 0;
        for k in 0..BUCKET_COUNT {
            let depth = RoutingTableInner::bucket_depth(k) as u64;
            let expected = if k + 1 >= 64 { 0u64 } else { n >> (k + 1) };
            total = total.saturating_add(expected.min(depth) as usize);
        }
        total
    }

    /// Count of entries beyond per-bucket capacity, awaiting lazy kick.
    pub fn excess_kickable_count(&self) -> usize {
        self.inner.read().excess_kickable_count()
    }

    /// Per-bucket entry counts for a crypto kind, indexed by position.
    pub fn bucket_counts(&self, ck: CryptoKind) -> Vec<usize> {
        self.inner.read().bucket_counts(ck)
    }

    /// Estimated number of reachable nodes in the network for a single crypto kind.
    #[expect(dead_code)]
    pub fn estimate_network_size(&self, ck: CryptoKind) -> u64 {
        self.network_estimator.lock().estimate(ck)
    }

    /// Estimated number of reachable nodes in the network across all crypto kinds.
    pub fn estimate_network_size_combined(&self) -> u64 {
        self.network_estimator.lock().estimate_combined()
    }

    pub fn node_id(&self, kind: CryptoKind) -> NodeId {
        self.node_ids.read().get(kind).unwrap_or_log()
    }

    pub fn public_key(&self, kind: CryptoKind) -> PublicKey {
        self.public_keys.read().get(kind).unwrap_or_log()
    }

    pub fn secret_key(&self, kind: CryptoKind) -> SecretKey {
        self.secret_keys.read().get(kind).unwrap_or_log()
    }

    pub fn node_ids(&self) -> NodeIdGroup {
        self.node_ids.read().clone()
    }

    pub fn public_keys(&self) -> PublicKeyGroup {
        self.public_keys.read().clone()
    }

    pub fn signing_key_pairs(&self) -> KeyPairGroup {
        let mut tkps = KeyPairGroup::new();
        for ck in VALID_CRYPTO_KINDS {
            tkps.add(KeyPair::new(
                ck,
                BareKeyPair::new(self.public_key(ck).value(), self.secret_key(ck).value()),
            ));
        }
        tkps
    }

    pub fn matches_own_node_id(&self, node_ids: &[NodeId]) -> bool {
        for ni in node_ids {
            if let Some(v) = self.node_ids().get(ni.kind()) {
                if v.ref_value() == ni.ref_value() {
                    return true;
                }
            }
        }
        false
    }

    pub fn matches_own_public_key(&self, public_keys: &[PublicKey]) -> bool {
        for pk in public_keys {
            if let Some(v) = self.public_keys().get(pk.kind()) {
                if v.ref_value() == pk.ref_value() {
                    return true;
                }
            }
        }
        false
    }

    /// Whether a dial info reaches this node: one of our own, or loopback on one of their ports
    pub fn matches_own_dial_info(&self, dial_info: &DialInfo) -> bool {
        let target = dial_info.socket_address();
        RoutingDomainSet::all().iter().any(|rd| {
            self.dial_info_details(rd).iter().any(|did| {
                let own = did.dial_info.socket_address();
                own == target || (target.address().is_loopback() && own.port() == target.port())
            })
        })
    }

    #[cfg(not(test))]
    async fn setup_public_key(
        &self,
        vcrypto: AsyncCryptoSystemGuard<'_>,
    ) -> VeilidAPIResult<(PublicKey, SecretKey)> {
        let config = self.config();
        let table_store = self.table_store();
        let ck = vcrypto.kind();
        let mut public_key = config.network.routing_table.public_keys.get(ck);
        let mut secret_key = config.network.routing_table.secret_keys.get(ck);

        let config_table = table_store.open("__veilid_config", 1).await?;

        // Old pre-0.5.0 locations
        let table_key_node_id = format!("node_id_{}", ck);
        let table_key_node_id_secret = format!("node_id_secret_{}", ck);
        // Post-0.5.0 locations
        let table_key_public_key = format!("public_key_{}", ck);
        let table_key_secret_key = format!("secret_key_{}", ck);

        // See if public key was previously stored in the table store
        if public_key.is_none() {
            veilid_log!(self debug "pulling {} from storage", table_key_public_key);
            if let Ok(Some(stored_public_key)) = config_table
                .load_serialized(0, Bytes::from(table_key_public_key.clone()))
                .await
            {
                let stored_public_key: PublicKey =
                    table_store.try_deserialize(&stored_public_key)?;
                veilid_log!(self debug "{} found in storage", table_key_public_key);
                public_key = Some(stored_public_key);
            } else {
                veilid_log!(self debug "{} not found in storage", table_key_public_key);
            }
        }
        if public_key.is_none() {
            veilid_log!(self debug "pulling {} from deprecated storage", table_key_node_id);
            if let Ok(Some(stored_public_key)) = config_table
                .load_serialized(0, Bytes::from(table_key_node_id.clone()))
                .await
            {
                let stored_public_key: PublicKey =
                    table_store.try_deserialize(&stored_public_key)?;
                veilid_log!(self debug "{} found in deprecated storage", table_key_node_id);
                public_key = Some(stored_public_key);
            } else {
                veilid_log!(self debug "{} not found in deprecated storage", table_key_node_id);
            }
        }

        // See if secret key was previously stored in the table store
        if secret_key.is_none() {
            veilid_log!(self debug "pulling {} from storage", table_key_secret_key);
            if let Ok(Some(stored_secret_key)) = config_table
                .load_serialized(0, Bytes::from(table_key_secret_key.clone()))
                .await
            {
                let stored_secret_key: SecretKey =
                    table_store.try_deserialize(&stored_secret_key)?;
                veilid_log!(self debug "{} found in storage", table_key_secret_key);
                secret_key = Some(stored_secret_key);
            } else {
                veilid_log!(self debug "{} not found in storage", table_key_secret_key);
            }
        }
        if secret_key.is_none() {
            veilid_log!(self debug "pulling {} from deprecated storage", table_key_node_id_secret);
            if let Ok(Some(stored_secret_key)) = config_table
                .load_serialized(0, Bytes::from(table_key_node_id_secret.clone()))
                .await
            {
                let stored_secret_key: SecretKey =
                    table_store.try_deserialize(&stored_secret_key)?;
                veilid_log!(self debug "{} found in deprecated storage", table_key_node_id_secret);
                secret_key = Some(stored_secret_key);
            } else {
                veilid_log!(self debug "{} not found in deprecated storage", table_key_node_id_secret);
            }
        }

        // If we have a public key from storage, check it
        let (public_key, secret_key) =
            if let (Some(public_key), Some(secret_key)) = (public_key, secret_key) {
                // Validate node id
                if !vcrypto.validate_keypair(&public_key, &secret_key).await? {
                    apibail_generic!(
                        "secret_key and public_key don't match:\npublic_key: {}\nsecret_key: {}",
                        public_key,
                        secret_key
                    );
                }
                (public_key, secret_key)
            } else {
                // If we still don't have a valid keypair, generate one
                veilid_log!(self debug "generating new node {} keypair", ck);
                vcrypto.generate_keypair().await.into_split()
            };

        // Save the public key + secret in storage
        let public_key_serialized = serialize_table_value(&public_key);
        config_table
            .store_serialized(
                0,
                Bytes::from(table_key_public_key.clone()),
                public_key_serialized,
            )
            .await?;
        let secret_key_serialized = serialize_table_value(&secret_key);
        config_table
            .store_serialized(
                0,
                Bytes::from(table_key_secret_key.clone()),
                secret_key_serialized,
            )
            .await?;

        Ok((public_key, secret_key))
    }

    /// Get the public keys from config if one is specified
    #[cfg_attr(test, allow(unused_variables))]
    async fn setup_public_keys(&self) -> VeilidAPIResult<()> {
        let crypto = self.crypto();

        let mut out_public_keys = PublicKeyGroup::new();
        let mut out_secret_keys = SecretKeyGroup::new();

        for ck in VALID_CRYPTO_KINDS {
            let vcrypto = crypto
                .get_async(ck)
                .expect_or_log("Valid crypto kind is not actually valid.");

            // Configured keys let a reopened test core keep its identity
            #[cfg(test)]
            let (public_key, secret_key) = {
                let config = self.config();
                match (
                    config.network.routing_table.public_keys.get(ck),
                    config.network.routing_table.secret_keys.get(ck),
                ) {
                    (Some(public_key), Some(secret_key)) => (public_key, secret_key),
                    _ => vcrypto.generate_keypair().await.into_split(),
                }
            };
            #[cfg(not(test))]
            let (public_key, secret_key) = self.setup_public_key(vcrypto).await?;

            // Save for config
            out_public_keys.add(public_key);
            out_secret_keys.add(secret_key);
        }

        veilid_log!(self info  "Public Keys: {}", out_public_keys);

        *self.public_keys.write() = out_public_keys;
        *self.secret_keys.write() = out_secret_keys;

        // Set up node ids
        let mut node_ids = NodeIdGroup::new();
        for pk in self.public_keys().iter() {
            let node_id = self.generate_node_id(pk)?;
            node_ids.add(node_id);
        }

        veilid_log!(self info "Node Ids: {}", node_ids);

        *self.node_ids.write() = node_ids;

        Ok(())
    }

    // Convenience validators
    pub fn check_route_id(&self, route_id: &RouteId) -> VeilidAPIResult<()> {
        let crypto = self.crypto();
        let Some(vcrypto) = crypto.get(route_id.kind()) else {
            apibail_invalid_argument!("unsupported crypto kind", "route_id", route_id);
        };
        if route_id.ref_value().len() != vcrypto.hash_digest_length() {
            apibail_invalid_argument!("invalid route id length", "route_id", route_id);
        }
        Ok(())
    }
    pub fn check_node_id(&self, node_id: &NodeId) -> VeilidAPIResult<()> {
        let crypto = self.crypto();
        let Some(_) = crypto.get(node_id.kind()) else {
            apibail_invalid_argument!("unsupported crypto kind", "node_id", node_id);
        };
        if node_id.ref_value().len() != HASH_COORDINATE_LENGTH {
            apibail_invalid_argument!("invalid node id length", "node_id", node_id);
        }
        Ok(())
    }

    /// Produce node id from public key
    pub fn generate_node_id(&self, public_key: &PublicKey) -> VeilidAPIResult<NodeId> {
        if public_key.ref_value().len() == HASH_COORDINATE_LENGTH {
            return Ok(NodeId::new(
                public_key.kind(),
                BareNodeId::new(public_key.ref_value()),
            ));
        }
        let crypto = self.crypto();
        let Some(vcrypto) = crypto.get(public_key.kind()) else {
            apibail_invalid_argument!("unsupported cryptosystem", "public_key", public_key);
        };

        let idhash = vcrypto.generate_hash(public_key.ref_value());
        if idhash.ref_value().len() < HASH_COORDINATE_LENGTH {
            apibail_internal!(format!(
                "generate_hash needs to produce at least {HASH_COORDINATE_LENGTH} bytes"
            ));
        }
        Ok(NodeId::new(
            public_key.kind(),
            BareNodeId::new(&idhash.ref_value()[0..HASH_COORDINATE_LENGTH]),
        ))
    }

    pub fn calculate_bucket_index(&self, node_id: &NodeId) -> EyreResult<BucketIndex> {
        if node_id.ref_value().len() * 8 != BUCKET_COUNT {
            bail!("NodeId should be hashed down to BUCKET_COUNT bits");
        }
        let self_hash_coordinate = self.node_id(node_id.kind()).to_hash_coordinate();
        Ok((
            node_id.kind(),
            node_id
                .to_hash_coordinate()
                .distance(&self_hash_coordinate)
                .first_nonzero_bit()
                .unwrap_or_log(),
        ))
    }

    /// Every entry in a bucket, once each
    fn bucketed_entries(&self) -> Vec<Arc<BucketEntry>> {
        let inner = self.inner.read();
        let mut seen: HashSet<*const BucketEntry> = HashSet::new();
        let mut entries = Vec::new();
        for buckets in inner.buckets.values() {
            for bucket in buckets {
                for (_, entry) in bucket.entries() {
                    if seen.insert(Arc::as_ptr(entry)) {
                        entries.push(entry.clone());
                    }
                }
            }
        }
        entries
    }

    /// Write what changed since the last save: bucket entries, network estimator history and
    /// per-routing-domain state
    async fn save(&self) -> EyreResult<SaveCounts> {
        let entries = self.bucketed_entries();
        let changes = self
            .persisted_entries
            .lock()
            .changes(&entries, Timestamp::now());
        let network_estimator = self.network_estimator.lock().clone();
        let network_estimator_changed =
            self.saved_network_estimator.lock().as_ref() != Some(&network_estimator);
        let routing_domain_states: Vec<(Bytes, TableDBSerializedData)> = self
            .get_routing_domain_controllers(RoutingDomainSet::all())
            .iter()
            .flat_map(|rdc| {
                let restore_point = (
                    restore_point_key(rdc.routing_domain()),
                    serialize_table_value(&rdc.read_dyn().get_restore_point().as_deref()),
                );
                rdc.persisted_state().into_iter().chain([restore_point])
            })
            .collect();
        let routing_domain_states: Vec<(Bytes, TableDBSerializedData)> = {
            let saved = self.saved_routing_domain_states.lock();
            routing_domain_states
                .into_iter()
                .filter(|(key, value)| saved.get(key) != Some(&value.0))
                .collect()
        };

        let tdb = self.table_db().await?;
        let dbx = tdb.transact();
        let staged: EyreResult<()> = async {
            // Deletes first: an entry that did not load may share its key with one written now
            for key in &changes.deleted {
                dbx.delete(PERSISTED_ENTRIES_COLUMN, key.clone()).await?;
            }
            for node_id in &changes.written {
                if let Some(persisted) = changes.entries.get(node_id) {
                    dbx.store_serialized(
                        PERSISTED_ENTRIES_COLUMN,
                        persisted_entry_key(node_id),
                        serialize_table_value(&**persisted),
                    )
                    .await?;
                }
            }
            if network_estimator_changed {
                dbx.store_serialized(
                    0,
                    NETWORK_ESTIMATOR_HISTORY,
                    serialize_table_value(&network_estimator),
                )
                .await?;
            }
            for (key, value) in &routing_domain_states {
                dbx.store_serialized(0, key.clone(), TableDBSerializedData(value.0.clone()))
                    .await?;
            }
            Ok(())
        }
        .await;
        if let Err(e) = staged {
            dbx.rollback();
            return Err(e);
        }
        dbx.commit().await?;

        let counts = SaveCounts {
            entries: changes.entries.entry_count(),
            written: changes.written.len(),
            deleted: changes.deleted.len(),
            routing_domain_states: routing_domain_states.len(),
        };
        *self.persisted_entries.lock() = changes.entries;
        if network_estimator_changed {
            *self.saved_network_estimator.lock() = Some(network_estimator);
        }
        self.saved_routing_domain_states.lock().extend(
            routing_domain_states
                .into_iter()
                .map(|(key, value)| (key, value.0)),
        );
        Ok(counts)
    }

    /// Held open between saves: every open runs a full integrity check of the file
    async fn table_db(&self) -> VeilidAPIResult<TableDB> {
        if let Some(tdb) = self.table_db.lock().clone() {
            return Ok(tdb);
        }
        let tdb = self
            .table_store()
            .open(ROUTING_TABLE, ROUTING_TABLE_COLUMN_COUNT)
            .await?;
        *self.table_db.lock() = Some(tdb.clone());
        Ok(tdb)
    }

    /// Throw away the saved routing table and start the cache over from empty
    async fn reset_saved_routing_table(&self, cache_validity_key: Bytes) -> EyreResult<()> {
        let table_store = self.table_store();
        table_store.delete(ROUTING_TABLE).await?;
        let db = table_store
            .open(ROUTING_TABLE, ROUTING_TABLE_COLUMN_COUNT)
            .await?;
        db.store(0, CACHE_VALIDITY_KEY, cache_validity_key).await?;
        *self.persisted_entries.lock() = PersistedEntries::default();
        *self.saved_network_estimator.lock() = None;
        self.saved_routing_domain_states.lock().clear();
        Ok(())
    }

    /// The validity a saved table must carry to load into this node
    fn cache_validity_key(&self) -> Bytes {
        let config = self.config();
        let routing_table_config = &config.network.routing_table;
        let node_ids = self.node_ids();
        serialize_table_value(&RoutingTableCacheValidity {
            format_version: ROUTING_TABLE_FORMAT_VERSION,
            node_ids: &node_ids,
            bootstrap: &routing_table_config.bootstrap,
            bootstrap_keys: &routing_table_config.bootstrap_keys,
            bootstrap_records: &routing_table_config.bootstrap_records,
            network_key_password: config.network.network_key_password.as_deref(),
        })
        .0
    }

    /// Load the saved routing table: bucket entries, network estimator history and
    /// per-routing-domain state
    async fn load(&self) -> EyreResult<()> {
        let cache_validity_key = self.cache_validity_key();

        let table_store = self.table_store();
        let db = table_store
            .open(ROUTING_TABLE, ROUTING_TABLE_COLUMN_COUNT)
            .await?;

        let caches_valid = match db.load(0, CACHE_VALIDITY_KEY).await? {
            Some(v) => v == cache_validity_key,
            None => false,
        };
        if !caches_valid {
            // Caches not valid, start over
            veilid_log!(self debug "cache validity key changed, emptying routing table");
            drop(db);
            self.reset_saved_routing_table(cache_validity_key).await?;
            return Ok(());
        }

        // A routing domain can reject the saved table outright, in which case none of it is
        // applied and the node bootstraps from nothing. This is how a table saved without a
        // recorded bootstrap is repaired: nothing else ever refetches bootstrap records.
        for rdc in self.get_routing_domain_controllers(RoutingDomainSet::all()) {
            if rdc.persisted_state_requires_reset(&db).await {
                veilid_log!(self info "{} has no recorded bootstrap, emptying routing table to bootstrap again", rdc.routing_domain());
                drop(db);
                self.reset_saved_routing_table(cache_validity_key).await?;
                return Ok(());
            }
        }

        // Restore the network estimator histogram if present.
        if let Ok(Some(serialized)) = db.load_serialized(0, NETWORK_ESTIMATOR_HISTORY).await {
            let estimator: NetworkEstimator = table_store.try_deserialize(&serialized)?;
            *self.network_estimator.lock() = estimator.clone();
            *self.saved_network_estimator.lock() = Some(estimator);
        }

        // An entry that does not load stays recorded, so the next save deletes it
        let registry = self.registry();
        let mut persisted_entries = PersistedEntries::default();
        let mut loaded_entries = Vec::new();
        for key in db.get_keys(PERSISTED_ENTRIES_COLUMN).await? {
            let Some(serialized) = db
                .load_serialized(PERSISTED_ENTRIES_COLUMN, key.clone())
                .await?
            else {
                continue;
            };
            let loaded = table_store
                .try_deserialize::<BucketEntryPersisted>(&serialized)
                .map_err(EyreReport::from)
                .and_then(|persisted| {
                    BucketEntryInner::from_persisted(
                        registry.clone(),
                        self.network_manager.clone(),
                        &persisted,
                    )
                    .map(|entry_inner| (persisted, entry_inner))
                });
            match loaded {
                Ok((persisted, entry_inner)) => loaded_entries.push((key, persisted, entry_inner)),
                Err(e) => {
                    veilid_log!(self debug "skipping saved routing table entry: {}", e);
                    persisted_entries.insert_unloaded(key);
                }
            }
        }
        let loaded = {
            let inner = &mut *self.inner.write();
            for (key, persisted, entry_inner) in loaded_entries {
                #[cfg(feature = "geolocation")]
                let entry_inner = {
                    let mut entry_inner = entry_inner;
                    entry_inner.update_geolocation_info();
                    entry_inner
                };
                let entry = Arc::new(BucketEntry::new_with_inner(entry_inner));
                if self.insert_loaded_entry(inner, entry) {
                    persisted_entries.insert(key, persisted);
                } else {
                    veilid_log!(self debug "skipping saved routing table entry whose node ids are taken: {}", persisted.node_ids);
                    persisted_entries.insert_unloaded(key);
                }
            }
            inner.bucket_entry_count()
        };
        *self.persisted_entries.lock() = persisted_entries;

        // Drop relay-only entries: low-quality hops that can't act as safety-route first hops
        // or relays. Important nodes will be re-discovered through bootstrap or peer exchange.
        let removed = self.purge_entries_without_direct_dial_info();
        veilid_log!(self info "loaded routing table: {} entries, {} relay-only purged", loaded, removed);

        // // Make all entries live so they can be tested again
        // let cur_ts = Timestamp::now();
        // inner.with_entries(cur_ts, BucketEntryState::Dead, |v| {
        //     v.with_mut(|e| {
        //         e.make_not_dead(cur_ts);
        //     });
        //     Option::<()>::None
        // });

        // Per-routing-domain state; failures are non-fatal
        let mut saved_routing_domain_states = HashMap::new();
        for rdc in self.get_routing_domain_controllers(RoutingDomainSet::all()) {
            if let Err(e) = rdc.load(&db).await {
                veilid_log!(self warn "failed to load persisted state for {}: {}", rdc.routing_domain(), e);
            }
            // Recorded as read, so the next save skips the states that did not change
            if let Some((key, _)) = rdc.persisted_state() {
                if let Ok(Some(serialized)) = db.load_serialized(0, key.clone()).await {
                    saved_routing_domain_states.insert(key, serialized.0);
                }
            }
            // A restore point that does not load costs a detection, nothing more
            let key = restore_point_key(rdc.routing_domain());
            if let Ok(Some(serialized)) = db.load_serialized(0, key.clone()).await {
                match table_store.try_deserialize::<Option<RoutingDomainRestorePoint>>(&serialized)
                {
                    Ok(restore_point) => {
                        rdc.read_dyn()
                            .replace_restore_point(restore_point.map(Arc::new));
                    }
                    Err(e) => {
                        veilid_log!(self debug "[{}] skipping saved restore point: {}", rdc.routing_domain(), e);
                    }
                }
                saved_routing_domain_states.insert(key, serialized.0);
            }
        }
        self.saved_routing_domain_states
            .lock()
            .extend(saved_routing_domain_states);

        Ok(())
    }

    pub fn route_spec_store(&self) -> &RouteSpecStore {
        &self.route_spec_store
    }

    /// Durations derived from the RPC timeout
    pub fn timings(&self) -> &RoutingTableTimings {
        &self.timings
    }

    /// Record bytes sent/received from this node
    pub fn record_sent_bytes(&self, bytes: ByteCount) {
        self.self_transfer_stats_accounting.lock().0.add_up(bytes);
    }

    pub fn record_received_bytes(&self, bytes: ByteCount) {
        self.self_transfer_stats_accounting.lock().0.add_down(bytes);
    }

    pub fn record_latency(&self, latency: TimestampDuration) {
        self.self_latency_stats_accounting
            .lock()
            .record_latency(latency);
    }

    /////////////////////////////////////
    // Locked operations

    /// Attempt to empty the routing table
    /// May not empty buckets completely if there are existing node_refs
    #[cfg_attr(not(feature = "debug-api"), expect(dead_code))]
    pub async fn purge_buckets(&self) -> EyreResult<()> {
        // Purge route spec store too because it refers to noderefs
        self.route_spec_store().purge().await?;

        // Clear out relays since they refer to noderefs
        for rd in RoutingDomainSet::all() {
            let rdc = self.get_routing_domain_controller(rd);
            {
                let rdd = rdc.write_dyn();
                rdd.clear_bootstrap_peers();
                rdd.set_bootstrap_cache(None);
            }
            let mut edit = rdc.edit_dyn();
            edit.reset();
            rdc.discard_restore_point();
        }

        // Purge buckets
        self.inner.write().purge_buckets();

        self.refresh_summaries(RoutingDomainSet::all());

        Ok(())
    }

    /// Attempt to remove last_connections from entries
    #[cfg_attr(not(feature = "debug-api"), expect(dead_code))]
    pub fn purge_last_connections(&self) {
        self.inner.write().purge_last_connections();
    }

    /// Remove any entry whose direct dial_info socket_address overlaps with `our_node_info`'s
    /// direct dial_info socket_address in `routing_domain`. Relay dial info on either side is
    /// ignored. Returns the number of entries removed.
    pub fn purge_entries_with_overlapping_dial_info(
        &self,
        routing_domain: RoutingDomain,
        our_node_info: &NodeInfo,
    ) -> usize {
        let our_addrs: BTreeSet<SocketAddress> = our_node_info
            .dial_info_detail_list()
            .iter()
            .map(|did| did.dial_info.socket_address())
            .collect();
        if our_addrs.is_empty() {
            return 0;
        }

        let mut inner = self.inner.write();

        // Collect node_ids to remove first; mutating buckets during iteration is unsafe
        let mut to_remove: Vec<NodeIdGroup> = Vec::new();
        for entry in inner.all_entries.iter() {
            let opt_node_ids = entry.with(|e| -> Option<NodeIdGroup> {
                let pi = e.get_peer_info(routing_domain)?;
                for did in pi.node_info().dial_info_detail_list() {
                    if our_addrs.contains(&did.dial_info.socket_address()) {
                        return Some(e.node_ids().clone());
                    }
                }
                None
            });
            if let Some(node_ids) = opt_node_ids {
                to_remove.push(node_ids);
            }
        }

        let removed = to_remove.len();
        for node_ids in &to_remove {
            veilid_log!(self debug "Purging entry {} (dial info overlap with our own)", node_ids);
            for node_id in node_ids.iter() {
                let ck = node_id.kind();
                if !VALID_CRYPTO_KINDS.contains(&ck) {
                    continue;
                }
                if let Ok(bucket_index) = self.calculate_bucket_index(node_id) {
                    let bucket = inner.get_bucket_mut(bucket_index);
                    bucket.remove_entry(node_id.ref_value());
                    self.kick_queue.lock().insert(bucket_index);
                }
            }
        }
        inner.all_entries.remove_expired();
        removed
    }

    /// Remove any entry that has no direct dial_info (only relay-routable). These are low-quality
    /// hops that can never serve as safety-route first hops or relays. Returns the number removed.
    pub fn purge_entries_without_direct_dial_info(&self) -> usize {
        let mut inner = self.inner.write();

        let mut to_remove: Vec<NodeIdGroup> = Vec::new();
        for entry in inner.all_entries.iter() {
            let opt_node_ids = entry.with(|e| -> Option<NodeIdGroup> {
                for routing_domain in RoutingDomainSet::all() {
                    if let Some(pi) = e.get_peer_info(routing_domain) {
                        if !pi.node_info().dial_info_detail_list().is_empty() {
                            return None;
                        }
                    }
                }
                Some(e.node_ids().clone())
            });
            if let Some(node_ids) = opt_node_ids {
                to_remove.push(node_ids);
            }
        }

        let removed = to_remove.len();
        for node_ids in &to_remove {
            veilid_log!(self debug "Purging entry {} (no direct dial info)", node_ids);
            for node_id in node_ids.iter() {
                let ck = node_id.kind();
                if !VALID_CRYPTO_KINDS.contains(&ck) {
                    continue;
                }
                if let Ok(bucket_index) = self.calculate_bucket_index(node_id) {
                    let bucket = inner.get_bucket_mut(bucket_index);
                    bucket.remove_entry(node_id.ref_value());
                    self.kick_queue.lock().insert(bucket_index);
                }
            }
        }
        inner.all_entries.remove_expired();
        removed
    }

    fn queue_bucket_kicks(&self, node_ids: NodeIdGroup) {
        for node_id in node_ids.iter() {
            // Skip node ids we didn't add to buckets
            if !VALID_CRYPTO_KINDS.contains(&node_id.kind()) {
                continue;
            }

            // Put it in the kick queue
            let x = self
                .calculate_bucket_index(node_id)
                .expect_or_log("node ids should already be the right length");
            self.kick_queue.lock().insert(x);
        }
    }

    // Update buckets with new node ids we may have learned belong to this entry
    fn update_bucket_entry_node_ids_inner(
        &self,
        inner: &mut RoutingTableInner,
        entry: Arc<BucketEntry>,
        node_ids: &[NodeId],
    ) -> EyreResult<()> {
        // Mutate the entry under a single entry lock so the node id replacement and its
        // surrounding peer-info reads are atomic. The lock-free routing (bucket membership,
        // subscriber notification) is done afterwards from the returned values.
        let (delta, peer_info_updates) =
            entry.with_mut(|e| -> EyreResult<(NodeIdsDelta, PeerInfoUpdatePairs)> {
                // Peer infos before the change (carry the old node ids).
                let old_peer_infos: Vec<Arc<PeerInfo>> = RoutingDomainSet::all()
                    .into_iter()
                    .filter_map(|rd| e.get_peer_info(rd))
                    .collect();

                // Atomically replace the entry's node ids with the new set, requiring at
                // least one valid node id to remain (refuses otherwise, leaving unchanged).
                let delta = e.replace_node_ids(node_ids)?;
                if delta.is_unchanged() {
                    return Ok((delta, Vec::new()));
                }

                // Peer infos after the change (carry the new node ids).
                let new_peer_infos: Vec<Arc<PeerInfo>> = RoutingDomainSet::all()
                    .into_iter()
                    .filter_map(|rd| e.get_peer_info(rd))
                    .collect();
                if old_peer_infos.len() != new_peer_infos.len() {
                    bail!("changing node ids should not change peer info routing domain count");
                }
                let updates = old_peer_infos.into_iter().zip(new_peer_infos).collect();
                Ok((delta, updates))
            })?;
        if delta.is_unchanged() {
            return Ok(());
        }

        // Update bucket membership to match the changed valid node ids, batching the
        // kick-queue inserts under a single lock.
        let mut kick_indices = Vec::new();
        for removed in &delta.removed {
            let bucket_index = self.calculate_bucket_index(removed)?;
            inner
                .get_bucket_mut(bucket_index)
                .remove_entry(removed.ref_value());
            kick_indices.push(bucket_index);
        }
        for added in &delta.added {
            let bucket_index = self.calculate_bucket_index(added)?;
            inner
                .get_bucket_mut(bucket_index)
                .add_existing_entry(added.value(), entry.clone());
            kick_indices.push(bucket_index);
        }
        self.kick_queue.lock().extend(kick_indices);

        // Inform subscribers that this entry's node ids changed, outside the entry lock.
        for (old_pi, new_pi) in peer_info_updates {
            if old_pi.routing_domain() != new_pi.routing_domain() {
                bail!("changing node ids should not change a peer's routing domain");
            }
            self.on_entry_peer_info_updated(Some(old_pi), Some(new_pi));
        }
        Ok(())
    }

    /// Create a node reference, possibly creating a bucket entry
    /// the 'update_func' closure is called on the node, and, if created,
    /// in a locked fashion as to ensure the bucket entry state is always valid
    #[cfg_attr(feature = "instrument", instrument(level = "trace", skip_all, err, fields(__VEILID_LOG_KEY = self.log_key())))]
    fn create_node_ref(
        &self,
        node_ids: &NodeIdGroup,
        opt_update_func: Option<CreateNodeRefUpdateFunc<'_>>,
    ) -> EyreResult<NodeRef> {
        if node_ids.is_empty() {
            bail!("Can't create node with no node id");
        }

        // Ensure someone isn't trying register this node itself
        if self.matches_own_node_id(node_ids) {
            bail!("can't register own node");
        }

        // A known node whose ids are unchanged needs no table edit, only the read lock
        if let Some(nr) = self.existing_node_ref_with_node_ids(node_ids)? {
            if let Some(update_func) = opt_update_func {
                nr.operate_mut(|e| update_func(e));
            }
            return Ok(nr);
        }

        // Look up all bucket entries and make sure we only have zero or one
        // If we have more than one, pick the one with the best cryptokind to add node ids to

        let (entry, nr) = {
            let mut inner = self.inner.write();

            let mut best_entry: Option<Arc<BucketEntry>> = None;
            let mut supported_node_ids = NodeIdGroup::new();
            for node_id in node_ids.iter() {
                // Ignore node ids we don't support
                if !VALID_CRYPTO_KINDS.contains(&node_id.kind()) {
                    continue;
                }
                supported_node_ids.add(node_id.clone());

                // Find the first in crypto sort order
                let bucket_index = self.calculate_bucket_index(node_id)?;
                let bucket = inner.get_bucket(bucket_index);
                if let Some(entry) = bucket.entry(node_id.ref_value()) {
                    // Best entry is the first one in sorted order that exists from the node id list
                    // Everything else that matches will be overwritten in the bucket and the
                    // existing noderefs will eventually unref and drop the old unindexed bucketentry
                    // We do this instead of merging for now. We could 'kill' entries and have node_refs
                    // rewrite themselves to point to the merged entry upon dereference. The use case for this
                    // may not be worth the effort.
                    best_entry = Some(entry);
                    break;
                };
            }

            // If the entry does exist already, update it
            let entry = if let Some(best_entry) = best_entry {
                // Update the entry with all of the node ids
                if let Err(e) = self.update_bucket_entry_node_ids_inner(
                    &mut inner,
                    best_entry.clone(),
                    node_ids,
                ) {
                    bail!("Not registering new ids for existing node: {}", e);
                }

                best_entry
            } else {
                // Fail out if we can't handle this node
                if supported_node_ids.is_empty() {
                    bail!("Not registering node with no supported node ids");
                }

                // If no entry exists yet, add the first entry to a bucket, possibly evicting a bucket member
                let first_node_id = supported_node_ids[0].clone();
                let bucket_entry = self.calculate_bucket_index(&first_node_id)?;
                let bucket = inner.get_bucket_mut(bucket_entry);
                let new_entry = bucket.add_new_entry(first_node_id.value());
                inner.all_entries.insert(new_entry.clone());
                self.kick_queue.lock().insert(bucket_entry);

                // Update the other bucket entries with the remaining node ids
                if let Err(e) =
                    self.update_bucket_entry_node_ids_inner(&mut inner, new_entry.clone(), node_ids)
                {
                    bail!("Not registering new node: {}", e);
                }

                new_entry
            };

            // Bump the NodeRef (ref_count) while still holding the routing table lock so a
            // concurrent bucket kick sees ref_count > 0 and won't evict this entry between
            // creation and the ref bump.
            let nr = NodeRef::new(self.registry(), entry.clone());
            (entry, nr)
        };

        // Apply the caller's update (e.g. peer info) only after node id replacement
        // succeeded; a failed replacement bails above, rejecting the whole update.
        if let Some(update_func) = opt_update_func {
            entry.with_mut(|e| update_func(e));
        }

        Ok(nr)
    }

    /// Bucket a reloaded entry under each of its valid node ids; false when one of them is ours
    /// or already belongs to another entry
    fn insert_loaded_entry(&self, inner: &mut RoutingTableInner, entry: Arc<BucketEntry>) -> bool {
        let node_ids = entry.with(|e| e.node_ids());
        if self.matches_own_node_id(&node_ids) {
            return false;
        }
        let mut placements = Vec::new();
        for node_id in node_ids
            .iter()
            .filter(|nid| VALID_CRYPTO_KINDS.contains(&nid.kind()))
        {
            let Ok(bucket_index) = self.calculate_bucket_index(node_id) else {
                return false;
            };
            if inner
                .get_bucket(bucket_index)
                .entry(node_id.ref_value())
                .is_some()
            {
                return false;
            }
            placements.push((bucket_index, node_id.value()));
        }
        let mut kick_queue = self.kick_queue.lock();
        for (bucket_index, bare_node_id) in placements {
            inner
                .get_bucket_mut(bucket_index)
                .add_existing_entry(bare_node_id, entry.clone());
            kick_queue.insert(bucket_index);
        }
        inner.all_entries.insert(entry);
        true
    }

    /// The entry create_node_ref would pick for `node_ids`, if its node ids are exactly those
    fn existing_node_ref_with_node_ids(
        &self,
        node_ids: &NodeIdGroup,
    ) -> EyreResult<Option<NodeRef>> {
        let inner = self.inner.read();
        for node_id in node_ids.iter() {
            if !VALID_CRYPTO_KINDS.contains(&node_id.kind()) {
                continue;
            }
            let bucket_index = self.calculate_bucket_index(node_id)?;
            if let Some(entry) = inner.get_bucket(bucket_index).entry(node_id.ref_value()) {
                // Ref bumped under the table lock, as in lookup_node_id
                return Ok(entry
                    .with(|e| e.node_ids_equal(node_ids))
                    .then(|| NodeRef::new(self.registry(), entry)));
            }
        }
        Ok(None)
    }

    /// Resolve an existing routing table entry using any crypto kind and return a reference to it
    #[cfg_attr(feature = "instrument", instrument(level = "trace", skip_all, err, fields(__VEILID_LOG_KEY = self.log_key())))]
    pub fn lookup_bare_node_id(&self, node_id_key: BareNodeId) -> EyreResult<Option<NodeRef>> {
        for ck in VALID_CRYPTO_KINDS {
            if let Some(nr) = self.lookup_node_id(NodeId::new(ck, node_id_key.clone()))? {
                return Ok(Some(nr));
            }
        }
        Ok(None)
    }

    /// Resolve an existing routing table entry and return a reference to it
    #[cfg_attr(feature = "instrument", instrument(level = "trace", skip_all, err, fields(__VEILID_LOG_KEY = self.log_key())))]
    pub fn lookup_node_id(&self, node_id: NodeId) -> EyreResult<Option<NodeRef>> {
        if self.matches_own_node_id(std::slice::from_ref(&node_id)) {
            bail!("can't look up own node id in routing table");
        }
        if !VALID_CRYPTO_KINDS.contains(&node_id.kind()) {
            bail!("can't look up node id with invalid crypto kind");
        }

        let bucket_index = self.calculate_bucket_index(&node_id)?;
        // Construct the NodeRef (bump ref_count) while still holding the read lock, so a
        // concurrent bucket kick sees ref_count > 0 and won't evict this entry in the
        // window between lookup and ref bump.
        let opt_nr = {
            let inner = self.inner.read();
            let bucket = inner.get_bucket(bucket_index);
            bucket
                .entry(node_id.ref_value())
                .map(|e| NodeRef::new(self.registry(), e))
        };
        Ok(opt_nr)
    }

    /// Our own published direct dial-info socket addresses for a routing domain.
    pub fn own_direct_dial_info_addresses(
        &self,
        routing_domain: RoutingDomain,
    ) -> BTreeSet<SocketAddress> {
        self.get_current_peer_info(routing_domain)
            .node_info()
            .dial_info_detail_list()
            .iter()
            .map(|did| did.dial_info.socket_address())
            .collect()
    }

    /// True if `socket_address` is one of our own published direct dial-info addresses in any
    /// routing domain; dialing it would connect us back to ourselves.
    pub fn is_own_direct_dial_info_address(&self, socket_address: &SocketAddress) -> bool {
        RoutingDomainSet::all().into_iter().any(|rd| {
            self.own_direct_dial_info_addresses(rd)
                .contains(socket_address)
        })
    }

    pub(crate) fn peer_info_verified(&self, key: &(Bytes, SignatureGroup)) -> bool {
        self.verified_peer_infos.lock().get(key).is_some()
    }

    pub(crate) fn note_peer_info_verified(&self, key: (Bytes, SignatureGroup)) {
        self.verified_peer_infos.lock().insert(key, ());
    }

    /// The verified peer info this table already holds for any of `node_ids` in `routing_domain`
    fn existing_peer_info(
        &self,
        routing_domain: RoutingDomain,
        node_ids: &NodeIdGroup,
    ) -> Option<Arc<PeerInfo>> {
        node_ids.iter().find_map(|node_id| {
            self.lookup_node_id(node_id.clone())
                .ok()
                .flatten()
                .and_then(|nr| nr.operate(|e| e.get_peer_info(routing_domain)))
        })
    }

    /// Shortcut function to add a node to our routing table if it doesn't exist
    /// and add the dial info we have for it. Returns a noderef filtered to
    /// the routing domain in which this node was registered for convenience.
    #[cfg_attr(feature = "instrument", instrument(level = "trace", skip_all, err, fields(__VEILID_LOG_KEY = self.log_key())))]
    pub fn register_node_with_peer_info(
        &self,
        unverified: Arc<UnverifiedPeerInfo>,
        allow_invalid: bool,
    ) -> EyreResult<FilteredNodeRef> {
        let routing_domain = unverified.routing_domain();

        // if our own node is in the list, then ignore it as we don't add ourselves to our own routing table
        if self
            .routing_table()
            .matches_own_node_id(unverified.node_ids())
        {
            bail!("can't register own node id in routing table");
        }

        // node can not be its own relay
        let node_info = unverified.node_info();
        let relay_ids = node_info.relay_ids();
        let node_ids = unverified.node_ids().clone();
        if node_ids.contains_any_from_iter(relay_ids.iter()) {
            bail!("node can not be its own relay");
        }

        // Reject if any of the node's own direct dial info exactly matches any of our own
        // direct dial info (relay dial info excluded; sharing a relay is fine)
        let our_addrs = self.own_direct_dial_info_addresses(routing_domain);
        if !our_addrs.is_empty()
            && node_info
                .dial_info_detail_list()
                .iter()
                .any(|did| our_addrs.contains(&did.dial_info.socket_address()))
        {
            bail!(
                "can't register node {:?} because its dial info matches our own",
                unverified.node_ids()
            );
        }

        if !allow_invalid {
            // verify signature
            if unverified.signatures().is_empty() {
                bail!(
                    "peerinfo for {:?} has no valid signature",
                    unverified.node_ids()
                );
            }
        }
        // verify node info is valid in this routing domain
        let valid_routing_domains = self.get_node_info_routing_domains(node_info);
        if !valid_routing_domains.contains(routing_domain) {
            bail!(
                "peerinfo for {:?} not valid in the {:?} routing domain",
                unverified.node_ids(),
                routing_domain
            );
        }

        // Signatures are checked once the cheap rejections are past, and not at all when the
        // table already holds this node info: same ids and timestamp mean the same bytes
        let peer_info = match self.existing_peer_info(routing_domain, &node_ids) {
            Some(current)
                if !current.signatures().is_empty()
                    && current.node_ids() == &node_ids
                    && current.node_info().timestamp() == unverified.timestamp() =>
            {
                current
            }
            _ => Arc::new(unverified.verify_and_extract(self)?),
        };

        let mut updated = false;
        let mut old_peer_info = None;
        let nr = self.create_node_ref(
            &node_ids,
            Some(Box::new(|e| {
                old_peer_info = e.get_peer_info(routing_domain);
                updated = e.update_peer_info(routing_domain, peer_info.clone());
            })),
        )?;

        // Process any new or updated PeerInfo
        if old_peer_info.is_none() || updated {
            self.on_entry_peer_info_updated(old_peer_info, Some(peer_info));
        }

        Ok(nr.custom_filtered(NodeRefFilter::new().with_routing_domain(routing_domain)))
    }

    /// Shortcut function to add a node to our routing table if it doesn't exist
    /// Adds no peer info to the node at this time. The node will unreachable except over the
    /// existing inbound flow that is established to us.
    /// Returns a noderef filtered to the routing domain in which this node was registered for convenience.
    #[cfg_attr(feature = "instrument", instrument(level = "trace", skip_all, err, fields(__VEILID_LOG_KEY = self.log_key())))]
    pub fn register_node_without_peer_info(
        &self,
        routing_domain: RoutingDomain,
        node_id: NodeId,
    ) -> EyreResult<FilteredNodeRef> {
        let nr = self.create_node_ref(&NodeIdGroup::from(node_id), None)?;

        // Enforce routing domain
        let nr = nr.custom_filtered(NodeRefFilter::new().with_routing_domain(routing_domain));
        Ok(nr)
    }

    /// Drop learned per-peer transport evidence table-wide.
    ///
    /// Call this only for OUR OWN address changes (a local routable interface address,
    /// or a detected external address change): failures we blamed on peers' transports
    /// may have been ours. Never on peer info changes: our peer info carries our relay
    /// set, a relay swap says nothing about other peers, and clearing would wipe the
    /// evidence relay candidacy needs.
    pub(crate) fn clear_all_transport_evidence(&self) {
        self.inner.write().clear_all_transport_evidence();
    }

    /// Signal that some peerinfo changed (ours or theirs) so contact-method
    /// viability gets recomputed; the task's basis check skips unchanged entries
    pub(crate) fn mark_cm_viability_dirty(&self) {
        self.cm_viability_dirty.store(true, Ordering::Release);
    }

    /// Take the viability dirty flag; changes that land during a recompute re-set it
    pub(crate) fn take_cm_viability_dirty(&self) -> bool {
        self.cm_viability_dirty.swap(false, Ordering::AcqRel)
    }

    /// Queue a save for once changes settle
    pub(crate) fn mark_save_dirty(&self) {
        self.save_schedule.lock().changed(Timestamp::now());
        #[cfg(feature = "app-lifecycle")]
        self.hold_background_time_for_save();
    }

    /// Queue a save that waits only for the least interval between saves, for state a restart
    /// restores at once
    pub(crate) fn mark_save_urgent(&self) {
        self.save_schedule.lock().urgent_change(Timestamp::now());
        #[cfg(feature = "app-lifecycle")]
        self.hold_background_time_for_save();
    }

    /// Count an entry changing state toward the next save
    pub(crate) fn mark_entry_state_changed(&self) {
        self.save_schedule.lock().state_changed(Timestamp::now());
        #[cfg(feature = "app-lifecycle")]
        self.hold_background_time_for_save();
    }

    /// Called whenever a routing table entry is:
    /// 1. created or updated with new peer information
    /// 2. has a node id added or removed (per CryptoKind)
    ///   * by a new peer info showing up with a different overlapping node id list
    ///   * by a bucket kick removing an entry from a bucket for some cryptokind
    /// 3. (todo) is removed from some routing domain (peer info gone)
    ///
    /// It is not called when:
    /// 1. nodes are registered by id for an existing connection but have no peer info yet
    /// 2. nodes are removed that don't have any peer info
    fn on_entry_peer_info_updated(
        &self,
        old_peer_info: Option<Arc<PeerInfo>>,
        new_peer_info: Option<Arc<PeerInfo>>,
    ) {
        self.mark_cm_viability_dirty();
        let (routing_domain, node_ids) = match (old_peer_info.as_ref(), new_peer_info.as_ref()) {
            (None, None) => {
                return;
            }
            (None, Some(new_pi)) => (new_pi.routing_domain(), new_pi.node_ids().clone()),
            (Some(old_pi), None) => (old_pi.routing_domain(), old_pi.node_ids().clone()),
            (Some(old_pi), Some(new_pi)) => {
                if old_pi.routing_domain() != new_pi.routing_domain() {
                    veilid_log!(self error "routing domains should be the same here");
                    return;
                }
                let mut node_ids = old_pi.node_ids().clone();
                node_ids.add_all_from_iter(new_pi.node_ids().iter());
                (new_pi.routing_domain(), node_ids)
            }
        };
        self.mark_save_dirty();

        // If this is our relay, then redo our own peerinfo because
        // if we have relayed peerinfo, then changing the relay's peerinfo
        // changes our own peer info
        let rdc = self.get_routing_domain_controller(routing_domain);
        let rdd = rdc.read_dyn();
        let our_relay_node_ids = rdd
            .relays()
            .iter()
            .flat_map(|rdr| rdr.relay_node.node_ids().to_vec())
            .collect::<Vec<_>>();
        if node_ids.contains_any_from_iter(our_relay_node_ids.iter()) {
            rdd.invalidate();
            // Publishing reads the detail again; a held read lock can deadlock a waiting writer
            drop(rdd);
            rdc.publish_peer_info();
        }

        // Update tables that use peer info
        // if let Some(_old_pi) = old_peer_info {
        //     // Remove old info
        // }
        // if let Some(_new_pi) = new_peer_info {
        //     // Add new info
        // }
    }

    pub fn clear_punishments(&self) {
        let cur_ts = Timestamp::now();
        self.inner
            .read()
            .with_entries(cur_ts, BucketEntryState::Punished, |e| {
                e.with_mut(|ei| ei.set_punished(None));
                Option::<()>::None
            });
    }

    pub fn get_outbound_relay_peer(
        &self,
        _routing_domain: routing_table::RoutingDomain,
    ) -> Option<Arc<routing_table::PeerInfo>> {
        // unimplemented!
        None
    }

    /// Find the first dial info that matches a routing domain set and filter
    pub fn first_filtered_dial_info_detail(
        &self,
        routing_domain_set: RoutingDomainSet,
        filter: &DialInfoFilter,
    ) -> Option<DialInfoDetail> {
        if filter.is_dead() || routing_domain_set.is_empty() {
            return None;
        }
        for rdd in self.get_routing_domain_controllers(routing_domain_set) {
            let rdd = rdd.read_dyn();
            if let Some(did) = rdd
                .dial_info_details()
                .iter()
                .find(|did| did.matches_filter(filter))
                .cloned()
            {
                return Some(did);
            }
        }
        None
    }

    /// Makes a filter that finds nodes with a matching inbound dialinfo
    #[cfg_attr(all(target_arch = "wasm32", target_os = "unknown"), expect(dead_code))]
    pub fn make_inbound_dial_info_entry_filter<'a>(
        &self,
        routing_domain: RoutingDomain,
        dial_info_filter: DialInfoFilter,
    ) -> RoutingTableEntryFilter<'a> {
        let self_has_matching_dial_info = self
            .first_filtered_dial_info_detail(routing_domain.into(), &dial_info_filter)
            .is_some();

        // does it have matching public dial info?
        Box::new(
            move |opt_snap: &Option<BucketEntrySnapshot>, _cur_ts: Timestamp| {
                if let Some(snap) = opt_snap {
                    if let Some(pi) = snap.get_peer_info(routing_domain) {
                        if pi
                            .node_info()
                            .first_filtered_dial_info_detail(DialInfoDetail::NO_SORT, &|did| {
                                did.matches_filter(&dial_info_filter)
                            })
                            .is_some()
                        {
                            return true;
                        }
                    }
                    false
                } else {
                    self_has_matching_dial_info
                }
            },
        )
    }

    /// Makes a filter that finds nodes capable of dialing a particular outbound dialinfo
    pub fn make_outbound_dial_info_entry_filter<'a>(
        &self,
        routing_domain: RoutingDomain,
        dial_info: DialInfo,
    ) -> RoutingTableEntryFilter<'a> {
        let outbound_dial_info_filter = self
            .get_routing_domain_controller(routing_domain)
            .read_dyn()
            .outbound_dial_info_filter();
        let self_has_matching_dial_info = dial_info.matches_filter(&outbound_dial_info_filter);

        // does the node's outbound capabilities match the dialinfo?
        Box::new(
            move |opt_snap: &Option<BucketEntrySnapshot>, _cur_ts: Timestamp| {
                if let Some(snap) = opt_snap {
                    if let Some(pi) = snap.get_peer_info(routing_domain) {
                        let ni = pi.node_info();
                        let dif = DialInfoFilter::all()
                            .with_protocol_type_set(ni.outbound_protocols())
                            .with_address_type_set(ni.address_types());
                        if dial_info.matches_filter(&dif) {
                            return true;
                        }
                    }
                    false
                } else {
                    self_has_matching_dial_info
                }
            },
        )
    }

    /// Registers a set of PeerInfo with the routing table
    #[cfg_attr(
        feature = "instrument",
        instrument(level = "trace", skip(self, peer_info_list), fields(__VEILID_LOG_KEY = self.log_key()))
    )]
    pub fn register_nodes_with_peer_info_list(
        &self,
        peer_info_list: Vec<Arc<UnverifiedPeerInfo>>,
    ) -> Vec<NodeRef> {
        // Register nodes we'd found
        let mut out = Vec::<NodeRef>::with_capacity(peer_info_list.len());
        for p in peer_info_list {
            // Don't register our own node
            if self.matches_own_node_id(p.node_ids()) {
                continue;
            }

            // Register the node if it's new
            match self.register_node_with_peer_info(p, false) {
                Ok(nr) => out.push(nr.unfiltered()),
                Err(e) => {
                    veilid_log!(self debug "failed to register node with peer info from find node answer: {}", e);
                }
            }
        }
        out
    }

    /// Find the best routing domain for a node info
    /// Returns Some(rd) if there is a 'best' routing domain for this node info given its stated origin
    /// Returns None if no node info from this origin is acceptable
    #[cfg_attr(
        feature = "instrument",
        instrument(level = "trace", target = "rtab", skip_all, fields(__VEILID_LOG_KEY = self.log_key()))
    )]
    pub fn find_best_node_info_routing_domain(
        &self,
        origin_routing_domain: RoutingDomain,
        node_info: &NodeInfo,
    ) -> Option<RoutingDomain> {
        // See what routing domains it could be placed in
        let valid_routing_domains = self.get_node_info_routing_domains(node_info);

        // For each valid routing domain in preference order,
        // see if the valid domains can accept peer info from this origin
        for rd in valid_routing_domains {
            let origin_routing_domains = self.origin_routing_domains(rd);
            if origin_routing_domains.contains(origin_routing_domain) {
                return Some(rd);
            }
        }

        None
    }
}
