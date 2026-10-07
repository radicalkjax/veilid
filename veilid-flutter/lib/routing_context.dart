import 'dart:async';
import 'dart:typed_data';

import 'veilid.dart';

//////////////////////////////////////
/// VeilidRoutingContext

/// Specifies the communication preferences for messages sent over the Veilid
/// network.
///
/// By default routing contexts have safety routing enabled, which offers sender
/// privacy. To receive privacy, send to an imported private route rather than
/// directly to a node id.
abstract class VeilidRoutingContext {
  /// Release this routing context and its resources.
  ///
  /// You must close every context you create; otherwise the native handle
  /// leaks (a finalizer is a backstop, not a guarantee). Closing an
  /// already-closed context is a no-op.
  void close();

  // Modifiers

  /// Get a copy of this context with default safety, sequencing, and stability
  /// parameters. Closes this context if [closeSelf] is set.
  ///
  /// The returned context is a separate handle you must [close] (or pass
  /// [closeSelf] to release this one in the same call).
  VeilidRoutingContext withDefaultSafety({bool closeSelf = false});

  /// Get a copy of this context using the given [safetySelection]. Can disable
  /// safety via [SafetySelectionUnsafe]. Closes this context if [closeSelf] is
  /// set.
  ///
  /// The returned context is a separate handle you must [close] (or pass
  /// [closeSelf] to release this one in the same call).
  ///
  /// Throws [VeilidAPIExceptionGeneric] if unsafe routing is requested without
  /// the `footgun-nodeid-target` feature, or if the hop count exceeds the
  /// configured max route hop count; [VeilidAPIExceptionInvalidArgument] if a
  /// preferred route is set whose id is an unsupported crypto kind or the wrong
  /// length.
  VeilidRoutingContext withSafety(
    SafetySelection safetySelection, {
    bool closeSelf = false,
  });

  /// Get a copy of this context using the given [sequencing] preference, with or
  /// without privacy. Closes this context if [closeSelf] is set.
  ///
  /// The returned context is a separate handle you must [close] (or pass
  /// [closeSelf] to release this one in the same call).
  VeilidRoutingContext withSequencing(
    Sequencing sequencing, {
    bool closeSelf = false,
  });

  /// Get the safety selection in use on this routing context.
  Future<SafetySelection> safety();

  // App call/message

  /// Send a bidirectional app-level [request] to [target] and await the answer
  /// blob. Up to 32768 bytes each way.
  ///
  /// Blocks on the network awaiting the reply; governed by
  /// `network.rpc.timeout_ms`.
  ///
  /// Throws [VeilidAPIExceptionInvalidTarget] if [target] is a node id (only a
  /// route id is permitted without the `footgun-nodeid-target` feature);
  /// [VeilidAPIExceptionNoConnection] if the route could not be resolved or
  /// allocated (retry), [VeilidAPIExceptionTimeout] if the reply deadline
  /// elapsed (retry), or [VeilidAPIExceptionTryAgain] if a route is temporarily
  /// unavailable (retry).
  Future<Uint8List> appCall(Target target, Uint8List request);

  /// Send a unidirectional app-level [message] to [target]. Up to 32768 bytes.
  ///
  /// Sends over the network but does not await a reply; returns once the
  /// message is dispatched.
  ///
  /// Throws [VeilidAPIExceptionInvalidTarget] if [target] is a node id (only a
  /// route id is permitted without the `footgun-nodeid-target` feature);
  /// [VeilidAPIExceptionNoConnection] if the route could not be resolved or
  /// allocated (retry), [VeilidAPIExceptionTimeout] if dispatch timed out
  /// (retry), or [VeilidAPIExceptionTryAgain] if a route is temporarily
  /// unavailable (retry).
  Future<void> appMessage(Target target, Uint8List message);

  // DHT Operations

