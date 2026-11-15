mod attachment_level_calculator;
#[cfg(feature = "stats")]
mod stats;

#[cfg(any(test, feature = "test-util"))]
#[doc(hidden)]
#[allow(
    clippy::disallowed_macros,
    clippy::disallowed_methods,
    reason = "test support: printing and raw clocks are the thing under test"
)]
pub mod tests_attachment_manager;

use super::*;
use attachment_level_calculator::*;
use network_manager::StartupDisposition;
use routing_table::{BucketEntryState, RoutingDomain, RoutingDomainReadyEvent, RoutingTable};

impl_veilid_log_facility!("attach");

const TICK_INTERVAL_MSEC: u32 = 1000;
const ATTACHMENT_MAINTAINER_INTERVAL_MSEC: u32 = 1000;
/// Tick lag past this is logged and offered to the stall handlers, which apply their own floors
const TICK_LAG_REPORT: TimestampDuration =
    TimestampDuration::new_ms(2 * (TICK_INTERVAL_MSEC as u64));

type UpdateStateCallback = Box<dyn FnOnce(MutexGuard<AttachmentManagerInner>)>;

#[derive(Debug, Clone)]
pub struct AttachmentManagerStartupContext {
    pub initialize_lock: Arc<StartupLock>,
    pub attachment_lock: Arc<StartupLock>,
}
impl AttachmentManagerStartupContext {
    pub fn new() -> Self {
        Self {
            initialize_lock: Arc::new(StartupLock::new()),
            attachment_lock: Arc::new(StartupLock::new()),
        }
    }
}
impl Default for AttachmentManagerStartupContext {
    fn default() -> Self {
        Self::new()
    }
}

/// Event sent every second while veilid-core is initialized
#[derive(Debug, Copy, Clone, Eq, PartialEq, Hash)]
pub struct TickEvent {
    pub last_tick_ts: Option<Timestamp>,
    pub cur_tick_ts: Timestamp,
}

/// Event sent when the OS held the whole process: the watchdog thread woke late, or on web
/// the tick did, the only witness there
#[derive(Debug, Clone)]
pub struct ProcessStallEvent {
    /// When the stall began, by the time slept
    pub began: Timestamp,
    /// When the process ran again
    pub ended: Timestamp,
}

/// Event sent when the host application moves in or out of the background, on platforms that
/// report it
#[cfg(feature = "app-lifecycle")]
#[derive(Debug, Clone)]
pub struct AppLifecycleEvent {
    /// Where the host application now stands
    pub state: VeilidAppLifecycleState,
    /// Background time claimed on the move; a handler with state to write holds a clone until done
    pub background_time: Option<Arc<BackgroundTimeGuard>>,
}

/// The lifecycle state the node acts on: the platform's where it reports one, the host's otherwise,
/// and only the host's with detection off
#[cfg(feature = "app-lifecycle")]
pub(crate) fn effective_app_lifecycle_state(
    detected: Option<VeilidAppLifecycleState>,
    reported: Option<VeilidAppLifecycleState>,
    auto_detect_state: bool,
) -> Option<VeilidAppLifecycleState> {
    if auto_detect_state {
        detected.or(reported)
    } else {
        reported
    }
}

#[cfg(feature = "app-lifecycle")]
impl From<AppLifecycleState> for VeilidAppLifecycleState {
    fn from(state: AppLifecycleState) -> Self {
        match state {
            AppLifecycleState::Active => Self::Active,
            AppLifecycleState::Background => Self::Background,
        }
    }
}

/// A caller waiting for the attachment to satisfy a condition
struct AttachmentWaiter {
    /// The condition that ends the wait
    condition: VeilidStateAttachmentCondition,
    /// Receives the first state that satisfies the condition
    sender: flume::Sender<VeilidStateAttachment>,
}

impl AttachmentWaiter {
    /// Hands over `state` if it satisfies the condition; true once the waiter is done
    fn resolve(&self, state: &VeilidStateAttachment) -> bool {
        if self.sender.is_disconnected() {
            return true;
        }
        if !self.condition.is_satisfied_by(state) {
            return false;
        }
        // Fails only if the caller has left, which ends the wait too
        let _ = self.sender.try_send(state.clone());
        true
    }
}

