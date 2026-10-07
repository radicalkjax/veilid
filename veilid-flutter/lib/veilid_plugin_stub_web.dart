// ignore_for_file: public_member_api_docs
// Low-level platform binding to veilid-core; not part of the documented public API.

import 'dart:async';

import 'package:flutter/services.dart';
import 'package:flutter_web_plugins/flutter_web_plugins.dart';

/// A stub web implementation of the Veilid plugin
/// Because everything is done with FFI or WASM, we don't use this interface
class VeilidPluginStubWeb {
  static void registerWith(Registrar registrar) {}

  Future<dynamic> handleMethodCall(MethodCall call) async {
    throw PlatformException(
      code: 'Unimplemented',
      details: "Veilid for Web doesn't implement '${call.method}'",
    );
  }
}