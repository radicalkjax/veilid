// GENERATED CODE - DO NOT MODIFY BY HAND

part of 'veilid_state.dart';

// **************************************************************************
// JsonSerializableGenerator
// **************************************************************************

_LatencyStats _$LatencyStatsFromJson(Map<String, dynamic> json) =>
    _LatencyStats(
      fastest: TimestampDuration.fromJson(json['fastest']),
      average: TimestampDuration.fromJson(json['average']),
      slowest: TimestampDuration.fromJson(json['slowest']),
      tm90: TimestampDuration.fromJson(json['tm90']),
      tm75: TimestampDuration.fromJson(json['tm75']),
      p90: TimestampDuration.fromJson(json['p90']),
      p75: TimestampDuration.fromJson(json['p75']),
    );

Map<String, dynamic> _$LatencyStatsToJson(_LatencyStats instance) =>
    <String, dynamic>{
      'fastest': instance.fastest.toJson(),
      'average': instance.average.toJson(),
      'slowest': instance.slowest.toJson(),
      'tm90': instance.tm90.toJson(),
      'tm75': instance.tm75.toJson(),
      'p90': instance.p90.toJson(),
      'p75': instance.p75.toJson(),
    };

_TransferStats _$TransferStatsFromJson(Map<String, dynamic> json) =>
    _TransferStats(
      total: BigInt.parse(json['total'] as String),
      maximum: BigInt.parse(json['maximum'] as String),
      average: BigInt.parse(json['average'] as String),
      minimum: BigInt.parse(json['minimum'] as String),
    );

Map<String, dynamic> _$TransferStatsToJson(_TransferStats instance) =>
    <String, dynamic>{
      'total': instance.total.toString(),
      'maximum': instance.maximum.toString(),
      'average': instance.average.toString(),
      'minimum': instance.minimum.toString(),
    };

_TransferStatsDownUp _$TransferStatsDownUpFromJson(Map<String, dynamic> json) =>
    _TransferStatsDownUp(
      down: TransferStats.fromJson(json['down']),
      up: TransferStats.fromJson(json['up']),
    );

Map<String, dynamic> _$TransferStatsDownUpToJson(
  _TransferStatsDownUp instance,
) => <String, dynamic>{
  'down': instance.down.toJson(),
  'up': instance.up.toJson(),
};

_PeerStats _$PeerStatsFromJson(Map<String, dynamic> json) => _PeerStats(
  transfer: TransferStatsDownUp.fromJson(json['transfer']),
  latency: json['latency'] == null
      ? null
      : LatencyStats.fromJson(json['latency']),
);

Map<String, dynamic> _$PeerStatsToJson(_PeerStats instance) =>
    <String, dynamic>{
      'transfer': instance.transfer.toJson(),
      'latency': instance.latency?.toJson(),
    };

_PeerTableData _$PeerTableDataFromJson(Map<String, dynamic> json) =>
    _PeerTableData(
      nodeIds: (json['nodeIds'] as List<dynamic>)
          .map(Typed<BareNodeId>.fromJson)
          .toList(),
      peerAddress: json['peerAddress'] as String,
      peerStats: PeerStats.fromJson(json['peerStats']),
    );

Map<String, dynamic> _$PeerTableDataToJson(_PeerTableData instance) =>
    <String, dynamic>{
      'nodeIds': instance.nodeIds.map((e) => e.toJson()).toList(),
      'peerAddress': instance.peerAddress,
      'peerStats': instance.peerStats.toJson(),
    };

VeilidLog _$VeilidLogFromJson(Map<String, dynamic> json) => VeilidLog(
  logLevel: VeilidLogLevel.fromJson(json['logLevel']),
  message: json['message'] as String,
  backtrace: json['backtrace'] as String?,
  $type: json['kind'] as String?,
);

Map<String, dynamic> _$VeilidLogToJson(VeilidLog instance) => <String, dynamic>{
  'logLevel': instance.logLevel.toJson(),
  'message': instance.message,
  'backtrace': instance.backtrace,
  'kind': instance.$type,
};

VeilidAppMessage _$VeilidAppMessageFromJson(Map<String, dynamic> json) =>
    VeilidAppMessage(
      message: const Uint8ListJsonConverter.jsIsArray().fromJson(
        json['message'],
      ),
      sender: json['sender'] == null
          ? null
          : Typed<BarePublicKey>.fromJson(json['sender']),
      routeId: json['routeId'] as String?,
      $type: json['kind'] as String?,
    );

Map<String, dynamic> _$VeilidAppMessageToJson(
  VeilidAppMessage instance,
) => <String, dynamic>{
  'message': const Uint8ListJsonConverter.jsIsArray().toJson(instance.message),
  'sender': instance.sender?.toJson(),
  'routeId': instance.routeId,
  'kind': instance.$type,
};

