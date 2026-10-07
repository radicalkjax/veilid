use clap::ValueEnum;
use directories::*;

use crate::tools::*;
use serde_derive::*;
use std::ffi::OsStr;
use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use url::Url;
use veilid_core::tools::*;
use veilid_core::*;

use lazy_static::*;

lazy_static! {
    static ref SYSTEM: sysinfo::System = {
        sysinfo::System::new_with_specifics(
            sysinfo::RefreshKind::nothing().with_memory(sysinfo::MemoryRefreshKind::everything()),
        )
    };
    static ref DISKS: sysinfo::Disks = {
        let mut disks = sysinfo::Disks::new_with_refreshed_list();
        disks.sort_by(|a, b| {
            b.mount_point()
                .to_string_lossy()
                .len()
                .cmp(&a.mount_point().to_string_lossy().len())
        });
        disks
    };
}

pub const PROGRAM_NAME: &str = "veilid-server";

pub fn load_default_config() -> EyreResult<config::Config> {
    let dek_password = if let Some(dek_password) = std::env::var_os("DEK_PASSWORD") {
        dek_password
            .to_str()
            .ok_or_else(|| eyre!("DEK_PASSWORD is not valid unicode"))?
            .to_owned()
    } else {
        "".to_owned()
    };
    let new_dek_password = match std::env::var_os("NEW_DEK_PASSWORD") {
        Some(s) => Some(
            s.to_str()
                .ok_or_else(|| eyre!("NEW_DEK_PASSWORD is not valid unicode"))?
                .to_owned(),
        ),
        None => None,
    };

    // Only the runtime-computed/parsed defaults are set here; every other default
    // comes from the Settings structs' `Default` via `#[serde(default)]` at deserialize.
    #[allow(unused_mut)]
    let mut builder = config::Config::builder()
        .set_default("auto_attach", true)?
        .set_default(
            "client_api.ipc_directory",
            Settings::get_default_ipc_directory()
                .to_string_lossy()
                .to_string(),
        )?
        .set_default("client_api.listen_address", "localhost:5959")?
        .set_default(
            "core.protected_store.directory",
            Settings::get_default_protected_store_directory()
                .to_string_lossy()
                .to_string(),
        )?
        .set_default(
            "core.protected_store.device_encryption_key_password",
            dek_password,
        )?
        .set_default(
            "core.table_store.directory",
            Settings::get_default_table_store_directory()
                .to_string_lossy()
                .to_string(),
        )?
        .set_default(
            "core.block_store.directory",
            Settings::get_default_block_store_directory()
                .to_string_lossy()
                .to_string(),
        )?
        .set_default(
            "core.network.tls.certificate_path",
            Settings::get_default_tls_certificate_path()
                .to_string_lossy()
                .to_string(),
        )?
        .set_default(
            "core.network.tls.private_key_path",
            Settings::get_default_tls_private_key_path()
                .to_string_lossy()
                .to_string(),
        )?
        .set_default(
            "core.network.dht.remote_max_subkey_cache_memory_mb",
            Settings::get_default_remote_max_subkey_cache_memory_mb(),
        )?
        .set_default("core.network.protocol.udp.listen_address", ":5150")?
        .set_default("core.network.protocol.tcp.listen_address", ":5150")?
        .set_default("core.network.protocol.ws.listen_address", ":5150")?;

    if let Some(new_dek_password) = new_dek_password {
        builder = builder.set_default(
            "core.protected_store.new_device_encryption_key_password",
            new_dek_password,
        )?;
    }

    #[cfg(feature = "enable-protocol-wss")]
    {
        builder = builder.set_default("core.network.protocol.wss.listen_address", ":5150")?;
    }

    #[cfg(feature = "opentelemetry-otlp")]
    {
        builder = builder.set_default("logging.otlp.grpc_endpoint", "localhost:4317")?;
    }

    #[cfg(feature = "virtual-network")]
    {
        builder = builder
            .set_default(
                "testing.virtual_network_server.tcp.listen_address",
                "localhost:5149",
            )?
            .set_default(
                "testing.virtual_network_server.ws.listen_address",
                "localhost:5148",
            )?;
    }

    builder.build().wrap_err("failed to build default config")
}

pub fn load_config(cfg: config::Config, config_file: &Path) -> EyreResult<config::Config> {
    if let Some(config_file_str) = config_file.to_str() {
        config::Config::builder()
            .add_source(cfg)
            .add_source(config::File::new(config_file_str, config::FileFormat::Yaml))
            .build()
            .wrap_err("failed to load config")
    } else {
        bail!("config file path is not valid UTF-8")
    }
}

/// Logging level threshold (`Off` disables logging).
#[derive(Copy, Clone, Debug, Eq, PartialEq, ValueEnum, Default)]
pub enum LogLevel {
    Off,
    Error,
    Warn,
    #[default]
    Info,
    Debug,
    Trace,
}
impl<'de> serde::Deserialize<'de> for LogLevel {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let s = String::deserialize(deserializer)?;
        match s.to_ascii_lowercase().as_str() {
            "off" => Ok(LogLevel::Off),
            "error" => Ok(LogLevel::Error),
            "warn" => Ok(LogLevel::Warn),
            "info" => Ok(LogLevel::Info),
            "debug" => Ok(LogLevel::Debug),
            "trace" => Ok(LogLevel::Trace),
            _ => Err(serde::de::Error::custom(format!(
                "Invalid log level: {}",
                s
            ))),
        }
    }
}
impl serde::Serialize for LogLevel {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        let s = match self {
            LogLevel::Off => "off",
            LogLevel::Error => "error",
            LogLevel::Warn => "warn",
            LogLevel::Info => "info",
            LogLevel::Debug => "debug",
            LogLevel::Trace => "trace",
        };
        s.serialize(serializer)
    }
}

impl From<LogLevel> for veilid_core::VeilidConfigLogLevel {
    fn from(value: LogLevel) -> Self {
        match value {
            LogLevel::Off => veilid_core::VeilidConfigLogLevel::Off,
            LogLevel::Error => veilid_core::VeilidConfigLogLevel::Error,
            LogLevel::Warn => veilid_core::VeilidConfigLogLevel::Warn,
            LogLevel::Info => veilid_core::VeilidConfigLogLevel::Info,
            LogLevel::Debug => veilid_core::VeilidConfigLogLevel::Debug,
            LogLevel::Trace => veilid_core::VeilidConfigLogLevel::Trace,
        }
    }
}

/// A URL paired with its original unparsed string form.
#[derive(Debug, Clone, Eq, PartialEq)]
pub struct ParsedUrl {
    /// The original URL string as written in the config.
    pub urlstring: String,
    /// The parsed URL.
    pub url: Url,
}

impl ParsedUrl {
    pub fn offset_port(&mut self, offset: u16) -> EyreResult<()> {
        let new_port = self.url.port().unwrap_or_log() + offset;
        // Bump port on url
        self.url
            .set_port(Some(new_port))
            .map_err(|_| eyre!("failed to set port {new_port} on url {}", self.url.as_str()))?;
        self.urlstring = self.url.to_string();
        Ok(())
    }
    pub fn with_offset_port(&self, offset: u16) -> EyreResult<Self> {
        let mut x = self.clone();
        x.offset_port(offset)?;
        Ok(x)
    }
}

impl FromStr for ParsedUrl {
    type Err = url::ParseError;
    fn from_str(s: &str) -> Result<ParsedUrl, url::ParseError> {
        let mut url = Url::parse(s)?;
        if url.scheme().to_lowercase() == "http" && url.port().is_none() {
            url.set_port(Some(80))
                .map_err(|_| url::ParseError::InvalidPort)?
        }
        if url.scheme().to_lowercase() == "https" && url.port().is_none() {
            url.set_port(Some(443))
                .map_err(|_| url::ParseError::InvalidPort)?;
        }
        let parsed_urlstring = url.to_string();
        Ok(Self {
            urlstring: parsed_urlstring,
            url,
        })
    }
}

impl<'de> serde::Deserialize<'de> for ParsedUrl {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let s = String::deserialize(deserializer)?;
        ParsedUrl::from_str(s.as_str()).map_err(serde::de::Error::custom)
    }
}

impl serde::Serialize for ParsedUrl {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        self.urlstring.serialize(serializer)
    }
}

/// A `host:port` string paired with its resolved socket addresses.
#[derive(Debug, Clone, Eq, PartialEq, Default)]
pub struct NamedSocketAddrs {
    /// The original `host:port` string as written in the config.
    pub name: String,
    /// Socket addresses resolved from `name`.
    pub addrs: Vec<SocketAddr>,
}

impl FromStr for NamedSocketAddrs {
    type Err = std::io::Error;
    fn from_str(s: &str) -> Result<NamedSocketAddrs, std::io::Error> {
        if s.is_empty() {
            return Ok(NamedSocketAddrs {
                name: String::new(),
                addrs: vec![],
            });
        }
        let addr_iter = listen_address_to_socket_addrs(s)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidInput, e))?;
        Ok(NamedSocketAddrs {
            name: s.to_owned(),
            addrs: addr_iter,
        })
    }
}

impl<'de> serde::Deserialize<'de> for NamedSocketAddrs {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let s = String::deserialize(deserializer)?;
        NamedSocketAddrs::from_str(s.as_str()).map_err(serde::de::Error::custom)
    }
}

impl serde::Serialize for NamedSocketAddrs {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        self.name.serialize(serializer)
    }
}

