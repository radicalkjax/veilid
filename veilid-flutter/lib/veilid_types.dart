import 'dart:async';
import 'dart:typed_data';

import 'package:change_case/change_case.dart';
import 'package:equatable/equatable.dart';
import 'package:freezed_annotation/freezed_annotation.dart';

import 'veilid.dart';

part 'veilid_types.freezed.dart';
part 'veilid_types.g.dart';

//////////////////////////////////////

/// Validation and subkey counting for a [DHTSchemaDFLT].
extension ValidateDFLT on DHTSchemaDFLT {
  /// Returns true if the owner subkey count is within schema limits.
  bool validate() {
    if (oCnt > DHTSchema.maxSubkeyCount) {
      return false;
    }
    if (oCnt <= 0) {
      return false;
    }
    return true;
  }

  /// The total number of subkeys this schema allocates.
  int get subkeyCount => oCnt;
}

/// Validation and subkey counting for a [DHTSchemaSMPL].
extension ValidateSMPL on DHTSchemaSMPL {
  /// Returns true if the subkey, member, and writer counts are within schema
  /// limits.
  bool validate() {
    final totalsv = subkeyCount;
    if (totalsv > DHTSchema.maxSubkeyCount) {
      return false;
    }
    if (totalsv <= 0) {
      return false;
    }

    var writerCount = 0;
    if (oCnt > 0) {
      writerCount += 1;
    }
    final writers = <BareMemberId>{
      ...members.where((m) => m.mCnt > 0).map((m) => m.mKey),
    };

    if (members.length > DHTSchema.maxMemberCount) {
      return false;
    }

    writerCount += writers.length;
    if (writerCount > DHTSchema.maxWriterCount) {
      return false;
    }

    return true;
  }

  /// The total number of subkeys this schema allocates: owner subkeys plus
  /// each member's subkey count.
  int get subkeyCount => members.fold(oCnt, (acc, v) => acc + v.mCnt);
}

/// Validation and subkey counting dispatched over any [DHTSchema] variant.
extension Validate on DHTSchema {
  /// Returns true if the schema's data representation is valid. Throws
  /// [TypeError] if the schema is neither a [DHTSchemaDFLT] nor a
  /// [DHTSchemaSMPL].
  bool validate() {
    if (this is DHTSchemaDFLT) {
      return (this as DHTSchemaDFLT).validate();
    } else if (this is DHTSchemaSMPL) {
      return (this as DHTSchemaSMPL).validate();
    }
    throw TypeError();
  }

  /// The total number of subkeys this schema allocates. Throws [TypeError] if
  /// the schema is neither a [DHTSchemaDFLT] nor a [DHTSchemaSMPL].
  int get subkeyCount {
    if (this is DHTSchemaDFLT) {
      return (this as DHTSchemaDFLT).subkeyCount;
    } else if (this is DHTSchemaSMPL) {
      return (this as DHTSchemaSMPL).subkeyCount;
    }
    throw TypeError();
  }
}

//////////////////////////////////////
/// DHT Schema

/// The schema controlling a DHT record's subkeys and who may write them.
@Freezed(unionKey: 'kind', unionValueCase: FreezedUnionCase.pascal)
sealed class DHTSchema with _$DHTSchema {
  /// Maximum number of subkeys a schema may allocate.
  static const maxSubkeyCount = 1024;

  /// Maximum number of distinct writers (owner plus members) a schema may have.
  static const maxWriterCount = 256;

  /// Maximum number of members a schema may have.
  static const maxMemberCount = 256;

  /// Default schema: a fixed number of owner-writable subkeys. In debug builds,
  /// throws [AssertionError] if `oCnt` is negative; use [validate] for a full
  /// schema check.
  @FreezedUnionValue('DFLT')
  // This is not an exhaustive assert, use validate() to check a schema
  @Assert('oCnt >= 0', 'value must not be negative')
  const factory DHTSchema.dflt({required int oCnt}) = DHTSchemaDFLT;

