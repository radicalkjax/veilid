"""Abstract base classes for the Veilid API, mirroring the veilid-core public API."""

from abc import ABC, abstractmethod
from typing import Optional, Self

from . import types
from .state import VeilidState


class RoutingContext(ABC):
    """Specifies the communication preferences for messages sent over the Veilid network.

    By default routing contexts have 'safety routing' enabled, which offers sender privacy.
    To disable it and send RPC operations straight from the node, use with_safety() with an
    unsafe SafetySelection. To enable receiver privacy, send to a private route RouteId you
    have imported rather than directly to a NodeId.

    Holds a server-side handle; release it (or use `async with`), else dropping it asserts.
    """

    ref_count: int

    def __init__(
        self,
    ):
        self.ref_count = 0

    async def __aenter__(self) -> Self:
        self.ref_count += 1
        return self

    async def __aexit__(self, *excinfo):
        self.ref_count -= 1
        if self.ref_count == 0 and not self.is_done():
            await self.release()

    @abstractmethod
    def is_done(self) -> bool:
        """Check whether this routing context has been released."""
        pass

    @abstractmethod
    async def release(self):
        """Release the routing context and free its resources.

        Idempotent: a no-op if already released. Otherwise awaits a server round-trip.
        """
        pass

    @abstractmethod
    async def with_default_safety(self, release=True) -> Self:
        """Return a routing context with default safety, sequencing, and stability parameters.

        Turns on sender privacy, enabling the use of safety routes. Returns a new context the
        caller must release; with release=True (default) the current context is released.

        Raises VeilidAPIErrorNotInitialized if the node is shut down.
        """
        pass

    @abstractmethod
    async def with_safety(
        self, safety_selection: types.SafetySelection, release=True
    ) -> Self:
        """Return a routing context using a custom SafetySelection. Pass an unsafe selection to disable safety.

        Returns a new context the caller must release; with release=True (default) the current
        context is released.

        Raises VeilidAPIErrorGeneric if an unsafe selection is requested without the footgun feature,
        or if the hop count exceeds the configured maximum. Raises VeilidAPIErrorInvalidArgument if a
        preferred route is set whose crypto kind is unsupported or whose length is wrong. Raises
        VeilidAPIErrorNotInitialized if the node is shut down.
        """
        pass

    @abstractmethod
    async def with_sequencing(self, sequencing: types.Sequencing, release=True) -> Self:
        """Return a routing context using the given Sequencing preference, with or without privacy.

        Returns a new context the caller must release; with release=True (default) the current
        context is released.
        """
        pass

    @abstractmethod
    async def safety(self) -> types.SafetySelection:
        """Get the safety selection in use on this routing context."""
        pass

    @abstractmethod
    async def app_call(self, target: types.Target, message: bytes) -> bytes:
        """Bidirectional app-level call that expects a response.

        target is a private route id (or a node id with the footgun feature enabled);
        message is an arbitrary blob of up to 32768 bytes. Returns an answer blob of up to 32768 bytes.

        Blocks on the network round-trip to the target.

        Raises VeilidAPIErrorInvalidTarget if target is a node id (only route ids are permitted
        without the footgun feature). Otherwise raises VeilidAPIErrorNoConnection if the route could
        not be resolved or allocated, VeilidAPIErrorTimeout if the reply deadline elapsed, or
        VeilidAPIErrorTryAgain if a route is temporarily unavailable; all three are retryable.
        Raises VeilidAPIErrorNotInitialized if the node is shut down.
        """
        pass

    @abstractmethod
    async def app_message(self, target: types.Target, message: bytes):
        """Unidirectional app-level message that expects no response.

        target is a private route (or a node id with the footgun feature enabled);
        message is an arbitrary blob of up to 32768 bytes.

        Raises VeilidAPIErrorInvalidTarget if target is a node id (only route ids are permitted
        without the footgun feature). Otherwise raises VeilidAPIErrorNoConnection if the route could
        not be resolved or allocated, VeilidAPIErrorTimeout if dispatch timed out, or
        VeilidAPIErrorTryAgain if a route is temporarily unavailable; all three are retryable.
        Raises VeilidAPIErrorNotInitialized if the node is shut down.
        """
        pass

    @abstractmethod
    async def create_dht_record(
        self, kind: types.CryptoKind, schema: types.DHTSchema, owner: Optional[types.KeyPair] = None
    ) -> types.DHTRecordDescriptor:
        """Create a new DHT record. The record is considered 'open' after creation succeeds.

        kind selects the cryptosystem. schema is the record's schema. If owner is given, its
        crypto kind must match kind; otherwise a random owner keypair is generated. Passing an
        owner makes this call deterministic: creating a record for an owner and schema that
        already exists will fail. Returns the new record's descriptor.

        Leaves the record open; close it with close_dht_record. Blocks on the network.

        Raises VeilidAPIErrorGeneric if kind is an unsupported crypto kind, schema has an invalid
        size or fourcc, or owner is a malformed keypair. Raises VeilidAPIErrorInvalidArgument if
        schema has an invalid subkey/member/writer count, owner is the wrong crypto kind for kind,
        or this node's id would be a schema member. Raises VeilidAPIErrorNotInitialized if the node
        is shut down.
        """
        pass

    @abstractmethod
    async def open_dht_record(
        self, key: types.RecordKey, writer: Optional[types.KeyPair] = None
    ) -> types.DHTRecordDescriptor:
        """Open a DHT record at a specific key.

        Associates a default writer keypair if one is given, granting writer capability that
        set_dht_value can override. Re-opening an already-open record adopts the new writer and
        routing context while keeping active watches. Returns the opened record's descriptor.

        Leaves the record open; close it with close_dht_record. Blocks on the network.

        Raises VeilidAPIErrorGeneric if key is an unsupported kind or malformed, or writer is a
        malformed keypair. Raises VeilidAPIErrorTryAgain (retryable) if the record is not yet local
        and the node is offline, VeilidAPIErrorKeyNotFound if the record does not exist on the
        network, and VeilidAPIErrorNotInitialized if the node is shut down.
        """
        pass

    @abstractmethod
    async def close_dht_record(self, key: types.RecordKey):
        """Close a DHT record opened with create_dht_record or open_dht_record, allowing it to be re-opened with a different routing context.

        Closing a record that is local but not currently open is a no-op; closing one not in the
        local store raises VeilidAPIErrorKeyNotFound. Raises VeilidAPIErrorGeneric if key is an
        unsupported kind or malformed, and VeilidAPIErrorNotInitialized if the node is shut down.
        None of these are retryable.
        """
        pass

    @abstractmethod
    async def delete_dht_record(self, key: types.RecordKey):
        """Delete a DHT record at a specific key.

        The record must be closed first. This removes only the local storage and stops this node
        from refreshing the value on the network; it does not delete the record from the network.

        Raises VeilidAPIErrorGeneric if key is an unsupported kind or malformed,
        VeilidAPIErrorKeyNotFound if the record is not in the local store, and
        VeilidAPIErrorNotInitialized if the node is shut down. None are retryable.
        """
        pass

    @abstractmethod
    async def get_dht_value(
        self, key: types.RecordKey, subkey: types.ValueSubkey, force_refresh: bool = False
    ) -> Optional[types.ValueData]:
        """Get the latest value of a subkey on an opened record.

        May pull from the network; force_refresh forces a network refresh. Returns None if the
        subkey has not been set, or the ValueData if it has.

        Raises VeilidAPIErrorInvalidArgument if the record is not open or key is malformed,
        VeilidAPIErrorGeneric if key is an unsupported kind, VeilidAPIErrorTryAgain (retryable) if a
        network refresh is needed and the node is offline, VeilidAPIErrorKeyNotFound if the record
        no longer exists, and VeilidAPIErrorNotInitialized if the node is shut down.
        """
        pass

    @abstractmethod
    async def set_dht_value(
        self, key: types.RecordKey, subkey: types.ValueSubkey, data: bytes, options: Optional[types.SetDHTValueOptions] = None
    ) -> Optional[types.ValueData]:
        """Push a changed subkey value to the network. The record must first be opened or created.

        The writer in options, if given, overrides the default writer from open. Returns None on
        success, or the network's ValueData if the value set was older than what is on the network.

        Raises VeilidAPIErrorInvalidArgument if the record is not open. Raises VeilidAPIErrorGeneric
        if key is an unsupported kind, the record is not writable, or the value fails schema
        validation (subkey out of range, data over the per-subkey limit, or wrong writer for the
        subkey). Raises VeilidAPIErrorTryAgain (retryable) if the record is currently in a
        transaction, VeilidAPIErrorKeyNotFound if the record no longer exists, and
        VeilidAPIErrorNotInitialized if the node is shut down. A failed network fanout does not
        raise; the write is deferred and None is returned.
        """
        pass

    @abstractmethod
    async def watch_dht_values(
        self,
        key: types.RecordKey,
        subkeys: list[tuple[types.ValueSubkey, types.ValueSubkey]] = [],
        expiration: types.Timestamp = types.Timestamp(0),
        count: int = 0xFFFFFFFF,
    ) -> bool:
        """Add or update a watch that fires a ValueChange update when watched subkeys change. Only one watch is permitted per record.

        key must first be opened. An empty subkeys list watches the entire range; otherwise at
        most 512 non-overlapping subranges. A zero expiration means no expiration; a count of
        zero is equivalent to a cancellation. Returns True if a watch is active, False if the
        whole watch was cancelled.

        Calling again on the same record replaces the prior watch rather than stacking. Blocks
        on the network.

        Raises VeilidAPIErrorInvalidArgument if the record is not open, key is malformed, or a
        non-zero expiration is sooner than the configured RPC timeout in the future. Raises
        VeilidAPIErrorGeneric if key is an unsupported kind or no local record is found, and
        VeilidAPIErrorNotInitialized if the node is shut down. None are retryable; reconciliation is
        deferred to a background task, so no network errors surface here.
        """
        pass

    @abstractmethod
    async def cancel_dht_watch(
        self,
        key: types.RecordKey,
        subkeys: list[tuple[types.ValueSubkey, types.ValueSubkey]] = [],
    ) -> bool:
        """Cancel a watch early by subtracting the given subkeys from the watched range.

        Expiration and count are unchanged. If no subkeys remain, the watch is cancelled entirely.
        Returns True if a watch is still active, False if the whole watch was cancelled.

        Raises VeilidAPIErrorInvalidArgument if the record is not open or key is malformed,
        VeilidAPIErrorGeneric if key is an unsupported kind or no local record is found, and
        VeilidAPIErrorNotInitialized if the node is shut down. None are retryable.
        """
        pass

    @abstractmethod
    async def inspect_dht_record(
        self,
        key: types.RecordKey,
        subkeys: list[tuple[types.ValueSubkey, types.ValueSubkey]],
        scope: types.DHTReportScope = types.DHTReportScope.LOCAL,
    ) -> types.DHTRecordReport:
        """Inspect an opened DHT record for subkey state, to decide what to push or retrieve.

        key must first be opened. subkeys is at most 512 non-overlapping subranges. scope selects
        whether local and/or network sequence numbers are reported and which fanout parameters
        apply. Returns a report with the subkey ranges overlapping the schema and their sequence
        numbers. The LOCAL scope is local-only; the Sync/Update scopes block on a network fanout.

        Raises VeilidAPIErrorInvalidArgument if the record is not open or key is malformed,
        VeilidAPIErrorGeneric if key is an unsupported kind, VeilidAPIErrorTryAgain (retryable) if a
        network scope is requested and the node is offline, and VeilidAPIErrorNotInitialized if the
        node is shut down.
        """
        pass

    @abstractmethod
    async def flush_dht_record(
        self,
        key: types.RecordKey,
        timeout_ms: Optional[int] = None
    ) -> bool:
        """Wait for pending offline subkey writes for a record to be flushed to the network.

        Returns True immediately if there are no pending writes. With a timeout, returns True if
        all writes flushed or False if the timeout elapsed first. Without a timeout, waits
        indefinitely and returns True.

        Raises VeilidAPIErrorGeneric if key is an unsupported kind or malformed, and
        VeilidAPIErrorNotInitialized if the node shuts down while waiting.
        """
        pass