impl NamedSocketAddrs {
    pub fn offset_port(&mut self, offset: u16) -> EyreResult<bool> {
        // Bump port on name
        if let Some(split) = self.name.rfind(':') {
            let hoststr = &self.name[0..split];
            let portstr = &self.name[split + 1..];
            let port: u16 = portstr.parse::<u16>().wrap_err("failed to parse port")? + offset;

            self.name = format!("{}:{}", hoststr, port);
        } else {
            return Ok(false);
        }

        // Bump port on addresses
        for addr in self.addrs.iter_mut() {
            addr.set_port(addr.port() + offset);
        }

        Ok(true)
    }

    pub fn with_offset_port(&self, offset: u16) -> EyreResult<Self> {
        let mut x = self.clone();
        x.offset_port(offset)?;
        Ok(x)
    }
}

/// Terminal (stdout) logging sink.
#[derive(Debug, Deserialize, Serialize)]
#[serde(default)]
pub struct Terminal {
    /// Enable terminal logging.
    pub enabled: bool,
    /// Minimum log level for this sink.
    pub level: LogLevel,
    /// Per-target log filter directives, e.g. "net=debug,rpc=trace".
    pub directives: String,
    /// Deprecated: use `directives` instead (a target here becomes `target=off`).
    pub ignore_log_targets: Vec<String>,
}
impl Default for Terminal {
    fn default() -> Self {
        Self {
            enabled: true,
            level: LogLevel::default(),
            directives: String::new(),
            ignore_log_targets: Vec::new(),
        }
    }
}

/// Flamegraph (folded-stack) profiling output sink.
#[cfg(feature = "flame")]
#[derive(Debug, Deserialize, Serialize, Default)]
#[serde(default)]
pub struct Flame {
    /// Enable flamegraph output.
    pub enabled: bool,
    /// Output file path (empty = platform temp dir default).
    pub path: String,
}

/// Perfetto trace output sink.
#[cfg(all(unix, feature = "perfetto"))]
#[derive(Debug, Deserialize, Serialize, Default)]
#[serde(default)]
pub struct Perfetto {
    /// Enable Perfetto tracing output.
    pub enabled: bool,
    /// Output file path (empty = platform temp dir default).
    pub path: String,
}

/// tokio-console diagnostics sink.
#[cfg(feature = "tokio-console")]
#[derive(Debug, Deserialize, Serialize, Default)]
#[serde(default)]
pub struct Console {
    /// Enable the tokio-console subscriber.
    pub enabled: bool,
}

/// File logging sink.
#[derive(Debug, Deserialize, Serialize)]
#[serde(default)]
pub struct File {
    /// Enable file logging.
    pub enabled: bool,
    /// Log file path.
    pub path: String,
    /// Append to an existing file rather than truncating it.
    pub append: bool,
    /// Minimum log level for this sink.
    pub level: LogLevel,
    /// Per-target log filter directives, e.g. "net=debug,rpc=trace".
    pub directives: String,
    /// Deprecated: use `directives` instead (a target here becomes `target=off`).
    pub ignore_log_targets: Vec<String>,
}
impl Default for File {
    fn default() -> Self {
        Self {
            enabled: false,
            path: String::new(),
            append: true,
            level: LogLevel::default(),
            directives: String::new(),
            ignore_log_targets: Vec::new(),
        }
    }
}

/// System logger sink (syslog/oslog/Android log, platform dependent).
#[derive(Debug, Deserialize, Serialize, Default)]
#[serde(default)]
pub struct System {
    /// Enable system logging.
    pub enabled: bool,
    /// Minimum log level for this sink.
    pub level: LogLevel,
    /// Per-target log filter directives, e.g. "net=debug,rpc=trace".
    pub directives: String,
    /// Deprecated: use `directives` instead (a target here becomes `target=off`).
    pub ignore_log_targets: Vec<String>,
}

/// Logging sink delivered to the connected veilid-remote-api client.
#[derive(Debug, Deserialize, Serialize)]
#[serde(default)]
pub struct Api {
    /// Enable delivery of logs to the veilid-remote-api client.
    pub enabled: bool,
    /// Minimum log level for this sink.
    pub level: LogLevel,
    /// Per-target log filter directives, e.g. "net=debug,rpc=trace".
    pub directives: String,
    /// Deprecated: use `directives` instead (a target here becomes `target=off`).
    pub ignore_log_targets: Vec<String>,
}
impl Default for Api {
    fn default() -> Self {
        Self {
            enabled: true,
            level: LogLevel::default(),
            directives: String::new(),
            ignore_log_targets: Vec::new(),
        }
    }
}

/// OpenTelemetry OTLP export sink.
#[cfg(feature = "opentelemetry-otlp")]
#[derive(Debug, Deserialize, Serialize)]
#[serde(default)]
pub struct Otlp {
    /// Enable OTLP export.
    pub enabled: bool,
    /// Minimum log level for this sink.
    pub level: LogLevel,
    /// gRPC endpoint of the OTLP collector, as `host:port`.
    pub grpc_endpoint: NamedSocketAddrs,
    /// Per-target log filter directives, e.g. "net=debug,rpc=trace".
    pub directives: String,
    /// Deprecated: use `directives` instead (a target here becomes `target=off`).
    pub ignore_log_targets: Vec<String>,
}
#[cfg(feature = "opentelemetry-otlp")]
impl Default for Otlp {
    fn default() -> Self {
        // grpc_endpoint is set as a runtime default in load_default_config.
        Self {
            enabled: false,
            level: LogLevel::Trace,
            grpc_endpoint: NamedSocketAddrs::default(),
            directives: String::new(),
            ignore_log_targets: Vec::new(),
        }
    }
}

/// How veilid-remote-api clients (e.g. veilid-cli) connect to this server.
#[derive(Debug, Deserialize, Serialize)]
#[serde(default)]
pub struct ClientApi {
    /// Accept client connections over a local IPC socket.
    pub ipc_enabled: bool,
    /// Directory holding the IPC socket(s).
    pub ipc_directory: PathBuf,
    /// Accept client connections over a TCP network socket.
    pub network_enabled: bool,
    /// Address to listen on for network clients, as `host:port`.
    pub listen_address: NamedSocketAddrs,
}
impl Default for ClientApi {
    fn default() -> Self {
        // ipc_directory and listen_address are set as runtime defaults in load_default_config.
        Self {
            ipc_enabled: true,
            ipc_directory: PathBuf::new(),
            network_enabled: false,
            listen_address: NamedSocketAddrs::default(),
        }
    }
}

/// Logging configuration: one section per output sink.
#[derive(Debug, Deserialize, Serialize, Default)]
#[serde(default)]
pub struct Logging {
    /// System logger sink.
    pub system: System,
    /// Terminal (stdout) sink.
    pub terminal: Terminal,
    /// File sink.
    pub file: File,
    /// veilid-remote-api client sink.
    pub api: Api,
    /// OpenTelemetry OTLP sink.
    #[cfg(feature = "opentelemetry-otlp")]
    pub otlp: Otlp,
    /// Flamegraph sink.
    #[cfg(feature = "flame")]
    pub flame: Flame,
    /// Perfetto sink.
    #[cfg(all(unix, feature = "perfetto"))]
    pub perfetto: Perfetto,
    /// tokio-console sink.
    #[cfg(feature = "tokio-console")]
    pub console: Console,
}

// `core` config structs use `Default` (pulled from veilid-core's VeilidConfig*
// where the server agrees, server-specific otherwise) as the single source of
// defaults; `#[serde(default)]` lets the default config omit fields equal to it.

/// UDP protocol configuration.
#[derive(Debug, Deserialize, Serialize)]
#[serde(default)]
pub struct Udp {
    /// Enable the UDP protocol.
    pub enabled: bool,
    /// Local address to bind, as `host:port`.
    pub listen_address: NamedSocketAddrs,
    /// Externally-reachable address to advertise, if behind NAT/port mapping.
    pub public_address: Option<NamedSocketAddrs>,
}
impl Default for Udp {
    fn default() -> Self {
        Self {
            enabled: VeilidConfigUDP::default().enabled,
            listen_address: NamedSocketAddrs::default(),
            public_address: None,
        }
    }
}

/// TCP protocol configuration.
#[derive(Debug, Deserialize, Serialize)]
#[serde(default)]
pub struct Tcp {
    /// Allow outbound TCP connections.
    pub connect: bool,
    /// Accept inbound TCP connections.
    pub listen: bool,
    /// Local address to bind, as `host:port`.
    pub listen_address: NamedSocketAddrs,
    /// Externally-reachable address to advertise, if behind NAT/port mapping.
    pub public_address: Option<NamedSocketAddrs>,
}
impl Default for Tcp {
    fn default() -> Self {
        let d = VeilidConfigTCP::default();
        Self {
            connect: d.connect,
            listen: d.listen,
            listen_address: NamedSocketAddrs::default(),
            public_address: None,
        }
    }
}

/// WebSocket protocol configuration.
#[derive(Debug, Deserialize, Serialize)]
#[serde(default)]
pub struct Ws {
    /// Allow outbound WebSocket connections.
    pub connect: bool,
    /// Accept inbound WebSocket connections.
    pub listen: bool,
    /// Local address to bind, as `host:port`.
    pub listen_address: NamedSocketAddrs,
    /// URL path served by the WebSocket listener.
    pub path: PathBuf,
    /// Externally-reachable URL to advertise, if behind NAT/proxy.
    pub url: Option<ParsedUrl>,
}
impl Default for Ws {
    fn default() -> Self {
        let d = VeilidConfigWS::default();
        Self {
            connect: d.connect,
            listen: d.listen,
            listen_address: NamedSocketAddrs::default(),
            path: PathBuf::from(d.path),
            url: None,
        }
    }
}

