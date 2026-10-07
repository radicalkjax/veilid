"""Veilid node state and update event types mirroring veilid-core's veilid_api types."""

from enum import StrEnum
from typing import Optional, Self

from .config import VeilidConfig
from .types import (
    ByteCount,
    BareRouteId,
    Timestamp,
    TimestampDuration,
    NodeId,
    RecordKey,
    ValueData,
    ValueSubkey,
    VeilidLogLevel,
    OperationId,
    urlsafe_b64decode_no_pad,
)


class AttachmentState(StrEnum):
    """Attachment abstraction for network 'signal strength'."""

    DETACHED = "Detached"
    ATTACHING = "Attaching"
    ATTACHED_WEAK = "AttachedWeak"
    ATTACHED_FAIR = "AttachedFair"
    ATTACHED_GOOD = "AttachedGood"
    ATTACHED_STRONG = "AttachedStrong"
    ATTACHED_FULL = "AttachedFull"
    DETACHING = "Detaching"

    def bar_count(self) -> int:
        """Signal-strength bars (0..=5)."""
        return {
            AttachmentState.DETACHED: 0,
            AttachmentState.DETACHING: 0,
            AttachmentState.ATTACHING: 0,
            AttachmentState.ATTACHED_WEAK: 1,
            AttachmentState.ATTACHED_FAIR: 2,
            AttachmentState.ATTACHED_GOOD: 3,
            AttachmentState.ATTACHED_STRONG: 4,
            AttachmentState.ATTACHED_FULL: 5,
        }[self]


class VeilidStateAttachment:
    """Attachment state of the Veilid node."""

    state: AttachmentState
    public_internet_ready: bool
    local_network_ready: bool
    uptime: TimestampDuration
    attached_uptime: Optional[TimestampDuration]
    reliable_peer_count: int
    live_peer_count: int
    estimated_network_size: int
    median_latency: Optional[TimestampDuration]
    over_attached_nodes: int

    def __init__(
        self,
        state: AttachmentState,
        public_internet_ready: bool,
        local_network_ready: bool,
        uptime: TimestampDuration,
        attached_uptime: Optional[TimestampDuration],
        reliable_peer_count: int,
        live_peer_count: int,
        estimated_network_size: int,
        median_latency: Optional[TimestampDuration],
        over_attached_nodes: int,
    ):
        self.state = state
        self.public_internet_ready = public_internet_ready
        self.local_network_ready = local_network_ready
        self.uptime = uptime
        self.attached_uptime = attached_uptime
        self.reliable_peer_count = reliable_peer_count
        self.live_peer_count = live_peer_count
        self.estimated_network_size = estimated_network_size
        self.median_latency = median_latency
        self.over_attached_nodes = over_attached_nodes

    @classmethod
    def from_json(cls, j: dict) -> Self:
        """JSON object hook"""
        return cls(
            AttachmentState(j["state"]),
            j["public_internet_ready"],
            j["local_network_ready"],
            j["uptime"],
            j["attached_uptime"],
            j["reliable_peer_count"],
            j["live_peer_count"],
            j["estimated_network_size"],
            j["median_latency"],
            j["over_attached_nodes"],
        )

    def to_json(self) -> dict:
        """Serialize to a JSON-compatible dict."""
        return self.__dict__

class LatencyStats:
    """Communications latency to a node over all RPC questions."""

    fastest: TimestampDuration
    average: TimestampDuration
    slowest: TimestampDuration
    tm90: TimestampDuration
    tm75: TimestampDuration
    p90: TimestampDuration
    p75: TimestampDuration

    def __init__(
        self,
        fastest: TimestampDuration,
        average: TimestampDuration,
        slowest: TimestampDuration,
        tm90: TimestampDuration,
        tm75: TimestampDuration,
        p90: TimestampDuration,
        p75: TimestampDuration,
    ):
        self.fastest = fastest
        self.average = average
        self.slowest = slowest
        self.tm90 = tm90
        self.tm75 = tm75
        self.p90 = p90
        self.p75 = p75

    @classmethod
    def from_json(cls, j: dict) -> Self:
        """JSON object hook"""
        return cls(
            TimestampDuration(j["fastest"]),
            TimestampDuration(j["average"]),
            TimestampDuration(j["slowest"]),
            TimestampDuration(j["tm90"]),
            TimestampDuration(j["tm75"]),
            TimestampDuration(j["p90"]),
            TimestampDuration(j["p75"]),
        )

    def to_json(self) -> dict:
        """Serialize to a JSON-compatible dict."""
        return self.__dict__


