import 'dart:async';

import 'processor_connection_state.dart';
import 'retry.dart';

/// Abstraction over Veilid attachment + network state.
///
/// Production app code implements this against the live Veilid attachment
/// state (see veilidchat's `VeilidProcessorRepository`). Integration-test
/// harnesses provide their own implementation backed by their fixture-side
/// connection-state stream so code under test doesn't depend on the
/// singletons the production app stands up.
abstract class VeilidProcessorInterface {
  /// Latest snapshot of the Veilid attachment + network state.
  ProcessorConnectionState get processorConnectionState;

  /// Stream of [ProcessorConnectionState] updates. Each implementation decides
  /// whether to emit on every update or only on changes; treat values as
  /// current-state snapshots.
  Stream<ProcessorConnectionState> streamProcessorConnectionState();
}

/// Network-aware retry, shared across every [VeilidProcessorInterface].
extension VeilidProcessorRetry on VeilidProcessorInterface {
  /// Complete once `publicInternetReady` is (or becomes) true.
  Future<void> waitPublicInternetReady({Duration? timeout}) async {
    if (processorConnectionState.isPublicInternetReady) {
      return;
    }
    var stream = streamProcessorConnectionState()
        .where((s) => s.isPublicInternetReady);
    if (timeout != null) {
      stream = stream.timeout(timeout);
    }
    await stream.first;
  }

  /// Wait until ready; if already ready, wait [fallbackDelay] instead so a
  /// retry that failed despite being online doesn't hammer.
  Future<void> waitReadyOrDelay([Duration? fallbackDelay]) async {
    if (processorConnectionState.isPublicInternetReady) {
      if (fallbackDelay != null) {
        await Future<void>.delayed(fallbackDelay);
      }
    } else {
      await waitPublicInternetReady();
    }
  }

  /// Run [closure], retrying VeilidAPI transients per [strategy] (default
  /// [VeilidAPIRetryStrategy], waiting on network readiness before a tryAgain
  /// retry).
  Future<T> retry<T>(
    Future<T> Function() closure, {
    RetryStrategy? strategy,
  }) =>
      (strategy ??
              VeilidAPIRetryStrategy(
                tryAgainRetry: (attemptNumber) async {
                  if (attemptNumber >= 3) {
                    return false;
                  }
                  await waitReadyOrDelay(const Duration(milliseconds: 250));
                  return true;
                },
              ))
          .retry(closure);
}
