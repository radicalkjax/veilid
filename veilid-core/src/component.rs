// Shared with the subsystems mock-api compiles out, so some have no caller in a mock build
#![cfg_attr(feature = "mock-api", allow(unused_macros))]

use super::*;

impl_veilid_log_facility!("registry");

pub(crate) trait AsAnyArcSendSync {
    fn as_any_arc_send_sync(self: Arc<Self>) -> Arc<dyn core::any::Any + Send + Sync>;
}

impl<T: Send + Sync + 'static> AsAnyArcSendSync for T {
    fn as_any_arc_send_sync(self: Arc<Self>) -> Arc<dyn core::any::Any + Send + Sync> {
        self
    }
}

// Wraps stats record-site statements so they compile out when the `stats` feature is off
#[cfg(feature = "stats")]
macro_rules! veilid_stats {
    ($($t:tt)*) => {{ $($t)* }};
}
#[cfg(not(feature = "stats"))]
macro_rules! veilid_stats {
    ($($t:tt)*) => {{}};
}
pub(crate) use veilid_stats;

/// A component's optimization stats snapshot. Consumed only by JSON serialization to the stats
/// log, so any `serde::Serialize` type qualifies via the blanket impl.
#[cfg(feature = "stats")]
pub(crate) trait VeilidComponentStats: core::fmt::Debug + Send + Sync {
    fn to_json_value(&self) -> serde_json::Value;
}

#[cfg(feature = "stats")]
impl<T: serde::Serialize + core::fmt::Debug + Send + Sync> VeilidComponentStats for T {
    fn to_json_value(&self) -> serde_json::Value {
        serde_json::to_value(self).unwrap_or(serde_json::Value::Null)
    }
}

/// PascalCase component name to snake_case stats key, acronym-aware (RPCProcessor to rpc_processor).
///
/// Always snake_case regardless of json-camel-case: debug-only output with no schema, so one
/// parse format beats matching each binding's convention.
#[cfg(feature = "stats")]
fn component_stats_key(name: &str) -> String {
    let chars: Vec<char> = name.chars().collect();
    let mut snake = String::new();
    for (i, &c) in chars.iter().enumerate() {
        if c.is_ascii_uppercase() {
            let prev_lower = i > 0 && chars[i - 1].is_ascii_lowercase();
            let acronym_end = i > 0
                && chars[i - 1].is_ascii_uppercase()
                && i + 1 < chars.len()
                && chars[i + 1].is_ascii_lowercase();
            if i > 0 && (prev_lower || acronym_end) {
                snake.push('_');
            }
            snake.push(c.to_ascii_lowercase());
        } else {
            snake.push(c);
        }
    }
    snake
}

pub(crate) trait VeilidComponent:
    AsAnyArcSendSync + VeilidComponentRegistryAccessor + core::fmt::Debug
{
    fn name(&self) -> &'static str;
    fn log_facilities(&self) -> VeilidComponentLogFacilities;
    /// Zero the component's cumulative stats counters, if it keeps any.
    #[cfg(feature = "stats")]
    fn reset_stats(&self) {}

    /// A JSON-serializable stats snapshot, or None if the component has no stats.
    #[cfg(feature = "stats")]
    fn stats(&self) -> Option<Box<dyn VeilidComponentStats>> {
        None
    }
    fn init(&self) -> PinBoxFuture<'_, EyreResult<()>>;
    fn post_init(&self) -> PinBoxFuture<'_, EyreResult<()>>;
    fn pre_terminate(&self) -> PinBoxFuture<'_, ()>;
    fn terminate(&self) -> PinBoxFuture<'_, ()>;
}

pub(crate) trait VeilidComponentRegistryAccessor {
    fn registry(&self) -> VeilidComponentRegistry;

    fn config(&self) -> Arc<VeilidConfig> {
        self.registry().unlocked_inner.startup_options.config()
    }
    fn update_callback(&self) -> UpdateCallback {
        self.registry()
            .unlocked_inner
            .startup_options
            .update_callback()
    }
    fn event_bus(&self) -> EventBus {
        self.registry().event_bus()
    }
    fn log_key(&self) -> VeilidLogKey {
        self.registry().log_key()
    }
}

/// Borrow guard holding a shared reference to a registered component for the duration of `'a`.
pub struct VeilidComponentGuard<'a, T: Send + Sync + 'static> {
    component: Arc<T>,
    _phantom: core::marker::PhantomData<&'a T>,
}