class TransferStats:
    """Amount of data transferred to or from a node over a time span."""

    total: ByteCount
    maximum: ByteCount
    average: ByteCount
    minimum: ByteCount

    def __init__(
        self,
        total: ByteCount,
        maximum: ByteCount,
        average: ByteCount,
        minimum: ByteCount,
    ):
        self.total = total
        self.maximum = maximum
        self.average = average
        self.minimum = minimum

    @classmethod
    def from_json(cls, j: dict) -> Self:
        """JSON object hook"""
        return cls(
            ByteCount(j["total"]),
            ByteCount(j["maximum"]),
            ByteCount(j["average"]),
            ByteCount(j["minimum"]),
        )


class TransferStatsDownUp:
    """Transfer stats in both directions: node to us (down) and us to node (up)."""

    down: TransferStats
    up: TransferStats

    def __init__(self, down: TransferStats, up: TransferStats):
        self.down = down
        self.up = up

    @classmethod
    def from_json(cls, j: dict) -> Self:
        """JSON object hook"""
        return cls(TransferStats.from_json(j["down"]), TransferStats.from_json(j["up"]))


class PeerStats:
    """Statistics for a peer in the routing table."""

    latency: Optional[LatencyStats]
    transfer: TransferStatsDownUp

    def __init__(
        self,
        latency: Optional[LatencyStats],
        transfer: TransferStatsDownUp,
    ):
        self.latency = latency
        self.transfer = transfer

    @classmethod
    def from_json(cls, j: dict) -> Self:
        """JSON object hook"""
        return cls(
            None if j["latency"] is None else LatencyStats.from_json(j["latency"]),
            TransferStatsDownUp.from_json(j["transfer"]),
        )

    def to_json(self) -> dict:
        """Serialize to a JSON-compatible dict."""
        return self.__dict__


class PeerTableData:
    """A recently accessed peer."""

    node_ids: list[NodeId]
    peer_address: str
    peer_stats: PeerStats

    def __init__(self, node_ids: list[NodeId], peer_address: str, peer_stats: PeerStats):
        self.node_ids = node_ids
        self.peer_address = peer_address
        self.peer_stats = peer_stats

    @classmethod
    def from_json(cls, j: dict) -> Self:
        """JSON object hook"""
        return cls([NodeId(node_id) for node_id in j["node_ids"]],
                   j["peer_address"],
                   PeerStats.from_json(j["peer_stats"]))

    def to_json(self) -> dict:
        """Serialize to a JSON-compatible dict."""
        return self.__dict__


class VeilidStateNetwork:
    """Current network state of the Veilid node."""

    started: bool
    bps_down: ByteCount
    bps_up: ByteCount
    peers: list[PeerTableData]
    node_ids: list[NodeId]

    def __init__(
        self,
        started: bool,
        bps_down: ByteCount,
        bps_up: ByteCount,
        peers: list[PeerTableData],
        node_ids: list[NodeId],
    ):
        self.started = started
        self.bps_down = bps_down
        self.bps_up = bps_up
        self.peers = peers
        self.node_ids = node_ids

    @classmethod
    def from_json(cls, j: dict) -> Self:
        """JSON object hook"""
        return cls(
            j["started"],
            ByteCount(j["bps_down"]),
            ByteCount(j["bps_up"]),
            [PeerTableData.from_json(peer) for peer in j["peers"]],
            [NodeId(node_id) for node_id in j["node_ids"]],
        )

    def to_json(self) -> dict:
        """Serialize to a JSON-compatible dict."""
        return self.__dict__


