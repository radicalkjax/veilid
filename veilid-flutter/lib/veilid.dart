import 'dart:async';
import 'dart:typed_data';

import 'package:equatable/equatable.dart';
import 'package:freezed_annotation/freezed_annotation.dart';

//////////////////////////////////////////////////////////

import 'routing_context.dart';
import 'veilid_config.dart';
import 'veilid_crypto.dart';
import 'veilid_dht_transaction.dart';
import 'veilid_state.dart';
import 'veilid_stub.dart'
    if (dart.library.io) 'veilid_ffi.dart'
    if (dart.library.js) 'veilid_js.dart';
import 'veilid_table_db.dart';
import 'veilid_timestamp.dart';
import 'veilid_types.dart';

export 'default_config.dart';
export 'processor_connection_state.dart';
export 'processor_interface.dart';
export 'retry.dart';
export 'routing_context.dart';
export 'value_subkey_range.dart';
export 'veilid.dart';
export 'veilid_api_exception.dart';
export 'veilid_config.dart';
export 'veilid_crypto.dart';
export 'veilid_dht_transaction.dart';
export 'veilid_encoding.dart';
export 'veilid_state.dart';
export 'veilid_table_db.dart';
export 'veilid_timestamp.dart';
export 'veilid_types.dart';

//////////////////////////////////////
/// VeilidVersion

/// The semantic version of veilid-core that this binding was built against.
@immutable
class VeilidVersion extends Equatable {
  /// Major version.
  final int major;

  /// Minor version.
  final int minor;

  /// Patch version.
  final int patch;

  /// Builds a version from its [major], [minor], and [patch] components.
  const VeilidVersion(this.major, this.minor, this.patch);

  @override
  List<Object> get props => [major, minor, patch];
}

//////////////////////////////////////
/// Veilid singleton factory

/// The primary developer entrypoint into veilid-core functionality.
///
/// From [Veilid] one can access cryptography, routing contexts, DHT operations,
/// private route allocation, the table store, and attach/detach from the network.
abstract class Veilid {
  /// The platform-appropriate [Veilid] singleton.
  static Veilid instance = getVeilid();

  /// Initialize veilid-core's logging and platform configuration.
  ///
  /// Must be called once before [startupVeilidCore].
  void initializeVeilidCore(Map<String, dynamic> platformConfigJson);

  /// Change the log filtering [directives] for the named log [layer].
  ///
  /// Throws [VeilidAPIExceptionParseError] if [layer] is not a known layer or
  /// [directives] cannot be parsed.
  void changeLogLevel(String layer, String directives);

  /// Change the per-target log ignore list for the named log [layer].
  @Deprecated("'ignore' syntax was confusing, migrate to changeLogLevel")
  void changeLogIgnore(String layer, List<String> changes);

  /// Start veilid-core with the given [config] and return the stream of
  /// [VeilidUpdate] events the node emits.
  ///
  /// Blocks on core initialization. The open half of the startup/shutdown pair:
  /// must be matched with [shutdownVeilidCore]. Throws if already started.
  ///
  /// Throws [VeilidAPIExceptionAlreadyInitialized] if a node is already running,
  /// [VeilidAPIExceptionGeneric] if [config] fails validation,
  /// [VeilidAPIExceptionInternal] if a subsystem fails to initialize. Fatal for
  /// this call; fix the config or environment and retry.
  Future<Stream<VeilidUpdate>> startupVeilidCore(VeilidConfig config);

  /// Get a full copy of the current state of Veilid.
  ///
  /// Throws [VeilidAPIExceptionNotInitialized] if Veilid has been shut down.
  Future<VeilidState> getVeilidState();

  /// Check to see if Veilid is already shut down.
  ///
  /// Local state check, does not block.
  Future<bool> isShutdown();

  /// Connect to the network.
  ///
  /// Sets the maintain-peers flag; the network connect proceeds in the
  /// background. Throws if already attached.
  ///
  /// Throws [VeilidAPIExceptionGeneric] if already attached, or
  /// [VeilidAPIExceptionNotInitialized] if Veilid has been shut down.
  Future<void> attach();

