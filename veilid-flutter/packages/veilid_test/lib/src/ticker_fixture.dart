import 'package:async_tools/async_tools.dart';

import 'periodic_ticker.dart';
import 'update_processor_fixture.dart';

abstract class TickerFixtureTickable {
  Future<void> onTick();
}

class TickerFixture {
  static final _fixtureMutex = Mutex();

  UpdateProcessorFixture updateProcessorFixture;

  final PeriodicTicker _ticker = PeriodicTicker();

  final List<TickerFixtureTickable> _tickables = [];

  TickerFixture({required this.updateProcessorFixture});

  Future<void> setUp() async {
    await _fixtureMutex.acquire();
    _ticker.start(const Duration(seconds: 1), () {
      singleFuture(this, _onTick);
    });
  }

  Future<void> tearDown() async {
    assert(_fixtureMutex.isLocked, 'should not tearDown without setUp');
    _ticker.stop();
    _fixtureMutex.release();
  }

  void register(TickerFixtureTickable tickable) {
    _tickables.add(tickable);
  }

  void unregister(TickerFixtureTickable tickable) {
    _tickables.remove(tickable);
  }

  Future<void> _onTick() async {
    await _tickables.map((t) => t.onTick()).wait;
  }
}
