import 'dart:async';
import 'dart:typed_data';

import 'package:freezed_annotation/freezed_annotation.dart';

import 'veilid.dart';

part 'veilid_dht_transaction.freezed.dart';
part 'veilid_dht_transaction.g.dart';

//////////////////////////////////////
/// DHTTransactionSetValueOptions

/// Options for a transactional set operation.
@freezed
sealed class DHTTransactionSetValueOptions
    with _$DHTTransactionSetValueOptions {
  /// Builds set options, optionally overriding the record's default [writer].
  const factory DHTTransactionSetValueOptions({KeyPair? writer}) =
      _DHTTransactionSetValueOptions;

  /// Decodes options from JSON.
  factory DHTTransactionSetValueOptions.fromJson(dynamic json) =>
      _$DHTTransactionSetValueOptionsFromJson(json as Map<String, dynamic>);

  @override
  Map<String, dynamic> toJson() => {'writer': writer};
}

//////////////////////////////////////
/// VeilidDHTTransaction

/// Performs multiple atomic operations over a set of DHT records.
///
/// Bound operations all succeed or fail together. Transactional operations work
/// only while the node is online, throwing [VeilidAPIExceptionTryAgain] if
/// offline, and must be committed once all operations are registered, or rolled
/// back to cancel them.
///
/// Holds a native transaction handle; [commit] or [rollback] releases it.
/// Dropping the handle without committing leaks it until a finalizer reclaims it
/// (and the transaction never commits).
abstract class VeilidDHTTransaction {
  /// Whether the transaction has been committed or rolled back.
  bool get isDone;

  /// Commit the transaction, performing all write operations atomically.
  ///
  /// Awaits the network, then releases the handle. Terminal: once committed or
  /// rolled back, every method except [isDone] throws.
  ///
  /// Throws [VeilidAPIExceptionTransactionNotFound] if already committed, rolled
  /// back, or unknown, and [VeilidAPIExceptionTryAgain] (retry) if the node is
  /// offline or the end/commit barriers did not reach consensus.
  Future<void> commit();

  /// Roll the transaction back, performing no write operations.
  ///
  /// Awaits the network, then releases the handle. Terminal: once rolled back or
  /// committed, every method except [isDone] throws.
  ///
  /// Throws [VeilidAPIExceptionTransactionNotFound] if already committed, rolled
  /// back, or unknown, and [VeilidAPIExceptionTryAgain] (retry) if the node is
  /// offline.
  Future<void> rollback();

  /// Extend the transaction with additional [recordKeys].
  ///
  /// Blocks on the network. Idempotent for keys already in the transaction.
  ///
  /// Throws [VeilidAPIExceptionTransactionNotFound] if the transaction is already
  /// completed or unknown, [VeilidAPIExceptionMissingArgument] if [recordKeys]
  /// contains duplicates, [VeilidAPIExceptionInvalidArgument] if the merged set
  /// would exceed the per-transaction record limit, and
  /// [VeilidAPIExceptionTryAgain] (retry) if the node is offline or the begin
  /// fanout for the added records did not reach consensus.
  Future<void> extend(
    List<RecordKey> recordKeys, {
    TransactDHTRecordsOptions? options,
  });

  /// Get the network value of [subkey] inside the transaction.
  ///
  /// Returns `null` if the subkey has not yet been set. Blocks on the network.
  ///
  /// Throws [VeilidAPIExceptionTransactionNotFound] if the transaction is already
  /// completed or no longer in the begin stage,
  /// [VeilidAPIExceptionInvalidArgument] if [key] is not in the transaction or
  /// [subkey] is outside the schema range, [VeilidAPIExceptionGeneric] if [key]
  /// is malformed (unsupported kind or bad length) or the transaction has not
  /// started, and [VeilidAPIExceptionTryAgain] (retry) if the node is offline or
  /// the network did not return the value that existed at begin time. A
  /// non-responding node is retried rather than surfaced as a timeout.
  Future<ValueData?> get(RecordKey key, int subkey);

  /// Add a set operation for [subkey] to the transaction.
  ///
  /// Returns `null` on success, or the newer network [ValueData] if the set was
  /// older than what is already on the network. Blocks on the network.
  ///
  /// Throws [VeilidAPIExceptionTransactionNotFound] if the transaction is already
  /// completed or no longer in the begin stage,
  /// [VeilidAPIExceptionInvalidArgument] if [key] is not open in the transaction
  /// or [subkey] is outside the schema range,
  /// [VeilidAPIExceptionGeneric] if [key] is malformed (unsupported kind or bad
  /// length) or the subkey has no writer, and [VeilidAPIExceptionTryAgain]
  /// (retry) if the node is offline or write consensus was not reached this
  /// round. A non-responding node is retried rather than surfaced as a timeout.
  Future<ValueData?> set(
    RecordKey key,
    int subkey,
    Uint8List data, {
    DHTTransactionSetValueOptions? options,
  });

  /// Inspect a record's subkey state inside the transaction. Performs no network
  /// activity.
  ///
  /// Throws [VeilidAPIExceptionTransactionNotFound] if the transaction is already
  /// completed, unknown, or no longer in the begin stage,
  /// [VeilidAPIExceptionInvalidArgument] if [key] is not in the transaction, and
  /// [VeilidAPIExceptionGeneric] if [key] is malformed (unsupported kind or bad
  /// length) or the transaction has not started. Cannot time out.
  Future<DHTRecordReport> inspect(
    RecordKey key, {
    List<ValueSubkeyRange>? subkeys,
    DHTReportScope scope = DHTReportScope.local,
  });
}
