// Allow environment variables
// ignore_for_file: do_not_use_environment

import 'dart:async';
import 'dart:io' show Platform;

import 'package:flutter_test/flutter_test.dart';
import 'package:path/path.dart' as p;
import 'package:veilid/veilid.dart';
import 'package:veilid_test/veilid_test.dart';

const _programName = 'veilid_flutter startup test';
const _updateTimeout = Duration(seconds: 60);

Future<VeilidConfig> _testConfig(String programName) async {
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

  return config.copyWith(
    capabilities:
        // XXX: Remove after https://gitlab.com/veilid/veilid/-/issues/492
        const VeilidConfigCapabilities(disable: ['DHTV']),
    protectedStore: config.protectedStore.copyWith(
      // Linux often does not have a secret storage mechanism installed
      allowInsecureFallback: !kIsWeb && Platform.isLinux,
    ),
  );
}

/// Keeps an update stream subscribed for as long as its reference is held.
///
/// Cancelling a subscription drops its sink, so a test that wants to release a
/// reference through shutdownVeilidCore must not cancel first.
class _UpdateCollector {
  _UpdateCollector(Stream<VeilidUpdate> stream) {
    _subscription = stream.listen((update) {
      if (update is VeilidUpdateAttachment && !_attachment.isCompleted) {
        _attachment.complete(update);
      }
      if (_pending != null && --_pendingCount <= 0) {
        _pending!.complete();
        _pending = null;
      }
    }, onDone: _done.complete);
  }

  final _done = Completer<void>();
  final _attachment = Completer<VeilidUpdateAttachment>();
  late final StreamSubscription<VeilidUpdate> _subscription;
  Completer<void>? _pending;
  int _pendingCount = 0;

  Future<void> get done => _done.future;
  Future<VeilidUpdateAttachment> get firstAttachment => _attachment.future;
  bool get isDone => _done.isCompleted;

  /// Completes once [count] more updates have arrived.
  Future<void> nextUpdates(int count) {
    final pending = _pending = Completer<void>();
    _pendingCount = count;
    return pending.future;
  }

  /// The generator only resumes to run its cleanup when the next update
  /// arrives, so this hangs unless updates are already flowing.
  Future<void> cancel() => _subscription.cancel();
}

class TestStartupShutdown {
  late final VeilidConfig config;
  late final VeilidConfig otherConfig;

  Future<void> setUpAll() async {
    config = await _testConfig(_programName);
    otherConfig = await _testConfig('$_programName other');
  }

  /// Release anything a failed test left running, so the next test starts from
  /// no node at all.
  Future<void> tearDown() async {
    for (var i = 0; i < 8 && !await Veilid.instance.isShutdown(); i++) {
      try {
        await Veilid.instance.shutdownVeilidCore();
      } on VeilidAPIExceptionNotInitialized {
        break;
      }
    }
    expect(await Veilid.instance.isShutdown(), isTrue);
  }

  Future<void> testTwoStartupsOneShutdownKeepsNodeAlive() async {
    final firstStream = await Veilid.instance.startupVeilidCore(config);
    final secondStream = await Veilid.instance.startupVeilidCore(config);
    final first = _UpdateCollector(firstStream);
    final second = _UpdateCollector(secondStream);

    expect(await Veilid.instance.isShutdown(), isFalse);

    // retires the second reference only
    await Veilid.instance.shutdownVeilidCore();
    expect(await Veilid.instance.isShutdown(), isFalse);
    await second.done.timeout(_updateTimeout);
    expect(first.isDone, isFalse);

    // the node is still usable through the surviving reference
    final state = await Veilid.instance.getVeilidState();
    expect(state.attachment.state, equals(AttachmentState.detached));

    await Veilid.instance.shutdownVeilidCore();
    expect(await Veilid.instance.isShutdown(), isTrue);
    await first.done.timeout(_updateTimeout);
  }

  Future<void> testJoinedCallerReceivesUpdates() async {
    final firstStream = await Veilid.instance.startupVeilidCore(config);
    final secondStream = await Veilid.instance.startupVeilidCore(config);
    final first = _UpdateCollector(firstStream);
    final second = _UpdateCollector(secondStream);

    await Veilid.instance.attach();
    // each joined caller attaches the node it shares
    await Veilid.instance.attach();
    expect(
      await first.firstAttachment.timeout(_updateTimeout),
      isA<VeilidUpdateAttachment>(),
    );
    expect(
      await second.firstAttachment.timeout(_updateTimeout),
      isA<VeilidUpdateAttachment>(),
    );
    await Veilid.instance.detach();
    await Veilid.instance.detach();

    await Veilid.instance.shutdownVeilidCore();
    await Veilid.instance.shutdownVeilidCore();
    await first.done.timeout(_updateTimeout);
    await second.done.timeout(_updateTimeout);
  }

  Future<void> testFailedStartupTakesNoReference() async {
    final invalidConfig = config.copyWith(programName: '$_programName/invalid');
    await expectLater(
      () => Veilid.instance.startupVeilidCore(invalidConfig),
      throwsA(isA<VeilidAPIException>()),
    );

    // one reference, so one shutdown must take the node down
    final stream = await Veilid.instance.startupVeilidCore(config);
    final collector = _UpdateCollector(stream);
    await Veilid.instance.shutdownVeilidCore();
    expect(await Veilid.instance.isShutdown(), isTrue);
    await collector.done.timeout(_updateTimeout);
  }

