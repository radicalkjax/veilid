import 'dart:async';
import 'dart:convert';
import 'dart:typed_data';

import 'package:charcode/charcode.dart';
import 'package:equatable/equatable.dart';
import 'package:freezed_annotation/freezed_annotation.dart';

import 'veilid.dart';

//////////////////////////////////////
/// CryptoKind

/// A four-character code identifying a cryptosystem, held as a big-endian
/// `uint32`. Keys, signatures, nonces, and digests carry their kind and are
/// only accepted by the matching cryptosystem.
@immutable
class CryptoKind extends Equatable {
  /// The fourcc packed into a big-endian `uint32`.
  final int kind;

  /// Wraps a fourcc already packed into a big-endian `uint32`.
  const CryptoKind.fromInt(this.kind);

  /// Reads the fourcc from the first four bytes of [b], big-endian.
  // Allow const
  // ignore: prefer_constructors_over_static_methods
  static CryptoKind fromBytes(Uint8List b) =>
      CryptoKind.fromInt(ByteData.sublistView(b).getUint32(0));

  /// Reads the fourcc from a four-character string. Throws [FormatException]
  /// if [s] is not exactly four characters.
  // Allow const
  // ignore: prefer_constructors_over_static_methods
  static CryptoKind fromString(String s) {
    if (s.codeUnits.length != 4) {
      throw const FormatException('malformed string');
    }
    return CryptoKind.fromInt(
      ByteData.sublistView(Uint8List.fromList(s.codeUnits)).getUint32(0),
    );
  }

  @override
  String toString() {
    final b = Uint8List(4);
    ByteData.sublistView(b).setUint32(0, kind);
    return b.map(String.fromCharCode).join();
  }

  /// The fourcc as a big-endian `uint32`.
  int toInt() => kind;

  /// The four bytes of the fourcc, big-endian.
  Uint8List toBytes() {
    final b = Uint8List(4);
    ByteData.sublistView(b).setUint32(0, kind);
    return b;
  }

  @override
  List<Object?> get props => [kind];
}

/// Decodes a list of [CryptoKind] from a JSON array of integer fourcc codes.
List<CryptoKind> cryptoKindsFromJson(dynamic json) =>
    (json as List<dynamic>).map((x) => CryptoKind.fromInt(x as int)).toList();

/// Encodes a list of [CryptoKind] as a JSON array of integer fourcc codes.
List<int> cryptoKindsToJson(List<CryptoKind> x) =>
    x.map((x) => x.toInt()).toList();

/// The VLD0 crypto kind, the current cryptosystem implementation.
const cryptoKindVLD0 = CryptoKind.fromInt(
  $V << 24 | $L << 16 | $D << 8 | $0 << 0,
); // "VLD0"
// final cryptoKindVLD1 = CryptoKind.fromInt(
//     $V << 24 | $L << 16 | $D << 8 | $1 << 0); // "VLD1"
/// The NONE crypto kind, a passthrough cryptosystem with no security.
const cryptoKindNONE = CryptoKind.fromInt(
  $N << 24 | $O << 16 | $N << 8 | $E << 0,
); // "NONE"

/// The preferred crypto kind to use when generating new keys.
const bestCryptoKind = cryptoKindVLD0;

//////////////////////////////////////
/// Types

/// A value tagged with the [CryptoKind] of the cryptosystem it belongs to.
abstract interface class TypedCryptoKey<V> {
  /// The cryptosystem this value belongs to.
  CryptoKind get kind;

  /// The untagged value.
  V get value;
}

/// A bare cryptographic value tagged with its [CryptoKind]. Parses and displays
/// as `kind:value`.
@immutable
class Typed<V extends EncodedString> extends Equatable
    implements TypedCryptoKey<V> {
  @override
  final CryptoKind kind;
  @override
  final V value;

  /// Pairs a value with the [CryptoKind] it belongs to.
  const Typed({required this.kind, required this.value});

  /// Parses a `kind:value` string. Throws [FormatException] if malformed.
  factory Typed.fromString(String s) {
    final parts = s.split(':');
    if (parts.length < 2 || parts[0].codeUnits.length != 4) {
      throw const FormatException('malformed string');
    }
    final kind = CryptoKind.fromString(parts[0]);
    final value = EncodedString.fromString<V>(parts.sublist(1).join(':'));
    return Typed(kind: kind, value: value);
  }

  /// Decodes from the kind fourcc followed by the value bytes.
  factory Typed.fromBytes(Uint8List b) {
    final kind = CryptoKind.fromBytes(b);
    final value = EncodedString.fromBytes<V>(b.sublist(4));
    return Typed(kind: kind, value: value);
  }

  /// Decodes from a JSON `kind:value` string.
  factory Typed.fromJson(dynamic json) => Typed.fromString(json as String);

  @override
  List<Object> get props => [kind, value];

  @override
  String toString() => '$kind:$value';

  /// The kind fourcc followed by the value bytes.
  Uint8List toBytes() {
    final b = BytesBuilder()
      ..add(kind.toBytes())
      ..add(value.toBytes());
    return b.toBytes();
  }

  /// The `kind:value` string form.
  String toJson() => toString();
}

