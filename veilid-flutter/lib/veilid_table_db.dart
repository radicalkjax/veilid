import 'dart:async';
import 'dart:convert';
import 'dart:typed_data';

/////////////////////////////////////
/// VeilidTableDB

/// A TableDB transaction. Atomically commits a group of writes or deletes to
/// the TableDB. Must be committed or rolled back before being discarded.
///
/// Holds a native transaction handle; [commit] or [rollback] releases it.
/// Dropping it uncommitted leaks the handle until a finalizer reclaims it.
abstract class VeilidTableDBTransaction {
  /// Whether the transaction has been committed or rolled back.
  bool get isDone;

  /// Commit the transaction, performing all actions atomically. An empty
  /// transaction commits as a no-op.
  ///
  /// Awaits the store, then releases the handle. Terminal: once committed or
  /// rolled back, every method except [isDone] throws.
  ///
  /// Throws VeilidAPIExceptionGeneric if already committed or rolled back, or if
  /// the atomic backing-store write fails (the buffered writes are then lost).
  Future<void> commit();

  /// Roll back the transaction. Does nothing to the TableDB.
  ///
  /// Releases the handle. Terminal: once rolled back or committed, every method
  /// except [isDone] throws.
  Future<void> rollback();

  /// Store [value] under [key] in column [col]. Buffered, not written until
  /// [commit].
  ///
  /// Throws VeilidAPIExceptionGeneric if [col] is at or above the opened column
  /// count, or if the transaction is already committed or rolled back.
  Future<void> store(int col, Uint8List key, Uint8List value);

  /// Delete [key] from column [col]. Buffered, not written until [commit].
  ///
  /// Throws VeilidAPIExceptionGeneric if [col] is at or above the opened column
  /// count, or if the transaction is already committed or rolled back.
  Future<void> delete(int col, Uint8List key);

  /// Store [object] as JSON under [key] in column [col]. Buffered, not written
  /// until [commit].
  ///
  /// Throws [JsonUnsupportedObjectError] if [object] cannot be JSON-encoded,
  /// otherwise the same errors as [store].
  Future<void> storeJson(
    int col,
    Uint8List key,
    Object? object, {
    Object? Function(Object? nonEncodable)? toEncodable,
  }) => store(
    col,
    key,
    utf8.encoder.convert(jsonEncode(object, toEncodable: toEncodable)),
  );

  /// Store [object] as JSON under the UTF-8 encoding of [key] in column [col].
  /// Buffered, not written until [commit].
  ///
  /// Throws [JsonUnsupportedObjectError] if [object] cannot be JSON-encoded,
  /// otherwise the same errors as [store].
  Future<void> storeStringJson(
    int col,
    String key,
    Object? object, {
    Object? Function(Object? nonEncodable)? toEncodable,
  }) => storeJson(
    col,
    utf8.encoder.convert(key),
    object,
    toEncodable: toEncodable,
  );
}

/// A handle to an opened encrypted key-value table, organized into columns.
///
/// The caller must [close] this handle when done; otherwise it leaks until a
/// finalizer reclaims it.
abstract class VeilidTableDB {
  /// Release this handle to the table. No-op if already closed.
  void close();

  /// Total number of columns the table can hold (not just those opened).
  int get columnCount;

  /// The list of keys in column [col].
  ///
  /// Throws VeilidAPIExceptionGeneric if [col] is at or above the opened column
  /// count, if the backing-store read fails, or if a stored key fails to decrypt
  /// or decompress (wrong device encryption key or corrupt data).
  Future<List<Uint8List>> getKeys(int col);

  /// Start a write transaction. The caller must [VeilidTableDBTransaction.commit]
  /// or [VeilidTableDBTransaction.rollback] the returned handle.
  VeilidTableDBTransaction transact();