  /// Create a new DHT record with the given crypto [kind] and [schema].
  ///
  /// The record is open after this succeeds. An [owner] keypair may be supplied,
  /// otherwise a random one is generated; its crypto kind must match [kind].
  ///
  /// Local-only: builds the record in the local store without network fanout.
  /// The returned record is left open; close it with [closeDHTRecord] or the
  /// open handle leaks.
  ///
  /// Throws [VeilidAPIExceptionGeneric] if [kind] is an unsupported crypto kind
  /// or [owner] is a malformed keypair; [VeilidAPIExceptionInvalidArgument] if
  /// [schema] has an invalid subkey/member/writer count, [owner] is the wrong
  /// crypto kind for [kind], or this node's id would be a schema member;
  /// [VeilidAPIExceptionNotInitialized] if the node is shut down.
  Future<DHTRecordDescriptor> createDHTRecord(
    CryptoKind kind,
    DHTSchema schema, {
    KeyPair? owner,
  });

  /// Open the DHT record at [key], optionally associating a [writer] keypair to
  /// grant write capability. Re-opening replaces the writer and routing context.
  ///
  /// Half of an open/close pair: close it with [closeDHTRecord] or the open
  /// handle and its watches leak. Re-opening an already-open record is safe and
  /// replaces the writer and safety selection in place, preserving active
  /// watches. Returns from the local store without a network round-trip when the
  /// record is already local; otherwise blocks on a network inspect and returns
  /// `TryAgain` if offline.
  ///
  /// Throws [VeilidAPIExceptionGeneric] if [key] is an unsupported kind or
  /// malformed, or [writer] is a malformed keypair; [VeilidAPIExceptionTryAgain]
  /// if the record is not yet local and the node is offline (retry);
  /// [VeilidAPIExceptionKeyNotFound] if the record does not exist on the
  /// network; [VeilidAPIExceptionNotInitialized] if the node is shut down.
  Future<DHTRecordDescriptor> openDHTRecord(RecordKey key, {KeyPair? writer});

  /// Close an opened DHT record so it can be re-opened with a different routing
  /// context.
  ///
  /// The release half of the open/close pair: cancels the record's watch (in
  /// the background) and drops any associated transaction. Blocks holding the
  /// record lock until pending writes flush to the local store. Closing a record
  /// that is local but not currently open is a no-op; closing one not in the
  /// local store throws [VeilidAPIExceptionKeyNotFound].
  ///
  /// Also throws [VeilidAPIExceptionGeneric] if [key] is an unsupported kind or
  /// malformed, or [VeilidAPIExceptionNotInitialized] if the node is shut down.
  /// None of these are retryable.
  Future<void> closeDHTRecord(RecordKey key);

  /// Delete the local storage for a DHT record. Does not delete it from the
  /// network. The record must be closed first.
  ///
  /// Local-only: removes the record from the local store with no network
  /// round-trip.
  ///
  /// Throws [VeilidAPIExceptionGeneric] if [key] is an unsupported kind or
  /// malformed; [VeilidAPIExceptionKeyNotFound] if the record is not in the
  /// local store; [VeilidAPIExceptionNotInitialized] if the node is shut down.
  /// None are retryable.
  Future<void> deleteDHTRecord(RecordKey key);

  /// Get the latest value of [subkey].
  ///
  /// Set [forceRefresh] to force a network refresh. Returns `null` if the subkey
  /// has not yet been set.
  ///
  /// Non-blocking when a local value exists and [forceRefresh] is false;
  /// otherwise blocks on a network fanout and returns `TryAgain` if offline.
  ///
  /// Throws [VeilidAPIExceptionInvalidArgument] if the record is not open;
  /// [VeilidAPIExceptionGeneric] if [key] is an unsupported kind or malformed;
  /// [VeilidAPIExceptionTryAgain] if a network refresh is needed and the node is
  /// offline (retry); [VeilidAPIExceptionKeyNotFound] if the record no longer
  /// exists; [VeilidAPIExceptionNotInitialized] if shut down.
  Future<ValueData?> getDHTValue(
    RecordKey key,
    int subkey, {
    bool forceRefresh = false,
  });