/// Untyped public/secret key pair, carrying no cryptosystem kind. Encodes as
/// `<public>:<secret>`.
@immutable
class BareKeyPair extends Equatable {
  /// The public key.
  final BarePublicKey key;

  /// The secret key.
  final BareSecretKey secret;

  /// Builds a key pair from a public key and its secret key.
  const BareKeyPair({required this.key, required this.secret});

  /// Decodes a pair from a `<public>:<secret>` string. Throws [FormatException]
  /// if malformed.
  factory BareKeyPair.fromString(String s) {
    final parts = s.split(':');
    if (parts.length != 2) {
      throw const FormatException('malformed string');
    }
    final key = BarePublicKey.fromString(parts[0]);
    final secret = BareSecretKey.fromString(parts[1]);
    return BareKeyPair(key: key, secret: secret);
  }

  /// Decodes a pair from a JSON `<public>:<secret>` string.
  factory BareKeyPair.fromJson(dynamic json) =>
      BareKeyPair.fromString(json as String);

  @override
  List<Object> get props => [key, secret];

  @override
  String toString() => '$key:$secret';

  /// The `<public>:<secret>` string form.
  String toJson() => toString();
}

/// A public/secret key pair tagged with its [CryptoKind]. Encodes as
/// `kind:public:secret`.
@immutable
class KeyPair extends Equatable implements TypedCryptoKey<BareKeyPair> {
  /// The kinded public key.
  final PublicKey key;

  /// The kinded secret key.
  final SecretKey secret;

  /// Builds a key pair from a kinded public and secret key, which must share a kind.
  KeyPair({required this.key, required this.secret})
    : assert(key.kind == secret.kind, 'keypair parts must have same kind');

  /// Parses a `kind:public:secret` string. Throws
  /// [VeilidAPIExceptionInvalidArgument] if malformed.
  factory KeyPair.fromString(String s) {
    final parts = s.split(':');
    if (parts.length != 3 || parts[0].codeUnits.length != 4) {
      throw VeilidAPIExceptionInvalidArgument('malformed string', 's', s);
    }
    final kind = CryptoKind.fromString(parts[0]);
    final key = PublicKey(
      kind: kind,
      value: BarePublicKey.fromString(parts[1]),
    );
    final secret = SecretKey(
      kind: kind,
      value: BareSecretKey.fromString(parts[2]),
    );
    return KeyPair(key: key, secret: secret);
  }

  /// Parses a JSON `kind:public:secret` string.
  factory KeyPair.fromJson(dynamic json) => KeyPair.fromString(json as String);

  /// Tags a [BareKeyPair] with [kind].
  factory KeyPair.fromBareKeyPair(CryptoKind kind, BareKeyPair keyPair) =>
      KeyPair(
        key: PublicKey(kind: kind, value: keyPair.key),
        secret: SecretKey(kind: kind, value: keyPair.secret),
      );

  /// Pairs a kinded public key with a bare secret key, taking the kind from [key].
  factory KeyPair.fromPublicAndBareSecret(
    PublicKey key,
    BareSecretKey secret,
  ) => KeyPair(
    key: key,
    secret: SecretKey(kind: key.kind, value: secret),
  );

  @override
  CryptoKind get kind => key.kind;

  @override
  BareKeyPair get value => BareKeyPair(key: key.value, secret: secret.value);

  @override
  List<Object> get props => [key, secret];

  @override
  String toString() => '${key.kind}:${key.value}:${secret.value}';

  /// The `kind:public:secret` string form.
  String toJson() => toString();

  /// Drops the kind, returning the untyped pair.
  BareKeyPair toBareKeyPair() =>
      BareKeyPair(key: key.value, secret: secret.value);
}

/// Untyped KEM encapsulation/decapsulation key pair, carrying no cryptosystem
/// kind. Encodes as `<encapsulation>:<decapsulation>`.
@immutable
class BareKemKeyPair extends Equatable {
  /// The encapsulation key.
  final BareEncapsulationKey key;

  /// The decapsulation key.
  final BareDecapsulationKey secret;

  /// Builds a KEM key pair from an encapsulation key and its decapsulation key.
  const BareKemKeyPair({required this.key, required this.secret});