class TableDbTransaction(ABC):
    """A table store transaction that atomically commits a group of stores and deletes.

    The transaction must be committed or rolled back before it is dropped.
    """

    ref_count: int

    def __init__(
        self,
    ):
        self.ref_count = 0

    async def __aenter__(self) -> Self:
        self.ref_count += 1
        return self

    async def __aexit__(self, *excinfo):
        self.ref_count -= 1
        if self.ref_count == 0 and not self.is_done():
            await self.rollback()

    @abstractmethod
    def is_done(self) -> bool:
        """Check whether this transaction has been committed or rolled back."""
        pass

    @abstractmethod
    async def commit(self):
        """Commit the transaction, performing all stores and deletes atomically.

        Consumes the transaction; raises if already committed or rolled back. Blocks on disk.

        An empty transaction commits as a no-op. Raises VeilidAPIErrorGeneric if the backing-store
        write fails (the buffered writes are then lost), or if the transaction was already completed
        server-side.
        """
        pass

    @abstractmethod
    async def rollback(self):
        """Roll back the transaction. Does nothing to the table.

        Consumes the transaction; raises if already committed or rolled back.
        """
        pass

    @abstractmethod
    async def store(self, key: bytes, value: bytes, col: int = 0):
        """Store a key with a value in a column. Staged in the transaction until commit."""
        pass

    @abstractmethod
    async def delete(self, key: bytes, col: int = 0):
        """Delete a key from a column. Staged in the transaction until commit."""
        pass


