"""Core Veilid types: crypto keys, nonces, DHT schema and record types, timestamps, safety selection, and targets.

Mirrors the types exported by veilid-core's veilid_api. String-backed types serialize as base64url-nopad; `CryptoTyped` values as `kind:value`.
"""

import base64
import json
from abc import ABC, abstractmethod
from enum import StrEnum
from functools import total_ordering
from typing import Any, Optional, Self, final

####################################################################


def urlsafe_b64encode_no_pad(b: bytes) -> str:
    """
    Removes any `=` used as padding from the encoded string.
    """
    return base64.urlsafe_b64encode(b).decode().rstrip("=")


def urlsafe_b64decode_no_pad(s: str) -> bytes:
    """
    Adds back in the required padding before decoding.
    """
    padding = 4 - (len(s) % 4)
    s = s + ("=" * padding)
    return base64.urlsafe_b64decode(s)


class VeilidJSONEncoder(json.JSONEncoder):
    """JSON encoder that emits bytes as base64url-nopad and delegates to a value's `to_json` method."""

    def default(self, o):
        """Encode bytes as base64url-nopad, otherwise call the value's `to_json` method."""
        if isinstance(o, bytes):
            return urlsafe_b64encode_no_pad(o)
        if hasattr(o, "to_json") and callable(o.to_json):
            return o.to_json()
        return json.JSONEncoder.default(self, o)

    @staticmethod
    def dumps(req: Any, *args, **kwargs) -> str:
        """Serialize a value to a JSON string using this encoder."""
        return json.dumps(req, cls=VeilidJSONEncoder, *args, **kwargs)


####################################################################


class VeilidLogLevel(StrEnum):
    """Log level for VeilidCore."""

    ERROR = "Error"
    WARN = "Warn"
    INFO = "Info"
    DEBUG = "Debug"
    TRACE = "Trace"


class CryptoKind(StrEnum):
    """Four-character code identifying the cryptosystem a typed value belongs to."""

    CRYPTO_KIND_NONE = "NONE"
    CRYPTO_KIND_VLD0 = "VLD0"
    CRYPTO_KIND_VLD1 = "VLD1"


class VeilidCapability(StrEnum):
    """Capability a node may advertise in a routing domain."""

    VEILID_CAPABILITY_ROUTE = "ROUT"
    VEILID_CAPABILITY_TUNNEL = "TUNL"
    VEILID_CAPABILITY_SIGNAL = "SGNL"
    VEILID_CAPABILITY_RELAY = "RLAY"
    VEILID_CAPABILITY_VALIDATE_DIAL_INFO = "DIAL"
    VEILID_CAPABILITY_DHT = "DHTV"
    VEILID_CAPABILITY_APPMESSAGE = "APPM"
    VEILID_CAPABILITY_BLOCKSTORE = "BLOC"


class Stability(StrEnum):
    """Choice of nodes to include in allocated routes: low latency or reliable uptime."""

    LOW_LATENCY = "LowLatency"
    RELIABLE = "Reliable"


class Sequencing(StrEnum):
    """Preferred ordering of RPC message delivery over a route or to a target.

    No sequencing preference guarantees delivery, as the network does not queue or buffer.
    """

    PREFER_UNORDERED = "PreferUnordered"
    PREFER_ORDERED = "PreferOrdered"
    ENSURE_ORDERED = "EnsureOrdered"


class DHTSchemaKind(StrEnum):
    """Kind tag of a DHT schema: DFLT (owner-writable subkeys) or SMPL (owner plus per-member subkeys)."""

    DFLT = "DFLT"
    SMPL = "SMPL"


class SafetySelectionKind(StrEnum):
    """Whether a routing context routes through a safety route (Safe) or not (Unsafe)."""

    UNSAFE = "Unsafe"
    SAFE = "Safe"

class TargetKind(StrEnum):
    """Whether a message target is a node id or a remote private route id."""

    ROUTE_ID= "RouteId"
    NODE_ID = "NodeId"

class DHTReportScope(StrEnum):
    """Which sequence numbers a DHT record report covers and the fanout used to gather network ones."""

    LOCAL = "Local"
    SYNC_GET = "SyncGet"
    SYNC_SET = "SyncSet"
    UPDATE_GET = "UpdateGet"
    UPDATE_SET = "UpdateSet"


####################################################################


class Timestamp(int):
    """Microseconds-since-epoch timestamp."""

    pass


class TimestampDuration(int):
    """A span of time measured in microseconds."""

    pass


class ByteCount(int):
    """A count of bytes."""

    pass


class OperationId(str):
    """Identifier for an in-flight RPC operation."""

    pass