  /// Push a changed [subkey] value to the network.
  ///
  /// Returns `null` on success, or the newer network [ValueData] if the set was
  /// older than what is already on the network.
  ///
  /// Blocks on a network fanout to push the value; when offline or the fanout
  /// fails, queues the write for a later flush (if offline writes are allowed)
  /// and returns `null`.
  ///
  /// Throws [VeilidAPIExceptionInvalidArgument] if the record is not open;
  /// [VeilidAPIExceptionGeneric] if [key] is an unsupported kind or malformed,
  /// the record is not writable (no writer), or the value fails schema
  /// validation (subkey out of schema range, [data] over the per-subkey size
  /// limit, or wrong writer for the subkey); [VeilidAPIExceptionTryAgain] if the
  /// record is currently in a
  /// transaction (retry); [VeilidAPIExceptionKeyNotFound] if the record no
  /// longer exists; [VeilidAPIExceptionNotInitialized] if shut down. A failed
  /// network fanout does not throw; the write is deferred and returns `null`.
  Future<ValueData?> setDHTValue(
    RecordKey key,
    int subkey,
    Uint8List data, {
    SetDHTValueOptions? options,
  });

  /// Add or update a watch on a DHT record, delivering changes via
  /// `VeilidUpdateValueChange`.
  ///
  /// [subkeys] defaults to the entire range. [expiration] is a desired
  /// termination timestamp, [count] the maximum number of updates. Returns
  /// whether a watch is active.
  ///
  /// Only one watch exists per record; re-watching the same record replaces the
  /// prior watch's desired parameters in place. Records the desired watch state
  /// and returns without a network round-trip; a background task reconciles it
  /// with a remote node.
  ///
  /// Throws [VeilidAPIExceptionInvalidArgument] if the record is not open or a
  /// non-zero [expiration] is sooner than `network.rpc.timeout_ms` in the
  /// future; [VeilidAPIExceptionGeneric] if [key] is an unsupported kind or
  /// malformed, or no local record is found; [VeilidAPIExceptionNotInitialized]
  /// if shut down. None are retryable; no network errors surface here since
  /// reconciliation is deferred to a background task.
  Future<bool> watchDHTValues(
    RecordKey key, {
    List<ValueSubkeyRange>? subkeys,
    Timestamp? expiration,
    int? count,
  });

  /// Cancel a watch over [subkeys], subtracting them from the watched range.
  /// Returns whether a watch is still active.
  ///
  /// A no-op returning `false` when no watch is active for the record. Records
  /// the reduced desired watch state and returns without a network round-trip; a
  /// background task sends the cancel.
  ///
  /// Throws [VeilidAPIExceptionInvalidArgument] if the record is not open;
  /// [VeilidAPIExceptionGeneric] if [key] is an unsupported kind or malformed;
  /// [VeilidAPIExceptionNotInitialized] if shut down.
  Future<bool> cancelDHTWatch(RecordKey key, {List<ValueSubkeyRange>? subkeys});

  /// Inspect a DHT record's subkey state for [subkeys] at the given [scope].
  ///
  /// Useful for deciding whether to push subkeys to or pull them from the
  /// network.
  ///
  /// [DHTReportScope.local] (the default) is local-only and non-blocking; the
  /// sync/update scopes block on a network inspect fanout and return `TryAgain`
  /// if offline.
  ///
  /// Throws [VeilidAPIExceptionInvalidArgument] if the record is not open;
  /// [VeilidAPIExceptionGeneric] if [key] is an unsupported kind or malformed;
  /// [VeilidAPIExceptionTryAgain] if a sync/update scope needs the network and
  /// the node is offline (retry); [VeilidAPIExceptionNotInitialized] if shut
  /// down.
  Future<DHTRecordReport> inspectDHTRecord(
    RecordKey key, {
    List<ValueSubkeyRange>? subkeys,
    DHTReportScope scope = DHTReportScope.local,
  });

  /// Wait for pending offline subkey writes to flush to the network.
  ///
  /// Returns `true` once flushed, or `false` if [timeout] elapses first.
  ///
  /// Blocks until pending writes flush, the [timeout] elapses, or shutdown;
  /// returns immediately when there are no pending writes. With no [timeout],
  /// waits indefinitely.
  ///
  /// Throws [VeilidAPIExceptionGeneric] if [key] is an unsupported kind or
  /// malformed, or [VeilidAPIExceptionNotInitialized] if the node shuts down
  /// while waiting.
  Future<bool> flushDHTRecord(RecordKey key, {Duration? timeout});
}