  /// Disconnect from the network.
  ///
  /// Clears the maintain-peers flag; the network teardown proceeds in the
  /// background. Throws if already detached.
  ///
  /// Throws [VeilidAPIExceptionGeneric] if already detached, or
  /// [VeilidAPIExceptionNotInitialized] if Veilid has been shut down.
  Future<void> detach();

  /// Shut down Veilid and terminate the API.
  ///
  /// Blocks until the core has finished shutting down.
  ///
  /// Throws [VeilidAPIExceptionNotInitialized] if Veilid was never started or
  /// has already been shut down.
  Future<void> shutdownVeilidCore();

  // Crypto

  /// The cryptosystem kinds supported by this build, best first.
  List<CryptoKind> validCryptoKinds();

  /// Get the cryptosystem for the given [kind].
  ///
  /// Throws [VeilidAPIExceptionGeneric] if [kind] is not in
  /// [validCryptoKinds].
  Future<VeilidCryptoSystem> getCryptoSystem(CryptoKind kind);

  /// Verify a set of signatures over [data] against [publicKeys].
  ///
  /// Returns the public keys whose signature validated, or `null` if any
  /// supported signature failed to validate.
  ///
  /// Throws [VeilidAPIExceptionNotInitialized] if Veilid has been shut down,
  /// [VeilidAPIExceptionGeneric] if a key or signature has the wrong length for
  /// its crypto kind, or [VeilidAPIExceptionParseError] if a key or signature is
  /// otherwise malformed.
  Future<List<PublicKey>?> verifySignatures(
    List<PublicKey> publicKeys,
    Uint8List data,
    List<Signature> signatures,
  );

  /// Generate signatures over [data] with [keyPairs].
  ///
  /// Key pairs of unsupported crypto kinds are silently dropped.
  ///
  /// Throws [VeilidAPIExceptionNotInitialized] if Veilid has been shut down,
  /// [VeilidAPIExceptionGeneric] if a key pair has the wrong length for its
  /// crypto kind, [VeilidAPIExceptionParseError] if a key pair is otherwise
  /// malformed, or [VeilidAPIExceptionInternal] if signing fails.
  Future<List<Signature>> generateSignatures(
    Uint8List data,
    List<KeyPair> keyPairs,
  );

  /// Generate a key pair for the given crypto [kind]. Does not require startup.
  ///
  /// Throws [VeilidAPIExceptionGeneric] if [kind] is not a supported crypto
  /// kind.
  Future<KeyPair> generateKeyPair(CryptoKind kind);

  // Routing context

  /// Get a new [VeilidRoutingContext] with default safety, sequencing, and
  /// stability parameters.
  ///
  /// Returns a handle holding a native routing context; the caller must
  /// [VeilidRoutingContext.close] it or the native context leaks.
  ///
  /// Throws [VeilidAPIExceptionNotInitialized] if Veilid has been shut down.
  Future<VeilidRoutingContext> routingContext();

  /// Get a [VeilidRoutingContext] with safety routing enabled and the given
  /// [stability] and [sequencing] preferences.
  ///
  /// Returns a handle the caller must [VeilidRoutingContext.close].
  ///
  /// Throws [VeilidAPIExceptionNotInitialized] if Veilid has been shut down.
  Future<VeilidRoutingContext> safeRoutingContext({
    Stability stability = Stability.lowLatency,
    Sequencing sequencing = Sequencing.preferOrdered,
  }) async {
    final rc = await routingContext();
    final originalSafety = await rc.safety() as SafetySelectionSafe;
    final safetySpec = originalSafety.safetySpec.copyWith(
      stability: stability,
      sequencing: sequencing,
    );
    return rc.withSafety(
      SafetySelectionSafe(safetySpec: safetySpec),
      closeSelf: true,
    );
  }