/// Secure WebSocket protocol configuration.
#[cfg(feature = "enable-protocol-wss")]
#[derive(Debug, Deserialize, Serialize)]
#[serde(default)]
pub struct Wss {
    /// Allow outbound secure WebSocket connections.
    pub connect: bool,
    /// Accept inbound secure WebSocket connections.
    pub listen: bool,
    /// Local address to bind, as `host:port`.
    pub listen_address: NamedSocketAddrs,
    /// URL path served by the secure WebSocket listener.
    pub path: PathBuf,
    /// Externally-reachable URL to advertise (required for TLS protocols).
    pub url: Option<ParsedUrl>,
}
#[cfg(feature = "enable-protocol-wss")]
impl Default for Wss {
    fn default() -> Self {
        let d = VeilidConfigWSS::default();
        Self {
            connect: d.connect,
            listen: d.listen,
            listen_address: NamedSocketAddrs::default(),
            path: PathBuf::from(d.path),
            url: None,
        }
    }
}

/// Per-protocol network configuration.
#[derive(Debug, Deserialize, Serialize, Default)]
#[serde(default)]
pub struct Protocol {
    /// UDP configuration.
    pub udp: Udp,
    /// TCP configuration.
    pub tcp: Tcp,
    /// WebSocket configuration.
    pub ws: Ws,
    /// Secure WebSocket configuration.
    #[cfg(feature = "enable-protocol-wss")]
    pub wss: Wss,
}

/// Privacy and relay preferences.
#[derive(Debug, Deserialize, Serialize, Default)]
#[serde(default)]
pub struct Privacy {
    /// Always use an inbound relay; never accept direct inbound connections.
    pub require_inbound_relay: bool,
    /// Two-letter country codes to refuse routing through (requires `geolocation`).
    #[cfg(feature = "geolocation")]
    pub country_code_denylist: Vec<CountryCode>,
}

/// TLS configuration for inbound secure protocols.
#[derive(Debug, Deserialize, Serialize)]
#[serde(default)]
pub struct Tls {
    /// Path to the TLS certificate (PEM).
    pub certificate_path: String,
    /// Path to the TLS private key (PEM).
    pub private_key_path: String,
    /// Timeout for completing a TLS handshake, in milliseconds.
    pub connection_initial_timeout_ms: u32,
}
impl Default for Tls {
    fn default() -> Self {
        Self {
            certificate_path: String::new(),
            private_key_path: String::new(),
            connection_initial_timeout_ms: VeilidConfigTLS::default().connection_initial_timeout_ms,
        }
    }
}

/// RPC configuration.
#[derive(Debug, Deserialize, Serialize)]
#[serde(default)]
pub struct Rpc {
    /// Default number of hops used when allocating private routes.
    pub default_route_hop_count: u8,
}
impl Default for Rpc {
    fn default() -> Self {
        Self {
            default_route_hop_count: VeilidConfigRPC::default().default_route_hop_count,
        }
    }
}

/// DHT cache and storage configuration.
#[derive(Debug, Deserialize, Serialize)]
#[serde(default)]
pub struct Dht {
    /// Number of subkeys cached for locally-created DHT records.
    pub local_subkey_cache_size: u32,
    /// Memory cap for the local subkey cache, in megabytes.
    pub local_max_subkey_cache_memory_mb: u32,
    /// Number of subkeys cached for DHT records stored on behalf of others.
    pub remote_subkey_cache_size: u32,
    /// Maximum number of remote DHT records stored on behalf of others.
    pub remote_max_records: u32,
    /// Memory cap for the remote subkey cache, in megabytes (0 = computed at load).
    pub remote_max_subkey_cache_memory_mb: u32,
    /// Disk cap for remote DHT record storage, in megabytes (0 = computed at load).
    pub remote_max_storage_space_mb: u32,
    /// Max concurrent DHT network operations in flight (local-only ops exempt).
    pub max_concurrent_operations: u32,
}
impl Default for Dht {
    fn default() -> Self {
        // Server-specific (heavier remote caches than core's embedded defaults).
        // The two memory/storage sizes are computed at load (0 = placeholder).
        Self {
            local_subkey_cache_size: 128,
            local_max_subkey_cache_memory_mb: 256,
            remote_subkey_cache_size: 1024,
            remote_max_records: 65536,
            remote_max_subkey_cache_memory_mb: 0,
            remote_max_storage_space_mb: 0,
            max_concurrent_operations: 16,
        }
    }
}

/// Routing table identity and bootstrap configuration.
#[derive(Debug, Deserialize, Serialize)]
#[serde(default)]
pub struct RoutingTable {
    /// This node's identity public keys, by crypto kind (`None` = generate fresh).
    pub public_keys: Option<veilid_core::PublicKeyGroup>,
    /// Node identity secret keys matching `public_keys` (`None` = generate fresh).
    pub secret_keys: Option<veilid_core::SecretKeyGroup>,
    /// Bootstrap server hostnames/URLs used to join the network.
    pub bootstrap: Vec<String>,
    /// Public keys trusted to sign bootstrap records.
    pub bootstrap_keys: Vec<veilid_core::PublicKey>,
}
impl Default for RoutingTable {
    fn default() -> Self {
        let d = VeilidConfigRoutingTable::default();
        Self {
            public_keys: None,
            secret_keys: None,
            bootstrap: d.bootstrap,
            bootstrap_keys: d.bootstrap_keys,
        }
    }
}

mod auto_bool {
    use serde::{Deserialize, Deserializer, Serialize, Serializer};

    pub fn from_str(s: &str) -> Result<Option<bool>, String> {
        match s {
            "auto" => Ok(None),
            "true" => Ok(Some(true)),
            "false" => Ok(Some(false)),
            _ => Err("Expected 'auto', 'true', or 'false'".to_owned()),
        }
    }

    pub fn serialize<S>(value: &Option<bool>, s: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        if let Some(value) = value {
            value.to_string().serialize(s)
        } else {
            "auto".serialize(s)
        }
    }

    pub fn deserialize<'de, D>(d: D) -> Result<Option<bool>, D::Error>
    where
        D: Deserializer<'de>,
    {
        let s: String = Deserialize::deserialize(d)?;
        from_str(s.as_str()).map_err(serde::de::Error::custom)
    }

    #[cfg(test)]
    mod tests {
        use super::*;
        use serde_test::{assert_tokens, Token};

        #[test]
        fn test_from_str() {
            let s = "auto";
            let b = from_str(s).unwrap();
            assert_eq!(b, None);

            let s = "true";
            let b = from_str(s).unwrap();
            assert_eq!(b, Some(true));

            let s = "false";
            let b = from_str(s).unwrap();
            assert_eq!(b, Some(false));

            let s = "invalid";
            let b = from_str(s).unwrap_err();
            assert_eq!(b, "Expected 'auto', 'true', or 'false'");
        }

        #[test]
        fn test_serde_tokens() {
            #[derive(Debug, Deserialize, Serialize, PartialEq)]
            struct Foo {
                #[serde(with = "super")]
                pub prop: Option<bool>,
            }

            let obj = Foo { prop: Some(true) };
            assert_tokens(
                &obj,
                &[
                    Token::Struct {
                        name: "Foo",
                        len: 1,
                    },
                    Token::Str("prop"),
                    Token::Str("true"),
                    Token::StructEnd,
                ],
            );
            let obj = Foo { prop: Some(false) };
            assert_tokens(
                &obj,
                &[
                    Token::Struct {
                        name: "Foo",
                        len: 1,
                    },
                    Token::Str("prop"),
                    Token::Str("false"),
                    Token::StructEnd,
                ],
            );

            let obj = Foo { prop: None };
            assert_tokens(
                &obj,
                &[
                    Token::Struct {
                        name: "Foo",
                        len: 1,
                    },
                    Token::Str("prop"),
                    Token::Str("auto"),
                    Token::StructEnd,
                ],
            );
        }
    }
}

/// Low-level network configuration.
#[derive(Debug, Deserialize, Serialize)]
#[serde(default)]
pub struct Network {
    /// Maximum total simultaneous connections across all protocols.
    pub max_connections: u32,
    /// Optional password; its presence joins a private network with a derived network key.
    pub network_key_password: Option<String>,
    /// Routing table identity and bootstrap configuration.
    pub routing_table: RoutingTable,
    /// RPC configuration.
    pub rpc: Rpc,
    /// DHT cache and storage configuration.
    pub dht: Dht,
    /// Enabled IP address families (e.g. `['ipv4']`); empty = all available.
    pub address_types: Vec<String>,
    /// Use UPnP to map ports on the local gateway.
    pub upnp: bool,
    /// React to local address changes: `'auto'`, `'true'`, or `'false'`.
    #[serde(with = "auto_bool")]
    pub detect_address_changes: Option<bool>,
    /// TLS configuration for inbound secure protocols.
    pub tls: Tls,
    /// Per-protocol configuration.
    pub protocol: Protocol,
    /// Privacy and relay preferences.
    pub privacy: Privacy,
    /// Virtual network client configuration (testing/simulation).
    #[cfg(feature = "virtual-network")]
    pub virtual_network: VirtualNetwork,
}
impl Default for Network {
    fn default() -> Self {
        // Server-specific: higher conn cap, upnp off, address-change detection on auto.
        Self {
            max_connections: 256,
            network_key_password: None,
            routing_table: RoutingTable::default(),
            rpc: Rpc::default(),
            dht: Dht::default(),
            address_types: Vec::new(),
            upnp: false,
            detect_address_changes: None,
            tls: Tls::default(),
            protocol: Protocol::default(),
            privacy: Privacy::default(),
            #[cfg(feature = "virtual-network")]
            virtual_network: VirtualNetwork::default(),
        }
    }
}

/// Virtual network client configuration (testing/simulation).
#[cfg(feature = "virtual-network")]
#[derive(Debug, Deserialize, Serialize, Default)]
#[serde(default)]
pub struct VirtualNetwork {
    /// Route all networking through the virtual network server.
    pub enabled: bool,
    /// Address of the virtual network server, as `host:port`.
    pub server_address: String,
}

