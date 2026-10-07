import 'dart:async';

import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:integration_test/integration_test.dart';
// ignore: implementation_imports
import 'package:test_api/src/backend/invoker.dart';
// ignore: implementation_imports
import 'package:test_api/src/backend/message.dart';
import 'package:veilid_test/veilid_test.dart';
import 'package:veilid_xterm/veilid_xterm.dart';

const _ansiReset = '\x1B[0m';
const _ansiCyan = '\x1B[36m';
const _ansiYellow = '\x1B[33m';
const _ansiGreen = '\x1B[32m';
const _ansiRed = '\x1B[31m';

/// Singleton buffer of integration-test log output, rendered by
/// [IntegrationLogApp] into the test app window.
class IntegrationLogSink {
  static final IntegrationLogSink instance = IntegrationLogSink._();
  IntegrationLogSink._();

  final Terminal terminal = Terminal(maxLines: 50000);

  void _writeColored(String line, String? color) {
    final body = line.replaceAll('\n', '\r\n');
    if (color == null) {
      terminal.write('$body\r\n');
    } else {
      terminal.write('$color$body$_ansiReset\r\n');
    }
  }

  /// veilid logs (and anything else uncolored).
  void write(String line) => _writeColored(line, null);

  /// Dart `print()` output from inside test bodies (and the runZoned body).
  void writePrint(String line) => _writeColored(line, _ansiCyan);

  /// "Current test" status line at test start.
  void writeStatus(String line) => _writeColored(line, _ansiYellow);

  /// Test pass line.
  void writePass(String line) => _writeColored(line, _ansiGreen);

  /// Test fail line.
  void writeFail(String line) => _writeColored(line, _ansiRed);
}

enum TestStatus { passed, failed }

class TestResult {
  final String name;
  final TestStatus status;
  final int elapsedMs;
  final List<String> errors;
  const TestResult({
    required this.name,
    required this.status,
    required this.elapsedMs,
    this.errors = const [],
  });
}

/// Singleton status sink for test progress. Updated by global setUp/tearDown
/// hooks installed in [runWithIntegrationLog]. Rendered by
/// [IntegrationTestStatusPane].
class IntegrationTestStatusSink extends ChangeNotifier {
  static final IntegrationTestStatusSink instance =
      IntegrationTestStatusSink._();
  IntegrationTestStatusSink._();

  final List<TestResult> results = [];
  String? currentTest;

  int get passCount =>
      results.where((r) => r.status == TestStatus.passed).length;
  int get failCount =>
      results.where((r) => r.status == TestStatus.failed).length;

  void onStart(String name) {
    currentTest = name;
    notifyListeners();
  }

  void onEnd(String name, int elapsedMs, List<String> errors) {
    currentTest = null;
    results.add(
      TestResult(
        name: name,
        status: errors.isEmpty ? TestStatus.passed : TestStatus.failed,
        elapsedMs: elapsedMs,
        errors: errors,
      ),
    );
    notifyListeners();
  }
}

final Map<String, Stopwatch> _testStopwatches = {};
StreamSubscription<Message>? _testMessageSub;

void _onTestStart() {
  final liveTest = Invoker.current?.liveTest;
  final name = liveTest?.test.name ?? '?';
  _testStopwatches[name] = Stopwatch()..start();
  IntegrationTestStatusSink.instance.onStart(name);
  IntegrationLogSink.instance.writeStatus('▶ $name');
  // package:test wraps each test in its own zone with a print handler that
  // emits Message.print events here, so subscribing to onMessage is the
  // only way to see in-test print() output from the parent zone.
  _testMessageSub = liveTest?.onMessage.listen((msg) {
    if (msg.type == MessageType.print) {
      IntegrationLogSink.instance.writePrint(msg.text);
    }
  });
}

void _onTestEnd() {
  _testMessageSub?.cancel();
  _testMessageSub = null;
  final liveTest = Invoker.current?.liveTest;
  final name = liveTest?.test.name ?? '?';
  final sw = _testStopwatches.remove(name);
  final elapsed = sw?.elapsedMilliseconds ?? 0;
  final errors = (liveTest?.errors ?? const [])
      .map((e) => '${e.error}')
      .toList();
  IntegrationTestStatusSink.instance.onEnd(name, elapsed, errors);
  if (errors.isEmpty) {
    IntegrationLogSink.instance.writePass('✓ $name  (${elapsed}ms)');
  } else {
    IntegrationLogSink.instance.writeFail('✗ $name  (${elapsed}ms)');
    for (final e in errors) {
      IntegrationLogSink.instance.writeFail('    $e');
    }
  }
}

/// Run an integration-test `main()` body with global setUp/tearDown hooks
/// that mirror test start/end into [IntegrationTestStatusSink] and write
/// status/pass/fail lines into [IntegrationLogSink]. The zone print handler
/// captures direct `print()` calls outside test bodies; in-test prints are
/// captured via [LiveTest.onMessage] in [_onTestStart].
void runWithIntegrationLog(void Function() body) {
  runZoned(
    () {
      setUp(_onTestStart);
      tearDown(_onTestEnd);
      body();
    },
    zoneSpecification: ZoneSpecification(
      print: (self, parent, zone, line) {
        IntegrationLogSink.instance.writePrint(line);
        parent.print(zone, line);
      },
    ),
  );
}

