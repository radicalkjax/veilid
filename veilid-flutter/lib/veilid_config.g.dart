// GENERATED CODE - DO NOT MODIFY BY HAND

part of 'veilid_config.dart';

// **************************************************************************
// JsonSerializableGenerator
// **************************************************************************

_VeilidFFIConfigLoggingTerminal _$VeilidFFIConfigLoggingTerminalFromJson(
  Map<String, dynamic> json,
) => _VeilidFFIConfigLoggingTerminal(
  enabled: json['enabled'] as bool? ?? true,
  level: json['level'] == null
      ? VeilidConfigLogLevel.info
      : VeilidConfigLogLevel.fromJson(json['level']),
  directives:
      (json['directives'] as List<dynamic>?)
          ?.map((e) => e as String)
          .toList() ??
      const [],
  ignoreLogTargets:
      (json['ignoreLogTargets'] as List<dynamic>?)
          ?.map((e) => e as String)
          .toList() ??
      const [],
);

Map<String, dynamic> _$VeilidFFIConfigLoggingTerminalToJson(
  _VeilidFFIConfigLoggingTerminal instance,
) => <String, dynamic>{
  'enabled': instance.enabled,
  'level': instance.level.toJson(),
  'directives': instance.directives,
  'ignoreLogTargets': instance.ignoreLogTargets,
};

_VeilidFFIConfigLoggingOtlp _$VeilidFFIConfigLoggingOtlpFromJson(
  Map<String, dynamic> json,
) => _VeilidFFIConfigLoggingOtlp(
  enabled: json['enabled'] as bool? ?? true,
  level: json['level'] == null
      ? VeilidConfigLogLevel.trace
      : VeilidConfigLogLevel.fromJson(json['level']),
  grpcEndpoint: json['grpcEndpoint'] as String,
  serviceName: json['serviceName'] as String,
  directives:
      (json['directives'] as List<dynamic>?)
          ?.map((e) => e as String)
          .toList() ??
      const [],
  ignoreLogTargets:
      (json['ignoreLogTargets'] as List<dynamic>?)
          ?.map((e) => e as String)
          .toList() ??
      const [],
);

Map<String, dynamic> _$VeilidFFIConfigLoggingOtlpToJson(
  _VeilidFFIConfigLoggingOtlp instance,
) => <String, dynamic>{
  'enabled': instance.enabled,
  'level': instance.level.toJson(),
  'grpcEndpoint': instance.grpcEndpoint,
  'serviceName': instance.serviceName,
  'directives': instance.directives,
  'ignoreLogTargets': instance.ignoreLogTargets,
};

_VeilidFFIConfigLoggingApi _$VeilidFFIConfigLoggingApiFromJson(
  Map<String, dynamic> json,
) => _VeilidFFIConfigLoggingApi(
  enabled: json['enabled'] as bool? ?? true,
  level: json['level'] == null
      ? VeilidConfigLogLevel.info
      : VeilidConfigLogLevel.fromJson(json['level']),
  directives:
      (json['directives'] as List<dynamic>?)
          ?.map((e) => e as String)
          .toList() ??
      const [],
  ignoreLogTargets:
      (json['ignoreLogTargets'] as List<dynamic>?)
          ?.map((e) => e as String)
          .toList() ??
      const [],
);

Map<String, dynamic> _$VeilidFFIConfigLoggingApiToJson(
  _VeilidFFIConfigLoggingApi instance,
) => <String, dynamic>{
  'enabled': instance.enabled,
  'level': instance.level.toJson(),
  'directives': instance.directives,
  'ignoreLogTargets': instance.ignoreLogTargets,
};

_VeilidFFIConfigLoggingFlame _$VeilidFFIConfigLoggingFlameFromJson(
  Map<String, dynamic> json,
) => _VeilidFFIConfigLoggingFlame(
  enabled: json['enabled'] as bool? ?? true,
  path: json['path'] as String,
);

Map<String, dynamic> _$VeilidFFIConfigLoggingFlameToJson(
  _VeilidFFIConfigLoggingFlame instance,
) => <String, dynamic>{'enabled': instance.enabled, 'path': instance.path};

_VeilidFFIConfigLoggingStats _$VeilidFFIConfigLoggingStatsFromJson(
  Map<String, dynamic> json,
) => _VeilidFFIConfigLoggingStats(
  enabled: json['enabled'] as bool? ?? false,
  intervalSecs: (json['intervalSecs'] as num?)?.toInt() ?? 10,
  path: json['path'] as String? ?? '',
);

Map<String, dynamic> _$VeilidFFIConfigLoggingStatsToJson(
  _VeilidFFIConfigLoggingStats instance,
) => <String, dynamic>{
  'enabled': instance.enabled,
  'intervalSecs': instance.intervalSecs,
  'path': instance.path,
};

