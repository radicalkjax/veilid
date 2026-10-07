import 'dart:typed_data';

import 'package:change_case/change_case.dart';
import 'package:freezed_annotation/freezed_annotation.dart';

import 'veilid.dart';

part 'veilid_state.freezed.dart';
part 'veilid_state.g.dart';

//////////////////////////////////////
/// AttachmentState

/// Network attachment level, expressed as a 'signal strength'.
enum AttachmentState {
  /// Not attached to the network.
  detached,

  /// Attaching, but not yet able to perform network operations.
  attaching,

  /// Attached with the weakest signal strength.
  attachedWeak,

  /// Attached with fair signal strength.
  attachedFair,

  /// Attached with good signal strength.
  attachedGood,

  /// Attached with strong signal strength.
  attachedStrong,

  /// Attached with the strongest signal strength.
  attachedFull,

  /// Detaching from the network and shutting down connections.
  detaching;

  /// Decode from its serialized PascalCase name.
  factory AttachmentState.fromJson(dynamic j) =>
      AttachmentState.values.byName((j as String).toCamelCase());

  /// Encode to its serialized PascalCase name.
  String toJson() => name.toPascalCase();

  /// Check if attachment is changing
  bool get isChanging => !isAttached && !isDetached;

  /// Check if attached
  bool get isAttached {
    switch (this) {
      case AttachmentState.detached:
      case AttachmentState.detaching:
      case AttachmentState.attaching:
        return false;
      case AttachmentState.attachedWeak:
      case AttachmentState.attachedFair:
      case AttachmentState.attachedGood:
      case AttachmentState.attachedStrong:
      case AttachmentState.attachedFull:
        return true;
    }
  }

  /// Check if detached
  bool get isDetached {
    switch (this) {
      case AttachmentState.detached:
        return true;
      case AttachmentState.detaching:
      case AttachmentState.attaching:
      case AttachmentState.attachedWeak:
      case AttachmentState.attachedFair:
      case AttachmentState.attachedGood:
      case AttachmentState.attachedStrong:
      case AttachmentState.attachedFull:
        return true;
    }
  }

  /// Signal-strength bars (0..=5).
  int get barCount {
    switch (this) {
      case AttachmentState.detached:
      case AttachmentState.detaching:
      case AttachmentState.attaching:
        return 0;
      case AttachmentState.attachedWeak:
        return 1;
      case AttachmentState.attachedFair:
        return 2;
      case AttachmentState.attachedGood:
        return 3;
      case AttachmentState.attachedStrong:
        return 4;
      case AttachmentState.attachedFull:
        return 5;
    }
  }
}

//////////////////////////////////////
/// VeilidLogLevel

/// Severity level of a log message emitted by veilid-core.
enum VeilidLogLevel {
  /// A fatal or unrecoverable condition.
  error,

  /// A recoverable problem worth surfacing.
  warn,

  /// Normal operational information.
  info,

  /// Diagnostic detail useful when debugging.
  debug,

  /// Fine-grained tracing detail.
  trace;

  /// Decode from its serialized PascalCase name.
  factory VeilidLogLevel.fromJson(dynamic j) =>
      VeilidLogLevel.values.byName((j as String).toCamelCase());

  /// Encode to its serialized PascalCase name.
  String toJson() => name.toPascalCase();
}

////////////

@freezed
sealed class LatencyStats with _$LatencyStats {
  const factory LatencyStats({
    required TimestampDuration fastest,
    required TimestampDuration average,
    required TimestampDuration slowest,
    required TimestampDuration tm90,
    required TimestampDuration tm75,
    required TimestampDuration p90,
    required TimestampDuration p75,
  }) = _LatencyStats;

  factory LatencyStats.fromJson(dynamic json) =>
      _$LatencyStatsFromJson(json as Map<String, dynamic>);
}

////////////

@freezed
sealed class TransferStats with _$TransferStats {
  const factory TransferStats({
    required BigInt total,
    required BigInt maximum,
    required BigInt average,
    required BigInt minimum,
  }) = _TransferStats;

  factory TransferStats.fromJson(dynamic json) =>
      _$TransferStatsFromJson(json as Map<String, dynamic>);
}

////////////

@freezed
sealed class TransferStatsDownUp with _$TransferStatsDownUp {
  const factory TransferStatsDownUp({
    required TransferStats down,
    required TransferStats up,
  }) = _TransferStatsDownUp;

  factory TransferStatsDownUp.fromJson(dynamic json) =>
      _$TransferStatsDownUpFromJson(json as Map<String, dynamic>);
}

////////////

@freezed
sealed class PeerStats with _$PeerStats {
  const factory PeerStats({
    required TransferStatsDownUp transfer,
    LatencyStats? latency,
  }) = _PeerStats;