  /// Decodes a pair from an `<encapsulation>:<decapsulation>` string. Throws
  /// [FormatException] if malformed.
  factory BareKemKeyPair.fromString(String s) {
    final parts = s.split(':');
    if (parts.length != 2) {
      throw const FormatException('malformed string');
    }
    final key = BareEncapsulationKey.fromString(parts[0]);
    final secret = BareDecapsulationKey.fromString(parts[1]);
    return BareKemKeyPair(key: key, secret: secret);
  }

  /// Decodes a pair from a JSON `<encapsulation>:<decapsulation>` string.
  factory BareKemKeyPair.fromJson(dynamic json) =>
      BareKemKeyPair.fromString(json as String);

  @override
  List<Object> get props => [key, secret];

  @override
  String toString() => '$key:$secret';

  /// The `<encapsulation>:<decapsulation>` string form.
  String toJson() => toString();
}

/// A KEM key pair tagged with its [CryptoKind], for HPKE seal/open. Signing
/// key pairs use [KeyPair]. Encodes as `kind:encapsulation:decapsulation`.
@immutable
class KemKeyPair extends Equatable implements TypedCryptoKey<BareKemKeyPair> {
  /// The kinded encapsulation key.
  final EncapsulationKey key;

  /// The kinded decapsulation key.
  final DecapsulationKey secret;

  /// Builds a KEM key pair from kinded keys, which must share a kind.
  KemKeyPair({required this.key, required this.secret})
    : assert(key.kind == secret.kind, 'keypair parts must have same kind');

  /// Parses a `kind:encapsulation:decapsulation` string. Throws
  /// [VeilidAPIExceptionInvalidArgument] if malformed.
  factory KemKeyPair.fromString(String s) {
    final parts = s.split(':');
    if (parts.length != 3 || parts[0].codeUnits.length != 4) {
      throw VeilidAPIExceptionInvalidArgument('malformed string', 's', s);
    }
    final kind = CryptoKind.fromString(parts[0]);
    final key = EncapsulationKey(
      kind: kind,
      value: BareEncapsulationKey.fromString(parts[1]),
    );
    final secret = DecapsulationKey(
      kind: kind,
      value: BareDecapsulationKey.fromString(parts[2]),
    );
    return KemKeyPair(key: key, secret: secret);
  }

  /// Parses a JSON `kind:encapsulation:decapsulation` string.
  factory KemKeyPair.fromJson(dynamic json) =>
      KemKeyPair.fromString(json as String);

  /// Tags a [BareKemKeyPair] with [kind].
  factory KemKeyPair.fromBareKemKeyPair(
    CryptoKind kind,
    BareKemKeyPair keyPair,
  ) => KemKeyPair(
    key: EncapsulationKey(kind: kind, value: keyPair.key),
    secret: DecapsulationKey(kind: kind, value: keyPair.secret),
  );

  @override
  CryptoKind get kind => key.kind;

  @override
  BareKemKeyPair get value =>
      BareKemKeyPair(key: key.value, secret: secret.value);

  @override
  List<Object> get props => [key, secret];

  @override
  String toString() => '${key.kind}:${key.value}:${secret.value}';

  /// The `kind:encapsulation:decapsulation` string form.
  String toJson() => toString();

  /// Drops the kind, returning the untyped pair.
  BareKemKeyPair toBareKemKeyPair() =>
      BareKemKeyPair(key: key.value, secret: secret.value);
}

/// Untyped DHT record key: an opaque record key with an optional record
/// encryption secret. Encodes as `<key>` or `<key>:<encryptionKey>`.
@immutable
class BareRecordKey extends Equatable {
  /// The opaque record key.
  final BareOpaqueRecordKey key;

  /// The record encryption secret, if present.
  final BareSharedSecret? encryptionKey;

  /// Builds a record key from an opaque record key and an optional encryption secret.
  const BareRecordKey({required this.key, required this.encryptionKey});

  /// Decodes from a `<key>` or `<key>:<encryptionKey>` string. Throws
  /// [FormatException] if malformed.
  factory BareRecordKey.fromString(String s) {
    final parts = s.split(':');
    if (parts.length > 2 || parts.isEmpty) {
      throw const FormatException('malformed string');
    }
    if (parts.length == 2) {
      final key = BareOpaqueRecordKey.fromString(parts[0]);
      final encryptionKey = BareSharedSecret.fromString(parts[1]);
      return BareRecordKey(key: key, encryptionKey: encryptionKey);
    }
    final key = BareOpaqueRecordKey.fromString(parts[0]);
    return BareRecordKey(key: key, encryptionKey: null);
  }

  /// Decodes from a JSON `<key>` or `<key>:<encryptionKey>` string.
  factory BareRecordKey.fromJson(dynamic json) =>
      BareRecordKey.fromString(json as String);

  @override
  List<Object?> get props => [key, encryptionKey];

  @override
  String toString() => encryptionKey != null ? '$key:$encryptionKey' : '$key';

