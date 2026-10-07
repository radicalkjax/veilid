use super::*;

fn default_bool_true() -> bool {
    true
}
fn default_veilid_config_log_level_info() -> veilid_core::VeilidConfigLogLevel {
    veilid_core::VeilidConfigLogLevel::Info
}
fn default_veilid_wasm_config_logging_performance_console(
) -> VeilidWASMConfigLoggingPerformanceConsole {
    VeilidWASMConfigLoggingPerformanceConsole {
        enabled: false,
        color: true,
        timestamp: true,
        origin_base_url: None,
    }
}
fn default_veilid_wasm_config_logging_performance() -> VeilidWASMConfigLoggingPerformance {
    VeilidWASMConfigLoggingPerformance {
        enabled: false,
        level: default_veilid_config_log_level_info(),
        timings: false,
        console: default_veilid_wasm_config_logging_performance_console(),
        directives: vec![],
        #[allow(deprecated)]
        ignore_log_targets: vec![],
    }
}
fn default_veilid_wasm_config_logging_api() -> VeilidWASMConfigLoggingAPI {
    VeilidWASMConfigLoggingAPI {
        enabled: false,
        level: default_veilid_config_log_level_info(),
        directives: vec![],
        #[allow(deprecated)]
        ignore_log_targets: vec![],
    }
}

fn default_veilid_wasm_config_logging() -> VeilidWASMConfigLogging {
    VeilidWASMConfigLogging {
        performance: default_veilid_wasm_config_logging_performance(),
        api: default_veilid_wasm_config_logging_api(),
    }
}

/// Configuration for the console logging
#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct VeilidWASMConfigLoggingPerformanceConsole {
    /// Determines if the console logging is enabled at all
    #[serde(default = "default_bool_true")]
    pub enabled: bool,
    /// Determines if the console logging should be colorized
    #[serde(default = "default_bool_true")]
    pub color: bool,
    /// Determines if the console logging should include a timestamp
    #[serde(default = "default_bool_true")]
    pub timestamp: bool,
    /// Optional URL to prepend to origins. E.g. to allow for showing full file paths that can be navigated when logged in the browser console.
    #[serde(default)]
    pub origin_base_url: Option<String>,
}

/// Configuration for the performance/console logging
#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct VeilidWASMConfigLoggingPerformance {
    /// Determines if the performance logging is enabled at all
    #[serde(default = "default_bool_true")]
    pub enabled: bool,
    /// The log level to use for the performance logging
    #[serde(default = "default_veilid_config_log_level_info")]
    pub level: veilid_core::VeilidConfigLogLevel,
    /// Determines if the logging events should be included in the performance timings
    #[serde(default)]
    pub timings: bool,
    /// Configuration for the console logging
    #[serde(default = "default_veilid_wasm_config_logging_performance_console")]
    pub console: VeilidWASMConfigLoggingPerformanceConsole,
    /// Log directives to apply to the performance logging in `veilid_core::VeilidLogDirective` format
    #[serde(default)]
    pub directives: Vec<String>,
    /// Log targets to ignore in the output (deprecated)
    #[deprecated(
        since = "0.5.3",
        note = "'ignore' syntax was confusing, migrate to 'directives'"
    )]
    #[serde(default)]
    pub ignore_log_targets: Vec<String>,
}

/// Configuration for the API logging
#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct VeilidWASMConfigLoggingAPI {
    /// Determines if the API logging is enabled at all
    #[serde(default = "default_bool_true")]
    pub enabled: bool,
    /// The log level to use for the API logging
    #[serde(default = "default_veilid_config_log_level_info")]
    pub level: veilid_core::VeilidConfigLogLevel,
    /// Log directives to apply to the API logging in `veilid_core::VeilidLogDirective` format
    #[serde(default)]
    pub directives: Vec<String>,
    /// Log targets to ignore in the output (deprecated)
    #[deprecated(
        since = "0.5.3",
        note = "'ignore' syntax was confusing, migrate to 'directives'"
    )]
    #[serde(default)]
    pub ignore_log_targets: Vec<String>,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct VeilidWASMConfigLogging {
    #[serde(default = "default_veilid_wasm_config_logging_performance")]
    pub performance: VeilidWASMConfigLoggingPerformance,
    #[serde(default = "default_veilid_wasm_config_logging_api")]
    pub api: VeilidWASMConfigLoggingAPI,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct VeilidWASMConfig {
    #[serde(default = "default_veilid_wasm_config_logging")]
    pub logging: VeilidWASMConfigLogging,
}
