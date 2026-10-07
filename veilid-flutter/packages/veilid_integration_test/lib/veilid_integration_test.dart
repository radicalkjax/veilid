/// In-window log harness for Veilid Flutter integration tests.
///
/// Provides:
///   - [IntegrationLogApp] / [IntegrationLogSink]: a scrollable xterm log
///     pane that mirrors Veilid core log lines (via [LogFixture] listener)
///     and direct `print()` calls inside [runWithIntegrationLog]'s zone.
///   - [IntegrationTestStatusPane] / [IntegrationTestStatusSink]: a separate
///     pane at the bottom showing pass/fail counts, current test, and a
///     scrollable list of finished tests with elapsed times.
///   - [runWithIntegrationLog]: wraps `main()` to install both feeds.
library;

export 'src/integration_log.dart';
