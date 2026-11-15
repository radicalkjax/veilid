/// How a StatsManager instance emits its periodic stats records.
#[derive(Clone, Debug, Default)]
pub enum StatsOutput {
    /// Emit each record as an NDJSON line on the "stats" log facility.
    #[default]
    Facility,
    /// Write each record as an NDJSON line to a file. Native only.
    #[cfg(not(all(target_arch = "wasm32", target_os = "unknown")))]
    File(std::path::PathBuf),
}

/// Per-log-key configuration for periodic stats logging.
#[derive(Clone, Debug)]
pub struct StatsSettings {
    /// Whether periodic stats logging is enabled.
    pub enabled: bool,
    /// Seconds between emitted records.
    pub interval_secs: u32,
    /// Where records are emitted.
    pub output: StatsOutput,
}

impl Default for StatsSettings {
    fn default() -> Self {
        Self {
            enabled: false,
            interval_secs: 10,
            output: StatsOutput::default(),
        }
    }
}