class EncodedString(str):
    """A byte string carried as its base64url-nopad encoding."""

    def to_bytes(self) -> bytes:
        """Decode to the underlying raw bytes."""
        return urlsafe_b64decode_no_pad(self)

    @classmethod
    def from_bytes(cls, b: bytes) -> Self:
        """Build from raw bytes by base64url-nopad encoding them."""
        assert isinstance(b, bytes)
        return cls(urlsafe_b64encode_no_pad(b))

class BarePublicKey(EncodedString):
    """Untyped signing public key, carrying no cryptosystem kind."""

    pass

class BareSecretKey(EncodedString):
    """Untyped signing secret key, carrying no cryptosystem kind."""

    pass

class BareEncapsulationKey(EncodedString):
    """Untyped KEM encapsulation key, carrying no cryptosystem kind."""

    pass

class BareDecapsulationKey(EncodedString):
    """Untyped KEM decapsulation key, carrying no cryptosystem kind."""

    pass

class BareSharedSecret(EncodedString):
    """Untyped shared secret, carrying no cryptosystem kind."""

    pass

class BareHashDigest(EncodedString):
    """Untyped hash digest, carrying no cryptosystem kind."""

    pass

class BareSignature(EncodedString):
    """Untyped signature, carrying no cryptosystem kind."""

    pass

class Nonce(EncodedString):
    """A random 24-byte nonce. Has no kinded variant."""

    pass

class BareRouteId(EncodedString):
    """Untyped route id, carrying no cryptosystem kind."""

    pass

class BareNodeId(EncodedString):
    """Untyped node id, carrying no cryptosystem kind."""

    pass

class BareMemberId(EncodedString):
    """Untyped schema member id, carrying no cryptosystem kind."""

    pass

class BareOpaqueRecordKey(EncodedString):
    """Untyped opaque DHT record key, carrying no cryptosystem kind."""

    pass

class BareRecordKey(str):
    """Untyped DHT record key: an opaque record key with an optional record encryption secret.

    Encodes as `<key>` or `<key>:<encryption_key>`.
    """

    @classmethod
    def from_parts(cls, key: BareOpaqueRecordKey, encryption_key: Optional[BareSharedSecret]) -> Self:
        """Build a record key from an opaque record key and an optional encryption secret."""
        assert isinstance(key, BareOpaqueRecordKey)
        if encryption_key is not None:
            assert isinstance(encryption_key, BareSharedSecret)
            return cls(f"{key}:{encryption_key}")
        return cls(f"{key}")

    def key(self) -> BareOpaqueRecordKey:
        """The opaque record key."""
        parts = self.split(":", 1)
        return BareOpaqueRecordKey(parts[0])

    def encryption_key(self) -> Optional[BareSharedSecret]:
        """The encryption secret, if present."""
        parts = self.split(":", 1)
        if len(parts) == 2:
            return BareSharedSecret(self.split(":", 1)[1])
        return None

class BareKeyPair(str):
    """Untyped public/secret key pair, carrying no cryptosystem kind. Encodes as `<public>:<secret>`."""

    @classmethod
    def from_parts(cls, key: BarePublicKey, secret: BareSecretKey) -> Self:
        """Build a key pair from a public key and its secret key."""
        assert isinstance(key, BarePublicKey)
        assert isinstance(secret, BareSecretKey)
        return cls(f"{key}:{secret}")

    def key(self) -> BarePublicKey:
        """The public key."""
        return BarePublicKey(self.split(":", 1)[0])

    def secret(self) -> BareSecretKey:
        """The secret key."""
        return BareSecretKey(self.split(":", 1)[1])

class BareKemKeyPair(str):
    """Untyped KEM key pair, carrying no cryptosystem kind. Encodes as `<encapsulation>:<decapsulation>`."""

    @classmethod
    def from_parts(cls, key: BareEncapsulationKey, secret: BareDecapsulationKey) -> Self:
        """Build a KEM key pair from an encapsulation key and its decapsulation key."""
        assert isinstance(key, BareEncapsulationKey)
        assert isinstance(secret, BareDecapsulationKey)
        return cls(f"{key}:{secret}")

    def key(self) -> BareEncapsulationKey:
        """The encapsulation key."""
        return BareEncapsulationKey(self.split(":", 1)[0])

    def secret(self) -> BareDecapsulationKey:
        """The decapsulation key."""
        return BareDecapsulationKey(self.split(":", 1)[1])

class CryptoTyped(str):
    """A value tagged with the `CryptoKind` of the cryptosystem it belongs to. Encodes as `kind:value`."""

    def kind(self) -> CryptoKind:
        """The cryptosystem this value belongs to."""
        if self[4] != ":":
            raise ValueError("Not CryptoTyped")
        return CryptoKind(self[0:4])

    def _value(self) -> str:
        if self[4] != ":":
            raise ValueError("Not CryptoTyped")
        return self[5:]