VeilidAppCall _$VeilidAppCallFromJson(Map<String, dynamic> json) =>
    VeilidAppCall(
      message: const Uint8ListJsonConverter.jsIsArray().fromJson(
        json['message'],
      ),
      callId: json['callId'] as String,
      sender: json['sender'] == null
          ? null
          : Typed<BarePublicKey>.fromJson(json['sender']),
      routeId: json['routeId'] as String?,
      $type: json['kind'] as String?,
    );

Map<String, dynamic> _$VeilidAppCallToJson(
  VeilidAppCall instance,
) => <String, dynamic>{
  'message': const Uint8ListJsonConverter.jsIsArray().toJson(instance.message),
  'callId': instance.callId,
  'sender': instance.sender?.toJson(),
  'routeId': instance.routeId,
  'kind': instance.$type,
};

VeilidUpdateAttachment _$VeilidUpdateAttachmentFromJson(
  Map<String, dynamic> json,
) => VeilidUpdateAttachment(
  state: AttachmentState.fromJson(json['state']),
  publicInternetReady: json['publicInternetReady'] as bool,
  localNetworkReady: json['localNetworkReady'] as bool,
  uptime: TimestampDuration.fromJson(json['uptime']),
  attachedUptime: json['attachedUptime'] == null
      ? null
      : TimestampDuration.fromJson(json['attachedUptime']),
  reliablePeerCount: BigInt.parse(json['reliablePeerCount'] as String),
  livePeerCount: BigInt.parse(json['livePeerCount'] as String),
  estimatedNetworkSize: BigInt.parse(json['estimatedNetworkSize'] as String),
  medianLatency: json['medianLatency'] == null
      ? null
      : TimestampDuration.fromJson(json['medianLatency']),
  overAttachedNodes: BigInt.parse(json['overAttachedNodes'] as String),
  $type: json['kind'] as String?,
);

Map<String, dynamic> _$VeilidUpdateAttachmentToJson(
  VeilidUpdateAttachment instance,
) => <String, dynamic>{
  'state': instance.state.toJson(),
  'publicInternetReady': instance.publicInternetReady,
  'localNetworkReady': instance.localNetworkReady,
  'uptime': instance.uptime.toJson(),
  'attachedUptime': instance.attachedUptime?.toJson(),
  'reliablePeerCount': instance.reliablePeerCount.toString(),
  'livePeerCount': instance.livePeerCount.toString(),
  'estimatedNetworkSize': instance.estimatedNetworkSize.toString(),
  'medianLatency': instance.medianLatency?.toJson(),
  'overAttachedNodes': instance.overAttachedNodes.toString(),
  'kind': instance.$type,
};

VeilidUpdateNetwork _$VeilidUpdateNetworkFromJson(Map<String, dynamic> json) =>
    VeilidUpdateNetwork(
      started: json['started'] as bool,
      bpsDown: BigInt.parse(json['bpsDown'] as String),
      bpsUp: BigInt.parse(json['bpsUp'] as String),
      peers: (json['peers'] as List<dynamic>)
          .map(PeerTableData.fromJson)
          .toList(),
      nodeIds: (json['nodeIds'] as List<dynamic>)
          .map(Typed<BareNodeId>.fromJson)
          .toList(),
      $type: json['kind'] as String?,
    );

Map<String, dynamic> _$VeilidUpdateNetworkToJson(
  VeilidUpdateNetwork instance,
) => <String, dynamic>{
  'started': instance.started,
  'bpsDown': instance.bpsDown.toString(),
  'bpsUp': instance.bpsUp.toString(),
  'peers': instance.peers.map((e) => e.toJson()).toList(),
  'nodeIds': instance.nodeIds.map((e) => e.toJson()).toList(),
  'kind': instance.$type,
};

VeilidUpdateConfig _$VeilidUpdateConfigFromJson(Map<String, dynamic> json) =>
    VeilidUpdateConfig(
      config: VeilidConfig.fromJson(json['config']),
      $type: json['kind'] as String?,
    );

Map<String, dynamic> _$VeilidUpdateConfigToJson(VeilidUpdateConfig instance) =>
    <String, dynamic>{
      'config': instance.config.toJson(),
      'kind': instance.$type,
    };

VeilidUpdateRouteChange _$VeilidUpdateRouteChangeFromJson(
  Map<String, dynamic> json,
) => VeilidUpdateRouteChange(
  deadRoutes: (json['deadRoutes'] as List<dynamic>)
      .map((e) => e as String)
      .toList(),
  deadRemoteRoutes: (json['deadRemoteRoutes'] as List<dynamic>)
      .map((e) => e as String)
      .toList(),
  $type: json['kind'] as String?,
);

