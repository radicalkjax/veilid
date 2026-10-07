import 'package:change_case/change_case.dart';
import 'package:flutter/foundation.dart';
import 'package:freezed_annotation/freezed_annotation.dart';

import 'veilid.dart';

part 'veilid_config.freezed.dart';
part 'veilid_config.g.dart';

//////////////////////////////////////////////////////////
// FFI Platform-specific config

/// Native terminal (stdout) logging configuration.
@freezed
sealed class VeilidFFIConfigLoggingTerminal
    with _$VeilidFFIConfigLoggingTerminal {
  const factory VeilidFFIConfigLoggingTerminal({
    @Default(true) bool enabled,
    @Default(VeilidConfigLogLevel.info) VeilidConfigLogLevel level,
    @Default([]) List<String> directives,
    @Deprecated('Use directives instead')
    @Default([])
    List<String> ignoreLogTargets,
  }) = _VeilidFFIConfigLoggingTerminal;

  factory VeilidFFIConfigLoggingTerminal.fromJson(dynamic json) =>
      _$VeilidFFIConfigLoggingTerminalFromJson(json as Map<String, dynamic>);
}

/// Native OpenTelemetry (OTLP/gRPC) logging configuration.
@freezed
sealed class VeilidFFIConfigLoggingOtlp with _$VeilidFFIConfigLoggingOtlp {
  const factory VeilidFFIConfigLoggingOtlp({
    @Default(true) bool enabled,
    @Default(VeilidConfigLogLevel.trace) VeilidConfigLogLevel level,
    required String grpcEndpoint,
    required String serviceName,
    @Default([]) List<String> directives,
    @Deprecated('Use directives instead')
    @Default([])
    List<String> ignoreLogTargets,
  }) = _VeilidFFIConfigLoggingOtlp;

  factory VeilidFFIConfigLoggingOtlp.fromJson(dynamic json) =>
      _$VeilidFFIConfigLoggingOtlpFromJson(json as Map<String, dynamic>);
}

/// Native API logging configuration; routes logs to [VeilidUpdate] events.
@freezed
sealed class VeilidFFIConfigLoggingApi with _$VeilidFFIConfigLoggingApi {
  const factory VeilidFFIConfigLoggingApi({
    @Default(true) bool enabled,
    @Default(VeilidConfigLogLevel.info) VeilidConfigLogLevel level,
    @Default([]) List<String> directives,
    @Deprecated('Use directives instead')
    @Default([])
    List<String> ignoreLogTargets,
  }) = _VeilidFFIConfigLoggingApi;

  factory VeilidFFIConfigLoggingApi.fromJson(dynamic json) =>
      _$VeilidFFIConfigLoggingApiFromJson(json as Map<String, dynamic>);
}

/// Native flamegraph logging configuration; writes timing data to [path].
@freezed
sealed class VeilidFFIConfigLoggingFlame with _$VeilidFFIConfigLoggingFlame {
  const factory VeilidFFIConfigLoggingFlame({
    @Default(true) bool enabled,
    required String path,
  }) = _VeilidFFIConfigLoggingFlame;

  factory VeilidFFIConfigLoggingFlame.fromJson(dynamic json) =>
      _$VeilidFFIConfigLoggingFlameFromJson(json as Map<String, dynamic>);
}

/// Native logging configuration across all sinks.
@freezed
sealed class VeilidFFIConfigLogging with _$VeilidFFIConfigLogging {
  const factory VeilidFFIConfigLogging({
    @Default(VeilidFFIConfigLoggingTerminal(enabled: false))
    VeilidFFIConfigLoggingTerminal terminal,
    @Default(VeilidFFIConfigLoggingApi(enabled: false))
    VeilidFFIConfigLoggingApi api,
    @Default(
      VeilidFFIConfigLoggingOtlp(
        enabled: false,
        grpcEndpoint: '',
        serviceName: '',
      ),
    )
    VeilidFFIConfigLoggingOtlp otlp,
    @Default(VeilidFFIConfigLoggingFlame(enabled: false, path: ''))
    VeilidFFIConfigLoggingFlame flame,
  }) = _VeilidFFIConfigLogging;