class TableDb(ABC):
    """A handle to an opened encrypted key-value table.

    Release it (or use `async with`), else dropping it asserts.
    """

    ref_count: int

    def __init__(
        self,
    ):
        self.ref_count = 0

    async def __aenter__(self) -> Self:
        self.ref_count += 1
        return self

    async def __aexit__(self, *excinfo):
        self.ref_count -= 1
        if self.ref_count == 0 and not self.is_done():
            await self.release()

    @abstractmethod
    def is_done(self) -> bool:
        """Check whether this table handle has been released."""
        pass

    @abstractmethod
    async def release(self):
        """Release this table handle and free its resources.

        Idempotent: a no-op if already released.
        """
        pass

    @abstractmethod
    async def get_column_count(self) -> int:
        """Get the total number of columns in the table, not the number opened.

        Raises VeilidAPIErrorGeneric (or VeilidAPIErrorInternal on out-of-memory) if the backing
        store fails to report its column count.
        """
        pass

    @abstractmethod
    async def get_keys(self, col: int = 0) -> list[bytes]:
        """Get the list of keys in a column.

        Raises VeilidAPIErrorGeneric if col is at or above the opened column count, the
        backing-store read fails, or a stored key fails to decrypt or decompress (wrong device
        encryption key or corrupt data).
        """
        pass

    @abstractmethod
    async def transact(self) -> TableDbTransaction:
        """Start a write transaction. It must be committed or rolled back before being dropped.

        Returns a handle the caller must commit or rollback (or use `async with`), else dropping
        it asserts.
        """
        pass

    @abstractmethod
    async def store(self, key: bytes, value: bytes, col: int = 0):
        """Store a key with a value in a column, committed immediately.

        Raises VeilidAPIErrorGeneric if col is at or above the opened column count or the
        backing-store write fails.
        """
        pass

    @abstractmethod
    async def load(self, key: bytes, col: int = 0) -> Optional[bytes]:
        """Read a key from a column. Returns None if the key is absent.

        Raises VeilidAPIErrorGeneric if col is at or above the opened column count, the
        backing-store read fails, or the stored value fails to decrypt or decompress (wrong device
        encryption key or corrupt data).
        """
        pass

    @abstractmethod
    async def delete(self, key: bytes, col: int = 0) -> Optional[bytes]:
        """Delete a key from a column, returning its old value or None if absent.

        Raises VeilidAPIErrorGeneric if col is at or above the opened column count, the
        backing-store delete fails, or the prior value fails to decrypt or decompress (wrong device
        encryption key or corrupt data).
        """
        pass


