use crate::dart_isolate_wrapper::*;
use allo_isolate::*;
use cfg_if::*;
use data_encoding::BASE64URL_NOPAD;
use ffi_support::*;
use lazy_static::*;

cfg_if::cfg_if! {
    if #[cfg(feature = "opentelemetry")] {
        use opentelemetry_otlp::WithExportConfig;
        use opentelemetry_sdk::trace::{BatchConfigBuilder, BatchSpanProcessor, SdkTracerProvider};
        use opentelemetry::trace::TracerProvider;
    }
}

use parking_lot::Mutex;
use serde::*;
use std::{backtrace::Backtrace, io::Write, os::raw::c_char, sync::Arc};
use tracing::*;
use tracing_flame::FlameLayer;
use tracing_subscriber::prelude::*;
use veilid_core::bytes::Bytes;
use veilid_core::tools::*;
use veilid_core::*;

cfg_if! {
    if #[cfg(target_os = "android")] {
        /// logd is a socket, and `AndroidLogWriter` writes to it on the thread
        /// that emitted the event. When logd pushes back (an attached `flutter
        /// run` reader fills its buffer), the blocking write stalls a whole
        /// tokio worker, and `measure_limit` wall-clock timings blame whatever
        /// the task was doing rather than the logging.
        ///
        /// So the socket gets a dedicated thread and a lossy queue. That is the
        /// general rule for kernel-level blocking near the runtime: its own
        /// thread, or veilid_tools' spawn_blocking / blocking_yielding. Lossy
        /// on purpose: drop log lines rather than stall the node.
        fn android_log_writer() -> tracing_appender::non_blocking::NonBlocking {
            use std::io::Write;
            use tracing_subscriber::fmt::MakeWriter;

            struct AndroidLog(paranoid_android::AndroidLogMakeWriter);

            impl Write for AndroidLog {
                fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
                    // AndroidLogWriter flushes in Drop and keeps its buffer, so a flush here doubles every line
                    let mut writer = self.0.make_writer();
                    writer.write(buf)
                }
                fn flush(&mut self) -> std::io::Result<()> {
                    Ok(())
                }
            }

            let (writer, guard) = tracing_appender::non_blocking(AndroidLog(
                paranoid_android::AndroidLogMakeWriter::new("com.veilid.veilid-flutter".to_owned()),
            ));
            // The worker stops when its guard drops, taking the log with it
            *ANDROID_LOG_GUARD.lock() = Some(guard);
            writer
        }

        lazy_static! {
            static ref ANDROID_LOG_GUARD: Mutex<Option<tracing_appender::non_blocking::WorkerGuard>> =
                Mutex::new(None);
        }
    }
}

// Globals
lazy_static! {
    static ref CORE_INITIALIZED: Mutex<bool> = Mutex::new(false);
    static ref VEILID_API: AsyncMutex<Option<VeilidAPI>> = AsyncMutex::new(None);
    static ref FILTERS: Mutex<BTreeMap<&'static str, VeilidLayerFilter>> =
        Mutex::new(BTreeMap::new());
    static ref ROUTING_CONTEXTS: Mutex<BTreeMap<u32, RoutingContext>> = Mutex::new(BTreeMap::new());
    static ref TABLE_DBS: Mutex<BTreeMap<u32, TableDB>> = Mutex::new(BTreeMap::new());
    static ref TABLE_DB_TRANSACTIONS: Mutex<BTreeMap<u32, TableDBTransaction>> =
        Mutex::new(BTreeMap::new());
    static ref DHT_TRANSACTIONS: Mutex<BTreeMap<u32, DHTTransaction>> = Mutex::new(BTreeMap::new());
    static ref FLAME_GUARD: Mutex<Option<tracing_flame::FlushGuard<std::io::BufWriter<std::fs::File>>>> =
        Mutex::new(None);
    static ref NODE_STATE: Mutex<Option<NodeState>> = Mutex::new(None);
}

async fn get_veilid_api() -> VeilidAPIResult<VeilidAPI> {
    let api_lock = VEILID_API.lock().await;
    api_lock
        .as_ref()
        .cloned()
        .ok_or(VeilidAPIError::NotInitialized)
}

/// veilid-core's per-node identity, `(program_name, namespace)`.
type InitKey = (String, String);

/// Shim state for the running node: who receives its updates, and what started it.
///
/// The number of registered sinks is the node's reference count. Startup registers one and
/// shutdown retires one, and the node is torn down when an explicit shutdown retires the last.
/// Reaping a dead sink is not a shutdown: an Android foreground service outliving its activity's
/// engine must keep the node running with nothing attached.
///
/// Every transition runs under the `VEILID_API` async lock. Racing two `shutdown().await` calls
/// would let the loser report completion before the node is down, and core's own Arc count is not
/// a substitute, since `get_veilid_api` clones it on every FFI call.
///
/// One instance today, shaped to become one per `InitKey`.
struct NodeState {
    sinks: Arc<DartUpdateSinkSet>,
    config_json: String,
    init_key: InitKey,
}

/// The fields of a startup config that name which node it starts.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct VeilidFFIConfigInitKey {
    program_name: String,
    namespace: String,
}

/// Read the init key from a startup config without consuming the config, so core still gets it
/// verbatim and still warns about unknown keys.
fn peek_init_key(config_json: &str) -> VeilidAPIResult<InitKey> {
    let key = veilid_core::deserialize_json::<VeilidFFIConfigInitKey>(config_json)?;
    if key.program_name.is_empty() {
        return Err(VeilidAPIError::generic(
            "Program name must not be empty in 'program_name'",
        ));
    }
    Ok((key.program_name, key.namespace))
}

fn update_callback(sinks: Arc<DartUpdateSinkSet>) -> UpdateCallback {
    Arc::new(move |update: VeilidUpdate| match update {
        VeilidUpdate::Shutdown => sinks.close_all(),
        _ => sinks.broadcast_json(update),
    })
}

/////////////////////////////////////////
// FFI Helpers

// Declare external routine to release ffi strings
define_string_destructor!(free_string);

/////////////////////////////////////////
// FFI-specific