/// Embedded virtual network server configuration (testing/simulation).
#[cfg(feature = "virtual-network")]
#[derive(Debug, Deserialize, Serialize, Default)]
#[serde(default)]
pub struct VirtualNetworkServer {
    /// Run the embedded virtual network server.
    pub enabled: bool,
    /// TCP listener for the virtual network server.
    pub tcp: VirtualNetworkServerTcp,
    /// WebSocket listener for the virtual network server.
    pub ws: VirtualNetworkServerWs,
}
/// Virtual network server TCP listener.
#[cfg(feature = "virtual-network")]
#[derive(Debug, Deserialize, Serialize)]
#[serde(default)]
pub struct VirtualNetworkServerTcp {
    /// Accept inbound TCP connections.
    pub listen: bool,
    /// Local address to bind, as `host:port`.
    pub listen_address: NamedSocketAddrs,
}
#[cfg(feature = "virtual-network")]
impl Default for VirtualNetworkServerTcp {
    fn default() -> Self {
        // listen_address is set as a runtime default in load_default_config.
        Self {
            listen: true,
            listen_address: NamedSocketAddrs::default(),
        }
    }
}
/// Virtual network server WebSocket listener.
#[cfg(feature = "virtual-network")]
#[derive(Debug, Deserialize, Serialize)]
#[serde(default)]
pub struct VirtualNetworkServerWs {
    /// Accept inbound WebSocket connections.
    pub listen: bool,
    /// Local address to bind, as `host:port`.
    pub listen_address: NamedSocketAddrs,
}
#[cfg(feature = "virtual-network")]
impl Default for VirtualNetworkServerWs {
    fn default() -> Self {
        // listen_address is set as a runtime default in load_default_config.
        Self {
            listen: true,
            listen_address: NamedSocketAddrs::default(),
        }
    }
}

/// Testing and multi-subnode configuration.
#[derive(Debug, Deserialize, Serialize)]
#[serde(default)]
pub struct Testing {
    /// Index of this subnode within a multi-subnode run (offsets ports/paths).
    pub subnode_index: u16,
    /// Number of subnodes this process runs.
    pub subnode_count: u16,
    /// Embedded virtual network server configuration.
    #[cfg(feature = "virtual-network")]
    pub virtual_network_server: VirtualNetworkServer,
}
impl Default for Testing {
    fn default() -> Self {
        Self {
            subnode_index: 0,
            subnode_count: 1,
            #[cfg(feature = "virtual-network")]
            virtual_network_server: VirtualNetworkServer::default(),
        }
    }
}

/// Table store (persistent encrypted database) configuration.
#[derive(Debug, Deserialize, Serialize)]
#[serde(default)]
pub struct TableStore {
    /// Directory holding the table store database (empty = platform default).
    pub directory: String,
    /// Delete the table store on startup.
    pub delete: bool,
    /// Wipe the table store on an invalid device encryption key, rather than failing.
    pub wipe_on_invalid_device_encryption_key: bool,
    /// Maximum size of a single stored value, in megabytes.
    pub max_value_size_mb: u32,
}
impl Default for TableStore {
    fn default() -> Self {
        let d = VeilidConfigTableStore::default();
        // Server keeps device-key data on wipe by default (core wipes).
        Self {
            directory: String::new(),
            delete: d.delete,
            wipe_on_invalid_device_encryption_key: false,
            max_value_size_mb: d.max_value_size_mb,
        }
    }
}

/// Block store (large content-addressable storage) configuration.
#[derive(Debug, Deserialize, Serialize)]
#[serde(default)]
pub struct BlockStore {
    /// Directory holding the block store (empty = platform default).
    pub directory: String,
    /// Delete the block store on startup.
    pub delete: bool,
}
impl Default for BlockStore {
    fn default() -> Self {
        Self {
            directory: String::new(),
            delete: VeilidConfigBlockStore::default().delete,
        }
    }
}

/// Protected store (OS keychain/keyring or insecure-fallback) configuration.
#[derive(Debug, Deserialize, Serialize)]
#[serde(default)]
pub struct ProtectedStore {
    /// Fall back to insecure file storage if no OS keychain/keyring is available.
    pub allow_insecure_fallback: bool,
    /// Always use insecure file storage, ignoring any OS keychain/keyring.
    pub always_use_insecure_storage: bool,
    /// Directory for insecure-fallback storage (empty = platform default).
    pub directory: String,
    /// Delete the protected store on startup.
    pub delete: bool,
    /// Password used to encrypt the device encryption key.
    pub device_encryption_key_password: String,
    /// New password to re-encrypt the device encryption key with, triggering a rotation.
    pub new_device_encryption_key_password: Option<String>,
}
impl Default for ProtectedStore {
    fn default() -> Self {
        // Server allows/uses insecure storage by default (core does not).
        Self {
            allow_insecure_fallback: true,
            always_use_insecure_storage: true,
            directory: String::new(),
            delete: VeilidConfigProtectedStore::default().delete,
            device_encryption_key_password: String::new(),
            new_device_encryption_key_password: None,
        }
    }
}

/// Capabilities advertised by this node.
#[derive(Debug, Deserialize, Serialize, Default)]
#[serde(default)]
pub struct Capabilities {
    /// Capability names to disable (advertised as unavailable).
    pub disable: Vec<String>,
}

/// The veilid-core configuration tree (mirrors `VeilidConfig`).
#[derive(Debug, Deserialize, Serialize, Default)]
#[serde(default)]
pub struct Core {
    /// Capabilities advertised by this node.
    pub capabilities: Capabilities,
    /// Protected store configuration.
    pub protected_store: ProtectedStore,
    /// Table store configuration.
    pub table_store: TableStore,
    /// Block store configuration.
    pub block_store: BlockStore,
    /// Network configuration.
    pub network: Network,
    /// Footgun tuning tree (veilid-core's type, single source of defaults).
    /// Only honored when veilid-core is built with `footgun-config`.
    pub internal: VeilidConfigInternal,
}

/// Background daemon (Unix) configuration.
#[derive(Debug, Deserialize, Serialize, Default)]
#[serde(default)]
pub struct Daemon {
    /// Run veilid-server as a background daemon.
    pub enabled: bool,
    /// Path to write the daemon's PID file.
    pub pid_file: Option<String>,
    /// Directory to chroot into after startup.
    pub chroot: Option<String>,
    /// Working directory to change into after startup.
    pub working_directory: Option<String>,
    /// User to drop privileges to.
    pub user: Option<String>,
    /// Group to drop privileges to.
    pub group: Option<String>,
    /// File to redirect the daemon's stdout to.
    pub stdout_file: Option<String>,
    /// File to redirect the daemon's stderr to.
    pub stderr_file: Option<String>,
}

/// Top level of the veilid-server settings tree.
#[derive(Debug, Deserialize, Serialize, Default)]
#[serde(default)]
pub struct SettingsInner {
    /// Background daemon configuration.
    pub daemon: Daemon,
    /// veilid-remote-api client connection configuration.
    pub client_api: ClientApi,
    /// Attach to the network automatically on startup.
    // auto_attach defaults to true; set as a runtime default in load_default_config.
    pub auto_attach: bool,
    /// Logging configuration.
    pub logging: Logging,
    /// Testing and multi-subnode configuration.
    pub testing: Testing,
    /// The veilid-core configuration tree.
    pub core: Core,
}

/// Loaded veilid-server settings: the parsed tree plus any unrecognized config keys.
#[derive(Clone, Debug)]
pub struct Settings {
    /// The parsed settings tree, behind a lock for runtime `set()` updates.
    inner: Arc<RwLock<SettingsInner>>,
    /// Config keys present in the file but not recognized; warned about once logging is up.
    unknown_keys: Arc<Vec<String>>,
}

impl Settings {
    pub fn new(config_file: Option<&OsStr>) -> EyreResult<Self> {
        // Load the default config
        let mut cfg = load_default_config()?;

        // Merge in the config file if we have one
        if let Some(config_file) = config_file {
            let config_file_path = Path::new(config_file);
            // If the user specifies a config file on the command line then it must exist
            cfg = load_config(cfg, config_file_path)?;
        }

        // Generate config, collecting any keys that don't map to a known setting (likely a typo
        // or a setting that moved under 'internal'). We warn about these once logging is up
        // rather than erroring, so an unknown key never blocks startup.
        let mut unknown_keys = Vec::<String>::new();
        let mut inner: SettingsInner = serde_ignored::deserialize(cfg, |path| {
            unknown_keys.push(path.to_string());
        })?;

        // Fill in missing defaults
        if inner.core.network.dht.remote_max_storage_space_mb == 0 {
            inner.core.network.dht.remote_max_storage_space_mb =
                Self::get_default_remote_max_storage_space_mb(&inner);
        }

        //
        Ok(Self {
            inner: Arc::new(RwLock::new(inner)),
            unknown_keys: Arc::new(unknown_keys),
        })
    }

    /// Warn about config keys that were present but not recognized. Call after logging is set up.
    pub fn warn_unknown_config_keys(&self) {
        for key in self.unknown_keys.iter() {
            warn!("ignoring unknown veilid-server config key: '{key}'");
        }
    }

    /// Warn about deprecated config keys still in use. Call after logging is set up.
    pub fn warn_deprecated_config(&self) {
        let inner = self.inner.read();
        let check = |section: &str, list: &[String]| {
            if !list.is_empty() {
                warn!(
                    "veilid-server config '{section}.ignore_log_targets' is deprecated; \
                     use '{section}.directives' instead (e.g. 'target=off')"
                );
            }
        };
        check(
            "logging.terminal",
            &inner.logging.terminal.ignore_log_targets,
        );
        check("logging.file", &inner.logging.file.ignore_log_targets);
        check("logging.system", &inner.logging.system.ignore_log_targets);
        check("logging.api", &inner.logging.api.ignore_log_targets);
        #[cfg(feature = "opentelemetry-otlp")]
        check("logging.otlp", &inner.logging.otlp.ignore_log_targets);
    }