  factory VeilidFFIConfigLogging.fromJson(dynamic json) =>
      _$VeilidFFIConfigLoggingFromJson(json as Map<String, dynamic>);
}

/// Platform-specific configuration for the native (FFI) backend.
@freezed
sealed class VeilidFFIConfig with _$VeilidFFIConfig {
  const factory VeilidFFIConfig({required VeilidFFIConfigLogging logging}) =
      _VeilidFFIConfig;

  factory VeilidFFIConfig.fromJson(dynamic json) =>
      _$VeilidFFIConfigFromJson(json as Map<String, dynamic>);
}

//////////////////////////////////////////////////////////
// WASM Platform-specific config

/// Browser console sink for the WASM performance logger.
@freezed
sealed class VeilidWASMConfigLoggingPerformanceConsole
    with _$VeilidWASMConfigLoggingPerformanceConsole {
  const factory VeilidWASMConfigLoggingPerformanceConsole({
    @Default(true) bool enabled,
    @Default(true) bool color,
    @Default(true) bool timestamp,
    String? originBaseUrl,
  }) = _VeilidWASMConfigLoggingPerformanceConsole;

  factory VeilidWASMConfigLoggingPerformanceConsole.fromJson(dynamic json) =>
      _$VeilidWASMConfigLoggingPerformanceConsoleFromJson(
        json as Map<String, dynamic>,
      );
}

/// WASM performance-timeline logging configuration.
@freezed
sealed class VeilidWASMConfigLoggingPerformance
    with _$VeilidWASMConfigLoggingPerformance {
  const factory VeilidWASMConfigLoggingPerformance({
    @Default(true) bool enabled,
    @Default(VeilidConfigLogLevel.info) VeilidConfigLogLevel level,
    @Default(false) bool timings,
    @Default(VeilidWASMConfigLoggingPerformanceConsole(enabled: true))
    VeilidWASMConfigLoggingPerformanceConsole console,
    @Default([]) List<String> directives,
    @Deprecated('Use directives instead')
    @Default([])
    List<String> ignoreLogTargets,
  }) = _VeilidWASMConfigLoggingPerformance;

  factory VeilidWASMConfigLoggingPerformance.fromJson(dynamic json) =>
      _$VeilidWASMConfigLoggingPerformanceFromJson(
        json as Map<String, dynamic>,
      );
}

/// WASM API logging configuration; routes logs to [VeilidUpdate] events.
@freezed
sealed class VeilidWASMConfigLoggingApi with _$VeilidWASMConfigLoggingApi {
  const factory VeilidWASMConfigLoggingApi({
    @Default(true) bool enabled,
    @Default(VeilidConfigLogLevel.info) VeilidConfigLogLevel level,
    @Default([]) List<String> directives,
    @Deprecated('Use directives instead')
    @Default([])
    List<String> ignoreLogTargets,
  }) = _VeilidWASMConfigLoggingApi;

  factory VeilidWASMConfigLoggingApi.fromJson(dynamic json) =>
      _$VeilidWASMConfigLoggingApiFromJson(json as Map<String, dynamic>);
}

/// WASM logging configuration across all sinks.
@freezed
sealed class VeilidWASMConfigLogging with _$VeilidWASMConfigLogging {
  const factory VeilidWASMConfigLogging({
    @Default(VeilidWASMConfigLoggingPerformance(enabled: false))
    VeilidWASMConfigLoggingPerformance performance,
    @Default(VeilidWASMConfigLoggingApi(enabled: false))
    VeilidWASMConfigLoggingApi api,
  }) = _VeilidWASMConfigLogging;

  factory VeilidWASMConfigLogging.fromJson(dynamic json) =>
      _$VeilidWASMConfigLoggingFromJson(json as Map<String, dynamic>);
}