  /// The `<key>` or `<key>:<encryptionKey>` string form.
  String toJson() => toString();
}

/// A DHT record key tagged with its [CryptoKind]: an opaque record key with an
/// optional encryption secret. Encodes as `kind:opaque` or
/// `kind:opaque:encryptionKey`.
@immutable
class RecordKey extends Equatable implements TypedCryptoKey<BareRecordKey> {
  /// The kinded opaque record key.
  final OpaqueRecordKey opaque;

  /// The kinded record encryption secret, if present.
  final SharedSecret? encryptionKey;

  /// Builds a record key from an opaque record key and optional encryption secret,
  /// which must share a kind.
  RecordKey({required this.opaque, required this.encryptionKey})
    : assert(
        encryptionKey == null || opaque.kind == encryptionKey.kind,
        'recordkey parts must have same kind',
      );

  /// Parses a `kind:opaque` or `kind:opaque:encryptionKey` string. Throws
  /// [VeilidAPIExceptionInvalidArgument] if malformed.
  factory RecordKey.fromString(String s) {
    final parts = s.split(':');
    if (parts.length < 2 ||
        parts.length > 3 ||
        parts[0].codeUnits.length != 4) {
      throw VeilidAPIExceptionInvalidArgument('malformed string', 's', s);
    }
    final kind = CryptoKind.fromString(parts[0]);
    final key = OpaqueRecordKey(
      kind: kind,
      value: BareOpaqueRecordKey.fromString(parts[1]),
    );
    if (parts.length == 3) {
      final encryptionKey = SharedSecret(
        kind: kind,
        value: BareSharedSecret.fromString(parts[2]),
      );
      return RecordKey(opaque: key, encryptionKey: encryptionKey);
    }
    return RecordKey(opaque: key, encryptionKey: null);
  }

  /// Parses a JSON `kind:opaque` or `kind:opaque:encryptionKey` string.
  factory RecordKey.fromJson(dynamic json) =>
      RecordKey.fromString(json as String);

  /// Tags a [BareRecordKey] with [kind].
  factory RecordKey.fromBareRecordKey(
    CryptoKind kind,
    BareRecordKey bareRecordKey,
  ) => RecordKey(
    opaque: OpaqueRecordKey(kind: kind, value: bareRecordKey.key),
    encryptionKey: bareRecordKey.encryptionKey == null
        ? null
        : SharedSecret(kind: kind, value: bareRecordKey.encryptionKey!),
  );

  /// Decodes from a length-prefixed opaque key followed by an optional
  /// encryption secret, taking the kind from the opaque key bytes.
  factory RecordKey.fromBytes(Uint8List bytes) {
    final keyLength = ByteData.sublistView(bytes).getUint8(0);
    final keyBytes = bytes.sublist(1, 1 + keyLength);
    final key = OpaqueRecordKey.fromBytes(keyBytes);
    SharedSecret? encryptionKey;
    if (bytes.length > 1 + keyLength) {
      final ekBytes = bytes.sublist(1 + keyLength, bytes.length);
      encryptionKey = SharedSecret(
        kind: key.kind,
        value: BareSharedSecret.fromBytes(ekBytes),
      );
    }
    return RecordKey(opaque: key, encryptionKey: encryptionKey);
  }

  @override
  List<Object?> get props => [opaque, encryptionKey];

  @override
  CryptoKind get kind => opaque.kind;

  @override
  BareRecordKey get value =>
      BareRecordKey(key: opaque.value, encryptionKey: encryptionKey?.value);

  @override
  String toString() => encryptionKey != null
      ? '${opaque.kind}:${opaque.value}:${encryptionKey!.value}'
      : '${opaque.kind}:${opaque.value}';

  /// The `kind:opaque` or `kind:opaque:encryptionKey` string form.
  String toJson() => toString();

  /// A length-prefixed opaque key followed by the optional encryption secret.
  Uint8List toBytes() {
    final keyBytes = opaque.toBytes();
    final b = BytesBuilder()
      ..addByte(keyBytes.lengthInBytes)
      ..add(keyBytes);
    final ek = encryptionKey;
    if (ek != null) {
      b.add(ek.value.toBytes());
    }
    return b.toBytes();
  }
}

/// A signing public key tagged with its [CryptoKind]: verification, identity,
/// and node ids. KEM encryption to a key uses [EncapsulationKey].
typedef PublicKey = Typed<BarePublicKey>;

/// A signature tagged with its [CryptoKind].
typedef Signature = Typed<BareSignature>;

/// A signing secret key tagged with its [CryptoKind]. KEM decryption uses
/// [DecapsulationKey].
typedef SecretKey = Typed<BareSecretKey>;

