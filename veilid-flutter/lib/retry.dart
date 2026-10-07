import 'dart:async';

import 'veilid_api_exception.dart';

/// Decides, after a matching failure on its [attemptNumber]th try, whether to
/// retry — performing any wait/telemetry itself and returning true to retry or
/// false to give up.
typedef RetryAction = Future<bool> Function(int attemptNumber);

/// One retry rule: which errors it matches and what to do before each retry.
class RetryRule {
  /// Whether this rule applies to the given error.
  final bool Function(Object error) matches;

  /// Action run after a matching failure; returns true to retry.
  final RetryAction beforeRetry;

  /// A rule that retries errors matched by [matches], running [beforeRetry]
  /// before each retry.
  const RetryRule({required this.matches, required this.beforeRetry});

  /// Retry up to [maxAttempts] times (null = unbounded), waiting [delay] (or
  /// not, if null) between tries.
  RetryRule.delay({
    required this.matches,
    int? maxAttempts,
    Duration? delay,
  }) : beforeRetry = ((n) async {
          if (maxAttempts != null && n >= maxAttempts) {
            return false;
          }
          if (delay != null) {
            await Future<void>.delayed(delay);
          }
          return true;
        });

  /// Retry up to [maxAttempts] times (null = unbounded), awaiting [wait] (e.g.
  /// network readiness) between tries.
  RetryRule.waitFor({
    required this.matches,
    int? maxAttempts,
    required Future<void> Function() wait,
  }) : beforeRetry = ((n) async {
          if (maxAttempts != null && n >= maxAttempts) {
            return false;
          }
          await wait();
          return true;
        });

  /// A copy whose action is [chain], which receives the attempt number and this
  /// rule's current action so it can do its own work and defer to it, e.g.
  /// `rule.withBeforeRetry((n, prev) async { record(); return prev(n); })`.
  RetryRule withBeforeRetry(
    Future<bool> Function(int attemptNumber, RetryAction previous) chain,
  ) =>
      RetryRule(matches: matches, beforeRetry: (n) => chain(n, beforeRetry));

  /// A copy that runs [onRetry] (e.g. telemetry) before this rule's action, or
  /// this rule unchanged if [onRetry] is null.
  RetryRule withOnRetry(Future<void> Function()? onRetry) => onRetry == null
      ? this
      : withBeforeRetry((n, previous) async {
          await onRetry();
          return previous(n);
        });
}

/// Retry policy: ordered rules (first match wins) plus optional overall timeout.
class RetryStrategy {
  /// Rules tried in order; the first match decides retry behavior.
  final List<RetryRule> rules;

  /// Overall deadline across all attempts (null = no limit).
  final Duration? timeout;

  /// A policy applying [rules] in order, bounded by an optional [timeout].
  const RetryStrategy({this.rules = const [], this.timeout});

  /// The first rule matching [error], or null if none apply.
  RetryRule? ruleFor(Object error) {
    for (final r in rules) {
      if (r.matches(error)) {
        return r;
      }
    }
    return null;
  }

  /// Run [closure] under this policy. An error with no matching rule propagates
  /// immediately. A matched error whose rule action returns false stops retrying,
  /// rethrowing that last error from [closure]. Throws [TimeoutException] if the
  /// [timeout] deadline is reached before an attempt starts or while awaiting a
  /// rule action.
  ///
  /// Blocks across all attempts (re-running [closure] and awaiting each rule
  /// action) until success, give-up, or the [timeout] deadline.
  Future<T> retry<T>(Future<T> Function() closure) async {
    final deadline = timeout == null ? null : DateTime.now().add(timeout!);
    final attempts = <RetryRule, int>{};

    while (true) {
      if (deadline != null && !DateTime.now().isBefore(deadline)) {
        throw TimeoutException('RetryStrategy.retry timeout', timeout);
      }
      try {
        return await closure();
      } catch (error) {
        final rule = ruleFor(error);
        if (rule == null) {
          rethrow;
        }
        final n = (attempts[rule] ?? 0) + 1;
        attempts[rule] = n;
        var action = rule.beforeRetry(n);
        if (deadline != null) {
          final rem = deadline.difference(DateTime.now());
          action = action.timeout(rem.isNegative ? Duration.zero : rem);
        }
        if (!await action) {
          rethrow;
        }
      }
    }
  }
}

/// VeilidAPI-level policy: retry the transients that clear once online. Each
/// rule's action defaults to a bounded fixed delay; override either to change
/// the wait (e.g. block on network readiness) or add telemetry.
class VeilidAPIRetryStrategy extends RetryStrategy {
  /// [tryAgainRetry] handles [VeilidAPIExceptionTryAgain]; [transientRetry]
  /// handles timeout and no-connection errors. Either defaults to
  /// [defaultRetry].
  // ignore: use_super_parameters
  VeilidAPIRetryStrategy({
    RetryAction? tryAgainRetry,
    RetryAction? transientRetry,
    Duration? timeout,
  }) : super(
          rules: [
            RetryRule(
              matches: (e) => e is VeilidAPIExceptionTryAgain,
              beforeRetry: tryAgainRetry ?? defaultRetry,
            ),
            RetryRule(
              matches: (e) =>
                  e is VeilidAPIExceptionTimeout ||
                  e is VeilidAPIExceptionNoConnection,
              beforeRetry: transientRetry ?? defaultRetry,
            ),
          ],
          timeout: timeout,
        );

  /// Up to 3 tries, 250ms apart.
  static Future<bool> defaultRetry(int attemptNumber) async {
    if (attemptNumber >= 3) {
      return false;
    }
    await Future<void>.delayed(const Duration(milliseconds: 250));
    return true;
  }
}
