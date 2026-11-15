import 'package:flutter/foundation.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:veilid/veilid.dart';

Future<void> testVeilidConfigDefaults() async {
  const programName = 'Veilid Tests';
  final defaultConfig = await getDefaultVeilidConfig(
    isWeb: kIsWeb,
    programName: programName,
  );
  assert(defaultConfig.programName == programName, 'program name should match');
}

Future<void> testVeilidConfigChange() async {
  final before = (await Veilid.instance.getVeilidState()).config.config;

  // Detached, a changeable key takes at once; the config getVeilidState
  // reports carries no secrets and still applies
  final flipped = before.copyWith(
    network: before.network.copyWith(upnp: !before.network.upnp),
  );
  await Veilid.instance.changeConfig(flipped);
  final after = (await Veilid.instance.getVeilidState()).config.config;
  expect(after.network.upnp, equals(flipped.network.upnp));

  // A key that needs a restart is refused and nothing changes
  await expectLater(
    Veilid.instance.changeConfig(
      flipped.copyWith(namespace: '${flipped.namespace}x'),
    ),
    throwsA(isA<VeilidAPIExceptionInvalidArgument>()),
  );
  final unchanged = (await Veilid.instance.getVeilidState()).config.config;
  expect(unchanged.namespace, equals(flipped.namespace));

  await Veilid.instance.changeConfig(before);
}
