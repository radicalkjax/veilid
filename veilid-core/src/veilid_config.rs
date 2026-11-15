use crate::*;

cfg_if::cfg_if! {
    if #[cfg(not(all(target_arch = "wasm32", target_os = "unknown")))] {
        use directories::ProjectDirs;
    }
}

cfg_if::cfg_if! {
    if #[cfg(all(target_arch = "wasm32", target_os = "unknown"))] {
        /// Minimum allowed total `network.max_connections` on wasm32/browser.
        pub const MAX_CONNECTIONS_MIN: u32 = 16;
        /// Maximum allowed total `network.max_connections` on wasm32/browser.
        pub const MAX_CONNECTIONS_MAX: u32 = 64;
    } else {
        /// Minimum allowed total `network.max_connections` on native platforms.
        pub const MAX_CONNECTIONS_MIN: u32 = 32;
        /// Maximum allowed total `network.max_connections` on native platforms.
        pub const MAX_CONNECTIONS_MAX: u32 = 512;
    }
}

/// Enable and configure UDP.
///
/// ```yaml
/// udp:
///     enabled: true
///     socket_pool_size: 0
///     listen_address: ':5150'
///     public_address: ''
/// ```
///
#[apply(api_data_struct!)]
#[api(eq, ts)]
pub struct VeilidConfigUDP {
    /// Enable the UDP protocol.
    pub enabled: bool,
    /// Local address to bind, as `ip:port` (empty binds the default port).
    pub listen_address: String,
    /// Externally-reachable `ip:port` to advertise, if behind NAT/port mapping.
    #[cfg_attr(all(target_arch = "wasm32", target_os = "unknown"), tsify(optional))]
    pub public_address: Option<String>,
}

impl Default for VeilidConfigUDP {
    fn default() -> Self {
        cfg_if::cfg_if! {
            if #[cfg(all(target_arch = "wasm32", target_os = "unknown"))] {
                let enabled = false;
            } else {
                let enabled = true;
            }
        }
        Self {
            enabled,
            listen_address: String::from(""),
            public_address: None,
        }
    }
}

/// Enable and configure TCP.
///
/// ```yaml
/// tcp:
///     connect: true
///     listen: true
///     listen_address: ':5150'
///     public_address: ''
///
#[apply(api_data_struct!)]
#[api(eq, ts)]
pub struct VeilidConfigTCP {
    /// Allow outbound TCP connections.
    pub connect: bool,
    /// Accept inbound TCP connections.
    pub listen: bool,
    /// Local address to bind, as `ip:port` (empty binds the default port).
    pub listen_address: String,
    /// Externally-reachable `ip:port` to advertise, if behind NAT/port mapping.
    #[cfg_attr(all(target_arch = "wasm32", target_os = "unknown"), tsify(optional))]
    pub public_address: Option<String>,
}

impl Default for VeilidConfigTCP {
    fn default() -> Self {
        cfg_if::cfg_if! {
            if #[cfg(all(target_arch = "wasm32", target_os = "unknown"))] {
                let connect = false;
                let listen = false;
            } else {
                let connect = true;
                let listen = true;
            }
        }
        Self {
            connect,
            listen,
            listen_address: String::from(""),
            public_address: None,
        }
    }
}

/// Enable and configure Web Sockets.
///
/// ```yaml
/// ws:
///     connect: true
///     listen: true
///     listen_address: ':5150'
///     path: 'ws'
///     url: 'ws://localhost:5150/ws'
///
#[apply(api_data_struct!)]
#[api(eq, ts)]
pub struct VeilidConfigWS {
    /// Allow outbound WebSocket connections.
    pub connect: bool,
    /// Accept inbound WebSocket connections.
    pub listen: bool,
    /// Local address to bind, as `ip:port` (empty binds the default port).
    pub listen_address: String,
    /// URL path served by the WebSocket listener.
    pub path: String,
    /// Externally-reachable URL to advertise, if behind NAT/proxy.
    #[cfg_attr(all(target_arch = "wasm32", target_os = "unknown"), tsify(optional))]
    pub url: Option<String>,
}

impl Default for VeilidConfigWS {
    fn default() -> Self {
        cfg_if::cfg_if! {
            if #[cfg(all(target_arch = "wasm32", target_os = "unknown"))] {
                let connect = true;
                let listen = false;
            } else {
                let connect = true;
                let listen = true;
            }
        }
        Self {
            connect,
            listen,
            listen_address: String::from(""),
            path: String::from("ws"),
            url: None,
        }
    }
}

/// Enable and configure Secure Web Sockets.
///
/// ```yaml
/// wss:
///     connect: true
///     listen: false
///     listen_address: ':5150'
///     path: 'ws'
///     url: ''
///
#[cfg(feature = "enable-protocol-wss")]
#[apply(api_data_struct!)]
#[api(eq, ts)]
pub struct VeilidConfigWSS {
    /// Allow outbound secure WebSocket connections.
    pub connect: bool,
    /// Accept inbound secure WebSocket connections.
    pub listen: bool,
    /// Local address to bind, as `ip:port` (empty binds the default port).
    pub listen_address: String,
    /// URL path served by the secure WebSocket listener.
    pub path: String,
    /// Externally-reachable URL to advertise (required and validated for TLS protocols).
    #[cfg_attr(all(target_arch = "wasm32", target_os = "unknown"), tsify(optional))]
    pub url: Option<String>, // Fixed URL is not optional for TLS-based protocols and is dynamically validated
}

#[cfg(feature = "enable-protocol-wss")]
impl Default for VeilidConfigWSS {
    fn default() -> Self {
        Self {
            connect: true,
            listen: false,
            listen_address: String::from(""),
            path: String::from("ws"),
            url: None,
        }
    }
}

/// Configure Network Protocols.
///
/// Veilid can communicate over UDP, TCP, and Web Sockets.
///
/// All protocols are available by default, and the Veilid node will
/// sort out which protocol is used for each peer connection.
///
#[apply(api_data_struct!)]
#[api(eq, default, ts)]
pub struct VeilidConfigProtocol {
    /// UDP protocol configuration.
    pub udp: VeilidConfigUDP,
    /// TCP protocol configuration.
    pub tcp: VeilidConfigTCP,
    /// WebSocket protocol configuration.
    pub ws: VeilidConfigWS,
    /// Secure WebSocket protocol configuration.
    #[cfg(feature = "enable-protocol-wss")]
    pub wss: VeilidConfigWSS,
}

/// Privacy preferences for routes.
///
/// ```yaml
/// privacy:
///     require_inbound_relay: false
///     country_code_denylist: [] # only with `--features=geolocation`
/// ```
#[apply(api_data_struct!)]
#[api(eq, default)]
#[cfg_attr(target_arch = "wasm32", derive(Tsify))]
pub struct VeilidConfigPrivacy {
    /// Always use an inbound relay; never accept direct inbound connections.
    pub require_inbound_relay: bool,
    /// Two-letter country codes to refuse routing through (requires `geolocation`).
    #[cfg(feature = "geolocation")]
    pub country_code_denylist: Vec<CountryCode>,
}

/// Virtual networking client support for testing/simulation purposes
///
/// ```yaml
/// virtual_network:
///     enabled: false
///     server_address: ""
/// ```
#[cfg(feature = "virtual-network")]
#[apply(api_data_struct!)]
#[api(eq, default)]
#[cfg_attr(target_arch = "wasm32", derive(Tsify))]
pub struct VeilidConfigVirtualNetwork {
    /// Route all networking through the virtual network server.
    pub enabled: bool,
    /// Address of the virtual network server, as `host:port`.
    pub server_address: String,
}