/// A KEM encapsulation key tagged with its [CryptoKind], sealed to with
/// [VeilidCryptoSystem.hpkeSeal]. Signing uses [PublicKey].
typedef EncapsulationKey = Typed<BareEncapsulationKey>;

/// A KEM decapsulation key tagged with its [CryptoKind], opening blobs with
/// [VeilidCryptoSystem.hpkeOpen]. Signing uses [SecretKey].
typedef DecapsulationKey = Typed<BareDecapsulationKey>;

/// A hash digest tagged with its [CryptoKind].
typedef HashDigest = Typed<BareHashDigest>;

/// A shared secret tagged with its [CryptoKind].
typedef SharedSecret = Typed<BareSharedSecret>;

/// A private route id tagged with its [CryptoKind].
typedef RouteId = Typed<BareRouteId>;

/// A node id tagged with its [CryptoKind].
typedef NodeId = Typed<BareNodeId>;

/// A member id tagged with its [CryptoKind].
typedef MemberId = Typed<BareMemberId>;

/// An opaque DHT record key tagged with its [CryptoKind].
typedef OpaqueRecordKey = Typed<BareOpaqueRecordKey>;

//////////////////////////////////////
/// VeilidCryptoSystem

/// The set of cryptographic primitives a single cryptosystem provides: key
/// generation, signing and verification, AEAD and unauthenticated encryption,
/// Diffie-Hellman key exchange and shared secret derivation, hashing, password
/// hashing, and random byte generation.
///
/// Each implementation is tagged by a [CryptoKind] fourcc; keys, signatures,
/// nonces, and digests carry that kind and are only accepted by the matching
/// cryptosystem.
abstract class VeilidCryptoSystem {
  /// The [CryptoKind] fourcc identifying this cryptosystem.
  CryptoKind kind();

  // Cached Operations

  /// Diffie-Hellman shared secret for the given public/secret key pair, memoized
  /// in the DH cache to avoid recomputing the same exchange. See [computeDH].
  ///
  /// Throws [VeilidAPIExceptionGeneric] if [key] or [secret] has the wrong kind
  /// or length, or the exchange is non-contributory; [VeilidAPIExceptionInternal]
  /// if the key bytes are otherwise unusable.
  Future<SharedSecret> cachedDH(PublicKey key, SecretKey secret);

  // Generation

  /// Fill a new buffer of [len] bytes from the cryptographic RNG.
  Future<Uint8List> randomBytes(int len);

  /// Hash a password with the given salt, returning a self-describing PHC hash
  /// string suitable for storage and later [verifyPassword].
  ///
  /// Throws [VeilidAPIExceptionGeneric] if [salt] is outside the allowed length
  /// range or the hash computation fails, [VeilidAPIExceptionParseError] if
  /// [salt] cannot be base64-encoded.
  Future<String> hashPassword(Uint8List password, Uint8List salt);

  /// Check a password against a PHC hash string produced by [hashPassword].
  /// Returns false on mismatch; errors only on a malformed hash string.
  ///
  /// Throws [VeilidAPIExceptionParseError] if [passwordHash] is not a valid PHC
  /// hash string.
  Future<bool> verifyPassword(Uint8List password, String passwordHash);

  /// Derive a shared secret from a password and salt via a password-hashing KDF.
  /// Deterministic: the same password and salt always yield the same secret.
  /// Distinct from [generateSharedSecret], which uses key exchange.
  ///
  /// Throws [VeilidAPIExceptionGeneric] if [salt] is outside the allowed length
  /// range or the derivation fails.
  Future<SharedSecret> deriveSharedSecret(Uint8List password, Uint8List salt);

  /// A fresh random nonce of [nonceLength] bytes.
  Future<Nonce> randomNonce();

  /// A fresh random shared secret of [sharedSecretLength] bytes.
  Future<SharedSecret> randomSharedSecret();

  /// Raw Diffie-Hellman shared secret for the given public/secret key pair, with
  /// no caching.
  ///
  /// Throws [VeilidAPIExceptionGeneric] if the exchange is non-contributory,
  /// [VeilidAPIExceptionInternal] if the key bytes are unusable.
  Future<SharedSecret> computeDH(PublicKey key, SecretKey secret);

  /// Derive a domain-separated shared secret from a key exchange: computes the DH
  /// secret, then hashes it together with [domain] and the Veilid API domain tag.
  /// Distinct [domain] values yield independent secrets from the same key pair.
  ///
  /// Throws the same errors as [computeDH] if the key exchange fails.
  Future<SharedSecret> generateSharedSecret(
    PublicKey key,
    SecretKey secret,
    Uint8List domain,
  );