impl<T> core::ops::Deref for VeilidComponentGuard<'_, T>
where
    T: Send + Sync + 'static,
{
    type Target = T;

    fn deref(&self) -> &Self::Target {
        &self.component
    }
}

/// Free arc guard holding a shared reference to a registered component. Must be dropped at or before termination.
pub struct VeilidComponentGuardArc<T: Send + Sync + 'static> {
    component: Arc<T>,
}

impl<T> fmt::Debug for VeilidComponentGuardArc<T>
where
    T: Send + Sync + 'static,
{
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "VeilidComponentGuardArc<{}>",
            core::any::type_name::<T>()
        )
    }
}

impl<T> core::ops::Deref for VeilidComponentGuardArc<T>
where
    T: Send + Sync + 'static,
{
    type Target = T;

    fn deref(&self) -> &Self::Target {
        &self.component
    }
}

type ComponentTypeMap = HashMap<core::any::TypeId, Arc<dyn VeilidComponent + Send + Sync>>;

#[derive(Debug)]
struct VeilidComponentRegistryInner {
    init_order: Vec<core::any::TypeId>,
    #[cfg(any(test, feature = "test-util"))]
    mock: bool,
}

#[derive(Debug)]
struct VeilidComponentRegistryUnlockedInner {
    /// Registration order, behind the lock that serializes registrations
    inner: Mutex<VeilidComponentRegistryInner>,
    /// Registered components, replaced whole on each registration so a lookup takes no lock
    type_map: ArcSwap<ComponentTypeMap>,
    /// Startup options the components are built from
    startup_options: VeilidStartupOptions,
    /// Namespace from the config
    namespace: &'static str,
    /// Program name from the config
    program_name: &'static str,
    /// Log key for this registry's components
    log_key: &'static str,
    /// Event bus shared by the components
    event_bus: EventBus,
    /// Serializes init and termination, and records whether init completed
    init_lock: AsyncMutex<bool>,
}

#[derive(Clone, Debug)]
pub(crate) struct VeilidComponentRegistry {
    unlocked_inner: Arc<VeilidComponentRegistryUnlockedInner>,
}

impl VeilidComponentRegistry {
    pub fn new(startup_options: VeilidStartupOptions) -> Self {
        let namespace = startup_options.config().namespace.to_static_str();
        let program_name = startup_options.config().program_name.to_static_str();

        let log_key = VeilidLayerFilter::make_veilid_log_key(program_name, namespace);

        Self {
            unlocked_inner: Arc::new(VeilidComponentRegistryUnlockedInner {
                inner: Mutex::new(VeilidComponentRegistryInner {
                    init_order: Vec::new(),
                    #[cfg(any(test, feature = "test-util"))]
                    mock: false,
                }),
                type_map: ArcSwap::from_pointee(HashMap::new()),
                startup_options,
                namespace,
                program_name,
                log_key,
                event_bus: EventBus::new(),
                init_lock: AsyncMutex::new(false),
            }),
        }
    }

    /// Replace the config every component reads through `config()`. The caller has checked
    /// that the node is detached and that the keys needing a restart are unchanged.
    pub fn set_config(&self, config: VeilidConfig) {
        self.unlocked_inner.startup_options.set_config(config);
    }

    #[cfg(any(test, feature = "test-util"))]
    pub fn enable_mock(&self) {
        let mut inner = self.unlocked_inner.inner.lock();
        inner.mock = true;
    }
    // #[cfg(any(test, feature = "test-util"))]
    // pub fn is_mock(&self) -> bool {
    //     let inner = self.unlocked_inner.inner.lock();
    //     inner.mock
    // }