    pub fn verify(&self) -> EyreResult<()> {
        cfg_if! {
            if #[cfg(windows)] {
                // no ipc setup for windows
            } else {
                let inner = self.inner.read();
                if inner.client_api.ipc_enabled
                    && !Self::get_or_create_private_directory(&inner.client_api.ipc_directory, true)
                {
                    bail!("unable to create default IPC directory {:?}", inner.client_api.ipc_directory);
                }
            }
        }

        Ok(())
    }

    pub fn read(&self) -> RwLockReadGuard<'_, SettingsInner> {
        self.inner.read()
    }
    pub fn write(&self) -> RwLockWriteGuard<'_, SettingsInner> {
        self.inner.write()
    }

    /// Determine default config path
    ///
    /// In a unix-like environment, veilid-server will look for its config file
    /// in /etc/veilid-server. If a config is not found in this location, it will
    /// follow the XDG user directory spec, and look in `~/.config/veilid-server/`.
    ///
    /// For Windows, a user-local config may be created at
    /// `C:\Users\<user>\AppData\Roaming\Veilid\Veilid`, and for macOS, at
    /// `/Users/<user>/Library/Application Support/org.Veilid.Veilid`
    ///
    pub fn get_default_config_path(subpath: &str) -> PathBuf {
        #[cfg(unix)]
        {
            let globalpath = PathBuf::from("/etc/veilid-server");

            if globalpath.exists() {
                return globalpath.join(subpath);
            }
        }

        let mut ts_path = if let Some(my_proj_dirs) = ProjectDirs::from("org", "Veilid", "Veilid") {
            PathBuf::from(my_proj_dirs.config_dir())
        } else {
            PathBuf::from("./")
        };
        ts_path.push(subpath);
        ts_path
    }

    /// Determine default flamegraph output path
    #[cfg(feature = "flame")]
    pub fn get_default_flame_path(subnode_index: u16, subnode_count: u16) -> PathBuf {
        let name = if subnode_count == 1 {
            if subnode_index == 0 {
                "veilid-server.folded".to_owned()
            } else {
                format!("veilid-server-{}.folded", subnode_index)
            }
        } else {
            format!(
                "veilid-server-{}-{}.folded",
                subnode_index,
                subnode_index + subnode_count - 1
            )
        };
        std::env::temp_dir().join(name)
    }

    /// Determine default perfetto output path
    #[cfg(all(unix, feature = "perfetto"))]
    pub fn get_default_perfetto_path(subnode_index: u16, subnode_count: u16) -> PathBuf {
        let name = if subnode_count == 1 {
            if subnode_index == 0 {
                "veilid-server.pftrace".to_owned()
            } else {
                format!("veilid-server-{}.pftrace", subnode_index)
            }
        } else {
            format!(
                "veilid-server-{}-{}.pftrace",
                subnode_index,
                subnode_index + subnode_count - 1
            )
        };
        std::env::temp_dir().join(name)
    }

    #[cfg_attr(windows, expect(dead_code))]
    fn get_or_create_private_directory<P: AsRef<Path>>(path: P, group_read: bool) -> bool {
        let path = path.as_ref();
        if !path.is_dir()
            && (std::fs::create_dir_all(path).is_err()
                || ensure_directory_private_owner(path, group_read).is_err())
        {
            return false;
        }
        true
    }

    fn get_default_directory(subpath: &str) -> PathBuf {
        #[cfg(unix)]
        {
            let globalpath = PathBuf::from("/var/db/veilid-server");

            if globalpath.exists() {
                return globalpath.join(subpath);
            }
        }

        let mut ts_path = if let Some(my_proj_dirs) = ProjectDirs::from("org", "Veilid", "Veilid") {
            PathBuf::from(my_proj_dirs.data_local_dir())
        } else {
            PathBuf::from("./")
        };
        ts_path.push(subpath);
        ts_path
    }

    pub fn get_default_ipc_directory() -> PathBuf {
        cfg_if! {
            if #[cfg(windows)] {
                PathBuf::from(r"\\.\PIPE\veilid-server")
            } else {
                Self::get_default_directory("ipc")
            }
        }
    }

    pub fn get_default_veilid_server_conf_path() -> PathBuf {
        Settings::get_default_config_path("veilid-server.conf")
    }
    pub fn get_default_table_store_directory() -> PathBuf {
        Settings::get_default_directory("table_store")
    }
    pub fn get_default_block_store_directory() -> PathBuf {
        Settings::get_default_directory("block_store")
    }
    pub fn get_default_protected_store_directory() -> PathBuf {
        Settings::get_default_directory("protected_store")
    }
    pub fn get_default_tls_certificate_path() -> PathBuf {
        Settings::get_default_config_path("ssl/certs/server.crt")
    }
    pub fn get_default_tls_private_key_path() -> PathBuf {
        Settings::get_default_config_path("ssl/keys/server.key")
    }

    pub fn get_default_remote_max_subkey_cache_memory_mb() -> u32 {
        if sysinfo::IS_SUPPORTED_SYSTEM {
            ((SYSTEM.free_memory() / (1024u64 * 1024u64)) / 16) as u32
        } else {
            256
        }
    }

    pub fn get_default_remote_max_storage_space_mb(inner: &SettingsInner) -> u32 {
        let dht_storage_path = inner.core.table_store.directory.clone();
        // Sort longer mount point paths first since we want the mount point closest to our table store directory

        if sysinfo::IS_SUPPORTED_SYSTEM {
            for disk in DISKS.list() {
                if dht_storage_path.starts_with(&*disk.mount_point().to_string_lossy()) {
                    let available_mb = disk.available_space() / 1_000_000u64;
                    if available_mb > 40_000 {
                        // Default to 10GB if more than 40GB is available
                        return 10_000;
                    }
                    // Default to 1/4 of the available space, if less than 40GB is available
                    return available_mb as u32;
                }
            }
        }

        // If we can't figure out our storage path go with 1GB of space and pray
        1_000
    }

    pub fn set(&self, key: &str, value: &str) -> EyreResult<()> {
        let mut inner = self.inner.write();

        macro_rules! set_config_value {
            ($innerkey:expr, $value:expr) => {{
                let innerkeyname = &stringify!($innerkey)[6..];
                if innerkeyname == key {
                    match veilid_core::deserialize_json_lenient(value) {
                        Ok(v) => {
                            $innerkey = v;
                            return Ok(());
                        }
                        Err(e) => {
                            return Err(eyre!(
                                "invalid type for key {}, value: {}: {}",
                                key,
                                value,
                                e
                            ))
                        }
                    }
                }
            }};
        }

        macro_rules! set_config_value_custom {
            ($innerkey:expr, $value:expr, $deserializer:expr) => {{
                let innerkeyname = &stringify!($innerkey)[6..];
                if innerkeyname == key {
                    match $deserializer(value) {
                        Ok(v) => {
                            $innerkey = v;
                            return Ok(());
                        }
                        Err(e) => {
                            return Err(eyre!(
                                "invalid type for key {}, value: {}: {}",
                                key,
                                value,
                                e
                            ))
                        }
                    }
                }
            }};
        }

        set_config_value!(inner.daemon.enabled, value);
        set_config_value!(inner.daemon.pid_file, value);
        set_config_value!(inner.daemon.chroot, value);
        set_config_value!(inner.daemon.working_directory, value);
        set_config_value!(inner.daemon.user, value);
        set_config_value!(inner.daemon.group, value);
        set_config_value!(inner.daemon.stdout_file, value);
        set_config_value!(inner.daemon.stderr_file, value);

        set_config_value!(inner.client_api.ipc_enabled, value);
        set_config_value!(inner.client_api.ipc_directory, value);
        set_config_value!(inner.client_api.network_enabled, value);
        set_config_value!(inner.client_api.listen_address, value);

        set_config_value!(inner.auto_attach, value);

        set_config_value!(inner.logging.system.enabled, value);
        set_config_value!(inner.logging.system.level, value);
        set_config_value!(inner.logging.system.directives, value);
        set_config_value!(inner.logging.system.ignore_log_targets, value);
        set_config_value!(inner.logging.terminal.enabled, value);
        set_config_value!(inner.logging.terminal.level, value);
        set_config_value!(inner.logging.terminal.directives, value);
        set_config_value!(inner.logging.terminal.ignore_log_targets, value);
        set_config_value!(inner.logging.file.enabled, value);
        set_config_value!(inner.logging.file.path, value);
        set_config_value!(inner.logging.file.append, value);
        set_config_value!(inner.logging.file.level, value);
        set_config_value!(inner.logging.file.directives, value);
        set_config_value!(inner.logging.file.ignore_log_targets, value);
        set_config_value!(inner.logging.api.enabled, value);
        set_config_value!(inner.logging.api.level, value);
        set_config_value!(inner.logging.api.directives, value);
        set_config_value!(inner.logging.api.ignore_log_targets, value);
        #[cfg(feature = "opentelemetry-otlp")]
        {
            set_config_value!(inner.logging.otlp.enabled, value);
            set_config_value!(inner.logging.otlp.level, value);
            set_config_value!(inner.logging.otlp.grpc_endpoint, value);
            set_config_value!(inner.logging.otlp.directives, value);
            set_config_value!(inner.logging.otlp.ignore_log_targets, value);
        }
        #[cfg(feature = "flame")]
        {
            set_config_value!(inner.logging.flame.enabled, value);
            set_config_value!(inner.logging.flame.path, value);
        }
        #[cfg(all(unix, feature = "perfetto"))]
        {
            set_config_value!(inner.logging.perfetto.enabled, value);
            set_config_value!(inner.logging.perfetto.path, value);
        }
        #[cfg(feature = "tokio-console")]
        set_config_value!(inner.logging.console.enabled, value);
        set_config_value!(inner.testing.subnode_index, value);
        #[cfg(feature = "virtual-network")]
        {
            set_config_value!(inner.testing.virtual_network_server.enabled, value);
            set_config_value!(inner.testing.virtual_network_server.tcp.listen, value);
            set_config_value!(
                inner.testing.virtual_network_server.tcp.listen_address,
                value
            );
            set_config_value!(inner.testing.virtual_network_server.ws.listen, value);
            set_config_value!(
                inner.testing.virtual_network_server.ws.listen_address,
                value
            );
        }
        set_config_value!(inner.core.capabilities.disable, value);
        set_config_value!(inner.core.protected_store.allow_insecure_fallback, value);
        set_config_value!(
            inner.core.protected_store.always_use_insecure_storage,
            value
        );
        set_config_value!(inner.core.protected_store.directory, value);
        set_config_value!(inner.core.protected_store.delete, value);
        set_config_value!(
            inner.core.protected_store.device_encryption_key_password,
            value
        );
        set_config_value!(
            inner
                .core
                .protected_store
                .new_device_encryption_key_password,
            value
        );
        set_config_value!(inner.core.table_store.directory, value);
        set_config_value!(inner.core.table_store.delete, value);
        set_config_value!(
            inner.core.table_store.wipe_on_invalid_device_encryption_key,
            value
        );
        set_config_value!(inner.core.table_store.max_value_size_mb, value);
        set_config_value!(inner.core.block_store.directory, value);
        set_config_value!(inner.core.block_store.delete, value);
        set_config_value!(inner.core.network.max_connections, value);
        set_config_value!(inner.core.network.network_key_password, value);
        set_config_value!(inner.core.network.routing_table.public_keys, value);
        set_config_value!(inner.core.network.routing_table.secret_keys, value);
        set_config_value!(inner.core.network.routing_table.bootstrap, value);
        set_config_value!(inner.core.network.routing_table.bootstrap_keys, value);
        set_config_value!(inner.core.network.rpc.default_route_hop_count, value);
        set_config_value!(inner.core.network.dht.local_subkey_cache_size, value);
        set_config_value!(
            inner.core.network.dht.local_max_subkey_cache_memory_mb,
            value
        );
        set_config_value!(inner.core.network.dht.remote_subkey_cache_size, value);
        set_config_value!(inner.core.network.dht.remote_max_records, value);
        set_config_value!(
            inner.core.network.dht.remote_max_subkey_cache_memory_mb,
            value
        );
        set_config_value!(inner.core.network.dht.remote_max_storage_space_mb, value);
        set_config_value!(inner.core.network.dht.max_concurrent_operations, value);
        set_config_value!(inner.core.network.address_types, value);
        set_config_value!(inner.core.network.upnp, value);
        set_config_value_custom!(
            inner.core.network.detect_address_changes,
            value,
            auto_bool::from_str
        );
        set_config_value!(inner.core.network.tls.certificate_path, value);
        set_config_value!(inner.core.network.tls.private_key_path, value);
        set_config_value!(inner.core.network.tls.connection_initial_timeout_ms, value);
        set_config_value!(inner.core.network.protocol.udp.enabled, value);
        set_config_value!(inner.core.network.protocol.udp.listen_address, value);
        set_config_value!(inner.core.network.protocol.udp.public_address, value);
        set_config_value!(
            inner.core.internal.network.connection_initial_timeout_ms,
            value
        );
        set_config_value!(
            inner.core.internal.network.connection_inactivity_timeout_ms,
            value
        );
        set_config_value!(inner.core.internal.network.max_connections_per_ip4, value);
        set_config_value!(
            inner.core.internal.network.max_connections_per_ip6_prefix,
            value
        );
        set_config_value!(
            inner
                .core
                .internal
                .network
                .max_connections_per_ip6_prefix_size,
            value
        );
        set_config_value!(
            inner.core.internal.network.max_connection_frequency_per_min,
            value
        );
        set_config_value!(
            inner.core.internal.network.client_allowlist_timeout_ms,
            value
        );
        set_config_value!(
            inner
                .core
                .internal
                .network
                .reverse_connection_receipt_time_ms,
            value
        );
        set_config_value!(
            inner.core.internal.network.hole_punch_receipt_time_ms,
            value
        );
        set_config_value!(inner.core.internal.network.restricted_nat_retries, value);
        set_config_value!(inner.core.internal.network.rpc.concurrency, value);
        set_config_value!(inner.core.internal.network.rpc.queue_size, value);
        set_config_value!(
            inner.core.internal.network.rpc.max_timestamp_behind_ms,
            value
        );
        set_config_value!(
            inner.core.internal.network.rpc.max_timestamp_ahead_ms,
            value
        );
        set_config_value!(inner.core.internal.network.rpc.timeout_ms, value);
        set_config_value!(inner.core.internal.network.rpc.max_route_hop_count, value);
        set_config_value!(inner.core.internal.network.dht.max_find_node_count, value);
        set_config_value!(
            inner.core.internal.network.dht.resolve_node_timeout_ms,
            value
        );
        set_config_value!(inner.core.internal.network.dht.resolve_node_count, value);
        set_config_value!(inner.core.internal.network.dht.resolve_node_fanout, value);
        set_config_value!(inner.core.internal.network.dht.get_value_timeout_ms, value);
        set_config_value!(inner.core.internal.network.dht.get_value_count, value);
        set_config_value!(inner.core.internal.network.dht.get_value_fanout, value);
        set_config_value!(inner.core.internal.network.dht.set_value_timeout_ms, value);
        set_config_value!(inner.core.internal.network.dht.set_value_count, value);
        set_config_value!(inner.core.internal.network.dht.set_value_fanout, value);
        set_config_value!(inner.core.internal.network.dht.consensus_width, value);
        set_config_value!(inner.core.internal.network.dht.min_peer_count, value);
        set_config_value!(
            inner.core.internal.network.dht.min_peer_refresh_time_ms,
            value
        );
        set_config_value!(
            inner
                .core
                .internal
                .network
                .dht
                .validate_dial_info_receipt_time_ms,
            value
        );
        set_config_value!(
            inner.core.internal.network.dht.max_watch_expiration_ms,
            value
        );
        set_config_value!(inner.core.internal.network.dht.public_watch_limit, value);
        set_config_value!(inner.core.internal.network.dht.member_watch_limit, value);
        set_config_value!(
            inner.core.internal.network.dht.public_transaction_limit,
            value
        );
        set_config_value!(
            inner.core.internal.network.dht.member_transaction_limit,
            value
        );
        set_config_value!(
            inner.core.internal.network.protocol.udp.socket_pool_size,
            value
        );
        set_config_value!(inner.core.network.protocol.tcp.connect, value);
        set_config_value!(inner.core.network.protocol.tcp.listen, value);
        set_config_value!(inner.core.network.protocol.tcp.listen_address, value);
        set_config_value!(inner.core.network.protocol.tcp.public_address, value);
        set_config_value!(inner.core.network.protocol.ws.connect, value);
        set_config_value!(inner.core.network.protocol.ws.listen, value);
        set_config_value!(inner.core.network.protocol.ws.listen_address, value);
        set_config_value!(inner.core.network.protocol.ws.path, value);
        set_config_value!(inner.core.network.protocol.ws.url, value);

        cfg_if::cfg_if! {
            if #[cfg(feature="enable-protocol-wss")] {
                set_config_value!(inner.core.network.protocol.wss.connect, value);
                set_config_value!(inner.core.network.protocol.wss.listen, value);
                set_config_value!(inner.core.network.protocol.wss.listen_address, value);
                set_config_value!(inner.core.network.protocol.wss.path, value);
                set_config_value!(inner.core.network.protocol.wss.url, value);
            }
        }
        set_config_value!(inner.core.network.privacy.require_inbound_relay, value);
        #[cfg(feature = "geolocation")]
        set_config_value!(inner.core.network.privacy.country_code_denylist, value);
        #[cfg(feature = "virtual-network")]
        {
            set_config_value!(inner.core.network.virtual_network.enabled, value);
            set_config_value!(inner.core.network.virtual_network.server_address, value);
        }

        Err(eyre!("settings key '{key}' not found"))
    }

    pub fn get_core_config(
        &self,
        subnode: u16,
        subnode_offset: u16,
    ) -> Result<veilid_core::VeilidConfig, VeilidAPIError> {
        let inner = self.inner.clone();

        let inner = inner.read();

        let core_config = VeilidConfig {
            program_name: PROGRAM_NAME.into(),
            namespace: subnode_namespace(subnode),
            capabilities: VeilidConfigCapabilities {
                disable: {
                    let mut caps = Vec::<veilid_core::VeilidCapability>::new();
                    for c in &inner.core.capabilities.disable {
                        let cap = veilid_core::VeilidCapability::from_str(c.as_str())?;
                        caps.push(cap);
                    }
                    caps
                },
            },
            protected_store: VeilidConfigProtectedStore {
                allow_insecure_fallback: inner.core.protected_store.allow_insecure_fallback,
                always_use_insecure_storage: inner.core.protected_store.always_use_insecure_storage,
                directory: inner.core.protected_store.directory.clone(),
                delete: inner.core.protected_store.delete,
                device_encryption_key_password: inner
                    .core
                    .protected_store
                    .device_encryption_key_password
                    .clone(),
                new_device_encryption_key_password: inner
                    .core
                    .protected_store
                    .new_device_encryption_key_password
                    .clone(),
            },
            table_store: VeilidConfigTableStore {
                directory: inner.core.table_store.directory.clone(),
                delete: inner.core.table_store.delete,
                wipe_on_invalid_device_encryption_key: inner
                    .core
                    .table_store
                    .wipe_on_invalid_device_encryption_key,
                max_value_size_mb: inner.core.table_store.max_value_size_mb,
            },
            block_store: VeilidConfigBlockStore {
                directory: inner.core.block_store.directory.clone(),
                delete: inner.core.block_store.delete,
            },
            network: VeilidConfigNetwork {
                max_connections: inner.core.network.max_connections,
                network_key_password: inner.core.network.network_key_password.clone(),
                routing_table: VeilidConfigRoutingTable {
                    public_keys: inner
                        .core
                        .network
                        .routing_table
                        .public_keys
                        .clone()
                        .unwrap_or_default(),
                    secret_keys: inner
                        .core
                        .network
                        .routing_table
                        .secret_keys
                        .clone()
                        .unwrap_or_default(),
                    bootstrap: inner.core.network.routing_table.bootstrap.clone(),
                    bootstrap_keys: inner.core.network.routing_table.bootstrap_keys.clone(),
                },
                rpc: VeilidConfigRPC {
                    default_route_hop_count: inner.core.network.rpc.default_route_hop_count,
                },
                dht: VeilidConfigDHT {
                    local_subkey_cache_size: inner.core.network.dht.local_subkey_cache_size,
                    local_max_subkey_cache_memory_mb: inner
                        .core
                        .network
                        .dht
                        .local_max_subkey_cache_memory_mb,
                    remote_subkey_cache_size: inner.core.network.dht.remote_subkey_cache_size,
                    remote_max_records: inner.core.network.dht.remote_max_records,
                    remote_max_subkey_cache_memory_mb: inner
                        .core
                        .network
                        .dht
                        .remote_max_subkey_cache_memory_mb,
                    remote_max_storage_space_mb: inner.core.network.dht.remote_max_storage_space_mb,
                    max_concurrent_operations: inner.core.network.dht.max_concurrent_operations,
                },
                address_types: {
                    let mut ats = Vec::<veilid_core::VeilidConfigAddressType>::new();
                    for at in &inner.core.network.address_types {
                        ats.push(veilid_core::VeilidConfigAddressType::from_str(at.as_str())?);
                    }
                    ats
                },
                upnp: inner.core.network.upnp,
                detect_address_changes: inner.core.network.detect_address_changes,
                tls: VeilidConfigTLS {
                    certificate_path: inner.core.network.tls.certificate_path.clone(),
                    private_key_path: inner.core.network.tls.private_key_path.clone(),
                    connection_initial_timeout_ms: inner
                        .core
                        .network
                        .tls
                        .connection_initial_timeout_ms,
                },
                protocol: VeilidConfigProtocol {
                    udp: VeilidConfigUDP {
                        enabled: inner.core.network.protocol.udp.enabled,
                        listen_address: inner
                            .core
                            .network
                            .protocol
                            .udp
                            .listen_address
                            .with_offset_port(subnode_offset)
                            .map_err(VeilidAPIError::internal)?
                            .name
                            .clone(),
                        public_address: inner
                            .core
                            .network
                            .protocol
                            .udp
                            .public_address
                            .as_ref()
                            .map(|a| a.name.clone()),
                    },
                    tcp: VeilidConfigTCP {
                        connect: inner.core.network.protocol.tcp.connect,
                        listen: inner.core.network.protocol.tcp.listen,
                        listen_address: inner
                            .core
                            .network
                            .protocol
                            .tcp
                            .listen_address
                            .with_offset_port(subnode_offset)
                            .map_err(VeilidAPIError::internal)?
                            .name
                            .clone(),
                        public_address: inner
                            .core
                            .network
                            .protocol
                            .tcp
                            .public_address
                            .as_ref()
                            .map(|a| a.name.clone()),
                    },
                    ws: VeilidConfigWS {
                        connect: inner.core.network.protocol.ws.connect,
                        listen: inner.core.network.protocol.ws.listen,
                        listen_address: inner
                            .core
                            .network
                            .protocol
                            .ws
                            .listen_address
                            .with_offset_port(subnode_offset)
                            .map_err(VeilidAPIError::internal)?
                            .name
                            .clone(),
                        path: inner
                            .core
                            .network
                            .protocol
                            .ws
                            .path
                            .to_string_lossy()
                            .to_string(),
                        url: match inner.core.network.protocol.ws.url {
                            Some(ref a) => Some(
                                a.with_offset_port(subnode_offset)
                                    .map_err(VeilidAPIError::internal)
                                    .map(|x| x.urlstring.clone())?,
                            ),
                            None => None,
                        },
                    },
                    #[cfg(feature = "enable-protocol-wss")]
                    wss: VeilidConfigWSS {
                        connect: inner.core.network.protocol.wss.connect,
                        listen: inner.core.network.protocol.wss.listen,
                        listen_address: inner
                            .core
                            .network
                            .protocol
                            .wss
                            .listen_address
                            .with_offset_port(subnode_offset)
                            .map_err(VeilidAPIError::internal)?
                            .name
                            .clone(),
                        path: inner
                            .core
                            .network
                            .protocol
                            .wss
                            .path
                            .to_string_lossy()
                            .to_string(),
                        url: match inner.core.network.protocol.wss.url {
                            Some(ref a) => Some(
                                a.with_offset_port(subnode_offset)
                                    .map_err(VeilidAPIError::internal)
                                    .map(|x| x.urlstring.clone())?,
                            ),
                            None => None,
                        },
                    },
                },
                privacy: VeilidConfigPrivacy {
                    require_inbound_relay: inner.core.network.privacy.require_inbound_relay,
                    #[cfg(feature = "geolocation")]
                    country_code_denylist: inner.core.network.privacy.country_code_denylist.clone(),
                },
                #[cfg(feature = "virtual-network")]
                virtual_network: VeilidConfigVirtualNetwork {
                    enabled: inner.core.network.virtual_network.enabled,
                    server_address: inner.core.network.virtual_network.server_address.clone(),
                },
            },
            // Footgun tuning passes through unchanged; veilid-core's runtime gate
            // ignores it (with a warning) unless built with `footgun-config`.
            internal: Some(inner.core.internal.clone()),
        };

        Ok(core_config)
    }
}

