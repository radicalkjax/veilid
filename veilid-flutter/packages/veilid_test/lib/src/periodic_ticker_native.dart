import 'dart:async';

/// Periodic ticker backed by a `dart:async` [Timer] on native platforms.
class PeriodicTicker {
  Timer? _timer;

  void start(Duration interval, void Function() onTick) {
    _timer = Timer.periodic(interval, (_) => onTick());
  }

  void stop() {
    _timer?.cancel();
    _timer = null;
  }
}
