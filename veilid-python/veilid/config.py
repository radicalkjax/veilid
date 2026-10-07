"""Node startup configuration mirroring veilid-core's `VeilidConfig` tree."""

from dataclasses import dataclass, fields
from enum import StrEnum
from typing import Optional, Self

from .types import PublicKey, SecretKey, VeilidCapability


class VeilidConfigLogLevel(StrEnum):
    """Logging level threshold (`OFF` disables logging)."""

    OFF = "Off"
    ERROR = "Error"
    WARN = "Warn"
    INFO = "Info"
    DEBUG = "Debug"
    TRACE = "Trace"


class VeilidConfigAddressType(StrEnum):
    """An IP address family the node may use."""

    IPV4 = "IPV4"
    IPV6 = "IPV6"


@dataclass
class ConfigBase:
    """Base for config dataclasses providing recursive JSON load/dump."""

    @classmethod
    def from_json(cls, json_data: dict) -> Self:
        """Return an instance of this type from the input data."""
        args = {}
        for field in fields(cls):
            key = field.name
            if key not in json_data:
                # Absent in JSON; fall back to the dataclass default if it has one.
                continue
            value = json_data[key]
            try:
                # See if this field's type knows how to load itself from JSON input.
                loader = field.type.from_json # type: ignore
            except AttributeError:
                # No, it doesn't. Use the raw value.
                args[key] = value
            else:
                # Yes, it does. Use the loading function's output.
                args[key] = loader(value)

        return cls(**args)

    def to_json(self) -> dict:
        """Return this config as a JSON-serializable dict."""
        return self.__dict__


@dataclass
class VeilidConfigCapabilities(ConfigBase):
    """Capabilities advertised by this node. `disable` lists ones to mark unavailable."""

    disable: list[VeilidCapability]


@dataclass
class VeilidConfigProtectedStore(ConfigBase):
    """Protected store: where secrets such as the device encryption key are kept."""

    allow_insecure_fallback: bool
    always_use_insecure_storage: bool
    directory: str
    delete: bool
    device_encryption_key_password: str
    new_device_encryption_key_password: Optional[str]


@dataclass
class VeilidConfigTableStore(ConfigBase):
    """Table store: the encrypted key-value database backing node state."""

    directory: str
    delete: bool
    wipe_on_invalid_device_encryption_key: bool
    max_value_size_mb: int


@dataclass
class VeilidConfigBlockStore(ConfigBase):
    """Block store: content-addressed block storage."""

    directory: str
    delete: bool


@dataclass
class VeilidConfigRoutingTable(ConfigBase):
    """Routing table identity and bootstrap configuration."""

    public_keys: list[PublicKey]
    secret_keys: list[SecretKey]
    bootstrap: list[str]
    bootstrap_keys: list[PublicKey]


@dataclass
class VeilidConfigRPC(ConfigBase):
    """RPC configuration."""

    default_route_hop_count: int


@dataclass
class VeilidConfigDHT(ConfigBase):
    """DHT cache and storage configuration.

    Defaults should be used unless you know what you're doing; changing the
    count/fanout/timeout parameters may render the node inoperable for DHT operations.
    """

    local_subkey_cache_size: int
    local_max_subkey_cache_memory_mb: int
    remote_subkey_cache_size: int
    remote_max_records: int
    remote_max_subkey_cache_memory_mb: int
    remote_max_storage_space_mb: int
    max_concurrent_operations: int


@dataclass
class VeilidConfigTLS(ConfigBase):
    """TLS configuration for inbound secure protocols."""

    certificate_path: str
    private_key_path: str
    connection_initial_timeout_ms: int


@dataclass
class VeilidConfigUDP(ConfigBase):
    """Enable and configure UDP."""

    enabled: bool
    listen_address: str
    public_address: Optional[str]


@dataclass
class VeilidConfigTCP(ConfigBase):
    """Enable and configure TCP."""

    connect: bool
    listen: bool
    listen_address: str
    public_address: Optional[str]


@dataclass
class VeilidConfigWS(ConfigBase):
    """Enable and configure Web Sockets."""

    connect: bool
    listen: bool
    listen_address: str
    path: str
    url: Optional[str]