_VeilidFFIConfigLogging _$VeilidFFIConfigLoggingFromJson(
  Map<String, dynamic> json,
) => _VeilidFFIConfigLogging(
  terminal: json['terminal'] == null
      ? const VeilidFFIConfigLoggingTerminal(enabled: false)
      : VeilidFFIConfigLoggingTerminal.fromJson(json['terminal']),
  api: json['api'] == null
      ? const VeilidFFIConfigLoggingApi(enabled: false)
      : VeilidFFIConfigLoggingApi.fromJson(json['api']),
  otlp: json['otlp'] == null
      ? const VeilidFFIConfigLoggingOtlp(
          enabled: false,
          grpcEndpoint: '',
          serviceName: '',
        )
      : VeilidFFIConfigLoggingOtlp.fromJson(json['otlp']),
  flame: json['flame'] == null
      ? const VeilidFFIConfigLoggingFlame(enabled: false, path: '')
      : VeilidFFIConfigLoggingFlame.fromJson(json['flame']),
  stats: json['stats'] == null
      ? const VeilidFFIConfigLoggingStats(
          enabled: false,
          intervalSecs: 10,
          path: '',
        )
      : VeilidFFIConfigLoggingStats.fromJson(json['stats']),
);

Map<String, dynamic> _$VeilidFFIConfigLoggingToJson(
  _VeilidFFIConfigLogging instance,
) => <String, dynamic>{
  'terminal': instance.terminal.toJson(),
  'api': instance.api.toJson(),
  'otlp': instance.otlp.toJson(),
  'flame': instance.flame.toJson(),
  'stats': instance.stats.toJson(),
};

_VeilidFFIConfig _$VeilidFFIConfigFromJson(Map<String, dynamic> json) =>
    _VeilidFFIConfig(logging: VeilidFFIConfigLogging.fromJson(json['logging']));

Map<String, dynamic> _$VeilidFFIConfigToJson(_VeilidFFIConfig instance) =>
    <String, dynamic>{'logging': instance.logging.toJson()};

_VeilidWASMConfigLoggingPerformanceConsole
_$VeilidWASMConfigLoggingPerformanceConsoleFromJson(
  Map<String, dynamic> json,
) => _VeilidWASMConfigLoggingPerformanceConsole(
  enabled: json['enabled'] as bool? ?? true,
  color: json['color'] as bool? ?? true,
  timestamp: json['timestamp'] as bool? ?? true,
  originBaseUrl: json['originBaseUrl'] as String?,
);

Map<String, dynamic> _$VeilidWASMConfigLoggingPerformanceConsoleToJson(
  _VeilidWASMConfigLoggingPerformanceConsole instance,
) => <String, dynamic>{
  'enabled': instance.enabled,
  'color': instance.color,
  'timestamp': instance.timestamp,
  'originBaseUrl': instance.originBaseUrl,
};

_VeilidWASMConfigLoggingPerformance
_$VeilidWASMConfigLoggingPerformanceFromJson(Map<String, dynamic> json) =>
    _VeilidWASMConfigLoggingPerformance(
      enabled: json['enabled'] as bool? ?? true,
      level: json['level'] == null
          ? VeilidConfigLogLevel.info
          : VeilidConfigLogLevel.fromJson(json['level']),
      timings: json['timings'] as bool? ?? false,
      console: json['console'] == null
          ? const VeilidWASMConfigLoggingPerformanceConsole(enabled: true)
          : VeilidWASMConfigLoggingPerformanceConsole.fromJson(json['console']),
      directives:
          (json['directives'] as List<dynamic>?)
              ?.map((e) => e as String)
              .toList() ??
          const [],
      ignoreLogTargets:
          (json['ignoreLogTargets'] as List<dynamic>?)
              ?.map((e) => e as String)
              .toList() ??
          const [],
    );

Map<String, dynamic> _$VeilidWASMConfigLoggingPerformanceToJson(
  _VeilidWASMConfigLoggingPerformance instance,
) => <String, dynamic>{
  'enabled': instance.enabled,
  'level': instance.level.toJson(),
  'timings': instance.timings,
  'console': instance.console.toJson(),
  'directives': instance.directives,
  'ignoreLogTargets': instance.ignoreLogTargets,
};

_VeilidWASMConfigLoggingApi _$VeilidWASMConfigLoggingApiFromJson(
  Map<String, dynamic> json,
) => _VeilidWASMConfigLoggingApi(
  enabled: json['enabled'] as bool? ?? true,
  level: json['level'] == null
      ? VeilidConfigLogLevel.info
      : VeilidConfigLogLevel.fromJson(json['level']),
  directives:
      (json['directives'] as List<dynamic>?)
          ?.map((e) => e as String)
          .toList() ??
      const [],
  ignoreLogTargets:
      (json['ignoreLogTargets'] as List<dynamic>?)
          ?.map((e) => e as String)
          .toList() ??
      const [],
);