    #[expect(dead_code)]
    pub fn namespace(&self) -> &'static str {
        self.unlocked_inner.namespace
    }

    #[allow(dead_code)]
    pub fn program_name(&self) -> &'static str {
        self.unlocked_inner.program_name
    }

    pub fn log_key(&self) -> VeilidLogKey {
        self.unlocked_inner.log_key
    }

    pub fn event_bus(&self) -> EventBus {
        self.unlocked_inner.event_bus.clone()
    }

    pub fn register<
        T: VeilidComponent + Send + Sync + 'static,
        F: FnOnce(VeilidComponentRegistry) -> T,
    >(
        &self,
        component_constructor: F,
    ) {
        let component = Arc::new(component_constructor(self.clone()));
        self.add_component(core::any::TypeId::of::<T>(), component);
    }

    pub fn register_with_context<
        C,
        T: VeilidComponent + Send + Sync + 'static,
        F: FnOnce(VeilidComponentRegistry, C) -> T,
    >(
        &self,
        component_constructor: F,
        context: C,
    ) {
        let component = Arc::new(component_constructor(self.clone(), context));
        self.add_component(core::any::TypeId::of::<T>(), component);
    }

    // Add to type map and initialization order
    fn add_component(
        &self,
        component_type_id: core::any::TypeId,
        component: Arc<dyn VeilidComponent + Send + Sync>,
    ) {
        let mut inner = self.unlocked_inner.inner.lock();
        let mut type_map = ComponentTypeMap::clone(&self.unlocked_inner.type_map.load());
        let replaced = type_map.insert(component_type_id, component).is_some();
        self.unlocked_inner.type_map.store(Arc::new(type_map));
        if replaced {
            veilid_log!(self error "should not register same component twice");
            return;
        }
        inner.init_order.push(component_type_id);
    }

    pub async fn init(&self) -> EyreResult<()> {
        let Some(mut _init_guard) = self.unlocked_inner.init_lock.try_lock() else {
            bail!("init should only happen one at a time");
        };
        if *_init_guard {
            bail!("already initialized");
        }

        VeilidLayerFilter::init_veilid_component_log_facilities(
            self.log_key(),
            self.get_init_order()
                .into_iter()
                .map(|x| x.log_facilities())
                .collect(),
        )?;

        // Event bus starts up early
        self.unlocked_inner.event_bus.startup()?;

        // Process components in initialization order
        let init_order = self.get_init_order();
        let mut initialized = vec![];
        for component in init_order {
            if let Err(e) = component.init().await {
                veilid_log!(self error "Error initializing component '{}': {}", component.name(), e);
                self.terminate_inner(initialized).await;
                self.unlocked_inner.event_bus.shutdown().await;
                return Err(e);
            }
            initialized.push(component);
        }

        *_init_guard = true;
        Ok(())
    }

    pub async fn post_init(&self) -> EyreResult<()> {
        let Some(mut _init_guard) = self.unlocked_inner.init_lock.try_lock() else {
            bail!("init should only happen one at a time");
        };
        if !*_init_guard {
            bail!("not initialized");
        }

        let init_order = self.get_init_order();
        let mut post_initialized = vec![];
        for component in init_order {
            if let Err(e) = component.post_init().await {
                self.pre_terminate_inner(post_initialized).await;
                return Err(e);
            }
            post_initialized.push(component)
        }
        Ok(())
    }

    pub async fn pre_terminate(&self) {
        let Some(mut _init_guard) = self.unlocked_inner.init_lock.try_lock() else {
            veilid_log!(self error "terminate should only happen one at a time");
            return;
        };
        if !*_init_guard {
            veilid_log!(self error "not initialized");
            return;
        }

        let init_order = self.get_init_order();
        self.pre_terminate_inner(init_order).await;
    }

    pub async fn terminate(&self) {
        let Some(mut _init_guard) = self.unlocked_inner.init_lock.try_lock() else {
            veilid_log!(self error "terminate should only happen one at a time");
            return;
        };
        if !*_init_guard {
            veilid_log!(self error "not initialized");
            return;
        }

        // Terminate components in reverse initialization order
        let init_order = self.get_init_order();
        self.terminate_inner(init_order).await;

        // Event bus shuts down last
        self.unlocked_inner
            .event_bus
            .shutdown()
            .measure(|dur| {
                veilid_log!(self debug "terminate 'EventBus' took {:#}", dur);
            })
            .await;

        // Remoave all registered component log facilities from VeilidLayerFilter for this log key
        if let Err(e) = VeilidLayerFilter::terminate_veilid_component_log_facilities(self.log_key())
        {
            #[expect(
                clippy::disallowed_macros,
                reason = "the log facilities are what failed, so nothing is left to log through"
            )]
            {
                eprintln!("Error terminating log facilities: {}", e);
            }
        }

        *_init_guard = false;
    }

    async fn pre_terminate_inner(
        &self,
        pre_initialized: Vec<Arc<dyn VeilidComponent + Send + Sync>>,
    ) {
        for component in pre_initialized.iter().rev() {
            let name = component.name();
            component
                .pre_terminate()
                .measure(|dur| {
                    veilid_log!(self debug "pre_terminate '{}' took {:#}", name, dur);
                })
                .await;
        }
    }
    async fn terminate_inner(&self, initialized: Vec<Arc<dyn VeilidComponent + Send + Sync>>) {
        for component in initialized.iter().rev() {
            let refs = Arc::strong_count(component);
            if refs > 2 {
                veilid_log!(self warn
                    "Terminating component '{}' while still referenced ({} extra references)",
                    component.name(),
                    refs - 2
                );
            }
            let name = component.name();
            component
                .terminate()
                .measure(|dur| {
                    veilid_log!(self debug "terminate '{}' took {:#}", name, dur);
                })
                .await;
        }
    }

    fn get_init_order(&self) -> Vec<Arc<dyn VeilidComponent + Send + Sync>> {
        let inner = self.unlocked_inner.inner.lock();
        let type_map = self.unlocked_inner.type_map.load();
        inner
            .init_order
            .iter()
            .map(|id| type_map.get(id).unwrap_or_log().clone())
            .collect::<Vec<_>>()
    }

    // Stats keyed by component name; the registry lock is dropped before each stats() call
    #[cfg(feature = "stats")]
    pub(crate) fn component_stats(&self) -> serde_json::Value {
        let mut map = serde_json::Map::new();
        for component in self.get_init_order() {
            if let Some(stats) = component.stats() {
                map.insert(component_stats_key(component.name()), stats.to_json_value());
            }
        }
        serde_json::Value::Object(map)
    }

    // Zero every component's cumulative counters; the registry lock is dropped first
    #[cfg(feature = "stats")]
    pub(crate) fn reset_component_stats(&self) {
        for component in self.get_init_order() {
            component.reset_stats();
        }
    }

    //////////////////////////////////////////////////////////////

    pub fn lookup<'a, T: VeilidComponent + Send + Sync + 'static>(
        &self,
    ) -> Option<VeilidComponentGuard<'a, T>> {
        let component_type_id = core::any::TypeId::of::<T>();
        let component_dyn = self
            .unlocked_inner
            .type_map
            .load()
            .get(&component_type_id)?
            .clone();
        let component = component_dyn
            .as_any_arc_send_sync()
            .downcast::<T>()
            .unwrap_or_log();
        Some(VeilidComponentGuard {
            component,
            _phantom: core::marker::PhantomData {},
        })
    }

    pub fn lookup_arc<T: VeilidComponent + Send + Sync + 'static>(
        &self,
    ) -> Option<VeilidComponentGuardArc<T>> {
        let component_type_id = core::any::TypeId::of::<T>();
        let component_dyn = self
            .unlocked_inner
            .type_map
            .load()
            .get(&component_type_id)?
            .clone();
        let component = component_dyn
            .as_any_arc_send_sync()
            .downcast::<T>()
            .unwrap_or_log();
        Some(VeilidComponentGuardArc { component })
    }
}