struct AttachmentManagerInner {
    /// The current attachment state
    attachment_state: AttachmentState,
    /// The last VeilidStateAttachment that was calculated
    last_veilid_state_attachment: Box<VeilidStateAttachment>,
    /// The last time PublicInternet changed readiness
    last_public_internet_ready_changed_ts: Option<Timestamp>,
    /// The last time LocalNetwork changed readiness
    last_local_network_ready_changed_ts: Option<Timestamp>,
    /// Whether to maintain peers
    maintain_peers: bool,
    /// Whether the network is up; Attaching starts before it is and spans any bind wait
    network_started: bool,
    /// The timestamp when the node started
    started_ts: Timestamp,
    /// The timestamp when the node attached
    attach_ts: Option<Timestamp>,
    /// The timestamp when the node should retry binding
    bind_retry_ts: Option<Timestamp>,
    /// The timestamp of the last tick
    last_tick_ts: Option<Timestamp>,
    /// The host application's lifecycle state as last seen
    #[cfg(feature = "app-lifecycle")]
    app_lifecycle_state: Option<VeilidAppLifecycleState>,
    /// The host application's own report of its lifecycle state
    #[cfg(feature = "app-lifecycle")]
    reported_app_lifecycle_state: Option<VeilidAppLifecycleState>,
    /// The platform's reports of the host application's lifecycle, while initialized
    #[cfg(feature = "app-lifecycle")]
    app_lifecycle_monitor: Option<AppLifecycleMonitor>,
    /// Held while attached, when the config prefers the foreground
    #[cfg(feature = "app-lifecycle")]
    foreground_guard: Option<ForegroundGuard>,
    /// The future for the attachment maintainer task
    tick_future: Option<PinBoxFutureStatic<()>>,
    /// OS-thread lateness reporter, alive while the ticker is
    #[cfg(not(all(target_arch = "wasm32", target_os = "unknown")))]
    process_watchdog: Option<ProcessWatchdog>,
    /// Subscription to RoutingDomainReadyEvent
    routing_domain_ready_subscription: Option<EventBusSubscription>,
    /// Callers waiting for the attachment to satisfy a condition
    attachment_waiters: Vec<AttachmentWaiter>,
    /// Send the next attachment state even if nothing in it changed
    resend_attachment: bool,
    /// Attachment state time-in-state and transition accounting
    #[cfg(feature = "stats")]
    stats_accounting: stats::AttachmentStatsAccounting,
}

impl fmt::Debug for AttachmentManagerInner {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut out = f.debug_struct("AttachmentManagerInner");
        out.field("attachment_state", &self.attachment_state)
            .field(
                "last_veilid_state_attachment",
                &self.last_veilid_state_attachment,
            )
            .field(
                "last_public_internet_ready_changed_ts",
                &self.last_public_internet_ready_changed_ts,
            )
            .field(
                "last_local_network_ready_changed_ts",
                &self.last_local_network_ready_changed_ts,
            )
            .field("maintain_peers", &self.maintain_peers)
            .field("network_started", &self.network_started)
            .field("started_ts", &self.started_ts)
            .field("attach_ts", &self.attach_ts)
            .field("bind_retry_ts", &self.bind_retry_ts)
            .field("last_tick_ts", &self.last_tick_ts);
        #[cfg(feature = "app-lifecycle")]
        out.field("app_lifecycle_state", &self.app_lifecycle_state)
            .field(
                "reported_app_lifecycle_state",
                &self.reported_app_lifecycle_state,
            )
            .field("foreground_guard", &self.foreground_guard);
        out.finish()
    }
}

pub struct AttachmentManager {
    registry: VeilidComponentRegistry,
    inner: Mutex<AttachmentManagerInner>,
    startup_context: AttachmentManagerStartupContext,
    attachment_maintainer_task: TickTask<EyreReport>,
    level_calculator: AttachmentLevelCalculator,
}

impl fmt::Debug for AttachmentManager {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("AttachmentManager")
            // .field("registry", &self.registry)
            .field("inner", &self.inner)
            .field("startup_context", &self.startup_context)
            .field("level_calculator", &self.level_calculator)
            // .field("attachment_maintainer_task", &self.attachment_maintainer_task)
            .finish()
    }
}

impl_veilid_component!(AttachmentManager, stats);

impl AttachmentManager {
    fn new_inner() -> AttachmentManagerInner {
        AttachmentManagerInner {
            attachment_state: AttachmentState::Detached,
            last_veilid_state_attachment: Box::new(VeilidStateAttachment::default()),
            last_public_internet_ready_changed_ts: None,
            last_local_network_ready_changed_ts: None,
            maintain_peers: false,
            network_started: false,
            started_ts: Timestamp::now(),
            attach_ts: None,
            last_tick_ts: None,
            #[cfg(feature = "app-lifecycle")]
            app_lifecycle_state: None,
            #[cfg(feature = "app-lifecycle")]
            reported_app_lifecycle_state: None,
            #[cfg(feature = "app-lifecycle")]
            app_lifecycle_monitor: None,
            #[cfg(feature = "app-lifecycle")]
            foreground_guard: None,
            bind_retry_ts: None,
            tick_future: None,
            #[cfg(not(all(target_arch = "wasm32", target_os = "unknown")))]
            process_watchdog: None,
            routing_domain_ready_subscription: None,
            attachment_waiters: Vec::new(),
            resend_attachment: false,
            #[cfg(feature = "stats")]
            stats_accounting: stats::AttachmentStatsAccounting::default(),
        }
    }
    pub fn new(
        registry: VeilidComponentRegistry,
        startup_context: AttachmentManagerStartupContext,
    ) -> Self {
        Self {
            level_calculator: AttachmentLevelCalculator::new(registry.clone()),
            registry,
            inner: Mutex::new(Self::new_inner()),
            startup_context,
            attachment_maintainer_task: TickTask::new_ms(
                "attachment_maintainer_task",
                ATTACHMENT_MAINTAINER_INTERVAL_MSEC,
            ),
        }
    }

    #[cfg_attr(not(feature = "debug-api"), expect(dead_code))]
    pub fn is_attached(&self) -> bool {
        self.inner.lock().attachment_state.is_attached()
    }

    #[allow(dead_code)]
    pub fn is_detached(&self) -> bool {
        self.inner.lock().attachment_state.is_detached()
    }

    #[allow(dead_code)]
    pub fn get_attach_timestamp(&self) -> Option<Timestamp> {
        self.inner.lock().attach_ts
    }

