// Running veilid-core inside a plain `flutter test` process.
//
// A `flutter test` process has no plugins, and veilid-core startup needs
// exactly one platform channel: `path_provider`, for its storage directory.
// Mocking that channel starts core normally, which gives unit tests crypto
// (keypairs, member ids, signing, nonces) and TableDB.
//
// Core does not attach here: no network, no DHT records, no peers. Tests
// needing those belong in the integration harness.

import 'dart:io';

import 'package:flutter/services.dart';
import 'package:flutter_test/flutter_test.dart';

import 'veilid_fixture.dart';

/// Point `path_provider` at a fresh temporary directory and return it.
///
/// Every method on that channel answers with a path, so one handler covers all
/// of them. The directory is new on every call, so concurrent test processes
/// never share a storage root.
Directory mockVeilidStorageDirectories({String prefix = 'veilid_unit'}) {
  TestWidgetsFlutterBinding.ensureInitialized();
  final dir = Directory.systemTemp.createTempSync(prefix);
  for (final channel in const [
    'plugins.flutter.io/path_provider',
    'plugins.flutter.io/path_provider_macos',
    'plugins.flutter.io/path_provider_linux',
    'plugins.flutter.io/path_provider_windows',
    'plugins.flutter.io/path_provider_android',
  ]) {
    TestDefaultBinaryMessengerBinding.instance.defaultBinaryMessenger
        .setMockMethodCallHandler(
          MethodChannel(channel),
          (call) async => dir.path,
        );
  }
  return dir;
}

/// A program name no concurrently-running test process can share.
///
/// [DefaultVeilidFixture] joins the program name into its table, protected and
/// block store directories, so it is the isolation key: two processes sharing
/// one share their stores. `flutter test` runs test files in separate
/// processes, so the pid separates them. A Dart program holds at most one
/// veilid instance, so no instance counter is needed on top.
String veilidUnitProgramName([String base = 'VeilidUnitTest']) => '$base-$pid';

/// Start veilid-core for this test file and shut it down afterwards.
///
/// Call once at the top of `main()`. veilid-core is a process singleton, so
/// this registers `setUpAll`/`tearDownAll`, not per-test hooks.
///
/// Startup is [DefaultVeilidFixture], the same one the integration tests use,
/// so logging and config honour the same environment variables; this adds the
/// mocked storage directory a pluginless process lacks and a per-process
/// program name (see [veilidUnitProgramName]).
///
/// [base] only labels the run; the pid supplies uniqueness, so two files may
/// share a base.
void useVeilidUnitCore({String base = 'VeilidUnitTest'}) {
  TestWidgetsFlutterBinding.ensureInitialized();

  late Directory storageDir;
  final programName = veilidUnitProgramName(base);
  final fixture = DefaultVeilidFixture(programName: programName);

  setUpAll(() async {
    storageDir = mockVeilidStorageDirectories(prefix: programName);
    await fixture.setUp();
  });

  tearDownAll(() async {
    await fixture.tearDown();
    try {
      storageDir.deleteSync(recursive: true);
    } on FileSystemException {
      // A leftover temp directory is not worth failing a suite over
    }
  });
}