Map<String, dynamic> _$VeilidWASMConfigLoggingApiToJson(
  _VeilidWASMConfigLoggingApi instance,
) => <String, dynamic>{
  'enabled': instance.enabled,
  'level': instance.level.toJson(),
  'directives': instance.directives,
  'ignoreLogTargets': instance.ignoreLogTargets,
};

_VeilidWASMConfigLoggingStats _$VeilidWASMConfigLoggingStatsFromJson(
  Map<String, dynamic> json,
) => _VeilidWASMConfigLoggingStats(
  enabled: json['enabled'] as bool? ?? false,
  intervalSecs: (json['intervalSecs'] as num?)?.toInt() ?? 10,
);

Map<String, dynamic> _$VeilidWASMConfigLoggingStatsToJson(
  _VeilidWASMConfigLoggingStats instance,
) => <String, dynamic>{
  'enabled': instance.enabled,
  'intervalSecs': instance.intervalSecs,
};

_VeilidWASMConfigLogging _$VeilidWASMConfigLoggingFromJson(
  Map<String, dynamic> json,
) => _VeilidWASMConfigLogging(
  performance: json['performance'] == null
      ? const VeilidWASMConfigLoggingPerformance(enabled: false)
      : VeilidWASMConfigLoggingPerformance.fromJson(json['performance']),
  api: json['api'] == null
      ? const VeilidWASMConfigLoggingApi(enabled: false)
      : VeilidWASMConfigLoggingApi.fromJson(json['api']),
  stats: json['stats'] == null
      ? const VeilidWASMConfigLoggingStats(enabled: false, intervalSecs: 10)
      : VeilidWASMConfigLoggingStats.fromJson(json['stats']),
);

Map<String, dynamic> _$VeilidWASMConfigLoggingToJson(
  _VeilidWASMConfigLogging instance,
) => <String, dynamic>{
  'performance': instance.performance.toJson(),
  'api': instance.api.toJson(),
  'stats': instance.stats.toJson(),
};

_VeilidWASMConfig _$VeilidWASMConfigFromJson(Map<String, dynamic> json) =>
    _VeilidWASMConfig(
      logging: json['logging'] == null
          ? const VeilidWASMConfigLogging()
          : VeilidWASMConfigLogging.fromJson(json['logging']),
    );

Map<String, dynamic> _$VeilidWASMConfigToJson(_VeilidWASMConfig instance) =>
    <String, dynamic>{'logging': instance.logging.toJson()};

_VeilidConfigUDP _$VeilidConfigUDPFromJson(Map<String, dynamic> json) =>
    _VeilidConfigUDP(
      enabled: json['enabled'] as bool,
      listenAddress: json['listenAddress'] as String,
      publicAddress: json['publicAddress'] as String?,
    );

Map<String, dynamic> _$VeilidConfigUDPToJson(_VeilidConfigUDP instance) =>
    <String, dynamic>{
      'enabled': instance.enabled,
      'listenAddress': instance.listenAddress,
      'publicAddress': instance.publicAddress,
    };

_VeilidConfigTCP _$VeilidConfigTCPFromJson(Map<String, dynamic> json) =>
    _VeilidConfigTCP(
      connect: json['connect'] as bool,
      listen: json['listen'] as bool,
      listenAddress: json['listenAddress'] as String,
      publicAddress: json['publicAddress'] as String?,
    );

Map<String, dynamic> _$VeilidConfigTCPToJson(_VeilidConfigTCP instance) =>
    <String, dynamic>{
      'connect': instance.connect,
      'listen': instance.listen,
      'listenAddress': instance.listenAddress,
      'publicAddress': instance.publicAddress,
    };

_VeilidConfigWS _$VeilidConfigWSFromJson(Map<String, dynamic> json) =>
    _VeilidConfigWS(
      connect: json['connect'] as bool,
      listen: json['listen'] as bool,
      listenAddress: json['listenAddress'] as String,
      path: json['path'] as String,
      url: json['url'] as String?,
    );

Map<String, dynamic> _$VeilidConfigWSToJson(_VeilidConfigWS instance) =>
    <String, dynamic>{
      'connect': instance.connect,
      'listen': instance.listen,
      'listenAddress': instance.listenAddress,
      'path': instance.path,
      'url': instance.url,
    };