/// Configure TLS.
///
/// ```yaml
/// tls:
///     certificate_path: /path/to/cert
///     private_key_path: /path/to/private/key
///     connection_initial_timeout_ms: 2000
///
#[apply(api_data_struct!)]
#[api(eq, ts)]
pub struct VeilidConfigTLS {
    /// Path to the TLS certificate (PEM) for inbound TLS protocols.
    pub certificate_path: String,
    /// Path to the TLS private key (PEM) for inbound TLS protocols.
    pub private_key_path: String,
    /// Timeout for completing a TLS handshake, in milliseconds.
    pub connection_initial_timeout_ms: u32,
}

impl Default for VeilidConfigTLS {
    fn default() -> Self {
        Self {
            certificate_path: "".to_string(),
            private_key_path: "".to_string(),
            connection_initial_timeout_ms: 2000,
        }
    }
}

#[cfg_attr(
    all(target_arch = "wasm32", target_os = "unknown"),
    allow(unused_variables)
)]
/// Default directory for TLS certificates and keys, given the program identity and a relative sub-path.
#[must_use]
pub fn get_default_ssl_directory(
    program_name: &str,
    organization: &str,
    qualifier: &str,
    sub_path: &str,
) -> String {
    cfg_if::cfg_if! {
        if #[cfg(all(target_arch = "wasm32", target_os = "unknown"))] {
            "".to_owned()
        } else {
            use std::path::PathBuf;
            ProjectDirs::from(qualifier, organization, program_name)
                .map(|dirs| dirs.data_local_dir().join("ssl").join(sub_path))
                .unwrap_or_else(|| PathBuf::from("./ssl").join(sub_path))
                .to_string_lossy()
                .into()
        }
    }
}

/// Configure the Distributed Hash Table (DHT).
/// Defaults should be used here unless you are absolutely sure you know what you're doing.
/// If you change the count/fanout/timeout parameters, you may render your node inoperable
/// for correct DHT operations.
#[apply(api_data_struct!)]
#[api(eq, ts)]
pub struct VeilidConfigDHT {
    /// Number of subkeys cached for locally-created DHT records.
    pub local_subkey_cache_size: u32,
    /// Memory cap for the local subkey cache, in megabytes.
    pub local_max_subkey_cache_memory_mb: u32,
    /// Number of subkeys cached for DHT records stored on behalf of others.
    pub remote_subkey_cache_size: u32,
    /// Maximum number of remote DHT records stored on behalf of others.
    pub remote_max_records: u32,
    /// Memory cap for the remote subkey cache, in megabytes.
    pub remote_max_subkey_cache_memory_mb: u32,
    /// Disk cap for remote DHT record storage, in megabytes.
    pub remote_max_storage_space_mb: u32,
    /// Max concurrent DHT network operations in flight (local-only ops exempt).
    #[serde(default = "default_dht_max_concurrent_operations")]
    pub max_concurrent_operations: u32,
}

/// Per-platform default for max_concurrent_operations (single source for serde + Default).
fn default_dht_max_concurrent_operations() -> u32 {
    cfg_if::cfg_if! {
        if #[cfg(all(target_arch = "wasm32", target_os = "unknown"))] {
            16
        } else {
            16
        }
    }
}

impl Default for VeilidConfigDHT {
    fn default() -> Self {
        cfg_if::cfg_if! {
            if #[cfg(all(target_arch = "wasm32", target_os = "unknown"))] {
                let local_subkey_cache_size = 128;
                let local_max_subkey_cache_memory_mb = 256;
                let remote_subkey_cache_size = 64;
                let remote_max_records = 64;
                let remote_max_subkey_cache_memory_mb = 256;
                let remote_max_storage_space_mb = 128;
            } else {
                let local_subkey_cache_size = 1024;
                let local_max_subkey_cache_memory_mb = match total_memory_bytes() {
                    Some(mem) => (mem / 32u64 / (1024u64 * 1024u64)) as u32,
                    None => 256,
                };
                let remote_subkey_cache_size = 128;
                let remote_max_records = 128;
                let remote_max_subkey_cache_memory_mb = match total_memory_bytes() {
                    Some(mem) => (mem / 32u64 / (1024u64 * 1024u64)) as u32,
                    None => 256,
                };
                let remote_max_storage_space_mb = 256;
            }
        }

        let max_concurrent_operations = default_dht_max_concurrent_operations();

        Self {
            local_subkey_cache_size,
            local_max_subkey_cache_memory_mb,
            remote_subkey_cache_size,
            remote_max_records,
            remote_max_subkey_cache_memory_mb,
            remote_max_storage_space_mb,
            max_concurrent_operations,
        }
    }
}

/// Configure RPC.
///
#[apply(api_data_struct!)]
#[api(eq, ts)]
pub struct VeilidConfigRPC {
    /// Default number of hops used when allocating private routes.
    pub default_route_hop_count: u8,
}

impl Default for VeilidConfigRPC {
    fn default() -> Self {
        Self {
            default_route_hop_count: 1,
        }
    }
}

/// Configure the network routing table.
///
#[apply(api_data_struct!)]
#[api(eq, ts)]
pub struct VeilidConfigRoutingTable {
    /// This node's identity public keys, by crypto kind (empty = generate fresh).
    #[cfg_attr(feature = "schemars", schemars(with = "Vec<String>"))]
    #[cfg_attr(
        all(target_arch = "wasm32", target_os = "unknown"),
        tsify(type = "string[]")
    )]
    pub public_keys: PublicKeyGroup,
    /// Node identity secret keys matching `public_keys` (empty = generate fresh).
    #[cfg_attr(feature = "schemars", schemars(with = "Vec<String>"))]
    #[cfg_attr(
        all(target_arch = "wasm32", target_os = "unknown"),
        tsify(type = "string[]")
    )]
    pub secret_keys: SecretKeyGroup,
    /// Bootstrap server hostnames/URLs used to join the network.
    pub bootstrap: Vec<String>,
    /// Public keys trusted to sign bootstrap records.
    #[cfg_attr(feature = "schemars", schemars(with = "Vec<String>"))]
    #[cfg_attr(
        all(target_arch = "wasm32", target_os = "unknown"),
        tsify(type = "string[]")
    )]
    pub bootstrap_keys: Vec<PublicKey>,
    /// Bootstrap records, verbatim, as `veilid-server --dump-txt-record` prints them.
    /// Served to direct bootstrap requests without any lookup, and usable to bootstrap this
    /// node itself. Both record versions are served; bootstrapping from them follows the same
    /// rule as a TXT bootstrap, so v0 records are only used when no bootstrap keys are set.
    /// Empty by default, in which case records come from the bootstrap servers.
    pub bootstrap_records: Vec<String>,
    /// Answer direct bootstrap requests (BOOT and B01T) from the last successful bootstrap set.
    /// Off by default: a bootstrap server turns it on, and an app has no reason to serve. A node
    /// that finds itself among its own bootstraps answers regardless.
    #[serde(default)]
    pub serve_direct_bootstrap: bool,
    // xxx pub enable_public_internet: bool,
    // xxx pub enable_local_network: bool,
}

