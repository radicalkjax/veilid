import 'dart:js_interop';

import 'package:web/web.dart' as web;

/// Periodic ticker backed by the browser's `window.setInterval` on web.
///
/// A `dart:async` `Timer` registers with the VM-service `Timer` stream, which
/// `flutter drive -d chrome`'s dwds debug connection tries to listen to and
/// fails on ("The stream `Timer` is not supported on web devices"), aborting
/// the test connection. `setInterval` is a raw browser timer that sidesteps it.
class PeriodicTicker {
  int? _intervalId;

  void start(Duration interval, void Function() onTick) {
    _intervalId = web.window
        .setInterval((() => onTick()).toJS, interval.inMilliseconds.toJS);
  }

  void stop() {
    final id = _intervalId;
    if (id != null) {
      web.window.clearInterval(id);
      _intervalId = null;
    }
  }
}