class CryptoSystem(ABC):
    """A single cryptosystem's primitives: key generation, signing, AEAD and unauthenticated
    encryption, Diffie-Hellman exchange and shared secret derivation, hashing, password
    hashing, and random byte generation.

    Each cryptosystem is tagged by a crypto kind; keys, signatures, nonces, and digests carry
    that kind and are only accepted by the matching cryptosystem.

    Holds a server-side handle; release it (or use `async with`), else dropping it asserts.
    """

    ref_count: int

    def __init__(
        self,
    ):
        self.ref_count = 0

    async def __aenter__(self) -> Self:
        self.ref_count += 1
        return self

    async def __aexit__(self, *excinfo):
        self.ref_count -= 1
        if self.ref_count == 0 and not self.is_done():
            await self.release()

    @abstractmethod
    async def kind(self) -> types.CryptoKind:
        """The crypto kind fourcc identifying this cryptosystem."""
        pass

    @abstractmethod
    def is_done(self) -> bool:
        """Check whether this cryptosystem handle has been released."""
        pass

    @abstractmethod
    async def release(self):
        """Release this cryptosystem handle and free its resources.

        Idempotent: a no-op if already released.
        """
        pass

    @abstractmethod
    async def cached_dh(self, key: types.PublicKey, secret: types.SecretKey) -> types.SharedSecret:
        """Diffie-Hellman shared secret for the public/secret key pair, memoized in the DH cache.

        Raises VeilidAPIErrorGeneric if key or secret carries the wrong kind or length, plus the
        compute_dh errors on a cache miss.
        """
        pass

    @abstractmethod
    async def compute_dh(
        self, key: types.PublicKey, secret: types.SecretKey
    ) -> types.SharedSecret:
        """Raw Diffie-Hellman shared secret for the public/secret key pair, with no caching.

        Raises VeilidAPIErrorInternal if key is not a valid curve point, VeilidAPIErrorGeneric if
        the exchange is non-contributory (low-order public key).
        """
        pass

    @abstractmethod
    async def generate_shared_secret(
        self, key: types.PublicKey, secret: types.SecretKey, domain: bytes
    ) -> types.SharedSecret:
        """Derive a domain-separated shared secret from a key exchange.

        Computes the DH secret, then hashes it with domain and the Veilid API domain tag.
        Distinct domain values yield independent secrets from the same key pair.

        Raises the compute_dh errors if the key exchange fails.
        """
        pass

    @abstractmethod
    async def hpke_seal(
        self, recipient: types.EncapsulationKey, aad: bytes, plaintext: bytes
    ) -> bytes:
        """Seal plaintext to a recipient KEM encapsulation key with HPKE base mode (RFC 9180), single-shot.

        aad is authenticated but not encrypted. Returns a self-describing sealed blob.

        Sealing is one-way: only the holder of the recipient DecapsulationKey can open
        the blob, and the sealer cannot decrypt what it just sealed. This differs from
        the DH pattern, where the shared secret let the encrypting party decrypt its own
        blobs. A sealer that needs to re-read stored blobs must also seal them to its own
        key. Callers who already share a symmetric key want encrypt_aead instead; HPKE is
        for encrypting to a recipient's key when no shared secret exists.

        Raises VeilidAPIErrorInvalidArgument if recipient is not a valid key,
        VeilidAPIErrorGeneric if encapsulation fails (including a low-order key).
        """
        pass

    @abstractmethod
    async def hpke_open(
        self, secret: types.DecapsulationKey, aad: bytes, sealed: bytes
    ) -> bytes:
        """Open a sealed blob produced by hpke_seal with the recipient KEM decapsulation key.

        aad must match what was supplied at seal. Only the recipient can open a sealed
        blob; the sealer cannot.

        Raises VeilidAPIErrorParseError if the blob is truncated or its version is unknown,
        VeilidAPIErrorInvalidArgument if the blob's kind is not this cryptosystem's kind or
        secret is not a valid key, VeilidAPIErrorGeneric if decryption fails.
        """
        pass

    @abstractmethod
    async def generate_kem_key_pair(self) -> types.KemKeyPair:
        """Generate a fresh random KEM key pair for this cryptosystem."""
        pass

    @abstractmethod
    async def encapsulation_key_from_signing_key(
        self, key: types.PublicKey
    ) -> types.EncapsulationKey:
        """Derive the KEM encapsulation key corresponding to a signing public key.

        VLD0-only bridge (ed25519 to x25519): kinds whose signing and KEM keys are
        unrelated raise VeilidAPIErrorUnimplemented.

        Raises VeilidAPIErrorInvalidArgument if key is not a valid signing public key.
        """
        pass

    @abstractmethod
    async def decapsulation_key_from_signing_secret(
        self, secret: types.SecretKey
    ) -> types.DecapsulationKey:
        """Derive the KEM decapsulation key corresponding to a signing secret key.

        VLD0-only bridge (ed25519 to x25519): kinds whose signing and KEM keys are
        unrelated raise VeilidAPIErrorUnimplemented.

        Raises VeilidAPIErrorInvalidArgument if secret is not a valid signing secret key.
        """
        pass

    @abstractmethod
    async def random_bytes(self, len: int) -> bytes:
        """Return len bytes from the cryptographic RNG."""
        pass

    @abstractmethod
    async def shared_secret_length(self) -> int:
        """Byte length of a shared secret."""
        pass

    @abstractmethod
    async def nonce_length(self) -> int:
        """Byte length of a nonce."""
        pass

    @abstractmethod
    async def hash_digest_length(self) -> int:
        """Byte length of a hash digest."""
        pass

    @abstractmethod
    async def public_key_length(self) -> int:
        """Byte length of a public key."""
        pass

    @abstractmethod
    async def secret_key_length(self) -> int:
        """Byte length of a secret key."""
        pass

    @abstractmethod
    async def encapsulation_key_length(self) -> int:
        """Byte length of a KEM encapsulation key."""
        pass

    @abstractmethod
    async def decapsulation_key_length(self) -> int:
        """Byte length of a KEM decapsulation key."""
        pass

    @abstractmethod
    async def signature_length(self) -> int:
        """Byte length of a signature."""
        pass

    @abstractmethod
    async def default_salt_length(self) -> int:
        """Default salt length in bytes for password hashing and KDF operations."""
        pass

    @abstractmethod
    async def aead_overhead(self) -> int:
        """Bytes an AEAD operation adds to the ciphertext (the authentication tag length)."""
        pass

    @abstractmethod
    async def check_shared_secret(self, secret: types.SharedSecret):
        """Verify a shared secret carries this cryptosystem's kind and the correct length.

        Raises VeilidAPIErrorGeneric if the shared secret has the wrong kind or length.
        """
        pass

    @abstractmethod
    async def check_nonce(self, nonce: types.Nonce):
        """Verify a nonce has the correct length.

        Raises VeilidAPIErrorGeneric if the nonce has the wrong length.
        """
        pass

    @abstractmethod
    async def check_hash_digest(self, digest: types.HashDigest):
        """Verify a hash digest carries this cryptosystem's kind and the correct length.

        Raises VeilidAPIErrorGeneric if the hash digest has the wrong kind or length.
        """
        pass

    @abstractmethod
    async def check_public_key(self, key: types.PublicKey):
        """Verify a public key carries this cryptosystem's kind and the correct length.

        Raises VeilidAPIErrorGeneric if the public key has the wrong kind or length.
        """
        pass

    @abstractmethod
    async def check_secret_key(self, key: types.SecretKey):
        """Verify a secret key carries this cryptosystem's kind and the correct length.

        Raises VeilidAPIErrorGeneric if the secret key has the wrong kind or length.
        """
        pass

    @abstractmethod
    async def check_signature(self, signature: types.Signature):
        """Verify a signature carries this cryptosystem's kind and the correct length.

        Raises VeilidAPIErrorGeneric if the signature has the wrong kind or length.
        """
        pass

    @abstractmethod
    async def hash_password(self, password: bytes, salt: bytes) -> str:
        """Hash a password with the given salt, returning a self-describing PHC hash string for storage and later verify_password.

        Raises VeilidAPIErrorGeneric if the salt length is outside the Argon2 bounds or the KDF
        fails, VeilidAPIErrorParseError if the salt fails base64 encoding.
        """
        pass

    @abstractmethod
    async def verify_password(self, password: bytes, password_hash: str) -> bool:
        """Check a password against a PHC hash string from hash_password. Returns False on mismatch; errors only on a malformed hash.

        Raises VeilidAPIErrorParseError if password_hash is not a valid PHC string.
        """
        pass

    @abstractmethod
    async def derive_shared_secret(self, password: bytes, salt: bytes) -> types.SharedSecret:
        """Derive a shared secret from a password and salt via a password-hashing KDF.

        Deterministic: the same password and salt always yield the same secret. Distinct from
        generate_shared_secret, which uses key exchange.

        Raises VeilidAPIErrorGeneric if the salt length is outside the Argon2 bounds or the KDF fails.
        """
        pass

    @abstractmethod
    async def random_nonce(self) -> types.Nonce:
        """A fresh random nonce of nonce_length bytes."""
        pass

    @abstractmethod
    async def random_shared_secret(self) -> types.SharedSecret:
        """A fresh random shared secret of shared_secret_length bytes."""
        pass

    @abstractmethod
    async def generate_key_pair(self) -> types.KeyPair:
        """Generate a fresh random signing key pair for this cryptosystem."""
        pass

    @abstractmethod
    async def generate_hash(self, data: bytes) -> types.HashDigest:
        """Hash a byte slice, returning a digest tagged with this cryptosystem's kind."""
        pass

    @abstractmethod
    async def validate_key_pair(self, key: types.PublicKey, secret: types.SecretKey) -> bool:
        """Check that a public and secret key form a usable signing pair. Returns False if they do not match.

        Raises VeilidAPIErrorGeneric if key or secret has the wrong kind or length.
        """
        pass

    @abstractmethod
    async def validate_hash(self, data: bytes, hash_digest: types.HashDigest) -> bool:
        """Recompute the hash of data and compare it against hash_digest. Returns True on match.

        Raises VeilidAPIErrorGeneric if hash_digest has the wrong kind or length.
        """
        pass

    @abstractmethod
    async def sign(
        self, key: types.PublicKey, secret: types.SecretKey, data: bytes
    ) -> types.Signature:
        """Sign data with the given key pair, returning a detached signature.

        Raises VeilidAPIErrorGeneric if key or secret has the wrong kind or length,
        VeilidAPIErrorParseError if they do not form a valid ed25519 keypair, VeilidAPIErrorInternal
        if signing fails.
        """
        pass

    @abstractmethod
    async def verify(self, key: types.PublicKey, data: bytes, signature: types.Signature) -> bool:
        """Verify a detached signature over data for the public key. Returns True if valid, False if not.

        Raises VeilidAPIErrorGeneric if key or signature has the wrong kind or length,
        VeilidAPIErrorParseError or VeilidAPIErrorInternal if key is not a valid ed25519 point. A
        signature that does not match returns False, not an error.
        """
        pass

    @abstractmethod
    async def decrypt_aead(
        self,
        body: bytes,
        nonce: types.Nonce,
        shared_secret: types.SharedSecret,
        associated_data: Optional[bytes],
    ) -> bytes:
        """Decrypt and authenticate body, returning the plaintext.

        associated_data must match what was supplied at encryption.

        Raises VeilidAPIErrorGeneric if nonce or shared_secret has the wrong kind or length, or if
        authentication fails (tampered ciphertext, wrong key/nonce, or mismatched associated data);
        VeilidAPIErrorInternal on an internal length conversion failure.
        """
        pass

    @abstractmethod
    async def encrypt_aead(
        self,
        body: bytes,
        nonce: types.Nonce,
        shared_secret: types.SharedSecret,
        associated_data: Optional[bytes],
    ) -> bytes:
        """Encrypt and authenticate body, returning the ciphertext with appended tag.

        associated_data is authenticated but not encrypted, and must be supplied again at
        decryption. Never reuse a nonce with the same shared secret.

        Raises VeilidAPIErrorGeneric if nonce or shared_secret has the wrong kind or length;
        VeilidAPIErrorInternal on an internal length conversion failure.
        """
        pass

    @abstractmethod
    async def crypt_no_auth(
        self, body: bytes, nonce: types.Nonce, shared_secret: types.SharedSecret
    ) -> bytes:
        """Apply the stream cipher to body without authentication, returning the result.

        The same operation in both directions: re-applying with the same nonce and secret
        reverses it. Provides confidentiality only, no integrity; use the AEAD variants when
        tamper detection is needed.

        Raises VeilidAPIErrorGeneric if nonce or shared_secret has the wrong kind or length;
        VeilidAPIErrorInternal on an internal length conversion failure.
        """
        pass


