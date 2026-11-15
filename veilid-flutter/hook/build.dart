// Builds veilid-flutter/rust as the code asset lib/veilid_ffi.dart resolves against,
// or bundles a prebuilt one the environment names (prebuilt.dart).
// `_assetName` below and that file's @DefaultAsset must move together.

import 'dart:io';

import 'package:code_assets/code_assets.dart';
import 'package:hooks/hooks.dart';
import 'package:native_toolchain_rust/native_toolchain_rust.dart';

import 'prebuilt.dart';

const _assetName = 'veilid_ffi.dart';

/// Cargo features a consuming app selects for this build, read from the
/// environment so a selection lives only as long as the process that made it.
///
/// Hooks run under a fixed environment allowlist that passes only a few name
/// prefixes. `CARGO_` reaches a hook from hooks_runner 1.6.2 on, which no
/// flutter stable ships yet, so `CCACHE_` carries the same selection until one
/// does and the runner scripts set both. Drop the `CCACHE_` spelling once the
/// minimum flutter bundles hooks_runner 1.6.2.
const _featuresVars = [
  'CARGO_VEILID_FLUTTER_FEATURES',
  'CCACHE_VEILID_FLUTTER_FEATURES',
];
const _noDefaultFeaturesVars = [
  'CARGO_VEILID_FLUTTER_NO_DEFAULT_FEATURES',
  'CCACHE_VEILID_FLUTTER_NO_DEFAULT_FEATURES',
];

void main(List<String> args) async {
  await build(args, (input, output) async {
    if (!input.config.buildCodeAssets) {
      return;
    }
    final selection = _featureSelection(input);
    _rejectMockApi(input, selection);
    final code = input.config.code;
    final prebuilt = prebuiltForBuild(
      Platform.environment,
      featuresSelected: selection != null,
      release: input.config.linkingEnabled,
      targetOS: code.targetOS,
    );
    if (prebuilt != null) {
      final file = await fetchPrebuilt(
        prebuilt,
        cacheDir: input.outputDirectoryShared,
      );
      checkPrebuiltMachine(file, code.targetArchitecture);
      output.assets.code.add(
        CodeAsset(
          package: input.packageName,
          name: _assetName,
          linkMode: DynamicLoadingBundled(),
          file: file.uri,
        ),
      );
      output.dependencies.add(file.uri);
      stderr.writeln('veilid-flutter: bundling the prebuilt ${prebuilt.url}');
      return;
    }
    stderr.writeln(
      'veilid-flutter: building from source, ${_sourceReason(input, selection)}',
    );
    await RustBuilder(
      assetName: _assetName,
      cratePath: 'rust',
      buildMode: _buildMode(input),
      features: selection?.features ?? const [],
      enableDefaultFeatures: selection?.defaultFeatures ?? true,
    ).run(input: input, output: output);
  });
}

String _sourceReason(BuildInput input, _FeatureSelection? selection) {
  if (selection != null) {
    return 'features selected by ${selection.source}';
  }
  return input.config.linkingEnabled ? 'no prebuilt configured' : 'debug build';
}

// linkingEnabled is false only for debug builds, and it sits in the hashed config
// subtree, so the two modes get separate hook output directories.
BuildMode _buildMode(BuildInput input) =>
    input.config.linkingEnabled ? BuildMode.release : BuildMode.debug;

typedef _FeatureSelection = ({
  List<String> features,
  bool defaultFeatures,
  String source,
});

/// A shipping build must never link the in-process mock, which answers every
/// network operation itself. This refuses rather than warns.
///
/// linkingEnabled stands in for the Flutter build mode, which hooks are not
/// told: false only for debug, so the mock stays available to the test suites.
void _rejectMockApi(BuildInput input, _FeatureSelection? selection) {
  if (!input.config.linkingEnabled || selection == null) {
    return;
  }
  final mock = selection.features.where((f) => f.startsWith('mock-api'));
  if (mock.isEmpty) {
    return;
  }
  throw StateError(
    'veilid-flutter: refusing to build ${mock.join(', ')} into a non-debug '
    'build. The mock replaces the network with an in-process fake.\n'
    'Selected by ${selection.source}; unset it and build again.',
  );
}

/// The environment wins over pubspec user-defines, which stay supported for an
/// app that wants one fixed feature set for every build. Null when neither
/// selects anything, which builds the default features.
_FeatureSelection? _featureSelection(BuildInput input) {
  final environment = _environmentSelection();
  if (environment != null) {
    return environment;
  }
  final features = _userDefineFeatures(input);
  if (features.isEmpty) {
    return null;
  }
  return (
    features: features,
    defaultFeatures: true,
    source: 'the pubspec user-defines',
  );
}

/// Null when the environment names no selection, so the caller falls back
/// rather than building something the app did not ask for.
_FeatureSelection? _environmentSelection() {
  final features = _firstSet(_featuresVars);
  final noDefaults = _firstSet(_noDefaultFeaturesVars);
  if (features == null && noDefaults == null) {
    return null;
  }
  return (
    features: _parseFeatures(features?.value),
    defaultFeatures: !_parseBool(noDefaults),
    source: (features ?? noDefaults!).name,
  );
}

typedef _EnvVar = ({String name, String value});

_EnvVar? _firstSet(List<String> names) {
  for (final name in names) {
    final value = Platform.environment[name];
    if (value != null) {
      return (name: name, value: value);
    }
  }
  return null;
}

/// Comma- or whitespace-separated, so it reads the same as cargo's own
/// `--features`.
List<String> _parseFeatures(String? value) =>
    (value ?? '').split(RegExp(r'[,\s]+')).where((f) => f.isNotEmpty).toList();

/// Throws on anything else: a build selection misspelled as `yes` must not
/// silently read as the safe answer.
bool _parseBool(_EnvVar? variable) {
  if (variable == null) {
    return false;
  }
  switch (variable.value.trim().toLowerCase()) {
    case '' || '0' || 'false':
      return false;
    case '1' || 'true':
      return true;
    default:
      throw StateError(
        '${variable.name} must be 1/true or 0/false, got "${variable.value}"',
      );
  }
}

List<String> _userDefineFeatures(BuildInput input) {
  final features = input.userDefines['features'];
  if (features is! List) {
    return const <String>[];
  }
  return features.cast<String>();
}