  /// Get a [VeilidRoutingContext] with safety routing disabled, sending RPC
  /// straight from the node with the given [sequencing] preference.
  ///
  /// Returns a handle the caller must [VeilidRoutingContext.close].
  ///
  /// Throws [VeilidAPIExceptionNotInitialized] if Veilid has been shut down.
  Future<VeilidRoutingContext> unsafeRoutingContext({
    Sequencing sequencing = Sequencing.preferOrdered,
  }) async => (await routingContext()).withSafety(
    SafetySelectionUnsafe(sequencing: sequencing),
  );

  // DHT operations

  /// Create a new [MemberId] for use in creating SMPL [DHTSchema]s.
  ///
  /// Throws [VeilidAPIExceptionGeneric] if [writerKey] names an unsupported
  /// crypto kind, or [VeilidAPIExceptionNotInitialized] if Veilid has been shut
  /// down.
  Future<MemberId> generateMemberId(PublicKey writerKey);

  /// Deterministically build the record key for a [schema] and [owner] key.
  ///
  /// The crypto kind of the returned key is that of the [owner] key. Local
  /// crypto only, no network round-trip.
  ///
  /// Throws [VeilidAPIExceptionInvalidArgument] if [schema] is malformed,
  /// [VeilidAPIExceptionGeneric] if [owner] or [encryptionKey] names an
  /// unsupported crypto kind, or [VeilidAPIExceptionNotInitialized] if Veilid
  /// has been shut down.
  Future<RecordKey> getDHTRecordKey(
    DHTSchema schema,
    PublicKey owner,
    SharedSecret? encryptionKey,
  );

  /// Start a transaction over a set of already-opened DHT [recordKeys].
  ///
  /// At most 32 records per transaction. [options] can supply a default signing
  /// keypair for records not opened for writing.
  ///
  /// Blocks on the network to begin the transaction (online-only). The returned
  /// handle holds a network-side resource the caller must release with
  /// [VeilidDHTTransaction.commit] or [VeilidDHTTransaction.rollback].
  ///
  /// Throws [VeilidAPIExceptionInvalidArgument] if a record is not open or more
  /// than 32 records are passed, [VeilidAPIExceptionMissingArgument] if
  /// [recordKeys] is empty or has duplicates, [VeilidAPIExceptionGeneric] if a
  /// record key is malformed or its encryption key does not match the opened
  /// record, [VeilidAPIExceptionTryAgain] if the DHT is offline, the records are
  /// contended, or begin consensus was not reached (retry), or
  /// [VeilidAPIExceptionNotInitialized] if Veilid has been shut down. Network
  /// failures surface as [VeilidAPIExceptionTimeout] or
  /// [VeilidAPIExceptionNoConnection].
  Future<VeilidDHTTransaction> transactDHTRecords(
    List<RecordKey> recordKeys, {
    TransactDHTRecordsOptions? options,
  });

  // Private route allocation

  /// Allocate a new private route set with default cryptography and network
  /// options, returning a route id and a publishable blob.
  ///
  /// Blocks on the network to allocate and test the route. The caller must free
  /// the route id with [releasePrivateRoute].
  ///
  /// Throws [VeilidAPIExceptionTryAgain] if there is no valid PublicInternet
  /// network class yet, not enough nodes are known to build the route, or the
  /// route failed its reachability test (retry), or
  /// [VeilidAPIExceptionNotInitialized] if Veilid has been shut down.
  Future<RouteBlob> newPrivateRoute();

  /// Allocate a new private route with the cryptosystem, stability, and
  /// sequencing preferences in [privateSpec], returning a route id and blob.
  ///
  /// Blocks on the network to allocate and test the route. The caller must free
  /// the route id with [releasePrivateRoute].
  ///
  /// Throws [VeilidAPIExceptionGeneric] if [privateSpec] names an invalid crypto
  /// kind, [VeilidAPIExceptionInvalidArgument] if its hop count exceeds the
  /// configured maximum, [VeilidAPIExceptionTryAgain] if there is no valid
  /// PublicInternet network class yet, not enough nodes are known to build the
  /// route, or the route failed its reachability test (retry), or
  /// [VeilidAPIExceptionNotInitialized] if Veilid has been shut down.
  Future<RouteBlob> newCustomPrivateRoute(PrivateSpec privateSpec);