    fn log_facilities_impl(&self) -> VeilidComponentLogFacilities {
        VeilidComponentLogFacilities::new()
            .with_facility(VeilidComponentLogFacility::try_new("attach").unwrap())
    }

    #[cfg_attr(feature = "instrument", instrument(level = "debug", skip_all, err, fields(__VEILID_LOG_KEY = self.log_key())))]
    #[allow(clippy::unused_async)]
    pub async fn init_async(&self) -> EyreResult<()> {
        let guard = self.startup_context.initialize_lock.startup()?;
        guard.success();
        Ok(())
    }

    #[cfg_attr(feature = "instrument", instrument(level = "debug", skip_all, err, fields(__VEILID_LOG_KEY = self.log_key())))]
    #[allow(clippy::unused_async)]
    pub async fn post_init_async(&self) -> EyreResult<()> {
        let registry = self.registry();

        veilid_log!(self debug "starting attachment maintainer task");
        impl_setup_task_async!(
            self,
            Self,
            attachment_maintainer_task,
            attachment_maintainer_task_routine
        );

        let routing_domain_ready_subscription =
            impl_subscribe_event_bus!(self, Self, routing_domain_ready_event_handler);

        #[cfg(feature = "app-lifecycle")]
        let app_lifecycle_monitor = {
            let registry = self.registry();
            AppLifecycleMonitor::new(move || {
                // Acted on as reported, not at the next tick: a page can be frozen before then
                if let Some(this) = registry.lookup::<AttachmentManager>() {
                    this.post_app_lifecycle_change();
                }
            })
        };

        let mut inner = self.inner.lock();
        inner.routing_domain_ready_subscription = Some(routing_domain_ready_subscription);
        #[cfg(feature = "app-lifecycle")]
        {
            inner.app_lifecycle_monitor = Some(app_lifecycle_monitor);
        }

        // Let the attachment maintainer run
        let guard = self.startup_context.attachment_lock.startup()?;
        guard.success();

        // Create top level tick interval
        let tick_future = interval(
            "attachment maintainer tick",
            TICK_INTERVAL_MSEC,
            true,
            move || {
                let registry = registry.clone();
                async move {
                    let this = registry.attachment_manager();
                    if let Err(e) = this.tick().await {
                        veilid_log!(this warn "attachment maintainer tick failed: {}", e);
                    }
                }
            },
        );

        // Keep ticker, drop this to stop the ticker
        inner.tick_future = Some(tick_future);

        #[cfg(not(all(target_arch = "wasm32", target_os = "unknown")))]
        {
            let registry = self.registry();
            inner.process_watchdog = Some(ProcessWatchdog::start(
                TICK_INTERVAL_MSEC as u64,
                TICK_LAG_REPORT.as_u64() / 1000,
                move |late_us, now_us| {
                    let this = registry.attachment_manager();
                    veilid_log!(this debug "process watchdog: OS thread late by {} (rust ts {:#})",
                        TimestampDuration::new(late_us), Timestamp::new(now_us));
                    // First to run after a freeze, so the stall is known before its envelopes
                    let slept_us = late_us + (TICK_INTERVAL_MSEC as u64) * 1000;
                    let began = Timestamp::new(now_us.saturating_sub(slept_us));
                    let ended = Timestamp::new(now_us);
                    registry
                        .network_manager()
                        .address_filter()
                        .record_stall(began, ended);
                    if let Err(e) = registry
                        .event_bus()
                        .post(ProcessStallEvent { began, ended })
                    {
                        veilid_log!(this warn "failed to post ProcessStallEvent: {}", e);
                    }
                },
            ));
        }

        Ok(())
    }

    fn routing_domain_ready_event_handler(&self, _evt: Arc<RoutingDomainReadyEvent>) {
        let inner = self.inner.lock();
        if !inner.network_started
            || !matches!(
                inner.attachment_state,
                AttachmentState::AttachedWeak
                    | AttachmentState::AttachedFair
                    | AttachmentState::AttachedGood
                    | AttachmentState::AttachedStrong
                    | AttachmentState::AttachedFull
                    | AttachmentState::Attaching
            )
        {
            return;
        }
        self.update_attached_state(inner, None);
    }