/// Platform-specific configuration for the WASM (browser) backend.
@freezed
sealed class VeilidWASMConfig with _$VeilidWASMConfig {
  const factory VeilidWASMConfig({
    @Default(VeilidWASMConfigLogging()) VeilidWASMConfigLogging logging,
  }) = _VeilidWASMConfig;

  factory VeilidWASMConfig.fromJson(dynamic json) =>
      _$VeilidWASMConfigFromJson(json as Map<String, dynamic>);
}

//////////////////////////////////////
/// VeilidConfigLogLevel

/// Logging level threshold; [off] disables logging.
enum VeilidConfigLogLevel {
  /// Logging disabled.
  off,

  /// Errors only.
  error,

  /// Errors and warnings.
  warn,

  /// Errors, warnings, and informational messages.
  info,

  /// Adds debug-level messages.
  debug,

  /// Adds trace-level messages (most verbose).
  trace;

  /// Parse from its JSON (PascalCase) name.
  factory VeilidConfigLogLevel.fromJson(dynamic j) =>
      VeilidConfigLogLevel.values.byName((j as String).toCamelCase());

  /// The PascalCase JSON name.
  String toJson() => name.toPascalCase();
}

////////////
/// Enable and configure the UDP protocol.
@freezed
sealed class VeilidConfigUDP with _$VeilidConfigUDP {
  const factory VeilidConfigUDP({
    required bool enabled,
    required String listenAddress,
    String? publicAddress,
  }) = _VeilidConfigUDP;

  factory VeilidConfigUDP.fromJson(dynamic json) =>
      _$VeilidConfigUDPFromJson(json as Map<String, dynamic>);
}

////////////
/// Enable and configure the TCP protocol.
@freezed
sealed class VeilidConfigTCP with _$VeilidConfigTCP {
  const factory VeilidConfigTCP({
    required bool connect,
    required bool listen,
    required String listenAddress,
    String? publicAddress,
  }) = _VeilidConfigTCP;

  factory VeilidConfigTCP.fromJson(dynamic json) =>
      _$VeilidConfigTCPFromJson(json as Map<String, dynamic>);
}

////////////
/// Enable and configure the WebSocket protocol.
@freezed
sealed class VeilidConfigWS with _$VeilidConfigWS {
  const factory VeilidConfigWS({
    required bool connect,
    required bool listen,
    required String listenAddress,
    required String path,
    String? url,
  }) = _VeilidConfigWS;

  factory VeilidConfigWS.fromJson(dynamic json) =>
      _$VeilidConfigWSFromJson(json as Map<String, dynamic>);
}

////////////
// @Deprecated('WSS is disabled by default in veilid-flutter')
// @freezed
// sealed class VeilidConfigWSS with _$VeilidConfigWSS {
//   @Deprecated('WSS is disabled by default in veilid-flutter')
//   const factory VeilidConfigWSS({
//     required bool connect,
//     required bool listen,
//     required int maxConnections,
//     required String listenAddress,
//     required String path,
//     String? url,
//   }) = _VeilidConfigWSS;

//   @Deprecated('WSS is disabled by default in veilid-flutter')
//   factory VeilidConfigWSS.fromJson(dynamic json) =>
//       _$VeilidConfigWSSFromJson(json as Map<String, dynamic>);
// }

////////////

/// Per-protocol (UDP/TCP/WebSocket) network configuration. All protocols are
/// available by default; the node selects which to use per peer.
@freezed
sealed class VeilidConfigProtocol with _$VeilidConfigProtocol {
  const factory VeilidConfigProtocol({
    required VeilidConfigUDP udp,
    required VeilidConfigTCP tcp,
    required VeilidConfigWS ws,
    // required VeilidConfigWSS wss,
  }) = _VeilidConfigProtocol;

  factory VeilidConfigProtocol.fromJson(dynamic json) =>
      _$VeilidConfigProtocolFromJson(json as Map<String, dynamic>);
}

////////////

/// Privacy and relay preferences for routes.
@freezed
sealed class VeilidConfigPrivacy with _$VeilidConfigPrivacy {
  const factory VeilidConfigPrivacy({required bool requireInboundRelay}) =
      _VeilidConfigPrivacy;