pub fn subnode_namespace(subnode_index: u16) -> String {
    if subnode_index == 0 {
        "".to_owned()
    } else {
        format!("subnode{}", subnode_index)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config() {
        let cfg = load_default_config().unwrap_or_log();
        let inner = cfg.try_deserialize::<SettingsInner>().unwrap_or_log();
        println!("default settings: {:?}", inner);
    }

    #[test]
    fn test_default_config_settings() {
        let settings = Settings::new(None).unwrap_or_log();

        // The default config must map exactly onto the struct for the active features
        // (no stray keys), or every startup would warn.
        assert!(
            settings.unknown_keys.is_empty(),
            "default config has unrecognized keys: {:?}",
            settings.unknown_keys
        );

        let s = settings.read();
        assert!(!s.daemon.enabled);
        assert_eq!(s.daemon.pid_file, None);
        assert_eq!(s.daemon.chroot, None);
        assert_eq!(s.daemon.working_directory, None);
        assert_eq!(s.daemon.user, None);
        assert_eq!(s.daemon.group, None);
        assert_eq!(s.daemon.stdout_file, None);
        assert_eq!(s.daemon.stderr_file, None);
        assert!(s.client_api.ipc_enabled);
        assert!(!s.client_api.network_enabled);
        assert_eq!(s.client_api.listen_address.name, "localhost:5959");
        assert_eq!(
            s.client_api.listen_address.addrs,
            listen_address_to_socket_addrs("localhost:5959").unwrap_or_log()
        );
        assert!(s.auto_attach);
        assert!(!s.logging.system.enabled);
        assert_eq!(s.logging.system.level, LogLevel::Info);
        assert!(s.logging.terminal.enabled);
        assert_eq!(s.logging.terminal.level, LogLevel::Info);
        assert!(!s.logging.file.enabled);
        assert_eq!(s.logging.file.path, "");
        assert!(s.logging.file.append);
        assert_eq!(s.logging.file.level, LogLevel::Info);
        assert!(s.logging.api.enabled);
        assert_eq!(s.logging.api.level, LogLevel::Info);
        #[cfg(feature = "opentelemetry-otlp")]
        {
            assert!(!s.logging.otlp.enabled);
            assert_eq!(s.logging.otlp.level, LogLevel::Trace);
            assert_eq!(
                s.logging.otlp.grpc_endpoint,
                NamedSocketAddrs::from_str("localhost:4317").unwrap_or_log()
            );
        }
        #[cfg(feature = "flame")]
        {
            assert!(!s.logging.flame.enabled);
            assert_eq!(s.logging.flame.path, "");
        }
        #[cfg(all(unix, feature = "perfetto"))]
        {
            assert!(!s.logging.perfetto.enabled);
            assert_eq!(s.logging.perfetto.path, "");
        }
        #[cfg(feature = "tokio-console")]
        assert!(!s.logging.console.enabled);
        assert_eq!(s.testing.subnode_index, 0);
        #[cfg(feature = "virtual-network")]
        {
            assert_eq!(s.testing.virtual_network_server.enabled, false);
            assert_eq!(s.testing.virtual_network_server.tcp.listen, false);
            assert_eq!(
                s.testing.virtual_network_server.tcp.listen_address,
                "localhost:5149"
            );
            assert_eq!(s.testing.virtual_network_server.ws.listen, false);
            assert_eq!(
                s.testing.virtual_network_server.ws.listen_address,
                "localhost:5148"
            );
        }
        assert_eq!(
            s.core.table_store.directory,
            Settings::get_default_table_store_directory()
                .to_string_lossy()
                .to_string()
        );
        assert!(!s.core.table_store.delete);
        assert!(!s.core.table_store.wipe_on_invalid_device_encryption_key);
        assert_eq!(s.core.table_store.max_value_size_mb, 64);

        assert_eq!(
            s.core.block_store.directory,
            Settings::get_default_block_store_directory()
                .to_string_lossy()
                .to_string()
        );
        assert!(!s.core.block_store.delete);

        assert!(s.core.protected_store.allow_insecure_fallback);
        assert!(s.core.protected_store.always_use_insecure_storage);
        assert_eq!(
            s.core.protected_store.directory,
            Settings::get_default_protected_store_directory()
                .to_string_lossy()
                .to_string()
        );
        assert!(!s.core.protected_store.delete);
        assert_eq!(s.core.protected_store.device_encryption_key_password, "");
        assert_eq!(
            s.core.protected_store.new_device_encryption_key_password,
            None
        );

        assert_eq!(
            s.core.internal.network.connection_initial_timeout_ms,
            2_000u32
        );
        assert_eq!(
            s.core.internal.network.connection_inactivity_timeout_ms,
            60_000u32
        );
        assert_eq!(s.core.internal.network.max_connections_per_ip4, 32u32);
        assert_eq!(
            s.core.internal.network.max_connections_per_ip6_prefix,
            32u32
        );
        assert_eq!(
            s.core.internal.network.max_connections_per_ip6_prefix_size,
            56u32
        );
        assert_eq!(
            s.core.internal.network.max_connection_frequency_per_min,
            128u32
        );
        assert_eq!(
            s.core.internal.network.client_allowlist_timeout_ms,
            300_000u32
        );
        assert_eq!(
            s.core.internal.network.reverse_connection_receipt_time_ms,
            5_000u32
        );
        assert_eq!(s.core.internal.network.hole_punch_receipt_time_ms, 5_000u32);
        assert_eq!(s.core.network.max_connections, 256);
        assert_eq!(s.core.network.network_key_password, None);
        assert_eq!(s.core.network.routing_table.public_keys, None);
        assert_eq!(s.core.network.routing_table.secret_keys, None);
        //
        assert_eq!(
            s.core.network.routing_table.bootstrap,
            vec!["bootstrap-v1.veilid.net".to_owned()]
        );
        assert_eq!(
            s.core.network.routing_table.bootstrap_keys,
            vec![
                PublicKey::from_str("VLD0:Vj0lKDdUQXmQ5Ol1SZdlvXkBHUccBcQvGLN9vbLSI7k")
                    .unwrap_or_log(),
                PublicKey::from_str("VLD0:QeQJorqbXtC7v3OlynCZ_W3m76wGNeB5NTF81ypqHAo")
                    .unwrap_or_log(),
                PublicKey::from_str("VLD0:QNdcl-0OiFfYVj9331XVR6IqZ49NG-E18d5P7lwi4TA")
                    .unwrap_or_log(),
            ]
        );
        //
        assert_eq!(s.core.internal.network.rpc.concurrency, 0);
        assert_eq!(s.core.internal.network.rpc.queue_size, 1024);
        assert_eq!(
            s.core.internal.network.rpc.max_timestamp_behind_ms,
            Some(10_000u32)
        );
        assert_eq!(
            s.core.internal.network.rpc.max_timestamp_ahead_ms,
            Some(10_000u32)
        );
        assert_eq!(s.core.internal.network.rpc.timeout_ms, 5_000u32);
        assert_eq!(s.core.internal.network.rpc.max_route_hop_count, 4);
        assert_eq!(s.core.network.rpc.default_route_hop_count, 1);
        //
        assert_eq!(s.core.network.dht.local_subkey_cache_size, 128u32);
        assert_eq!(s.core.network.dht.local_max_subkey_cache_memory_mb, 256u32);
        assert_eq!(s.core.network.dht.remote_subkey_cache_size, 1024u32);
        assert_eq!(s.core.network.dht.remote_max_records, 65536u32);
        //
        assert_eq!(s.core.internal.network.dht.max_find_node_count, 20u32);
        assert_eq!(
            s.core.internal.network.dht.resolve_node_timeout_ms,
            10_000u32
        );
        assert_eq!(s.core.internal.network.dht.resolve_node_count, 1u32);
        assert_eq!(s.core.internal.network.dht.resolve_node_fanout, 5u32);
        assert_eq!(s.core.internal.network.dht.get_value_timeout_ms, 10_000u32);
        assert_eq!(s.core.internal.network.dht.get_value_count, 3u32);
        assert_eq!(s.core.internal.network.dht.get_value_fanout, 5u32);
        assert_eq!(s.core.internal.network.dht.set_value_timeout_ms, 10_000u32);
        assert_eq!(s.core.internal.network.dht.set_value_count, 5u32);
        assert_eq!(s.core.internal.network.dht.set_value_fanout, 6u32);
        assert_eq!(s.core.internal.network.dht.consensus_width, 10u32);
        assert_eq!(s.core.internal.network.dht.min_peer_count, 20u32);
        assert_eq!(
            s.core.internal.network.dht.min_peer_refresh_time_ms,
            60_000u32
        );
        assert_eq!(
            s.core
                .internal
                .network
                .dht
                .validate_dial_info_receipt_time_ms,
            1_000u32
        );
        assert_eq!(s.core.internal.network.dht.public_watch_limit, 32u32);
        assert_eq!(s.core.internal.network.dht.member_watch_limit, 8u32);
        assert_eq!(
            s.core.internal.network.dht.max_watch_expiration_ms,
            600_000u32
        );
        assert_eq!(s.core.internal.network.dht.public_transaction_limit, 4u32);
        assert_eq!(s.core.internal.network.dht.member_transaction_limit, 1u32);
        //
        assert!(!s.core.network.upnp);
        assert_eq!(s.core.network.detect_address_changes, None);
        assert_eq!(s.core.internal.network.restricted_nat_retries, 0u32);
        //
        assert_eq!(
            s.core.network.tls.certificate_path,
            Settings::get_default_tls_certificate_path()
                .to_string_lossy()
                .to_string()
        );
        assert_eq!(
            s.core.network.tls.private_key_path,
            Settings::get_default_tls_private_key_path()
                .to_string_lossy()
                .to_string()
        );
        assert_eq!(s.core.network.tls.connection_initial_timeout_ms, 2_000u32);
        //
        //
        let valid_socket_addrs = [
            SocketAddr::new(IpAddr::V4(Ipv4Addr::new(0, 0, 0, 0)), 5150),
            SocketAddr::new(IpAddr::V6(Ipv6Addr::new(0, 0, 0, 0, 0, 0, 0, 0)), 5150),
        ];

        assert!(s.core.network.protocol.udp.enabled);
        assert_eq!(s.core.internal.network.protocol.udp.socket_pool_size, 0);
        assert_eq!(s.core.network.protocol.udp.listen_address.name, ":5150");
        for addr in &s.core.network.protocol.udp.listen_address.addrs {
            assert!(valid_socket_addrs.contains(addr));
        }
        assert!(!s.core.network.protocol.udp.listen_address.addrs.is_empty());
        assert_eq!(s.core.network.protocol.udp.public_address, None);

        //
        assert!(s.core.network.protocol.tcp.connect);
        assert!(s.core.network.protocol.tcp.listen);
        assert_eq!(s.core.network.protocol.tcp.listen_address.name, ":5150");
        for addr in &s.core.network.protocol.tcp.listen_address.addrs {
            assert!(valid_socket_addrs.contains(addr));
        }
        assert!(!s.core.network.protocol.tcp.listen_address.addrs.is_empty());
        assert_eq!(s.core.network.protocol.tcp.public_address, None);

        //
        assert!(s.core.network.protocol.ws.connect);
        assert!(s.core.network.protocol.ws.listen);
        assert_eq!(s.core.network.protocol.ws.listen_address.name, ":5150");
        for addr in &s.core.network.protocol.ws.listen_address.addrs {
            assert!(valid_socket_addrs.contains(addr));
        }
        assert!(!s.core.network.protocol.ws.listen_address.addrs.is_empty());
        assert_eq!(
            s.core.network.protocol.ws.path,
            std::path::PathBuf::from("ws")
        );
        assert_eq!(s.core.network.protocol.ws.url, None);
        //
        cfg_if::cfg_if! {
            if #[cfg(feature="enable-protocol-wss")] {
                assert!(s.core.network.protocol.wss.connect);
                assert!(!s.core.network.protocol.wss.listen);
                assert_eq!(s.core.network.protocol.wss.listen_address.name, ":5150");
                for addr in &s.core.network.protocol.wss.listen_address.addrs {
                    assert!(valid_socket_addrs.contains(addr));
                }
                assert!(!s.core.network.protocol.wss.listen_address.addrs.is_empty());
                assert_eq!(
                    s.core.network.protocol.wss.path,
                    std::path::PathBuf::from("ws")
                );
                assert_eq!(s.core.network.protocol.wss.url, None);
            }
        }
        //
        assert!(!s.core.network.privacy.require_inbound_relay);
        #[cfg(feature = "geolocation")]
        assert_eq!(s.core.network.privacy.country_code_denylist, &[]);
        #[cfg(feature = "virtual-network")]
        {
            assert_eq!(s.core.network.virtual_network.enabled, false);
            assert_eq!(s.core.network.virtual_network.server_address, "");
        }
    }
}