  factory PeerStats.fromJson(dynamic json) =>
      _$PeerStatsFromJson(json as Map<String, dynamic>);
}

////////////

@freezed
sealed class PeerTableData with _$PeerTableData {
  const factory PeerTableData({
    required List<NodeId> nodeIds,
    required String peerAddress,
    required PeerStats peerStats,
  }) = _PeerTableData;

  factory PeerTableData.fromJson(dynamic json) =>
      _$PeerTableDataFromJson(json as Map<String, dynamic>);
}

//////////////////////////////////////
/// VeilidUpdate

@Freezed(unionKey: 'kind', unionValueCase: FreezedUnionCase.pascal)
sealed class VeilidUpdate with _$VeilidUpdate {
  const factory VeilidUpdate.log({
    required VeilidLogLevel logLevel,
    required String message,
    String? backtrace,
  }) = VeilidLog;
  const factory VeilidUpdate.appMessage({
    @Uint8ListJsonConverter.jsIsArray() required Uint8List message,
    PublicKey? sender,
    String? routeId,
  }) = VeilidAppMessage;
  const factory VeilidUpdate.appCall({
    @Uint8ListJsonConverter.jsIsArray() required Uint8List message,
    required String callId,
    PublicKey? sender,
    String? routeId,
  }) = VeilidAppCall;
  const factory VeilidUpdate.attachment({
    required AttachmentState state,
    required bool publicInternetReady,
    required bool localNetworkReady,
    required TimestampDuration uptime,
    required TimestampDuration? attachedUptime,
    required BigInt reliablePeerCount,
    required BigInt livePeerCount,
    required BigInt estimatedNetworkSize,
    required TimestampDuration? medianLatency,
    required BigInt overAttachedNodes,
  }) = VeilidUpdateAttachment;
  const factory VeilidUpdate.network({
    required bool started,
    required BigInt bpsDown,
    required BigInt bpsUp,
    required List<PeerTableData> peers,
    required List<NodeId> nodeIds,
  }) = VeilidUpdateNetwork;
  const factory VeilidUpdate.config({required VeilidConfig config}) =
      VeilidUpdateConfig;
  const factory VeilidUpdate.routeChange({
    required List<String> deadRoutes,
    required List<String> deadRemoteRoutes,
  }) = VeilidUpdateRouteChange;
  const factory VeilidUpdate.valueChange({
    required RecordKey key,
    required List<ValueSubkeyRange> subkeys,
    required int count,
    required ValueData? value,
  }) = VeilidUpdateValueChange;

  factory VeilidUpdate.fromJson(dynamic json) =>
      _$VeilidUpdateFromJson(json as Map<String, dynamic>);
}

//////////////////////////////////////
/// VeilidStateAttachment

@freezed
sealed class VeilidStateAttachment with _$VeilidStateAttachment {
  const factory VeilidStateAttachment({
    required AttachmentState state,
    required bool publicInternetReady,
    required bool localNetworkReady,
    required TimestampDuration uptime,
    required TimestampDuration? attachedUptime,
    required BigInt reliablePeerCount,
    required BigInt livePeerCount,
    required BigInt estimatedNetworkSize,
    required TimestampDuration? medianLatency,
    required BigInt overAttachedNodes,
  }) = _VeilidStateAttachment;

  factory VeilidStateAttachment.fromJson(dynamic json) =>
      _$VeilidStateAttachmentFromJson(json as Map<String, dynamic>);
}

//////////////////////////////////////
/// VeilidStateNetwork

@freezed
sealed class VeilidStateNetwork with _$VeilidStateNetwork {
  const factory VeilidStateNetwork({
    required bool started,
    required BigInt bpsDown,
    required BigInt bpsUp,
    required List<PeerTableData> peers,
  }) = _VeilidStateNetwork;

  factory VeilidStateNetwork.fromJson(dynamic json) =>
      _$VeilidStateNetworkFromJson(json as Map<String, dynamic>);
}

//////////////////////////////////////
/// VeilidStateConfig

@freezed
sealed class VeilidStateConfig with _$VeilidStateConfig {
  const factory VeilidStateConfig({required VeilidConfig config}) =
      _VeilidStateConfig;

  factory VeilidStateConfig.fromJson(dynamic json) =>
      _$VeilidStateConfigFromJson(json as Map<String, dynamic>);
}

//////////////////////////////////////
/// VeilidState

@freezed
sealed class VeilidState with _$VeilidState {
  const factory VeilidState({
    required VeilidStateAttachment attachment,
    required VeilidStateNetwork network,
    required VeilidStateConfig config,
  }) = _VeilidState;

  factory VeilidState.fromJson(dynamic json) =>
      _$VeilidStateFromJson(json as Map<String, dynamic>);
}