class DHTTransaction(ABC):
    """Performs multiple simultaneous atomic operations over a set of DHT records.

    Bound operations all succeed or fail together, at the same time. Transactional operations
    require the node to be online and will error with TryAgain if offline. A transaction must be
    committed when its operations are registered, or rolled back to cancel them.

    Holds a server-side handle; commit or rollback it (or use `async with`), else dropping it
    asserts.
    """

    ref_count: int

    def __init__(
        self,
    ):
        self.ref_count = 0

    async def __aenter__(self) -> Self:
        self.ref_count += 1
        return self

    async def __aexit__(self, *excinfo):
        self.ref_count -= 1
        if self.ref_count == 0 and not self.is_done():
            await self.rollback()

    @abstractmethod
    def is_done(self) -> bool:
        """Check whether this transaction has been committed or rolled back."""
        pass

    @abstractmethod
    async def commit(self):
        """Commit the transaction. All write operations are performed atomically.

        Consumes the transaction; raises if already committed or rolled back. Blocks on the network.

        Raises VeilidAPIErrorTransactionNotFound if the transaction was already committed, rolled
        back, or is unknown, and VeilidAPIErrorTryAgain (retryable) if the node is offline or the
        end/commit barriers could not reach consensus.
        """
        pass

    @abstractmethod
    async def rollback(self):
        """Roll back the transaction. No write operations are performed.

        Consumes the transaction; raises if already committed or rolled back.
        """
        pass

    @abstractmethod
    async def extend(
        self,
        record_keys: list[types.RecordKey],
        options: Optional[types.TransactDHTRecordsOptions] = None,
    ):
        """Extend the transaction with additional record keys.

        Raises VeilidAPIErrorTransactionNotFound if the transaction handle is already completed or
        unknown, VeilidAPIErrorMissingArgument if record_keys contains duplicates,
        VeilidAPIErrorInvalidArgument if the merged record set would exceed the per-transaction
        record limit, and VeilidAPIErrorTryAgain (retryable) if the node is offline or the begin
        fanout for the added records could not reach consensus.
        """
        pass

    @abstractmethod
    async def get(
        self, key: types.RecordKey, subkey: types.ValueSubkey
    ) -> Optional[types.ValueData]:
        """Perform a get inside the transaction.

        Fails offline, fails if the local value is newer, and fails if offline writes exist for
        the record. Returns None if the subkey has not been set, or its ValueData if it has.

        Raises VeilidAPIErrorTransactionNotFound if the transaction handle is already completed or no
        longer in the Begin stage, VeilidAPIErrorGeneric if key is an unsupported kind or malformed,
        VeilidAPIErrorInvalidArgument if key is not in the transaction or subkey is outside the schema
        range, and VeilidAPIErrorTryAgain (retryable) if the node is offline or the network did not
        return the value that existed at begin time. A non-responding node is retried rather than
        surfaced as a timeout.
        """
        pass

    @abstractmethod
    async def set(
        self, key: types.RecordKey, subkey: types.ValueSubkey, data: bytes, options: Optional[types.DHTTransactionSetValueOptions] = None
    ) -> Optional[types.ValueData]:
        """Add a set operation to the transaction.

        Fails offline and fails if offline writes exist for the record. The writer in options, if
        given, overrides the default writer from open. Returns None on success, or the network's
        ValueData if the value set was older than what is on the network.

        Raises VeilidAPIErrorTransactionNotFound if the transaction handle is already completed or no
        longer in the Begin stage, VeilidAPIErrorInvalidArgument if key is not open in the transaction
        or subkey is outside the schema range, VeilidAPIErrorGeneric if key is an unsupported kind or
        malformed or the subkey has no writer, and VeilidAPIErrorTryAgain (retryable) if the node is
        offline or write consensus was not reached this round. A non-responding node is retried rather
        than surfaced as a timeout.
        """
        pass

    @abstractmethod
    async def inspect(
        self,
        key: types.RecordKey,
        subkeys: list[tuple[types.ValueSubkey, types.ValueSubkey]],
        scope: types.DHTReportScope = types.DHTReportScope.LOCAL,
    ) -> types.DHTRecordReport:
        """Perform an inspect inside the transaction, using state captured at begin without network activity.

        Returns a report with the subkey ranges overlapping the schema and their sequence numbers.

        Raises VeilidAPIErrorTransactionNotFound if the transaction handle is already completed,
        unknown, or no longer in the Begin stage, VeilidAPIErrorInvalidArgument if key is not in the
        transaction, and VeilidAPIErrorGeneric if key is an unsupported kind or malformed or the
        transaction has not started. Performs no network activity and cannot time out.
        """
        pass