# @dataclass
# class VeilidConfigWSS(ConfigBase):
#     connect: bool
#     listen: bool
#     listen_address: str
#     path: str
#     url: Optional[str]


@dataclass
class VeilidConfigProtocol(ConfigBase):
    """Per-protocol (UDP/TCP/WS) transport configuration."""

    udp: VeilidConfigUDP
    tcp: VeilidConfigTCP
    ws: VeilidConfigWS
#    wss: VeilidConfigWSS


@dataclass
class VeilidConfigPrivacy(ConfigBase):
    """Privacy and relay preferences for routes."""

    require_inbound_relay: bool


@dataclass
class VeilidConfigNetwork(ConfigBase):
    """Network subsystem: connections, routing table, RPC, DHT, transports, and privacy."""

    max_connections: int
    network_key_password: Optional[str]
    routing_table: VeilidConfigRoutingTable
    rpc: VeilidConfigRPC
    dht: VeilidConfigDHT
    address_types: list[VeilidConfigAddressType]
    upnp: bool
    detect_address_changes: Optional[bool]
    tls: VeilidConfigTLS
    protocol: VeilidConfigProtocol
    privacy: VeilidConfigPrivacy


# "Footgun" internal tuning tree, parallel to the main config. Only honored when
# veilid-core is built with the `footgun-config` feature; otherwise ignored.
@dataclass
class VeilidConfigInternalUDP(ConfigBase):
    """Internal "footgun" UDP tuning."""

    socket_pool_size: int


@dataclass
class VeilidConfigInternalProtocol(ConfigBase):
    """Internal "footgun" per-protocol tuning."""

    udp: VeilidConfigInternalUDP


@dataclass
class VeilidConfigInternalRPC(ConfigBase):
    """Internal "footgun" RPC tuning."""

    concurrency: int
    queue_size: int
    max_timestamp_behind_ms: Optional[int]
    max_timestamp_ahead_ms: Optional[int]
    timeout_ms: int
    max_route_hop_count: int


@dataclass
class VeilidConfigInternalDHT(ConfigBase):
    """Internal "footgun" DHT tuning.

    Changing the count/fanout/timeout parameters may render the node inoperable for
    DHT operations.
    """

    max_find_node_count: int
    resolve_node_timeout_ms: int
    resolve_node_count: int
    resolve_node_fanout: int
    get_value_timeout_ms: int
    get_value_count: int
    get_value_fanout: int
    set_value_timeout_ms: int
    set_value_count: int
    set_value_fanout: int
    consensus_width: int
    min_peer_count: int
    min_peer_refresh_time_ms: int
    validate_dial_info_receipt_time_ms: int
    max_watch_expiration_ms: int
    public_watch_limit: int
    member_watch_limit: int
    public_transaction_limit: int
    member_transaction_limit: int


@dataclass
class VeilidConfigInternalNetwork(ConfigBase):
    """Internal "footgun" network tuning."""

    connection_initial_timeout_ms: int
    connection_inactivity_timeout_ms: int
    max_connections_per_ip4: int
    max_connections_per_ip6_prefix: int
    max_connections_per_ip6_prefix_size: int
    max_connection_frequency_per_min: int
    client_allowlist_timeout_ms: int
    reverse_connection_receipt_time_ms: int
    hole_punch_receipt_time_ms: int
    restricted_nat_retries: int
    rpc: VeilidConfigInternalRPC
    dht: VeilidConfigInternalDHT
    protocol: VeilidConfigInternalProtocol


@dataclass
class VeilidConfigInternal(ConfigBase):
    """Internal "footgun" tuning tree. Only honored when veilid-core is built with the
    `footgun-config` feature; otherwise ignored."""

    network: VeilidConfigInternalNetwork


@dataclass
class VeilidConfig(ConfigBase):
    """Top level of the Veilid configuration tree."""

    program_name: str
    namespace: str
    capabilities: VeilidConfigCapabilities
    protected_store: VeilidConfigProtectedStore
    table_store: VeilidConfigTableStore
    block_store: VeilidConfigBlockStore
    network: VeilidConfigNetwork
    internal: Optional[VeilidConfigInternal] = None