class VeilidStateConfig:
    """Current Veilid node configuration."""

    config: VeilidConfig

    def __init__(self, config: VeilidConfig):
        self.config = config

    @classmethod
    def from_json(cls, j: dict) -> Self:
        """JSON object hook"""
        return cls(VeilidConfig.from_json(j["config"]))

    def to_json(self) -> dict:
        """Serialize to a JSON-compatible dict."""
        return self.__dict__


class VeilidState:
    """Queriable snapshot of the Veilid node internals."""

    attachment: VeilidStateAttachment
    network: VeilidStateNetwork
    config: VeilidStateConfig

    def __init__(
        self,
        attachment: VeilidStateAttachment,
        network: VeilidStateNetwork,
        config: VeilidStateConfig,
    ):
        self.attachment = attachment
        self.network = network
        self.config = config

    @classmethod
    def from_json(cls, j: dict) -> Self:
        """JSON object hook"""
        return cls(
            VeilidStateAttachment.from_json(j["attachment"]),
            VeilidStateNetwork.from_json(j["network"]),
            VeilidStateConfig.from_json(j["config"]),
        )

    def to_json(self) -> dict:
        """Serialize to a JSON-compatible dict."""
        return self.__dict__


class VeilidLog:
    """A VeilidCore log message with optional backtrace."""

    log_level: VeilidLogLevel
    message: str
    backtrace: Optional[str]

    def __init__(self, log_level: VeilidLogLevel, message: str, backtrace: Optional[str]):
        self.log_level = log_level
        self.message = message
        self.backtrace = backtrace

    @classmethod
    def from_json(cls, j: dict) -> Self:
        """JSON object hook"""
        return cls(VeilidLogLevel(j["log_level"]), j["message"], j["backtrace"])

    def to_json(self) -> dict:
        """Serialize to a JSON-compatible dict."""
        return self.__dict__


class VeilidAppMessage:
    """A one-way application message blob delivered to the host application."""

    sender: Optional[NodeId]
    route_id: Optional[BareRouteId]
    message: bytes

    def __init__(self, sender: Optional[NodeId], route_id: Optional[BareRouteId], message: bytes):
        self.sender = sender
        self.route_id = route_id
        self.message = message

    @classmethod
    def from_json(cls, j: dict) -> Self:
        """JSON object hook"""
        return cls(
            None if j["sender"] is None else NodeId(j["sender"]),
            None if j["route_id"] is None else BareRouteId(j["route_id"]),
            urlsafe_b64decode_no_pad(j["message"]),
        )

    def to_json(self) -> dict:
        """Serialize to a JSON-compatible dict."""
        return self.__dict__


class VeilidAppCall:
    """An application question blob delivered to the host application, expecting an AppReply."""

    sender: Optional[NodeId]
    route_id: Optional[BareRouteId]
    message: bytes
    call_id: OperationId

    def __init__(self, sender: Optional[NodeId], route_id: Optional[BareRouteId], message: bytes, call_id: OperationId):
        self.sender = sender
        self.route_id = route_id
        self.message = message
        self.call_id = call_id

    @classmethod
    def from_json(cls, j: dict) -> Self:
        """JSON object hook"""
        return cls(
            None if j["sender"] is None else NodeId(j["sender"]),
            None if j["route_id"] is None else BareRouteId(j["route_id"]),
            urlsafe_b64decode_no_pad(j["message"]),
            OperationId(j["call_id"]),
        )

    def to_json(self) -> dict:
        """Serialize to a JSON-compatible dict."""
        return self.__dict__