  factory VeilidConfigPrivacy.fromJson(dynamic json) =>
      _$VeilidConfigPrivacyFromJson(json as Map<String, dynamic>);
}

////////////

/// TLS configuration for inbound secure protocols.
@freezed
sealed class VeilidConfigTLS with _$VeilidConfigTLS {
  const factory VeilidConfigTLS({
    required String certificatePath,
    required String privateKeyPath,
    required int connectionInitialTimeoutMs,
  }) = _VeilidConfigTLS;

  factory VeilidConfigTLS.fromJson(dynamic json) =>
      _$VeilidConfigTLSFromJson(json as Map<String, dynamic>);
}

////////////
/// Distributed Hash Table (DHT) cache and storage configuration. Use the
/// defaults unless you are sure; bad count/fanout/timeout values can render a
/// node inoperable for DHT operations.
@freezed
sealed class VeilidConfigDHT with _$VeilidConfigDHT {
  const factory VeilidConfigDHT({
    required int localSubkeyCacheSize,
    required int localMaxSubkeyCacheMemoryMb,
    required int remoteSubkeyCacheSize,
    required int remoteMaxRecords,
    required int remoteMaxSubkeyCacheMemoryMb,
    required int remoteMaxStorageSpaceMb,
    required int maxConcurrentOperations,
  }) = _VeilidConfigDHT;

  factory VeilidConfigDHT.fromJson(dynamic json) =>
      _$VeilidConfigDHTFromJson(json as Map<String, dynamic>);
}

////////////

/// RPC configuration.
@freezed
sealed class VeilidConfigRPC with _$VeilidConfigRPC {
  const factory VeilidConfigRPC({
    required int defaultRouteHopCount,
  }) = _VeilidConfigRPC;

  factory VeilidConfigRPC.fromJson(dynamic json) =>
      _$VeilidConfigRPCFromJson(json as Map<String, dynamic>);
}

////////////

/// Routing table identity and bootstrap configuration.
@freezed
sealed class VeilidConfigRoutingTable with _$VeilidConfigRoutingTable {
  const factory VeilidConfigRoutingTable({
    required List<PublicKey> publicKeys,
    required List<SecretKey> secretKeys,
    required List<String> bootstrap,
    required List<PublicKey> bootstrapKeys,
  }) = _VeilidConfigRoutingTable;

  factory VeilidConfigRoutingTable.fromJson(dynamic json) =>
      _$VeilidConfigRoutingTableFromJson(json as Map<String, dynamic>);
}

////////////

//////////////////////////////////////
/// VeilidConfigAddressType

/// An IP address family the node may use.
enum VeilidConfigAddressType {
  /// IPv4 (32-bit) addresses.
  ipv4,

  /// IPv6 (128-bit) addresses.
  ipv6;

  /// Parse from its JSON name (`IPV4`/`IPV6`).
  factory VeilidConfigAddressType.fromJson(dynamic j) => switch (j as String) {
        'IPV4' => VeilidConfigAddressType.ipv4,
        'IPV6' => VeilidConfigAddressType.ipv6,
        _ =>
          throw ArgumentError.value(j, 'j', 'invalid VeilidConfigAddressType'),
      };

  /// The JSON name (`IPV4`/`IPV6`).
  String toJson() => switch (this) {
        VeilidConfigAddressType.ipv4 => 'IPV4',
        VeilidConfigAddressType.ipv6 => 'IPV6',
      };
}

////////////

/// Network subsystem configuration: connections, routing table, RPC, DHT,
/// transports, and privacy.
@freezed
sealed class VeilidConfigNetwork with _$VeilidConfigNetwork {
  const factory VeilidConfigNetwork({
    required int maxConnections,
    required VeilidConfigRoutingTable routingTable,
    required VeilidConfigRPC rpc,
    required VeilidConfigDHT dht,
    @Default(<VeilidConfigAddressType>[])
    List<VeilidConfigAddressType> addressTypes,
    required bool upnp,
    required bool? detectAddressChanges,
    required VeilidConfigTLS tls,
    required VeilidConfigProtocol protocol,
    required VeilidConfigPrivacy privacy,
    String? networkKeyPassword,
  }) = _VeilidConfigNetwork;