    /// Close the attachment lock, then run the maintainer until it detaches. It detaches when it
    /// finds the lock closed, and a run still winding down from before the close holds off a new
    /// one without detaching, so each run is awaited.
    #[cfg_attr(feature = "instrument", instrument(level = "debug", skip_all, fields(__VEILID_LOG_KEY = self.log_key())))]
    pub async fn pre_terminate_async(&self) {
        // Start attachment shutdown
        let guard = self
            .startup_context
            .attachment_lock
            .shutdown()
            .await
            .expect_or_log("should be initialized");

        // Kick the maintainer rather than waiting out a tick
        while !matches!(
            self.inner.lock().attachment_state,
            AttachmentState::Detached
        ) {
            if let Err(e) = self.attachment_maintainer_task.try_tick_now().await {
                veilid_log!(self warn "attachment_maintainer not ticked: {}", e);
                break;
            }
            if let Err(e) = self.attachment_maintainer_task.join().await {
                veilid_log!(self warn "attachment_maintainer run failed: {}", e);
                break;
            }
        }

        let sub = self.inner.lock().routing_domain_ready_subscription.take();
        if let Some(sub) = sub {
            self.event_bus().unsubscribe(sub);
        }

        // Stop watchdog and ticker
        #[cfg(not(all(target_arch = "wasm32", target_os = "unknown")))]
        drop(self.inner.lock().process_watchdog.take());
        let tick_future = self.inner.lock().tick_future.take();
        if let Some(tick_future) = tick_future {
            tick_future.await;
        }
        // Detached now, and no tick is left to let the foreground go
        #[cfg(feature = "app-lifecycle")]
        drop(self.inner.lock().foreground_guard.take());
        // Its callback holds the registry, so it goes before the registry can
        #[cfg(feature = "app-lifecycle")]
        {
            let app_lifecycle_monitor = self.inner.lock().app_lifecycle_monitor.take();
            drop(app_lifecycle_monitor);
        }

        // Stop background operations
        veilid_log!(self debug "stopping attachment maintainer task");
        if let Err(e) = self.attachment_maintainer_task.stop().await {
            veilid_log!(self warn "attachment_maintainer not stopped: {}", e);
        }

        // Attachent shutdown successful
        guard.success();
    }

    #[cfg_attr(feature = "instrument", instrument(level = "debug", skip_all, fields(__VEILID_LOG_KEY = self.log_key())))]
    pub async fn terminate_async(&self) {
        let guard = self
            .startup_context
            .initialize_lock
            .shutdown()
            .await
            .expect_or_log("should be initialized");

        // Shutdown successful
        guard.success();
    }

    /// Moves to Attaching, which lasts through any bind wait until the node attaches or detaches
    #[cfg_attr(feature = "instrument", instrument(level = "trace", skip_all, fields(__VEILID_LOG_KEY = self.log_key())))]
    pub async fn attach(&self) -> VeilidAPIResult<()> {
        let Ok(_guard) = self.startup_context.initialize_lock.enter() else {
            apibail_not_initialized!();
        };

        let changed = {
            let mut inner = self.inner.lock();
            // Attaching is disabled while terminating
            if !self.startup_context.attachment_lock.is_started() {
                apibail_not_initialized!();
            }
            if inner.attachment_state.is_attached() {
                apibail_generic!("Already attached");
            }
            let maintain_peers_changed = !inner.maintain_peers;
            inner.maintain_peers = true;
            // A bind waiting on its retry period tries again now
            let bind_retry_waiting = inner.bind_retry_ts.take().is_some();
            if inner.attachment_state.is_detached() {
                self.transition_to_non_attached_state(inner, AttachmentState::Attaching, None);
            }
            maintain_peers_changed || bind_retry_waiting
        };

        // Act on the request now rather than at the next tick
        if changed {
            if let Err(e) = self.attachment_maintainer_task.try_tick_now().await {
                veilid_log!(self warn "attachment_maintainer not ticked: {}", e);
            }
        }

        Ok(())
    }

    /// Moves to Detaching until the network is down, cancelling an attach in progress
    #[cfg_attr(feature = "instrument", instrument(level = "trace", skip_all, fields(__VEILID_LOG_KEY = self.log_key())))]
    pub async fn detach(&self) -> VeilidAPIResult<()> {
        let Ok(_guard) = self.startup_context.initialize_lock.enter() else {
            apibail_not_initialized!();
        };

        {
            let mut inner = self.inner.lock();
            if inner.attachment_state.is_detached() {
                apibail_generic!("Not attached");
            }
            inner.maintain_peers = false;
            inner.bind_retry_ts = None;
            if inner.attachment_state != AttachmentState::Detaching {
                self.transition_to_non_attached_state(inner, AttachmentState::Detaching, None);
            }
        }

        // Act on the state change now rather than at the next tick
        if let Err(e) = self.attachment_maintainer_task.try_tick_now().await {
            veilid_log!(self warn "attachment_maintainer not ticked: {}", e);
        }

        Ok(())
    }

    /////////////////////////////////////////////////////////////////////////////

    async fn tick(&self) -> EyreResult<()> {
        let cur_tick_ts = Timestamp::now_non_decreasing();
        let last_tick_ts = {
            let mut inner = self.inner.lock();
            let last_tick_ts = inner.last_tick_ts;
            inner.last_tick_ts = Some(cur_tick_ts);
            last_tick_ts
        };

        // Log if we're seeing missed ticks
        if let Some(last_tick_ts) = last_tick_ts {
            let lag = cur_tick_ts.duration_since(last_tick_ts);
            if lag > TICK_LAG_REPORT {
                veilid_log!(self debug "tick lag: {}{}", lag, executor_snapshot_suffix());
                // A stalled executor stalls every in-flight RPC; their timeouts are about us
                let stalled = self
                    .network_manager()
                    .online_detector()
                    .record_local_stall(lag);
                // No watchdog thread here, so the tick is the only witness to a frozen tab
                #[cfg(all(target_arch = "wasm32", target_os = "unknown"))]
                {
                    self.network_manager()
                        .address_filter()
                        .record_stall(last_tick_ts, cur_tick_ts);
                    self.event_bus().post(ProcessStallEvent {
                        began: last_tick_ts,
                        ended: cur_tick_ts,
                    })?;
                }
                // An app in the background hears on its return instead
                if stalled && !self.in_background() {
                    self.resend_veilid_state_attachment();
                }
            }
        }

        // Tick our own ticktask for the attachment maintainer state machine
        self.attachment_maintainer_task.tick().await?;

        #[cfg(feature = "app-lifecycle")]
        self.update_foreground_preference();

        // Ahead of the tick event, so a background save runs on this tick
        #[cfg(feature = "app-lifecycle")]
        self.post_app_lifecycle_change();

        // Send a 'tick' event for the rest of the system to get ticks
        self.event_bus().post(TickEvent {
            last_tick_ts,
            cur_tick_ts,
        })?;

        Ok(())
    }