  /// Simple schema: owner subkeys plus per-member writable subkeys. In debug
  /// builds, throws [AssertionError] if `oCnt` is negative; use [validate] for a
  /// full schema check.
  @FreezedUnionValue('SMPL')
  // This is not an exhaustive assert, use validate() to check a schema
  @Assert('oCnt >= 0', 'value must not be negative')
  const factory DHTSchema.smpl({
    required int oCnt,
    required List<DHTSchemaMember> members,
  }) = DHTSchemaSMPL;

  /// Decode a [DHTSchema] from its JSON representation.
  factory DHTSchema.fromJson(dynamic json) =>
      _$DHTSchemaFromJson(json as Map<String, dynamic>);
}

/// A default schema with a single owner-writable subkey.
const defaultDHTSchema = DHTSchema.dflt(oCnt: 1);

/// A member of a [DHTSchemaSMPL] and the count of subkeys it may write.
@freezed
sealed class DHTSchemaMember with _$DHTSchemaMember {
  /// Make a schema member writing `mCnt` subkeys with member id `mKey`. In
  /// debug builds, throws [AssertionError] if `mCnt` is negative; use
  /// [DHTSchema.validate] for a full schema check.
  // This is not an exhaustive assert, use validate() to check a schema
  @Assert('mCnt >= 0', 'value must not be negative')
  const factory DHTSchemaMember({
    required BareMemberId mKey,
    required int mCnt,
  }) = _DHTSchemaMember;

  /// Decode a [DHTSchemaMember] from its JSON representation.
  factory DHTSchemaMember.fromJson(dynamic json) =>
      _$DHTSchemaMemberFromJson(json as Map<String, dynamic>);

  /// Make a schema member from a writer's [PublicKey], deriving its member id.
  ///
  /// Throws the [VeilidAPIException] raised by [Veilid.generateMemberId]:
  /// [VeilidAPIExceptionNotInitialized] when Veilid is not started or has shut
  /// down, or [VeilidAPIExceptionGeneric] when [publicKey] has an unsupported
  /// crypto kind or wrong length.
  static Future<DHTSchemaMember> fromPublicKey(
    Veilid veilid,
    PublicKey publicKey,
    int mCnt,
  ) async => DHTSchemaMember(
    mKey: (await veilid.generateMemberId(publicKey)).value,
    mCnt: mCnt,
  );
}

//////////////////////////////////////
/// DHTRecordDescriptor

/// Identifies a DHT record and carries its owner key and schema.
@freezed
sealed class DHTRecordDescriptor with _$DHTRecordDescriptor {
  /// `ownerSecret` is present when this node created the record, absent when it
  /// was opened.
  const factory DHTRecordDescriptor({
    required RecordKey key,
    required PublicKey owner,
    required DHTSchema schema,
    SecretKey? ownerSecret,
  }) = _DHTRecordDescriptor;

  /// Decode a [DHTRecordDescriptor] from its JSON representation.
  factory DHTRecordDescriptor.fromJson(dynamic json) =>
      _$DHTRecordDescriptorFromJson(json as Map<String, dynamic>);
}

/// Owner key accessors derived from a [DHTRecordDescriptor].
extension DHTRecordDescriptorExt on DHTRecordDescriptor {
  /// The owner's public and secret keys as a [BareKeyPair], or null when the
  /// secret is unknown.
  BareKeyPair? get ownerBareKeyPair {
    if (ownerSecret == null) {
      return null;
    }
    return BareKeyPair(key: owner.value, secret: ownerSecret!.value);
  }

  /// The owner's secret key as a [BareSecretKey], or null when unknown.
  BareSecretKey? get ownerBareSecretKey {
    if (ownerSecret == null) {
      return null;
    }
    return ownerSecret!.value;
  }

