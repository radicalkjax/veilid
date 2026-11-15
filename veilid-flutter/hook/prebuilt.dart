// A release veilid-flutter library built elsewhere, which the build hook bundles
// instead of compiling one when the environment names it and its SHA-256.

import 'dart:io';
import 'dart:typed_data';

import 'package:code_assets/code_assets.dart';
import 'package:crypto/crypto.dart';

/// The prebuilt's URL, `file:` included. Both spellings for the reason the
/// feature variables have both: hooks see `CARGO_` only from hooks_runner 1.6.2.
const prebuiltUrlVars = [
  'CARGO_VEILID_FLUTTER_PREBUILT',
  'CCACHE_VEILID_FLUTTER_PREBUILT',
];

/// The prebuilt's SHA-256, 64 hex digits.
const prebuiltSha256Vars = [
  'CARGO_VEILID_FLUTTER_PREBUILT_SHA256',
  'CCACHE_VEILID_FLUTTER_PREBUILT_SHA256',
];

/// What the producer job publishes and the build bundles.
const prebuiltFileName = 'veilid_flutter.dll';

/// A library to bundle in place of a source build, and the hash it must have.
typedef Prebuilt = ({Uri url, String sha256});

/// The prebuilt this build bundles, or null to compile from source.
///
/// A feature selection or a debug build always compiles, since the prebuilt is
/// a release build with default features. Otherwise the environment decides,
/// and a prebuilt it names but this build cannot use fails the build rather
/// than falling back.
Prebuilt? prebuiltForBuild(
  Map<String, String> environment, {
  required bool featuresSelected,
  required bool release,
  required OS targetOS,
}) {
  if (featuresSelected || !release) {
    return null;
  }
  final prebuilt = configuredPrebuilt(environment);
  if (prebuilt != null && targetOS != OS.windows) {
    throw StateError(
      'veilid-flutter: prebuilt libraries exist for Windows only and this '
      'build targets $targetOS; unset ${prebuiltUrlVars.join(' and ')}',
    );
  }
  return prebuilt;
}

/// The prebuilt [environment] names, or null when it names none. An empty
/// value counts as unset. Throws when only one of the URL and the hash is set,
/// or either is malformed.
Prebuilt? configuredPrebuilt(Map<String, String> environment) {
  final url = _firstSet(environment, prebuiltUrlVars);
  final hash = _firstSet(environment, prebuiltSha256Vars);
  if (url == null && hash == null) {
    return null;
  }
  if (url == null) {
    throw StateError(
      'veilid-flutter: ${hash!.name} is set but no prebuilt URL is: set '
      '${prebuiltUrlVars.join(' or ')} too, or neither',
    );
  }
  if (hash == null) {
    throw StateError(
      'veilid-flutter: ${url.name} is set but no SHA-256 is, and a prebuilt '
      'is never bundled unverified: set ${prebuiltSha256Vars.join(' or ')}',
    );
  }
  final uri = Uri.tryParse(url.value);
  if (uri == null || !const ['http', 'https', 'file'].contains(uri.scheme)) {
    throw StateError(
      'veilid-flutter: ${url.name} must be an http, https or file URL, got '
      '"${url.value}"',
    );
  }
  final digest = hash.value.toLowerCase();
  if (!RegExp(r'^[0-9a-f]{64}$').hasMatch(digest)) {
    throw StateError(
      'veilid-flutter: ${hash.name} must be 64 hex digits, got "${hash.value}"',
    );
  }
  return (url: uri, sha256: digest);
}

/// The verified library for [prebuilt]: an earlier fetch into [cacheDir] when
/// it still hashes right, else a fresh one. Throws when the fetch fails or the
/// bytes do not hash to [Prebuilt.sha256].
Future<File> fetchPrebuilt(Prebuilt prebuilt, {required Uri cacheDir}) async {
  final file = File.fromUri(
    cacheDir.resolve('prebuilt/${prebuilt.sha256}/$prebuiltFileName'),
  );
  if (file.existsSync() && await _sha256Of(file) == prebuilt.sha256) {
    return file;
  }
  file.parent.createSync(recursive: true);
  final partial = File('${file.path}.part');
  await _fetch(prebuilt.url, partial);
  final got = await _sha256Of(partial);
  if (got != prebuilt.sha256) {
    partial.deleteSync();
    throw StateError(
      'veilid-flutter: the prebuilt from ${prebuilt.url} hashes to $got, not '
      '${prebuilt.sha256}',
    );
  }
  if (file.existsSync()) {
    file.deleteSync();
  }
  return partial.renameSync(file.path);
}

/// Throws unless [file] is a Windows library for [architecture], going by the
/// machine its PE header names.
void checkPrebuiltMachine(File file, Architecture architecture) {
  final want = _peMachines[architecture];
  final got = _peMachine(file);
  if (want == null || got != want) {
    final named = got == null
        ? 'no PE machine'
        : 'PE machine 0x${got.toRadixString(16)}';
    throw StateError(
      'veilid-flutter: the prebuilt ${file.path} is not a Windows '
      '$architecture library ($named)',
    );
  }
}

const _peMachines = {
  Architecture.x64: 0x8664,
  Architecture.arm64: 0xaa64,
  Architecture.ia32: 0x014c,
};

/// The machine field of a PE image's COFF header, or null when [file] is not a
/// PE image.
int? _peMachine(File file) {
  final raf = file.openSync();
  try {
    final dos = raf.readSync(0x40);
    // 'MZ'
    if (dos.length < 0x40 || dos[0] != 0x4d || dos[1] != 0x5a) {
      return null;
    }
    raf.setPositionSync(
      ByteData.sublistView(dos).getUint32(0x3c, Endian.little),
    );
    final coff = raf.readSync(6);
    // 'PE\0\0'
    if (coff.length < 6 ||
        coff[0] != 0x50 ||
        coff[1] != 0x45 ||
        coff[2] != 0 ||
        coff[3] != 0) {
      return null;
    }
    return ByteData.sublistView(coff).getUint16(4, Endian.little);
  } finally {
    raf.closeSync();
  }
}

typedef _EnvVar = ({String name, String value});

_EnvVar? _firstSet(Map<String, String> environment, List<String> names) {
  for (final name in names) {
    final value = environment[name]?.trim();
    if (value != null && value.isNotEmpty) {
      return (name: name, value: value);
    }
  }
  return null;
}

Future<String> _sha256Of(File file) async =>
    (await sha256.bind(file.openRead()).first).toString();

Future<void> _fetch(Uri url, File to) async {
  if (url.isScheme('file')) {
    await File.fromUri(url).copy(to.path);
    return;
  }
  final client = HttpClient()..findProxy = HttpClient.findProxyFromEnvironment;
  try {
    final response = await (await client.getUrl(url)).close();
    if (response.statusCode != HttpStatus.ok) {
      throw HttpException(
        'veilid-flutter: HTTP ${response.statusCode} fetching the prebuilt',
        uri: url,
      );
    }
    await response.pipe(to.openWrite());
  } finally {
    client.close(force: true);
  }
}