  factory VeilidConfigNetwork.fromJson(dynamic json) =>
      _$VeilidConfigNetworkFromJson(json as Map<String, dynamic>);
}

////////////

/// Table store configuration: the encrypted key-value database backing node
/// state.
@freezed
sealed class VeilidConfigTableStore with _$VeilidConfigTableStore {
  const factory VeilidConfigTableStore({
    required String directory,
    required bool delete,
    required bool wipeOnInvalidDeviceEncryptionKey,
    required int maxValueSizeMb,
  }) = _VeilidConfigTableStore;

  factory VeilidConfigTableStore.fromJson(dynamic json) =>
      _$VeilidConfigTableStoreFromJson(json as Map<String, dynamic>);
}

////////////

/// Block store configuration: content-addressed block storage.
@freezed
sealed class VeilidConfigBlockStore with _$VeilidConfigBlockStore {
  const factory VeilidConfigBlockStore({
    required String directory,
    required bool delete,
  }) = _VeilidConfigBlockStore;

  factory VeilidConfigBlockStore.fromJson(dynamic json) =>
      _$VeilidConfigBlockStoreFromJson(json as Map<String, dynamic>);
}

////////////

/// Protected store configuration: where secrets such as the device encryption
/// key are kept (keychain/keyring/etc).
@freezed
sealed class VeilidConfigProtectedStore with _$VeilidConfigProtectedStore {
  const factory VeilidConfigProtectedStore({
    required bool allowInsecureFallback,
    required bool alwaysUseInsecureStorage,
    required String directory,
    required bool delete,
    required String deviceEncryptionKeyPassword,
    String? newDeviceEncryptionKeyPassword,
  }) = _VeilidConfigProtectedStore;

  factory VeilidConfigProtectedStore.fromJson(dynamic json) =>
      _$VeilidConfigProtectedStoreFromJson(json as Map<String, dynamic>);
}

////////////

/// Capabilities advertised by this node.
@freezed
sealed class VeilidConfigCapabilities with _$VeilidConfigCapabilities {
  const factory VeilidConfigCapabilities({required List<String> disable}) =
      _VeilidConfigCapabilities;

  factory VeilidConfigCapabilities.fromJson(dynamic json) =>
      _$VeilidConfigCapabilitiesFromJson(json as Map<String, dynamic>);
}

////////////
// "Footgun" internal tuning tree, parallel to the main config. Only honored when
// veilid-core is built with the `footgun-config` feature; otherwise ignored.

/// Internal "footgun" UDP tuning. See [VeilidConfigInternal].
@freezed
sealed class VeilidConfigInternalUDP with _$VeilidConfigInternalUDP {
  const factory VeilidConfigInternalUDP({
    required int socketPoolSize,
  }) = _VeilidConfigInternalUDP;

  factory VeilidConfigInternalUDP.fromJson(dynamic json) =>
      _$VeilidConfigInternalUDPFromJson(json as Map<String, dynamic>);
}

////////////
/// Internal "footgun" per-protocol tuning. See [VeilidConfigInternal].
@freezed
sealed class VeilidConfigInternalProtocol with _$VeilidConfigInternalProtocol {
  const factory VeilidConfigInternalProtocol({
    required VeilidConfigInternalUDP udp,
  }) = _VeilidConfigInternalProtocol;

  factory VeilidConfigInternalProtocol.fromJson(dynamic json) =>
      _$VeilidConfigInternalProtocolFromJson(json as Map<String, dynamic>);
}

////////////
/// Internal "footgun" RPC tuning. See [VeilidConfigInternal].
@freezed
sealed class VeilidConfigInternalRPC with _$VeilidConfigInternalRPC {
  const factory VeilidConfigInternalRPC({
    required int concurrency,
    required int queueSize,
    required int timeoutMs,
    required int maxRouteHopCount,
    int? maxTimestampBehindMs,
    int? maxTimestampAheadMs,
  }) = _VeilidConfigInternalRPC;

