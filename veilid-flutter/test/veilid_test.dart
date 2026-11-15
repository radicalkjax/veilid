import 'package:flutter_test/flutter_test.dart';

import 'test_encoding.dart';
import 'test_native_asset.dart';
import 'test_prebuilt.dart';
import 'test_timestamp.dart';
import 'test_value_subkey_range.dart';

void main() {
  TestWidgetsFlutterBinding.ensureInitialized();

  setUp(() {});
  tearDown(() {});

  group('native asset', () {
    test('test FFI entry points resolve', testNativeAssetResolves);
  });

  group('prebuilt', () {
    test('nothing configured builds from source', testPrebuiltUnset);
    test('URL and hash configure a prebuilt', testPrebuiltConfigured);
    test('the CARGO_ spelling wins', testPrebuiltCargoSpellingWins);
    test('a URL or hash alone fails', testPrebuiltNeedsUrlAndHash);
    test('a malformed URL or hash fails', testPrebuiltMalformed);
    test(
      'a feature selection or debug build ignores it',
      testPrebuiltSkippedForSelectionOrDebug,
    );
    test('only Windows targets take one', testPrebuiltWindowsOnly);
    test('fetch verifies and reuses', testFetchPrebuiltVerifies);
    test('fetch rejects a wrong hash', testFetchPrebuiltRejectsWrongHash);
    test('the PE machine must match', testPrebuiltMachine);
  });

  group('encoding', () {
    test('test encoding known vectors', testEncodingKnownVectors);
    test('test encode/decode garbage', testEncodeDecodeGarbage);
    test('test encode/decode garbage with pad', testEncodeDecodeGarbagePad);
  });

  group('Timestamp', () {
    test('human form matches veilid-core', testTimestampHumanForm);
    test('machine form round trips', testTimestampMachineForm);
    test(
      'duration human form matches veilid-core',
      testTimestampDurationHumanForm,
    );
    test('duration machine form round trips', testTimestampDurationMachineForm);
  });

  group('ValueSubkeyRange', () {
    test('test ValueSubkeyRange', testValueSubkeyRange);
    test('test List<ValueSubkeyRange>', testValueSubkeyRangeList);
  });
}
