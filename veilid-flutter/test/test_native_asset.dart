import 'package:flutter_test/flutter_test.dart';
import 'package:veilid/veilid.dart';

// Renaming either side of the @DefaultAsset/assetName pair still compiles and
// fails only at the first FFI call.
void testNativeAssetResolves() {
  expect(Veilid.instance.veilidVersionString(), isNotEmpty);
  expect(Veilid.instance.veilidFeatures(), isNotEmpty);
  expect(Veilid.instance.validCryptoKinds(), isNotEmpty);
  expect(Veilid.instance.now().value, greaterThan(BigInt.zero));
}