class SharedSecret(CryptoTyped):
    """Kinded shared secret."""

    @classmethod
    def from_value(cls, kind: CryptoKind, value: BareSharedSecret) -> Self:
        """Pair a bare shared secret with the cryptosystem kind it belongs to."""
        assert isinstance(kind, CryptoKind)
        assert isinstance(value, BareSharedSecret)
        return cls(f"{kind}:{value}")

    def value(self) -> BareSharedSecret:
        """The untagged shared secret."""
        return BareSharedSecret(self._value())

class RecordKey(CryptoTyped):
    """Kinded DHT record key, with an optional record encryption secret."""

    @classmethod
    def from_value(cls, kind: CryptoKind, value: BareRecordKey) -> Self:
        """Pair a bare record key with the cryptosystem kind it belongs to."""
        assert isinstance(kind, CryptoKind)
        assert isinstance(value, BareRecordKey)
        return cls(f"{kind}:{value}")

    def value(self) -> BareRecordKey:
        """The untagged record key."""
        return BareRecordKey(self._value())

    def encryption_key(self) -> Optional[SharedSecret]:
        """The kinded encryption secret, if present."""
        ek = self.value().encryption_key()
        return None if ek == None else SharedSecret.from_value(self.kind(), ek)

class HashDigest(CryptoTyped):
    """Kinded hash digest."""

    @classmethod
    def from_value(cls, kind: CryptoKind, value: BareHashDigest) -> Self:
        """Pair a bare hash digest with the cryptosystem kind it belongs to."""
        assert isinstance(kind, CryptoKind)
        assert isinstance(value, BareHashDigest)
        return cls(f"{kind}:{value}")

    def value(self) -> BareHashDigest:
        """The untagged hash digest."""
        return BareHashDigest(self._value())

class PublicKey(CryptoTyped):
    """Kinded signing public key: verification, identity, and node ids. KEM encryption to a key uses EncapsulationKey."""

    @classmethod
    def from_value(cls, kind: CryptoKind, value: BarePublicKey) -> Self:
        """Pair a bare public key with the cryptosystem kind it belongs to."""
        assert isinstance(kind, CryptoKind)
        assert isinstance(value, BarePublicKey)
        return cls(f"{kind}:{value}")

    def value(self) -> BarePublicKey:
        """The untagged public key."""
        return BarePublicKey(self._value())


class SecretKey(CryptoTyped):
    """Kinded signing secret key. KEM decryption uses DecapsulationKey."""

    @classmethod
    def from_value(cls, kind: CryptoKind, value: BareSecretKey) -> Self:
        """Pair a bare secret key with the cryptosystem kind it belongs to."""
        assert isinstance(kind, CryptoKind)
        assert isinstance(value, BareSecretKey)
        return cls(f"{kind}:{value}")

    def value(self) -> BareSecretKey:
        """The untagged secret key."""
        return BareSecretKey(self._value())


class EncapsulationKey(CryptoTyped):
    """Kinded KEM encapsulation key, sealed to with hpke_seal. Signing uses PublicKey."""

    @classmethod
    def from_value(cls, kind: CryptoKind, value: BareEncapsulationKey) -> Self:
        """Pair a bare encapsulation key with the cryptosystem kind it belongs to."""
        assert isinstance(kind, CryptoKind)
        assert isinstance(value, BareEncapsulationKey)
        return cls(f"{kind}:{value}")

    def value(self) -> BareEncapsulationKey:
        """The untagged encapsulation key."""
        return BareEncapsulationKey(self._value())


class DecapsulationKey(CryptoTyped):
    """Kinded KEM decapsulation key, opening blobs with hpke_open. Signing uses SecretKey."""

    @classmethod
    def from_value(cls, kind: CryptoKind, value: BareDecapsulationKey) -> Self:
        """Pair a bare decapsulation key with the cryptosystem kind it belongs to."""
        assert isinstance(kind, CryptoKind)
        assert isinstance(value, BareDecapsulationKey)
        return cls(f"{kind}:{value}")

    def value(self) -> BareDecapsulationKey:
        """The untagged decapsulation key."""
        return BareDecapsulationKey(self._value())


