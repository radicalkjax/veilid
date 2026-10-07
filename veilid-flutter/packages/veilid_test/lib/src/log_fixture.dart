import 'package:veilid/veilid.dart';

/// Test logging fixture. Formats and prints whatever veilid-core forwards;
/// filtering is done internally by veilid-core via LOG_DIRECTIVES.
LogFixture get log => LogFixture.instance;

class LogFixture {
  static final LogFixture instance = LogFixture._();

  LogFixture._();

  final List<void Function(String)> _listeners = [];

  /// Flap-detector warnings seen this run (messages containing the FLAPPING marker).
  /// Tests assert this is empty at teardown to catch online/relay/route/connection/readiness
  /// flapping regressions automatically.
  final List<String> flapWarnings = [];

  /// Register a sink that receives every formatted log line in addition to
  /// the stdout print. Used by veilid_integration_test to mirror lines into
  /// the in-window terminal widget.
  void addListener(void Function(String) listener) => _listeners.add(listener);

  void removeListener(void Function(String) listener) =>
      _listeners.remove(listener);

  String _timestamp() {
    final dt = DateTime.now();
    return '${dt.hour.toString().padLeft(2, '0')}:'
        '${dt.minute.toString().padLeft(2, '0')}:'
        '${dt.second.toString().padLeft(2, '0')}.'
        '${dt.millisecond.toString().padLeft(3, '0')}';
  }

  void _log(VeilidConfigLogLevel level, String message) {
    final line =
        '${_timestamp()} ${level.name.toUpperCase()} [test] $message';
    // ignore: avoid_print
    print(line);
    for (final listener in _listeners) {
      listener(line);
    }
  }

  /// Process a VeilidLog update from the Veilid core update stream.
  void processVeilidLog(VeilidLog log) {
    final level = switch (log.logLevel) {
      VeilidLogLevel.error => VeilidConfigLogLevel.error,
      VeilidLogLevel.warn => VeilidConfigLogLevel.warn,
      VeilidLogLevel.info => VeilidConfigLogLevel.info,
      VeilidLogLevel.debug => VeilidConfigLogLevel.debug,
      VeilidLogLevel.trace => VeilidConfigLogLevel.trace,
    };
    if (log.message.contains('FLAPPING')) {
      flapWarnings.add(log.message);
    }
    _log(level, log.message);
  }

  /// Throw if any flapping was detected during the test. Call in tearDownAll.
  void assertNoFlapping() {
    if (flapWarnings.isNotEmpty) {
      final seen = flapWarnings.join('\n  ');
      flapWarnings.clear();
      throw StateError('Flap detector tripped during test:\n  $seen');
    }
  }

  void error(String message) => _log(VeilidConfigLogLevel.error, message);
  void warn(String message) => _log(VeilidConfigLogLevel.warn, message);
  void info(String message) => _log(VeilidConfigLogLevel.info, message);
  void debug(String message) => _log(VeilidConfigLogLevel.debug, message);
  void trace(String message) => _log(VeilidConfigLogLevel.trace, message);
}
