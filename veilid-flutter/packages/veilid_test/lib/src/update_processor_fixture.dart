import 'dart:async';

import 'package:async_tools/async_tools.dart';
import 'package:veilid/veilid.dart';

import 'veilid_fixture.dart';

class UpdateProcessorFixture implements VeilidProcessorInterface {
  static final _fixtureMutex = Mutex();

  VeilidFixture veilidFixture;

  @override
  ProcessorConnectionState processorConnectionState =
      ProcessorConnectionState(
    attachment: VeilidStateAttachment(
      state: AttachmentState.detached,
      publicInternetReady: false,
      localNetworkReady: false,
      uptime: TimestampDuration(value: BigInt.zero),
      attachedUptime: null,
      reliablePeerCount: BigInt.zero,
      livePeerCount: BigInt.zero,
      estimatedNetworkSize: BigInt.zero,
      medianLatency: null,
      overAttachedNodes: BigInt.zero,
    ),
    network: VeilidStateNetwork(
      started: false,
      bpsDown: BigInt.zero,
      bpsUp: BigInt.zero,
      peers: [],
    ),
  );

  final _connectionStateController =
      StreamController<ProcessorConnectionState>.broadcast(sync: true);

  StreamSubscription<VeilidUpdate>? _updateSubscription;

  @override
  Stream<ProcessorConnectionState> streamProcessorConnectionState() =>
      _connectionStateController.stream;

  UpdateProcessorFixture({required this.veilidFixture});

  Future<void> setUp() async {
    await _fixtureMutex.acquire();
    _updateSubscription = veilidFixture.updateStream.listen((update) {
      if (update is VeilidUpdateNetwork) {
        processorConnectionState = processorConnectionState.copyWith(
          network: VeilidStateNetwork(
            started: update.started,
            bpsDown: update.bpsDown,
            bpsUp: update.bpsUp,
            peers: update.peers,
          ),
        );
        _connectionStateController.add(processorConnectionState);
      } else if (update is VeilidUpdateAttachment) {
        processorConnectionState = processorConnectionState.copyWith(
          attachment: VeilidStateAttachment(
            state: update.state,
            publicInternetReady: update.publicInternetReady,
            localNetworkReady: update.localNetworkReady,
            uptime: update.uptime,
            attachedUptime: update.attachedUptime,
            reliablePeerCount: update.reliablePeerCount,
            livePeerCount: update.livePeerCount,
            estimatedNetworkSize: update.estimatedNetworkSize,
            medianLatency: update.medianLatency,
            overAttachedNodes: update.overAttachedNodes,
          ),
        );
        _connectionStateController.add(processorConnectionState);
      }
    });

    // Seed from current state so consumers don't wait on the next
    // attachment update push if attach() already completed.
    final state = await Veilid.instance.getVeilidState();
    processorConnectionState = processorConnectionState.copyWith(
      attachment: state.attachment,
      network: state.network,
    );
    _connectionStateController.add(processorConnectionState);
  }

  Future<void> tearDown() async {
    assert(_fixtureMutex.isLocked, 'should not tearDown without setUp');
    await _updateSubscription?.cancel();
    _updateSubscription = null;
    await _connectionStateController.close();
    _fixtureMutex.release();
  }
}