_VeilidConfigProtocol _$VeilidConfigProtocolFromJson(
  Map<String, dynamic> json,
) => _VeilidConfigProtocol(
  udp: VeilidConfigUDP.fromJson(json['udp']),
  tcp: VeilidConfigTCP.fromJson(json['tcp']),
  ws: VeilidConfigWS.fromJson(json['ws']),
);

Map<String, dynamic> _$VeilidConfigProtocolToJson(
  _VeilidConfigProtocol instance,
) => <String, dynamic>{
  'udp': instance.udp.toJson(),
  'tcp': instance.tcp.toJson(),
  'ws': instance.ws.toJson(),
};

_VeilidConfigPrivacy _$VeilidConfigPrivacyFromJson(Map<String, dynamic> json) =>
    _VeilidConfigPrivacy(
      requireInboundRelay: json['requireInboundRelay'] as bool,
    );

Map<String, dynamic> _$VeilidConfigPrivacyToJson(
  _VeilidConfigPrivacy instance,
) => <String, dynamic>{'requireInboundRelay': instance.requireInboundRelay};

_VeilidConfigTLS _$VeilidConfigTLSFromJson(Map<String, dynamic> json) =>
    _VeilidConfigTLS(
      certificatePath: json['certificatePath'] as String,
      privateKeyPath: json['privateKeyPath'] as String,
      connectionInitialTimeoutMs: (json['connectionInitialTimeoutMs'] as num)
          .toInt(),
    );

Map<String, dynamic> _$VeilidConfigTLSToJson(_VeilidConfigTLS instance) =>
    <String, dynamic>{
      'certificatePath': instance.certificatePath,
      'privateKeyPath': instance.privateKeyPath,
      'connectionInitialTimeoutMs': instance.connectionInitialTimeoutMs,
    };

_VeilidConfigDHT _$VeilidConfigDHTFromJson(Map<String, dynamic> json) =>
    _VeilidConfigDHT(
      localSubkeyCacheSize: (json['localSubkeyCacheSize'] as num).toInt(),
      localMaxSubkeyCacheMemoryMb: (json['localMaxSubkeyCacheMemoryMb'] as num)
          .toInt(),
      remoteSubkeyCacheSize: (json['remoteSubkeyCacheSize'] as num).toInt(),
      remoteMaxRecords: (json['remoteMaxRecords'] as num).toInt(),
      remoteMaxSubkeyCacheMemoryMb:
          (json['remoteMaxSubkeyCacheMemoryMb'] as num).toInt(),
      remoteMaxStorageSpaceMb: (json['remoteMaxStorageSpaceMb'] as num).toInt(),
      maxConcurrentOperations: (json['maxConcurrentOperations'] as num).toInt(),
    );

Map<String, dynamic> _$VeilidConfigDHTToJson(_VeilidConfigDHT instance) =>
    <String, dynamic>{
      'localSubkeyCacheSize': instance.localSubkeyCacheSize,
      'localMaxSubkeyCacheMemoryMb': instance.localMaxSubkeyCacheMemoryMb,
      'remoteSubkeyCacheSize': instance.remoteSubkeyCacheSize,
      'remoteMaxRecords': instance.remoteMaxRecords,
      'remoteMaxSubkeyCacheMemoryMb': instance.remoteMaxSubkeyCacheMemoryMb,
      'remoteMaxStorageSpaceMb': instance.remoteMaxStorageSpaceMb,
      'maxConcurrentOperations': instance.maxConcurrentOperations,
    };

_VeilidConfigRPC _$VeilidConfigRPCFromJson(Map<String, dynamic> json) =>
    _VeilidConfigRPC(
      defaultRouteHopCount: (json['defaultRouteHopCount'] as num).toInt(),
    );

Map<String, dynamic> _$VeilidConfigRPCToJson(_VeilidConfigRPC instance) =>
    <String, dynamic>{'defaultRouteHopCount': instance.defaultRouteHopCount};

_VeilidConfigRoutingTable _$VeilidConfigRoutingTableFromJson(
  Map<String, dynamic> json,
) => _VeilidConfigRoutingTable(
  publicKeys: (json['publicKeys'] as List<dynamic>)
      .map(Typed<BarePublicKey>.fromJson)
      .toList(),
  secretKeys: (json['secretKeys'] as List<dynamic>)
      .map(Typed<BareSecretKey>.fromJson)
      .toList(),
  bootstrap: (json['bootstrap'] as List<dynamic>)
      .map((e) => e as String)
      .toList(),
  bootstrapKeys: (json['bootstrapKeys'] as List<dynamic>)
      .map(Typed<BarePublicKey>.fromJson)
      .toList(),
  bootstrapRecords: (json['bootstrapRecords'] as List<dynamic>)
      .map((e) => e as String)
      .toList(),
  serveDirectBootstrap: json['serveDirectBootstrap'] as bool? ?? false,
);