class KeyPair(CryptoTyped):
    """Kinded public/secret key pair."""

    @classmethod
    def from_value(cls, kind: CryptoKind, value: BareKeyPair) -> Self:
        """Pair a bare key pair with the cryptosystem kind it belongs to."""
        assert isinstance(kind, CryptoKind)
        assert isinstance(value, BareKeyPair)
        return cls(f"{kind}:{value}")

    def value(self) -> BareKeyPair:
        """The untagged key pair."""
        return BareKeyPair(self._value())

    def key(self) -> PublicKey:
        """The kinded public key."""
        return PublicKey.from_value(kind=self.kind(), value=self.value().key())

    def secret(self) -> SecretKey:
        """The kinded secret key."""
        return SecretKey.from_value(kind=self.kind(), value=self.value().secret())

class KemKeyPair(CryptoTyped):
    """Kinded KEM key pair for HPKE seal/open. Signing key pairs use KeyPair."""

    @classmethod
    def from_value(cls, kind: CryptoKind, value: BareKemKeyPair) -> Self:
        """Pair a bare KEM key pair with the cryptosystem kind it belongs to."""
        assert isinstance(kind, CryptoKind)
        assert isinstance(value, BareKemKeyPair)
        return cls(f"{kind}:{value}")

    def value(self) -> BareKemKeyPair:
        """The untagged KEM key pair."""
        return BareKemKeyPair(self._value())

    def key(self) -> EncapsulationKey:
        """The kinded encapsulation key."""
        return EncapsulationKey.from_value(kind=self.kind(), value=self.value().key())

    def secret(self) -> DecapsulationKey:
        """The kinded decapsulation key."""
        return DecapsulationKey.from_value(kind=self.kind(), value=self.value().secret())

class Signature(CryptoTyped):
    """Kinded signature."""

    @classmethod
    def from_value(cls, kind: CryptoKind, value: BareSignature) -> Self:
        """Pair a bare signature with the cryptosystem kind it belongs to."""
        assert isinstance(kind, CryptoKind)
        assert isinstance(value, BareSignature)
        return cls(f"{kind}:{value}")

    def value(self) -> BareSignature:
        """The untagged signature."""
        return BareSignature(self._value())

class RouteId(CryptoTyped):
    """Kinded route id."""

    @classmethod
    def from_value(cls, kind: CryptoKind, value: BareRouteId) -> Self:
        """Pair a bare route id with the cryptosystem kind it belongs to."""
        assert isinstance(kind, CryptoKind)
        assert isinstance(value, BareRouteId)
        return cls(f"{kind}:{value}")

    def value(self) -> BareRouteId:
        """The untagged route id."""
        return BareRouteId(self._value())

class NodeId(CryptoTyped):
    """Kinded node id."""

    @classmethod
    def from_value(cls, kind: CryptoKind, value: BareNodeId) -> Self:
        """Pair a bare node id with the cryptosystem kind it belongs to."""
        assert isinstance(kind, CryptoKind)
        assert isinstance(value, BareNodeId)
        return cls(f"{kind}:{value}")

    def value(self) -> BareNodeId:
        """The untagged node id."""
        return BareNodeId(self._value())

class MemberId(CryptoTyped):
    """Kinded schema member id."""

    @classmethod
    def from_value(cls, kind: CryptoKind, value: BareMemberId) -> Self:
        """Pair a bare member id with the cryptosystem kind it belongs to."""
        assert isinstance(kind, CryptoKind)
        assert isinstance(value, BareMemberId)
        return cls(f"{kind}:{value}")

    def value(self) -> BareMemberId:
        """The untagged member id."""
        return BareMemberId(self._value())

class ValueSubkey(int):
    """Index of a subkey within a DHT record."""

    pass


class ValueSeqNum(int):
    """Increasing sequence number ordering changes to a DHT subkey."""

    pass

####################################################################


@total_ordering
class VeilidVersion:
    """Semantic version of VeilidCore."""

    _major: int
    _minor: int
    _patch: int

    def __init__(self, major: int, minor: int, patch: int):
        self._major = major
        self._minor = minor
        self._patch = patch

    def __lt__(self, other):
        if other is None:
            return False
        if self._major < other._major:
            return True
        if self._major > other._major:
            return False
        if self._minor < other._minor:
            return True
        if self._minor > other._minor:
            return False
        if self._patch < other._patch:
            return True
        return False

    def __eq__(self, other):
        return (
            isinstance(other, VeilidVersion)
            and self._major == other._major
            and self._minor == other._minor
            and self._patch == other._patch
        )

    @property
    def major(self):
        """Major version number."""
        return self._major

    @property
    def minor(self):
        """Minor version number."""
        return self._minor

    @property
    def patch(self):
        """Patch version number."""
        return self._patch