impl Default for VeilidConfigRoutingTable {
    fn default() -> Self {
        cfg_if::cfg_if! {
            if #[cfg(all(target_arch = "wasm32", target_os = "unknown"))] {
                let bootstrap = vec!["ws://bootstrap-v1.veilid.net:5150/ws".to_string()];
            } else {
                let bootstrap = vec!["bootstrap-v1.veilid.net".to_string()];
            }
        }
        let bootstrap_keys = vec![
            // Primary Veilid Foundation bootstrap signing key
            PublicKey::from_str("VLD0:Vj0lKDdUQXmQ5Ol1SZdlvXkBHUccBcQvGLN9vbLSI7k").unwrap_or_log(),
            // Secondary Veilid Foundation bootstrap signing key
            PublicKey::from_str("VLD0:QeQJorqbXtC7v3OlynCZ_W3m76wGNeB5NTF81ypqHAo").unwrap_or_log(),
            // Backup Veilid Foundation bootstrap signing key
            PublicKey::from_str("VLD0:QNdcl-0OiFfYVj9331XVR6IqZ49NG-E18d5P7lwi4TA").unwrap_or_log(),
        ];

        Self {
            public_keys: PublicKeyGroup::default(),
            secret_keys: SecretKeyGroup::default(),
            bootstrap,
            bootstrap_keys,
            bootstrap_records: vec![],
            serve_direct_bootstrap: false,
        }
    }
}

/// An IP address family (IP version) the node may use.
#[apply(api_data_enum!)]
#[api(eq, copy, ord, hash, ts(namespace))]
pub enum VeilidConfigAddressType {
    /// IPv4 (32-bit) addresses.
    #[serde(rename = "IPV4", alias = "ipv4", alias = "v4", alias = "4")]
    Ipv4,
    /// IPv6 (128-bit) addresses.
    #[serde(rename = "IPV6", alias = "ipv6", alias = "v6", alias = "6")]
    Ipv6,
}

impl fmt::Display for VeilidConfigAddressType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            VeilidConfigAddressType::Ipv4 => write!(f, "IPV4"),
            VeilidConfigAddressType::Ipv6 => write!(f, "IPV6"),
        }
    }
}

impl FromStr for VeilidConfigAddressType {
    type Err = VeilidAPIError;
    fn from_str(s: &str) -> VeilidAPIResult<VeilidConfigAddressType> {
        match s.to_ascii_lowercase().as_str() {
            "v4" | "4" | "ipv4" => Ok(VeilidConfigAddressType::Ipv4),
            "v6" | "6" | "ipv6" => Ok(VeilidConfigAddressType::Ipv6),
            _ => apibail_invalid_argument!("invalid VeilidConfigAddressType string", "s", s),
        }
    }
}

/// Network subsystem configuration: connections, routing table, RPC, DHT, transports, and privacy.
#[apply(api_data_struct!)]
#[api(eq, ts)]
pub struct VeilidConfigNetwork {
    /// Maximum total simultaneous connections across all protocols.
    /// Range: native 32-512, wasm32/browser 16-64.
    pub max_connections: u32,
    /// Optional password; its presence joins a private network with a derived network key.
    #[cfg_attr(all(target_arch = "wasm32", target_os = "unknown"), tsify(optional))]
    pub network_key_password: Option<String>,
    /// Routing table identity and bootstrap configuration.
    pub routing_table: VeilidConfigRoutingTable,
    /// RPC configuration.
    pub rpc: VeilidConfigRPC,
    /// DHT cache and storage configuration.
    pub dht: VeilidConfigDHT,
    /// Enabled IP address families (empty = all available).
    #[serde(default)]
    #[cfg_attr(all(target_arch = "wasm32", target_os = "unknown"), tsify(optional))]
    pub address_types: Vec<VeilidConfigAddressType>,
    /// Use UPnP to map ports on the local gateway.
    pub upnp: bool,
    /// Watch for and react to local address changes (`None` = auto-detect).
    pub detect_address_changes: Option<bool>,
    /// Wait before retrying a listen address that failed to bind, in milliseconds.
    #[serde(default = "default_bind_retry_ms")]
    #[cfg_attr(all(target_arch = "wasm32", target_os = "unknown"), tsify(optional))]
    pub bind_retry_ms: u32,
    /// TLS configuration for inbound secure protocols.
    pub tls: VeilidConfigTLS,
    /// Per-protocol (UDP/TCP/WS/WSS) configuration.
    pub protocol: VeilidConfigProtocol,
    /// Privacy and relay preferences.
    pub privacy: VeilidConfigPrivacy,
    /// Virtual network client configuration (testing/simulation).
    #[cfg(feature = "virtual-network")]
    pub virtual_network: VeilidConfigVirtualNetwork,
}

impl Default for VeilidConfigNetwork {
    fn default() -> Self {
        Self {
            max_connections: 32,
            network_key_password: None,
            address_types: Vec::new(),
            routing_table: VeilidConfigRoutingTable::default(),
            rpc: VeilidConfigRPC::default(),
            dht: VeilidConfigDHT::default(),
            upnp: true,
            detect_address_changes: Some(true),
            bind_retry_ms: default_bind_retry_ms(),
            tls: VeilidConfigTLS::default(),
            protocol: VeilidConfigProtocol::default(),
            privacy: VeilidConfigPrivacy::default(),
            #[cfg(feature = "virtual-network")]
            virtual_network: VeilidConfigVirtualNetwork::default(),
        }
    }
}

/// Default for bind_retry_ms (single source for serde + Default).
fn default_bind_retry_ms() -> u32 {
    10_000
}

/// Table store configuration: the encrypted key-value database backing node state.
#[apply(api_data_struct!)]
#[api(eq, ts)]
pub struct VeilidConfigTableStore {
    /// Directory holding the table store database (empty = platform default).
    pub directory: String,
    /// Delete the table store on startup.
    pub delete: bool,
    /// Wipe the table store on an invalid device encryption key, rather than failing.
    pub wipe_on_invalid_device_encryption_key: bool,
    /// Maximum size of a single stored value, in megabytes.
    pub max_value_size_mb: u32,
}

impl Default for VeilidConfigTableStore {
    fn default() -> Self {
        Self {
            directory: "".to_string(),
            delete: false,
            wipe_on_invalid_device_encryption_key: true,
            max_value_size_mb: 64,
        }
    }
}

#[cfg_attr(
    all(target_arch = "wasm32", target_os = "unknown"),
    allow(unused_variables)
)]
#[must_use]
fn get_default_store_path(
    program_name: &str,
    organization: &str,
    qualifier: &str,
    store_type: &str,
) -> String {
    cfg_if::cfg_if! {
        if #[cfg(all(target_arch = "wasm32", target_os = "unknown"))] {
            "".to_owned()
        } else {
            use std::path::PathBuf;
            ProjectDirs::from(qualifier, organization, program_name)
                .map(|dirs| dirs.data_local_dir().to_path_buf())
                .unwrap_or_else(|| PathBuf::from("./"))
                .join(store_type)
                .to_string_lossy()
                .into()
        }
    }
}

/// Block store configuration: content-addressed block storage.
#[apply(api_data_struct!)]
#[api(eq, ts)]
pub struct VeilidConfigBlockStore {
    /// Directory holding the block store (empty = platform default).
    pub directory: String,
    /// Delete the block store on startup.
    pub delete: bool,
}

impl Default for VeilidConfigBlockStore {
    fn default() -> Self {
        Self {
            directory: "".to_string(),
            delete: false,
        }
    }
}

/// Which storage backend the protected store uses for secrets.
///
/// The browser-only variants (`LocalStorage`, `IndexedDb`, `ExtensionStorage`) are rejected on
/// native platforms. `PlatformDefault` chooses per platform and runtime context: the OS keyring on
/// native, `localStorage` in a browser Window, IndexedDB in a worker, and extension storage in a
/// WebExtension.
#[apply(api_data_enum!)]
#[api(eq, copy, ord, default, ts(namespace))]
pub enum VeilidConfigProtectedStoreBackend {
    /// Choose automatically based on the platform and runtime context.
    #[default]
    PlatformDefault,
    /// Browser `localStorage` (Window contexts only).
    LocalStorage,
    /// Browser IndexedDB (available in worker contexts).
    IndexedDb,
    /// WebExtension storage (`chrome.storage.local`/`browser.storage.local`).
    ExtensionStorage,
    // /// Delegate to a registered secret-resolver callback.
    // Callback,
}