    /// Holds the foreground while attached, when the config prefers it
    #[cfg(feature = "app-lifecycle")]
    fn update_foreground_preference(&self) {
        let prefer_foreground = self.config().attachment.app_lifecycle.prefer_foreground;
        let mut inner = self.inner.lock();
        let wanted = prefer_foreground && !inner.attachment_state.is_detached();
        if wanted == inner.foreground_guard.is_some() {
            return;
        }
        inner.foreground_guard = if wanted {
            ForegroundGuard::begin("Veilid node attached")
        } else {
            None
        };
        if inner.foreground_guard.is_some() == wanted {
            veilid_log!(self debug "foreground preferred while attached: {}", wanted);
        }
    }

    /// Takes the host application's report of its lifecycle state
    #[cfg(feature = "app-lifecycle")]
    pub fn report_app_lifecycle_state(&self, state: VeilidAppLifecycleState) {
        self.inner.lock().reported_app_lifecycle_state = Some(state);
        self.post_app_lifecycle_change();
    }

    #[cfg(feature = "app-lifecycle")]
    fn post_app_lifecycle_change(&self) {
        let Some(evt) = self.app_lifecycle_change() else {
            return;
        };
        if let Err(e) = self.event_bus().post(evt) {
            veilid_log!(self debug "app lifecycle event not posted: {}", e);
        }
    }

    /// The host application's lifecycle, when it changed or brought background time to hold
    #[cfg(feature = "app-lifecycle")]
    fn app_lifecycle_change(&self) -> Option<AppLifecycleEvent> {
        let auto_detect_state = self.config().attachment.app_lifecycle.auto_detect_state;
        let mut inner = self.inner.lock();
        let monitor = inner.app_lifecycle_monitor.as_ref()?;
        let detected = monitor.state().map(VeilidAppLifecycleState::from);
        let state = effective_app_lifecycle_state(
            detected,
            inner.reported_app_lifecycle_state,
            auto_detect_state,
        )?;
        // A return to the background between two ticks brings time without a change
        let background_time = (state == VeilidAppLifecycleState::Background)
            .then(|| monitor.take_background_entry_time())
            .flatten();
        let previous = inner.app_lifecycle_state.replace(state);
        let changed = previous != Some(state);
        // Back from the background, the app hears what it missed with the next update
        let returned = previous == Some(VeilidAppLifecycleState::Background)
            && state == VeilidAppLifecycleState::Active;
        if returned {
            inner.resend_attachment = true;
            // Back in the foreground, a bind waiting on its retry period tries at the next pass
            inner.bind_retry_ts = None;
        }
        drop(inner);
        if !changed && background_time.is_none() {
            return None;
        }
        if changed {
            veilid_log!(self info "app lifecycle: {:?}", state);
        }
        if returned {
            veilid_log!(self debug "resending attachment state on return from the background");
        }
        Some(AppLifecycleEvent {
            state,
            background_time: background_time.map(Arc::new),
        })
    }