class VeilidAPI(ABC):
    """The primary entrypoint into veilid-core functionality.

    From here one can access the node state and configuration, attach and detach from the
    network, obtain routing contexts and cryptosystems, open table stores, create and import
    private routes, and reply to AppCall RPCs.

    Owns the server connection and receive task; release it (or use `async with`), else the
    connection and task leak.
    """

    ref_count: int

    def __init__(
        self,
    ):
        self.ref_count = 0

    async def __aenter__(self) -> Self:
        self.ref_count += 1
        return self

    async def __aexit__(self, *excinfo):
        self.ref_count -= 1
        if self.ref_count == 0 and not self.is_done():
            await self.release()

    @abstractmethod
    def is_done(self) -> bool:
        """Check whether this API handle has been released."""
        pass

    @abstractmethod
    async def release(self):
        """Release this API handle and free its resources.

        Idempotent: a no-op if already released. Cancels the receive task and closes the connection.
        """
        pass

    @abstractmethod
    async def control(self, args: list[str]) -> str:
        """Send a control command to the API server and return its response.

        Raises VeilidAPIErrorGeneric if no control request is given, the request is unknown, or it
        has the wrong number of arguments; VeilidAPIErrorInvalidArgument for an invalid schema name.
        """
        pass

    @abstractmethod
    async def get_state(self) -> VeilidState:
        """Get a full copy of the current state of Veilid.

        Raises VeilidAPIErrorNotInitialized if the node is shut down.
        """
        pass

    @abstractmethod
    async def is_shutdown(self) -> bool:
        """Check whether Veilid is already shut down."""
        pass

    @abstractmethod
    async def attach(self):
        """Connect to the network.

        Raises VeilidAPIErrorGeneric if already attached, or VeilidAPIErrorNotInitialized if the
        node is shut down.
        """
        pass

    @abstractmethod
    async def detach(self):
        """Disconnect from the network.

        Raises VeilidAPIErrorGeneric if already detached, or VeilidAPIErrorNotInitialized if the
        node is shut down.
        """
        pass

    @abstractmethod
    async def new_private_route(self) -> types.RouteBlob:
        """Allocate a new private route set with default cryptography and network options.

        Defaults to reliable stability and prefer-ordered sequencing. Returns a route id and a
        publishable blob encrypted with each crypto kind, letting importers choose which to use.

        The route id must be freed with release_private_route. Blocks on route allocation.

        Raises VeilidAPIErrorTryAgain (retryable) if there is no valid PublicInternet network class
        yet, not enough nodes are known to build the route, or the route failed its reachability
        test; or VeilidAPIErrorNotInitialized if the node is shut down.
        """
        pass

    @abstractmethod
    async def new_custom_private_route(
        self,
        private_spec: types.PrivateSpec,
    ) -> types.RouteBlob:
        """Allocate a new private route with a specific cryptosystem, stability, and sequencing preference.

        Low-latency stability and prefer-unordered sequencing may be faster at the cost of some
        lost messages. Returns a route id and a publishable blob encrypted with each crypto kind.

        The route id must be freed with release_private_route. Blocks on route allocation.

        Raises VeilidAPIErrorGeneric if the spec names an invalid crypto kind,
        VeilidAPIErrorInvalidArgument if the hop count exceeds the configured maximum,
        VeilidAPIErrorTryAgain (retryable) if there is no valid PublicInternet network class yet,
        not enough nodes are known to build the route, or the route failed its reachability test; or
        VeilidAPIErrorNotInitialized if the node is shut down.
        """
        pass

    @abstractmethod
    async def import_remote_private_route(self, blob: bytes) -> types.RouteId:
        """Import a private route blob as a remote private route.

        Returns a route id usable to send private messages to the node that created the route.
        The route id must be freed with release_private_route.

        Raises VeilidAPIErrorInvalidArgument if blob is empty or names too many crypto kinds,
        VeilidAPIErrorParseError if it is malformed, VeilidAPIErrorGeneric if the decoded route has
        no first hop, or VeilidAPIErrorNotInitialized if the node is shut down.
        """
        pass

    @abstractmethod
    async def release_private_route(self, route_id: types.RouteId):
        """Release a locally allocated or remotely imported private route, freeing its resources.

        The release/free half for new_private_route, new_custom_private_route, and
        import_remote_private_route.

        Raises VeilidAPIErrorInvalidArgument if route_id is unknown, already released, or malformed
        (unsupported crypto kind or bad length), or VeilidAPIErrorNotInitialized if the node is shut
        down.
        """
        pass

    @abstractmethod
    async def app_call_reply(self, call_id: types.OperationId, message: bytes):
        """Respond to an AppCall received over a VeilidUpdate.

        call_id is the id of the call to reply to, from the AppCall update. message is the answer
        blob returned by the remote node's app_call, up to 32768 bytes.

        Each call_id may be answered only once; replying to an unknown or already-answered call_id
        raises VeilidAPIErrorGeneric. Raises VeilidAPIErrorTryAgain if the node is mid-shutdown, or
        VeilidAPIErrorNotInitialized if the node is shut down.
        """
        pass

    @abstractmethod
    async def new_routing_context(self) -> RoutingContext:
        """Get a new routing context with default safety, sequencing, and stability parameters.

        Returns a handle the caller must release (or use `async with`), else dropping it asserts.

        Raises VeilidAPIErrorNotInitialized if the node is shut down.
        """
        pass

    @abstractmethod
    async def open_table_db(self, name: str, column_count: int) -> TableDb:
        """Open or create an encrypted key-value table with the given name and column count.

        Returns a handle the caller must release (or use `async with`), else dropping it asserts.

        Raises VeilidAPIErrorInvalidArgument if column_count is zero or name contains characters
        other than alphanumeric, underscore, or hyphen; VeilidAPIErrorNotInitialized before init or
        after shutdown, or when existing data fails to decrypt (wrong device key) and wiping is
        disabled; VeilidAPIErrorGeneric if the table is already open with a smaller column count
        (close it first) or the backing-store open fails.
        """
        pass

    @abstractmethod
    async def delete_table_db(self, name: str) -> bool:
        """Delete a table store by name. Returns True if a table was deleted.

        Returns False if no table by that name exists. Raises VeilidAPIErrorNotInitialized before
        init or after shutdown, VeilidAPIErrorInvalidArgument if name contains characters other than
        alphanumeric, underscore, or hyphen, and VeilidAPIErrorGeneric if the table is still opened
        (drop all handles first) or the backing-store delete fails.
        """
        pass

    @abstractmethod
    async def get_crypto_system(self, kind: types.CryptoKind) -> CryptoSystem:
        """Get the cryptosystem for the given crypto kind.

        Returns a handle the caller must release (or use `async with`), else dropping it asserts.

        Raises VeilidAPIErrorInvalidArgument if kind is an unsupported cryptosystem, or
        VeilidAPIErrorNotInitialized if the node is shut down.
        """
        pass

    @abstractmethod
    async def verify_signatures(
        self,
        node_ids: list[types.PublicKey],
        data: bytes,
        signatures: list[types.Signature],
    ) -> Optional[list[types.PublicKey]]:
        """Verify a set of signatures over data.

        Returns the public keys whose signatures validated and are supported, or None if any
        supported crypto kind fails to validate.

        A supported signature that does not match returns None, not an error. Raises
        VeilidAPIErrorGeneric, VeilidAPIErrorParseError, or VeilidAPIErrorInternal if a matching
        public key or signature is malformed, or VeilidAPIErrorNotInitialized if the node is shut
        down.
        """
        pass

    @abstractmethod
    async def generate_signatures(
        self, data: bytes, key_pairs: list[types.KeyPair]
    ) -> list[types.Signature]:
        """Generate signatures over data for the supported key pairs. Unsupported crypto kinds are silently dropped.

        Raises VeilidAPIErrorGeneric, VeilidAPIErrorParseError, or VeilidAPIErrorInternal if a
        supported keypair is malformed, or VeilidAPIErrorNotInitialized if the node is shut down.
        """
        pass

    @abstractmethod
    async def generate_key_pair(self, kind: types.CryptoKind) -> list[types.KeyPair]:
        """Generate a signing key pair for the given crypto kind. Does not require startup.

        Raises VeilidAPIErrorGeneric if kind is not a supported cryptosystem.
        """
        pass

    @abstractmethod
    async def generate_member_id(self, writer_key: types.PublicKey) -> types.MemberId:
        """Create a new member id from a writer public key, for use in building SMPL schemas.

        Raises VeilidAPIErrorGeneric if writer_key names an unsupported crypto kind, or
        VeilidAPIErrorNotInitialized if the node is shut down.
        """
        pass

    @abstractmethod
    async def get_dht_record_key(
        self, schema: types.DHTSchema, owner: types.PublicKey, encryption_key: Optional[types.SharedSecret]) -> types.RecordKey:
        """Deterministically build the record key for a schema and owner public key. The key's crypto kind is that of owner.

        Raises VeilidAPIErrorInvalidArgument if schema is malformed, VeilidAPIErrorGeneric if owner or
        encryption_key names an unsupported crypto kind or has the wrong length, or
        VeilidAPIErrorNotInitialized if the node is shut down.
        """
        pass

    @abstractmethod
    async def transact_dht_records(
        self, record_keys: list[types.RecordKey], options: Optional[types.TransactDHTRecordsOptions]) -> DHTTransaction:
        """Start a transaction over a set of DHT records.

        Record keys must already be opened via a routing context. At most 32 records per
        transaction. options may supply a default signing keypair for records not opened for
        writing.

        Returns a handle the caller must commit or rollback (or use `async with`), else dropping
        it asserts.

        Raises VeilidAPIErrorInvalidArgument if a record is not open or more than 32 records are
        passed, VeilidAPIErrorMissingArgument if record_keys is empty or has duplicates,
        VeilidAPIErrorGeneric if a record key is an unsupported kind or malformed,
        VeilidAPIErrorTryAgain (retryable) if the DHT is offline, the records are contended, or begin
        consensus was not reached, and VeilidAPIErrorNotInitialized if the node is shut down. Network
        failures surface as VeilidAPIErrorTimeout or VeilidAPIErrorNoConnection.
        """
        pass

    @abstractmethod
    async def now(self) -> types.Timestamp:
        """Current wall-clock time. May move backward if the system clock is adjusted."""
        pass

    @abstractmethod
    async def now_non_decreasing(self) -> types.Timestamp:
        """Current time, clamped to never read earlier than a previous call. Repeated values are allowed."""
        pass

    @abstractmethod
    async def now_increasing(self) -> types.Timestamp:
        """Current time, clamped to always read at least 1us later than a previous call. Never repeats a value."""
        pass

    @abstractmethod
    async def debug(self, command: str) -> str:
        """Run a debug command and return its output.

        Raises VeilidAPIErrorParseError if the command cannot be split into words,
        VeilidAPIErrorGeneric for an unknown command or subcommand, and
        VeilidAPIErrorInvalidArgument for a bad argument to a known command; individual subcommands
        propagate their own VeilidAPIError.
        """
        pass

    @abstractmethod
    async def veilid_version_string(self) -> str:
        """Return the veilid-core package version as a string."""
        pass

    @abstractmethod
    async def veilid_features(self) -> list[str]:
        """Return the features that were enabled when veilid-core was built."""
        pass

    @abstractmethod
    async def veilid_version(self) -> types.VeilidVersion:
        """Return the veilid-core package version as major, minor, and patch components."""
        pass

    @abstractmethod
    async def default_veilid_config(self) -> str:
        """Return the default Veilid configuration as a JSON string."""
        pass

    @abstractmethod
    async def valid_crypto_kinds(self) -> list[types.CryptoKind]:
        """Return the crypto kinds supported by this build."""
        pass