class VeilidRouteChange:
    """A private or safety route change that has happened."""

    dead_routes: list[BareRouteId]
    dead_remote_routes: list[BareRouteId]

    def __init__(self, dead_routes: list[BareRouteId], dead_remote_routes: list[BareRouteId]):
        self.dead_routes = dead_routes
        self.dead_remote_routes = dead_remote_routes

    @classmethod
    def from_json(cls, j: dict) -> Self:
        """JSON object hook"""
        return cls(
            [BareRouteId(route) for route in j["dead_routes"]],
            [BareRouteId(route) for route in j["dead_remote_routes"]],
        )

    def to_json(self) -> dict:
        """Serialize to a JSON-compatible dict."""
        return self.__dict__


class VeilidValueChange:
    """A change to the subkey values of a watched DHT record."""

    key: RecordKey
    subkeys: list[tuple[ValueSubkey, ValueSubkey]]
    count: int
    value: Optional[ValueData]

    def __init__(self, key: RecordKey, subkeys: list[tuple[ValueSubkey, ValueSubkey]], count: int, value: Optional[ValueData]):
        self.key = key
        self.subkeys = subkeys
        self.count = count
        self.value = value

    @classmethod
    def from_json(cls, j: dict) -> Self:
        """JSON object hook"""
        return cls(
            RecordKey(j["key"]),
            [(p[0], p[1]) for p in j["subkeys"]],
            j["count"],
            None if j["value"] is None else ValueData.from_json(j["value"]),
        )

    def to_json(self) -> dict:
        """Serialize to a JSON-compatible dict."""
        return self.__dict__


class VeilidUpdateKind(StrEnum):
    """Discriminant for the detail carried by a VeilidUpdate."""

    LOG = "Log"
    APP_MESSAGE = "AppMessage"
    APP_CALL = "AppCall"
    ATTACHMENT = "Attachment"
    NETWORK = "Network"
    CONFIG = "Config"
    ROUTE_CHANGE = "RouteChange"
    VALUE_CHANGE = "ValueChange"
    SHUTDOWN = "Shutdown"


VeilidUpdateDetailType = Optional[
    VeilidLog
    | VeilidAppMessage
    | VeilidAppCall
    | VeilidStateAttachment
    | VeilidStateNetwork
    | VeilidStateConfig
    | VeilidRouteChange
    | VeilidValueChange
]


class VeilidUpdate:
    """An update from veilid-core describing a change to the node's internal state."""

    kind: VeilidUpdateKind
    detail: VeilidUpdateDetailType

    def __init__(
        self,
        kind: VeilidUpdateKind,
        detail: VeilidUpdateDetailType,
    ):
        self.kind = kind
        self.detail = detail

    @classmethod
    def from_json(cls, j: dict) -> Self:
        """JSON object hook"""
        kind = VeilidUpdateKind(j["kind"])
        detail: VeilidUpdateDetailType = None
        match kind:
            case VeilidUpdateKind.LOG:
                detail = VeilidLog.from_json(j)
            case VeilidUpdateKind.APP_MESSAGE:
                detail = VeilidAppMessage.from_json(j)
            case VeilidUpdateKind.APP_CALL:
                detail = VeilidAppCall.from_json(j)
            case VeilidUpdateKind.ATTACHMENT:
                detail = VeilidStateAttachment.from_json(j)
            case VeilidUpdateKind.NETWORK:
                detail = VeilidStateNetwork.from_json(j)
            case VeilidUpdateKind.CONFIG:
                detail = VeilidStateConfig.from_json(j)
            case VeilidUpdateKind.ROUTE_CHANGE:
                detail = VeilidRouteChange.from_json(j)
            case VeilidUpdateKind.VALUE_CHANGE:
                detail = VeilidValueChange.from_json(j)
            case VeilidUpdateKind.SHUTDOWN:
                detail = None
            case _:
                raise ValueError("Unknown VeilidUpdateKind")
        return cls(kind, detail)

    def to_json(self) -> dict:
        """Serialize to a JSON-compatible dict."""
        return self.__dict__