/// Protected store configuration: where secrets such as the device encryption key are kept.
#[apply(api_data_struct!)]
#[api(eq, ts)]
pub struct VeilidConfigProtectedStore {
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
    #[cfg_attr(all(target_arch = "wasm32", target_os = "unknown"), tsify(optional))]
    pub new_device_encryption_key_password: Option<String>,
    /// Storage backend for secrets (`PlatformDefault` chooses per platform and runtime context).
    pub backend: VeilidConfigProtectedStoreBackend,
}

impl Default for VeilidConfigProtectedStore {
    fn default() -> Self {
        Self {
            allow_insecure_fallback: false,
            always_use_insecure_storage: false,
            directory: "".to_string(),
            delete: false,
            device_encryption_key_password: "".to_owned(),
            new_device_encryption_key_password: None,
            backend: VeilidConfigProtectedStoreBackend::PlatformDefault,
        }
    }
}

/// Capabilities advertised by this node.
#[apply(api_data_struct!)]
#[api(eq, default, ts)]
pub struct VeilidConfigCapabilities {
    /// Capabilities to disable (advertised as unavailable).
    pub disable: Vec<VeilidCapability>,
}

/// How the node lives through its host application moving between foreground and background.
///
/// ```yaml
/// app_lifecycle:
///     prefer_foreground: true
///     auto_detect_state: true
/// ```
#[cfg(feature = "app-lifecycle")]
#[apply(api_data_struct!)]
#[api(eq, ts)]
pub struct VeilidConfigAppLifecycle {
    /// While attached, keep a hidden desktop app from being throttled (macOS App Nap, Windows power throttling); no effect on mobile.
    pub prefer_foreground: bool,
    /// Follow the platform's own lifecycle reports where it has them; when false, only `report_app_lifecycle_state` counts.
    #[serde(default = "default_auto_detect_state")]
    pub auto_detect_state: bool,
}

#[cfg(feature = "app-lifecycle")]
impl Default for VeilidConfigAppLifecycle {
    fn default() -> Self {
        Self {
            prefer_foreground: true,
            auto_detect_state: default_auto_detect_state(),
        }
    }
}

/// Default for auto_detect_state (single source for serde + Default).
#[cfg(feature = "app-lifecycle")]
fn default_auto_detect_state() -> bool {
    true
}

/// Attachment configuration, owned by the attachment manager.
///
/// ```yaml
/// attachment:
///     app_lifecycle: # only with `--features=app-lifecycle`
///         prefer_foreground: true
///         auto_detect_state: true
/// ```
#[apply(api_data_struct!)]
#[api(eq, default, ts)]
pub struct VeilidConfigAttachment {
    /// Host application lifecycle handling (requires `app-lifecycle`).
    #[cfg(feature = "app-lifecycle")]
    #[serde(default)]
    #[cfg_attr(all(target_arch = "wasm32", target_os = "unknown"), tsify(optional))]
    pub app_lifecycle: VeilidConfigAppLifecycle,
}

/// Logging level threshold (`Off` disables logging).
#[apply(api_data_enum!)]
#[api(eq, copy, ord, default, ts(namespace))]
pub enum VeilidConfigLogLevel {
    /// Logging disabled.
    #[default]
    Off,
    /// Errors only.
    Error,
    /// Warnings and above.
    Warn,
    /// Informational messages and above.
    Info,
    /// Debug messages and above.
    Debug,
    /// All messages, including trace.
    Trace,
}

impl From<VeilidLogLevel> for VeilidConfigLogLevel {
    fn from(value: VeilidLogLevel) -> Self {
        match value {
            VeilidLogLevel::Error => Self::Error,
            VeilidLogLevel::Warn => Self::Warn,
            VeilidLogLevel::Info => Self::Info,
            VeilidLogLevel::Debug => Self::Debug,
            VeilidLogLevel::Trace => Self::Trace,
        }
    }
}

impl From<Option<VeilidLogLevel>> for VeilidConfigLogLevel {
    fn from(value: Option<VeilidLogLevel>) -> Self {
        match value {
            None => Self::Off,
            Some(VeilidLogLevel::Error) => Self::Error,
            Some(VeilidLogLevel::Warn) => Self::Warn,
            Some(VeilidLogLevel::Info) => Self::Info,
            Some(VeilidLogLevel::Debug) => Self::Debug,
            Some(VeilidLogLevel::Trace) => Self::Trace,
        }
    }
}

impl From<tracing::level_filters::LevelFilter> for VeilidConfigLogLevel {
    fn from(value: tracing::level_filters::LevelFilter) -> Self {
        match value {
            tracing::level_filters::LevelFilter::OFF => Self::Off,
            tracing::level_filters::LevelFilter::ERROR => Self::Error,
            tracing::level_filters::LevelFilter::WARN => Self::Warn,
            tracing::level_filters::LevelFilter::INFO => Self::Info,
            tracing::level_filters::LevelFilter::DEBUG => Self::Debug,
            tracing::level_filters::LevelFilter::TRACE => Self::Trace,
        }
    }
}

impl From<VeilidConfigLogLevel> for tracing::level_filters::LevelFilter {
    fn from(val: VeilidConfigLogLevel) -> Self {
        match val {
            VeilidConfigLogLevel::Off => tracing::level_filters::LevelFilter::OFF,
            VeilidConfigLogLevel::Error => tracing::level_filters::LevelFilter::ERROR,
            VeilidConfigLogLevel::Warn => tracing::level_filters::LevelFilter::WARN,
            VeilidConfigLogLevel::Info => tracing::level_filters::LevelFilter::INFO,
            VeilidConfigLogLevel::Debug => tracing::level_filters::LevelFilter::DEBUG,
            VeilidConfigLogLevel::Trace => tracing::level_filters::LevelFilter::TRACE,
        }
    }
}

impl From<tracing::log::LevelFilter> for VeilidConfigLogLevel {
    fn from(value: tracing::log::LevelFilter) -> Self {
        match value {
            tracing::log::LevelFilter::Off => Self::Off,
            tracing::log::LevelFilter::Error => Self::Error,
            tracing::log::LevelFilter::Warn => Self::Warn,
            tracing::log::LevelFilter::Info => Self::Info,
            tracing::log::LevelFilter::Debug => Self::Debug,
            tracing::log::LevelFilter::Trace => Self::Trace,
        }
    }
}

impl From<VeilidConfigLogLevel> for tracing::log::LevelFilter {
    fn from(val: VeilidConfigLogLevel) -> Self {
        match val {
            VeilidConfigLogLevel::Off => tracing::log::LevelFilter::Off,
            VeilidConfigLogLevel::Error => tracing::log::LevelFilter::Error,
            VeilidConfigLogLevel::Warn => tracing::log::LevelFilter::Warn,
            VeilidConfigLogLevel::Info => tracing::log::LevelFilter::Info,
            VeilidConfigLogLevel::Debug => tracing::log::LevelFilter::Debug,
            VeilidConfigLogLevel::Trace => tracing::log::LevelFilter::Trace,
        }
    }
}

impl TryFrom<&str> for VeilidConfigLogLevel {
    type Error = VeilidAPIError;