/// Two-pane debug app for integration tests: scrollable xterm log on top,
/// test-status pane on the bottom. Call `runApp(const IntegrationLogApp())`
/// right after `IntegrationTestWidgetsFlutterBinding.ensureInitialized()`.
class IntegrationLogApp extends StatefulWidget {
  const IntegrationLogApp({super.key});

  @override
  State<IntegrationLogApp> createState() => _IntegrationLogAppState();
}

class _IntegrationLogAppState extends State<IntegrationLogApp> {
  final _terminalController = TerminalController();
  final _scrollController = ScrollController();

  @override
  void initState() {
    super.initState();
    // integration_test binding defaults to fadePointers — only paints frames
    // on test-driver interaction. fullyLive paints every system frame so the
    // terminal repaints as we write into it.
    final binding = IntegrationTestWidgetsFlutterBinding.instance;
    binding.framePolicy = LiveTestWidgetsFlutterBindingFramePolicy.fullyLive;
    // Default is false — real OS pointer events get dropped in dispatchEvent
    // to keep tests deterministic. We need them here so the user can scroll,
    // select text, and click into the terminal pane.
    binding.shouldPropagateDevicePointerEvents = true;

    LogFixture.instance.addListener(IntegrationLogSink.instance.write);
    _terminalController.addListener(_onSelectionChanged);
  }

  @override
  void dispose() {
    _terminalController.removeListener(_onSelectionChanged);
    LogFixture.instance.removeListener(IntegrationLogSink.instance.write);
    _terminalController.dispose();
    _scrollController.dispose();
    super.dispose();
  }

  // Copy selection to clipboard whenever it becomes non-empty (mouse-drag
  // selection auto-copy). xterm.dart's built-in keyboard handler covers
  // Cmd+C / Ctrl+C when the terminal has focus.
  Future<void> _onSelectionChanged() async {
    final selection = _terminalController.selection;
    if (selection == null) return;
    final text = IntegrationLogSink.instance.terminal.buffer.getText(selection);
    if (text.isEmpty) return;
    await Clipboard.setData(ClipboardData(text: text));
  }

  @override
  Widget build(BuildContext context) => MaterialApp(
    debugShowCheckedModeBanner: false,
    theme: ThemeData.dark(),
    home: Scaffold(
      body: Column(
        children: [
          Expanded(
            child: Scrollbar(
              controller: _scrollController,
              thumbVisibility: true,
              child: TerminalView(
                IntegrationLogSink.instance.terminal,
                controller: _terminalController,
                scrollController: _scrollController,
                autofocus: true,
                textStyle: const TerminalStyle(fontSize: 10),
              ),
            ),
          ),
          Container(height: 1, color: Colors.grey.shade800),
          const SizedBox(height: 180, child: IntegrationTestStatusPane()),
        ],
      ),
    ),
  );
}

class IntegrationTestStatusPane extends StatelessWidget {
  const IntegrationTestStatusPane({super.key});

  @override
  Widget build(BuildContext context) => AnimatedBuilder(
    animation: IntegrationTestStatusSink.instance,
    builder: (context, _) {
      final sink = IntegrationTestStatusSink.instance;
      return Container(
        color: Colors.black,
        child: Column(
          crossAxisAlignment: CrossAxisAlignment.stretch,
          children: [
            Container(
              padding: const EdgeInsets.symmetric(horizontal: 8, vertical: 4),
              color: Colors.grey.shade900,
              child: Row(
                children: [
                  Text(
                    '✓ ${sink.passCount}',
                    style: const TextStyle(
                      color: Colors.greenAccent,
                      fontSize: 11,
                      fontFamily: 'monospace',
                    ),
                  ),
                  const SizedBox(width: 8),
                  Text(
                    '✗ ${sink.failCount}',
                    style: TextStyle(
                      color: sink.failCount > 0
                          ? Colors.redAccent
                          : Colors.grey.shade600,
                      fontSize: 11,
                      fontFamily: 'monospace',
                    ),
                  ),
                  const SizedBox(width: 12),
                  if (sink.currentTest != null)
                    Expanded(
                      child: Text(
                        '▶ ${sink.currentTest}',
                        style: const TextStyle(
                          color: Colors.yellowAccent,
                          fontSize: 11,
                          fontFamily: 'monospace',
                        ),
                        overflow: TextOverflow.ellipsis,
                      ),
                    ),
                ],
              ),
            ),
            Expanded(
              child: ListView.builder(
                reverse: true,
                itemCount: sink.results.length,
                itemBuilder: (context, i) {
                  final r = sink.results[sink.results.length - 1 - i];
                  final icon = r.status == TestStatus.passed ? '✓' : '✗';
                  final color = r.status == TestStatus.passed
                      ? Colors.greenAccent
                      : Colors.redAccent;
                  return Padding(
                    padding: const EdgeInsets.symmetric(
                      horizontal: 8,
                      vertical: 1,
                    ),
                    child: Text(
                      '$icon ${r.name}  (${r.elapsedMs}ms)',
                      style: TextStyle(
                        color: color,
                        fontSize: 11,
                        fontFamily: 'monospace',
                      ),
                    ),
                  );
                },
              ),
            ),
          ],
        ),
      );
    },
  );
}