impl VeilidComponentRegistryAccessor for VeilidComponentRegistry {
    fn registry(&self) -> VeilidComponentRegistry {
        self.clone()
    }
}

impl VeilidComponentRegistryAccessor for &VeilidComponentRegistry {
    fn registry(&self) -> VeilidComponentRegistry {
        (*self).clone()
    }
}

////////////////////////////////////////////////////////////////////

macro_rules! impl_veilid_component_accessors {
    ($struct_name:ty) => {
        impl VeilidComponentRegistryAccessor for $struct_name {
            fn registry(&self) -> VeilidComponentRegistry {
                self.registry.clone()
            }
        }
        impl VeilidComponentRegistryAccessor for &$struct_name {
            fn registry(&self) -> VeilidComponentRegistry {
                self.registry.clone()
            }
        }
    };
}

pub(crate) use impl_veilid_component_accessors;

/////////////////////////////////////////////////////////////////////

macro_rules! impl_veilid_component {
    (@inner $component_name:ty, $($stats_method:tt)*) => {
        impl_veilid_component_accessors!($component_name);

        impl VeilidComponent for $component_name {
            fn name(&self) -> &'static str {
                stringify!($component_name)
            }

            fn log_facilities(&self) -> VeilidComponentLogFacilities {
                <$component_name>::log_facilities_impl(self)
            }

            $($stats_method)*

            fn init(&self) -> PinBoxFuture<'_, EyreResult<()>> {
                Box::pin(async { self.init_async().await })
            }

            fn post_init(&self) -> PinBoxFuture<'_, EyreResult<()>> {
                Box::pin(async { self.post_init_async().await })
            }

            fn pre_terminate(&self) -> PinBoxFuture<'_, ()> {
                Box::pin(async { self.pre_terminate_async().await })
            }

            fn terminate(&self) -> PinBoxFuture<'_, ()> {
                Box::pin(async { self.terminate_async().await })
            }
        }
    };
    ($component_name:ty) => {
        impl_veilid_component!(@inner $component_name,);
    };
    ($component_name:ty, stats) => {
        impl_veilid_component!(@inner $component_name,
            #[cfg(feature = "stats")]
            fn stats(&self) -> Option<Box<dyn VeilidComponentStats>> {
                <$component_name>::stats_impl(self)
            }
        );
    };
    ($component_name:ty, stats, reset) => {
        impl_veilid_component!(@inner $component_name,
            #[cfg(feature = "stats")]
            fn stats(&self) -> Option<Box<dyn VeilidComponentStats>> {
                <$component_name>::stats_impl(self)
            }
            #[cfg(feature = "stats")]
            fn reset_stats(&self) {
                <$component_name>::reset_stats_impl(self)
            }
        );
    };
}