  /// Seal [plaintext] to [recipient] with HPKE base mode (RFC 9180), single-shot.
  /// [aad] is authenticated but not encrypted. Returns a self-describing sealed
  /// blob.
  ///
  /// Sealing is one-way: only the holder of the recipient's [DecapsulationKey]
  /// can open the blob, and the sealer cannot decrypt what it just sealed. This
  /// differs from the DH pattern, where the shared secret let the encrypting
  /// party decrypt its own blobs. A sealer that needs to re-read stored blobs
  /// must also seal them to its own key. Callers who already share a symmetric
  /// key want [encryptAead] instead; HPKE is for encrypting to a recipient's
  /// key when no shared secret exists.
  ///
  /// Throws [VeilidAPIExceptionInvalidArgument] if [recipient] is not a valid
  /// key, [VeilidAPIExceptionGeneric] if encapsulation fails (including a
  /// low-order key).
  Future<Uint8List> hpkeSeal(
    EncapsulationKey recipient,
    Uint8List aad,
    Uint8List plaintext,
  );

  /// Open a sealed blob produced by [hpkeSeal] with the recipient [secret].
  /// [aad] must match what was supplied at seal. Only the recipient can open a
  /// sealed blob; the sealer cannot.
  ///
  /// Throws [VeilidAPIExceptionParseError] if the blob is truncated or its
  /// version is unknown, [VeilidAPIExceptionInvalidArgument] if the blob's kind
  /// is not this cryptosystem's kind or [secret] is not a valid key,
  /// [VeilidAPIExceptionGeneric] if decryption fails.
  Future<Uint8List> hpkeOpen(
    DecapsulationKey secret,
    Uint8List aad,
    Uint8List sealed,
  );

  /// Generate a fresh random signing key pair for this cryptosystem.
  Future<KeyPair> generateKeyPair();

  /// Generate a fresh random KEM key pair for this cryptosystem.
  Future<KemKeyPair> generateKemKeyPair();

  /// Derive the KEM encapsulation key corresponding to a signing public key.
  ///
  /// VLD0-only bridge (ed25519 to x25519): kinds whose signing and KEM keys are
  /// unrelated throw [VeilidAPIExceptionUnimplemented].
  ///
  /// Throws [VeilidAPIExceptionInvalidArgument] if [key] is not a valid signing
  /// public key.
  Future<EncapsulationKey> encapsulationKeyFromSigningKey(PublicKey key);

  /// Derive the KEM decapsulation key corresponding to a signing secret key.
  ///
  /// VLD0-only bridge (ed25519 to x25519): kinds whose signing and KEM keys are
  /// unrelated throw [VeilidAPIExceptionUnimplemented].
  ///
  /// Throws [VeilidAPIExceptionInvalidArgument] if [secret] is not a valid
  /// signing secret key.
  Future<DecapsulationKey> decapsulationKeyFromSigningSecret(SecretKey secret);

  /// Hash a byte slice, returning a digest tagged with this cryptosystem's kind.
  Future<HashDigest> generateHash(Uint8List data);
  //Future<HashDigest> generateHashReader(Stream<List<int>> reader);

  // Validation

  /// Byte length of a shared secret.
  Future<int> sharedSecretLength();

  /// Byte length of a nonce.
  Future<int> nonceLength();

  /// Byte length of a hash digest.
  Future<int> hashDigestLength();

  /// Byte length of a public key.
  Future<int> publicKeyLength();

  /// Byte length of a secret key.
  Future<int> secretKeyLength();

  /// Byte length of a KEM encapsulation key.
  Future<int> encapsulationKeyLength();

  /// Byte length of a KEM decapsulation key.
  Future<int> decapsulationKeyLength();

  /// Byte length of a signature.
  Future<int> signatureLength();

  /// Bytes an AEAD operation adds to the ciphertext (the authentication tag length).
  Future<int> aeadOverhead();

  /// Default salt length in bytes for password hashing and KDF operations.
  Future<int> defaultSaltLength();

  /// Verify a shared secret carries this cryptosystem's kind and the correct length.
  ///
  /// Throws [VeilidAPIExceptionGeneric] if [secret] has the wrong kind or length.
  Future<void> checkSharedSecret(SharedSecret secret);

  /// Verify a nonce has the correct length.
  ///
  /// Throws [VeilidAPIExceptionGeneric] if [nonce] has the wrong length.
  Future<void> checkNonce(Nonce nonce);

  /// Verify a hash digest carries this cryptosystem's kind and the correct length.
  ///
  /// Throws [VeilidAPIExceptionGeneric] if [digest] has the wrong kind or length.
  Future<void> checkHashDigest(HashDigest digest);

  /// Verify a public key carries this cryptosystem's kind and the correct length.
  ///
  /// Throws [VeilidAPIExceptionGeneric] if [key] has the wrong kind or length.
  Future<void> checkPublicKey(PublicKey key);