  /// Import a private route [blob], returning a route id for sending private
  /// messages to the node that created the route.
  ///
  /// Local import, no network round-trip. The caller must free the route id
  /// with [releasePrivateRoute].
  ///
  /// Throws [VeilidAPIExceptionInvalidArgument] if [blob] is empty or names too
  /// many crypto kinds, [VeilidAPIExceptionParseError] if it is malformed,
  /// [VeilidAPIExceptionGeneric] if the decoded route has no first hop, or
  /// [VeilidAPIExceptionNotInitialized] if Veilid has been shut down.
  Future<RouteId> importRemotePrivateRoute(Uint8List blob);

  /// Release a locally allocated or remotely imported private route, freeing
  /// its resources.
  ///
  /// The release for [newPrivateRoute], [newCustomPrivateRoute], and
  /// [importRemotePrivateRoute]. Local, no network round-trip. Releasing an
  /// unknown or already-released route id throws.
  ///
  /// Throws [VeilidAPIExceptionInvalidArgument] if [routeId] is unknown, already
  /// released, or malformed, or [VeilidAPIExceptionNotInitialized] if Veilid has
  /// been shut down.
  Future<void> releasePrivateRoute(RouteId routeId);

  // App calls

  /// Reply to an AppCall identified by [callId] with an answer [message].
  ///
  /// Completes the call locally, does not block on the network. Each [callId]
  /// may be answered only once; an unknown or already-answered id throws.
  ///
  /// Throws [VeilidAPIExceptionGeneric] if [callId] is unknown or already
  /// answered, [VeilidAPIExceptionTryAgain] if the node is mid-shutdown, or
  /// [VeilidAPIExceptionNotInitialized] if Veilid has been shut down.
  Future<void> appCallReply(String callId, Uint8List message);

  // TableStore

  /// Open the named table database, creating it with [columnCount] columns if
  /// it does not exist.
  ///
  /// Returns a handle the caller must [VeilidTableDB.close].
  ///
  /// Throws [VeilidAPIExceptionInvalidArgument] if [columnCount] is zero or
  /// [name] contains characters other than alphanumeric, `_`, or `-`,
  /// [VeilidAPIExceptionGeneric] if the table is already open with a smaller
  /// column count (close it first) or the backing store fails to open, or
  /// [VeilidAPIExceptionNotInitialized] if Veilid has been shut down.
  Future<VeilidTableDB> openTableDB(String name, int columnCount);

  /// Delete the named table database. Returns whether it existed.
  ///
  /// Throws [VeilidAPIExceptionInvalidArgument] if [name] contains characters
  /// other than alphanumeric, `_`, or `-`, [VeilidAPIExceptionGeneric] if the
  /// table is still open (close all handles first) or the backing-store delete
  /// fails, or [VeilidAPIExceptionNotInitialized] if Veilid has been shut down.
  Future<bool> deleteTableDB(String name);

  // Misc

  /// Current wall-clock time. May move backward if the system clock is adjusted.
  Timestamp now();

  /// Current time, clamped to never read earlier than a previous call.
  Timestamp nowNonDecreasing();

  /// Current time, clamped to always read at least 1us later than a previous
  /// call.
  Timestamp nowIncreasing();

  /// The cargo package version of veilid-core in string format.
  String veilidVersionString();

  /// The cargo package version of veilid-core in tuple format.
  VeilidVersion veilidVersion();

  /// The default veilid-core configuration as a JSON string.
  String defaultVeilidConfig();

  /// Run a veilid-core debug [command] and return its text output.
  ///
  /// Throws [VeilidAPIExceptionNotInitialized] if Veilid has been shut down, or
  /// [VeilidAPIExceptionInvalidArgument]/[VeilidAPIExceptionMissingArgument] for
  /// an unrecognized command or bad arguments; a command run in the wrong node
  /// state throws [VeilidAPIExceptionInternal].
  Future<String> debug(String command);

  /// The features that were enabled when veilid-core was built.
  List<String> veilidFeatures();
}
