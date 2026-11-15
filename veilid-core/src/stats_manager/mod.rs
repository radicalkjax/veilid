mod types;

pub use types::*;

use super::*;
use crate::attachment_manager::TickEvent;

impl_veilid_log_facility!("stats");

/// One stats record: timestamps plus each component's stats keyed by name.
#[derive(Serialize)]
struct VeilidStats {
    timestamp: Timestamp,
    since_ts: Timestamp,
    components: serde_json::Value,
}

//////////////////////////////////////////////////////////////////////
// Process-global per-log-key config, mirroring ApiTracingLayer.
// The "" key is the default applied to all instances.

struct StatsManagerGlobalInner {
    settings_by_log_key: HashMap<String, StatsSettings>,
}

static STATS_GLOBALS: Mutex<Option<StatsManagerGlobalInner>> = Mutex::new(None);

//////////////////////////////////////////////////////////////////////

struct StatsManagerInner {
    tick_subscription: Option<EventBusSubscription>,
    since_ts: Timestamp,
    last_emit_ts: Option<Timestamp>,
    #[cfg(not(all(target_arch = "wasm32", target_os = "unknown")))]
    output_file: Option<(std::path::PathBuf, std::fs::File)>,
}

/// Per-VeilidAPI engine that samples subsystem metrics and emits periodic stats records.
///
/// The static [StatsManager::init] is the configuration surface: call it before or after startup
/// to enable periodic logging for a log key (program_name+namespace), or `None` for the default
/// applied to every instance.
pub struct StatsManager {
    registry: VeilidComponentRegistry,
    inner: Mutex<StatsManagerInner>,
}

impl fmt::Debug for StatsManager {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("StatsManager").finish()
    }
}

impl_veilid_component!(StatsManager);

impl StatsManager {
    //////////////////////////////////////////////////////////////////
    // Process-global configuration surface (external to VeilidAPI)

    /// Configure stats logging for a log key; `None` sets the default applied to every instance.
    pub fn init(opt_log_key: Option<VeilidLogKey>, settings: StatsSettings) {
        let key = opt_log_key.unwrap_or("").to_owned();
        let mut g = STATS_GLOBALS.lock();
        if g.is_none() {
            *g = Some(StatsManagerGlobalInner {
                settings_by_log_key: HashMap::new(),
            });
        }
        g.as_mut()
            .unwrap_or_log()
            .settings_by_log_key
            .insert(key, settings);
    }

    /// Remove stats logging configuration for a log key; `None` removes the default.
    pub fn uninit(opt_log_key: Option<VeilidLogKey>) {
        let key = opt_log_key.unwrap_or("").to_owned();
        let mut g = STATS_GLOBALS.lock();
        if let Some(inner) = g.as_mut() {
            inner.settings_by_log_key.remove(&key);
            if inner.settings_by_log_key.is_empty() {
                *g = None;
            }
        }
    }

    fn get_settings(log_key: VeilidLogKey) -> StatsSettings {
        let g = STATS_GLOBALS.lock();
        if let Some(inner) = g.as_ref() {
            if let Some(s) = inner.settings_by_log_key.get(log_key) {
                return s.clone();
            }
            if let Some(s) = inner.settings_by_log_key.get("") {
                return s.clone();
            }
        }
        StatsSettings::default()
    }

    //////////////////////////////////////////////////////////////////
    // Per-VeilidAPI engine (component)

    pub(crate) fn new(registry: VeilidComponentRegistry) -> Self {
        Self {
            registry,
            inner: Mutex::new(StatsManagerInner {
                tick_subscription: None,
                since_ts: Timestamp::now(),
                last_emit_ts: None,
                #[cfg(not(all(target_arch = "wasm32", target_os = "unknown")))]
                output_file: None,
            }),
        }
    }

    fn log_facilities_impl(&self) -> VeilidComponentLogFacilities {
        VeilidComponentLogFacilities::new().with_facility(
            VeilidComponentLogFacility::try_new_with_tags("stats", ["#common"]).unwrap(),
        )
    }

    #[allow(clippy::unused_async)]
    pub(crate) async fn init_async(&self) -> EyreResult<()> {
        Ok(())
    }