  /// Verify a secret key carries this cryptosystem's kind and the correct length.
  ///
  /// Throws [VeilidAPIExceptionGeneric] if [key] has the wrong kind or length.
  Future<void> checkSecretKey(SecretKey key);

  /// Verify a signature carries this cryptosystem's kind and the correct length.
  ///
  /// Throws [VeilidAPIExceptionGeneric] if [signature] has the wrong kind or
  /// length.
  Future<void> checkSignature(Signature signature);

  /// Check that a public and secret key form a usable signing pair by signing
  /// test data and verifying it. Returns false if they do not match; errors only
  /// on a malformed key.
  ///
  /// Throws [VeilidAPIExceptionGeneric] if [key] or [secret] has the wrong kind
  /// or length.
  Future<bool> validateKeyPair(PublicKey key, SecretKey secret);

  /// [validateKeyPair] for the parts of [keyPair].
  Future<bool> validateKeyPairWithKeyPair(KeyPair keyPair) =>
      validateKeyPair(keyPair.key, keyPair.secret);

  /// Recompute the hash of [data] and compare it against [hash]. Returns true on match.
  ///
  /// Throws [VeilidAPIExceptionGeneric] if [hash] has the wrong kind or length.
  Future<bool> validateHash(Uint8List data, HashDigest hash);
  //Future<bool> validateHashReader(Stream<List<int>> reader, HashDigest hash);

  // Authentication

  /// Sign [data] with the given key pair, returning a detached signature.
  ///
  /// Throws [VeilidAPIExceptionGeneric] if [key] or [secret] has the wrong kind
  /// or length, [VeilidAPIExceptionParseError] if they do not form a valid pair.
  Future<Signature> sign(PublicKey key, SecretKey secret, Uint8List data);

  /// [sign] with the parts of [keyPair].
  Future<Signature> signWithKeyPair(KeyPair keyPair, Uint8List data) =>
      sign(keyPair.key, keyPair.secret, data);

  /// Verify a detached [signature] over [data] for [key]. Returns true if valid,
  /// false if not; errors only on a malformed key or signature.
  ///
  /// Throws [VeilidAPIExceptionGeneric] if [key] or [signature] has the wrong
  /// kind or length, [VeilidAPIExceptionParseError] if [key] is structurally
  /// invalid.
  Future<bool> verify(PublicKey key, Uint8List data, Signature signature);

  // AEAD Encrypt/Decrypt

  /// Decrypt and authenticate [body], returning the plaintext. [associatedData]
  /// must match what was supplied at encryption. Errors if authentication fails
  /// (tampered ciphertext, wrong key/nonce, or mismatched associated data).
  ///
  /// Throws [VeilidAPIExceptionGeneric] on authentication failure or if [nonce]
  /// or [sharedSecret] has the wrong length.
  Future<Uint8List> decryptAead(
    Uint8List body,
    Nonce nonce,
    SharedSecret sharedSecret,
    Uint8List? associatedData,
  );

  /// Encrypt and authenticate [body], returning the ciphertext with appended tag.
  /// [associatedData] is authenticated but not encrypted, and must be supplied
  /// again at decryption. The same nonce must never be reused with the same
  /// shared secret.
  ///
  /// Throws [VeilidAPIExceptionGeneric] if [nonce] or [sharedSecret] has the
  /// wrong length.
  Future<Uint8List> encryptAead(
    Uint8List body,
    Nonce nonce,
    SharedSecret sharedSecret,
    Uint8List? associatedData,
  );

  /// Apply the stream cipher to [body], without authentication. Same operation
  /// for both directions: re-applying with the same nonce and secret reverses it.
  /// Provides confidentiality only, no integrity; callers needing tamper
  /// detection must use the AEAD variants.
  ///
  /// Throws [VeilidAPIExceptionGeneric] if [nonce] or [sharedSecret] has the
  /// wrong length.
  Future<Uint8List> cryptNoAuth(
    Uint8List body,
    Nonce nonce,
    SharedSecret sharedSecret,
  );

  /// Encrypt [body] with [encryptAead] under a fresh nonce, appending that nonce
  /// to the ciphertext so [decryptAeadWithNonce] can recover it.
  Future<Uint8List> encryptAeadWithNonce(
    Uint8List body,
    SharedSecret secret,
  ) async {
    // generate nonce
    final nonce = await randomNonce();
    // crypt and append nonce
    final b = BytesBuilder()
      ..add(await encryptAead(body, nonce, secret, null))
      ..add(nonce.toBytes());
    return b.toBytes();
  }