Map<String, dynamic> _$VeilidConfigRoutingTableToJson(
  _VeilidConfigRoutingTable instance,
) => <String, dynamic>{
  'publicKeys': instance.publicKeys.map((e) => e.toJson()).toList(),
  'secretKeys': instance.secretKeys.map((e) => e.toJson()).toList(),
  'bootstrap': instance.bootstrap,
  'bootstrapKeys': instance.bootstrapKeys.map((e) => e.toJson()).toList(),
  'bootstrapRecords': instance.bootstrapRecords,
  'serveDirectBootstrap': instance.serveDirectBootstrap,
};

_VeilidConfigNetwork _$VeilidConfigNetworkFromJson(Map<String, dynamic> json) =>
    _VeilidConfigNetwork(
      maxConnections: (json['maxConnections'] as num).toInt(),
      routingTable: VeilidConfigRoutingTable.fromJson(json['routingTable']),
      rpc: VeilidConfigRPC.fromJson(json['rpc']),
      dht: VeilidConfigDHT.fromJson(json['dht']),
      addressTypes:
          (json['addressTypes'] as List<dynamic>?)
              ?.map(VeilidConfigAddressType.fromJson)
              .toList() ??
          const <VeilidConfigAddressType>[],
      upnp: json['upnp'] as bool,
      detectAddressChanges: json['detectAddressChanges'] as bool?,
      bindRetryMs: (json['bindRetryMs'] as num?)?.toInt() ?? 10000,
      tls: VeilidConfigTLS.fromJson(json['tls']),
      protocol: VeilidConfigProtocol.fromJson(json['protocol']),
      privacy: VeilidConfigPrivacy.fromJson(json['privacy']),
      networkKeyPassword: json['networkKeyPassword'] as String?,
    );

Map<String, dynamic> _$VeilidConfigNetworkToJson(
  _VeilidConfigNetwork instance,
) => <String, dynamic>{
  'maxConnections': instance.maxConnections,
  'routingTable': instance.routingTable.toJson(),
  'rpc': instance.rpc.toJson(),
  'dht': instance.dht.toJson(),
  'addressTypes': instance.addressTypes.map((e) => e.toJson()).toList(),
  'upnp': instance.upnp,
  'detectAddressChanges': instance.detectAddressChanges,
  'bindRetryMs': instance.bindRetryMs,
  'tls': instance.tls.toJson(),
  'protocol': instance.protocol.toJson(),
  'privacy': instance.privacy.toJson(),
  'networkKeyPassword': instance.networkKeyPassword,
};

_VeilidConfigTableStore _$VeilidConfigTableStoreFromJson(
  Map<String, dynamic> json,
) => _VeilidConfigTableStore(
  directory: json['directory'] as String,
  delete: json['delete'] as bool,
  wipeOnInvalidDeviceEncryptionKey:
      json['wipeOnInvalidDeviceEncryptionKey'] as bool,
  maxValueSizeMb: (json['maxValueSizeMb'] as num).toInt(),
);

Map<String, dynamic> _$VeilidConfigTableStoreToJson(
  _VeilidConfigTableStore instance,
) => <String, dynamic>{
  'directory': instance.directory,
  'delete': instance.delete,
  'wipeOnInvalidDeviceEncryptionKey': instance.wipeOnInvalidDeviceEncryptionKey,
  'maxValueSizeMb': instance.maxValueSizeMb,
};

_VeilidConfigBlockStore _$VeilidConfigBlockStoreFromJson(
  Map<String, dynamic> json,
) => _VeilidConfigBlockStore(
  directory: json['directory'] as String,
  delete: json['delete'] as bool,
);

Map<String, dynamic> _$VeilidConfigBlockStoreToJson(
  _VeilidConfigBlockStore instance,
) => <String, dynamic>{
  'directory': instance.directory,
  'delete': instance.delete,
};

_VeilidConfigProtectedStore _$VeilidConfigProtectedStoreFromJson(
  Map<String, dynamic> json,
) => _VeilidConfigProtectedStore(
  allowInsecureFallback: json['allowInsecureFallback'] as bool,
  alwaysUseInsecureStorage: json['alwaysUseInsecureStorage'] as bool,
  directory: json['directory'] as String,
  delete: json['delete'] as bool,
  deviceEncryptionKeyPassword: json['deviceEncryptionKeyPassword'] as String,
  backend: VeilidConfigProtectedStoreBackend.fromJson(json['backend']),
  newDeviceEncryptionKeyPassword:
      json['newDeviceEncryptionKeyPassword'] as String?,
);

