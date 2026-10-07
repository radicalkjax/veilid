// Allow environment variables
// ignore_for_file: do_not_use_environment, avoid_redundant_argument_values

import 'dart:async';
import 'dart:io' show Platform;

import 'package:async_tools/async_tools.dart';
import 'package:path/path.dart' as p;
import 'package:veilid/veilid.dart';

import 'log_fixture.dart';

const kIsWeb = bool.fromEnvironment('dart.library.js_util');

abstract class VeilidFixture {
  Future<void> setUp();
  Future<void> tearDown();
  Future<void> attach();
  Future<void> detach();
  Stream<VeilidUpdate> get updateStream;
}

class DefaultVeilidFixture implements VeilidFixture {
  StreamSubscription<VeilidUpdate>? _veilidUpdateSubscription;

  Stream<VeilidUpdate>? _veilidUpdateStream;

  late final StreamController<VeilidUpdate> _updateStreamController;

  static final _fixtureMutex = Mutex();

  final String programName;
  final LogFixture? logFixture;

  DefaultVeilidFixture({required this.programName, this.logFixture});

  @override
  Future<void> setUp() async {
    await _fixtureMutex.acquire();
    assert(_veilidUpdateStream == null, 'should not set up fixture twice');

    _updateStreamController = StreamController.broadcast();

    final ignoreLogTargetsStr = const String.fromEnvironment(
      'IGNORE_LOG_TARGETS',
    ).trim();
    final logDirectivesStr = const String.fromEnvironment(
      'LOG_DIRECTIVES',
    ).trim();
    final ignoreLogTargets = ignoreLogTargetsStr.isEmpty
        ? <String>[]
        : ignoreLogTargetsStr.split(',').map((e) => e.trim()).toList();
    final logDirectives = logDirectivesStr.isEmpty
        ? <String>[]
        : logDirectivesStr.split(',').map((e) => e.trim()).toList();

    final logLevel = VeilidConfigLogLevel.fromJson(
      const String.fromEnvironment('LOG_LEVEL', defaultValue: 'info'),
    );

    final terminalLogLevel = VeilidConfigLogLevel.fromJson(
      const String.fromEnvironment('TERMINAL_LOG_LEVEL', defaultValue: 'off'),
    );

    final flamePathStr = const String.fromEnvironment('FLAME').trim();
    final otlpEndpoint = const String.fromEnvironment(
      'OTLP_GRPC_ENDPOINT',
      defaultValue: 'localhost:4317',
    ).trim();
    final otlpEnabled = const String.fromEnvironment('OTLP').trim() == '1';
    // OTLP gets its own directive list so the user can keep api-log scope
    // broad (e.g. dht=debug for stdout) while emitting only the spans they
    // want over OTLP. Empty defaults to off-level + no directives, which
    // emits no spans.
    final otlpDirectivesStr = const String.fromEnvironment(
      'OTLP_DIRECTIVES',
    ).trim();
    final otlpDirectives = otlpDirectivesStr.isEmpty
        ? <String>[]
        : otlpDirectivesStr.split(',').map((e) => e.trim()).toList();

    final Map<String, dynamic> platformConfigJson;
    if (kIsWeb) {
      final platformConfig = VeilidWASMConfig(
        logging: VeilidWASMConfigLogging(
          performance: VeilidWASMConfigLoggingPerformance(
            enabled: true,
            level: logLevel,
            directives: logDirectives,
            // pass through for backwards compatibility
            // ignore: deprecated_member_use
            ignoreLogTargets: ignoreLogTargets,
          ),
          api: VeilidWASMConfigLoggingApi(
            enabled: true,
            level: logLevel,
            directives: logDirectives,
            // pass through for backwards compatibility
            // ignore: deprecated_member_use
            ignoreLogTargets: ignoreLogTargets,
          ),
        ),
      );
      platformConfigJson = platformConfig.toJson();
    } else {
      final platformConfig = VeilidFFIConfig(
        logging: VeilidFFIConfigLogging(
          terminal: VeilidFFIConfigLoggingTerminal(
            enabled: terminalLogLevel != VeilidConfigLogLevel.off,
            level: terminalLogLevel,
            directives: logDirectives,
            // pass through for backwards compatibility
            // ignore: deprecated_member_use
            ignoreLogTargets: ignoreLogTargets,
          ),
          api: VeilidFFIConfigLoggingApi(
            enabled: true,
            level: logLevel,
            directives: logDirectives,
            // pass through for backwards compatibility
            // ignore: deprecated_member_use
            ignoreLogTargets: ignoreLogTargets,
          ),
          otlp: VeilidFFIConfigLoggingOtlp(
            enabled: otlpEnabled,
            level: VeilidConfigLogLevel.off,
            grpcEndpoint: otlpEndpoint,
            serviceName: 'Veilid Tests',
            directives: otlpDirectives,
            // pass through for backwards compatibility
            // ignore: deprecated_member_use
            ignoreLogTargets: ignoreLogTargets,
          ),
          flame: VeilidFFIConfigLoggingFlame(
            enabled: flamePathStr.isNotEmpty,
            path: flamePathStr,
          ),
        ),
      );
      platformConfigJson = platformConfig.toJson();
    }

    Veilid.instance.initializeVeilidCore(platformConfigJson);

    var config = await getDefaultVeilidConfig(
      isWeb: kIsWeb,
      programName: programName,
      namespace: const String.fromEnvironment('NAMESPACE'),
      bootstrap: const String.fromEnvironment('BOOTSTRAP'),
      bootstrapKeys: const bool.hasEnvironment('BOOTSTRAP_KEYS')
          ? const String.fromEnvironment('BOOTSTRAP_KEYS')
          : null,
      networkKeyPassword: const String.fromEnvironment('NETWORK_KEY'),
    );

    // Isolate per-programName stores from the host app's stores.
    if (!kIsWeb) {
      config = config.copyWith(
        tableStore: config.tableStore.copyWith(
          directory: p.join(config.tableStore.directory, programName),
          delete: true,
        ),
        protectedStore: config.protectedStore.copyWith(
          directory: p.join(config.protectedStore.directory, programName),
          delete: true,
        ),
        blockStore: config.blockStore.copyWith(
          directory: p.join(config.blockStore.directory, programName),
          delete: true,
        ),
      );
    } else {
      config = config.copyWith(
        tableStore: config.tableStore.copyWith(delete: true),
        protectedStore: config.protectedStore.copyWith(delete: true),
        blockStore: config.blockStore.copyWith(delete: true),
      );
    }
    config = config.copyWith(
      capabilities:
          // XXX: Remove after https://gitlab.com/veilid/veilid/-/issues/492
          const VeilidConfigCapabilities(disable: ['DHTV']),
      protectedStore:
          // Linux often does not have a secret storage mechanism installed
          config.protectedStore.copyWith(
            allowInsecureFallback: !kIsWeb && Platform.isLinux,
          ),
    );

    final us = _veilidUpdateStream = await Veilid.instance.startupVeilidCore(
      config,
    );

    _veilidUpdateSubscription = us.listen((update) {
      if (update is VeilidLog) {
        logFixture?.processVeilidLog(update);
      } else if (update is VeilidUpdateAttachment) {
      } else if (update is VeilidUpdateConfig) {
      } else if (update is VeilidUpdateNetwork) {
      } else if (update is VeilidAppMessage) {
      } else if (update is VeilidAppCall) {
      } else if (update is VeilidUpdateValueChange) {
      } else if (update is VeilidUpdateRouteChange) {
      } else {
        throw Exception('unexpected update: $update');
      }
      _updateStreamController.sink.add(update);
    });
  }

