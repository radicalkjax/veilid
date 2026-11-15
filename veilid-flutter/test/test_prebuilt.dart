import 'dart:io';
import 'dart:typed_data';

import 'package:code_assets/code_assets.dart';
import 'package:crypto/crypto.dart';
import 'package:flutter_test/flutter_test.dart';

import '../hook/prebuilt.dart';

const _url = 'CCACHE_VEILID_FLUTTER_PREBUILT';
const _hash = 'CCACHE_VEILID_FLUTTER_PREBUILT_SHA256';
final _digest = 'ab' * 32;

Map<String, String> _configured([String? url, String? hash]) => {
  _url: url ?? 'https://example.invalid/veilid_flutter.dll',
  _hash: hash ?? _digest,
};

Prebuilt? _forRelease(Map<String, String> environment) => prebuiltForBuild(
  environment,
  featuresSelected: false,
  release: true,
  targetOS: OS.windows,
);

void testPrebuiltUnset() {
  expect(_forRelease({}), isNull);
  expect(_forRelease({_url: '', _hash: ' '}), isNull);
}

void testPrebuiltConfigured() {
  final prebuilt = _forRelease(
    _configured(null, ' ${_digest.toUpperCase()}\n'),
  );
  expect(
    prebuilt?.url,
    Uri.parse('https://example.invalid/veilid_flutter.dll'),
  );
  expect(prebuilt?.sha256, _digest);
}

void testPrebuiltCargoSpellingWins() {
  final prebuilt = _forRelease({
    ..._configured(),
    'CARGO_VEILID_FLUTTER_PREBUILT': 'file:///cargo/veilid_flutter.dll',
    'CARGO_VEILID_FLUTTER_PREBUILT_SHA256': 'cd' * 32,
  });
  expect(prebuilt?.url, Uri.parse('file:///cargo/veilid_flutter.dll'));
  expect(prebuilt?.sha256, 'cd' * 32);
}

void testPrebuiltNeedsUrlAndHash() {
  expect(
    () => _forRelease({_url: 'https://example.invalid/a.dll'}),
    throwsStateError,
  );
  expect(() => _forRelease({_hash: _digest}), throwsStateError);
}

void testPrebuiltMalformed() {
  expect(() => _forRelease(_configured(null, 'ab' * 31)), throwsStateError);
  expect(() => _forRelease(_configured(null, 'zz' * 32)), throwsStateError);
  expect(
    () => _forRelease(_configured('ftp://example.invalid/a.dll')),
    throwsStateError,
  );
}

void testPrebuiltSkippedForSelectionOrDebug() {
  // Not even a half-configured prebuilt is read: these always build from source
  for (final environment in [
    _configured(),
    {_url: 'https://example.invalid/a.dll'},
  ]) {
    expect(
      prebuiltForBuild(
        environment,
        featuresSelected: true,
        release: true,
        targetOS: OS.windows,
      ),
      isNull,
    );
    expect(
      prebuiltForBuild(
        environment,
        featuresSelected: false,
        release: false,
        targetOS: OS.windows,
      ),
      isNull,
    );
  }
}

void testPrebuiltWindowsOnly() {
  expect(
    () => prebuiltForBuild(
      _configured(),
      featuresSelected: false,
      release: true,
      targetOS: OS.linux,
    ),
    throwsStateError,
  );
}

Future<void> testFetchPrebuiltVerifies() async {
  final dir = Directory.systemTemp.createTempSync('veilid_prebuilt_test');
  try {
    final source = File('${dir.path}/source.dll')
      ..writeAsBytesSync(List.filled(4096, 7));
    final digest = sha256.convert(source.readAsBytesSync()).toString();
    final cacheDir = Directory('${dir.path}/cache').uri;
    final prebuilt = (url: source.uri, sha256: digest);

    final fetched = await fetchPrebuilt(prebuilt, cacheDir: cacheDir);
    expect(fetched.path, endsWith('prebuilt/$digest/$prebuiltFileName'));
    expect(fetched.readAsBytesSync(), source.readAsBytesSync());

    // A verified copy is reused without fetching again
    source.deleteSync();
    final reused = await fetchPrebuilt(prebuilt, cacheDir: cacheDir);
    expect(reused.path, fetched.path);
  } finally {
    dir.deleteSync(recursive: true);
  }
}

Future<void> testFetchPrebuiltRejectsWrongHash() async {
  final dir = Directory.systemTemp.createTempSync('veilid_prebuilt_test');
  try {
    final source = File('${dir.path}/source.dll')
      ..writeAsBytesSync(List.filled(4096, 7));
    final cacheDir = Directory('${dir.path}/cache');
    await expectLater(
      fetchPrebuilt((url: source.uri, sha256: _digest), cacheDir: cacheDir.uri),
      throwsStateError,
    );
    expect(
      cacheDir.listSync(recursive: true).whereType<File>(),
      isEmpty,
      reason: 'a rejected download must not stay in the cache',
    );
  } finally {
    dir.deleteSync(recursive: true);
  }
}

/// The smallest file whose PE header names [machine].
File _peImage(Directory dir, int machine) {
  final bytes = ByteData(0x50)
    ..setUint8(0, 0x4d)
    ..setUint8(1, 0x5a)
    ..setUint32(0x3c, 0x40, Endian.little)
    ..setUint8(0x40, 0x50)
    ..setUint8(0x41, 0x45)
    ..setUint16(0x44, machine, Endian.little);
  return File('${dir.path}/pe_$machine.dll')
    ..writeAsBytesSync(bytes.buffer.asUint8List());
}

void testPrebuiltMachine() {
  final dir = Directory.systemTemp.createTempSync('veilid_prebuilt_test');
  try {
    final x64 = _peImage(dir, 0x8664);
    checkPrebuiltMachine(x64, Architecture.x64);
    expect(
      () => checkPrebuiltMachine(x64, Architecture.arm64),
      throwsStateError,
    );
    checkPrebuiltMachine(_peImage(dir, 0xaa64), Architecture.arm64);

    final notPe = File('${dir.path}/lib.so')
      ..writeAsBytesSync(List.filled(256, 0x7f));
    expect(
      () => checkPrebuiltMachine(notPe, Architecture.x64),
      throwsStateError,
    );
  } finally {
    dir.deleteSync(recursive: true);
  }
}