  /// The owner's public and secret keys as a [KeyPair], or null when the secret
  /// is unknown.
  KeyPair? get ownerKeyPair {
    if (ownerSecret == null) {
      return null;
    }
    return KeyPair(key: owner, secret: ownerSecret!);
  }
}

//////////////////////////////////////
/// ValueData

/// A DHT value and its metadata.
@freezed
sealed class ValueData with _$ValueData {
  /// Maximum length in bytes of the data a single subkey may hold.
  static const maxLen = 32768;

  /// `seq` time-orders changes to the subkey; `writer` is the public identity
  /// key of the writer. In debug builds, throws [AssertionError] if `seq` is
  /// outside `0..=4294967295` or `data` exceeds [maxLen] bytes.
  @Assert('seq >= 0 && seq <= 4294967295', 'seq out of range')
  @Assert('data.length <= ValueData.maxLen', 'data too large')
  const factory ValueData({
    required int seq,
    @Uint8ListJsonConverter.jsIsArray() required Uint8List data,
    required PublicKey writer,
  }) = _ValueData;

  /// Decode a [ValueData] from its JSON representation.
  factory ValueData.fromJson(dynamic json) =>
      _$ValueDataFromJson(json as Map<String, dynamic>);
}

//////////////////////////////////////
/// Stability

/// Choice of nodes to include in allocated routes.
enum Stability {
  /// Prefer nodes with low latency, but may be unreliable and require route
  /// reallocation.
  lowLatency,

  /// Prefer nodes with reliable uptime, but may have higher latency.
  reliable;

  /// Decode a [Stability] from its JSON representation.
  factory Stability.fromJson(dynamic j) =>
      Stability.values.byName((j as String).toCamelCase());

  /// Encode this [Stability] to its JSON representation.
  String toJson() => name.toPascalCase();
}

//////////////////////////////////////
/// Sequencing

/// Preferred ordering of RPC message delivery over a route or to a target.
enum Sequencing {
  /// Any ordering is acceptable, but unordered delivery is chosen if available.
  preferUnordered,

  /// Any ordering is acceptable, but ordered delivery is chosen if available.
  preferOrdered,

  /// Only ordered delivery is acceptable; otherwise the route is not created or
  /// the message is not sent.
  ensureOrdered;

  /// Decode a [Sequencing] from its JSON representation.
  factory Sequencing.fromJson(dynamic j) =>
      Sequencing.values.byName((j as String).toCamelCase());

  /// Encode this [Sequencing] to its JSON representation.
  String toJson() => name.toPascalCase();
}

//////////////////////////////////////
/// SafetySelection

/// The choice of safety route to include in compiled routes.
@immutable
abstract class SafetySelection {
  /// Decode a [SafetySelection] from its JSON representation. Throws
  /// [VeilidAPIExceptionInternal] if the JSON has neither an `Unsafe` nor a
  /// `Safe` key.
  factory SafetySelection.fromJson(dynamic jsond) {
    final json = jsond as Map<String, dynamic>;
    if (json.containsKey('Unsafe')) {
      return SafetySelectionUnsafe(
        sequencing: Sequencing.fromJson(json['Unsafe']),
      );
    } else if (json.containsKey('Safe')) {
      return SafetySelectionSafe(safetySpec: SafetySpec.fromJson(json['Safe']));
    } else {
      throw const VeilidAPIExceptionInternal('Invalid SafetySelection');
    }
  }

  /// Encode this [SafetySelection] to its JSON representation.
  Map<String, dynamic> toJson();
}

/// Don't use a safety route, only specify the sequencing preference.
@immutable
class SafetySelectionUnsafe extends Equatable implements SafetySelection {
  /// The sequencing preference for unsafe sends.
  final Sequencing sequencing;

  /// Select unsafe (non-private-routed) sends with the given [sequencing].
  const SafetySelectionUnsafe({required this.sequencing});

  @override
  List<Object> get props => [sequencing];

  @override
  bool? get stringify => null;

  @override
  Map<String, dynamic> toJson() => {'Unsafe': sequencing.toJson()};
}