class RouteBlob:
    """An allocated route's id paired with its encoded blob for import by another node."""

    route_id: RouteId
    blob: bytes

    def __init__(self, route_id: RouteId, blob: bytes):
        assert isinstance(route_id, RouteId)
        assert isinstance(blob, bytes)

        self.route_id = route_id
        self.blob = blob

    @classmethod
    def from_json(cls, j: dict) -> Self:
        """Build from its JSON representation."""
        return cls(RouteId(j["route_id"]), urlsafe_b64decode_no_pad(j["blob"]))

    def to_json(self) -> dict:
        """Convert to its JSON representation."""
        return self.__dict__


class DHTSchemaSMPLMember:
    """A member of a SMPL schema: a member key and its writable subkey count."""

    m_key: BareMemberId
    m_cnt: int

    def __init__(self, m_key: BareMemberId, m_cnt: int):
        assert isinstance(m_key, BareMemberId)
        assert isinstance(m_cnt, int)

        self.m_key = m_key
        self.m_cnt = m_cnt

    @classmethod
    def from_json(cls, j: dict) -> Self:
        """Build from its JSON representation."""
        return cls(BareMemberId(j["m_key"]), j["m_cnt"])

    def to_json(self) -> dict:
        """Convert to its JSON representation."""
        return self.__dict__


class DHTSchema(ABC):
    """Schema governing the subkeys of a DHT record."""

    kind: DHTSchemaKind

    def __init__(self, kind: DHTSchemaKind):
        self.kind = kind

    @classmethod
    def dflt(cls, o_cnt: int) -> Self:
        """Make a default schema with the given owner subkey count."""
        return DHTSchemaDFLT(o_cnt=o_cnt) # type: ignore

    @classmethod
    def smpl(cls, o_cnt: int, members: list[DHTSchemaSMPLMember]) -> Self:
        """Make a simple schema with the given owner subkey count and members."""
        return DHTSchemaSMPL(o_cnt=o_cnt, members=members) # type: ignore

    @classmethod
    def from_json(cls, j: dict) -> Self:
        """Build the matching schema kind from its JSON representation."""
        if DHTSchemaKind(j["kind"]) == DHTSchemaKind.DFLT:
            return cls.dflt(j["o_cnt"])
        if DHTSchemaKind(j["kind"]) == DHTSchemaKind.SMPL:
            return cls.smpl(
                j["o_cnt"],
                [DHTSchemaSMPLMember.from_json(member) for member in j["members"]],
            )
        raise Exception("Unknown DHTSchema kind", j["kind"])

    def to_json(self) -> dict:
        """Convert to its JSON representation."""
        return self.__dict__

@final
class DHTSchemaDFLT(DHTSchema):
    """Default schema: a fixed number of owner-writable subkeys."""

    o_cnt: int

    def __init__(
        self,
        o_cnt: int
    ):
        super().__init__(DHTSchemaKind.DFLT)

        assert isinstance(o_cnt, int)
        self.o_cnt = o_cnt


    @classmethod
    def from_json(cls, j: dict) -> Self:
        """Build from its JSON representation."""
        if DHTSchemaKind(j["kind"]) == DHTSchemaKind.DFLT:
            return cls(j["o_cnt"])
        raise Exception("Invalid DHTSchemaDFLT")


@final
class DHTSchemaSMPL(DHTSchema):
    """Simple schema: owner subkeys plus per-member writable subkeys."""

    o_cnt: int
    members: list[DHTSchemaSMPLMember]

    def __init__(
        self,
        o_cnt: int,
        members: list[DHTSchemaSMPLMember]
    ):
        super().__init__(DHTSchemaKind.SMPL)

        assert isinstance(o_cnt, int)
        assert isinstance(members, list)
        for m in members:
            assert isinstance(m, DHTSchemaSMPLMember)

        self.o_cnt = o_cnt
        self.members = members

    @classmethod
    def from_json(cls, j: dict) -> Self:
        """Build from its JSON representation."""
        if DHTSchemaKind(j["kind"]) == DHTSchemaKind.SMPL:
            return cls(j["o_cnt"],
                [DHTSchemaSMPLMember.from_json(member) for member in j["members"]])
        raise Exception("Invalid DHTSchemaSMPL")