    fn try_from(value: &str) -> Result<Self, <Self as TryFrom<&str>>::Error> {
        Self::from_str(value)
    }
}

impl TryFrom<String> for VeilidConfigLogLevel {
    type Error = VeilidAPIError;

    fn try_from(value: String) -> Result<Self, <Self as TryFrom<String>>::Error> {
        Self::from_str(value.as_str())
    }
}

impl TryFrom<&String> for VeilidConfigLogLevel {
    type Error = VeilidAPIError;

    fn try_from(value: &String) -> Result<Self, <Self as TryFrom<&String>>::Error> {
        Self::from_str(value.as_str())
    }
}

impl FromStr for VeilidConfigLogLevel {
    type Err = VeilidAPIError;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(match s.to_ascii_lowercase().as_str() {
            "off" => Self::Off,
            "error" => Self::Error,
            "warn" => Self::Warn,
            "info" => Self::Info,
            "debug" => Self::Debug,
            "trace" => Self::Trace,
            _ => {
                apibail_invalid_argument!("invalid VeilidConfigLogLevel string", "s", s);
            }
        })
    }
}
impl fmt::Display for VeilidConfigLogLevel {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> Result<(), fmt::Error> {
        let text = match self {
            Self::Off => "Off",
            Self::Error => "Error",
            Self::Warn => "Warn",
            Self::Info => "Info",
            Self::Debug => "Debug",
            Self::Trace => "Trace",
        };
        write!(f, "{}", text)
    }
}

/// Internal "footgun" UDP configuration. See [VeilidConfigInternal].
#[apply(api_data_struct!)]
#[api(eq, default, ts)]
pub struct VeilidConfigInternalUDP {
    /// Number of UDP sockets in the send/receive pool (0 = automatic).
    pub socket_pool_size: u32,
}

/// Internal "footgun" per-protocol configuration. See [VeilidConfigInternal].
#[apply(api_data_struct!)]
#[api(eq, default, ts)]
pub struct VeilidConfigInternalProtocol {
    /// Internal UDP tuning.
    pub udp: VeilidConfigInternalUDP,
}

/// Internal "footgun" RPC configuration. See [VeilidConfigInternal].
#[apply(api_data_struct!)]
#[api(eq, ts)]
pub struct VeilidConfigInternalRPC {
    /// Number of concurrent RPC worker tasks (0 = automatic).
    pub concurrency: u32,
    /// Maximum number of queued RPC operations.
    pub queue_size: u32,
    /// Reject messages timestamped more than this many ms in the past (`None` = no limit).
    #[cfg_attr(all(target_arch = "wasm32", target_os = "unknown"), tsify(optional))]
    pub max_timestamp_behind_ms: Option<u32>,
    /// Reject messages timestamped more than this many ms in the future (`None` = no limit).
    #[cfg_attr(all(target_arch = "wasm32", target_os = "unknown"), tsify(optional))]
    pub max_timestamp_ahead_ms: Option<u32>,
    /// Timeout for an RPC round-trip, in milliseconds.
    pub timeout_ms: u32,
    /// Maximum number of hops allowed in a route.
    pub max_route_hop_count: u8,
}
impl Default for VeilidConfigInternalRPC {
    fn default() -> Self {
        Self {
            concurrency: 0,
            queue_size: 1024,
            max_timestamp_behind_ms: Some(10000),
            max_timestamp_ahead_ms: Some(10000),
            timeout_ms: 5000,
            max_route_hop_count: 4,
        }
    }
}

/// Internal "footgun" DHT configuration. See [VeilidConfigInternal].
/// Changing the count/fanout/timeout parameters may render your node inoperable for
/// correct DHT operations.
#[apply(api_data_struct!)]
#[api(eq, ts)]
pub struct VeilidConfigInternalDHT {
    /// Maximum number of nodes returned by a FindNode query.
    pub max_find_node_count: u32,
    /// Timeout for resolving a node, in milliseconds.
    pub resolve_node_timeout_ms: u32,
    /// Number of nodes sought when resolving a node.
    pub resolve_node_count: u32,
    /// Parallel fanout width when resolving a node.
    pub resolve_node_fanout: u32,
    /// Timeout for a GetValue operation, in milliseconds.
    pub get_value_timeout_ms: u32,
    /// Number of matching values sought for GetValue consensus.
    pub get_value_count: u32,
    /// Parallel fanout width for GetValue.
    pub get_value_fanout: u32,
    /// Timeout for a SetValue operation, in milliseconds.
    pub set_value_timeout_ms: u32,
    /// Number of nodes that must accept a SetValue for consensus.
    pub set_value_count: u32,
    /// Parallel fanout width for SetValue.
    pub set_value_fanout: u32,
    /// Maximum number of nodes considered 'close to a record key' for storing a record.
    pub consensus_width: u32,
    /// Minimum number of peers to keep in the routing table.
    pub min_peer_count: u32,
    /// Minimum interval between peer-refresh rounds, in milliseconds.
    pub min_peer_refresh_time_ms: u32,
    /// Time allowed to receive a dial-info validation receipt, in milliseconds.
    pub validate_dial_info_receipt_time_ms: u32,
    /// Maximum lifetime of a DHT watch, in milliseconds.
    pub max_watch_expiration_ms: u32,
    /// Longest interval between change inspections of a watch, in milliseconds: the interval
    /// doubles up to it while inspections find nothing. Half the watch lifetime by default, so a
    /// quiet watch inspects before each renewal; a watch short of consensus delivers changes at
    /// most this late.
    pub max_watch_inspect_interval_ms: u32,
    /// Maximum concurrent watches by anonymous watchers (signer not a schema member).
    pub public_watch_limit: u32,
    /// Reserved watch slots for watchers whose signer is a schema member of the record.
    pub member_watch_limit: u32,
    /// Maximum concurrent transactions by anonymous signers (not a schema member).
    pub public_transaction_limit: u32,
    /// Reserved transaction slots for signers who are schema members of the record.
    pub member_transaction_limit: u32,
}
impl Default for VeilidConfigInternalDHT {
    fn default() -> Self {
        Self {
            max_find_node_count: 20,
            resolve_node_timeout_ms: 10000,
            resolve_node_count: 1,
            resolve_node_fanout: 5,
            get_value_timeout_ms: 10000,
            get_value_count: 3,
            get_value_fanout: 5,
            set_value_timeout_ms: 10000,
            set_value_count: 5,
            set_value_fanout: 6,
            consensus_width: 10,
            min_peer_count: 20,
            min_peer_refresh_time_ms: 60000,
            validate_dial_info_receipt_time_ms: 1000,
            max_watch_expiration_ms: 600000,
            max_watch_inspect_interval_ms: 300000,
            public_watch_limit: 32,
            member_watch_limit: 8,
            public_transaction_limit: 8,
            member_transaction_limit: 4,
        }
    }
}

/// Internal "footgun" routing table save schedule. See [VeilidConfigInternal].
#[apply(api_data_struct!)]
#[api(eq, ts)]
pub struct VeilidConfigInternalRoutingTableSave {
    /// Quiet time after a change worth saving before the table is saved, in milliseconds.
    pub settle_ms: u32,
    /// Least time between saves driven by changes, in milliseconds.
    pub min_interval_ms: u32,
    /// Least time between saves in the background, where the process may be frozen within
    /// seconds, in milliseconds.
    pub background_min_interval_ms: u32,
    /// Longest the table goes unsaved, so changes too small to drive a save still reach disk,
    /// in milliseconds.
    pub period_ms: u32,
}
impl Default for VeilidConfigInternalRoutingTableSave {
    fn default() -> Self {
        Self {
            settle_ms: 5000,
            min_interval_ms: 10000,
            background_min_interval_ms: 5000,
            period_ms: 300000,
        }
    }
}