/// Use a safety route with the parameters in a [SafetySpec].
@immutable
class SafetySelectionSafe extends Equatable implements SafetySelection {
  /// The safety route parameters.
  final SafetySpec safetySpec;

  /// Select safe (private-routed) sends governed by [safetySpec].
  const SafetySelectionSafe({required this.safetySpec});

  @override
  List<Object> get props => [safetySpec];

  @override
  bool? get stringify => null;

  @override
  Map<String, dynamic> toJson() => {'Safe': safetySpec.toJson()};
}

/// Options for safety routes (sender privacy).
@freezed
sealed class SafetySpec with _$SafetySpec {
  /// `hopCount` of zero uses the default route hop count; `preferredRoute`
  /// reuses an existing safety route if it still exists.
  const factory SafetySpec({
    @Default(0) int hopCount,
    @Default(Stability.reliable) Stability stability,
    @Default(Sequencing.preferOrdered) Sequencing sequencing,
    RouteId? preferredRoute,
  }) = _SafetySpec;

  /// Decode a [SafetySpec] from its JSON representation.
  factory SafetySpec.fromJson(dynamic json) =>
      _$SafetySpecFromJson(json as Map<String, dynamic>);
}

/// Options for private routes (receiver privacy).
@freezed
sealed class PrivateSpec with _$PrivateSpec {
  /// Empty `cryptoKinds` uses all available crypto kinds; `hopCount` of zero
  /// uses the default route hop count.
  const factory PrivateSpec({
    @JsonKey(fromJson: cryptoKindsFromJson, toJson: cryptoKindsToJson)
    @Default([])
    List<CryptoKind> cryptoKinds,
    @Default(0) int hopCount,
    @Default(Stability.reliable) Stability stability,
    @Default(Sequencing.preferOrdered) Sequencing sequencing,
  }) = _PrivateSpec;

  /// Decode a [PrivateSpec] from its JSON representation.
  factory PrivateSpec.fromJson(dynamic json) =>
      _$PrivateSpecFromJson(json as Map<String, dynamic>);
}

//////////////////////////////////////
/// Target

/// A destination for a message sent over a routing context.
@immutable
abstract class Target {
  /// Decode a [Target] from its JSON representation. Throws
  /// [VeilidAPIExceptionInternal] if the JSON has neither a `NodeId` nor a
  /// `RouteId` key.
  factory Target.fromJson(dynamic jsond) {
    final json = jsond as Map<String, dynamic>;
    if (json.containsKey('NodeId')) {
      return TargetNodeId(nodeId: NodeId.fromJson(json['NodeId']));
    } else if (json.containsKey('RouteId')) {
      return TargetRouteId(routeId: RouteId.fromJson(json['RouteId']));
    } else {
      throw const VeilidAPIExceptionInternal('Invalid Target');
    }
  }

  /// Encode this [Target] to its JSON representation.
  Map<String, dynamic> toJson();
}

/// A node addressed directly by its node id.
@immutable
class TargetNodeId extends Equatable implements Target {
  /// The target node's id.
  final NodeId nodeId;

  /// Target a node directly by its [nodeId].
  const TargetNodeId({required this.nodeId});

  @override
  List<Object> get props => [nodeId];

  @override
  bool? get stringify => null;

  @override
  Map<String, dynamic> toJson() => {'NodeId': nodeId.toJson()};
}

/// A remote private route addressed by its id.
@immutable
class TargetRouteId extends Equatable implements Target {
  /// The target route's id.
  final RouteId routeId;

  /// Target a private route by its [routeId].
  const TargetRouteId({required this.routeId});

  @override
  List<Object> get props => [routeId];

  @override
  bool? get stringify => null;

  @override
  Map<String, dynamic> toJson() => {'RouteId': routeId.toJson()};
}