Map<String, dynamic> _$VeilidConfigProtectedStoreToJson(
  _VeilidConfigProtectedStore instance,
) => <String, dynamic>{
  'allowInsecureFallback': instance.allowInsecureFallback,
  'alwaysUseInsecureStorage': instance.alwaysUseInsecureStorage,
  'directory': instance.directory,
  'delete': instance.delete,
  'deviceEncryptionKeyPassword': instance.deviceEncryptionKeyPassword,
  'backend': instance.backend.toJson(),
  'newDeviceEncryptionKeyPassword': instance.newDeviceEncryptionKeyPassword,
};

_VeilidConfigCapabilities _$VeilidConfigCapabilitiesFromJson(
  Map<String, dynamic> json,
) => _VeilidConfigCapabilities(
  disable: (json['disable'] as List<dynamic>).map((e) => e as String).toList(),
);

Map<String, dynamic> _$VeilidConfigCapabilitiesToJson(
  _VeilidConfigCapabilities instance,
) => <String, dynamic>{'disable': instance.disable};

_VeilidConfigAppLifecycle _$VeilidConfigAppLifecycleFromJson(
  Map<String, dynamic> json,
) => _VeilidConfigAppLifecycle(
  preferForeground: json['preferForeground'] as bool? ?? true,
);

Map<String, dynamic> _$VeilidConfigAppLifecycleToJson(
  _VeilidConfigAppLifecycle instance,
) => <String, dynamic>{'preferForeground': instance.preferForeground};

_VeilidConfigAttachment _$VeilidConfigAttachmentFromJson(
  Map<String, dynamic> json,
) => _VeilidConfigAttachment(
  appLifecycle: json['appLifecycle'] == null
      ? const VeilidConfigAppLifecycle()
      : VeilidConfigAppLifecycle.fromJson(json['appLifecycle']),
);

Map<String, dynamic> _$VeilidConfigAttachmentToJson(
  _VeilidConfigAttachment instance,
) => <String, dynamic>{'appLifecycle': instance.appLifecycle.toJson()};

_VeilidConfigInternalUDP _$VeilidConfigInternalUDPFromJson(
  Map<String, dynamic> json,
) => _VeilidConfigInternalUDP(
  socketPoolSize: (json['socketPoolSize'] as num).toInt(),
);

Map<String, dynamic> _$VeilidConfigInternalUDPToJson(
  _VeilidConfigInternalUDP instance,
) => <String, dynamic>{'socketPoolSize': instance.socketPoolSize};

_VeilidConfigInternalProtocol _$VeilidConfigInternalProtocolFromJson(
  Map<String, dynamic> json,
) => _VeilidConfigInternalProtocol(
  udp: VeilidConfigInternalUDP.fromJson(json['udp']),
);

Map<String, dynamic> _$VeilidConfigInternalProtocolToJson(
  _VeilidConfigInternalProtocol instance,
) => <String, dynamic>{'udp': instance.udp.toJson()};

_VeilidConfigInternalRPC _$VeilidConfigInternalRPCFromJson(
  Map<String, dynamic> json,
) => _VeilidConfigInternalRPC(
  concurrency: (json['concurrency'] as num).toInt(),
  queueSize: (json['queueSize'] as num).toInt(),
  timeoutMs: (json['timeoutMs'] as num).toInt(),
  maxRouteHopCount: (json['maxRouteHopCount'] as num).toInt(),
  maxTimestampBehindMs: (json['maxTimestampBehindMs'] as num?)?.toInt(),
  maxTimestampAheadMs: (json['maxTimestampAheadMs'] as num?)?.toInt(),
);

Map<String, dynamic> _$VeilidConfigInternalRPCToJson(
  _VeilidConfigInternalRPC instance,
) => <String, dynamic>{
  'concurrency': instance.concurrency,
  'queueSize': instance.queueSize,
  'timeoutMs': instance.timeoutMs,
  'maxRouteHopCount': instance.maxRouteHopCount,
  'maxTimestampBehindMs': instance.maxTimestampBehindMs,
  'maxTimestampAheadMs': instance.maxTimestampAheadMs,
};

