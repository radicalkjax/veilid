import 'dart:convert';
import 'dart:typed_data';

import 'package:flutter_test/flutter_test.dart';
import 'package:veilid/veilid.dart';

const testKey = '__dart_test_secret';
const testKey2 = '__dart_test_secret2';

// Keys veilid-core keeps for itself. Applications must neither see nor touch these.
const reservedKeys = ['device_encryption_key', '_test_key'];

Future<void> _cleanup() async {
  await Veilid.instance.removeUserSecret(testKey);
  await Veilid.instance.removeUserSecret(testKey2);
}

Future<void> testSaveLoadRemoveUserSecret() async {
  await _cleanup();

  // Saving a new secret reports that nothing was there before.
  expect(
    await Veilid.instance.saveUserSecret(testKey, utf8.encode('hello')),
    isFalse,
  );
  expect(
    utf8.decode((await Veilid.instance.loadUserSecret(testKey))!),
    equals('hello'),
  );

  // Overwriting reports the previous value.
  expect(
    await Veilid.instance.saveUserSecret(testKey, utf8.encode('world')),
    isTrue,
  );
  expect(
    utf8.decode((await Veilid.instance.loadUserSecret(testKey))!),
    equals('world'),
  );

  // Removal is idempotent.
  expect(await Veilid.instance.removeUserSecret(testKey), isTrue);
  expect(await Veilid.instance.removeUserSecret(testKey), isFalse);
  expect(await Veilid.instance.loadUserSecret(testKey), isNull);
}

Future<void> testBinaryUserSecretRoundtrip() async {
  await _cleanup();

  // Non-utf8 bytes, including nulls, must survive the base64 FFI/wasm encoding.
  final value = Uint8List.fromList(List<int>.generate(256, (i) => i));
  await Veilid.instance.saveUserSecret(testKey, value);
  expect(await Veilid.instance.loadUserSecret(testKey), equals(value));

  // An empty value is a value, not an absence.
  await Veilid.instance.saveUserSecret(testKey, Uint8List(0));
  expect(await Veilid.instance.loadUserSecret(testKey), equals(Uint8List(0)));

  await _cleanup();
}

Future<void> testMissingUserSecret() async {
  expect(await Veilid.instance.loadUserSecret('__dart_never_stored'), isNull);
  expect(
    await Veilid.instance.removeUserSecret('__dart_never_stored'),
    isFalse,
  );
}

Future<void> testListUserSecrets() async {
  await _cleanup();

  await Veilid.instance.saveUserSecret(testKey, utf8.encode('a'));
  await Veilid.instance.saveUserSecret(testKey2, utf8.encode('b'));

  var keys = await Veilid.instance.listUserSecrets();
  expect(keys, contains(testKey));
  expect(keys, contains(testKey2));

  expect(await Veilid.instance.removeUserSecret(testKey), isTrue);

  keys = await Veilid.instance.listUserSecrets();
  expect(keys, isNot(contains(testKey)));
  expect(keys, contains(testKey2));

  await _cleanup();
}

/// The node has a device_encryption_key of its own; it must stay invisible to apps.
Future<void> testListNeverReturnsReservedKeys() async {
  await Veilid.instance.saveUserSecret(testKey, utf8.encode('a'));

  final keys = await Veilid.instance.listUserSecrets();
  for (final reserved in reservedKeys) {
    expect(keys, isNot(contains(reserved)));
  }

  await _cleanup();
}

/// An app must not be able to read or clobber veilid's own keys by name.
Future<void> testReservedKeysAreRejected() async {
  for (final reserved in reservedKeys) {
    await expectLater(
      () => Veilid.instance.loadUserSecret(reserved),
      throwsA(isA<VeilidAPIException>()),
    );
    await expectLater(
      () => Veilid.instance.saveUserSecret(reserved, utf8.encode('clobber')),
      throwsA(isA<VeilidAPIException>()),
    );
    await expectLater(
      () => Veilid.instance.removeUserSecret(reserved),
      throwsA(isA<VeilidAPIException>()),
    );
  }
}

Future<void> testDeleteAllUserSecrets() async {
  await Veilid.instance.saveUserSecret(testKey, utf8.encode('a'));
  await Veilid.instance.saveUserSecret(testKey2, utf8.encode('b'));

  await Veilid.instance.deleteAllUserSecrets();

  var keys = await Veilid.instance.listUserSecrets();
  expect(keys, isNot(contains(testKey)));
  expect(keys, isNot(contains(testKey2)));

  // Veilid's own keys are untouched by an application-side wipe, so they must
  // still be absent from the app's view rather than having become visible.
  for (final reserved in reservedKeys) {
    expect(keys, isNot(contains(reserved)));
  }

  // The store is still usable afterwards.
  expect(
    await Veilid.instance.saveUserSecret(testKey, utf8.encode('c')),
    isFalse,
  );
  keys = await Veilid.instance.listUserSecrets();
  expect(keys, contains(testKey));

  await _cleanup();
}