/// Internal "footgun" routing table configuration. See [VeilidConfigInternal].
#[apply(api_data_struct!)]
#[api(eq, default, ts)]
pub struct VeilidConfigInternalRoutingTable {
    /// When the routing table is saved.
    pub save: VeilidConfigInternalRoutingTableSave,
}

/// Internal "footgun" network configuration. See [VeilidConfigInternal].
#[apply(api_data_struct!)]
#[api(eq, ts)]
pub struct VeilidConfigInternalNetwork {
    /// Timeout to establish a new connection, in milliseconds.
    pub connection_initial_timeout_ms: u32,
    /// Cap on any single low-level send, in milliseconds, so a send on a wedged
    /// transport cannot consume the caller's whole RPC timeout budget.
    pub single_send_timeout_ms: u32,
    /// Idle time before an inactive connection is dropped, in milliseconds.
    pub connection_inactivity_timeout_ms: u32,
    /// Maximum simultaneous connections from a single IPv4 address.
    pub max_connections_per_ip4: u32,
    /// Maximum simultaneous connections from a single IPv6 prefix.
    pub max_connections_per_ip6_prefix: u32,
    /// IPv6 prefix length (bits) used to group connections for the per-prefix limit.
    pub max_connections_per_ip6_prefix_size: u32,
    /// Maximum new connections accepted per minute from one source.
    pub max_connection_frequency_per_min: u32,
    /// Time a client stays on the allowlist after connecting, in milliseconds.
    pub client_allowlist_timeout_ms: u32,
    /// Time allowed to receive a reverse-connection receipt, in milliseconds.
    pub reverse_connection_receipt_time_ms: u32,
    /// Time allowed to receive a hole-punch receipt, in milliseconds.
    pub hole_punch_receipt_time_ms: u32,
    /// TTL (hop limit) of a hole punch, enough to pass this node's NAT but not reach the peer's.
    pub default_hole_punch_ttl: u32,
    /// Hole punch TTL behind a carrier-grade NAT, which sits more hops out than a home router.
    pub cgnat_hole_punch_ttl: u32,
    /// NAT-detection retries during dial-info confirmation for port/address-restricted NAT
    /// (some NATs open to full-cone after a few attempts; off by default).
    pub restricted_nat_retries: u32,
    /// Internal RPC tuning.
    pub rpc: VeilidConfigInternalRPC,
    /// Internal DHT tuning.
    pub dht: VeilidConfigInternalDHT,
    /// Internal per-protocol tuning.
    pub protocol: VeilidConfigInternalProtocol,
    /// Internal routing table tuning.
    pub routing_table: VeilidConfigInternalRoutingTable,
}
impl Default for VeilidConfigInternalNetwork {
    fn default() -> Self {
        Self {
            connection_initial_timeout_ms: 5000,
            single_send_timeout_ms: 2000,
            connection_inactivity_timeout_ms: 60000,
            max_connections_per_ip4: 32,
            max_connections_per_ip6_prefix: 32,
            max_connections_per_ip6_prefix_size: 56,
            max_connection_frequency_per_min: 128,
            client_allowlist_timeout_ms: 300000,
            reverse_connection_receipt_time_ms: 5000,
            hole_punch_receipt_time_ms: 5000,
            default_hole_punch_ttl: 3,
            cgnat_hole_punch_ttl: 8,
            restricted_nat_retries: 0,
            rpc: VeilidConfigInternalRPC::default(),
            dht: VeilidConfigInternalDHT::default(),
            protocol: VeilidConfigInternalProtocol::default(),
            routing_table: VeilidConfigInternalRoutingTable::default(),
        }
    }
}

/// Internal "footgun" configuration tree, parallel to the main config.
///
/// These fields tune low-level network/DHT timing, fanout, consensus and connection limits.
/// Changing them from the defaults can render a node inoperable. They are only honored when
/// veilid-core is built with the `footgun-config` feature; without it, any non-default values
/// here are reset to defaults at startup (with a warning).
#[apply(api_data_struct!)]
#[api(eq, default, ts)]
pub struct VeilidConfigInternal {
    /// Internal network tuning.
    pub network: VeilidConfigInternalNetwork,
}

/// Top level of the Veilid configuration tree
#[apply(api_data_struct!)]
#[api(eq, default, ts)]
pub struct VeilidConfig {
    /// An identifier used to describe the program using veilid-core.
    /// Used to partition storage locations in places like the ProtectedStore.
    /// Must be non-empty and a valid filename for all Veilid-capable systems, which means
    /// no backslashes or forward slashes in the name. Stick to a-z,0-9,_ and space and you should be fine.
    ///
    /// Caution: If you change this string, there is no migration support. Your app's protected store and
    /// table store will very likely experience data loss. Pick a program name and stick with it. This is
    /// not a 'visible' identifier and it should uniquely identify your application.
    pub program_name: String,
    /// To run multiple Veilid nodes within the same application, either through a single process running
    /// api_startup/api_startup_json multiple times, or your application running mulitple times side-by-side
    /// there needs to be a key used to partition the application's storage (in the TableStore, ProtectedStore, etc).
    /// An empty value here is the default, but if you run multiple veilid nodes concurrently, you should set this
    /// to a string that uniquely identifies this -instance- within the same 'program_name'.
    /// Must be a valid filename for all Veilid-capable systems, which means no backslashes or forward slashes
    /// in the name. Stick to a-z,0-9,_ and space and you should be fine.
    pub namespace: String,
    /// Capabilities to enable for your application/node
    pub capabilities: VeilidConfigCapabilities,
    /// Configuring the protected store (keychain/keyring/etc)
    pub protected_store: VeilidConfigProtectedStore,
    /// Configuring the table store (persistent encrypted database)
    pub table_store: VeilidConfigTableStore,
    /// Configuring the block store (storage of large content-addressable content)
    pub block_store: VeilidConfigBlockStore,
    /// Configuring how Veilid interacts with the low level network
    pub network: VeilidConfigNetwork,
    /// Configuring how the node attaches to the network
    #[serde(default)]
    #[cfg_attr(all(target_arch = "wasm32", target_os = "unknown"), tsify(optional))]
    pub attachment: VeilidConfigAttachment,
    /// Internal "footgun" tuning. `None` uses safe defaults. Only honored with the
    /// `footgun-config` feature; otherwise ignored (a warning is logged at startup).
    #[cfg_attr(all(target_arch = "wasm32", target_os = "unknown"), tsify(optional))]
    pub internal: Option<VeilidConfigInternal>,
}

impl VeilidConfig {
    /// The effective internal config: configured values with the `footgun-config` feature,
    /// otherwise always the built-in defaults.
    pub fn internal(&self) -> &VeilidConfigInternal {
        static DEFAULT: std::sync::OnceLock<VeilidConfigInternal> = std::sync::OnceLock::new();
        let default = || DEFAULT.get_or_init(VeilidConfigInternal::default);
        #[cfg(feature = "footgun-config")]
        {
            self.internal.as_ref().unwrap_or_else(default)
        }
        #[cfg(not(feature = "footgun-config"))]
        {
            let _ = &self.internal;
            default()
        }
    }