    #[allow(clippy::unused_async)]
    pub(crate) async fn post_init_async(&self) -> EyreResult<()> {
        let sub = impl_subscribe_event_bus!(self, Self, tick_event_handler);
        self.inner.lock().tick_subscription = Some(sub);
        Ok(())
    }

    #[allow(clippy::unused_async)]
    pub(crate) async fn pre_terminate_async(&self) {
        let sub = self.inner.lock().tick_subscription.take();
        if let Some(sub) = sub {
            self.event_bus().unsubscribe(sub);
        }
    }

    #[allow(clippy::unused_async)]
    pub(crate) async fn terminate_async(&self) {}

    fn tick_event_handler(&self, evt: Arc<TickEvent>) {
        let settings = Self::get_settings(self.log_key());
        if !settings.enabled {
            return;
        }
        let cur_ts = evt.cur_tick_ts;
        {
            let inner = self.inner.lock();
            if let Some(last) = inner.last_emit_ts {
                let interval_us = (settings.interval_secs as u64).saturating_mul(1_000_000u64);
                if cur_ts.duration_since(last).as_u64() < interval_us {
                    return;
                }
            }
        }
        let stats = self.snapshot();
        self.emit(&settings, &stats);
        self.inner.lock().last_emit_ts = Some(cur_ts);
    }

    fn snapshot(&self) -> VeilidStats {
        let since_ts = self.inner.lock().since_ts;
        VeilidStats {
            timestamp: Timestamp::now(),
            since_ts,
            components: self.registry().component_stats(),
        }
    }

    /// Start a fresh collection window and zero the cumulative counters.
    #[cfg(feature = "debug-api")]
    pub(crate) fn reset(&self) {
        self.registry().reset_component_stats();
        self.inner.lock().since_ts = Timestamp::now();
    }

    /// Current snapshot as pretty-printed JSON.
    #[cfg(feature = "debug-api")]
    pub(crate) fn print(&self) -> String {
        serde_json::to_string_pretty(&self.snapshot())
            .unwrap_or_else(|e| format!("{{\"error\":\"{e}\"}}"))
    }

    fn emit(&self, settings: &StatsSettings, stats: &VeilidStats) {
        let Ok(line) = serde_json::to_string(stats) else {
            veilid_log!(self warn "failed to serialize stats");
            return;
        };
        match &settings.output {
            StatsOutput::Facility => {
                veilid_log!(self info target: "stats", "{}", line);
            }
            #[cfg(not(all(target_arch = "wasm32", target_os = "unknown")))]
            StatsOutput::File(path) => {
                self.emit_to_file(path, &line);
            }
        }
    }

    /// Relative paths resolve under the temp dir, which is writable inside app
    /// sandboxes on every platform; absolute paths are used as given.
    #[cfg(not(all(target_arch = "wasm32", target_os = "unknown")))]
    fn resolve_output_path(path: &std::path::Path) -> std::path::PathBuf {
        if path.is_absolute() {
            path.to_path_buf()
        } else {
            std::env::temp_dir().join(path)
        }
    }

    #[cfg(not(all(target_arch = "wasm32", target_os = "unknown")))]
    fn emit_to_file(&self, path: &std::path::Path, line: &str) {
        use std::io::Write;
        let path = Self::resolve_output_path(path);
        let mut inner = self.inner.lock();
        let need_open = match &inner.output_file {
            Some((p, _)) => p.as_path() != path.as_path(),
            None => true,
        };
        if need_open {
            match std::fs::OpenOptions::new()
                .create(true)
                .append(true)
                .open(&path)
            {
                Ok(f) => {
                    // At info so the resolved path is visible without debug logging
                    veilid_log!(self info "stats logging to {}", path.display());
                    inner.output_file = Some((path.clone(), f));
                }
                Err(e) => {
                    veilid_log!(self warn "failed to open stats log file {}: {}", path.display(), e);
                    inner.output_file = None;
                    return;
                }
            }
        }
        if let Some((_, f)) = inner.output_file.as_mut() {
            if let Err(e) = writeln!(f, "{line}") {
                veilid_log!(self warn "failed to write stats log: {}", e);
            }
        }
    }
}