_VeilidConfigInternalDHT _$VeilidConfigInternalDHTFromJson(
  Map<String, dynamic> json,
) => _VeilidConfigInternalDHT(
  maxFindNodeCount: (json['maxFindNodeCount'] as num).toInt(),
  resolveNodeTimeoutMs: (json['resolveNodeTimeoutMs'] as num).toInt(),
  resolveNodeCount: (json['resolveNodeCount'] as num).toInt(),
  resolveNodeFanout: (json['resolveNodeFanout'] as num).toInt(),
  getValueTimeoutMs: (json['getValueTimeoutMs'] as num).toInt(),
  getValueCount: (json['getValueCount'] as num).toInt(),
  getValueFanout: (json['getValueFanout'] as num).toInt(),
  setValueTimeoutMs: (json['setValueTimeoutMs'] as num).toInt(),
  setValueCount: (json['setValueCount'] as num).toInt(),
  setValueFanout: (json['setValueFanout'] as num).toInt(),
  consensusWidth: (json['consensusWidth'] as num).toInt(),
  minPeerCount: (json['minPeerCount'] as num).toInt(),
  minPeerRefreshTimeMs: (json['minPeerRefreshTimeMs'] as num).toInt(),
  validateDialInfoReceiptTimeMs: (json['validateDialInfoReceiptTimeMs'] as num)
      .toInt(),
  maxWatchExpirationMs: (json['maxWatchExpirationMs'] as num).toInt(),
  maxWatchInspectIntervalMs: (json['maxWatchInspectIntervalMs'] as num).toInt(),
  publicWatchLimit: (json['publicWatchLimit'] as num).toInt(),
  memberWatchLimit: (json['memberWatchLimit'] as num).toInt(),
  publicTransactionLimit: (json['publicTransactionLimit'] as num).toInt(),
  memberTransactionLimit: (json['memberTransactionLimit'] as num).toInt(),
);

Map<String, dynamic> _$VeilidConfigInternalDHTToJson(
  _VeilidConfigInternalDHT instance,
) => <String, dynamic>{
  'maxFindNodeCount': instance.maxFindNodeCount,
  'resolveNodeTimeoutMs': instance.resolveNodeTimeoutMs,
  'resolveNodeCount': instance.resolveNodeCount,
  'resolveNodeFanout': instance.resolveNodeFanout,
  'getValueTimeoutMs': instance.getValueTimeoutMs,
  'getValueCount': instance.getValueCount,
  'getValueFanout': instance.getValueFanout,
  'setValueTimeoutMs': instance.setValueTimeoutMs,
  'setValueCount': instance.setValueCount,
  'setValueFanout': instance.setValueFanout,
  'consensusWidth': instance.consensusWidth,
  'minPeerCount': instance.minPeerCount,
  'minPeerRefreshTimeMs': instance.minPeerRefreshTimeMs,
  'validateDialInfoReceiptTimeMs': instance.validateDialInfoReceiptTimeMs,
  'maxWatchExpirationMs': instance.maxWatchExpirationMs,
  'maxWatchInspectIntervalMs': instance.maxWatchInspectIntervalMs,
  'publicWatchLimit': instance.publicWatchLimit,
  'memberWatchLimit': instance.memberWatchLimit,
  'publicTransactionLimit': instance.publicTransactionLimit,
  'memberTransactionLimit': instance.memberTransactionLimit,
};

_VeilidConfigInternalRoutingTableSave
_$VeilidConfigInternalRoutingTableSaveFromJson(Map<String, dynamic> json) =>
    _VeilidConfigInternalRoutingTableSave(
      settleMs: (json['settleMs'] as num).toInt(),
      minIntervalMs: (json['minIntervalMs'] as num).toInt(),
      backgroundMinIntervalMs: (json['backgroundMinIntervalMs'] as num).toInt(),
      periodMs: (json['periodMs'] as num).toInt(),
    );

Map<String, dynamic> _$VeilidConfigInternalRoutingTableSaveToJson(
  _VeilidConfigInternalRoutingTableSave instance,
) => <String, dynamic>{
  'settleMs': instance.settleMs,
  'minIntervalMs': instance.minIntervalMs,
  'backgroundMinIntervalMs': instance.backgroundMinIntervalMs,
  'periodMs': instance.periodMs,
};

_VeilidConfigInternalRoutingTable _$VeilidConfigInternalRoutingTableFromJson(
  Map<String, dynamic> json,
) => _VeilidConfigInternalRoutingTable(
  save: VeilidConfigInternalRoutingTableSave.fromJson(json['save']),
);

Map<String, dynamic> _$VeilidConfigInternalRoutingTableToJson(
  _VeilidConfigInternalRoutingTable instance,
) => <String, dynamic>{'save': instance.save.toJson()};