class DHTRecordDescriptor:
    """Metadata describing a DHT record: its key, owner, optional owner secret, and schema."""

    key: RecordKey
    owner: PublicKey
    owner_secret: Optional[SecretKey]
    schema: DHTSchema

    def __init__(
        self,
        key: RecordKey,
        owner: PublicKey,
        owner_secret: Optional[SecretKey],
        schema: DHTSchema,
    ):
        self.key = key
        self.owner = owner
        self.owner_secret = owner_secret
        self.schema = schema

    def __repr__(self) -> str:
        return f"<{self.__class__.__name__}(key={self.key!r}, owner={self.owner!r}, owner_secret={self.owner_secret!r}, schema={self.schema!r})>"

    def owner_bare_key_pair(self) -> Optional[BareKeyPair]:
        """The owner's untyped public and secret keys as a pair, or `None` when the secret is unknown."""
        if self.owner_secret is None:
            return None
        return BareKeyPair.from_parts(self.owner.value(), self.owner_secret.value())

    def owner_key_pair(self) -> Optional[KeyPair]:
        """The owner's kinded public and secret keys as a pair, or `None` when the secret is unknown."""
        if self.owner_secret is None:
            return None
        return KeyPair.from_value(self.owner.kind(), BareKeyPair.from_parts(self.owner.value(), self.owner_secret.value()))

    @classmethod
    def from_json(cls, j: dict) -> Self:
        """Build from its JSON representation."""
        return cls(
            RecordKey(j["key"]),
            PublicKey(j["owner"]),
            None if j["owner_secret"] is None else SecretKey(j["owner_secret"]),
            DHTSchema.from_json(j["schema"]),
        )

    def to_json(self) -> dict:
        """Convert to its JSON representation."""
        return self.__dict__



class DHTRecordReport:
    """Report of the subkey ranges and local versus network sequence numbers of a DHT record."""

    subkeys: list[tuple[ValueSubkey, ValueSubkey]]
    offline_subkeys: list[tuple[ValueSubkey, ValueSubkey]]
    local_seqs: list[Optional[ValueSeqNum]]
    network_seqs: list[Optional[ValueSeqNum]]

    def __init__(
        self,
        subkeys: list[tuple[ValueSubkey, ValueSubkey]],
        offline_subkeys: list[tuple[ValueSubkey, ValueSubkey]],
        local_seqs: list[Optional[ValueSeqNum]],
        network_seqs: list[Optional[ValueSeqNum]],
    ):
        self.subkeys = subkeys
        self.offline_subkeys = offline_subkeys
        self.local_seqs = local_seqs
        self.network_seqs = network_seqs

    def __repr__(self) -> str:
        return f"<{self.__class__.__name__}(subkeys={self.subkeys!r}, offline_subkeys={self.offline_subkeys!r}, local_seqs={self.local_seqs!r}, network_seqs={self.network_seqs!r})>"

    @classmethod
    def from_json(cls, j: dict) -> Self:
        """Build from its JSON representation."""
        return cls(
            [(p[0], p[1]) for p in j["subkeys"]],
            [(p[0], p[1]) for p in j["offline_subkeys"]],
            [(ValueSeqNum(s) if s is not None else None) for s in j["local_seqs"] ],
            [(ValueSeqNum(s) if s is not None else None) for s in j["network_seqs"] ],
        )

    def to_json(self) -> dict:
        """Convert to its JSON representation."""
        return self.__dict__


class SetDHTValueOptions:
    """Options that override defaults for set_dht_value: an override writer and the offline-write policy."""

    writer: Optional[KeyPair]
    allow_offline: Optional[bool]

    def __init__(self, writer: Optional[KeyPair] = None, allow_offline: Optional[bool] = None):
        self.writer = writer
        self.allow_offline = allow_offline

    def __repr__(self) -> str:
        return f"<{self.__class__.__name__}(writer={self.writer!r}, allow_offline={self.allow_offline!r})>"

    @classmethod
    def from_json(cls, j: dict) -> Self:
        """Build from its JSON representation."""
        return cls(
            KeyPair(j["writer"]) if "writer" in j else None,
            j["allow_offline"] if "allow_offline" in j else None,
        )

    def to_json(self) -> dict:
        """Convert to its JSON representation."""
        return self.__dict__


class DHTTransactionSetValueOptions:
    """Options that override defaults for a transaction set: an override writer key pair."""

    writer: Optional[KeyPair]

    def __init__(self, writer: Optional[KeyPair] = None):
        self.writer = writer

    def __repr__(self) -> str:
        return f"<{self.__class__.__name__}(writer={self.writer!r})>"

    @classmethod
    def from_json(cls, j: dict) -> Self:
        """Build from its JSON representation."""
        return cls(
            KeyPair(j["writer"]) if "writer" in j else None,
        )

    def to_json(self) -> dict:
        """Convert to its JSON representation."""
        return self.__dict__


class TransactDHTRecordsOptions:
    """Options for opening a DHT record transaction: the default signing keypair for read-only records."""

    default_signing_keypair: Optional[KeyPair]

    def __init__(self, default_signing_keypair: Optional[KeyPair] = None):
        self.default_signing_keypair = default_signing_keypair

    def __repr__(self) -> str:
        return f"<{self.__class__.__name__}(default_signing_keypair={self.default_signing_keypair!r})>"

    @classmethod
    def from_json(cls, j: dict) -> Self:
        """Build from its JSON representation."""
        return cls(
            KeyPair(j["default_signing_keypair"]) if "default_signing_keypair" in j else None,
        )

    def to_json(self) -> dict:
        """Convert to its JSON representation."""
        return self.__dict__