  Future<void> testDifferentInitKeyIsRejected() async {
    final stream = await Veilid.instance.startupVeilidCore(config);
    final collector = _UpdateCollector(stream);

    await expectLater(
      () => Veilid.instance.startupVeilidCore(otherConfig),
      throwsA(isA<VeilidAPIExceptionAlreadyInitialized>()),
    );
    expect(collector.isDone, isFalse);

    // the rejected caller took no reference
    await Veilid.instance.shutdownVeilidCore();
    expect(await Veilid.instance.isShutdown(), isTrue);
    await collector.done.timeout(_updateTimeout);
  }

  Future<void> testExtraShutdownsDoNotUnderflow() async {
    await expectLater(
      Veilid.instance.shutdownVeilidCore,
      throwsA(isA<VeilidAPIExceptionNotInitialized>()),
    );

    final stream = await Veilid.instance.startupVeilidCore(config);
    final collector = _UpdateCollector(stream);
    await Veilid.instance.shutdownVeilidCore();
    expect(await Veilid.instance.isShutdown(), isTrue);
    await collector.done.timeout(_updateTimeout);

    await expectLater(
      Veilid.instance.shutdownVeilidCore,
      throwsA(isA<VeilidAPIExceptionNotInitialized>()),
    );
    expect(await Veilid.instance.isShutdown(), isTrue);

    // the node comes back after the extra shutdowns
    final restarted = await Veilid.instance.startupVeilidCore(config);
    final restartedCollector = _UpdateCollector(restarted);
    expect(await Veilid.instance.isShutdown(), isFalse);
    await Veilid.instance.shutdownVeilidCore();
    await restartedCollector.done.timeout(_updateTimeout);
  }

  Future<void> testReapedSinkHoldsNoReference() async {
    final firstStream = await Veilid.instance.startupVeilidCore(config);
    final secondStream = await Veilid.instance.startupVeilidCore(config);
    final first = _UpdateCollector(firstStream);
    final second = _UpdateCollector(secondStream);

    await Veilid.instance.attach();
    await second.firstAttachment.timeout(_updateTimeout);

    // cancelling drops the first sink: native reaps the dead port on a later
    // post, web deregisters it from onCancel. The parked generator resumes
    // only on an update and the mock emits none unprompted, so drive the flow
    // with detach/attach transitions instead of waiting on ambient updates.
    final cancelled = first.cancel();
    await Veilid.instance.detach();
    await Veilid.instance.attach();
    await cancelled.timeout(_updateTimeout);

    // the cancelled port is closed now, so these posts fail on it and reap it
    final delivered = second.nextUpdates(2);
    await Veilid.instance.detach();
    await Veilid.instance.attach();
    await delivered.timeout(_updateTimeout);

    // reaping a dead sink is not a shutdown
    expect(await Veilid.instance.isShutdown(), isFalse);
    await Veilid.instance.detach();

    // the reaped sink holds no reference, so the survivor is the last one
    await Veilid.instance.shutdownVeilidCore();
    expect(await Veilid.instance.isShutdown(), isTrue);
    await second.done.timeout(_updateTimeout);

    // the reaped caller's own release finds nothing left to shut down
    await expectLater(
      Veilid.instance.shutdownVeilidCore,
      throwsA(isA<VeilidAPIExceptionNotInitialized>()),
    );
  }

  Future<void> testReportAppLifecycleState() async {
    final updates = _UpdateCollector(
      await Veilid.instance.startupVeilidCore(config),
    );

    await Veilid.instance.reportAppLifecycleState(
      VeilidAppLifecycleState.background,
    );
    await Veilid.instance.reportAppLifecycleState(
      VeilidAppLifecycleState.active,
    );

    await Veilid.instance.shutdownVeilidCore();
    await updates.done.timeout(_updateTimeout);
  }

  Future<void> testWaitForAttachmentCondition() async {
    final updates = _UpdateCollector(
      await Veilid.instance.startupVeilidCore(config),
    );

    final detached = await Veilid.instance.waitForAttachmentCondition(
      condition: const VeilidStateAttachmentCondition(
        optMaxState: AttachmentState.detached,
      ),
    );
    expect(detached.state, equals(AttachmentState.detached));

    // never attached, so the default condition cannot hold
    await expectLater(
      Veilid.instance.waitForAttachmentCondition(
        timeout: const Duration(milliseconds: 500),
      ),
      throwsA(isA<VeilidAPIExceptionTimeout>()),
    );

    await expectLater(
      Veilid.instance.waitForAttachmentCondition(
        condition: const VeilidStateAttachmentCondition(
          optMinState: AttachmentState.attachedFull,
          optMaxState: AttachmentState.attachedWeak,
        ),
      ),
      throwsA(isA<VeilidAPIExceptionInvalidArgument>()),
    );

    await Veilid.instance.shutdownVeilidCore();
    await updates.done.timeout(_updateTimeout);
  }
}