_VeilidConfigInternalNetwork _$VeilidConfigInternalNetworkFromJson(
  Map<String, dynamic> json,
) => _VeilidConfigInternalNetwork(
  connectionInitialTimeoutMs: (json['connectionInitialTimeoutMs'] as num)
      .toInt(),
  singleSendTimeoutMs: (json['singleSendTimeoutMs'] as num).toInt(),
  connectionInactivityTimeoutMs: (json['connectionInactivityTimeoutMs'] as num)
      .toInt(),
  maxConnectionsPerIp4: (json['maxConnectionsPerIp4'] as num).toInt(),
  maxConnectionsPerIp6Prefix: (json['maxConnectionsPerIp6Prefix'] as num)
      .toInt(),
  maxConnectionsPerIp6PrefixSize:
      (json['maxConnectionsPerIp6PrefixSize'] as num).toInt(),
  maxConnectionFrequencyPerMin: (json['maxConnectionFrequencyPerMin'] as num)
      .toInt(),
  clientAllowlistTimeoutMs: (json['clientAllowlistTimeoutMs'] as num).toInt(),
  reverseConnectionReceiptTimeMs:
      (json['reverseConnectionReceiptTimeMs'] as num).toInt(),
  holePunchReceiptTimeMs: (json['holePunchReceiptTimeMs'] as num).toInt(),
  defaultHolePunchTtl: (json['defaultHolePunchTtl'] as num).toInt(),
  cgnatHolePunchTtl: (json['cgnatHolePunchTtl'] as num).toInt(),
  restrictedNatRetries: (json['restrictedNatRetries'] as num).toInt(),
  rpc: VeilidConfigInternalRPC.fromJson(json['rpc']),
  dht: VeilidConfigInternalDHT.fromJson(json['dht']),
  protocol: VeilidConfigInternalProtocol.fromJson(json['protocol']),
  routingTable: VeilidConfigInternalRoutingTable.fromJson(json['routingTable']),
);

Map<String, dynamic> _$VeilidConfigInternalNetworkToJson(
  _VeilidConfigInternalNetwork instance,
) => <String, dynamic>{
  'connectionInitialTimeoutMs': instance.connectionInitialTimeoutMs,
  'singleSendTimeoutMs': instance.singleSendTimeoutMs,
  'connectionInactivityTimeoutMs': instance.connectionInactivityTimeoutMs,
  'maxConnectionsPerIp4': instance.maxConnectionsPerIp4,
  'maxConnectionsPerIp6Prefix': instance.maxConnectionsPerIp6Prefix,
  'maxConnectionsPerIp6PrefixSize': instance.maxConnectionsPerIp6PrefixSize,
  'maxConnectionFrequencyPerMin': instance.maxConnectionFrequencyPerMin,
  'clientAllowlistTimeoutMs': instance.clientAllowlistTimeoutMs,
  'reverseConnectionReceiptTimeMs': instance.reverseConnectionReceiptTimeMs,
  'holePunchReceiptTimeMs': instance.holePunchReceiptTimeMs,
  'defaultHolePunchTtl': instance.defaultHolePunchTtl,
  'cgnatHolePunchTtl': instance.cgnatHolePunchTtl,
  'restrictedNatRetries': instance.restrictedNatRetries,
  'rpc': instance.rpc.toJson(),
  'dht': instance.dht.toJson(),
  'protocol': instance.protocol.toJson(),
  'routingTable': instance.routingTable.toJson(),
};

_VeilidConfigInternal _$VeilidConfigInternalFromJson(
  Map<String, dynamic> json,
) => _VeilidConfigInternal(
  network: VeilidConfigInternalNetwork.fromJson(json['network']),
);

Map<String, dynamic> _$VeilidConfigInternalToJson(
  _VeilidConfigInternal instance,
) => <String, dynamic>{'network': instance.network.toJson()};

_VeilidConfig _$VeilidConfigFromJson(Map<String, dynamic> json) =>
    _VeilidConfig(
      programName: json['programName'] as String,
      namespace: json['namespace'] as String,
      capabilities: VeilidConfigCapabilities.fromJson(json['capabilities']),
      protectedStore: VeilidConfigProtectedStore.fromJson(
        json['protectedStore'],
      ),
      tableStore: VeilidConfigTableStore.fromJson(json['tableStore']),
      blockStore: VeilidConfigBlockStore.fromJson(json['blockStore']),
      network: VeilidConfigNetwork.fromJson(json['network']),
      attachment: json['attachment'] == null
          ? const VeilidConfigAttachment()
          : VeilidConfigAttachment.fromJson(json['attachment']),
      internal: json['internal'] == null
          ? null
          : VeilidConfigInternal.fromJson(json['internal']),
    );

Map<String, dynamic> _$VeilidConfigToJson(_VeilidConfig instance) =>
    <String, dynamic>{
      'programName': instance.programName,
      'namespace': instance.namespace,
      'capabilities': instance.capabilities.toJson(),
      'protectedStore': instance.protectedStore.toJson(),
      'tableStore': instance.tableStore.toJson(),
      'blockStore': instance.blockStore.toJson(),
      'network': instance.network.toJson(),
      'attachment': instance.attachment.toJson(),
      'internal': instance.internal?.toJson(),
    };