@total_ordering
class ValueData:
    """A DHT value and its metadata: sequence number, data bytes, and writer public key."""

    seq: ValueSeqNum
    data: bytes
    writer: PublicKey

    def __init__(self, seq: ValueSeqNum, data: bytes, writer: PublicKey):
        self.seq = seq
        self.data = data
        self.writer = writer

    def __repr__(self) -> str:
        return f"<{self.__class__.__name__}(seq={self.seq!r}, data={self.data!r}, writer={self.writer!r})>"

    def __lt__(self, other):
        if other is None:
            return True
        if self.data < other.data:
            return True
        if self.data > other.data:
            return False
        if self.seq < other.seq:
            return True
        if self.seq > other.seq:
            return False
        if self.writer < other.writer:
            return True
        return False

    def __eq__(self, other):
        return (
            isinstance(other, ValueData)
            and self.data == other.data
            and self.seq == other.seq
            and self.writer == other.writer
        )

    @classmethod
    def from_json(cls, j: dict) -> Self:
        """Build from its JSON representation."""
        return cls(
            ValueSeqNum(j["seq"]),
            urlsafe_b64decode_no_pad(j["data"]),
            PublicKey(j["writer"]),
        )

    def to_json(self) -> dict:
        """Convert to its JSON representation."""
        return self.__dict__


####################################################################


class PrivateSpec:
    """Options for private routes (receiver privacy): crypto kinds, hop count, stability, and sequencing."""

    crypto_kinds: list[CryptoKind]
    hop_count: int
    stability: Stability
    sequencing: Sequencing

    def __init__(
        self,
        crypto_kinds: list[CryptoKind] = [],
        hop_count: int = 0,
        stability: Stability = Stability.RELIABLE,
        sequencing: Sequencing = Sequencing.PREFER_ORDERED,
    ):
        self.crypto_kinds = crypto_kinds
        self.hop_count = hop_count
        self.stability = stability
        self.sequencing = sequencing

    @classmethod
    def from_json(cls, j: dict) -> Self:
        """Build from its JSON representation."""
        return cls(
            [CryptoKind(ck) for ck in j["crypto_kinds"]] if "crypto_kinds" in j else [],
            j["hop_count"] if "hop_count" in j else 0,
            Stability(j["stability"]) if "stability" in j else Stability.RELIABLE,
            Sequencing(j["sequencing"]) if "sequencing" in j else Sequencing.PREFER_ORDERED,
        )

    def to_json(self) -> dict:
        """Convert to its JSON representation."""
        return self.__dict__

class SafetySpec:
    """Options for safety routes (sender privacy): preferred route, hop count, stability, and sequencing."""

    preferred_route: Optional[RouteId]
    hop_count: int
    stability: Stability
    sequencing: Sequencing

    def __init__(
        self,
        preferred_route: Optional[RouteId] = None,
        hop_count: int = 0,
        stability: Stability = Stability.RELIABLE,
        sequencing: Sequencing = Sequencing.PREFER_ORDERED,
    ):
        self.preferred_route = preferred_route
        self.hop_count = hop_count
        self.stability = stability
        self.sequencing = sequencing

    @classmethod
    def from_json(cls, j: dict) -> Self:
        """Build from its JSON representation."""
        return cls(
            RouteId(j["preferred_route"]) if "preferred_route" in j else None,
            j["hop_count"] if "hop_count" in j else 0,
            Stability(j["stability"]) if "stability" in j else Stability.RELIABLE,
            Sequencing(j["sequencing"]) if "sequencing" in j else Sequencing.PREFER_ORDERED,
        )

    def to_json(self) -> dict:
        """Convert to its JSON representation."""
        return self.__dict__

class SafetySelection(ABC):
    """The choice of safety route to include in compiled routes: unsafe, or safe with a SafetySpec."""

    @property
    @abstractmethod
    def kind(self) -> SafetySelectionKind:
        """Whether this selection is Safe or Unsafe."""
        pass

    @classmethod
    def unsafe(cls, sequencing: Sequencing = Sequencing.PREFER_ORDERED) -> Self:
        """Make a selection that uses no safety route, only a sequencing preference."""
        return SafetySelectionUnsafe(sequencing=sequencing) # type: ignore

    @classmethod
    def safe(cls, safety_spec: SafetySpec) -> Self:
        """Make a selection that uses a safety route described by a SafetySpec."""
        return SafetySelectionSafe(safety_spec=safety_spec) # type: ignore

    @classmethod
    def from_json(cls, j: dict) -> Self:
        """Build the matching selection variant from its JSON representation."""
        if "Safe" in j:
            return cls.safe(SafetySpec.from_json(j["Safe"]))
        elif "Unsafe" in j:
            return cls.unsafe(Sequencing(j["Unsafe"]))
        raise Exception("Invalid SafetySelection")

    @abstractmethod
    def to_json(self) -> dict:
        """Convert to its JSON representation."""
        pass