#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct VeilidFFIConfigLoggingTerminal {
    pub enabled: bool,
    pub level: VeilidConfigLogLevel,
    #[deprecated(note = "Use directives instead")]
    pub ignore_log_targets: Vec<String>,
    pub directives: Vec<String>,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct VeilidFFIConfigLoggingOtlp {
    pub enabled: bool,
    pub level: VeilidConfigLogLevel,
    pub grpc_endpoint: String,
    pub service_name: String,
    #[deprecated(note = "Use directives instead")]
    pub ignore_log_targets: Vec<String>,
    pub directives: Vec<String>,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct VeilidFFIConfigLoggingApi {
    pub enabled: bool,
    pub level: VeilidConfigLogLevel,
    #[deprecated(note = "Use directives instead")]
    pub ignore_log_targets: Vec<String>,
    pub directives: Vec<String>,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct VeilidFFIConfigLoggingFlame {
    pub enabled: bool,
    pub path: String,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct VeilidFFIConfigLoggingStats {
    /// Emit periodic stats records
    pub enabled: bool,
    /// Seconds between records
    pub interval_secs: u32,
    /// Directory or file for NDJSON records; empty routes to the "stats" log facility
    pub path: String,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct VeilidFFIConfigLogging {
    pub terminal: VeilidFFIConfigLoggingTerminal,
    pub api: VeilidFFIConfigLoggingApi,
    pub otlp: VeilidFFIConfigLoggingOtlp,
    pub flame: VeilidFFIConfigLoggingFlame,
    pub stats: VeilidFFIConfigLoggingStats,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct VeilidFFIConfig {
    pub logging: VeilidFFIConfigLogging,
}

/////////////////////////////////////////
// Initializer
#[no_mangle]
#[instrument]
pub extern "C" fn initialize_veilid_flutter(
    dart_post_c_object_ptr: ffi::DartPostCObjectFnType,
    crash_path: FfiStr,
) {
    // A duplicate image's statics are not the live core's: failing loudly beats its silent hang
    #[cfg(unix)]
    if let Some(desc) = crate::duplicate_image_guard::duplicate_image() {
        eprintln!(
            "veilid-flutter: FATAL: refusing to initialize a duplicate \
             veilid_flutter image in this process: {desc}"
        );
        std::process::abort();
    }

    // SAFETY: the pointer is Dart's NativeApi.postCObject, valid for the life of the process
    unsafe {
        store_dart_post_cobject(dart_post_c_object_ptr);
    }
    let crash_path = crash_path.into_opt_string().unwrap_or_default();

    use std::sync::Once;
    static INIT_BACKTRACE: Once = Once::new();
    INIT_BACKTRACE.call_once(move || {
        // unsafe {
        //     std::env::set_var("RUST_BACKTRACE", "1");
        // }
        std::panic::set_hook(Box::new(move |panic_info| {
            // Nothing here may panic: a nested panic in the hook loses the report
            let crash_file = if crash_path.is_empty() {
                None
            } else {
                std::fs::File::create(&crash_path).ok()
            };

            let (file, line) = if let Some(loc) = panic_info.location() {
                (loc.file(), loc.line())
            } else {
                ("<unknown>", 0)
            };

            let mut out = String::new();

            out += &format!("### Rust `panic!` hit at file '{}', line {}\n", file, line);
            if let Some(s) = panic_info.payload().downcast_ref::<&str>() {
                out += &format!("panic payload: {:?}\n", s);
            } else if let Some(s) = panic_info.payload().downcast_ref::<String>() {
                out += &format!("panic payload: {:?}\n", s);
            } else if let Some(a) = panic_info.payload().downcast_ref::<std::fmt::Arguments>() {
                out += &format!("panic payload: {:?}\n", a);
            } else {
                out += "no panic payload\n";
            }
            out += &format!("  Complete stack trace:\n{}\n", Backtrace::force_capture());

            if let Some(mut crash_file) = crash_file {
                let _ = write!(crash_file, "{}", out);
                let _ = crash_file.flush();
            } else {
                eprintln!("{}", out);
            }

            // And stop the process, no recovery is going to be possible here
            eprintln!("aborting!");
            std::process::abort();
        }));
    });
}

//////////////////////////////////////////////////////////////////////////////////
// C-compatible FFI Functions

#[no_mangle]
#[instrument]
pub extern "C" fn initialize_veilid_core(platform_config: FfiStr) {
    // Only do this once, ever
    // Until we have Dart native finalizers running on hot-restart, this will cause a crash if run more than once
    {
        let mut core_init = CORE_INITIALIZED.lock();
        if *core_init {
            return;
        }
        *core_init = true;
    }

    let platform_config = platform_config.into_opt_string();
    let platform_config: VeilidFFIConfig = deserialize_opt_json(platform_config)
        .expect_or_log("failed to deserialize plaform config json");

    // Set up subscriber and layers
    let subscriber = tracing_subscriber::Registry::default();
    let mut layers = Vec::new();
    let mut filters = (*FILTERS).lock();

    let mut fields_to_strip = HashSet::<&'static str>::new();
    fields_to_strip.insert(VEILID_LOG_KEY_FIELD);

    // Terminal logger
    if platform_config.logging.terminal.enabled {
        let timer = time::format_description::parse_borrowed::<2>("[hour]:[minute]:[second]")
            .expect("invalid time format");

        // Get time offset for local timezone from UTC
        // let time_offset =
        //     time::UtcOffset::current_local_offset().unwrap_or(time::UtcOffset::UTC);
        // nerd fight: https://www.reddit.com/r/learnrust/comments/1bgc4p7/time_crate_never_manages_to_get_local_time/
        // Use chrono instead of time crate to get local offset
        let offset_in_sec = chrono::Local::now().offset().local_minus_utc();
        let time_offset =
            time::UtcOffset::from_whole_seconds(offset_in_sec).expect("invalid utc offset");
        let timer = tracing_subscriber::fmt::time::OffsetTime::new(time_offset, timer);

        let filter = VeilidLayerFilter::new_with_config(
            VeilidLayerFilterConfig::new()
                .with_common_log_level(platform_config.logging.terminal.level),
        );
        #[allow(deprecated)]
        filter.apply_ignore_change_list(&platform_config.logging.terminal.ignore_log_targets);
        filter
            .try_apply_directives(platform_config.logging.terminal.directives)
            .expect_or_log("failed to apply terminal logging directives");

        cfg_if! {
            if #[cfg(target_os = "android")] {
                let layer = tracing_subscriber::fmt::Layer::new()
                    .map_fmt_fields(FmtStripVeilidFields::mapper())
                    .with_timer(timer)
                    .with_target(true)
                    .with_ansi(false)
                    .with_writer(android_log_writer())
                    .with_filter(filter.clone());
                filters.insert("terminal", filter);
                layers.push(layer.boxed());
            } else if #[cfg(target_os = "ios")] {
                let layer = tracing_subscriber::fmt::Layer::new()
                    .compact()
                    .map_fmt_fields(FmtStripVeilidFields::mapper())
                    .with_timer(timer)
                    .with_ansi(false)
                    .with_writer(std::io::stdout)
                    .with_filter(filter.clone());
                filters.insert("terminal", filter);
                layers.push(layer.boxed());
            } else {
                let layer = tracing_subscriber::fmt::Layer::new()
                    .compact()
                    .map_fmt_fields(FmtStripVeilidFields::mapper())
                    .with_timer(timer)
                    .with_writer(std::io::stdout)
                    .with_filter(filter.clone());
                filters.insert("terminal", filter);
                layers.push(layer.boxed());
            }
        }
    };

    // API logger
    if platform_config.logging.api.enabled {
        let filter = VeilidLayerFilter::new_with_config(
            VeilidLayerFilterConfig::new().with_common_log_level(platform_config.logging.api.level),
        );
        #[allow(deprecated)]
        filter.apply_ignore_change_list(&platform_config.logging.api.ignore_log_targets);
        filter
            .try_apply_directives(platform_config.logging.api.directives)
            .expect_or_log("failed to apply api logging directives");

        let layer = ApiTracingLayer::init().with_filter(filter.clone());
        filters.insert("api", filter);
        layers.push(layer.boxed());
    }

    // OpenTelemetry logger
    cfg_if! {
        if #[cfg(feature = "opentelemetry")] {
            if platform_config.logging.otlp.enabled {
                let grpc_endpoint = platform_config.logging.otlp.grpc_endpoint.clone();

                cfg_if! {
                    if #[cfg(feature="rt-tokio")] {
                        // OTLP build() spawns runtime workers; enter global runtime first.
                        let _guard = crate::tools::enter_global_runtime();
                        let exporter = opentelemetry_otlp::SpanExporter::builder()
                            .with_tonic()
                            .with_endpoint(format!("http://{}", grpc_endpoint))
                            .build()
                            .expect("failed to build OpenTelemetry exporter");
                        // Default BatchSpanProcessor queue is 2048 spans; with verbose
                        // tracing directives this overflows in <1s and drops earlier
                        // spans. Bump queue + batch size + reduce delay so diagnostic
                        // runs can capture multi-minute traces without loss.
                        let batch_config = BatchConfigBuilder::default()
                            .with_max_queue_size(1_048_576)
                            .with_max_export_batch_size(8192)
                            .with_scheduled_delay(std::time::Duration::from_millis(500))
                            .build();
                        let batch = BatchSpanProcessor::builder(exporter)
                            .with_batch_config(batch_config)
                            .build();
                        let provider = SdkTracerProvider::builder()
                            .with_span_processor(batch)
                            .build();
                        let tracer = provider.tracer(format!(
                            "{}:{}",
                            platform_config.logging.otlp.service_name,
                            hostname::get()
                                .map(|s| s.to_string_lossy().into_owned())
                                .unwrap_or_else(|_| "unknown".to_owned())));
                    } else {
                        compile_error!("needs executor implementation");
                    }
                }

                let filter =
                    VeilidLayerFilter::new_with_config(VeilidLayerFilterConfig::new().with_directives([
                        VeilidLogDirective::global_level(Some(platform_config.logging.otlp.level)),
                    ]));

                #[allow(deprecated)]
                filter.apply_ignore_change_list(&platform_config.logging.otlp.ignore_log_targets);
                filter
                    .try_apply_directives(platform_config.logging.otlp.directives)
                    .expect_or_log("failed to apply otlp logging directives");

                let layer = tracing_opentelemetry::layer()
                    .with_tracer(tracer)
                    .with_filter(filter.clone());
                filters.insert("otlp", filter);
                layers.push(layer.boxed());
            }
        }
    }

    // Flamegraph logger
    if platform_config.logging.flame.enabled {
        let (flame_layer, guard) =
            FlameLayer::with_file(&platform_config.logging.flame.path).unwrap_or_log();
        *FLAME_GUARD.lock() = Some(guard);

        layers.push(
            flame_layer
                .with_threads_collapsed(true)
                .with_empty_samples(false)
                .boxed(),
        );
    }

    let subscriber = subscriber.with(layers);

    subscriber
        .try_init()
        .map_err(|e| format!("failed to initialize logging: {}", e))
        .expect_or_log("failed to initalize ffi platform");

    // Stats manager: process-global, under the default log key
    #[cfg(feature = "stats")]
    {
        let stats = &platform_config.logging.stats;
        let output = if stats.path.is_empty() {
            StatsOutput::Facility
        } else {
            StatsOutput::File(std::path::PathBuf::from(&stats.path))
        };
        StatsManager::init(
            None,
            StatsSettings {
                enabled: stats.enabled,
                interval_secs: stats.interval_secs,
                output,
            },
        );
    }
}

#[no_mangle]
pub extern "C" fn change_log_level(layer: FfiStr, directives: FfiStr) -> i32 {
    // get layer to apply directives on
    let layer = layer.into_opt_string().unwrap_or("all".to_owned());
    let layer = if layer == "all" { "".to_owned() } else { layer };

    // get directive to apply to layer
    let directives = directives.into_opt_string().unwrap_or_default();

    // change log level on appropriate layer
    let filters = (*FILTERS).lock();
    if layer.is_empty() {
        // Change all layers
        for f in filters.values() {
            if f.try_apply_directives_string(directives.clone()).is_err() {
                return 2;
            }
        }
    } else {
        // Change a specific layer
        let Some(f) = filters.get(layer.as_str()) else {
            return 1;
        };
        if f.try_apply_directives_string(directives).is_err() {
            return 2;
        }
    }
    0
}

#[no_mangle]
pub extern "C" fn change_log_ignore(layer: FfiStr, log_ignore: FfiStr) {
    // get layer to change level on
    let layer = layer.into_opt_string().unwrap_or("all".to_owned());
    let layer = if layer == "all" { "".to_owned() } else { layer };

    // get changes to make
    let log_ignore = log_ignore.into_opt_string().unwrap_or_default();

    // change log level on appropriate layer
    let filters = (*FILTERS).lock();
    if layer.is_empty() {
        // Change all layers
        for f in filters.values() {
            #[allow(deprecated)]
            f.apply_ignore_change_string(log_ignore.clone());
        }
    } else {
        // Change a specific layer
        let Some(f) = filters.get(layer.as_str()) else {
            return;
        };
        #[allow(deprecated)]
        f.apply_ignore_change_string(log_ignore);
    }
}

#[no_mangle]
#[instrument(level = "trace", target = "ffi", skip_all)]
pub extern "C" fn startup_veilid_core(port: i64, stream_port: i64, config: FfiStr) {
    let config = config.into_opt_string();
    DartIsolateWrapper::new(port).spawn_result(
        async move {
            let config_json = match config {
                Some(v) => v,
                None => {
                    let err = VeilidAPIError::MissingArgument {
                        context: "startup_veilid_core".to_owned(),
                        argument: "config".to_owned(),
                    };
                    return VeilidAPIResult::Err(err);
                }
            };
            let init_key = peek_init_key(&config_json)?;

            let mut api_lock = VEILID_API.lock().await;

            // join the running node when it is the same one this config asks for
            if api_lock.is_some() {
                let mut node_state = NODE_STATE.lock();
                let Some(node_state) = node_state.as_mut() else {
                    return VeilidAPIResult::Err(VeilidAPIError::NotInitialized);
                };
                if node_state.init_key != init_key {
                    return VeilidAPIResult::Err(VeilidAPIError::AlreadyInitialized);
                }
                if node_state.config_json != config_json {
                    warn!(
                        target: "ffi",
                        "joining a node started with a different config, this config is ignored"
                    );
                }
                // Hot-restart orphans: with no live consumer left, a table they hold open could never be deleted
                node_state.sinks.reap_dead_except(stream_port);
                if node_state.sinks.len() == 0 {
                    TABLE_DB_TRANSACTIONS.lock().clear();
                    TABLE_DBS.lock().clear();
                    DHT_TRANSACTIONS.lock().clear();
                    ROUTING_CONTEXTS.lock().clear();
                }
                node_state.sinks.register(stream_port);
                return VeilidAPIResult::Ok(());
            }

            let sinks = Arc::new(DartUpdateSinkSet::new());
            sinks.register(stream_port);

            let veilid_api =
                match api_startup_json(update_callback(sinks.clone()), config_json.clone()).await {
                    Ok(v) => v,
                    Err(e) => {
                        sinks.retire(stream_port);
                        return VeilidAPIResult::Err(e);
                    }
                };
            *api_lock = Some(veilid_api);
            *NODE_STATE.lock() = Some(NodeState {
                sinks,
                config_json,
                init_key,
            });

            VeilidAPIResult::Ok(())
        }
        .in_current_span(),
    );
}

#[no_mangle]
#[instrument(level = "trace", target = "ffi", skip_all)]
pub extern "C" fn get_veilid_state(port: i64) {
    DartIsolateWrapper::new(port).spawn_result_json(
        async move {
            let veilid_api = get_veilid_api().await?;
            let core_state = veilid_api.get_state().await?;
            VeilidAPIResult::Ok(core_state)
        }
        .in_current_span(),
    );
}

#[no_mangle]
#[instrument(level = "trace", target = "ffi", skip_all)]
pub extern "C" fn is_shutdown(port: i64) {
    DartIsolateWrapper::new(port).spawn_result(
        async move {
            let veilid_api = get_veilid_api().await;
            if let Err(VeilidAPIError::NotInitialized) = veilid_api {
                return VeilidAPIResult::Ok(true);
            }
            let veilid_api = veilid_api.unwrap_or_log();
            let is_shutdown = veilid_api.is_shutdown();
            VeilidAPIResult::Ok(is_shutdown)
        }
        .in_current_span(),
    );
}

#[no_mangle]
#[instrument(level = "trace", target = "ffi", skip_all)]
pub extern "C" fn attach(port: i64) {
    DartIsolateWrapper::new(port).spawn_result(
        async move {
            let veilid_api = get_veilid_api().await?;
            // every caller sharing the node attaches it, so core's "already attached" Generic
            // (its only error here other than NotInitialized) is this caller's success
            if let Err(e) = veilid_api.attach().await {
                if !matches!(e, VeilidAPIError::Generic { .. }) {
                    return VeilidAPIResult::Err(e);
                }
            }
            VeilidAPIResult::Ok(())
        }
        .in_current_span(),
    );
}

#[no_mangle]
#[instrument(level = "trace", target = "ffi", skip_all)]
pub extern "C" fn detach(port: i64) {
    DartIsolateWrapper::new(port).spawn_result(
        async move {
            let veilid_api = get_veilid_api().await?;
            // symmetric with attach: "not attached" is another caller's doing, not an error
            if let Err(e) = veilid_api.detach().await {
                if !matches!(e, VeilidAPIError::Generic { .. }) {
                    return VeilidAPIResult::Err(e);
                }
            }
            VeilidAPIResult::Ok(())
        }
        .in_current_span(),
    );
}

#[no_mangle]
#[instrument(level = "trace", target = "ffi", skip_all)]
pub extern "C" fn change_config(port: i64, config: FfiStr) {
    let config = config.into_opt_string();
    DartIsolateWrapper::new(port).spawn_result(
        async move {
            let config: VeilidConfig = deserialize_opt_json(config)?;
            let veilid_api = get_veilid_api().await?;
            veilid_api.change_config(config)?;
            VeilidAPIResult::Ok(())
        }
        .in_current_span(),
    );
}

#[no_mangle]
#[instrument(level = "trace", target = "ffi", skip_all)]
pub extern "C" fn report_app_lifecycle_state(port: i64, state: FfiStr) {
    let state = state.into_opt_string();
    DartIsolateWrapper::new(port).spawn_result(
        async move {
            let state: VeilidAppLifecycleState = deserialize_opt_json(state)?;
            let veilid_api = get_veilid_api().await?;
            veilid_api.report_app_lifecycle_state(state)?;
            VeilidAPIResult::Ok(())
        }
        .in_current_span(),
    );
}

#[no_mangle]
#[instrument(level = "trace", target = "ffi", skip_all)]
pub extern "C" fn wait_for_attachment_condition(port: i64, condition: FfiStr, timeout_ms: u64) {
    let condition = condition.into_opt_string();
    let timeout = if timeout_ms == 0 {
        None
    } else {
        Some(Duration::from_millis(timeout_ms))
    };

    DartIsolateWrapper::new(port).spawn_result_json(
        async move {
            let condition: Option<VeilidStateAttachmentCondition> =
                deserialize_optional_json(condition)?;
            let veilid_api = get_veilid_api().await?;
            let attachment = veilid_api
                .wait_for_attachment_condition(condition, timeout)
                .await?;
            VeilidAPIResult::Ok(attachment)
        }
        .in_current_span(),
    );
}

#[no_mangle]
#[instrument(level = "trace", target = "ffi", skip_all)]
pub extern "C" fn shutdown_veilid_core(port: i64, stream_port: i64) {
    DartIsolateWrapper::new(port).spawn_result(
        async move {
            let mut api_lock = VEILID_API.lock().await;
            if api_lock.is_none() {
                return VeilidAPIResult::Err(VeilidAPIError::NotInitialized);
            }

            let sinks = {
                let node_state = NODE_STATE.lock();
                let Some(node_state) = node_state.as_ref() else {
                    return VeilidAPIResult::Err(VeilidAPIError::NotInitialized);
                };
                // a caller with no registered sink holds no reference
                if !node_state.sinks.contains(stream_port) {
                    return VeilidAPIResult::Ok(());
                }
                node_state.sinks.clone()
            };

            // a dead engine's sink counts until something reaps it, and a detached node can go
            // arbitrarily long without an update to reap it on
            sinks.reap_dead_except(stream_port);

            if sinks.len() > 1 {
                sinks.retire(stream_port);
                return VeilidAPIResult::Ok(());
            }

            // the last reference stays registered so its consumer sees the updates core emits on the way down
            let Some(veilid_api) = api_lock.take() else {
                return VeilidAPIResult::Err(VeilidAPIError::NotInitialized);
            };
            // hot restart never releases handles; drop them so sqlite pools do not stay pinned
            // into the next startup. Last reference only - a live caller's handles are still its own
            TABLE_DB_TRANSACTIONS.lock().clear();
            TABLE_DBS.lock().clear();
            DHT_TRANSACTIONS.lock().clear();
            ROUTING_CONTEXTS.lock().clear();
            veilid_api.shutdown().await;

            // unconditional: without a Shutdown update every consumer would park in `await for` forever
            sinks.close_all();
            // dropped outside the guard's scope: retiring a sink posts, and posting under a lock deadlocks
            let retired_state = NODE_STATE.lock().take();
            drop(retired_state);

            VeilidAPIResult::Ok(())
        }
        .in_current_span(),
    );
}

// Dart-supplied arguments are parsed after the spawn: a panic inside an extern "C" fn
// aborts the app, an Err here reaches the port
fn base64_decode(s: &str) -> VeilidAPIResult<Vec<u8>> {
    BASE64URL_NOPAD.decode(s.as_bytes()).map_err(|e| {
        VeilidAPIError::parse_error(
            format!("invalid base64: {}", e),
            format!("base64_decode: {} bytes", s.len()),
        )
    })
}

fn base64_decode_opt(arg: Option<String>) -> VeilidAPIResult<Vec<u8>> {
    let Some(s) = arg else {
        return Err(VeilidAPIError::parse_error(
            "invalid null string",
            "base64_decode_opt: null",
        ));
    };
    base64_decode(&s)
}

fn deserialize_optional_json<T: de::DeserializeOwned + std::fmt::Debug>(
    arg: Option<String>,
) -> VeilidAPIResult<Option<T>> {
    arg.map(|s| deserialize_json(&s)).transpose()
}

fn add_routing_context(
    rc: &mut BTreeMap<u32, RoutingContext>,
    routing_context: RoutingContext,
) -> u32 {
    let mut next_id: u32 = 1;
    while rc.contains_key(&next_id) {
        next_id += 1;
    }
    rc.insert(next_id, routing_context);
    next_id
}

#[no_mangle]
#[instrument(level = "trace", target = "ffi", skip_all)]
pub extern "C" fn routing_context(port: i64) {
    DartIsolateWrapper::new(port).spawn_result(
        async move {
            let veilid_api = get_veilid_api().await?;
            let routing_context = veilid_api.routing_context()?;
            let mut rc = ROUTING_CONTEXTS.lock();
            let new_id = add_routing_context(&mut rc, routing_context);
            VeilidAPIResult::Ok(new_id)
        }
        .in_current_span(),
    );
}

#[no_mangle]
#[instrument(level = "trace", target = "ffi", skip_all)]
pub extern "C" fn release_routing_context(id: u32) -> i32 {
    let mut rc = ROUTING_CONTEXTS.lock();
    if rc.remove(&id).is_none() {
        return 0;
    }
    1
}

#[no_mangle]
#[instrument(level = "trace", target = "ffi", skip_all)]
pub extern "C" fn routing_context_with_default_safety(id: u32) -> u32 {
    let mut rc = ROUTING_CONTEXTS.lock();
    let Some(routing_context) = rc.get(&id) else {
        return 0;
    };
    let Ok(routing_context) = routing_context.clone().with_default_safety() else {
        return 0;
    };

    add_routing_context(&mut rc, routing_context)
}

#[no_mangle]
#[instrument(level = "trace", target = "ffi", skip_all)]
pub extern "C" fn routing_context_with_safety(id: u32, safety_selection: FfiStr) -> u32 {
    let Ok(safety_selection) =
        deserialize_opt_json::<SafetySelection>(safety_selection.into_opt_string())
    else {
        return 0;
    };

    let mut rc = ROUTING_CONTEXTS.lock();
    let Some(routing_context) = rc.get(&id) else {
        return 0;
    };
    let Ok(routing_context) = routing_context.clone().with_safety(safety_selection) else {
        return 0;
    };

    add_routing_context(&mut rc, routing_context)
}

#[no_mangle]
#[instrument(level = "trace", target = "ffi", skip_all)]
pub extern "C" fn routing_context_with_sequencing(id: u32, sequencing: FfiStr) -> u32 {
    let Ok(sequencing) = deserialize_opt_json::<Sequencing>(sequencing.into_opt_string()) else {
        return 0;
    };

    let mut rc = ROUTING_CONTEXTS.lock();
    let Some(routing_context) = rc.get(&id) else {
        return 0;
    };
    let routing_context = routing_context.clone().with_sequencing(sequencing);

    add_routing_context(&mut rc, routing_context)
}

fn get_routing_context(id: u32, func_name: &str) -> VeilidAPIResult<RoutingContext> {
    let rc = ROUTING_CONTEXTS.lock();
    let Some(routing_context) = rc.get(&id) else {
        return VeilidAPIResult::Err(VeilidAPIError::invalid_argument(func_name, "id", id));
    };
    Ok(routing_context.clone())
}

#[no_mangle]
#[instrument(level = "trace", target = "ffi", skip_all)]
pub extern "C" fn routing_context_safety(port: i64, id: u32) {
    DartIsolateWrapper::new(port).spawn_result_json(async move {
        let routing_context = get_routing_context(id, "routing_context_safety")?;
        VeilidAPIResult::Ok(routing_context.safety())
    });
}

#[no_mangle]
#[instrument(level = "trace", target = "ffi", skip_all)]
pub extern "C" fn routing_context_app_call(port: i64, id: u32, target: FfiStr, request: FfiStr) {
    let target = target.into_opt_string();
    let request = request.into_opt_string();
    DartIsolateWrapper::new(port).spawn_result(
        async move {
            let target: Target = deserialize_opt_json(target)?;
            let request = base64_decode_opt(request)?;
            let routing_context = get_routing_context(id, "routing_context_app_call")?;

            let answer = routing_context.app_call(target, request).await?;
            let answer = data_encoding::BASE64URL_NOPAD.encode(&answer);
            VeilidAPIResult::Ok(answer)
        }
        .in_current_span(),
    );
}

#[no_mangle]
#[instrument(level = "trace", target = "ffi", skip_all)]
pub extern "C" fn routing_context_app_message(port: i64, id: u32, target: FfiStr, message: FfiStr) {
    let target = target.into_opt_string();
    let message = message.into_opt_string();
    DartIsolateWrapper::new(port).spawn_result(
        async move {
            let target: Target = deserialize_opt_json(target)?;
            let message = base64_decode_opt(message)?;
            let routing_context = get_routing_context(id, "routing_context_app_message")?;

            routing_context.app_message(target, message).await?;
            VeilidAPIResult::Ok(())
        }
        .in_current_span(),
    );
}

#[no_mangle]
#[instrument(level = "trace", target = "ffi", skip_all)]
pub extern "C" fn routing_context_create_dht_record(
    port: i64,
    id: u32,
    kind: u32,
    schema: FfiStr,
    owner: FfiStr,
) {
    let crypto_kind = CryptoKind::from(kind);
    let schema = schema.into_opt_string();
    let owner = owner.into_opt_string();

    DartIsolateWrapper::new(port).spawn_result_json(
        async move {
            let schema: DHTSchema = deserialize_opt_json(schema)?;
            let owner: Option<KeyPair> = deserialize_optional_json(owner)?;
            let routing_context = get_routing_context(id, "routing_context_create_dht_record")?;

            let dht_record_descriptor = routing_context
                .create_dht_record(crypto_kind, schema, owner)
                .await?;
            VeilidAPIResult::Ok(dht_record_descriptor)
        }
        .in_current_span(),
    );
}

#[no_mangle]
#[instrument(level = "trace", target = "ffi", skip_all)]
pub extern "C" fn routing_context_open_dht_record(
    port: i64,
    id: u32,
    key: FfiStr,
    writer: FfiStr,
    options: FfiStr,
) {
    let key = key.into_opt_string();
    let writer = writer.into_opt_string();
    let options = options.into_opt_string();
    DartIsolateWrapper::new(port).spawn_result_json(
        async move {
            let key: RecordKey = deserialize_opt_json(key)?;
            let writer: Option<KeyPair> = deserialize_optional_json(writer)?;
            let options: Option<OpenDHTRecordOptions> = deserialize_optional_json(options)?;
            let routing_context = get_routing_context(id, "routing_context_open_dht_record")?;

            let dht_record_descriptor = routing_context
                .open_dht_record(key, writer, options)
                .await?;
            VeilidAPIResult::Ok(dht_record_descriptor)
        }
        .in_current_span(),
    );
}

#[no_mangle]
#[instrument(level = "trace", target = "ffi", skip_all)]
pub extern "C" fn routing_context_close_dht_record(port: i64, id: u32, key: FfiStr) {
    let key = key.into_opt_string();
    DartIsolateWrapper::new(port).spawn_result(
        async move {
            let key: RecordKey = deserialize_opt_json(key)?;
            let routing_context = get_routing_context(id, "routing_context_close_dht_record")?;

            routing_context.close_dht_record(key).await?;
            VeilidAPIResult::Ok(())
        }
        .in_current_span(),
    );
}

#[no_mangle]
#[instrument(level = "trace", target = "ffi", skip_all)]
pub extern "C" fn routing_context_delete_dht_record(port: i64, id: u32, key: FfiStr) {
    let key = key.into_opt_string();
    DartIsolateWrapper::new(port).spawn_result(
        async move {
            let key: RecordKey = deserialize_opt_json(key)?;
            let routing_context = get_routing_context(id, "routing_context_delete_dht_record")?;

            routing_context.delete_dht_record(key).await?;
            VeilidAPIResult::Ok(())
        }
        .in_current_span(),
    );
}

#[no_mangle]
#[instrument(level = "trace", target = "ffi", skip_all)]
pub extern "C" fn routing_context_get_dht_value(
    port: i64,
    id: u32,
    key: FfiStr,
    subkey: u32,
    force_refresh: bool,
) {
    let key = key.into_opt_string();
    DartIsolateWrapper::new(port).spawn_result_json(
        async move {
            let key: RecordKey = deserialize_opt_json(key)?;
            let routing_context = get_routing_context(id, "routing_context_get_dht_value")?;

            let res = routing_context
                .get_dht_value(key, subkey, force_refresh)
                .await?;
            VeilidAPIResult::Ok(res)
        }
        .in_current_span(),
    );
}

#[no_mangle]
#[instrument(level = "trace", target = "ffi", skip_all)]
pub extern "C" fn routing_context_set_dht_value(
    port: i64,
    id: u32,
    key: FfiStr,
    subkey: u32,
    data: FfiStr,
    options: FfiStr,
) {
    let key = key.into_opt_string();
    let data = data.into_opt_string();
    let options = options.into_opt_string();

    DartIsolateWrapper::new(port).spawn_result_json(
        async move {
            let key: RecordKey = deserialize_opt_json(key)?;
            let data = base64_decode_opt(data)?;
            let options: Option<SetDHTValueOptions> = deserialize_optional_json(options)?;
            let routing_context = get_routing_context(id, "routing_context_set_dht_value")?;

            let res = routing_context
                .set_dht_value(key, subkey, data, options)
                .await?;
            VeilidAPIResult::Ok(res)
        }
        .in_current_span(),
    );
}

#[no_mangle]
#[instrument(level = "trace", target = "ffi", skip_all)]
pub extern "C" fn routing_context_watch_dht_values(
    port: i64,
    id: u32,
    key: FfiStr,
    subkeys: FfiStr,
    expiration: u64,
    count: u32,
) {
    let key = key.into_opt_string();
    let subkeys = subkeys.into_opt_string();
    let expiration = Timestamp::from(expiration);

    DartIsolateWrapper::new(port).spawn_result(
        async move {
            let key: RecordKey = deserialize_opt_json(key)?;
            let subkeys: Option<ValueSubkeyRangeSet> = deserialize_optional_json(subkeys)?;
            let routing_context = get_routing_context(id, "routing_context_watch_dht_values")?;

            let res = routing_context
                .watch_dht_values(key, subkeys, Some(expiration), Some(count))
                .await?;
            VeilidAPIResult::Ok(res)
        }
        .in_current_span(),
    );
}

#[no_mangle]
#[instrument(level = "trace", target = "ffi", skip_all)]
pub extern "C" fn routing_context_cancel_dht_watch(
    port: i64,
    id: u32,
    key: FfiStr,
    subkeys: FfiStr,
) {
    let key = key.into_opt_string();
    let subkeys = subkeys.into_opt_string();

    DartIsolateWrapper::new(port).spawn_result(
        async move {
            let key: RecordKey = deserialize_opt_json(key)?;
            let subkeys: Option<ValueSubkeyRangeSet> = deserialize_optional_json(subkeys)?;
            let routing_context = get_routing_context(id, "routing_context_cancel_dht_watch")?;

            let res = routing_context.cancel_dht_watch(key, subkeys).await?;
            VeilidAPIResult::Ok(res)
        }
        .in_current_span(),
    );
}

#[no_mangle]
#[instrument(level = "trace", target = "ffi", skip_all)]
pub extern "C" fn routing_context_inspect_dht_record(
    port: i64,
    id: u32,
    key: FfiStr,
    subkeys: FfiStr,
    scope: FfiStr,
) {
    let key = key.into_opt_string();

    let subkeys = subkeys.into_opt_string();

    let scope = scope.into_opt_string();

    DartIsolateWrapper::new(port).spawn_result_json(
        async move {
            let key: RecordKey = deserialize_opt_json(key)?;
            let scope: DHTReportScope = deserialize_opt_json(scope)?;
            let subkeys: Option<ValueSubkeyRangeSet> = deserialize_optional_json(subkeys)?;
            let routing_context = get_routing_context(id, "routing_context_inspect_dht_record")?;

            let res = routing_context
                .inspect_dht_record(key, subkeys, scope)
                .await?;
            VeilidAPIResult::Ok(res)
        }
        .in_current_span(),
    );
}

#[no_mangle]
#[instrument(level = "trace", target = "ffi", skip_all)]
pub extern "C" fn routing_context_flush_dht_record(
    port: i64,
    id: u32,
    key: FfiStr,
    timeout_ms: u64,
) {
    let key = key.into_opt_string();
    let timeout = if timeout_ms == 0 {
        None
    } else {
        Some(Duration::from_millis(timeout_ms))
    };

    DartIsolateWrapper::new(port).spawn_result(
        async move {
            let key: RecordKey = deserialize_opt_json(key)?;
            let routing_context = get_routing_context(id, "routing_context_flush_dht_record")?;

            let res = routing_context.flush_dht_record(key, timeout).await?;
            VeilidAPIResult::Ok(res)
        }
        .in_current_span(),
    );
}

#[no_mangle]
#[instrument(level = "trace", target = "ffi", skip_all)]
pub extern "C" fn generate_member_id(port: i64, writer_key: FfiStr) {
    let writer_key = writer_key.into_opt_string();

    DartIsolateWrapper::new(port).spawn_result_json(
        async move {
            let writer_key: PublicKey = deserialize_opt_json(writer_key)?;
            let veilid_api = get_veilid_api().await?;

            let member_id = veilid_api.generate_member_id(&writer_key)?;

            VeilidAPIResult::Ok(member_id)
        }
        .in_current_span(),
    );
}

#[no_mangle]
#[instrument(level = "trace", target = "ffi", skip_all)]
pub extern "C" fn get_dht_record_key(
    port: i64,
    schema: FfiStr,
    owner: FfiStr,
    encryption_key: FfiStr,
) {
    let schema = schema.into_opt_string();
    let owner = owner.into_opt_string();
    let encryption_key = encryption_key.into_opt_string();

    DartIsolateWrapper::new(port).spawn_result_json(
        async move {
            let schema: DHTSchema = deserialize_opt_json(schema)?;
            let owner: PublicKey = deserialize_opt_json(owner)?;
            let encryption_key: Option<SharedSecret> = deserialize_optional_json(encryption_key)?;
            let veilid_api = get_veilid_api().await?;

            let record_key = veilid_api
                .get_dht_record_key(schema, owner, encryption_key)
                .await?;
            VeilidAPIResult::Ok(record_key)
        }
        .in_current_span(),
    );
}

#[no_mangle]
#[instrument(level = "trace", target = "ffi", skip_all)]
pub extern "C" fn new_private_route(port: i64) {
    DartIsolateWrapper::new(port).spawn_result_json(
        async move {
            let veilid_api = get_veilid_api().await?;

            let route_blob = veilid_api.new_private_route().await?;

            VeilidAPIResult::Ok(route_blob)
        }
        .in_current_span(),
    );
}

#[no_mangle]
#[instrument(level = "trace", target = "ffi", skip_all)]
pub extern "C" fn new_custom_private_route(port: i64, private_spec: FfiStr) {
    let private_spec = private_spec.into_opt_string();

    DartIsolateWrapper::new(port).spawn_result_json(
        async move {
            let private_spec: PrivateSpec = deserialize_opt_json(private_spec)?;
            let veilid_api = get_veilid_api().await?;

            let route_blob = veilid_api.new_custom_private_route(private_spec).await?;

            VeilidAPIResult::Ok(route_blob)
        }
        .in_current_span(),
    );
}

#[no_mangle]
#[instrument(level = "trace", target = "ffi", skip_all)]
pub extern "C" fn import_remote_private_route(port: i64, blob: FfiStr) {
    let blob = blob.into_opt_string();
    DartIsolateWrapper::new(port).spawn_result_json(
        async move {
            let blob = base64_decode_opt(blob)?;
            let veilid_api = get_veilid_api().await?;

            let route_id = veilid_api.import_remote_private_route(blob)?;
            VeilidAPIResult::Ok(route_id)
        }
        .in_current_span(),
    );
}

#[no_mangle]
#[instrument(level = "trace", target = "ffi", skip_all)]
pub extern "C" fn release_private_route(port: i64, route_id: FfiStr) {
    let route_id = route_id.into_opt_string();

    DartIsolateWrapper::new(port).spawn_result(
        async move {
            let route_id: RouteId = deserialize_opt_json(route_id)?;
            let veilid_api = get_veilid_api().await?;
            veilid_api.release_private_route(route_id)?;
            VeilidAPIResult::Ok(())
        }
        .in_current_span(),
    );
}

#[no_mangle]
#[instrument(level = "trace", target = "ffi", skip_all)]
pub extern "C" fn app_call_reply(port: i64, call_id: FfiStr, message: FfiStr) {
    let call_id = call_id.into_opt_string().unwrap_or_default();
    let message = message.into_opt_string();

    DartIsolateWrapper::new(port).spawn_result(
        async move {
            let message = base64_decode_opt(message)?;
            let call_id = match call_id.parse() {
                Ok(v) => v,
                Err(e) => {
                    return VeilidAPIResult::Err(VeilidAPIError::parse_error(
                        format!("failed to parse call_id: {}", e),
                        call_id,
                    ));
                }
            };

            let veilid_api = get_veilid_api().await?;
            veilid_api.app_call_reply(call_id, message).await?;
            Ok(())
        }
        .in_current_span(),
    );
}

fn add_dht_transaction(dht_tx: DHTTransaction) -> u32 {
    let mut next_id: u32 = 1;
    let mut rc = DHT_TRANSACTIONS.lock();
    while rc.contains_key(&next_id) {
        next_id += 1;
    }
    rc.insert(next_id, dht_tx);
    next_id
}

#[no_mangle]
#[instrument(level = "trace", target = "ffi", skip_all)]
pub extern "C" fn transact_dht_records(port: i64, record_keys: FfiStr, options: FfiStr) {
    let record_keys = record_keys.into_opt_string();
    let options = options.into_opt_string();

    DartIsolateWrapper::new(port).spawn_result(
        async move {
            let record_keys: Vec<RecordKey> = deserialize_opt_json(record_keys)?;
            let options: Option<TransactDHTRecordsOptions> = deserialize_optional_json(options)?;
            let veilid_api = get_veilid_api().await?;
            let dht_tx = veilid_api
                .transact_dht_records(record_keys, options)
                .await?;
            let new_id = add_dht_transaction(dht_tx);
            VeilidAPIResult::Ok(new_id)
        }
        .in_current_span(),
    );
}

#[no_mangle]
#[instrument(level = "trace", target = "ffi", skip_all)]
pub extern "C" fn release_dht_transaction(id: u32) -> i32 {
    let mut rc = DHT_TRANSACTIONS.lock();
    if rc.remove(&id).is_none() {
        return 0;
    }
    1
}

fn get_dht_transaction(id: u32, func_name: &str) -> VeilidAPIResult<DHTTransaction> {
    let dht_transactions = DHT_TRANSACTIONS.lock();
    let Some(dht_tx) = dht_transactions.get(&id) else {
        return VeilidAPIResult::Err(VeilidAPIError::invalid_argument(func_name, "id", id));
    };
    Ok(dht_tx.clone())
}

#[no_mangle]
#[instrument(level = "trace", target = "ffi", skip_all)]
pub extern "C" fn dht_transaction_commit(port: i64, id: u32) {
    DartIsolateWrapper::new(port).spawn_result_json(
        async move {
            let dht_tx = get_dht_transaction(id, "dht_transaction_commit")?;

            dht_tx.commit().await?;
            VeilidAPIResult::Ok(())
        }
        .in_current_span(),
    );
}

#[no_mangle]
#[instrument(level = "trace", target = "ffi", skip_all)]
pub extern "C" fn dht_transaction_rollback(port: i64, id: u32) {
    DartIsolateWrapper::new(port).spawn_result_json(
        async move {
            let dht_tx = get_dht_transaction(id, "dht_transaction_rollback")?;

            dht_tx.rollback().await?;
            VeilidAPIResult::Ok(())
        }
        .in_current_span(),
    );
}

#[no_mangle]
#[instrument(level = "trace", target = "ffi", skip_all)]
pub extern "C" fn dht_transaction_extend(port: i64, id: u32, record_keys: FfiStr, options: FfiStr) {
    let record_keys = record_keys.into_opt_string();
    let options = options.into_opt_string();

    DartIsolateWrapper::new(port).spawn_result_json(
        async move {
            let record_keys: Vec<RecordKey> = deserialize_opt_json(record_keys)?;
            let options: Option<TransactDHTRecordsOptions> = deserialize_optional_json(options)?;
            let dht_tx = get_dht_transaction(id, "dht_transaction_extend")?;

            dht_tx.extend(record_keys, options).await?;
            VeilidAPIResult::Ok(())
        }
        .in_current_span(),
    );
}

#[no_mangle]
#[instrument(level = "trace", target = "ffi", skip_all)]
pub extern "C" fn dht_transaction_get(port: i64, id: u32, key: FfiStr, subkey: u32) {
    let key = key.into_opt_string();

    DartIsolateWrapper::new(port).spawn_result_json(
        async move {
            let key: RecordKey = deserialize_opt_json(key)?;
            let dht_tx = get_dht_transaction(id, "dht_transaction_get")?;

            let out = dht_tx.get(key, subkey).await?;
            VeilidAPIResult::Ok(out)
        }
        .in_current_span(),
    );
}

#[no_mangle]
#[instrument(level = "trace", target = "ffi", skip_all)]
pub extern "C" fn dht_transaction_set(
    port: i64,
    id: u32,
    key: FfiStr,
    subkey: u32,
    data: FfiStr,
    options: FfiStr,
) {
    let key = key.into_opt_string();
    let data = data.into_opt_string();
    let options = options.into_opt_string();

    DartIsolateWrapper::new(port).spawn_result_json(
        async move {
            let key: RecordKey = deserialize_opt_json(key)?;
            let data = base64_decode_opt(data)?;
            let options: Option<DHTTransactionSetValueOptions> =
                deserialize_optional_json(options)?;
            let dht_tx = get_dht_transaction(id, "dht_transaction_set")?;

            let out = dht_tx.set(key, subkey, data, options).await?;
            VeilidAPIResult::Ok(out)
        }
        .in_current_span(),
    );
}

#[no_mangle]
#[instrument(level = "trace", target = "ffi", skip_all)]
pub extern "C" fn dht_transaction_inspect(
    port: i64,
    id: u32,
    key: FfiStr,
    subkeys: FfiStr,
    scope: FfiStr,
) {
    let key = key.into_opt_string();
    let subkeys = subkeys.into_opt_string();
    let scope = scope.into_opt_string();

    DartIsolateWrapper::new(port).spawn_result_json(
        async move {
            let key: RecordKey = deserialize_opt_json(key)?;
            let scope: DHTReportScope = deserialize_opt_json(scope)?;
            let subkeys: Option<ValueSubkeyRangeSet> = deserialize_optional_json(subkeys)?;
            let dht_tx = get_dht_transaction(id, "dht_transaction_inspect")?;

            let out = dht_tx.inspect(key, subkeys, scope).await?;
            VeilidAPIResult::Ok(out)
        }
        .in_current_span(),
    );
}

fn add_table_db(table_db: TableDB) -> u32 {
    let mut next_id: u32 = 1;
    let mut rc = TABLE_DBS.lock();
    while rc.contains_key(&next_id) {
        next_id += 1;
    }
    rc.insert(next_id, table_db);
    next_id
}

#[no_mangle]
#[instrument(level = "trace", target = "ffi", skip_all)]
pub extern "C" fn open_table_db(port: i64, name: FfiStr, column_count: u32) {
    let name = name.into_opt_string().unwrap_or_default();
    DartIsolateWrapper::new(port).spawn_result(
        async move {
            let veilid_api = get_veilid_api().await?;
            let table_db = veilid_api.open_table_db(&name, column_count).await?;
            let new_id = add_table_db(table_db);
            VeilidAPIResult::Ok(new_id)
        }
        .in_current_span(),
    );
}

#[no_mangle]
#[instrument(level = "trace", target = "ffi", skip_all)]
pub extern "C" fn release_table_db(id: u32) -> i32 {
    let mut rc = TABLE_DBS.lock();
    if rc.remove(&id).is_none() {
        return 0;
    }
    1
}

#[no_mangle]
#[instrument(level = "trace", target = "ffi", skip_all)]
pub extern "C" fn delete_table_db(port: i64, name: FfiStr) {
    let name = name.into_opt_string().unwrap_or_default();
    DartIsolateWrapper::new(port).spawn_result(
        async move {
            let veilid_api = get_veilid_api().await?;
            let deleted = veilid_api.delete_table_db(&name).await?;
            VeilidAPIResult::Ok(deleted)
        }
        .in_current_span(),
    );
}

#[no_mangle]
#[instrument(level = "trace", target = "ffi", skip_all)]
pub extern "C" fn rename_table_db(port: i64, old_name: FfiStr, new_name: FfiStr) {
    let old_name = old_name.into_opt_string().unwrap_or_default();
    let new_name = new_name.into_opt_string().unwrap_or_default();
    DartIsolateWrapper::new(port).spawn_result(
        async move {
            let veilid_api = get_veilid_api().await?;
            veilid_api.rename_table_db(&old_name, &new_name).await?;
            VeilidAPIResult::Ok(())
        }
        .in_current_span(),
    );
}

#[no_mangle]
#[instrument(level = "trace", target = "ffi", skip_all)]
pub extern "C" fn table_db_get_column_count(id: u32) -> u32 {
    let table_dbs = TABLE_DBS.lock();
    let Some(table_db) = table_dbs.get(&id) else {
        return 0;
    };
    let Ok(cc) = table_db.clone().get_column_count() else {
        return 0;
    };
    cc
}

fn get_table_db(id: u32, func_name: &str) -> VeilidAPIResult<TableDB> {
    let table_dbs = TABLE_DBS.lock();
    let Some(table_db) = table_dbs.get(&id) else {
        return VeilidAPIResult::Err(VeilidAPIError::invalid_argument(func_name, "id", id));
    };
    Ok(table_db.clone())
}

#[no_mangle]
#[instrument(level = "trace", target = "ffi", skip_all)]
pub extern "C" fn table_db_get_keys(port: i64, id: u32, col: u32) {
    DartIsolateWrapper::new(port).spawn_result_json(
        async move {
            let table_db = get_table_db(id, "table_db_get_keys")?;

            let keys = table_db.get_keys(col).await?;
            let out: Vec<String> = keys
                .into_iter()
                .map(|k| BASE64URL_NOPAD.encode(&k))
                .collect();
            VeilidAPIResult::Ok(out)
        }
        .in_current_span(),
    );
}

fn add_table_db_transaction(tdbt: TableDBTransaction) -> u32 {
    let mut next_id: u32 = 1;
    let mut tdbts = TABLE_DB_TRANSACTIONS.lock();
    while tdbts.contains_key(&next_id) {
        next_id += 1;
    }
    tdbts.insert(next_id, tdbt);
    next_id
}

#[no_mangle]
#[instrument(level = "trace", target = "ffi", skip_all)]
pub extern "C" fn table_db_transact(id: u32) -> u32 {
    let table_dbs = TABLE_DBS.lock();
    let Some(table_db) = table_dbs.get(&id) else {
        return 0;
    };
    let tdbt = table_db.clone().transact();

    add_table_db_transaction(tdbt)
}

#[no_mangle]
#[instrument(level = "trace", target = "ffi", skip_all)]
pub extern "C" fn release_table_db_transaction(id: u32) -> i32 {
    let mut tdbts = TABLE_DB_TRANSACTIONS.lock();
    if tdbts.remove(&id).is_none() {
        return 0;
    }
    1
}

fn get_table_db_transaction(id: u32, func_name: &str) -> VeilidAPIResult<TableDBTransaction> {
    let tdbts = TABLE_DB_TRANSACTIONS.lock();
    let Some(tdbt) = tdbts.get(&id) else {
        return VeilidAPIResult::Err(VeilidAPIError::invalid_argument(func_name, "id", id));
    };
    Ok(tdbt.clone())
}

#[no_mangle]
#[instrument(level = "trace", target = "ffi", skip_all)]
pub extern "C" fn table_db_transaction_commit(port: i64, id: u32) {
    DartIsolateWrapper::new(port).spawn_result(
        async move {
            let tdbt = get_table_db_transaction(id, "table_db_transaction_commit")?;

            tdbt.commit().await?;
            VeilidAPIResult::Ok(())
        }
        .in_current_span(),
    );
}
#[no_mangle]
#[instrument(level = "trace", target = "ffi", skip_all)]
pub extern "C" fn table_db_transaction_rollback(port: i64, id: u32) {
    DartIsolateWrapper::new(port).spawn_result(
        async move {
            let tdbt = get_table_db_transaction(id, "table_db_transaction_rollback")?;

            tdbt.rollback();
            VeilidAPIResult::Ok(())
        }
        .in_current_span(),
    );
}

#[no_mangle]
#[instrument(level = "trace", target = "ffi", skip_all)]
pub extern "C" fn table_db_transaction_store(
    port: i64,
    id: u32,
    col: u32,
    key: FfiStr,
    value: FfiStr,
) {
    let key = key.into_opt_string();
    let value = value.into_opt_string();
    DartIsolateWrapper::new(port).spawn_result(
        async move {
            let key = base64_decode_opt(key)?;
            let value = base64_decode_opt(value)?;
            let tdbt = get_table_db_transaction(id, "table_db_transaction_store")?;

            tdbt.store(col, Bytes::from(key), Bytes::from(value))
                .await?;
            VeilidAPIResult::Ok(())
        }
        .in_current_span(),
    );
}

#[no_mangle]
#[instrument(level = "trace", target = "ffi", skip_all)]
pub extern "C" fn table_db_transaction_delete(port: i64, id: u32, col: u32, key: FfiStr) {
    let key = key.into_opt_string();
    DartIsolateWrapper::new(port).spawn_result(
        async move {
            let key = base64_decode_opt(key)?;
            let tdbt = get_table_db_transaction(id, "table_db_transaction_delete")?;

            tdbt.delete(col, Bytes::from(key)).await?;
            VeilidAPIResult::Ok(())
        }
        .in_current_span(),
    );
}

#[no_mangle]
#[instrument(level = "trace", target = "ffi", skip_all)]
pub extern "C" fn table_db_store(port: i64, id: u32, col: u32, key: FfiStr, value: FfiStr) {
    let key = key.into_opt_string();
    let value = value.into_opt_string();
    DartIsolateWrapper::new(port).spawn_result(
        async move {
            let key = base64_decode_opt(key)?;
            let value = base64_decode_opt(value)?;
            let table_db = get_table_db(id, "table_db_store")?;

            table_db
                .store(col, Bytes::from(key), Bytes::from(value))
                .await?;
            VeilidAPIResult::Ok(())
        }
        .in_current_span(),
    );
}

#[no_mangle]
#[instrument(level = "trace", target = "ffi", skip_all)]
pub extern "C" fn table_db_load(port: i64, id: u32, col: u32, key: FfiStr) {
    let key = key.into_opt_string();
    DartIsolateWrapper::new(port).spawn_result(
        async move {
            let key = base64_decode_opt(key)?;
            let table_db = get_table_db(id, "table_db_load")?;

            let out = table_db.load(col, Bytes::from(key)).await?;
            let out = out.map(|x| data_encoding::BASE64URL_NOPAD.encode(&x));
            VeilidAPIResult::Ok(out)
        }
        .in_current_span(),
    );
}

#[no_mangle]
#[instrument(level = "trace", target = "ffi", skip_all)]
pub extern "C" fn table_db_delete(port: i64, id: u32, col: u32, key: FfiStr) {
    let key = key.into_opt_string();
    DartIsolateWrapper::new(port).spawn_result(
        async move {
            let key = base64_decode_opt(key)?;
            let table_db = get_table_db(id, "table_db_delete")?;

            let out = table_db.delete(col, Bytes::from(key)).await?;
            let out = out.map(|x| data_encoding::BASE64URL_NOPAD.encode(&x));
            VeilidAPIResult::Ok(out)
        }
        .in_current_span(),
    );
}

#[no_mangle]
#[instrument(level = "trace", target = "ffi", skip_all)]
pub extern "C" fn valid_crypto_kinds() -> *mut c_char {
    serialize_json(
        VALID_CRYPTO_KINDS
            .iter()
            .map(|k| (*k).into())
            .collect::<Vec<u32>>(),
    )
    .into_ffi_value()
}

#[no_mangle]
#[instrument(level = "trace", target = "ffi", skip_all)]
pub extern "C" fn verify_signatures(port: i64, node_ids: FfiStr, data: FfiStr, signatures: FfiStr) {
    let node_ids = node_ids.into_opt_string();

    let data = data.into_opt_string();

    let typed_signatures = signatures.into_opt_string();

    DartIsolateWrapper::new(port).spawn_result_json(
        async move {
            let node_ids: Vec<PublicKey> = deserialize_opt_json(node_ids)?;
            let typed_signatures: Vec<Signature> = deserialize_opt_json(typed_signatures)?;
            let data = base64_decode_opt(data)?;
            let veilid_api = get_veilid_api().await?;
            let crypto = veilid_api.crypto()?;
            let out = crypto.verify_signatures(&node_ids, &data, &typed_signatures)?;
            VeilidAPIResult::Ok(out)
        }
        .in_current_span(),
    );
}

#[no_mangle]
#[instrument(level = "trace", target = "ffi", skip_all)]
pub extern "C" fn generate_signatures(port: i64, data: FfiStr, key_pairs: FfiStr) {
    let data = data.into_opt_string();

    let key_pairs = key_pairs.into_opt_string();

    DartIsolateWrapper::new(port).spawn_result_json(
        async move {
            let key_pairs: Vec<KeyPair> = deserialize_opt_json(key_pairs)?;
            let data = base64_decode_opt(data)?;
            let veilid_api = get_veilid_api().await?;
            let crypto = veilid_api.crypto()?;
            let out = crypto.generate_signatures(&data, &key_pairs, |_k, s| s)?;
            VeilidAPIResult::Ok(out)
        }
        .in_current_span(),
    );
}

#[no_mangle]
#[instrument(level = "trace", target = "ffi", skip_all)]
pub extern "C" fn generate_key_pair(port: i64, kind: u32) {
    let kind: CryptoKind = CryptoKind::from(kind);

    DartIsolateWrapper::new(port).spawn_result_json(
        async move {
            let out = Crypto::generate_keypair(kind)?;
            VeilidAPIResult::Ok(out)
        }
        .in_current_span(),
    );
}

#[no_mangle]
pub extern "C" fn crypto_cached_dh(port: i64, kind: u32, key: FfiStr, secret: FfiStr) {
    let kind: CryptoKind = CryptoKind::from(kind);

    let key = key.into_opt_string();
    let secret = secret.into_opt_string();

    DartIsolateWrapper::new(port).spawn_result_json(
        async move {
            let key: PublicKey = deserialize_opt_json(key)?;
            let secret: SecretKey = deserialize_opt_json(secret)?;
            let veilid_api = get_veilid_api().await?;
            let crypto = veilid_api.crypto()?;
            let csv = crypto.get(kind).ok_or_else(|| {
                VeilidAPIError::invalid_argument("crypto_cached_dh", "kind", kind.to_string())
            })?;
            let out = csv.cached_dh(&key, &secret)?;
            VeilidAPIResult::Ok(out)
        }
        .in_current_span(),
    );
}

#[no_mangle]
#[instrument(level = "trace", target = "ffi", skip_all)]
pub extern "C" fn crypto_compute_dh(port: i64, kind: u32, key: FfiStr, secret: FfiStr) {
    let kind: CryptoKind = CryptoKind::from(kind);

    let key = key.into_opt_string();
    let secret = secret.into_opt_string();

    DartIsolateWrapper::new(port).spawn_result_json(
        async move {
            let key: PublicKey = deserialize_opt_json(key)?;
            let secret: SecretKey = deserialize_opt_json(secret)?;
            let veilid_api = get_veilid_api().await?;
            let crypto = veilid_api.crypto()?;
            let csv = crypto.get(kind).ok_or_else(|| {
                VeilidAPIError::invalid_argument("crypto_compute_dh", "kind", kind.to_string())
            })?;
            let out = csv.compute_dh(&key, &secret)?;
            VeilidAPIResult::Ok(out)
        }
        .in_current_span(),
    );
}

#[no_mangle]
#[instrument(level = "trace", target = "ffi", skip_all)]
pub extern "C" fn crypto_generate_shared_secret(
    port: i64,
    kind: u32,
    key: FfiStr,
    secret: FfiStr,
    domain: FfiStr,
) {
    let kind: CryptoKind = CryptoKind::from(kind);

    let key = key.into_opt_string();
    let secret = secret.into_opt_string();
    let domain = domain.into_opt_string();

    DartIsolateWrapper::new(port).spawn_result_json(
        async move {
            let key: PublicKey = deserialize_opt_json(key)?;
            let secret: SecretKey = deserialize_opt_json(secret)?;
            let domain = base64_decode_opt(domain)?;
            let veilid_api = get_veilid_api().await?;
            let crypto = veilid_api.crypto()?;
            let csv = crypto.get(kind).ok_or_else(|| {
                VeilidAPIError::invalid_argument(
                    "crypto_generate_shared_secret",
                    "kind",
                    kind.to_string(),
                )
            })?;
            let out = csv.generate_shared_secret(&key, &secret, &domain)?;
            VeilidAPIResult::Ok(out)
        }
        .in_current_span(),
    );
}

#[no_mangle]
#[instrument(level = "trace", target = "ffi", skip_all)]
pub extern "C" fn crypto_hpke_seal(
    port: i64,
    kind: u32,
    recipient: FfiStr,
    aad: FfiStr,
    plaintext: FfiStr,
) {
    let kind: CryptoKind = CryptoKind::from(kind);

    let recipient = recipient.into_opt_string();
    let aad = aad.into_opt_string();
    let plaintext = plaintext.into_opt_string();

    DartIsolateWrapper::new(port).spawn_result(
        async move {
            let recipient: EncapsulationKey = deserialize_opt_json(recipient)?;
            let aad = base64_decode_opt(aad)?;
            let plaintext = base64_decode_opt(plaintext)?;
            let veilid_api = get_veilid_api().await?;
            let crypto = veilid_api.crypto()?;
            let csv = crypto.get(kind).ok_or_else(|| {
                VeilidAPIError::invalid_argument("crypto_hpke_seal", "kind", kind.to_string())
            })?;
            let out = csv.hpke_seal(&recipient, &aad, &plaintext)?;
            let out = data_encoding::BASE64URL_NOPAD.encode(&out);
            VeilidAPIResult::Ok(out)
        }
        .in_current_span(),
    );
}

#[no_mangle]
#[instrument(level = "trace", target = "ffi", skip_all)]
pub extern "C" fn crypto_hpke_open(
    port: i64,
    kind: u32,
    secret: FfiStr,
    aad: FfiStr,
    sealed: FfiStr,
) {
    let kind: CryptoKind = CryptoKind::from(kind);

    let secret = secret.into_opt_string();
    let aad = aad.into_opt_string();
    let sealed = sealed.into_opt_string();

    DartIsolateWrapper::new(port).spawn_result(
        async move {
            let secret: DecapsulationKey = deserialize_opt_json(secret)?;
            let aad = base64_decode_opt(aad)?;
            let sealed = base64_decode_opt(sealed)?;
            let veilid_api = get_veilid_api().await?;
            let crypto = veilid_api.crypto()?;
            let csv = crypto.get(kind).ok_or_else(|| {
                VeilidAPIError::invalid_argument("crypto_hpke_open", "kind", kind.to_string())
            })?;
            let out = csv.hpke_open(&secret, &aad, &sealed)?;
            let out = data_encoding::BASE64URL_NOPAD.encode(&out);
            VeilidAPIResult::Ok(out)
        }
        .in_current_span(),
    );
}

#[no_mangle]
#[instrument(level = "trace", target = "ffi", skip_all)]
pub extern "C" fn crypto_random_bytes(port: i64, kind: u32, len: u32) {
    let kind: CryptoKind = CryptoKind::from(kind);

    DartIsolateWrapper::new(port).spawn_result(
        async move {
            let veilid_api = get_veilid_api().await?;
            let crypto = veilid_api.crypto()?;
            let csv = crypto.get(kind).ok_or_else(|| {
                VeilidAPIError::invalid_argument("crypto_random_bytes", "kind", kind.to_string())
            })?;
            let out = csv.random_bytes(len as usize);
            let out = data_encoding::BASE64URL_NOPAD.encode(&out);
            VeilidAPIResult::Ok(out)
        }
        .in_current_span(),
    );
}

#[no_mangle]
#[instrument(level = "trace", target = "ffi", skip_all)]
pub extern "C" fn crypto_shared_secret_length(port: i64, kind: u32) {
    let kind: CryptoKind = CryptoKind::from(kind);

    DartIsolateWrapper::new(port).spawn_result(
        async move {
            let veilid_api = get_veilid_api().await?;
            let crypto = veilid_api.crypto()?;
            let csv = crypto.get(kind).ok_or_else(|| {
                VeilidAPIError::invalid_argument(
                    "crypto_shared_secret_length",
                    "kind",
                    kind.to_string(),
                )
            })?;
            let out = csv.shared_secret_length();
            VeilidAPIResult::Ok(out)
        }
        .in_current_span(),
    );
}

#[no_mangle]
#[instrument(level = "trace", target = "ffi", skip_all)]
pub extern "C" fn crypto_nonce_length(port: i64, kind: u32) {
    let kind: CryptoKind = CryptoKind::from(kind);

    DartIsolateWrapper::new(port).spawn_result(
        async move {
            let veilid_api = get_veilid_api().await?;
            let crypto = veilid_api.crypto()?;
            let csv = crypto.get(kind).ok_or_else(|| {
                VeilidAPIError::invalid_argument("crypto_nonce_length", "kind", kind.to_string())
            })?;
            let out = csv.nonce_length();
            VeilidAPIResult::Ok(out)
        }
        .in_current_span(),
    );
}

#[no_mangle]
#[instrument(level = "trace", target = "ffi", skip_all)]
pub extern "C" fn crypto_hash_digest_length(port: i64, kind: u32) {
    let kind: CryptoKind = CryptoKind::from(kind);

    DartIsolateWrapper::new(port).spawn_result(
        async move {
            let veilid_api = get_veilid_api().await?;
            let crypto = veilid_api.crypto()?;
            let csv = crypto.get(kind).ok_or_else(|| {
                VeilidAPIError::invalid_argument(
                    "crypto_hash_digest_length",
                    "kind",
                    kind.to_string(),
                )
            })?;
            let out = csv.hash_digest_length();
            VeilidAPIResult::Ok(out)
        }
        .in_current_span(),
    );
}

#[no_mangle]
#[instrument(level = "trace", target = "ffi", skip_all)]
pub extern "C" fn crypto_public_key_length(port: i64, kind: u32) {
    let kind: CryptoKind = CryptoKind::from(kind);

    DartIsolateWrapper::new(port).spawn_result(
        async move {
            let veilid_api = get_veilid_api().await?;
            let crypto = veilid_api.crypto()?;
            let csv = crypto.get(kind).ok_or_else(|| {
                VeilidAPIError::invalid_argument(
                    "crypto_public_key_length",
                    "kind",
                    kind.to_string(),
                )
            })?;
            let out = csv.public_key_length();
            VeilidAPIResult::Ok(out)
        }
        .in_current_span(),
    );
}

#[no_mangle]
#[instrument(level = "trace", target = "ffi", skip_all)]
pub extern "C" fn crypto_secret_key_length(port: i64, kind: u32) {
    let kind: CryptoKind = CryptoKind::from(kind);

    DartIsolateWrapper::new(port).spawn_result(
        async move {
            let veilid_api = get_veilid_api().await?;
            let crypto = veilid_api.crypto()?;
            let csv = crypto.get(kind).ok_or_else(|| {
                VeilidAPIError::invalid_argument(
                    "crypto_secret_key_length",
                    "kind",
                    kind.to_string(),
                )
            })?;
            let out = csv.secret_key_length();
            VeilidAPIResult::Ok(out)
        }
        .in_current_span(),
    );
}

#[no_mangle]
#[instrument(level = "trace", target = "ffi", skip_all)]
pub extern "C" fn crypto_encapsulation_key_length(port: i64, kind: u32) {
    let kind: CryptoKind = CryptoKind::from(kind);

    DartIsolateWrapper::new(port).spawn_result(
        async move {
            let veilid_api = get_veilid_api().await?;
            let crypto = veilid_api.crypto()?;
            let csv = crypto.get(kind).ok_or_else(|| {
                VeilidAPIError::invalid_argument(
                    "crypto_encapsulation_key_length",
                    "kind",
                    kind.to_string(),
                )
            })?;
            let out = csv.encapsulation_key_length();
            VeilidAPIResult::Ok(out)
        }
        .in_current_span(),
    );
}

#[no_mangle]
#[instrument(level = "trace", target = "ffi", skip_all)]
pub extern "C" fn crypto_decapsulation_key_length(port: i64, kind: u32) {
    let kind: CryptoKind = CryptoKind::from(kind);

    DartIsolateWrapper::new(port).spawn_result(
        async move {
            let veilid_api = get_veilid_api().await?;
            let crypto = veilid_api.crypto()?;
            let csv = crypto.get(kind).ok_or_else(|| {
                VeilidAPIError::invalid_argument(
                    "crypto_decapsulation_key_length",
                    "kind",
                    kind.to_string(),
                )
            })?;
            let out = csv.decapsulation_key_length();
            VeilidAPIResult::Ok(out)
        }
        .in_current_span(),
    );
}

#[no_mangle]
#[instrument(level = "trace", target = "ffi", skip_all)]
pub extern "C" fn crypto_signature_length(port: i64, kind: u32) {
    let kind: CryptoKind = CryptoKind::from(kind);

    DartIsolateWrapper::new(port).spawn_result(
        async move {
            let veilid_api = get_veilid_api().await?;
            let crypto = veilid_api.crypto()?;
            let csv = crypto.get(kind).ok_or_else(|| {
                VeilidAPIError::invalid_argument(
                    "crypto_signature_length",
                    "kind",
                    kind.to_string(),
                )
            })?;
            let out = csv.signature_length();
            VeilidAPIResult::Ok(out)
        }
        .in_current_span(),
    );
}

#[no_mangle]
#[instrument(level = "trace", target = "ffi", skip_all)]
pub extern "C" fn crypto_default_salt_length(port: i64, kind: u32) {
    let kind: CryptoKind = CryptoKind::from(kind);

    DartIsolateWrapper::new(port).spawn_result(
        async move {
            let veilid_api = get_veilid_api().await?;
            let crypto = veilid_api.crypto()?;
            let csv = crypto.get(kind).ok_or_else(|| {
                VeilidAPIError::invalid_argument(
                    "crypto_default_salt_length",
                    "kind",
                    kind.to_string(),
                )
            })?;
            let out = csv.default_salt_length();
            VeilidAPIResult::Ok(out)
        }
        .in_current_span(),
    );
}

#[no_mangle]
#[instrument(level = "trace", target = "ffi", skip_all)]
pub extern "C" fn crypto_aead_overhead(port: i64, kind: u32) {
    let kind: CryptoKind = CryptoKind::from(kind);

    DartIsolateWrapper::new(port).spawn_result(
        async move {
            let veilid_api = get_veilid_api().await?;
            let crypto = veilid_api.crypto()?;
            let csv = crypto.get(kind).ok_or_else(|| {
                VeilidAPIError::invalid_argument("crypto_aead_overhead", "kind", kind.to_string())
            })?;
            let out = csv.aead_overhead();
            VeilidAPIResult::Ok(out)
        }
        .in_current_span(),
    );
}

#[no_mangle]
#[instrument(level = "trace", target = "ffi", skip_all)]
pub extern "C" fn crypto_check_shared_secret(port: i64, kind: u32, secret: FfiStr) {
    let kind: CryptoKind = CryptoKind::from(kind);
    let secret = secret.into_opt_string();

    DartIsolateWrapper::new(port).spawn_result(
        async move {
            let secret: SharedSecret = deserialize_opt_json(secret)?;
            let veilid_api = get_veilid_api().await?;
            let crypto = veilid_api.crypto()?;
            let csv = crypto.get(kind).ok_or_else(|| {
                VeilidAPIError::invalid_argument(
                    "crypto_check_shared_secret",
                    "kind",
                    kind.to_string(),
                )
            })?;
            csv.check_shared_secret(&secret)?;
            VeilidAPIResult::Ok(())
        }
        .in_current_span(),
    );
}

#[no_mangle]
#[instrument(level = "trace", target = "ffi", skip_all)]
pub extern "C" fn crypto_check_nonce(port: i64, kind: u32, nonce: FfiStr) {
    let kind: CryptoKind = CryptoKind::from(kind);
    let nonce = nonce.into_opt_string();

    DartIsolateWrapper::new(port).spawn_result(
        async move {
            let nonce: Nonce = deserialize_opt_json(nonce)?;
            let veilid_api = get_veilid_api().await?;
            let crypto = veilid_api.crypto()?;
            let csv = crypto.get(kind).ok_or_else(|| {
                VeilidAPIError::invalid_argument("crypto_check_nonce", "kind", kind.to_string())
            })?;
            csv.check_nonce(&nonce)?;
            VeilidAPIResult::Ok(())
        }
        .in_current_span(),
    );
}

#[no_mangle]
#[instrument(level = "trace", target = "ffi", skip_all)]
pub extern "C" fn crypto_check_hash_digest(port: i64, kind: u32, digest: FfiStr) {
    let kind: CryptoKind = CryptoKind::from(kind);
    let digest = digest.into_opt_string();

    DartIsolateWrapper::new(port).spawn_result(
        async move {
            let digest: HashDigest = deserialize_opt_json(digest)?;
            let veilid_api = get_veilid_api().await?;
            let crypto = veilid_api.crypto()?;
            let csv = crypto.get(kind).ok_or_else(|| {
                VeilidAPIError::invalid_argument(
                    "crypto_check_hash_digest",
                    "kind",
                    kind.to_string(),
                )
            })?;
            csv.check_hash_digest(&digest)?;
            VeilidAPIResult::Ok(())
        }
        .in_current_span(),
    );
}

#[no_mangle]
#[instrument(level = "trace", target = "ffi", skip_all)]
pub extern "C" fn crypto_check_public_key(port: i64, kind: u32, key: FfiStr) {
    let kind: CryptoKind = CryptoKind::from(kind);
    let key = key.into_opt_string();

    DartIsolateWrapper::new(port).spawn_result(
        async move {
            let key: PublicKey = deserialize_opt_json(key)?;
            let veilid_api = get_veilid_api().await?;
            let crypto = veilid_api.crypto()?;
            let csv = crypto.get(kind).ok_or_else(|| {
                VeilidAPIError::invalid_argument(
                    "crypto_check_public_key",
                    "kind",
                    kind.to_string(),
                )
            })?;
            csv.check_public_key(&key)?;
            VeilidAPIResult::Ok(())
        }
        .in_current_span(),
    );
}

#[no_mangle]
#[instrument(level = "trace", target = "ffi", skip_all)]
pub extern "C" fn crypto_check_secret_key(port: i64, kind: u32, key: FfiStr) {
    let kind: CryptoKind = CryptoKind::from(kind);
    let key = key.into_opt_string();

    DartIsolateWrapper::new(port).spawn_result(
        async move {
            let key: SecretKey = deserialize_opt_json(key)?;
            let veilid_api = get_veilid_api().await?;
            let crypto = veilid_api.crypto()?;
            let csv = crypto.get(kind).ok_or_else(|| {
                VeilidAPIError::invalid_argument(
                    "crypto_check_secret_key",
                    "kind",
                    kind.to_string(),
                )
            })?;
            csv.check_secret_key(&key)?;
            VeilidAPIResult::Ok(())
        }
        .in_current_span(),
    );
}

#[no_mangle]
#[instrument(level = "trace", target = "ffi", skip_all)]
pub extern "C" fn crypto_check_signature(port: i64, kind: u32, signature: FfiStr) {
    let kind: CryptoKind = CryptoKind::from(kind);
    let signature = signature.into_opt_string();

    DartIsolateWrapper::new(port).spawn_result(
        async move {
            let signature: Signature = deserialize_opt_json(signature)?;
            let veilid_api = get_veilid_api().await?;
            let crypto = veilid_api.crypto()?;
            let csv = crypto.get(kind).ok_or_else(|| {
                VeilidAPIError::invalid_argument("crypto_check_signature", "kind", kind.to_string())
            })?;
            csv.check_signature(&signature)?;
            VeilidAPIResult::Ok(())
        }
        .in_current_span(),
    );
}

#[no_mangle]
#[instrument(level = "trace", target = "ffi", skip_all)]
pub extern "C" fn crypto_hash_password(port: i64, kind: u32, password: FfiStr, salt: FfiStr) {
    let kind: CryptoKind = CryptoKind::from(kind);
    let password = password.into_opt_string();
    let salt = salt.into_opt_string();

    DartIsolateWrapper::new(port).spawn_result(
        async move {
            let password = base64_decode_opt(password)?;
            let salt = base64_decode_opt(salt)?;
            let veilid_api = get_veilid_api().await?;
            let crypto = veilid_api.crypto()?;
            let csv = crypto.get(kind).ok_or_else(|| {
                VeilidAPIError::invalid_argument("crypto_hash_password", "kind", kind.to_string())
            })?;
            let out = csv.hash_password(&password, &salt)?;
            VeilidAPIResult::Ok(out)
        }
        .in_current_span(),
    );
}

#[no_mangle]
#[instrument(level = "trace", target = "ffi", skip_all)]
pub extern "C" fn crypto_verify_password(
    port: i64,
    kind: u32,
    password: FfiStr,
    password_hash: FfiStr,
) {
    let kind: CryptoKind = CryptoKind::from(kind);
    let password = password.into_opt_string();
    let password_hash = password_hash.into_opt_string();

    DartIsolateWrapper::new(port).spawn_result(
        async move {
            let password = base64_decode_opt(password)?;
            let password_hash = password_hash.ok_or_else(|| {
                VeilidAPIError::missing_argument("crypto_verify_password", "password_hash")
            })?;
            let veilid_api = get_veilid_api().await?;
            let crypto = veilid_api.crypto()?;
            let csv = crypto.get(kind).ok_or_else(|| {
                VeilidAPIError::invalid_argument("crypto_verify_password", "kind", kind.to_string())
            })?;
            let out = csv.verify_password(&password, &password_hash)?;
            VeilidAPIResult::Ok(out)
        }
        .in_current_span(),
    );
}

#[no_mangle]
#[instrument(level = "trace", target = "ffi", skip_all)]
pub extern "C" fn crypto_derive_shared_secret(
    port: i64,
    kind: u32,
    password: FfiStr,
    salt: FfiStr,
) {
    let kind: CryptoKind = CryptoKind::from(kind);
    let password = password.into_opt_string();
    let salt = salt.into_opt_string();

    DartIsolateWrapper::new(port).spawn_result_json(
        async move {
            let password = base64_decode_opt(password)?;
            let salt = base64_decode_opt(salt)?;
            let veilid_api = get_veilid_api().await?;
            let crypto = veilid_api.crypto()?;
            let csv = crypto.get(kind).ok_or_else(|| {
                VeilidAPIError::invalid_argument(
                    "crypto_derive_shared_secret",
                    "kind",
                    kind.to_string(),
                )
            })?;
            let out = csv.derive_shared_secret(&password, &salt)?;
            VeilidAPIResult::Ok(out)
        }
        .in_current_span(),
    );
}

#[no_mangle]
#[instrument(level = "trace", target = "ffi", skip_all)]
pub extern "C" fn crypto_random_nonce(port: i64, kind: u32) {
    let kind: CryptoKind = CryptoKind::from(kind);

    DartIsolateWrapper::new(port).spawn_result_json(
        async move {
            let veilid_api = get_veilid_api().await?;
            let crypto = veilid_api.crypto()?;
            let csv = crypto.get(kind).ok_or_else(|| {
                VeilidAPIError::invalid_argument("crypto_random_nonce", "kind", kind.to_string())
            })?;
            let out = csv.random_nonce();
            VeilidAPIResult::Ok(out)
        }
        .in_current_span(),
    );
}

#[no_mangle]
#[instrument(level = "trace", target = "ffi", skip_all)]
pub extern "C" fn crypto_random_shared_secret(port: i64, kind: u32) {
    let kind: CryptoKind = CryptoKind::from(kind);

    DartIsolateWrapper::new(port).spawn_result_json(
        async move {
            let veilid_api = get_veilid_api().await?;
            let crypto = veilid_api.crypto()?;
            let csv = crypto.get(kind).ok_or_else(|| {
                VeilidAPIError::invalid_argument(
                    "crypto_random_shared_secret",
                    "kind",
                    kind.to_string(),
                )
            })?;
            let out = csv.random_shared_secret();
            VeilidAPIResult::Ok(out)
        }
        .in_current_span(),
    );
}

#[no_mangle]
#[instrument(level = "trace", target = "ffi", skip_all)]
pub extern "C" fn crypto_generate_key_pair(port: i64, kind: u32) {
    let kind: CryptoKind = CryptoKind::from(kind);

    DartIsolateWrapper::new(port).spawn_result_json(
        async move {
            let veilid_api = get_veilid_api().await?;
            let crypto = veilid_api.crypto()?;
            let csv = crypto.get(kind).ok_or_else(|| {
                VeilidAPIError::invalid_argument(
                    "crypto_generate_key_pair",
                    "kind",
                    kind.to_string(),
                )
            })?;
            let out = csv.generate_keypair();
            VeilidAPIResult::Ok(out)
        }
        .in_current_span(),
    );
}

#[no_mangle]
#[instrument(level = "trace", target = "ffi", skip_all)]
pub extern "C" fn crypto_generate_kem_key_pair(port: i64, kind: u32) {
    let kind: CryptoKind = CryptoKind::from(kind);

    DartIsolateWrapper::new(port).spawn_result_json(
        async move {
            let veilid_api = get_veilid_api().await?;
            let crypto = veilid_api.crypto()?;
            let csv = crypto.get(kind).ok_or_else(|| {
                VeilidAPIError::invalid_argument(
                    "crypto_generate_kem_key_pair",
                    "kind",
                    kind.to_string(),
                )
            })?;
            let out = csv.generate_kem_keypair();
            VeilidAPIResult::Ok(out)
        }
        .in_current_span(),
    );
}

#[no_mangle]
#[instrument(level = "trace", target = "ffi", skip_all)]
pub extern "C" fn crypto_encapsulation_key_from_signing_key(port: i64, kind: u32, key: FfiStr) {
    let kind: CryptoKind = CryptoKind::from(kind);

    let key = key.into_opt_string();

    DartIsolateWrapper::new(port).spawn_result_json(
        async move {
            let key: PublicKey = deserialize_opt_json(key)?;
            let veilid_api = get_veilid_api().await?;
            let crypto = veilid_api.crypto()?;
            let csv = crypto.get(kind).ok_or_else(|| {
                VeilidAPIError::invalid_argument(
                    "crypto_encapsulation_key_from_signing_key",
                    "kind",
                    kind.to_string(),
                )
            })?;
            let out = csv.encapsulation_key_from_signing_key(&key)?;
            VeilidAPIResult::Ok(out)
        }
        .in_current_span(),
    );
}

#[no_mangle]
#[instrument(level = "trace", target = "ffi", skip_all)]
pub extern "C" fn crypto_decapsulation_key_from_signing_secret(
    port: i64,
    kind: u32,
    secret: FfiStr,
) {
    let kind: CryptoKind = CryptoKind::from(kind);

    let secret = secret.into_opt_string();

    DartIsolateWrapper::new(port).spawn_result_json(
        async move {
            let secret: SecretKey = deserialize_opt_json(secret)?;
            let veilid_api = get_veilid_api().await?;
            let crypto = veilid_api.crypto()?;
            let csv = crypto.get(kind).ok_or_else(|| {
                VeilidAPIError::invalid_argument(
                    "crypto_decapsulation_key_from_signing_secret",
                    "kind",
                    kind.to_string(),
                )
            })?;
            let out = csv.decapsulation_key_from_signing_secret(&secret)?;
            VeilidAPIResult::Ok(out)
        }
        .in_current_span(),
    );
}

#[no_mangle]
#[instrument(level = "trace", target = "ffi", skip_all)]
pub extern "C" fn crypto_generate_hash(port: i64, kind: u32, data: FfiStr) {
    let kind: CryptoKind = CryptoKind::from(kind);

    let data = data.into_opt_string();

    DartIsolateWrapper::new(port).spawn_result_json(
        async move {
            let data = base64_decode_opt(data)?;
            let veilid_api = get_veilid_api().await?;
            let crypto = veilid_api.crypto()?;
            let csv = crypto.get(kind).ok_or_else(|| {
                VeilidAPIError::invalid_argument("crypto_generate_hash", "kind", kind.to_string())
            })?;
            let out = csv.generate_hash(&data);
            VeilidAPIResult::Ok(out)
        }
        .in_current_span(),
    );
}

#[no_mangle]
#[instrument(level = "trace", target = "ffi", skip_all)]
pub extern "C" fn crypto_validate_key_pair(port: i64, kind: u32, key: FfiStr, secret: FfiStr) {
    let kind: CryptoKind = CryptoKind::from(kind);

    let key = key.into_opt_string();
    let secret = secret.into_opt_string();

    DartIsolateWrapper::new(port).spawn_result(
        async move {
            let key: PublicKey = deserialize_opt_json(key)?;
            let secret: SecretKey = deserialize_opt_json(secret)?;
            let veilid_api = get_veilid_api().await?;
            let crypto = veilid_api.crypto()?;
            let csv = crypto.get(kind).ok_or_else(|| {
                VeilidAPIError::invalid_argument(
                    "crypto_validate_key_pair",
                    "kind",
                    kind.to_string(),
                )
            })?;
            let out = csv.validate_keypair(&key, &secret);
            VeilidAPIResult::Ok(out)
        }
        .in_current_span(),
    );
}

#[no_mangle]
#[instrument(level = "trace", target = "ffi", skip_all)]
pub extern "C" fn crypto_validate_hash(port: i64, kind: u32, data: FfiStr, hash: FfiStr) {
    let kind: CryptoKind = CryptoKind::from(kind);

    let data = data.into_opt_string();

    let hash = hash.into_opt_string();

    DartIsolateWrapper::new(port).spawn_result(
        async move {
            let hash: HashDigest = deserialize_opt_json(hash)?;
            let data = base64_decode_opt(data)?;
            let veilid_api = get_veilid_api().await?;
            let crypto = veilid_api.crypto()?;
            let csv = crypto.get(kind).ok_or_else(|| {
                VeilidAPIError::invalid_argument("crypto_validate_hash", "kind", kind.to_string())
            })?;
            let out = csv.validate_hash(&data, &hash);
            VeilidAPIResult::Ok(out)
        }
        .in_current_span(),
    );
}

#[no_mangle]
#[instrument(level = "trace", target = "ffi", skip_all)]
pub extern "C" fn crypto_sign(port: i64, kind: u32, key: FfiStr, secret: FfiStr, data: FfiStr) {
    let kind: CryptoKind = CryptoKind::from(kind);

    let key = key.into_opt_string();
    let secret = secret.into_opt_string();
    let data = data.into_opt_string();

    DartIsolateWrapper::new(port).spawn_result_json(
        async move {
            let key: PublicKey = deserialize_opt_json(key)?;
            let secret: SecretKey = deserialize_opt_json(secret)?;
            let data = base64_decode_opt(data)?;
            let veilid_api = get_veilid_api().await?;
            let crypto = veilid_api.crypto()?;
            let csv = crypto.get(kind).ok_or_else(|| {
                VeilidAPIError::invalid_argument("crypto_sign", "kind", kind.to_string())
            })?;
            let out = csv.sign(&key, &secret, &data)?;
            VeilidAPIResult::Ok(out)
        }
        .in_current_span(),
    );
}

#[no_mangle]
#[instrument(level = "trace", target = "ffi", skip_all)]
pub extern "C" fn crypto_verify(
    port: i64,
    kind: u32,
    key: FfiStr,
    data: FfiStr,
    signature: FfiStr,
) {
    let kind: CryptoKind = CryptoKind::from(kind);

    let key = key.into_opt_string();
    let data = data.into_opt_string();
    let signature = signature.into_opt_string();

    DartIsolateWrapper::new(port).spawn_result(
        async move {
            let key: PublicKey = deserialize_opt_json(key)?;
            let signature: Signature = deserialize_opt_json(signature)?;
            let data = base64_decode_opt(data)?;
            let veilid_api = get_veilid_api().await?;
            let crypto = veilid_api.crypto()?;
            let csv = crypto.get(kind).ok_or_else(|| {
                VeilidAPIError::invalid_argument("crypto_verify", "kind", kind.to_string())
            })?;
            let out = csv.verify(&key, &data, &signature)?;
            VeilidAPIResult::Ok(out)
        }
        .in_current_span(),
    );
}

#[no_mangle]
#[instrument(level = "trace", target = "ffi", skip_all)]
pub extern "C" fn crypto_decrypt_aead(
    port: i64,
    kind: u32,
    body: FfiStr,
    nonce: FfiStr,
    shared_secret: FfiStr,
    associated_data: FfiStr,
) {
    let kind: CryptoKind = CryptoKind::from(kind);

    let body = body.into_opt_string();

    let nonce = nonce.into_opt_string();

    let shared_secret = shared_secret.into_opt_string();

    let associated_data = associated_data.into_opt_string();

    DartIsolateWrapper::new(port).spawn_result(
        async move {
            let nonce: Nonce = deserialize_opt_json(nonce)?;
            let shared_secret: SharedSecret = deserialize_opt_json(shared_secret)?;
            let body = base64_decode_opt(body)?;
            let associated_data = associated_data.map(|s| base64_decode(&s)).transpose()?;
            let veilid_api = get_veilid_api().await?;
            let crypto = veilid_api.crypto()?;
            let csv = crypto.get(kind).ok_or_else(|| {
                VeilidAPIError::invalid_argument("crypto_decrypt_aead", "kind", kind.to_string())
            })?;
            let out = csv.decrypt_aead(
                &body,
                &nonce,
                &shared_secret,
                match &associated_data {
                    Some(ad) => Some(ad.as_slice()),
                    None => None,
                },
            )?;
            let out = data_encoding::BASE64URL_NOPAD.encode(&out);
            VeilidAPIResult::Ok(out)
        }
        .in_current_span(),
    );
}

#[no_mangle]
#[instrument(level = "trace", target = "ffi", skip_all)]
pub extern "C" fn crypto_encrypt_aead(
    port: i64,
    kind: u32,
    body: FfiStr,
    nonce: FfiStr,
    shared_secret: FfiStr,
    associated_data: FfiStr,
) {
    let kind: CryptoKind = CryptoKind::from(kind);

    let body = body.into_opt_string();

    let nonce = nonce.into_opt_string();

    let shared_secret = shared_secret.into_opt_string();

    let associated_data = associated_data.into_opt_string();

    DartIsolateWrapper::new(port).spawn_result(
        async move {
            let nonce: Nonce = deserialize_opt_json(nonce)?;
            let shared_secret: SharedSecret = deserialize_opt_json(shared_secret)?;
            let body = base64_decode_opt(body)?;
            let associated_data = associated_data.map(|s| base64_decode(&s)).transpose()?;
            let veilid_api = get_veilid_api().await?;
            let crypto = veilid_api.crypto()?;
            let csv = crypto.get(kind).ok_or_else(|| {
                VeilidAPIError::invalid_argument("crypto_encrypt_aead", "kind", kind.to_string())
            })?;
            let out = csv.encrypt_aead(
                &body,
                &nonce,
                &shared_secret,
                match &associated_data {
                    Some(ad) => Some(ad.as_slice()),
                    None => None,
                },
            )?;
            let out = data_encoding::BASE64URL_NOPAD.encode(&out);
            VeilidAPIResult::Ok(out)
        }
        .in_current_span(),
    );
}

#[no_mangle]
#[instrument(level = "trace", target = "ffi", skip_all)]
pub extern "C" fn crypto_crypt_no_auth(
    port: i64,
    kind: u32,
    body: FfiStr,
    nonce: FfiStr,
    shared_secret: FfiStr,
) {
    let kind: CryptoKind = CryptoKind::from(kind);

    let body = body.into_opt_string();

    let nonce = nonce.into_opt_string();

    let shared_secret = shared_secret.into_opt_string();

    DartIsolateWrapper::new(port).spawn_result(
        async move {
            let nonce: Nonce = deserialize_opt_json(nonce)?;
            let shared_secret: SharedSecret = deserialize_opt_json(shared_secret)?;
            let mut body = base64_decode_opt(body)?;
            let veilid_api = get_veilid_api().await?;
            let crypto = veilid_api.crypto()?;
            let csv = crypto.get(kind).ok_or_else(|| {
                VeilidAPIError::invalid_argument("crypto_crypt_no_auth", "kind", kind.to_string())
            })?;
            csv.crypt_in_place_no_auth(&mut body, &nonce, &shared_secret)?;
            let body = data_encoding::BASE64URL_NOPAD.encode(&body);
            VeilidAPIResult::Ok(body)
        }
        .in_current_span(),
    );
}

#[no_mangle]
#[instrument(level = "trace", target = "ffi", skip_all)]
pub extern "C" fn now() -> u64 {
    Timestamp::now().as_u64()
}

#[no_mangle]
#[instrument(level = "trace", target = "ffi", skip_all)]
pub extern "C" fn now_non_decreasing() -> u64 {
    Timestamp::now_non_decreasing().as_u64()
}

#[no_mangle]
#[instrument(level = "trace", target = "ffi", skip_all)]
pub extern "C" fn now_increasing() -> u64 {
    Timestamp::now_increasing().as_u64()
}

#[no_mangle]
#[instrument(level = "trace", target = "ffi", skip_all)]
pub extern "C" fn debug(port: i64, command: FfiStr) {
    let command = command.into_opt_string().unwrap_or_default();
    DartIsolateWrapper::new(port).spawn_result(
        async move {
            let veilid_api = get_veilid_api().await?;
            let out = veilid_api.debug(command).await?;
            VeilidAPIResult::Ok(out)
        }
        .in_current_span(),
    );
}

#[no_mangle]
#[instrument(level = "trace", target = "ffi", skip_all)]
pub extern "C" fn save_user_secret(port: i64, key: FfiStr, value: FfiStr) {
    let key = key.into_opt_string().unwrap_or_default();
    let value = value.into_opt_string();
    DartIsolateWrapper::new(port).spawn_result(
        async move {
            let value = base64_decode_opt(value)?;
            let veilid_api = get_veilid_api().await?;
            let out = veilid_api.save_user_secret(key, value).await?;
            VeilidAPIResult::Ok(out)
        }
        .in_current_span(),
    );
}

#[no_mangle]
#[instrument(level = "trace", target = "ffi", skip_all)]
pub extern "C" fn load_user_secret(port: i64, key: FfiStr) {
    let key = key.into_opt_string().unwrap_or_default();
    DartIsolateWrapper::new(port).spawn_result(
        async move {
            let veilid_api = get_veilid_api().await?;
            let out = veilid_api
                .load_user_secret(key)
                .await?
                .map(|x| BASE64URL_NOPAD.encode(&x));
            VeilidAPIResult::Ok(out)
        }
        .in_current_span(),
    );
}

#[no_mangle]
#[instrument(level = "trace", target = "ffi", skip_all)]
pub extern "C" fn remove_user_secret(port: i64, key: FfiStr) {
    let key = key.into_opt_string().unwrap_or_default();
    DartIsolateWrapper::new(port).spawn_result(
        async move {
            let veilid_api = get_veilid_api().await?;
            let out = veilid_api.remove_user_secret(key).await?;
            VeilidAPIResult::Ok(out)
        }
        .in_current_span(),
    );
}

#[no_mangle]
#[instrument(level = "trace", target = "ffi", skip_all)]
pub extern "C" fn list_user_secrets(port: i64) {
    DartIsolateWrapper::new(port).spawn_result_json(
        async move {
            let veilid_api = get_veilid_api().await?;
            let out = veilid_api.list_user_secrets().await?;
            VeilidAPIResult::Ok(out)
        }
        .in_current_span(),
    );
}

#[no_mangle]
#[instrument(level = "trace", target = "ffi", skip_all)]
pub extern "C" fn delete_all_user_secrets(port: i64) {
    DartIsolateWrapper::new(port).spawn_result(
        async move {
            let veilid_api = get_veilid_api().await?;
            veilid_api.delete_all_user_secrets().await?;
            VeilidAPIResult::Ok(())
        }
        .in_current_span(),
    );
}

#[no_mangle]
#[instrument(level = "trace", target = "ffi", skip_all)]
pub extern "C" fn veilid_version_string() -> *mut c_char {
    veilid_core::veilid_version_string().into_ffi_value()
}

#[repr(C)]
pub struct VeilidVersion {
    pub major: u32,
    pub minor: u32,
    pub patch: u32,
}

#[no_mangle]
#[instrument(level = "trace", target = "ffi", skip_all)]
pub extern "C" fn veilid_version() -> VeilidVersion {
    let (major, minor, patch) = veilid_core::veilid_version();
    VeilidVersion {
        major,
        minor,
        patch,
    }
}

#[no_mangle]
#[instrument(level = "trace", target = "ffi", skip_all)]
pub extern "C" fn default_veilid_config() -> *mut c_char {
    veilid_core::default_veilid_config().into_ffi_value()
}

#[no_mangle]
#[instrument(level = "trace", target = "ffi", skip_all)]
pub extern "C" fn veilid_features() -> *mut c_char {
    serde_json::to_string(&veilid_core::veilid_features())
        .expect_or_log("Failed to serialize features")
        .into_ffi_value()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::ffi::CString;

    #[test]
    fn base64_helpers_report_instead_of_panicking() {
        assert_eq!(base64_decode("aGk").expect("valid"), b"hi");
        assert!(base64_decode("not base64!").is_err());
        assert!(base64_decode_opt(None).is_err());
        assert_eq!(
            base64_decode_opt(Some("aGk".to_owned())).expect("valid"),
            b"hi"
        );
    }

    #[test]
    fn optional_json_helper() {
        assert_eq!(deserialize_optional_json::<u32>(None).expect("none"), None);
        assert_eq!(
            deserialize_optional_json::<u32>(Some("7".to_owned())).expect("some"),
            Some(7)
        );
        assert!(deserialize_optional_json::<u32>(Some("x".to_owned())).is_err());
    }

    #[test]
    fn sync_routing_context_builders_fail_closed() {
        let bad = CString::new("{not json").expect("cstring");
        assert_eq!(routing_context_with_safety(0, FfiStr::from_cstr(&bad)), 0);
        assert_eq!(
            routing_context_with_sequencing(0, FfiStr::from_cstr(&bad)),
            0
        );
        // Valid input against an unknown routing context id also yields the sentinel
        let seq = CString::new("\"EnsureOrdered\"").expect("cstring");
        assert_eq!(
            routing_context_with_sequencing(0, FfiStr::from_cstr(&seq)),
            0
        );
    }
}
