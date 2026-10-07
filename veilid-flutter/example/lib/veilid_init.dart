import 'package:flutter/foundation.dart';
import 'package:veilid/veilid.dart';

// Initialize Veilid
// Call only once.
void veilidInit() {
  if (kIsWeb) {
    const platformConfig = VeilidWASMConfig(
        logging: VeilidWASMConfigLogging(
            performance: VeilidWASMConfigLoggingPerformance(
                enabled: false, level: VeilidConfigLogLevel.debug),
            api: VeilidWASMConfigLoggingApi(
                enabled: true, level: VeilidConfigLogLevel.info)));
    Veilid.instance.initializeVeilidCore(platformConfig.toJson());
  } else {
    const platformConfig = VeilidFFIConfig(
        logging: VeilidFFIConfigLogging(
      terminal: VeilidFFIConfigLoggingTerminal(
          enabled: false, level: VeilidConfigLogLevel.debug),
      api: VeilidFFIConfigLoggingApi(
          enabled: true, level: VeilidConfigLogLevel.info),
    ));
    Veilid.instance.initializeVeilidCore(platformConfig.toJson());
  }
}