  factory VeilidConfigInternalRPC.fromJson(dynamic json) =>
      _$VeilidConfigInternalRPCFromJson(json as Map<String, dynamic>);
}

////////////
/// Internal "footgun" DHT tuning. See [VeilidConfigInternal]. Changing the
/// count/fanout/timeout fields may render a node inoperable for DHT operations.
@freezed
sealed class VeilidConfigInternalDHT with _$VeilidConfigInternalDHT {
  const factory VeilidConfigInternalDHT({
    required int maxFindNodeCount,
    required int resolveNodeTimeoutMs,
    required int resolveNodeCount,
    required int resolveNodeFanout,
    required int getValueTimeoutMs,
    required int getValueCount,
    required int getValueFanout,
    required int setValueTimeoutMs,
    required int setValueCount,
    required int setValueFanout,
    required int consensusWidth,
    required int minPeerCount,
    required int minPeerRefreshTimeMs,
    required int validateDialInfoReceiptTimeMs,
    required int maxWatchExpirationMs,
    required int publicWatchLimit,
    required int memberWatchLimit,
    required int publicTransactionLimit,
    required int memberTransactionLimit,
  }) = _VeilidConfigInternalDHT;

  factory VeilidConfigInternalDHT.fromJson(dynamic json) =>
      _$VeilidConfigInternalDHTFromJson(json as Map<String, dynamic>);
}

////////////
/// Internal "footgun" network tuning. See [VeilidConfigInternal].
@freezed
sealed class VeilidConfigInternalNetwork with _$VeilidConfigInternalNetwork {
  const factory VeilidConfigInternalNetwork({
    required int connectionInitialTimeoutMs,
    required int connectionInactivityTimeoutMs,
    required int maxConnectionsPerIp4,
    required int maxConnectionsPerIp6Prefix,
    required int maxConnectionsPerIp6PrefixSize,
    required int maxConnectionFrequencyPerMin,
    required int clientAllowlistTimeoutMs,
    required int reverseConnectionReceiptTimeMs,
    required int holePunchReceiptTimeMs,
    required int restrictedNatRetries,
    required VeilidConfigInternalRPC rpc,
    required VeilidConfigInternalDHT dht,
    required VeilidConfigInternalProtocol protocol,
  }) = _VeilidConfigInternalNetwork;

  factory VeilidConfigInternalNetwork.fromJson(dynamic json) =>
      _$VeilidConfigInternalNetworkFromJson(json as Map<String, dynamic>);
}

////////////
/// Internal "footgun" tuning tree, parallel to the main config. Tunes low-level
/// network/DHT timing, fanout, consensus, and connection limits. Only honored
/// when veilid-core is built with the `footgun-config` feature; otherwise any
/// non-default values are reset at startup (with a warning).
@freezed
sealed class VeilidConfigInternal with _$VeilidConfigInternal {
  const factory VeilidConfigInternal({
    required VeilidConfigInternalNetwork network,
  }) = _VeilidConfigInternal;

  factory VeilidConfigInternal.fromJson(dynamic json) =>
      _$VeilidConfigInternalFromJson(json as Map<String, dynamic>);
}

////////////

/// Top level of the Veilid configuration tree, passed to startup.
@freezed
sealed class VeilidConfig with _$VeilidConfig {
  const factory VeilidConfig({
    required String programName,
    required String namespace,
    required VeilidConfigCapabilities capabilities,
    required VeilidConfigProtectedStore protectedStore,
    required VeilidConfigTableStore tableStore,
    required VeilidConfigBlockStore blockStore,
    required VeilidConfigNetwork network,
    VeilidConfigInternal? internal,
  }) = _VeilidConfig;

  factory VeilidConfig.fromJson(dynamic json) =>
      _$VeilidConfigFromJson(json as Map<String, dynamic>);
}