    // Manage attachment state
    #[cfg_attr(
        feature = "instrument",
        instrument(level = "trace", target = "stor", skip_all, err, fields(__VEILID_LOG_KEY = self.log_key()))
    )]
    async fn attachment_maintainer_task_routine(
        &self,
        _stop_token: StopToken,
        _last_ts: Timestamp,
        cur_ts: Timestamp,
    ) -> EyreResult<()> {
        // Enter the attachment lock and return if we are shutting down
        let Ok(_guard) = self.startup_context.attachment_lock.enter() else {
            // Only shutdown is possible if we get here
            let do_shutdown = {
                let inner = self.inner.lock();
                if inner.attachment_state.is_detached() {
                    false
                } else if inner.network_started {
                    veilid_log!(self debug "terminating attachment maintainer task");
                    self.transition_to_non_attached_state(inner, AttachmentState::Detaching, None);
                    true
                } else {
                    // Attaching without a network, so there is nothing to shut down
                    self.transition_to_non_attached_state(inner, AttachmentState::Detached, None);
                    false
                }
            };
            if do_shutdown {
                self.shutdown().await;
                let mut inner = self.inner.lock();
                inner.network_started = false;
                self.transition_to_non_attached_state(inner, AttachmentState::Detached, None);
            }

            return Ok(());
        };

        // Process the attachment state machine
        loop {
            let mut do_startup = false;
            let mut do_shutdown = false;
            let mut continue_loop = false;

            // Process the current state
            {
                let mut inner = self.inner.lock();

                let state = inner.attachment_state;
                let maintain_peers = inner.maintain_peers;
                let bind_retry_waiting =
                    if inner.bind_retry_ts.is_some() && inner.bind_retry_ts.unwrap() > cur_ts {
                        true
                    } else {
                        inner.bind_retry_ts = None;
                        false
                    };

                match state {
                    AttachmentState::Detached => {
                        if maintain_peers {
                            self.transition_to_non_attached_state(
                                inner,
                                AttachmentState::Attaching,
                                None,
                            );
                            continue_loop = true;
                        }
                    }
                    // The network is not up yet: starting it, or waiting to retry a bind
                    AttachmentState::Attaching if !inner.network_started => {
                        if !maintain_peers {
                            self.transition_to_non_attached_state(
                                inner,
                                AttachmentState::Detaching,
                                None,
                            );
                            continue_loop = true;
                        } else if !bind_retry_waiting {
                            veilid_log!(self debug "attachment starting");
                            do_startup = true;
                        }
                    }
                    AttachmentState::Attaching
                    | AttachmentState::AttachedWeak
                    | AttachmentState::AttachedFair
                    | AttachmentState::AttachedGood
                    | AttachmentState::AttachedStrong
                    | AttachmentState::AttachedFull => {
                        if maintain_peers {
                            let network_manager = self.network_manager();
                            if network_manager.network_needs_restart() {
                                veilid_log!(self info "Restarting network");
                                self.transition_to_non_attached_state(
                                    inner,
                                    AttachmentState::Detaching,
                                    None,
                                );
                                continue_loop = true;
                            } else {
                                self.update_attached_state(inner, None);
                            }
                        } else {
                            veilid_log!(self debug "stopped maintaining peers");
                            self.transition_to_non_attached_state(
                                inner,
                                AttachmentState::Detaching,
                                None,
                            );
                            continue_loop = true;
                        }
                    }
                    AttachmentState::Detaching => {
                        veilid_log!(self debug "shutting down attachment");
                        do_shutdown = true;
                    }
                }
            };

            if do_startup {
                match self.startup().await {
                    Err(err) => {
                        error!("attachment startup failed: {}", err);
                    }
                    Ok(StartupDisposition::BindRetry) => {
                        veilid_log!(self info "waiting for network to bind...");
                        let bind_retry =
                            TimestampDuration::new_ms(self.config().network.bind_retry_ms.into());
                        self.inner.lock().bind_retry_ts =
                            Some(Timestamp::now_non_decreasing().later(bind_retry));
                    }
                    Ok(StartupDisposition::Success) => {
                        veilid_log!(self debug "started maintaining peers");

                        let now = Timestamp::now_non_decreasing();
                        let mut inner = self.inner.lock();
                        inner.network_started = true;
                        // Readiness is timed from the network start, not from the attach request
                        inner.last_public_internet_ready_changed_ts = Some(now);
                        inner.last_local_network_ready_changed_ts = Some(now);
                    }
                }
                // A detach that arrived during startup is acted on now
                continue_loop = self.inner.lock().attachment_state == AttachmentState::Detaching;
            } else if do_shutdown {
                let network_started = self.inner.lock().network_started;
                if network_started {
                    self.shutdown().await;
                }

                let mut inner = self.inner.lock();
                inner.network_started = false;
                // Attach was asked for again while detaching, or the network is restarting
                if inner.maintain_peers {
                    self.transition_to_non_attached_state(inner, AttachmentState::Attaching, None);
                    continue_loop = true;
                } else {
                    self.transition_to_non_attached_state(inner, AttachmentState::Detached, None);
                }
            }

            // Loop again if we should process the next state immediately
            if !continue_loop {
                break;
            }
        }

        Ok(())
    }

    async fn startup(&self) -> EyreResult<StartupDisposition> {
        veilid_log!(self info "Attaching...");

        let rpc_processor = self.rpc_processor();
        let network_manager = self.network_manager();

        // Startup network manager
        let res = network_manager.startup().await?;
        match res {
            StartupDisposition::Success => {
                veilid_log!(self debug "NetworkManager startup success");
            }
            StartupDisposition::BindRetry => {
                veilid_log!(self debug "NetworkManager bind retry");
                return Ok(StartupDisposition::BindRetry);
            }
        }

        // Startup rpc processor
        if let Err(e) = rpc_processor.startup() {
            network_manager.shutdown().await;
            return Err(e);
        }

        // Startup routing table
        let routing_table = self.routing_table();
        if let Err(e) = routing_table.startup().await {
            rpc_processor.shutdown().await;
            network_manager.shutdown().await;
            return Err(e);
        }

        // Inform api clients that things have changed
        network_manager.send_network_update();

        veilid_log!(self info "Attach successful");

        Ok(StartupDisposition::Success)
    }

    async fn shutdown(&self) {
        veilid_log!(self info "Detaching...");

        let routing_table = self.routing_table();
        let rpc_processor = self.rpc_processor();
        let network_manager = self.network_manager();

        // Shutdown RoutingTable
        routing_table.shutdown().await;

        // Shutdown NetworkManager
        network_manager.shutdown().await;

        // Shutdown RPCProcessor
        rpc_processor.shutdown().await;

        // Send update
        network_manager.send_network_update();

        veilid_log!(self info "Detach successful");
    }

    /// Update attachment and network readiness state
    /// and possibly send a VeilidUpdate::Attachment.
    fn update_attached_state(
        &self,
        mut inner: MutexGuard<AttachmentManagerInner>,
        opt_callback: Option<UpdateStateCallback>,
    ) {
        let routing_table = self.network_manager().routing_table();
        let health = routing_table.get_routing_table_health();

        let now = Timestamp::now_non_decreasing();
        let uptime = now.duration_since(inner.started_ts);
        let attached_uptime = inner.attach_ts.map(|ts| now.duration_since(ts));

        let new_state = self.level_calculator.recompute();
        let inputs = self.level_calculator.last_inputs();
        inner.attachment_state = new_state;
        veilid_stats!(inner.stats_accounting.record_state(now, new_state));

        let veilid_state_attachment = Box::new(VeilidStateAttachment {
            state: new_state,
            public_internet_ready: health.routing_domain_ready(RoutingDomain::PublicInternet),
            local_network_ready: health.routing_domain_ready(RoutingDomain::LocalNetwork),
            uptime,
            attached_uptime,
            reliable_peer_count: NodeCount::from(inputs.reliable_count as u64),
            live_peer_count: NodeCount::from(inputs.live_count as u64),
            estimated_network_size: NodeCount::from(inputs.estimated_network_size),
            median_latency: inputs.median_latency,
            over_attached_nodes: NodeCount::from(inputs.excess_kickable as u64),
        });

        self.update_veilid_state_attachment(inner, veilid_state_attachment, opt_callback);
    }

    fn transition_to_non_attached_state(
        &self,
        mut inner: MutexGuard<AttachmentManagerInner>,
        state: AttachmentState,
        opt_callback: Option<UpdateStateCallback>,
    ) {
        // Update timestamps
        let now = Timestamp::now_non_decreasing();
        match state {
            AttachmentState::Attaching => {
                // Republishing the current state keeps its start
                if inner.attachment_state != AttachmentState::Attaching {
                    inner.attach_ts = Some(now);
                }
            }
            AttachmentState::Detached => {
                inner.attach_ts = None;
                inner.last_public_internet_ready_changed_ts = None;
                inner.last_local_network_ready_changed_ts = None;
            }
            AttachmentState::Detaching => {
                // ok
            }
            AttachmentState::AttachedWeak
            | AttachmentState::AttachedFair
            | AttachmentState::AttachedGood
            | AttachmentState::AttachedStrong
            | AttachmentState::AttachedFull => {
                veilid_log!(self error "don't use this for attached states, use update_attached_state()");
                return;
            }
        }
        let uptime = now.duration_since(inner.started_ts);
        let attached_uptime = inner.attach_ts.map(|ts| now.duration_since(ts));

        // Update attachment state
        inner.attachment_state = state;
        veilid_stats!(inner.stats_accounting.record_state(now, state));
        let veilid_state_attachment = Box::new(VeilidStateAttachment {
            state,
            public_internet_ready: false,
            local_network_ready: false,
            uptime,
            attached_uptime,
            reliable_peer_count: NodeCount::from(0),
            live_peer_count: NodeCount::from(0),
            estimated_network_size: NodeCount::from(0),
            median_latency: None,
            over_attached_nodes: NodeCount::from(0),
        });
        self.update_veilid_state_attachment(inner, veilid_state_attachment, opt_callback);
    }

    fn update_veilid_state_attachment(
        &self,
        mut inner: MutexGuard<AttachmentManagerInner>,
        veilid_state_attachment: Box<VeilidStateAttachment>,
        opt_callback: Option<UpdateStateCallback>,
    ) {
        // Print PublicInternet readiness change
        let mut public_internet_ready_changed = false;
        if inner.last_veilid_state_attachment.public_internet_ready
            != veilid_state_attachment.public_internet_ready
        {
            let cur_ts = Timestamp::now_non_decreasing();
            if let Some(last_changed_ts) = inner.last_public_internet_ready_changed_ts {
                let dur = cur_ts.duration_since(last_changed_ts);
                veilid_log!(self info "PublicInternet {} in {:#}", if veilid_state_attachment.public_internet_ready { "ready" } else { "unavailable" }, dur);
            }
            inner.last_public_internet_ready_changed_ts = Some(cur_ts);
            public_internet_ready_changed = true;
        }

        // Print LocalNetwork readiness change
        let mut local_network_ready_changed = false;
        if inner.last_veilid_state_attachment.local_network_ready
            != veilid_state_attachment.local_network_ready
        {
            let cur_ts = Timestamp::now_non_decreasing();
            if let Some(last_changed_ts) = inner.last_local_network_ready_changed_ts {
                let dur = cur_ts.duration_since(last_changed_ts);
                veilid_log!(self info "LocalNetwork {} in {:#}", if veilid_state_attachment.local_network_ready { "ready" } else { "unavailable" }, dur);
            }
            inner.last_local_network_ready_changed_ts = Some(cur_ts);
            local_network_ready_changed = true;
        }

        // Send update if one of:
        // * a resend was asked for
        // * the attachment state has changed
        // * routing domain readiness has changed
        let resend = std::mem::take(&mut inner.resend_attachment);
        let send_update = resend
            || inner.last_veilid_state_attachment.state != veilid_state_attachment.state
            || public_internet_ready_changed
            || local_network_ready_changed;

        let opt_update_to_send = if send_update {
            Some(VeilidUpdate::Attachment(veilid_state_attachment.clone()))
        } else {
            None
        };

        inner.last_veilid_state_attachment = veilid_state_attachment;

        // Under the lock that stored the state, so no waiter misses a change
        let inner_mut = &mut *inner;
        let state = &inner_mut.last_veilid_state_attachment;
        inner_mut
            .attachment_waiters
            .retain(|waiter| !waiter.resolve(state));

        if let Some(callback) = opt_callback {
            callback(inner);
        } else {
            drop(inner);
        }

        // Send update
        if let Some(update_to_send) = opt_update_to_send {
            (self.update_callback())(update_to_send);
        }
    }

    /// Recompute the attachment state now, possibly sending an update, then run `callback` under
    /// the lock that stored it
    fn refresh_veilid_state(&self, callback: UpdateStateCallback) {
        let inner = self.inner.lock();
        let current_attachment_state = inner.attachment_state;
        match current_attachment_state {
            // Nothing to measure before the network is up
            AttachmentState::Attaching if !inner.network_started => {
                self.transition_to_non_attached_state(
                    inner,
                    AttachmentState::Attaching,
                    Some(callback),
                );
            }
            AttachmentState::Attaching
            | AttachmentState::AttachedWeak
            | AttachmentState::AttachedFair
            | AttachmentState::AttachedGood
            | AttachmentState::AttachedStrong
            | AttachmentState::AttachedFull => {
                self.update_attached_state(inner, Some(callback));
            }
            AttachmentState::Detached | AttachmentState::Detaching => {
                // Transition to the current state just returns the current state
                self.transition_to_non_attached_state(
                    inner,
                    current_attachment_state,
                    Some(callback),
                )
            }
        }
    }

    /// Whether the host application is in the background, as last seen
    fn in_background(&self) -> bool {
        cfg_if! {
            if #[cfg(feature = "app-lifecycle")] {
                self.inner.lock().app_lifecycle_state == Some(VeilidAppLifecycleState::Background)
            } else {
                false
            }
        }
    }

    /// Sends the attachment state even if unchanged: after a stall an app has heard nothing
    fn resend_veilid_state_attachment(&self) {
        veilid_log!(self debug "resending attachment state after a stall");
        self.inner.lock().resend_attachment = true;
        self.refresh_veilid_state(Box::new(|_| {}));
    }

    pub fn get_veilid_state(&self) -> Box<VeilidStateAttachment> {
        let out_state = Arc::new(Mutex::new(None));
        let out_state_clone = out_state.clone();
        self.refresh_veilid_state(Box::new(move |inner| {
            *out_state_clone.lock() = Some(inner.last_veilid_state_attachment.clone())
        }));

        // Return the last attachment state
        let out = { out_state.lock().take() };
        out.unwrap_or_default()
    }

    /// Wait until the attachment satisfies `condition`, returning the state that did
    pub async fn wait_for_condition(
        &self,
        condition: VeilidStateAttachmentCondition,
        timeout: Option<Duration>,
    ) -> VeilidAPIResult<VeilidStateAttachment> {
        let Ok(_guard) = self.startup_context.initialize_lock.enter() else {
            apibail_not_initialized!();
        };
        let Some(shutdown_token) = self.startup_context.initialize_lock.stop_token() else {
            apibail_not_initialized!();
        };

        let (sender, receiver) = flume::bounded(1);
        let waiter = AttachmentWaiter { condition, sender };
        self.refresh_veilid_state(Box::new(move |mut inner| {
            if !waiter.resolve(&inner.last_veilid_state_attachment) {
                inner.attachment_waiters.push(waiter);
            }
        }));

        let received = receiver.into_recv_async().timeout_at(shutdown_token);
        let result = match timeout {
            Some(timeout) => {
                let timeout_ms = u32::try_from(timeout.as_millis()).unwrap_or(u32::MAX);
                crate::timeout(timeout_ms, received).await
            }
            None => Ok(received.await),
        };

        // A caller that stopped waiting leaves a waiter nobody receives from
        self.inner
            .lock()
            .attachment_waiters
            .retain(|waiter| !waiter.sender.is_disconnected());

        match result {
            Ok(Ok(Ok(state))) => Ok(state),
            Err(_) => apibail_timeout!(),
            Ok(Err(_)) | Ok(Ok(Err(_))) => apibail_not_initialized!(),
        }
    }

    #[cfg(feature = "app-lifecycle")]
    #[cfg_attr(not(feature = "debug-api"), allow(dead_code))]
    pub fn debug_info_lifecycle(&self) -> String {
        let auto_detect_state = self.config().attachment.app_lifecycle.auto_detect_state;
        let inner = self.inner.lock();
        let detected = inner
            .app_lifecycle_monitor
            .as_ref()
            .and_then(|monitor| monitor.state())
            .map(VeilidAppLifecycleState::from);
        let show = |state: Option<VeilidAppLifecycleState>| {
            state.map_or_else(|| "-".to_owned(), |state| format!("{:?}", state))
        };
        format!(
            "Lifecycle...\n  state: {}\n  detected: {}\n  reported: {}\n  auto_detect_state: {}\n",
            show(inner.app_lifecycle_state),
            show(detected),
            show(inner.reported_app_lifecycle_state),
            auto_detect_state
        )
    }

    #[cfg_attr(not(feature = "debug-api"), expect(dead_code))]
    pub fn debug_info_nodeinfo(&self) -> String {
        let mut out = String::new();
        out += &format!(
            "VeilidStateAttachment:\n{}\n",
            indent_all_string(format!("{:#}", self.get_veilid_state()))
        );
        out
    }
}