pub(crate) use impl_veilid_component;

/////////////////////////////////////////////////////////////////////

// Utility macro for setting up a background TickTask
// Should be called during new/construction of a component with background tasks
// and before any post-init 'tick' operations are started
macro_rules! impl_setup_task {
    ($this:expr, $this_type:ty, $task_name:ident, $task_routine:ident ) => {{
        let registry = $this.registry();
        $this.$task_name.set_routine(move |s, l, t| {
            let registry = registry.clone();
            Box::pin(async move {
                let this = registry.lookup::<$this_type>().unwrap_or_log();
                this.$task_routine(s, Timestamp::new(l), Timestamp::new(t))
            })
        });
    }};
}

pub(crate) use impl_setup_task;

macro_rules! impl_setup_task_clone {
    ($this:expr, $task_name:ident, $task_routine:ident ) => {{
        let this = $this.clone();
        $this.$task_name.set_routine(move |s, l, t| {
            let this = this.clone();
            Box::pin(async move { this.$task_routine(s, Timestamp::new(l), Timestamp::new(t)) })
        });
    }};
}

pub(crate) use impl_setup_task_clone;

macro_rules! impl_setup_task_async {
    ($this:expr, $this_type:ty, $task_name:ident, $task_routine:ident ) => {{
        let registry = $this.registry();
        $this.$task_name.set_routine(move |s, l, t| {
            let registry = registry.clone();
            Box::pin(async move {
                let this = registry.lookup::<$this_type>().unwrap_or_log();
                this.$task_routine(s, Timestamp::new(l), Timestamp::new(t))
                    .await
            })
        });
    }};
}

pub(crate) use impl_setup_task_async;

macro_rules! impl_setup_task_async_clone {
    ($this:expr, $task_name:ident, $task_routine:ident ) => {{
        let this = $this.clone();
        $this.$task_name.set_routine(move |s, l, t| {
            let this = this.clone();
            Box::pin(async move {
                this.$task_routine(s, Timestamp::new(l), Timestamp::new(t))
                    .await
            })
        });
    }};
}

pub(crate) use impl_setup_task_async_clone;

// Utility macro for setting up an event bus handler
// Should be called after init, during post-init or later
// Subscription should be unsubscribed before termination
macro_rules! impl_subscribe_event_bus {
    ($this:expr, $this_type:ty, $event_handler:ident ) => {{
        let registry = $this.registry();
        $this.event_bus().subscribe(move |evt| {
            let registry = registry.clone();
            Box::pin(async move {
                let this = registry.lookup::<$this_type>().unwrap_or_log();
                this.$event_handler(evt);
            })
        })
    }};
}

pub(crate) use impl_subscribe_event_bus;

macro_rules! impl_subscribe_event_bus_async {
    ($this:expr, $this_type:ty, $event_handler:ident ) => {{
        let registry = $this.registry();
        $this.event_bus().subscribe(move |evt| {
            let registry = registry.clone();
            Box::pin(async move {
                let this = registry.lookup::<$this_type>().unwrap_or_log();
                this.$event_handler(evt).await;
            })
        })
    }};
}

pub(crate) use impl_subscribe_event_bus_async;

macro_rules! impl_subscribe_event_bus_async_clone {
    ($this:expr, $event_handler:ident ) => {{
        let this = $this.clone();
        $this.event_bus().subscribe(move |evt| {
            let this = this.clone();
            Box::pin(async move {
                this.$event_handler(evt).await;
            })
        })
    }};
}

pub(crate) use impl_subscribe_event_bus_async_clone;

macro_rules! impl_subscribe_event_bus_clone {
    ($this:expr, $event_handler:ident ) => {{
        let this = $this.clone();
        $this.event_bus().subscribe(move |evt| {
            let this = this.clone();
            Box::pin(async move {
                this.$event_handler(evt);
            })
        })
    }};
}

pub(crate) use impl_subscribe_event_bus_clone;