@final
class SafetySelectionUnsafe(SafetySelection):
    """Selection that uses no safety route, only a sequencing preference."""

    sequencing: Sequencing

    def __init__(self, sequencing: Sequencing = Sequencing.PREFER_ORDERED):
        assert isinstance(sequencing, Sequencing)
        self.sequencing = sequencing

    @property
    def kind(self):
        """Always SafetySelectionKind.UNSAFE."""
        return SafetySelectionKind.UNSAFE

    @classmethod
    def from_json(cls, j: dict) -> Self:
        """Build from its JSON representation."""
        if "Unsafe" in j:
            return cls(Sequencing(j["Unsafe"]))
        raise Exception("Invalid SafetySelectionUnsafe")

    def to_json(self) -> dict:
        """Convert to its JSON representation."""
        return {"Unsafe": self.sequencing}

@final
class SafetySelectionSafe(SafetySelection):
    """Selection that uses a safety route described by a SafetySpec."""

    safety_spec: SafetySpec

    def __init__(self, safety_spec: SafetySpec):
        assert isinstance(safety_spec, SafetySpec)
        self.safety_spec = safety_spec

    @property
    def kind(self):
        """Always SafetySelectionKind.SAFE."""
        return SafetySelectionKind.SAFE

    @classmethod
    def from_json(cls, j: dict) -> Self:
        """Build from its JSON representation."""
        if "Safe" in j:
            return cls(SafetySpec.from_json(j["Safe"]))
        raise Exception("Invalid SafetySelectionUnsafe")

    def to_json(self) -> dict:
        """Convert to its JSON representation."""
        return {"Safe": self.safety_spec.to_json()}



class Target(ABC):
    """A destination for a message sent over a routing context: a node id or a remote private route id."""

    @property
    @abstractmethod
    def kind(self) -> TargetKind:
        """Whether this target is a node id or a route id."""
        pass

    @classmethod
    def node_id(cls, node_id: NodeId) -> Self:
        """Make a target addressing a node by its node id."""
        return TargetNodeId(node_id=node_id) # type: ignore

    @classmethod
    def route_id(cls, route_id: RouteId) -> Self:
        """Make a target addressing a remote private route by its id."""
        return TargetRouteId(route_id=route_id) # type: ignore

    @classmethod
    def from_json(cls, j: dict) -> Self:
        """Build the matching target variant from its JSON representation."""
        if "NodeId" in j:
            return cls.node_id(NodeId(j["NodeId"]))
        elif "RouteId" in j:
            return cls.route_id(RouteId(j["Unsafe"]))
        raise Exception("Invalid Target")

    @abstractmethod
    def to_json(self) -> dict:
        """Convert to its JSON representation."""
        pass

@final
class TargetNodeId(Target):
    """Target addressing a node by its node id."""

    id: NodeId

    def __init__(self, node_id: NodeId):
        assert isinstance(node_id, NodeId)
        self.id = node_id

    @property
    def kind(self):
        """Always TargetKind.NODE_ID."""
        return TargetKind.NODE_ID

    @classmethod
    def from_json(cls, j: dict) -> Self:
        """Build from its JSON representation."""
        if "NodeId" in j:
            return cls(NodeId(j["NodeId"]))
        raise Exception("Invalid TargetNodeId")

    def to_json(self) -> dict:
        """Convert to its JSON representation."""
        return {"NodeId": self.id}

@final
class TargetRouteId(Target):
    """Target addressing a remote private route by its id."""

    id: RouteId

    def __init__(self, route_id: RouteId):
        assert isinstance(route_id, RouteId)
        self.id = route_id

    @property
    def kind(self):
        """Always TargetKind.ROUTE_ID."""
        return TargetKind.ROUTE_ID

    @classmethod
    def from_json(cls, j: dict) -> Self:
        """Build from its JSON representation."""
        if "RouteId" in j:
            return cls(RouteId(j["RouteId"]))
        raise Exception("Invalid TargetRouteId")

    def to_json(self) -> dict:
        """Convert to its JSON representation."""
        return {"RouteId": self.id}
