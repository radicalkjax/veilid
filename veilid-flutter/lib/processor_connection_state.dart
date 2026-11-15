import 'package:freezed_annotation/freezed_annotation.dart';

import 'veilid_state.dart';

part 'processor_connection_state.freezed.dart';

/// Snapshot of the Veilid attachment + network state surfaced by a
/// [VeilidProcessorInterface]. Mirrors what consumers care about for retry policy
/// and connection-meter UI without exposing the underlying update stream.
@freezed
sealed class ProcessorConnectionState with _$ProcessorConnectionState {
  const factory ProcessorConnectionState({
    required VeilidStateAttachment attachment,
    required VeilidStateNetwork network,
  }) = _ProcessorConnectionState;
  const ProcessorConnectionState._();

  /// Attached at any signal strength (excludes detaching/attaching transitions).
  bool get isAttached =>
      !(attachment.state == AttachmentState.detached ||
          attachment.state == AttachmentState.detaching ||
          attachment.state == AttachmentState.attaching);

  /// Fully detached from the network.
  bool get isDetached => attachment.state == AttachmentState.detached;

  /// PublicInternet routing domain is ready for all operations.
  bool get isPublicInternetReady => attachment.publicInternetReady;
}
