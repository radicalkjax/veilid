import 'package:veilid/veilid.dart';

/// Test logging fixture. Formats and prints whatever veilid-core forwards;
/// veilid-core filters that by LOG_LEVEL and LOG_DIRECTIVES. The fixture's own
/// messages stop at LOG_LEVEL.
LogFixture get log => LogFixture.instance;

/// The LOG_LEVEL define: the level veilid-core logs at, and the cutoff for the
/// fixture's own messages.
final VeilidConfigLogLevel testLogLevel = VeilidConfigLogLevel.fromJson(
  const String.fromEnvironment('LOG_LEVEL', defaultValue: 'info'),
);

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
    final line = '${_timestamp()} ${level.name.toUpperCase()} [test] $message';
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

  // Forwarded lines skip this: veilid-core applied LOG_LEVEL and LOG_DIRECTIVES
  void _own(VeilidConfigLogLevel level, String message) {
    if (level.index <= testLogLevel.index) {
      _log(level, message);
    }
  }

  void error(String message) => _own(VeilidConfigLogLevel.error, message);
  void warn(String message) => _own(VeilidConfigLogLevel.warn, message);
  void info(String message) => _own(VeilidConfigLogLevel.info, message);
  void debug(String message) => _own(VeilidConfigLogLevel.debug, message);
  void trace(String message) => _own(VeilidConfigLogLevel.trace, message);
}
