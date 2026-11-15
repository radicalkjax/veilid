import 'dart:async';

import 'veilid.dart';

/// Abstraction over Veilid attachment + network state.
///
/// Production app code extends [DefaultVeilidProcessor] against the live
/// Veilid attachment state (see veilidchat's `VeilidProcessorRepository`).
/// Integration-test harnesses extend it too, backed by their fixture-side
/// connection-state stream, so code under test doesn't depend on the
/// singletons the production app stands up.
abstract interface class VeilidProcessorInterface {
  /// Latest snapshot of the Veilid attachment + network state.
  ProcessorConnectionState get processorConnectionState;

  /// Stream of [ProcessorConnectionState] updates. Each implementation decides
  /// whether to emit on every update or only on changes; treat values as
  /// current-state snapshots.
  Stream<ProcessorConnectionState> streamProcessorConnectionState();

  /// Complete once the node is attached with PublicInternet ready. Throws
  /// [TimeoutException] if [timeout] elapses first.
  Future<void> waitPublicInternetReady({Duration? timeout});

  /// Wait until ready; if already ready, wait [fallbackDelay] instead so a
  /// retry that failed despite being online doesn't hammer.
  Future<void> waitReadyOrDelay([Duration? fallbackDelay]);
}

/// A [VeilidProcessorInterface] whose readiness waits ask core rather than
/// [processorConnectionState], which holds what core last reported and is
/// stale after the process was frozen.
abstract class DefaultVeilidProcessor implements VeilidProcessorInterface {
  @override
  Future<void> waitPublicInternetReady({Duration? timeout}) async {
    try {
      await Veilid.instance.waitForAttachmentCondition(timeout: timeout);
    } on VeilidAPIExceptionTimeout {
      throw TimeoutException('PublicInternet not ready', timeout);
    }
  }

  @override
  Future<void> waitReadyOrDelay([Duration? fallbackDelay]) async {
    final state = await Veilid.instance.getVeilidState();
    if (state.attachment.publicInternetReady) {
      if (fallbackDelay != null) {
        await Future<void>.delayed(fallbackDelay);
      }
    } else {
      await waitPublicInternetReady();
    }
  }
}