  /// Store [value] under [key] in column [col]. Committed immediately.
  ///
  /// Throws VeilidAPIExceptionGeneric if [col] is at or above the opened column
  /// count or the backing-store write fails.
  Future<void> store(int col, Uint8List key, Uint8List value);

  /// Read [key] from column [col], or null if absent.
  ///
  /// Throws VeilidAPIExceptionGeneric if [col] is at or above the opened column
  /// count, if the backing-store read fails, or if the stored value fails to
  /// decrypt or decompress (wrong device encryption key or corrupt data).
  Future<Uint8List?> load(int col, Uint8List key);

  /// Delete [key] from column [col], returning its prior value if any.
  ///
  /// Throws VeilidAPIExceptionGeneric if [col] is at or above the opened column
  /// count, if the backing-store delete fails, or if the prior value fails to
  /// decrypt or decompress (wrong device encryption key or corrupt data).
  Future<Uint8List?> delete(int col, Uint8List key);

  /// Store [object] as JSON under [key] in column [col]. Committed immediately.
  ///
  /// Throws [JsonUnsupportedObjectError] if [object] cannot be JSON-encoded,
  /// otherwise the same errors as [store].
  Future<void> storeJson(
    int col,
    Uint8List key,
    Object? object, {
    Object? Function(Object? nonEncodable)? toEncodable,
  }) => store(
    col,
    key,
    utf8.encoder.convert(jsonEncode(object, toEncodable: toEncodable)),
  );

  /// Store [object] as JSON under the UTF-8 encoding of [key] in column [col].
  /// Committed immediately.
  ///
  /// Throws [JsonUnsupportedObjectError] if [object] cannot be JSON-encoded,
  /// otherwise the same errors as [store].
  Future<void> storeStringJson(
    int col,
    String key,
    Object? object, {
    Object? Function(Object? nonEncodable)? toEncodable,
  }) => storeJson(
    col,
    utf8.encoder.convert(key),
    object,
    toEncodable: toEncodable,
  );

  /// Read and JSON-decode [key] from column [col], or null if absent.
  ///
  /// Throws [FormatException] if the stored bytes are not valid UTF-8 JSON,
  /// otherwise the same errors as [load].
  Future<Object?> loadJson(
    int col,
    Uint8List key, {
    Object? Function(Object? key, Object? value)? reviver,
  }) async {
    final s = await load(col, key);
    if (s == null) {
      return null;
    }
    return jsonDecode(utf8.decode(s, allowMalformed: false), reviver: reviver);
  }

  /// Read and JSON-decode the UTF-8 encoding of [key] from column [col], or
  /// null if absent.
  ///
  /// Throws [FormatException] if the stored bytes are not valid UTF-8 JSON,
  /// otherwise the same errors as [load].
  Future<Object?> loadStringJson(
    int col,
    String key, {
    Object? Function(Object? key, Object? value)? reviver,
  }) => loadJson(col, utf8.encoder.convert(key), reviver: reviver);

  /// Delete [key] from column [col], returning its JSON-decoded prior value if
  /// any.
  ///
  /// Throws [FormatException] if the stored bytes are not valid UTF-8 JSON,
  /// otherwise the same errors as [delete].
  Future<Object?> deleteJson(
    int col,
    Uint8List key, {
    Object? Function(Object? key, Object? value)? reviver,
  }) async {
    final s = await delete(col, key);
    if (s == null) {
      return null;
    }
    return jsonDecode(utf8.decode(s, allowMalformed: false), reviver: reviver);
  }

  /// Delete the UTF-8 encoding of [key] from column [col], returning its
  /// JSON-decoded prior value if any.
  ///
  /// Throws [FormatException] if the stored bytes are not valid UTF-8 JSON,
  /// otherwise the same errors as [delete].
  Future<Object?> deleteStringJson(
    int col,
    String key, {
    Object? Function(Object? key, Object? value)? reviver,
  }) => deleteJson(col, utf8.encoder.convert(key), reviver: reviver);
}