//////////////////////////////////////
/// RouteBlob
/// An allocated route's id paired with its encoded blob for import by another
/// node.
@freezed
sealed class RouteBlob with _$RouteBlob {
  /// `blob` is the encoded route to import as a remote private route.
  const factory RouteBlob({
    required RouteId routeId,
    @Uint8ListJsonConverter.jsIsArray() required Uint8List blob,
  }) = _RouteBlob;

  /// Decode a [RouteBlob] from its JSON representation.
  factory RouteBlob.fromJson(dynamic json) =>
      _$RouteBlobFromJson(json as Map<String, dynamic>);
}

//////////////////////////////////////
/// Inspect
/// Local and network sequence numbers reported across a DHT record's subkeys.
@freezed
sealed class DHTRecordReport with _$DHTRecordReport {
  /// `subkeys` is the range covered, possibly trimmed to schema limits;
  /// `offlineSubkeys` were written offline and still need flushing; `localSeqs`
  /// and `networkSeqs` give the per-subkey sequence numbers in ascending order.
  const factory DHTRecordReport({
    required List<ValueSubkeyRange> subkeys,
    required List<ValueSubkeyRange> offlineSubkeys,
    required List<int?> localSeqs,
    required List<int?> networkSeqs,
  }) = _DHTRecordReport;

  /// Decode a [DHTRecordReport] from its JSON representation.
  factory DHTRecordReport.fromJson(dynamic json) =>
      _$DHTRecordReportFromJson(json as Map<String, dynamic>);
}

/// Which sequence numbers a DHT record report should gather.
enum DHTReportScope {
  /// Return only the local copy sequence numbers.
  local,

  /// Local plus network sequence numbers using GetValue fanout parameters.
  syncGet,

  /// Local plus network sequence numbers using SetValue fanout parameters.
  syncSet,

  /// As if a GetValue were performed, including accepting newer network values.
  updateGet,

  /// As if a SetValue were performed, including accepting newer network values.
  updateSet;

  /// Decode a [DHTReportScope] from its JSON representation.
  factory DHTReportScope.fromJson(dynamic j) =>
      DHTReportScope.values.byName((j as String).toCamelCase());

  /// Encode this [DHTReportScope] to its JSON representation.
  String toJson() => name.toPascalCase();
}

//////////////////////////////////////
/// SetDHTValueOptions

/// Options that override defaults for set_dht_value.
@freezed
sealed class SetDHTValueOptions with _$SetDHTValueOptions {
  /// `writer` overrides the writer key pair for the operation. `allowOffline`
  /// defaults to true; when false, an offline node returns a TryAgain error
  /// instead of writing.
  const factory SetDHTValueOptions({KeyPair? writer, bool? allowOffline}) =
      _SetDHTValueOptions;

  /// Decode a [SetDHTValueOptions] from its JSON representation.
  factory SetDHTValueOptions.fromJson(dynamic json) =>
      _$SetDHTValueOptionsFromJson(json as Map<String, dynamic>);

  @override
  Map<String, dynamic> toJson() => {
    'writer': writer,
    'allow_offline': allowOffline,
  };
}

//////////////////////////////////////
/// TransactDHTRecordsOptions

/// Options for DHT record transactions.
@freezed
sealed class TransactDHTRecordsOptions with _$TransactDHTRecordsOptions {
  /// `defaultSigningKeyPair` is used when opening the transaction. It does not
  /// override writer keys used by transaction operations, and only matters for
  /// records in the transaction that are open for reading only.
  const factory TransactDHTRecordsOptions({KeyPair? defaultSigningKeyPair}) =
      _TransactDHTRecordsOptions;

  /// Decode a [TransactDHTRecordsOptions] from its JSON representation.
  factory TransactDHTRecordsOptions.fromJson(dynamic json) =>
      _$TransactDHTRecordsOptionsFromJson(json as Map<String, dynamic>);

  @override
  Map<String, dynamic> toJson() => {
    'default_signing_keypair': defaultSigningKeyPair,
  };
}
