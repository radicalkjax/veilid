import 'package:flutter/foundation.dart' show kIsWeb;
import 'package:flutter/material.dart';
import 'package:flutter/services.dart' show BrowserContextMenu;

import 'app.dart';
import 'log.dart';
import 'veilid_init.dart';
import 'veilid_theme.dart';

/////////////////////////////// Entrypoint
void main() {
  WidgetsFlutterBinding.ensureInitialized();

  // Initialize Log
  initLoggy();

  // Initialize Veilid
  veilidInit();

  // Disable context menu
  if (kIsWeb) {
    BrowserContextMenu.disableContextMenu();
  }

  // Run the app
  runApp(MaterialApp(
      title: 'Veilid Plugin Demo',
      theme: newVeilidTheme(),
      home: const MyApp()));
}