    /// Create a new 'VeilidConfig' for use with `setup_from_config`
    /// Should match the application bundle name if used elsewhere in the format:
    /// `qualifier.organization.program_name` - for example `org.veilid.veilidchat`
    ///
    /// The 'bundle name' will be used when choosing the default storage location for the
    /// application in a platform-dependent fashion, unless 'storage_directory' is
    /// specified to override this location
    ///
    /// * `program_name` - Pick a program name and do not change it from release to release,
    ///   see `VeilidConfig::program_name` for details.
    /// * `organization_name` - Similar to program_name, but for the organization publishing this app
    /// * `qualifier` - Suffix for the application bundle name
    /// * `storage_directory` - Override for the path where veilid-core stores its content
    ///   such as the table store, protected store, and block store
    /// * `config_directory` - Override for the path where veilid-core can retrieve extra configuration files
    ///   such as certificates and keys
    pub fn new(
        program_name: &str,
        organization: &str,
        qualifier: &str,
        storage_directory: Option<&str>,
        config_directory: Option<&str>,
    ) -> Self {
        let mut out = Self {
            program_name: program_name.to_owned(),
            ..Default::default()
        };

        if let Some(storage_directory) = storage_directory {
            out.protected_store.directory = (std::path::PathBuf::from(storage_directory)
                .join("protected_store"))
            .to_string_lossy()
            .to_string();
            out.table_store.directory = (std::path::PathBuf::from(storage_directory)
                .join("table_store"))
            .to_string_lossy()
            .to_string();
            out.block_store.directory = (std::path::PathBuf::from(storage_directory)
                .join("block_store"))
            .to_string_lossy()
            .to_string();
        } else {
            out.protected_store.directory =
                get_default_store_path(program_name, organization, qualifier, "protected_store");
            out.table_store.directory =
                get_default_store_path(program_name, organization, qualifier, "table_store");
            out.block_store.directory =
                get_default_store_path(program_name, organization, qualifier, "block_store");
        }

        if let Some(config_directory) = config_directory {
            out.network.tls.certificate_path = (std::path::PathBuf::from(config_directory)
                .join("ssl/certs/server.crt"))
            .to_string_lossy()
            .to_string();
            out.network.tls.private_key_path = (std::path::PathBuf::from(config_directory)
                .join("ssl/keys/server.key"))
            .to_string_lossy()
            .to_string();
        } else {
            out.network.tls.certificate_path = get_default_ssl_directory(
                program_name,
                organization,
                qualifier,
                "certs/server.crt",
            );
            out.network.tls.private_key_path =
                get_default_ssl_directory(program_name, organization, qualifier, "keys/server.key");
        }

        out
    }

    /// Clone the config with secrets stripped (routing-table secret keys and encryption-key passwords), safe to log or serialize.
    #[must_use]
    pub fn safe(&self) -> Arc<VeilidConfig> {
        let mut safe_cfg = self.clone();

        // Remove secrets
        safe_cfg.network.routing_table.secret_keys = SecretKeyGroup::new();
        "".clone_into(&mut safe_cfg.protected_store.device_encryption_key_password);
        safe_cfg.protected_store.new_device_encryption_key_password = None;

        Arc::new(safe_cfg)
    }

    /// The RPC reply timeout, the base most other durations derive from
    pub(crate) fn rpc_timeout(&self) -> TimestampDuration {
        TimestampDuration::new_ms(self.internal().network.rpc.timeout_ms.into())
    }

    /// Serialize the config, or the subtree at a dot-separated `key` path, to JSON. Empty `key` returns the whole config.
    pub fn get_key_json(&self, key: &str, pretty: bool) -> VeilidAPIResult<String> {
        // Generate json from whole config
        let jvc = serde_json::to_value(self).map_err(VeilidAPIError::generic)?;

        // Find requested subkey
        if key.is_empty() {
            Ok(if pretty {
                serde_json::to_string_pretty(&jvc).map_err(VeilidAPIError::generic)?
            } else {
                serde_json::to_string(&jvc).map_err(VeilidAPIError::generic)?
            })
        } else {
            // Split key into path parts
            let keypath: Vec<&str> = key.split('.').collect();
            let mut out = &jvc;
            for k in keypath {
                let Some(next_out) = out.get(k) else {
                    apibail_parse_error!(format!("invalid subkey in key '{}'", key), k);
                };
                out = next_out;
            }
            if pretty {
                serde_json::to_string_pretty(out).map_err(VeilidAPIError::generic)
            } else {
                serde_json::to_string(out).map_err(VeilidAPIError::generic)
            }
        }
    }

    // Rejects illegal/control chars, Windows-reserved names, trailing dot/space, len > 255
    fn is_valid_filename(s: &str) -> bool {
        if s.len() > 255 {
            return false;
        }
        if s.chars().any(|c| {
            matches!(c, '/' | '?' | '<' | '>' | '\\' | ':' | '*' | '|' | '"')
                || c <= '\u{1f}'
                || ('\u{80}'..='\u{9f}').contains(&c)
        }) {
            return false;
        }
        if !s.is_empty() && s.bytes().all(|b| b == b'.') {
            return false;
        }
        if s.ends_with('.') || s.ends_with(' ') {
            return false;
        }
        let base = s.split('.').next().unwrap_or_default().as_bytes();
        match base.len() {
            3 => {
                !(base.eq_ignore_ascii_case(b"con")
                    || base.eq_ignore_ascii_case(b"prn")
                    || base.eq_ignore_ascii_case(b"aux")
                    || base.eq_ignore_ascii_case(b"nul"))
            }
            4 => {
                !((base[..3].eq_ignore_ascii_case(b"com")
                    || base[..3].eq_ignore_ascii_case(b"lpt"))
                    && base[3].is_ascii_digit())
            }
            _ => true,
        }
    }

    fn validate_program_name(program_name: &str) -> VeilidAPIResult<()> {
        if program_name.is_empty() {
            apibail_generic!("Program name must not be empty in 'program_name'");
        }
        if !Self::is_valid_filename(program_name) {
            apibail_generic!("'program_name' must not be an invalid filename");
        }
        Ok(())
    }

    fn validate_namespace(namespace: &str) -> VeilidAPIResult<()> {
        if namespace.is_empty() {
            return Ok(());
        }
        if !Self::is_valid_filename(namespace) {
            apibail_generic!("'namespace' must not be an invalid filename");
        }

        Ok(())
    }

    fn validate_max_connections(max_connections: u32, key: &str) -> VeilidAPIResult<()> {
        if !(MAX_CONNECTIONS_MIN..=MAX_CONNECTIONS_MAX).contains(&max_connections) {
            apibail_generic!(format!(
                "max connections must be in the range {}-{} in config key '{}'",
                MAX_CONNECTIONS_MIN, MAX_CONNECTIONS_MAX, key
            ));
        }
        Ok(())
    }