Map<String, dynamic> _$VeilidUpdateRouteChangeToJson(
  VeilidUpdateRouteChange instance,
) => <String, dynamic>{
  'deadRoutes': instance.deadRoutes,
  'deadRemoteRoutes': instance.deadRemoteRoutes,
  'kind': instance.$type,
};

VeilidUpdateValueChange _$VeilidUpdateValueChangeFromJson(
  Map<String, dynamic> json,
) => VeilidUpdateValueChange(
  key: RecordKey.fromJson(json['key']),
  subkeys: (json['subkeys'] as List<dynamic>)
      .map(ValueSubkeyRange.fromJson)
      .toList(),
  count: (json['count'] as num).toInt(),
  value: json['value'] == null ? null : ValueData.fromJson(json['value']),
  $type: json['kind'] as String?,
);

Map<String, dynamic> _$VeilidUpdateValueChangeToJson(
  VeilidUpdateValueChange instance,
) => <String, dynamic>{
  'key': instance.key.toJson(),
  'subkeys': instance.subkeys.map((e) => e.toJson()).toList(),
  'count': instance.count,
  'value': instance.value?.toJson(),
  'kind': instance.$type,
};

_VeilidStateAttachment _$VeilidStateAttachmentFromJson(
  Map<String, dynamic> json,
) => _VeilidStateAttachment(
  state: AttachmentState.fromJson(json['state']),
  publicInternetReady: json['publicInternetReady'] as bool,
  localNetworkReady: json['localNetworkReady'] as bool,
  uptime: TimestampDuration.fromJson(json['uptime']),
  attachedUptime: json['attachedUptime'] == null
      ? null
      : TimestampDuration.fromJson(json['attachedUptime']),
  reliablePeerCount: BigInt.parse(json['reliablePeerCount'] as String),
  livePeerCount: BigInt.parse(json['livePeerCount'] as String),
  estimatedNetworkSize: BigInt.parse(json['estimatedNetworkSize'] as String),
  medianLatency: json['medianLatency'] == null
      ? null
      : TimestampDuration.fromJson(json['medianLatency']),
  overAttachedNodes: BigInt.parse(json['overAttachedNodes'] as String),
);

Map<String, dynamic> _$VeilidStateAttachmentToJson(
  _VeilidStateAttachment instance,
) => <String, dynamic>{
  'state': instance.state.toJson(),
  'publicInternetReady': instance.publicInternetReady,
  'localNetworkReady': instance.localNetworkReady,
  'uptime': instance.uptime.toJson(),
  'attachedUptime': instance.attachedUptime?.toJson(),
  'reliablePeerCount': instance.reliablePeerCount.toString(),
  'livePeerCount': instance.livePeerCount.toString(),
  'estimatedNetworkSize': instance.estimatedNetworkSize.toString(),
  'medianLatency': instance.medianLatency?.toJson(),
  'overAttachedNodes': instance.overAttachedNodes.toString(),
};

_VeilidStateNetwork _$VeilidStateNetworkFromJson(Map<String, dynamic> json) =>
    _VeilidStateNetwork(
      started: json['started'] as bool,
      bpsDown: BigInt.parse(json['bpsDown'] as String),
      bpsUp: BigInt.parse(json['bpsUp'] as String),
      peers: (json['peers'] as List<dynamic>)
          .map(PeerTableData.fromJson)
          .toList(),
    );

Map<String, dynamic> _$VeilidStateNetworkToJson(_VeilidStateNetwork instance) =>
    <String, dynamic>{
      'started': instance.started,
      'bpsDown': instance.bpsDown.toString(),
      'bpsUp': instance.bpsUp.toString(),
      'peers': instance.peers.map((e) => e.toJson()).toList(),
    };

_VeilidStateConfig _$VeilidStateConfigFromJson(Map<String, dynamic> json) =>
    _VeilidStateConfig(config: VeilidConfig.fromJson(json['config']));

Map<String, dynamic> _$VeilidStateConfigToJson(_VeilidStateConfig instance) =>
    <String, dynamic>{'config': instance.config.toJson()};

_VeilidState _$VeilidStateFromJson(Map<String, dynamic> json) => _VeilidState(
  attachment: VeilidStateAttachment.fromJson(json['attachment']),
  network: VeilidStateNetwork.fromJson(json['network']),
  config: VeilidStateConfig.fromJson(json['config']),
);

Map<String, dynamic> _$VeilidStateToJson(_VeilidState instance) =>
    <String, dynamic>{
      'attachment': instance.attachment.toJson(),
      'network': instance.network.toJson(),
      'config': instance.config.toJson(),
    };