  @override
  Stream<VeilidUpdate> get updateStream => _updateStreamController.stream;

  @override
  Future<void> attach() async {
    await Veilid.instance.attach();

    // Wait for attached state
    while (true) {
      final state = await Veilid.instance.getVeilidState();
      var done = false;
      if (state.attachment.publicInternetReady) {
        switch (state.attachment.state) {
          case AttachmentState.detached:
            break;
          case AttachmentState.attaching:
            break;
          case AttachmentState.detaching:
            break;
          case AttachmentState.attachedWeak:
          case AttachmentState.attachedFair:
          case AttachmentState.attachedGood:
          case AttachmentState.attachedStrong:
          case AttachmentState.attachedFull:
            done = true;
        }
      }
      if (done) {
        logFixture?.debug("Veilid attached: ${state.attachment}");
        break;
      }
      await Future<void>.delayed(const Duration(milliseconds: 250));
    }
  }

  @override
  Future<void> detach() async {
    await Veilid.instance.detach();

    // Wait for detached state
    while (true) {
      final state = await Veilid.instance.getVeilidState();
      var done = false;
      switch (state.attachment.state) {
        case AttachmentState.detached:
          done = true;
          break;
        case AttachmentState.attaching:
        case AttachmentState.detaching:
        case AttachmentState.attachedWeak:
        case AttachmentState.attachedFair:
        case AttachmentState.attachedGood:
        case AttachmentState.attachedStrong:
        case AttachmentState.attachedFull:
          break;
      }
      if (done) {
        logFixture?.debug("Veilid detached: ${state.attachment}");
        break;
      }
      await Future<void>.delayed(const Duration(milliseconds: 250));
    }
  }

  @override
  Future<void> tearDown() async {
    assert(_fixtureMutex.isLocked, 'should not tearDown without setUp');

    await Veilid.instance.shutdownVeilidCore();
    await _veilidUpdateSubscription?.cancel();
    await _updateStreamController.close();

    _veilidUpdateSubscription = null;
    _veilidUpdateStream = null;

    _fixtureMutex.release();
  }
}