  /// Decrypt a [body] produced by [encryptAeadWithNonce], reading the nonce from
  /// its trailing bytes. Throws [FormatException] if [body] is too short, and
  /// [VeilidAPIExceptionGeneric] (from [decryptAead]) on authentication failure.
  Future<Uint8List> decryptAeadWithNonce(
    Uint8List body,
    SharedSecret secret,
  ) async {
    final nlen = await nonceLength();
    if (body.length < nlen) {
      throw const FormatException('not enough data to decrypt');
    }
    final nonce = Nonce.fromBytes(body.sublist(body.length - nlen));
    final encryptedData = body.sublist(0, body.length - nlen);
    // decrypt
    return decryptAead(encryptedData, nonce, secret, null);
  }

  /// Encrypt [body] under a secret derived from [password], appending the
  /// random salt so [decryptAeadWithPassword] can recover it.
  Future<Uint8List> encryptAeadWithPassword(
    Uint8List body,
    String password,
  ) async {
    final ekbytes = Uint8List.fromList(utf8.encode(password));
    final nonce = await randomNonce();
    final saltBytes = nonce.toBytes();
    final sharedSecret = await deriveSharedSecret(ekbytes, saltBytes);
    return Uint8List.fromList(
      (await encryptAead(body, nonce, sharedSecret, null)) + saltBytes,
    );
  }

  /// Decrypt a [body] produced by [encryptAeadWithPassword], deriving the secret
  /// from [password] and the trailing salt. Throws [FormatException] if [body]
  /// is too short, and [VeilidAPIExceptionGeneric] (from [decryptAead]) on
  /// authentication failure, e.g. a wrong [password].
  Future<Uint8List> decryptAeadWithPassword(
    Uint8List body,
    String password,
  ) async {
    final nlen = await nonceLength();
    if (body.length < nlen) {
      throw const FormatException('not enough data to decrypt');
    }
    final ekbytes = Uint8List.fromList(utf8.encode(password));
    final bodyBytes = body.sublist(0, body.length - nlen);
    final saltBytes = body.sublist(body.length - nlen);
    final nonce = Nonce.fromBytes(saltBytes);
    final sharedSecret = await deriveSharedSecret(ekbytes, saltBytes);
    return decryptAead(bodyBytes, nonce, sharedSecret, null);
  }

  // NoAuth Encrypt/Decrypt

  /// Stream-cipher [body] with [cryptNoAuth] under a fresh nonce, appending that
  /// nonce so [decryptNoAuthWithNonce] can recover it. No integrity (see
  /// [cryptNoAuth]).
  Future<Uint8List> encryptNoAuthWithNonce(
    Uint8List body,
    SharedSecret secret,
  ) async {
    // generate nonce
    final nonce = await randomNonce();
    // crypt and append nonce
    final b = BytesBuilder()
      ..add(await cryptNoAuth(body, nonce, secret))
      ..add(nonce.toBytes());
    return b.toBytes();
  }

  /// Decrypt a [body] produced by [encryptNoAuthWithNonce], reading the nonce
  /// from its trailing bytes. Throws [FormatException] if [body] is too short.
  Future<Uint8List> decryptNoAuthWithNonce(
    Uint8List body,
    SharedSecret secret,
  ) async {
    final nlen = await nonceLength();
    if (body.length < nlen) {
      throw const FormatException('not enough data to decrypt');
    }
    final nonce = Nonce.fromBytes(body.sublist(body.length - nlen));
    final encryptedData = body.sublist(0, body.length - nlen);
    // decrypt
    return cryptNoAuth(encryptedData, nonce, secret);
  }

  /// Stream-cipher [body] under a secret derived from [password], appending the
  /// salt so [decryptNoAuthWithPassword] can recover it. No integrity (see
  /// [cryptNoAuth]).
  Future<Uint8List> encryptNoAuthWithPassword(
    Uint8List body,
    String password,
  ) async {
    final ekbytes = Uint8List.fromList(utf8.encode(password));
    final nonce = await randomNonce();
    final saltBytes = nonce.toBytes();
    final sharedSecret = await deriveSharedSecret(ekbytes, saltBytes);
    return Uint8List.fromList(
      (await cryptNoAuth(body, nonce, sharedSecret)) + saltBytes,
    );
  }

  /// Decrypt a [body] produced by [encryptNoAuthWithPassword], deriving the
  /// secret from [password] and the trailing salt. Throws [FormatException] if
  /// [body] is too short.
  Future<Uint8List> decryptNoAuthWithPassword(
    Uint8List body,
    String password,
  ) async {
    final nlen = await nonceLength();
    if (body.length < nlen) {
      throw const FormatException('not enough data to decrypt');
    }
    final ekbytes = Uint8List.fromList(utf8.encode(password));
    final bodyBytes = body.sublist(0, body.length - nlen);
    final saltBytes = body.sublist(body.length - nlen);
    final nonce = Nonce.fromBytes(saltBytes);
    final sharedSecret = await deriveSharedSecret(ekbytes, saltBytes);
    return cryptNoAuth(bodyBytes, nonce, sharedSecret);
  }
}