    /// Check the config for invalid or out-of-range values.
    pub fn validate(&self) -> VeilidAPIResult<()> {
        Self::validate_program_name(&self.program_name)?;
        Self::validate_namespace(&self.namespace)?;

        // Total connection cap across all protocols
        Self::validate_max_connections(self.network.max_connections, "network.max_connections")?;

        // if inner.network.protocol.udp.enabled {
        //     // Validate UDP settings
        // }
        #[cfg(feature = "enable-protocol-wss")]
        if self.network.protocol.wss.listen {
            // Validate WSS settings
            if self
                .network
                .protocol
                .wss
                .url
                .as_ref()
                .map(|u| u.is_empty())
                .unwrap_or_default()
            {
                apibail_generic!(
                    "WSS URL must be specified in config key 'network.protocol.wss.url'"
                );
            }
        }
        if self.internal().network.rpc.max_route_hop_count == 0 {
            apibail_generic!(
                "max route hop count must be >= 1 in 'network.rpc.max_route_hop_count'"
            );
        }
        if self.internal().network.rpc.max_route_hop_count > 5 {
            apibail_generic!(
                "max route hop count must be <= 5 in 'network.rpc.max_route_hop_count'"
            );
        }
        if self.network.rpc.default_route_hop_count == 0 {
            apibail_generic!(
                "default route hop count must be >= 1 in 'network.rpc.default_route_hop_count'"
            );
        }
        if self.network.rpc.default_route_hop_count
            > self.internal().network.rpc.max_route_hop_count
        {
            apibail_generic!(
                "default route hop count must be <= max route hop count in 'network.rpc.default_route_hop_count <= network.rpc.max_route_hop_count'"
            );
        }
        for (ttl, key) in [
            (
                self.internal().network.default_hole_punch_ttl,
                "network.default_hole_punch_ttl",
            ),
            (
                self.internal().network.cgnat_hole_punch_ttl,
                "network.cgnat_hole_punch_ttl",
            ),
        ] {
            if !(1..=255).contains(&ttl) {
                apibail_generic!("hole punch ttl must be from 1 to 255 in '{}'", key);
            }
        }
        if self.internal().network.rpc.queue_size < 256 {
            apibail_generic!("rpc queue size must be >= 256 in 'network.rpc.queue_size'");
        }
        if self.internal().network.rpc.timeout_ms < 1000 {
            apibail_generic!("rpc timeout must be >= 1000 in 'network.rpc.timeout_ms'");
        }
        if self.internal().network.dht.consensus_width < self.internal().network.dht.set_value_count
        {
            apibail_generic!(
                "consensus width must be >= set value count in 'network.dht.consensus_width'"
            );
        }
        if self.internal().network.dht.get_value_count
            <= (self.internal().network.dht.set_value_count / 2)
        {
            apibail_generic!("get consensus count must be >= (set value count / 2) in 'network.dht.get_value_count'");
        }
        if self.internal().network.dht.get_value_fanout < 1 {
            apibail_generic!("get value fanout must be >= 1 in 'network.dht.get_value_fanout'");
        }
        if self.internal().network.dht.set_value_fanout < 1 {
            apibail_generic!("set value fanout must be >= 1 in 'network.dht.set_value_fanout'");
        }
        if self.internal().network.dht.get_value_timeout_ms
            < (2 * self.internal().network.rpc.timeout_ms)
        {
            apibail_generic!("get value timeout must be >= (2 * the rpc timeout) in 'network.dht.get_value_timeout_ms'");
        }
        if self.internal().network.dht.set_value_timeout_ms
            < (2 * self.internal().network.rpc.timeout_ms)
        {
            apibail_generic!("set value timeout must be >= (2 * the rpc timeout) in 'network.dht.set_value_timeout_ms'");
        }

        if self.internal().network.dht.public_watch_limit < 1 {
            apibail_generic!("public watch limit must be >= 1 in 'network.dht.public_watch_limit'");
        }
        if self.internal().network.dht.member_watch_limit < 1 {
            apibail_generic!("member watch limit must be >= 1 in 'network.dht.member_watch_limit'");
        }
        if self.internal().network.dht.max_watch_expiration_ms
            < (2 * self.internal().network.rpc.timeout_ms)
        {
            apibail_generic!("max watch expiration must be >= (2 * rpc timeout) 'network.dht.max_watch_expiration_ms'");
        }
        if self.internal().network.dht.public_transaction_limit < 1 {
            apibail_generic!(
                "public transaction limit must be >= 1 in 'network.dht.public_transaction_limit'"
            );
        }
        if self.internal().network.dht.member_transaction_limit < 1 {
            apibail_generic!(
                "member transaction limit must be >= 1 in 'network.dht.member_transaction_limit'"
            );
        }

        Ok(())
    }

    /// Take the secrets a config from `get_state` lacks from `current`: blank
    /// `network.routing_table.secret_keys` or `protected_store` passwords keep the current values.
    pub fn fill_secrets_from(&mut self, current: &VeilidConfig) {
        if self.network.routing_table.secret_keys.is_empty() {
            self.network.routing_table.secret_keys =
                current.network.routing_table.secret_keys.clone();
        }
        if self
            .protected_store
            .device_encryption_key_password
            .is_empty()
        {
            self.protected_store.device_encryption_key_password = current
                .protected_store
                .device_encryption_key_password
                .clone();
        }
        if self
            .protected_store
            .new_device_encryption_key_password
            .is_none()
        {
            self.protected_store.new_device_encryption_key_password = current
                .protected_store
                .new_device_encryption_key_password
                .clone();
        }
    }

    /// Check that this config can replace `current` on a node that stays up: the keys a
    /// running node cannot take a new value for must be unchanged. Those are the storage
    /// identity (`program_name`, `namespace`, `protected_store`, `table_store`,
    /// `block_store`), the node identity (`network.routing_table.public_keys` and
    /// `secret_keys`), the network partition (`network.network_key_password`), the
    /// `network.dht` storage sizing and `internal`.
    ///
    /// Errors with [VeilidAPIError::InvalidArgument] naming the first such key that differs.
    pub fn ensure_changeable_from(&self, current: &VeilidConfig) -> VeilidAPIResult<()> {
        let fixed = [
            (self.program_name == current.program_name, "program_name"),
            (self.namespace == current.namespace, "namespace"),
            (
                self.protected_store == current.protected_store,
                "protected_store",
            ),
            (self.table_store == current.table_store, "table_store"),
            (self.block_store == current.block_store, "block_store"),
            (
                self.network.network_key_password == current.network.network_key_password,
                "network.network_key_password",
            ),
            (
                self.network.routing_table.public_keys == current.network.routing_table.public_keys,
                "network.routing_table.public_keys",
            ),
            (
                self.network.routing_table.secret_keys == current.network.routing_table.secret_keys,
                "network.routing_table.secret_keys",
            ),
            (self.network.dht == current.network.dht, "network.dht"),
            (self.internal == current.internal, "internal"),
        ];
        if let Some((_, key)) = fixed.iter().find(|(same, _)| !same) {
            apibail_invalid_argument!("change_config", key, "needs a restart to change");
        }
        Ok(())
    }
}

/// The configuration built for each Veilid node during API startup
#[derive(Clone)]
#[must_use]
pub struct VeilidStartupOptions {
    /// Where the node delivers its updates to the app
    update_cb: UpdateCallback,
    /// Shared by every clone, so a change reaches every component's `config()`
    config: Arc<ArcSwap<VeilidConfig>>,
}

impl fmt::Debug for VeilidStartupOptions {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("VeilidConfig")
            .field("config", &*self.config())
            .finish()
    }
}

impl VeilidStartupOptions {
    pub(crate) fn try_new(
        config: VeilidConfig,
        update_cb: UpdateCallback,
    ) -> VeilidAPIResult<Self> {
        config.validate()?;

        Ok(Self {
            update_cb,
            config: Arc::new(ArcSwap::from_pointee(config)),
        })
    }

    /// The callback invoked to deliver `VeilidUpdate` events to the application.
    #[must_use]
    pub fn update_callback(&self) -> UpdateCallback {
        self.update_cb.clone()
    }

    /// The validated configuration for this node, as it stands now.
    #[must_use]
    pub fn config(&self) -> Arc<VeilidConfig> {
        self.config.load_full()
    }

    /// Replace the configuration; the caller has validated it
    pub(crate) fn set_config(&self, config: VeilidConfig) {
        self.config.store(Arc::new(config));
    }
}

/// Return the default veilid config as a json object.
#[must_use]
pub fn default_veilid_config() -> String {
    serialize_json(VeilidConfig::default())
}
