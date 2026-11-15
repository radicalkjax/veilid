// ignore_for_file: public_member_api_docs
// Low-level platform binding to veilid-core; not part of the documented public API.
// @DefaultAsset below names the asset hook/build.dart declares as `assetName`.
@DefaultAsset('package:veilid/veilid_ffi.dart')
library;

import 'dart:async';
import 'dart:convert';
import 'dart:ffi';
import 'dart:isolate';
import 'dart:typed_data';

import 'package:ffi/ffi.dart';

import 'veilid.dart';

//////////////////////////////////////////////////////////

// Linkage for initialization
typedef _DartPostCObject =
    NativeFunction<Int8 Function(Int64, Pointer<Dart_CObject>)>;

// Return layout of veilid_version()
final class VeilidVersionFFI extends Struct {
  @Uint32()
  external int major;
  @Uint32()
  external int minor;
  @Uint32()
  external int patch;
}

// Async message types
const messageOk = 0;
const messageErr = 1;
const messageOkJson = 2;
const messageErrJson = 3;
const messageStreamItem = 4;
const messageStreamItemJson = 5;
const messageStreamAbort = 6;
const messageStreamAbortJson = 7;
const messageStreamClose = 8;

// Interface factory for high level Veilid API
Veilid getVeilid() => VeilidFFI();

/// Force-resolve every @Native symbol immediately. Per-symbol lazy resolution
/// re-opens the native asset path on first touch, and a rebuilt library file
/// swapped in at that path loads as a SECOND image with fresh statics whose
/// calls never complete. Binding everything here pins the whole API to the
/// image present at initialization, and makes a poisoned load fail at startup
/// instead of at a random later call.
void _eagerResolveNativeSymbols() {
  final resolved = <Object>[
    Native.addressOf<NativeFunction<Void Function(Pointer<Utf8>)>>(
      VeilidFFI._freeString,
    ),
    Native.addressOf<NativeFunction<Void Function(Pointer<Utf8>)>>(
      VeilidFFI._initializeVeilidCore,
    ),
    Native.addressOf<
      NativeFunction<Int32 Function(Pointer<Utf8>, Pointer<Utf8>)>
    >(VeilidFFI._changeLogLevel),
    Native.addressOf<
      NativeFunction<Void Function(Pointer<Utf8>, Pointer<Utf8>)>
    >(VeilidFFI._changeLogIgnore),
    Native.addressOf<
      NativeFunction<Void Function(Int64, Int64, Pointer<Utf8>)>
    >(VeilidFFI._startupVeilidCore),
    Native.addressOf<NativeFunction<Void Function(Int64)>>(
      VeilidFFI._isShutdown,
    ),
    Native.addressOf<NativeFunction<Void Function(Int64)>>(
      VeilidFFI._getVeilidState,
    ),
    Native.addressOf<NativeFunction<Void Function(Int64)>>(VeilidFFI._attach),
    Native.addressOf<NativeFunction<Void Function(Int64)>>(VeilidFFI._detach),
    Native.addressOf<NativeFunction<Void Function(Int64, Pointer<Utf8>)>>(
      VeilidFFI._changeConfig,
    ),
    Native.addressOf<NativeFunction<Void Function(Int64, Pointer<Utf8>)>>(
      VeilidFFI._reportAppLifecycleState,
    ),
    Native.addressOf<
      NativeFunction<Void Function(Int64, Pointer<Utf8>, Uint64)>
    >(VeilidFFI._waitForAttachmentCondition),
    Native.addressOf<NativeFunction<Void Function(Int64, Int64)>>(
      VeilidFFI._shutdownVeilidCore,
    ),
    Native.addressOf<NativeFunction<Void Function(Int64)>>(
      VeilidFFI._routingContext,
    ),
    Native.addressOf<NativeFunction<Int32 Function(Uint32)>>(
      VeilidFFI._releaseRoutingContext,
    ),
    Native.addressOf<NativeFunction<Uint32 Function(Uint32)>>(
      VeilidFFI._routingContextWithDefaultSafety,
    ),
    Native.addressOf<NativeFunction<Uint32 Function(Uint32, Pointer<Utf8>)>>(
      VeilidFFI._routingContextWithSafety,
    ),
    Native.addressOf<NativeFunction<Uint32 Function(Uint32, Pointer<Utf8>)>>(
      VeilidFFI._routingContextWithSequencing,
    ),
    Native.addressOf<NativeFunction<Void Function(Int64, Uint32)>>(
      VeilidFFI._routingContextSafety,
    ),
    Native.addressOf<
      NativeFunction<Void Function(Int64, Uint32, Pointer<Utf8>, Pointer<Utf8>)>
    >(VeilidFFI._routingContextAppCall),
    Native.addressOf<
      NativeFunction<Void Function(Int64, Uint32, Pointer<Utf8>, Pointer<Utf8>)>
    >(VeilidFFI._routingContextAppMessage),
    Native.addressOf<
      NativeFunction<
        Void Function(Int64, Uint32, Uint32, Pointer<Utf8>, Pointer<Utf8>)
      >
    >(VeilidFFI._routingContextCreateDHTRecord),
    Native.addressOf<
      NativeFunction<
        Void Function(
          Int64,
          Uint32,
          Pointer<Utf8>,
          Pointer<Utf8>,
          Pointer<Utf8>,
        )
      >
    >(VeilidFFI._routingContextOpenDHTRecord),
    Native.addressOf<
      NativeFunction<Void Function(Int64, Uint32, Pointer<Utf8>)>
    >(VeilidFFI._routingContextCloseDHTRecord),
    Native.addressOf<
      NativeFunction<Void Function(Int64, Uint32, Pointer<Utf8>)>
    >(VeilidFFI._routingContextDeleteDHTRecord),
    Native.addressOf<
      NativeFunction<Void Function(Int64, Uint32, Pointer<Utf8>, Uint32, Bool)>
    >(VeilidFFI._routingContextGetDHTValue),
    Native.addressOf<
      NativeFunction<
        Void Function(
          Int64,
          Uint32,
          Pointer<Utf8>,
          Uint32,
          Pointer<Utf8>,
          Pointer<Utf8>,
        )
      >
    >(VeilidFFI._routingContextSetDHTValue),
    Native.addressOf<
      NativeFunction<
        Void Function(
          Int64,
          Uint32,
          Pointer<Utf8>,
          Pointer<Utf8>,
          Uint64,
          Uint32,
        )
      >
    >(VeilidFFI._routingContextWatchDHTValues),
    Native.addressOf<
      NativeFunction<Void Function(Int64, Uint32, Pointer<Utf8>, Pointer<Utf8>)>
    >(VeilidFFI._routingContextCancelDHTWatch),
    Native.addressOf<
      NativeFunction<
        Void Function(
          Int64,
          Uint32,
          Pointer<Utf8>,
          Pointer<Utf8>,
          Pointer<Utf8>,
        )
      >
    >(VeilidFFI._routingContextInspectDHTRecord),
    Native.addressOf<
      NativeFunction<Void Function(Int64, Uint32, Pointer<Utf8>, Uint64)>
    >(VeilidFFI._routingContextFlushDHTRecord),
    Native.addressOf<NativeFunction<Void Function(Int64, Pointer<Utf8>)>>(
      VeilidFFI._generateMemberId,
    ),
    Native.addressOf<
      NativeFunction<
        Void Function(Int64, Pointer<Utf8>, Pointer<Utf8>, Pointer<Utf8>)
      >
    >(VeilidFFI._getDHTRecordKey),
    Native.addressOf<
      NativeFunction<Void Function(Int64, Pointer<Utf8>, Pointer<Utf8>)>
    >(VeilidFFI._transactDHTRecords),
    Native.addressOf<NativeFunction<Void Function(Int64)>>(
      VeilidFFI._newPrivateRoute,
    ),
    Native.addressOf<NativeFunction<Void Function(Int64, Pointer<Utf8>)>>(
      VeilidFFI._newCustomPrivateRoute,
    ),
    Native.addressOf<NativeFunction<Void Function(Int64, Pointer<Utf8>)>>(
      VeilidFFI._importRemotePrivateRoute,
    ),
    Native.addressOf<NativeFunction<Void Function(Int64, Pointer<Utf8>)>>(
      VeilidFFI._releasePrivateRoute,
    ),
    Native.addressOf<
      NativeFunction<Void Function(Int64, Pointer<Utf8>, Pointer<Utf8>)>
    >(VeilidFFI._appCallReply),
    Native.addressOf<NativeFunction<Int32 Function(Uint32)>>(
      VeilidFFI._releaseDHTTransaction,
    ),
    Native.addressOf<NativeFunction<Void Function(Int64, Uint32)>>(
      VeilidFFI._dhtTransactionCommit,
    ),
    Native.addressOf<NativeFunction<Void Function(Int64, Uint32)>>(
      VeilidFFI._dhtTransactionRollback,
    ),
    Native.addressOf<
      NativeFunction<Void Function(Int64, Uint32, Pointer<Utf8>, Pointer<Utf8>)>
    >(VeilidFFI._dhtTransactionExtend),
    Native.addressOf<
      NativeFunction<Void Function(Int64, Uint32, Pointer<Utf8>, Uint32)>
    >(VeilidFFI._dhtTransactionGet),
    Native.addressOf<
      NativeFunction<
        Void Function(
          Int64,
          Uint32,
          Pointer<Utf8>,
          Uint32,
          Pointer<Utf8>,
          Pointer<Utf8>,
        )
      >
    >(VeilidFFI._dhtTransactionSet),
    Native.addressOf<
      NativeFunction<
        Void Function(
          Int64,
          Uint32,
          Pointer<Utf8>,
          Pointer<Utf8>,
          Pointer<Utf8>,
        )
      >
    >(VeilidFFI._dhtTransactionInspect),
    Native.addressOf<
      NativeFunction<Void Function(Int64, Pointer<Utf8>, Uint32)>
    >(VeilidFFI._openTableDb),
    Native.addressOf<NativeFunction<Int32 Function(Uint32)>>(
      VeilidFFI._releaseTableDb,
    ),
    Native.addressOf<NativeFunction<Void Function(Int64, Pointer<Utf8>)>>(
      VeilidFFI._deleteTableDb,
    ),
    Native.addressOf<
      NativeFunction<Void Function(Int64, Pointer<Utf8>, Pointer<Utf8>)>
    >(VeilidFFI._renameTableDb),
    Native.addressOf<NativeFunction<Uint32 Function(Uint32)>>(
      VeilidFFI._tableDbGetColumnCount,
    ),
    Native.addressOf<NativeFunction<Void Function(Int64, Uint32, Uint32)>>(
      VeilidFFI._tableDbGetKeys,
    ),
    Native.addressOf<
      NativeFunction<
        Void Function(Int64, Uint32, Uint32, Pointer<Utf8>, Pointer<Utf8>)
      >
    >(VeilidFFI._tableDbStore),
    Native.addressOf<
      NativeFunction<Void Function(Int64, Uint32, Uint32, Pointer<Utf8>)>
    >(VeilidFFI._tableDbLoad),
    Native.addressOf<
      NativeFunction<Void Function(Int64, Uint32, Uint32, Pointer<Utf8>)>
    >(VeilidFFI._tableDbDelete),
    Native.addressOf<NativeFunction<Uint32 Function(Uint32)>>(
      VeilidFFI._tableDbTransact,
    ),
    Native.addressOf<NativeFunction<Int32 Function(Uint32)>>(
      VeilidFFI._releaseTableDbTransaction,
    ),
    Native.addressOf<NativeFunction<Void Function(Int64, Uint32)>>(
      VeilidFFI._tableDbTransactionCommit,
    ),
    Native.addressOf<NativeFunction<Void Function(Int64, Uint32)>>(
      VeilidFFI._tableDbTransactionRollback,
    ),
    Native.addressOf<
      NativeFunction<
        Void Function(Int64, Uint32, Uint32, Pointer<Utf8>, Pointer<Utf8>)
      >
    >(VeilidFFI._tableDbTransactionStore),
    Native.addressOf<
      NativeFunction<Void Function(Int64, Uint32, Uint32, Pointer<Utf8>)>
    >(VeilidFFI._tableDbTransactionDelete),
    Native.addressOf<NativeFunction<Pointer<Utf8> Function()>>(
      VeilidFFI._validCryptoKinds,
    ),
    Native.addressOf<
      NativeFunction<
        Void Function(Int64, Pointer<Utf8>, Pointer<Utf8>, Pointer<Utf8>)
      >
    >(VeilidFFI._verifySignatures),
    Native.addressOf<
      NativeFunction<Void Function(Int64, Pointer<Utf8>, Pointer<Utf8>)>
    >(VeilidFFI._generateSignatures),
    Native.addressOf<NativeFunction<Void Function(Int64, Uint32)>>(
      VeilidFFI._generateKeyPair,
    ),
    Native.addressOf<
      NativeFunction<Void Function(Int64, Uint32, Pointer<Utf8>, Pointer<Utf8>)>
    >(VeilidFFI._cryptoCachedDH),
    Native.addressOf<
      NativeFunction<Void Function(Int64, Uint32, Pointer<Utf8>, Pointer<Utf8>)>
    >(VeilidFFI._cryptoComputeDH),
    Native.addressOf<
      NativeFunction<
        Void Function(
          Int64,
          Uint32,
          Pointer<Utf8>,
          Pointer<Utf8>,
          Pointer<Utf8>,
        )
      >
    >(VeilidFFI._cryptoGenerateSharedSecret),
    Native.addressOf<
      NativeFunction<
        Void Function(
          Int64,
          Uint32,
          Pointer<Utf8>,
          Pointer<Utf8>,
          Pointer<Utf8>,
        )
      >
    >(VeilidFFI._cryptoHpkeSeal),
    Native.addressOf<
      NativeFunction<
        Void Function(
          Int64,
          Uint32,
          Pointer<Utf8>,
          Pointer<Utf8>,
          Pointer<Utf8>,
        )
      >
    >(VeilidFFI._cryptoHpkeOpen),
    Native.addressOf<NativeFunction<Void Function(Int64, Uint32)>>(
      VeilidFFI._cryptoSharedSecretLength,
    ),
    Native.addressOf<NativeFunction<Void Function(Int64, Uint32)>>(
      VeilidFFI._cryptoNonceLength,
    ),
    Native.addressOf<NativeFunction<Void Function(Int64, Uint32)>>(
      VeilidFFI._cryptoHashDigestLength,
    ),
    Native.addressOf<NativeFunction<Void Function(Int64, Uint32)>>(
      VeilidFFI._cryptoPublicKeyLength,
    ),
    Native.addressOf<NativeFunction<Void Function(Int64, Uint32)>>(
      VeilidFFI._cryptoSecretKeyLength,
    ),
    Native.addressOf<NativeFunction<Void Function(Int64, Uint32)>>(
      VeilidFFI._cryptoEncapsulationKeyLength,
    ),
    Native.addressOf<NativeFunction<Void Function(Int64, Uint32)>>(
      VeilidFFI._cryptoDecapsulationKeyLength,
    ),
    Native.addressOf<NativeFunction<Void Function(Int64, Uint32)>>(
      VeilidFFI._cryptoSignatureLength,
    ),
    Native.addressOf<NativeFunction<Void Function(Int64, Uint32)>>(
      VeilidFFI._cryptoDefaultSaltLength,
    ),
    Native.addressOf<NativeFunction<Void Function(Int64, Uint32)>>(
      VeilidFFI._cryptoAeadOverhead,
    ),
    Native.addressOf<
      NativeFunction<Void Function(Int64, Uint32, Pointer<Utf8>)>
    >(VeilidFFI._cryptoCheckSharedSecret),
    Native.addressOf<
      NativeFunction<Void Function(Int64, Uint32, Pointer<Utf8>)>
    >(VeilidFFI._cryptoCheckNonce),
    Native.addressOf<
      NativeFunction<Void Function(Int64, Uint32, Pointer<Utf8>)>
    >(VeilidFFI._cryptoCheckHashDigest),
    Native.addressOf<
      NativeFunction<Void Function(Int64, Uint32, Pointer<Utf8>)>
    >(VeilidFFI._cryptoCheckPublicKey),
    Native.addressOf<
      NativeFunction<Void Function(Int64, Uint32, Pointer<Utf8>)>
    >(VeilidFFI._cryptoCheckSecretKey),
    Native.addressOf<
      NativeFunction<Void Function(Int64, Uint32, Pointer<Utf8>)>
    >(VeilidFFI._cryptoCheckSignature),
    Native.addressOf<NativeFunction<Void Function(Int64, Uint32, Uint32)>>(
      VeilidFFI._cryptoRandomBytes,
    ),
    Native.addressOf<
      NativeFunction<Void Function(Int64, Uint32, Pointer<Utf8>, Pointer<Utf8>)>
    >(VeilidFFI._cryptoHashPassword),
    Native.addressOf<
      NativeFunction<Void Function(Int64, Uint32, Pointer<Utf8>, Pointer<Utf8>)>
    >(VeilidFFI._cryptoVerifyPassword),
    Native.addressOf<
      NativeFunction<Void Function(Int64, Uint32, Pointer<Utf8>, Pointer<Utf8>)>
    >(VeilidFFI._cryptoDeriveSharedSecret),
    Native.addressOf<NativeFunction<Void Function(Int64, Uint32)>>(
      VeilidFFI._cryptoRandomNonce,
    ),
    Native.addressOf<NativeFunction<Void Function(Int64, Uint32)>>(
      VeilidFFI._cryptoRandomSharedSecret,
    ),
    Native.addressOf<NativeFunction<Void Function(Int64, Uint32)>>(
      VeilidFFI._cryptoGenerateKeyPair,
    ),
    Native.addressOf<NativeFunction<Void Function(Int64, Uint32)>>(
      VeilidFFI._cryptoGenerateKemKeyPair,
    ),
    Native.addressOf<
      NativeFunction<Void Function(Int64, Uint32, Pointer<Utf8>)>
    >(VeilidFFI._cryptoEncapsulationKeyFromSigningKey),
    Native.addressOf<
      NativeFunction<Void Function(Int64, Uint32, Pointer<Utf8>)>
    >(VeilidFFI._cryptoDecapsulationKeyFromSigningSecret),
    Native.addressOf<
      NativeFunction<Void Function(Int64, Uint32, Pointer<Utf8>)>
    >(VeilidFFI._cryptoGenerateHash),
    Native.addressOf<
      NativeFunction<Void Function(Int64, Uint32, Pointer<Utf8>, Pointer<Utf8>)>
    >(VeilidFFI._cryptoValidateKeyPair),
    Native.addressOf<
      NativeFunction<Void Function(Int64, Uint32, Pointer<Utf8>, Pointer<Utf8>)>
    >(VeilidFFI._cryptoValidateHash),
    Native.addressOf<
      NativeFunction<
        Void Function(
          Int64,
          Uint32,
          Pointer<Utf8>,
          Pointer<Utf8>,
          Pointer<Utf8>,
        )
      >
    >(VeilidFFI._cryptoSign),
    Native.addressOf<
      NativeFunction<
        Void Function(
          Int64,
          Uint32,
          Pointer<Utf8>,
          Pointer<Utf8>,
          Pointer<Utf8>,
        )
      >
    >(VeilidFFI._cryptoVerify),
    Native.addressOf<
      NativeFunction<
        Void Function(
          Int64,
          Uint32,
          Pointer<Utf8>,
          Pointer<Utf8>,
          Pointer<Utf8>,
          Pointer<Utf8>,
        )
      >
    >(VeilidFFI._cryptoDecryptAead),
    Native.addressOf<
      NativeFunction<
        Void Function(
          Int64,
          Uint32,
          Pointer<Utf8>,
          Pointer<Utf8>,
          Pointer<Utf8>,
          Pointer<Utf8>,
        )
      >
    >(VeilidFFI._cryptoEncryptAead),
    Native.addressOf<
      NativeFunction<
        Void Function(
          Int64,
          Uint32,
          Pointer<Utf8>,
          Pointer<Utf8>,
          Pointer<Utf8>,
        )
      >
    >(VeilidFFI._cryptoCryptNoAuth),
    Native.addressOf<NativeFunction<Uint64 Function()>>(VeilidFFI._now),
    Native.addressOf<NativeFunction<Uint64 Function()>>(
      VeilidFFI._nowNonDecreasing,
    ),
    Native.addressOf<NativeFunction<Uint64 Function()>>(
      VeilidFFI._nowIncreasing,
    ),
    Native.addressOf<NativeFunction<Void Function(Int64, Pointer<Utf8>)>>(
      VeilidFFI._debug,
    ),
    Native.addressOf<
      NativeFunction<Void Function(Int64, Pointer<Utf8>, Pointer<Utf8>)>
    >(VeilidFFI._saveUserSecret),
    Native.addressOf<NativeFunction<Void Function(Int64, Pointer<Utf8>)>>(
      VeilidFFI._loadUserSecret,
    ),
    Native.addressOf<NativeFunction<Void Function(Int64, Pointer<Utf8>)>>(
      VeilidFFI._removeUserSecret,
    ),
    Native.addressOf<NativeFunction<Void Function(Int64)>>(
      VeilidFFI._listUserSecrets,
    ),
    Native.addressOf<NativeFunction<Void Function(Int64)>>(
      VeilidFFI._deleteAllUserSecrets,
    ),
    Native.addressOf<NativeFunction<Pointer<Utf8> Function()>>(
      VeilidFFI._veilidVersionString,
    ),
    Native.addressOf<NativeFunction<VeilidVersionFFI Function()>>(
      VeilidFFI._veilidVersion,
    ),
    Native.addressOf<NativeFunction<Pointer<Utf8> Function()>>(
      VeilidFFI._defaultVeilidConfig,
    ),
    Native.addressOf<NativeFunction<Pointer<Utf8> Function()>>(
      VeilidFFI._veilidFeatures,
    ),
    Native.addressOf<
      NativeFunction<Void Function(Pointer<_DartPostCObject>, Pointer<Utf8>)>
    >(VeilidFFI._initializeVeilidFlutter),
  ];
  assert(resolved.isNotEmpty);
}

// Uint8List marshaling
Uint8List convertUint8ListFromJson(dynamic json) =>
    base64UrlNoPadDecode(json as String);
dynamic convertUint8ListToJson(Uint8List data) => base64UrlNoPadEncode(data);

// Parse handle async returns
Future<T> processFuturePlain<T>(Future<dynamic> future) => future
    .then((value) {
      final list = value as List<dynamic>;
      switch (list[0] as int) {
        case messageOk:
          {
            if (list[1] == null && null is! T) {
              throw const VeilidAPIExceptionInternal(
                'Null MESSAGE_OK value on non-nullable type',
              );
            }
            return list[1] as T;
          }
        case messageErr:
          {
            throw VeilidAPIExceptionInternal('Internal API Error: ${list[1]}');
          }
        case messageErrJson:
          {
            throw VeilidAPIException.fromJson(jsonDecode(list[1] as String));
          }
        default:
          {
            throw VeilidAPIExceptionInternal(
              'Unexpected async return message type: ${list[0]}',
            );
          }
      }
      // Any errors at all from Veilid need to be caught
      // ignore: inference_failure_on_untyped_parameter
    })
    .catchError((dynamic e, StackTrace s) {
      // Wrap all other errors in VeilidAPIExceptionInternal
      throw VeilidAPIExceptionInternal('$e\nStack Trace:\n$s');
    }, test: (e) => e is! VeilidAPIException);

Future<T> processFutureJson<T>(
  T Function(dynamic) jsonConstructor,
  Future<dynamic> future,
) => future
    .then((value) {
      final list = value as List<dynamic>;
      switch (list[0] as int) {
        case messageErr:
          {
            throw VeilidAPIExceptionInternal('Internal API Error: ${list[1]}');
          }
        case messageOkJson:
          {
            if (list[1] is! String) {
              throw const VeilidAPIExceptionInternal(
                'Non-string MESSAGE_OK_JSON value',
              );
            }
            final ret = jsonDecode(list[1] as String);
            if (ret == null) {
              throw const VeilidAPIExceptionInternal(
                'Null JSON object on non nullable type',
              );
            }
            return jsonConstructor(ret);
          }
        case messageErrJson:
          {
            throw VeilidAPIException.fromJson(jsonDecode(list[1] as String));
          }
        default:
          {
            throw VeilidAPIExceptionInternal(
              'Unexpected async return message type: ${list[0]}',
            );
          }
      }
      // Any errors at all from Veilid need to be caught
      // ignore: inference_failure_on_untyped_parameter
    })
    .catchError((dynamic e, StackTrace s) {
      // Wrap all other errors in VeilidAPIExceptionInternal
      throw VeilidAPIExceptionInternal('$e\nStack Trace:\n$s');
    }, test: (e) => e is! VeilidAPIException);

Future<T?> processFutureOptJson<T>(
  T Function(dynamic) jsonConstructor,
  Future<dynamic> future,
) => future
    .then((value) {
      final list = value as List<dynamic>;
      switch (list[0] as int) {
        case messageErr:
          {
            throw VeilidAPIExceptionInternal('Internal API Error: ${list[1]}');
          }
        case messageOkJson:
          {
            if (list[1] == null) {
              return null;
            }
            if (list[1] is! String) {
              throw const VeilidAPIExceptionInternal(
                'Non-string MESSAGE_OK_JSON optional value',
              );
            }
            final ret = jsonDecode(list[1] as String);
            if (ret == null) {
              return null;
            }
            return jsonConstructor(ret);
          }
        case messageErrJson:
          {
            throw VeilidAPIException.fromJson(jsonDecode(list[1] as String));
          }
        default:
          {
            throw VeilidAPIExceptionInternal(
              'Unexpected async return message type: ${list[0]}',
            );
          }
      }
      // Any errors at all from Veilid need to be caught
      // ignore: inference_failure_on_untyped_parameter
    })
    .catchError((dynamic e, StackTrace s) {
      // Wrap all other errors in VeilidAPIExceptionInternal
      throw VeilidAPIExceptionInternal('$e\nStack Trace:\n$s');
    }, test: (e) => e is! VeilidAPIException);

Future<void> processFutureVoid(Future<dynamic> future) => future
    .then((value) {
      final list = value as List<dynamic>;
      switch (list[0] as int) {
        case messageOk:
          {
            if (list[1] != null) {
              throw VeilidAPIExceptionInternal(
                'Unexpected MESSAGE_OK value'
                ' "${list[1]}" where null expected',
              );
            }
            return;
          }
        case messageErr:
          {
            throw VeilidAPIExceptionInternal('Internal API Error: ${list[1]}');
          }
        case messageOkJson:
          {
            final ret = jsonDecode(list[1] as String);
            if (ret != null) {
              throw VeilidAPIExceptionInternal(
                'Unexpected MESSAGE_OK_JSON value'
                ' "$ret" where null expected',
              );
            }
            return;
          }
        case messageErrJson:
          {
            throw VeilidAPIException.fromJson(jsonDecode(list[1] as String));
          }
        default:
          {
            throw VeilidAPIExceptionInternal(
              'Unexpected async return message type: ${list[0]}',
            );
          }
      }
      // Any errors at all from Veilid need to be caught
      // ignore: inference_failure_on_untyped_parameter
    })
    .catchError((dynamic e, StackTrace s) {
      // Wrap all other errors in VeilidAPIExceptionInternal
      throw VeilidAPIExceptionInternal('$e\nStack Trace:\n$s');
    }, test: (e) => e is! VeilidAPIException);

Future<Stream<T>> processFutureStream<T>(
  Stream<T> returnStream,
  Future<dynamic> future,
) => future
    .then((value) {
      final list = value as List<dynamic>;
      switch (list[0] as int) {
        case messageOk:
          {
            if (list[1] != null) {
              throw VeilidAPIExceptionInternal(
                'Unexpected MESSAGE_OK value'
                ' "${list[1]}" where null expected',
              );
            }
            return returnStream;
          }
        case messageErr:
          {
            throw VeilidAPIExceptionInternal('Internal API Error: ${list[1]}');
          }
        case messageOkJson:
          {
            final ret = jsonDecode(list[1] as String);
            if (ret != null) {
              throw VeilidAPIExceptionInternal(
                'Unexpected MESSAGE_OK_JSON value'
                ' "$ret" where null expected',
              );
            }
            return returnStream;
          }
        case messageErrJson:
          {
            throw VeilidAPIException.fromJson(jsonDecode(list[1] as String));
          }
        default:
          {
            throw VeilidAPIExceptionInternal(
              'Unexpected async return message type: ${list[0]}',
            );
          }
      }
      // Any errors at all from Veilid need to be caught
      // ignore: inference_failure_on_untyped_parameter
    })
    .catchError((dynamic e, StackTrace s) {
      // Wrap all other errors in VeilidAPIExceptionInternal
      throw VeilidAPIExceptionInternal('$e\nStack Trace:\n$s');
    }, test: (e) => e is! VeilidAPIException);

Stream<T> processStreamJson<T>(
  T Function(dynamic) jsonConstructor,
  ReceivePort port,
) async* {
  try {
    await for (final value in port) {
      // rust probes a suspect port with a bare int to tell a dead port from a
      // payload the dart serializer refused
      if (value is! List<dynamic>) {
        continue;
      }
      final list = value;
      switch (list[0] as int) {
        case messageStreamItemJson:
          {
            if (list[1] == null) {
              throw const VeilidAPIExceptionInternal(
                'Null MESSAGE_STREAM_ITEM_JSON value',
              );
            }
            final ret = jsonDecode(list[1] as String);
            yield jsonConstructor(ret);
            break;
          }
        case messageStreamAbort:
          {
            throw VeilidAPIExceptionInternal('Internal API Error: ${list[1]}');
          }
        case messageStreamAbortJson:
          {
            throw VeilidAPIException.fromJson(jsonDecode(list[1] as String));
          }
        case messageStreamClose:
          {
            port.close();
            break;
          }
        default:
          {
            throw VeilidAPIExceptionInternal(
              'Unexpected async return message type: ${list[0]}',
            );
          }
      }
    }
  } on VeilidAPIException catch (_) {
    rethrow;
  } on Exception catch (e, s) {
    // Wrap all other errors in VeilidAPIExceptionInternal
    throw VeilidAPIExceptionInternal('$e\nStack Trace:\n$s');
  } finally {
    port.close();
  }
}

class _Ctx {
  int? id;

  _Ctx(int this.id);

  void ensureValid() {
    if (id == null) {
      throw VeilidAPIExceptionNotInitialized();
    }
  }

  void close() {
    if (id != null) {
      VeilidFFI._releaseRoutingContext(id!);
      id = null;
    }
  }
}

// FFI implementation of VeilidRoutingContext
class VeilidRoutingContextFFI extends VeilidRoutingContext {
  final _Ctx _ctx;
  static final Finalizer<_Ctx> _finalizer = Finalizer((ctx) => ctx.close());

  VeilidRoutingContextFFI._(this._ctx) {
    _finalizer.attach(this, _ctx, detach: this);
  }

  @override
  void close() {
    _ctx.close();
  }

  @override
  VeilidRoutingContextFFI withDefaultSafety({bool closeSelf = false}) {
    _ctx.ensureValid();
    final newId = VeilidFFI._routingContextWithDefaultSafety(_ctx.id!);
    final out = VeilidRoutingContextFFI._(_Ctx(newId));
    if (closeSelf) {
      close();
    }
    return out;
  }

  @override
  VeilidRoutingContextFFI withSafety(
    SafetySelection safetySelection, {
    bool closeSelf = false,
  }) {
    _ctx.ensureValid();
    final nativeSafetySelection = jsonEncode(safetySelection).toNativeUtf8();
    final newId = VeilidFFI._routingContextWithSafety(
      _ctx.id!,
      nativeSafetySelection,
    );
    malloc.free(nativeSafetySelection);
    final out = VeilidRoutingContextFFI._(_Ctx(newId));
    if (closeSelf) {
      close();
    }
    return out;
  }

  @override
  VeilidRoutingContextFFI withSequencing(
    Sequencing sequencing, {
    bool closeSelf = false,
  }) {
    _ctx.ensureValid();
    final nativeSequencing = jsonEncode(sequencing).toNativeUtf8();
    final newId = VeilidFFI._routingContextWithSequencing(
      _ctx.id!,
      nativeSequencing,
    );
    malloc.free(nativeSequencing);
    final out = VeilidRoutingContextFFI._(_Ctx(newId));
    if (closeSelf) {
      close();
    }
    return out;
  }

  @override
  Future<SafetySelection> safety() async {
    _ctx.ensureValid();
    final recvPort = ReceivePort('routing_context_safety');
    final sendPort = recvPort.sendPort;
    VeilidFFI._routingContextSafety(sendPort.nativePort, _ctx.id!);
    final out = await processFutureJson<SafetySelection>(
      SafetySelection.fromJson,
      recvPort.first,
    );
    return out;
  }

  @override
  Future<Uint8List> appCall(Target target, Uint8List request) async {
    _ctx.ensureValid();
    final nativeTarget = jsonEncode(target).toNativeUtf8();
    final nativeEncodedRequest = base64UrlNoPadEncode(request).toNativeUtf8();

    final recvPort = ReceivePort('routing_context_app_call');
    final sendPort = recvPort.sendPort;
    VeilidFFI._routingContextAppCall(
      sendPort.nativePort,
      _ctx.id!,
      nativeTarget,
      nativeEncodedRequest,
    );
    malloc.free(nativeTarget);
    malloc.free(nativeEncodedRequest);
    final out = await processFuturePlain<String>(recvPort.first);
    return base64UrlNoPadDecode(out);
  }

  @override
  Future<void> appMessage(Target target, Uint8List message) {
    _ctx.ensureValid();
    final nativeTarget = jsonEncode(target).toNativeUtf8();
    final nativeEncodedMessage = base64UrlNoPadEncode(message).toNativeUtf8();

    final recvPort = ReceivePort('routing_context_app_message');
    final sendPort = recvPort.sendPort;
    VeilidFFI._routingContextAppMessage(
      sendPort.nativePort,
      _ctx.id!,
      nativeTarget,
      nativeEncodedMessage,
    );
    malloc.free(nativeTarget);
    malloc.free(nativeEncodedMessage);
    return processFutureVoid(recvPort.first);
  }

  @override
  Future<DHTRecordDescriptor> createDHTRecord(
    CryptoKind kind,
    DHTSchema schema, {
    KeyPair? owner,
  }) async {
    _ctx.ensureValid();
    final nativeKind = kind.toInt();
    final nativeSchema = jsonEncode(schema).toNativeUtf8();
    final nativeOwner = owner != null
        ? jsonEncode(owner).toNativeUtf8()
        : nullptr;
    final recvPort = ReceivePort('routing_context_create_dht_record');
    final sendPort = recvPort.sendPort;
    VeilidFFI._routingContextCreateDHTRecord(
      sendPort.nativePort,
      _ctx.id!,
      nativeKind,
      nativeSchema,
      nativeOwner,
    );
    malloc.free(nativeSchema);
    if (nativeOwner != nullptr) {
      malloc.free(nativeOwner);
    }
    final dhtRecordDescriptor = await processFutureJson(
      DHTRecordDescriptor.fromJson,
      recvPort.first,
    );
    return dhtRecordDescriptor;
  }

  @override
  Future<DHTRecordDescriptor> openDHTRecord(
    RecordKey key, {
    KeyPair? writer,
    OpenDHTRecordOptions? options,
  }) async {
    _ctx.ensureValid();
    final nativeKey = jsonEncode(key).toNativeUtf8();
    final nativeWriter = writer != null
        ? jsonEncode(writer).toNativeUtf8()
        : nullptr;
    final nativeOptions = options != null
        ? jsonEncode(options).toNativeUtf8()
        : nullptr;
    final recvPort = ReceivePort('routing_context_open_dht_record');
    final sendPort = recvPort.sendPort;
    VeilidFFI._routingContextOpenDHTRecord(
      sendPort.nativePort,
      _ctx.id!,
      nativeKey,
      nativeWriter,
      nativeOptions,
    );
    malloc.free(nativeKey);
    if (nativeWriter != nullptr) {
      malloc.free(nativeWriter);
    }
    if (nativeOptions != nullptr) {
      malloc.free(nativeOptions);
    }
    final dhtRecordDescriptor = await processFutureJson(
      DHTRecordDescriptor.fromJson,
      recvPort.first,
    );
    return dhtRecordDescriptor;
  }

  @override
  Future<void> closeDHTRecord(RecordKey key) {
    _ctx.ensureValid();
    final nativeKey = jsonEncode(key).toNativeUtf8();
    final recvPort = ReceivePort('routing_context_close_dht_record');
    final sendPort = recvPort.sendPort;
    VeilidFFI._routingContextCloseDHTRecord(
      sendPort.nativePort,
      _ctx.id!,
      nativeKey,
    );
    malloc.free(nativeKey);
    return processFutureVoid(recvPort.first);
  }

  @override
  Future<void> deleteDHTRecord(RecordKey key) {
    _ctx.ensureValid();
    final nativeKey = jsonEncode(key).toNativeUtf8();
    final recvPort = ReceivePort('routing_context_delete_dht_record');
    final sendPort = recvPort.sendPort;
    VeilidFFI._routingContextDeleteDHTRecord(
      sendPort.nativePort,
      _ctx.id!,
      nativeKey,
    );
    malloc.free(nativeKey);
    return processFutureVoid(recvPort.first);
  }

  @override
  Future<ValueData?> getDHTValue(
    RecordKey key,
    int subkey, {
    bool forceRefresh = false,
  }) async {
    _ctx.ensureValid();
    final nativeKey = jsonEncode(key).toNativeUtf8();
    final recvPort = ReceivePort('routing_context_get_dht_value');
    final sendPort = recvPort.sendPort;
    VeilidFFI._routingContextGetDHTValue(
      sendPort.nativePort,
      _ctx.id!,
      nativeKey,
      subkey,
      forceRefresh,
    );
    malloc.free(nativeKey);
    final valueData = await processFutureOptJson(
      ValueData.fromJson,
      recvPort.first,
    );
    return valueData;
  }

  @override
  Future<ValueData?> setDHTValue(
    RecordKey key,
    int subkey,
    Uint8List data, {
    SetDHTValueOptions? options,
  }) async {
    _ctx.ensureValid();
    final nativeKey = jsonEncode(key).toNativeUtf8();
    final nativeData = base64UrlNoPadEncode(data).toNativeUtf8();
    final nativeOptions = options != null
        ? jsonEncode(options).toNativeUtf8()
        : nullptr;

    final recvPort = ReceivePort('routing_context_set_dht_value');
    final sendPort = recvPort.sendPort;
    VeilidFFI._routingContextSetDHTValue(
      sendPort.nativePort,
      _ctx.id!,
      nativeKey,
      subkey,
      nativeData,
      nativeOptions,
    );
    malloc.free(nativeKey);
    malloc.free(nativeData);
    if (nativeOptions != nullptr) {
      malloc.free(nativeOptions);
    }
    final valueData = await processFutureOptJson(
      ValueData.fromJson,
      recvPort.first,
    );
    return valueData;
  }

  @override
  Future<bool> watchDHTValues(
    RecordKey key, {
    List<ValueSubkeyRange>? subkeys,
    Timestamp? expiration,
    int? count,
  }) async {
    subkeys ??= [];
    expiration ??= Timestamp.zero();
    count ??= 0xFFFFFFFF;

    _ctx.ensureValid();
    final nativeKey = jsonEncode(key).toNativeUtf8();
    final nativeSubkeys = jsonEncode(subkeys).toNativeUtf8();
    final nativeExpiration = expiration.value.toInt();

    final recvPort = ReceivePort('routing_context_watch_dht_values');
    final sendPort = recvPort.sendPort;
    VeilidFFI._routingContextWatchDHTValues(
      sendPort.nativePort,
      _ctx.id!,
      nativeKey,
      nativeSubkeys,
      nativeExpiration,
      count,
    );
    malloc.free(nativeKey);
    malloc.free(nativeSubkeys);
    final active = await processFuturePlain<bool>(recvPort.first);
    return active;
  }

  @override
  Future<bool> cancelDHTWatch(
    RecordKey key, {
    List<ValueSubkeyRange>? subkeys,
  }) async {
    subkeys ??= [];

    _ctx.ensureValid();
    final nativeKey = jsonEncode(key).toNativeUtf8();
    final nativeSubkeys = jsonEncode(subkeys).toNativeUtf8();

    final recvPort = ReceivePort('routing_context_cancel_dht_watch');
    final sendPort = recvPort.sendPort;
    VeilidFFI._routingContextCancelDHTWatch(
      sendPort.nativePort,
      _ctx.id!,
      nativeKey,
      nativeSubkeys,
    );
    malloc.free(nativeKey);
    malloc.free(nativeSubkeys);
    final active = await processFuturePlain<bool>(recvPort.first);
    return active;
  }

  @override
  Future<DHTRecordReport> inspectDHTRecord(
    RecordKey key, {
    List<ValueSubkeyRange>? subkeys,
    DHTReportScope scope = DHTReportScope.local,
  }) async {
    _ctx.ensureValid();
    final nativeKey = jsonEncode(key).toNativeUtf8();
    final nativeSubkeys = subkeys != null
        ? jsonEncode(subkeys).toNativeUtf8()
        : nullptr;
    final nativeScope = jsonEncode(scope).toNativeUtf8();

    final recvPort = ReceivePort('routing_context_inspect_dht_record');
    final sendPort = recvPort.sendPort;
    VeilidFFI._routingContextInspectDHTRecord(
      sendPort.nativePort,
      _ctx.id!,
      nativeKey,
      nativeSubkeys,
      nativeScope,
    );
    malloc.free(nativeKey);
    if (nativeSubkeys != nullptr) {
      malloc.free(nativeSubkeys);
    }
    malloc.free(nativeScope);
    final report = await processFutureJson(
      DHTRecordReport.fromJson,
      recvPort.first,
    );
    return report;
  }

  @override
  Future<bool> flushDHTRecord(RecordKey key, {Duration? timeout}) async {
    _ctx.ensureValid();
    final nativeKey = jsonEncode(key).toNativeUtf8();

    final recvPort = ReceivePort('routing_context_flush_dht_record');
    final sendPort = recvPort.sendPort;
    VeilidFFI._routingContextFlushDHTRecord(
      sendPort.nativePort,
      _ctx.id!,
      nativeKey,
      timeout?.inMilliseconds ?? 0,
    );
    malloc.free(nativeKey);
    return processFuturePlain<bool>(recvPort.first);
  }
}

class _TDBT {
  int? id;
  final VeilidTableDBFFI tdbffi;

  _TDBT(int this.id, this.tdbffi);

  void ensureValid() {
    if (id == null) {
      throw VeilidAPIExceptionNotInitialized();
    }
  }

  void close() {
    if (id != null) {
      VeilidFFI._releaseTableDbTransaction(id!);
      id = null;
    }
  }
}

// FFI implementation of VeilidDHTTransaction
class VeilidDHTTransactionFFI extends VeilidDHTTransaction {
  final _DTX _dtx;
  static final Finalizer<_DTX> _finalizer = Finalizer((dtx) => dtx.close());

  VeilidDHTTransactionFFI._(this._dtx) {
    _finalizer.attach(this, _dtx, detach: this);
  }

  @override
  bool get isDone => _dtx.id == null;

  @override
  Future<void> commit() async {
    final id = _dtx.requireId();
    final recvPort = ReceivePort('veilid_dht_transaction_commit');
    final sendPort = recvPort.sendPort;
    VeilidFFI._dhtTransactionCommit(sendPort.nativePort, id);
    await processFutureVoid(recvPort.first);
    _dtx.close();
  }

  @override
  Future<void> rollback() async {
    final id = _dtx.requireId();
    final recvPort = ReceivePort('veilid_dht_transaction_rollback');
    final sendPort = recvPort.sendPort;
    VeilidFFI._dhtTransactionRollback(sendPort.nativePort, id);
    await processFutureVoid(recvPort.first);
    _dtx.close();
  }

  @override
  Future<void> extend(
    List<RecordKey> recordKeys, {
    TransactDHTRecordsOptions? options,
  }) async {
    final id = _dtx.requireId();
    final nativeRecordKeys = jsonEncode(recordKeys).toNativeUtf8();
    final nativeOptions = options != null
        ? jsonEncode(options).toNativeUtf8()
        : nullptr;

    final recvPort = ReceivePort('veilid_dht_transaction_extend');
    final sendPort = recvPort.sendPort;
    VeilidFFI._dhtTransactionExtend(
      sendPort.nativePort,
      id,
      nativeRecordKeys,
      nativeOptions,
    );
    malloc.free(nativeRecordKeys);
    if (nativeOptions != nullptr) {
      malloc.free(nativeOptions);
    }
    await processFutureVoid(recvPort.first);
  }

  @override
  Future<ValueData?> get(RecordKey key, int subkey) async {
    final id = _dtx.requireId();
    final nativeKey = jsonEncode(key).toNativeUtf8();
    final recvPort = ReceivePort('veilid_dht_transaction_get');
    final sendPort = recvPort.sendPort;
    VeilidFFI._dhtTransactionGet(sendPort.nativePort, id, nativeKey, subkey);
    malloc.free(nativeKey);
    final valueData = await processFutureOptJson(
      ValueData.fromJson,
      recvPort.first,
    );
    return valueData;
  }

  @override
  Future<ValueData?> set(
    RecordKey key,
    int subkey,
    Uint8List data, {
    DHTTransactionSetValueOptions? options,
  }) async {
    final id = _dtx.requireId();
    final nativeKey = jsonEncode(key).toNativeUtf8();
    final nativeData = base64UrlNoPadEncode(data).toNativeUtf8();
    final nativeOptions = options != null
        ? jsonEncode(options).toNativeUtf8()
        : nullptr;

    final recvPort = ReceivePort('veilid_dht_transaction_set');
    final sendPort = recvPort.sendPort;
    VeilidFFI._dhtTransactionSet(
      sendPort.nativePort,
      id,
      nativeKey,
      subkey,
      nativeData,
      nativeOptions,
    );
    malloc.free(nativeKey);
    malloc.free(nativeData);
    if (nativeOptions != nullptr) {
      malloc.free(nativeOptions);
    }
    final valueData = await processFutureOptJson(
      ValueData.fromJson,
      recvPort.first,
    );
    return valueData;
  }

  @override
  Future<DHTRecordReport> inspect(
    RecordKey key, {
    List<ValueSubkeyRange>? subkeys,
    DHTReportScope scope = DHTReportScope.local,
  }) async {
    final id = _dtx.requireId();
    final nativeKey = jsonEncode(key).toNativeUtf8();
    final nativeSubkeys = (subkeys != null)
        ? jsonEncode(subkeys).toNativeUtf8()
        : nullptr;
    final nativeScope = jsonEncode(scope).toNativeUtf8();

    final recvPort = ReceivePort('veilid_dht_transaction_inspect');
    final sendPort = recvPort.sendPort;
    VeilidFFI._dhtTransactionInspect(
      sendPort.nativePort,
      id,
      nativeKey,
      nativeSubkeys,
      nativeScope,
    );
    malloc.free(nativeKey);
    if (nativeSubkeys != nullptr) {
      malloc.free(nativeSubkeys);
    }
    malloc.free(nativeScope);
    final report = await processFutureJson(
      DHTRecordReport.fromJson,
      recvPort.first,
    );
    return report;
  }
}

class _DTX {
  int? id;

  _DTX(int this.id);

  int requireId() {
    if (id == null) {
      throw VeilidAPIExceptionNotInitialized();
    }
    return id!;
  }

  void close() {
    if (id != null) {
      VeilidFFI._releaseDHTTransaction(id!);
      id = null;
    }
  }
}

// FFI implementation of VeilidTableDBTransaction
class VeilidTableDBTransactionFFI extends VeilidTableDBTransaction {
  final _TDBT _tdbt;
  static final Finalizer<_TDBT> _finalizer = Finalizer((tdbt) => tdbt.close());

  VeilidTableDBTransactionFFI._(this._tdbt) {
    _finalizer.attach(this, _tdbt, detach: this);
  }

  @override
  bool get isDone => _tdbt.id == null;

  @override
  Future<void> commit() async {
    _tdbt.ensureValid();
    final recvPort = ReceivePort('veilid_table_db_transaction_commit');
    final sendPort = recvPort.sendPort;
    VeilidFFI._tableDbTransactionCommit(sendPort.nativePort, _tdbt.id!);
    await processFutureVoid(recvPort.first);
    _tdbt.close();
  }

  @override
  Future<void> rollback() async {
    _tdbt.ensureValid();
    final recvPort = ReceivePort('veilid_table_db_transaction_rollback');
    final sendPort = recvPort.sendPort;
    VeilidFFI._tableDbTransactionRollback(sendPort.nativePort, _tdbt.id!);
    await processFutureVoid(recvPort.first);
    _tdbt.close();
  }

  @override
  Future<void> store(int col, Uint8List key, Uint8List value) {
    _tdbt.ensureValid();
    final nativeEncodedKey = base64UrlNoPadEncode(key).toNativeUtf8();
    final nativeEncodedValue = base64UrlNoPadEncode(value).toNativeUtf8();

    final recvPort = ReceivePort('veilid_table_db_transaction_store');
    final sendPort = recvPort.sendPort;
    VeilidFFI._tableDbTransactionStore(
      sendPort.nativePort,
      _tdbt.id!,
      col,
      nativeEncodedKey,
      nativeEncodedValue,
    );
    malloc.free(nativeEncodedKey);
    malloc.free(nativeEncodedValue);
    return processFutureVoid(recvPort.first);
  }

  @override
  Future<void> delete(int col, Uint8List key) {
    _tdbt.ensureValid();
    final nativeEncodedKey = base64UrlNoPadEncode(key).toNativeUtf8();

    final recvPort = ReceivePort('veilid_table_db_transaction_delete');
    final sendPort = recvPort.sendPort;
    VeilidFFI._tableDbTransactionDelete(
      sendPort.nativePort,
      _tdbt.id!,
      col,
      nativeEncodedKey,
    );
    malloc.free(nativeEncodedKey);
    return processFuturePlain(recvPort.first);
  }
}

class _TDB {
  int? id;

  _TDB(int this.id);

  void ensureValid() {
    if (id == null) {
      throw VeilidAPIExceptionNotInitialized();
    }
  }

  void close() {
    if (id != null) {
      VeilidFFI._releaseTableDb(id!);
      id = null;
    }
  }
}

// FFI implementation of VeilidTableDB
class VeilidTableDBFFI extends VeilidTableDB {
  final _TDB _tdb;
  static final Finalizer<_TDB> _finalizer = Finalizer((tdb) => tdb.close());

  VeilidTableDBFFI._(this._tdb) {
    _finalizer.attach(this, _tdb, detach: this);
  }

  @override
  void close() {
    _tdb.close();
  }

  @override
  int get columnCount {
    _tdb.ensureValid();
    return VeilidFFI._tableDbGetColumnCount(_tdb.id!);
  }

  @override
  Future<List<Uint8List>> getKeys(int col) {
    _tdb.ensureValid();

    final recvPort = ReceivePort('veilid_table_db_get_keys');
    final sendPort = recvPort.sendPort;

    VeilidFFI._tableDbGetKeys(sendPort.nativePort, _tdb.id!, col);

    return processFutureJson(
      jsonListConstructor<Uint8List>(base64UrlNoPadDecodeDynamic),
      recvPort.first,
    );
  }

  @override
  VeilidTableDBTransaction transact() {
    _tdb.ensureValid();

    final id = VeilidFFI._tableDbTransact(_tdb.id!);
    return VeilidTableDBTransactionFFI._(_TDBT(id, this));
  }

  @override
  Future<void> store(int col, Uint8List key, Uint8List value) {
    _tdb.ensureValid();

    final nativeEncodedKey = base64UrlNoPadEncode(key).toNativeUtf8();
    final nativeEncodedValue = base64UrlNoPadEncode(value).toNativeUtf8();

    final recvPort = ReceivePort('veilid_table_db_store');
    final sendPort = recvPort.sendPort;
    VeilidFFI._tableDbStore(
      sendPort.nativePort,
      _tdb.id!,
      col,
      nativeEncodedKey,
      nativeEncodedValue,
    );
    malloc.free(nativeEncodedKey);
    malloc.free(nativeEncodedValue);
    return processFutureVoid(recvPort.first);
  }

  @override
  Future<Uint8List?> load(int col, Uint8List key) async {
    _tdb.ensureValid();
    final nativeEncodedKey = base64UrlNoPadEncode(key).toNativeUtf8();

    final recvPort = ReceivePort('veilid_table_db_load');
    final sendPort = recvPort.sendPort;
    VeilidFFI._tableDbLoad(
      sendPort.nativePort,
      _tdb.id!,
      col,
      nativeEncodedKey,
    );
    malloc.free(nativeEncodedKey);
    final out = await processFuturePlain<String?>(recvPort.first);
    if (out == null) {
      return null;
    }
    return base64UrlNoPadDecode(out);
  }

  @override
  Future<Uint8List?> delete(int col, Uint8List key) async {
    _tdb.ensureValid();
    final nativeEncodedKey = base64UrlNoPadEncode(key).toNativeUtf8();

    final recvPort = ReceivePort('veilid_table_db_delete');
    final sendPort = recvPort.sendPort;
    VeilidFFI._tableDbDelete(
      sendPort.nativePort,
      _tdb.id!,
      col,
      nativeEncodedKey,
    );
    malloc.free(nativeEncodedKey);
    final out = await processFuturePlain<String?>(recvPort.first);
    if (out == null) {
      return null;
    }
    return base64UrlNoPadDecode(out);
  }
}

// FFI implementation of VeilidCryptoSystem
class VeilidCryptoSystemFFI extends VeilidCryptoSystem {
  final int _kind;

  VeilidCryptoSystemFFI._(this._kind);

  @override
  CryptoKind kind() => CryptoKind.fromInt(_kind);

  @override
  Future<SharedSecret> cachedDH(PublicKey key, SecretKey secret) {
    final nativeKey = jsonEncode(key).toNativeUtf8();
    final nativeSecret = jsonEncode(secret).toNativeUtf8();

    final recvPort = ReceivePort('crypto_cached_dh');
    final sendPort = recvPort.sendPort;
    VeilidFFI._cryptoCachedDH(
      sendPort.nativePort,
      _kind,
      nativeKey,
      nativeSecret,
    );
    malloc.free(nativeKey);
    malloc.free(nativeSecret);
    return processFutureJson(SharedSecret.fromJson, recvPort.first);
  }

  @override
  Future<SharedSecret> computeDH(PublicKey key, SecretKey secret) {
    final nativeKey = jsonEncode(key).toNativeUtf8();
    final nativeSecret = jsonEncode(secret).toNativeUtf8();

    final recvPort = ReceivePort('crypto_compute_dh');
    final sendPort = recvPort.sendPort;
    VeilidFFI._cryptoComputeDH(
      sendPort.nativePort,
      _kind,
      nativeKey,
      nativeSecret,
    );
    malloc.free(nativeKey);
    malloc.free(nativeSecret);
    return processFutureJson(SharedSecret.fromJson, recvPort.first);
  }

  @override
  Future<SharedSecret> generateSharedSecret(
    PublicKey key,
    SecretKey secret,
    Uint8List domain,
  ) {
    final nativeKey = jsonEncode(key).toNativeUtf8();
    final nativeSecret = jsonEncode(secret).toNativeUtf8();
    final nativeDomain = base64UrlNoPadEncode(domain).toNativeUtf8();

    final recvPort = ReceivePort('crypto_generate_shared_secret');
    final sendPort = recvPort.sendPort;
    VeilidFFI._cryptoGenerateSharedSecret(
      sendPort.nativePort,
      _kind,
      nativeKey,
      nativeSecret,
      nativeDomain,
    );
    malloc.free(nativeKey);
    malloc.free(nativeSecret);
    malloc.free(nativeDomain);
    return processFutureJson(SharedSecret.fromJson, recvPort.first);
  }

  @override
  Future<Uint8List> hpkeSeal(
    EncapsulationKey recipient,
    Uint8List aad,
    Uint8List plaintext,
  ) async {
    final nativeRecipient = jsonEncode(recipient).toNativeUtf8();
    final nativeAad = base64UrlNoPadEncode(aad).toNativeUtf8();
    final nativePlaintext = base64UrlNoPadEncode(plaintext).toNativeUtf8();

    final recvPort = ReceivePort('crypto_hpke_seal');
    final sendPort = recvPort.sendPort;
    VeilidFFI._cryptoHpkeSeal(
      sendPort.nativePort,
      _kind,
      nativeRecipient,
      nativeAad,
      nativePlaintext,
    );
    malloc.free(nativeRecipient);
    malloc.free(nativeAad);
    malloc.free(nativePlaintext);
    final out = await processFuturePlain<String>(recvPort.first);
    return base64UrlNoPadDecode(out);
  }

  @override
  Future<Uint8List> hpkeOpen(
    DecapsulationKey secret,
    Uint8List aad,
    Uint8List sealed,
  ) async {
    final nativeSecret = jsonEncode(secret).toNativeUtf8();
    final nativeAad = base64UrlNoPadEncode(aad).toNativeUtf8();
    final nativeSealed = base64UrlNoPadEncode(sealed).toNativeUtf8();

    final recvPort = ReceivePort('crypto_hpke_open');
    final sendPort = recvPort.sendPort;
    VeilidFFI._cryptoHpkeOpen(
      sendPort.nativePort,
      _kind,
      nativeSecret,
      nativeAad,
      nativeSealed,
    );
    malloc.free(nativeSecret);
    malloc.free(nativeAad);
    malloc.free(nativeSealed);
    final out = await processFuturePlain<String>(recvPort.first);
    return base64UrlNoPadDecode(out);
  }

  @override
  Future<Uint8List> randomBytes(int len) async {
    final recvPort = ReceivePort('crypto_random_bytes');
    final sendPort = recvPort.sendPort;
    VeilidFFI._cryptoRandomBytes(sendPort.nativePort, _kind, len);
    final out = await processFuturePlain<String>(recvPort.first);
    return base64UrlNoPadDecode(out);
  }

  @override
  Future<int> sharedSecretLength() {
    final recvPort = ReceivePort('crypto_shared_secret_length');
    final sendPort = recvPort.sendPort;
    VeilidFFI._cryptoSharedSecretLength(sendPort.nativePort, _kind);
    return processFuturePlain(recvPort.first);
  }

  @override
  Future<int> nonceLength() {
    final recvPort = ReceivePort('crypto_nonce_length');
    final sendPort = recvPort.sendPort;
    VeilidFFI._cryptoNonceLength(sendPort.nativePort, _kind);
    return processFuturePlain(recvPort.first);
  }

  @override
  Future<int> hashDigestLength() {
    final recvPort = ReceivePort('crypto_hash_digest_length');
    final sendPort = recvPort.sendPort;
    VeilidFFI._cryptoHashDigestLength(sendPort.nativePort, _kind);
    return processFuturePlain(recvPort.first);
  }

  @override
  Future<int> publicKeyLength() {
    final recvPort = ReceivePort('crypto_public_key_length');
    final sendPort = recvPort.sendPort;
    VeilidFFI._cryptoPublicKeyLength(sendPort.nativePort, _kind);
    return processFuturePlain(recvPort.first);
  }

  @override
  Future<int> secretKeyLength() {
    final recvPort = ReceivePort('crypto_secret_key_length');
    final sendPort = recvPort.sendPort;
    VeilidFFI._cryptoSecretKeyLength(sendPort.nativePort, _kind);
    return processFuturePlain(recvPort.first);
  }

  @override
  Future<int> encapsulationKeyLength() {
    final recvPort = ReceivePort('crypto_encapsulation_key_length');
    final sendPort = recvPort.sendPort;
    VeilidFFI._cryptoEncapsulationKeyLength(sendPort.nativePort, _kind);
    return processFuturePlain(recvPort.first);
  }

  @override
  Future<int> decapsulationKeyLength() {
    final recvPort = ReceivePort('crypto_decapsulation_key_length');
    final sendPort = recvPort.sendPort;
    VeilidFFI._cryptoDecapsulationKeyLength(sendPort.nativePort, _kind);
    return processFuturePlain(recvPort.first);
  }

  @override
  Future<int> signatureLength() {
    final recvPort = ReceivePort('crypto_signature_length');
    final sendPort = recvPort.sendPort;
    VeilidFFI._cryptoSignatureLength(sendPort.nativePort, _kind);
    return processFuturePlain(recvPort.first);
  }

  @override
  Future<int> defaultSaltLength() {
    final recvPort = ReceivePort('crypto_default_salt_length');
    final sendPort = recvPort.sendPort;
    VeilidFFI._cryptoDefaultSaltLength(sendPort.nativePort, _kind);
    return processFuturePlain(recvPort.first);
  }

  @override
  Future<int> aeadOverhead() {
    final recvPort = ReceivePort('crypto_aead_overhead');
    final sendPort = recvPort.sendPort;
    VeilidFFI._cryptoAeadOverhead(sendPort.nativePort, _kind);
    return processFuturePlain(recvPort.first);
  }

  @override
  Future<void> checkSharedSecret(SharedSecret secret) {
    final nativeSecret = jsonEncode(secret).toNativeUtf8();
    final recvPort = ReceivePort('crypto_check_shared_secret');
    final sendPort = recvPort.sendPort;
    VeilidFFI._cryptoCheckSharedSecret(
      sendPort.nativePort,
      _kind,
      nativeSecret,
    );
    malloc.free(nativeSecret);
    return processFutureVoid(recvPort.first);
  }

  @override
  Future<void> checkNonce(Nonce nonce) {
    final nativeNonce = jsonEncode(nonce).toNativeUtf8();
    final recvPort = ReceivePort('crypto_check_nonce');
    final sendPort = recvPort.sendPort;
    VeilidFFI._cryptoCheckNonce(sendPort.nativePort, _kind, nativeNonce);
    malloc.free(nativeNonce);
    return processFutureVoid(recvPort.first);
  }

  @override
  Future<void> checkHashDigest(HashDigest digest) {
    final nativeDigest = jsonEncode(digest).toNativeUtf8();
    final recvPort = ReceivePort('crypto_check_hash_digest');
    final sendPort = recvPort.sendPort;
    VeilidFFI._cryptoCheckHashDigest(sendPort.nativePort, _kind, nativeDigest);
    malloc.free(nativeDigest);
    return processFutureVoid(recvPort.first);
  }

  @override
  Future<void> checkPublicKey(PublicKey key) {
    final nativeKey = jsonEncode(key).toNativeUtf8();
    final recvPort = ReceivePort('crypto_check_public_key');
    final sendPort = recvPort.sendPort;
    VeilidFFI._cryptoCheckPublicKey(sendPort.nativePort, _kind, nativeKey);
    malloc.free(nativeKey);
    return processFutureVoid(recvPort.first);
  }

  @override
  Future<void> checkSecretKey(SecretKey key) {
    final nativeKey = jsonEncode(key).toNativeUtf8();
    final recvPort = ReceivePort('crypto_check_secret_key');
    final sendPort = recvPort.sendPort;
    VeilidFFI._cryptoCheckSecretKey(sendPort.nativePort, _kind, nativeKey);
    malloc.free(nativeKey);
    return processFutureVoid(recvPort.first);
  }

  @override
  Future<void> checkSignature(Signature signature) {
    final nativeSignature = jsonEncode(signature).toNativeUtf8();
    final recvPort = ReceivePort('crypto_check_signature');
    final sendPort = recvPort.sendPort;
    VeilidFFI._cryptoCheckSignature(
      sendPort.nativePort,
      _kind,
      nativeSignature,
    );
    malloc.free(nativeSignature);
    return processFutureVoid(recvPort.first);
  }

  @override
  Future<String> hashPassword(Uint8List password, Uint8List salt) {
    final nativeEncodedPassword = base64UrlNoPadEncode(password).toNativeUtf8();
    final nativeEncodedSalt = base64UrlNoPadEncode(salt).toNativeUtf8();

    final recvPort = ReceivePort('crypto_hash_password');
    final sendPort = recvPort.sendPort;
    VeilidFFI._cryptoHashPassword(
      sendPort.nativePort,
      _kind,
      nativeEncodedPassword,
      nativeEncodedSalt,
    );
    malloc.free(nativeEncodedPassword);
    malloc.free(nativeEncodedSalt);
    return processFuturePlain(recvPort.first);
  }

  @override
  Future<bool> verifyPassword(Uint8List password, String passwordHash) {
    final nativeEncodedPassword = base64UrlNoPadEncode(password).toNativeUtf8();
    final nativeEncodedPasswordHash = passwordHash.toNativeUtf8();

    final recvPort = ReceivePort('crypto_verify_password');
    final sendPort = recvPort.sendPort;
    VeilidFFI._cryptoVerifyPassword(
      sendPort.nativePort,
      _kind,
      nativeEncodedPassword,
      nativeEncodedPasswordHash,
    );
    malloc.free(nativeEncodedPassword);
    malloc.free(nativeEncodedPasswordHash);
    return processFuturePlain(recvPort.first);
  }

  @override
  Future<SharedSecret> deriveSharedSecret(Uint8List password, Uint8List salt) {
    final nativeEncodedPassword = base64UrlNoPadEncode(password).toNativeUtf8();
    final nativeEncodedSalt = base64UrlNoPadEncode(salt).toNativeUtf8();

    final recvPort = ReceivePort('crypto_derive_shared_secret');
    final sendPort = recvPort.sendPort;
    VeilidFFI._cryptoDeriveSharedSecret(
      sendPort.nativePort,
      _kind,
      nativeEncodedPassword,
      nativeEncodedSalt,
    );
    malloc.free(nativeEncodedPassword);
    malloc.free(nativeEncodedSalt);
    return processFutureJson(SharedSecret.fromJson, recvPort.first);
  }

  @override
  Future<Nonce> randomNonce() {
    final recvPort = ReceivePort('crypto_random_nonce');
    final sendPort = recvPort.sendPort;
    VeilidFFI._cryptoRandomNonce(sendPort.nativePort, _kind);
    return processFutureJson(Nonce.fromJson, recvPort.first);
  }

  @override
  Future<SharedSecret> randomSharedSecret() {
    final recvPort = ReceivePort('crypto_random_shared_secret');
    final sendPort = recvPort.sendPort;
    VeilidFFI._cryptoRandomSharedSecret(sendPort.nativePort, _kind);
    return processFutureJson(SharedSecret.fromJson, recvPort.first);
  }

  @override
  Future<KeyPair> generateKeyPair() {
    final recvPort = ReceivePort('crypto_generate_key_pair');
    final sendPort = recvPort.sendPort;
    VeilidFFI._cryptoGenerateKeyPair(sendPort.nativePort, _kind);
    return processFutureJson(KeyPair.fromJson, recvPort.first);
  }

  @override
  Future<KemKeyPair> generateKemKeyPair() {
    final recvPort = ReceivePort('crypto_generate_kem_key_pair');
    final sendPort = recvPort.sendPort;
    VeilidFFI._cryptoGenerateKemKeyPair(sendPort.nativePort, _kind);
    return processFutureJson(KemKeyPair.fromJson, recvPort.first);
  }

  @override
  Future<EncapsulationKey> encapsulationKeyFromSigningKey(PublicKey key) {
    final nativeKey = jsonEncode(key).toNativeUtf8();

    final recvPort = ReceivePort('crypto_encapsulation_key_from_signing_key');
    final sendPort = recvPort.sendPort;
    VeilidFFI._cryptoEncapsulationKeyFromSigningKey(
      sendPort.nativePort,
      _kind,
      nativeKey,
    );
    malloc.free(nativeKey);
    return processFutureJson(EncapsulationKey.fromJson, recvPort.first);
  }

  @override
  Future<DecapsulationKey> decapsulationKeyFromSigningSecret(SecretKey secret) {
    final nativeSecret = jsonEncode(secret).toNativeUtf8();

    final recvPort = ReceivePort(
      'crypto_decapsulation_key_from_signing_secret',
    );
    final sendPort = recvPort.sendPort;
    VeilidFFI._cryptoDecapsulationKeyFromSigningSecret(
      sendPort.nativePort,
      _kind,
      nativeSecret,
    );
    malloc.free(nativeSecret);
    return processFutureJson(DecapsulationKey.fromJson, recvPort.first);
  }

  @override
  Future<HashDigest> generateHash(Uint8List data) {
    final nativeEncodedData = base64UrlNoPadEncode(data).toNativeUtf8();

    final recvPort = ReceivePort('crypto_generate_hash');
    final sendPort = recvPort.sendPort;
    VeilidFFI._cryptoGenerateHash(
      sendPort.nativePort,
      _kind,
      nativeEncodedData,
    );
    malloc.free(nativeEncodedData);
    return processFutureJson(HashDigest.fromJson, recvPort.first);
  }

  @override
  Future<bool> validateKeyPair(PublicKey key, SecretKey secret) {
    final nativeKey = jsonEncode(key).toNativeUtf8();
    final nativeSecret = jsonEncode(secret).toNativeUtf8();

    final recvPort = ReceivePort('crypto_validate_key_pair');
    final sendPort = recvPort.sendPort;
    VeilidFFI._cryptoValidateKeyPair(
      sendPort.nativePort,
      _kind,
      nativeKey,
      nativeSecret,
    );
    malloc.free(nativeKey);
    malloc.free(nativeSecret);
    return processFuturePlain(recvPort.first);
  }

  @override
  Future<bool> validateHash(Uint8List data, HashDigest hash) {
    final nativeEncodedData = base64UrlNoPadEncode(data).toNativeUtf8();
    final nativeHash = jsonEncode(hash).toNativeUtf8();

    final recvPort = ReceivePort('crypto_validate_hash');
    final sendPort = recvPort.sendPort;
    VeilidFFI._cryptoValidateHash(
      sendPort.nativePort,
      _kind,
      nativeEncodedData,
      nativeHash,
    );
    malloc.free(nativeEncodedData);
    malloc.free(nativeHash);
    return processFuturePlain(recvPort.first);
  }

  @override
  Future<Signature> sign(PublicKey key, SecretKey secret, Uint8List data) {
    final nativeKey = jsonEncode(key).toNativeUtf8();
    final nativeSecret = jsonEncode(secret).toNativeUtf8();
    final nativeEncodedData = base64UrlNoPadEncode(data).toNativeUtf8();

    final recvPort = ReceivePort('crypto_sign');
    final sendPort = recvPort.sendPort;
    VeilidFFI._cryptoSign(
      sendPort.nativePort,
      _kind,
      nativeKey,
      nativeSecret,
      nativeEncodedData,
    );
    malloc.free(nativeKey);
    malloc.free(nativeSecret);
    malloc.free(nativeEncodedData);
    return processFutureJson(Signature.fromJson, recvPort.first);
  }

  @override
  Future<bool> verify(PublicKey key, Uint8List data, Signature signature) {
    final nativeKey = jsonEncode(key).toNativeUtf8();
    final nativeEncodedData = base64UrlNoPadEncode(data).toNativeUtf8();
    final nativeSignature = jsonEncode(signature).toNativeUtf8();

    final recvPort = ReceivePort('crypto_verify');
    final sendPort = recvPort.sendPort;
    VeilidFFI._cryptoVerify(
      sendPort.nativePort,
      _kind,
      nativeKey,
      nativeEncodedData,
      nativeSignature,
    );
    malloc.free(nativeKey);
    malloc.free(nativeEncodedData);
    malloc.free(nativeSignature);
    return processFuturePlain(recvPort.first);
  }

  @override
  Future<Uint8List> decryptAead(
    Uint8List body,
    Nonce nonce,
    SharedSecret sharedSecret,
    Uint8List? associatedData,
  ) async {
    final nativeEncodedBody = base64UrlNoPadEncode(body).toNativeUtf8();
    final nativeNonce = jsonEncode(nonce).toNativeUtf8();
    final nativeSharedSecret = jsonEncode(sharedSecret).toNativeUtf8();
    final nativeSignature = (associatedData != null)
        ? jsonEncode(associatedData).toNativeUtf8()
        : nullptr;

    final recvPort = ReceivePort('crypto_decrypt_aead');
    final sendPort = recvPort.sendPort;
    VeilidFFI._cryptoDecryptAead(
      sendPort.nativePort,
      _kind,
      nativeEncodedBody,
      nativeNonce,
      nativeSharedSecret,
      nativeSignature,
    );
    malloc.free(nativeEncodedBody);
    malloc.free(nativeNonce);
    malloc.free(nativeSharedSecret);
    if (nativeSignature != nullptr) {
      malloc.free(nativeSignature);
    }
    final out = await processFuturePlain<String>(recvPort.first);
    return base64UrlNoPadDecode(out);
  }

  @override
  Future<Uint8List> encryptAead(
    Uint8List body,
    Nonce nonce,
    SharedSecret sharedSecret,
    Uint8List? associatedData,
  ) async {
    final nativeEncodedBody = base64UrlNoPadEncode(body).toNativeUtf8();
    final nativeNonce = jsonEncode(nonce).toNativeUtf8();
    final nativeSharedSecret = jsonEncode(sharedSecret).toNativeUtf8();
    final nativeSignature = (associatedData != null)
        ? jsonEncode(associatedData).toNativeUtf8()
        : nullptr;

    final recvPort = ReceivePort('crypto_encrypt_aead');
    final sendPort = recvPort.sendPort;
    VeilidFFI._cryptoEncryptAead(
      sendPort.nativePort,
      _kind,
      nativeEncodedBody,
      nativeNonce,
      nativeSharedSecret,
      nativeSignature,
    );
    malloc.free(nativeEncodedBody);
    malloc.free(nativeNonce);
    malloc.free(nativeSharedSecret);
    if (nativeSignature != nullptr) {
      malloc.free(nativeSignature);
    }
    final out = await processFuturePlain<String>(recvPort.first);
    return base64UrlNoPadDecode(out);
  }

  @override
  Future<Uint8List> cryptNoAuth(
    Uint8List body,
    Nonce nonce,
    SharedSecret sharedSecret,
  ) async {
    final nativeEncodedBody = base64UrlNoPadEncode(body).toNativeUtf8();
    final nativeNonce = jsonEncode(nonce).toNativeUtf8();
    final nativeSharedSecret = jsonEncode(sharedSecret).toNativeUtf8();

    final recvPort = ReceivePort('crypto_crypt_no_auth');
    final sendPort = recvPort.sendPort;
    VeilidFFI._cryptoCryptNoAuth(
      sendPort.nativePort,
      _kind,
      nativeEncodedBody,
      nativeNonce,
      nativeSharedSecret,
    );
    malloc.free(nativeEncodedBody);
    malloc.free(nativeNonce);
    malloc.free(nativeSharedSecret);
    final out = await processFuturePlain<String>(recvPort.first);
    return base64UrlNoPadDecode(out);
  }
}

// FFI implementation of high level Veilid API
class VeilidFFI extends Veilid {
  // One stream port per reference this isolate holds on the running node.
  // Shutdown releases the most recent one.
  final _updateStreamPorts = <int>[];

  // Shared library functions
  @Native<Void Function(Pointer<Utf8>)>(symbol: 'free_string')
  external static void _freeString(Pointer<Utf8> s);

  @Native<Void Function(Pointer<Utf8>)>(symbol: 'initialize_veilid_core')
  external static void _initializeVeilidCore(Pointer<Utf8> platformConfig);

  @Native<Int32 Function(Pointer<Utf8>, Pointer<Utf8>)>(
    symbol: 'change_log_level',
  )
  external static int _changeLogLevel(
    Pointer<Utf8> layer,
    Pointer<Utf8> directives,
  );

  @Native<Void Function(Pointer<Utf8>, Pointer<Utf8>)>(
    symbol: 'change_log_ignore',
  )
  external static void _changeLogIgnore(
    Pointer<Utf8> layer,
    Pointer<Utf8> logIgnore,
  );

  @Native<Void Function(Int64, Int64, Pointer<Utf8>)>(
    symbol: 'startup_veilid_core',
  )
  external static void _startupVeilidCore(
    int port,
    int streamPort,
    Pointer<Utf8> config,
  );

  @Native<Void Function(Int64)>(symbol: 'is_shutdown')
  external static void _isShutdown(int port);

  @Native<Void Function(Int64)>(symbol: 'get_veilid_state')
  external static void _getVeilidState(int port);

  @Native<Void Function(Int64)>(symbol: 'attach')
  external static void _attach(int port);

  @Native<Void Function(Int64)>(symbol: 'detach')
  external static void _detach(int port);

  @Native<Void Function(Int64, Pointer<Utf8>)>(symbol: 'change_config')
  external static void _changeConfig(int port, Pointer<Utf8> config);

  @Native<Void Function(Int64, Pointer<Utf8>)>(
    symbol: 'report_app_lifecycle_state',
  )
  external static void _reportAppLifecycleState(int port, Pointer<Utf8> state);

  @Native<Void Function(Int64, Pointer<Utf8>, Uint64)>(
    symbol: 'wait_for_attachment_condition',
  )
  external static void _waitForAttachmentCondition(
    int port,
    Pointer<Utf8> condition,
    int timeoutMs,
  );

  @Native<Void Function(Int64, Int64)>(symbol: 'shutdown_veilid_core')
  external static void _shutdownVeilidCore(int port, int streamPort);

  @Native<Void Function(Int64)>(symbol: 'routing_context')
  external static void _routingContext(int port);

  @Native<Int32 Function(Uint32)>(symbol: 'release_routing_context')
  external static int _releaseRoutingContext(int id);

  @Native<Uint32 Function(Uint32)>(
    symbol: 'routing_context_with_default_safety',
  )
  external static int _routingContextWithDefaultSafety(int id);

  @Native<Uint32 Function(Uint32, Pointer<Utf8>)>(
    symbol: 'routing_context_with_safety',
  )
  external static int _routingContextWithSafety(
    int id,
    Pointer<Utf8> safetySelection,
  );

  @Native<Uint32 Function(Uint32, Pointer<Utf8>)>(
    symbol: 'routing_context_with_sequencing',
  )
  external static int _routingContextWithSequencing(
    int id,
    Pointer<Utf8> sequencing,
  );

  @Native<Void Function(Int64, Uint32)>(symbol: 'routing_context_safety')
  external static void _routingContextSafety(int port, int id);

  @Native<Void Function(Int64, Uint32, Pointer<Utf8>, Pointer<Utf8>)>(
    symbol: 'routing_context_app_call',
  )
  external static void _routingContextAppCall(
    int port,
    int id,
    Pointer<Utf8> target,
    Pointer<Utf8> request,
  );

  @Native<Void Function(Int64, Uint32, Pointer<Utf8>, Pointer<Utf8>)>(
    symbol: 'routing_context_app_message',
  )
  external static void _routingContextAppMessage(
    int port,
    int id,
    Pointer<Utf8> target,
    Pointer<Utf8> message,
  );

  @Native<Void Function(Int64, Uint32, Uint32, Pointer<Utf8>, Pointer<Utf8>)>(
    symbol: 'routing_context_create_dht_record',
  )
  external static void _routingContextCreateDHTRecord(
    int port,
    int id,
    int kind,
    Pointer<Utf8> schema,
    Pointer<Utf8> owner,
  );

  @Native<
    Void Function(Int64, Uint32, Pointer<Utf8>, Pointer<Utf8>, Pointer<Utf8>)
  >(symbol: 'routing_context_open_dht_record')
  external static void _routingContextOpenDHTRecord(
    int port,
    int id,
    Pointer<Utf8> key,
    Pointer<Utf8> writer,
    Pointer<Utf8> options,
  );

  @Native<Void Function(Int64, Uint32, Pointer<Utf8>)>(
    symbol: 'routing_context_close_dht_record',
  )
  external static void _routingContextCloseDHTRecord(
    int port,
    int id,
    Pointer<Utf8> key,
  );

  @Native<Void Function(Int64, Uint32, Pointer<Utf8>)>(
    symbol: 'routing_context_delete_dht_record',
  )
  external static void _routingContextDeleteDHTRecord(
    int port,
    int id,
    Pointer<Utf8> key,
  );

  @Native<Void Function(Int64, Uint32, Pointer<Utf8>, Uint32, Bool)>(
    symbol: 'routing_context_get_dht_value',
  )
  external static void _routingContextGetDHTValue(
    int port,
    int id,
    Pointer<Utf8> key,
    int subkey,
    bool forceRefresh,
  );

  @Native<
    Void Function(
      Int64,
      Uint32,
      Pointer<Utf8>,
      Uint32,
      Pointer<Utf8>,
      Pointer<Utf8>,
    )
  >(symbol: 'routing_context_set_dht_value')
  external static void _routingContextSetDHTValue(
    int port,
    int id,
    Pointer<Utf8> key,
    int subkey,
    Pointer<Utf8> data,
    Pointer<Utf8> options,
  );

  @Native<
    Void Function(Int64, Uint32, Pointer<Utf8>, Pointer<Utf8>, Uint64, Uint32)
  >(symbol: 'routing_context_watch_dht_values')
  external static void _routingContextWatchDHTValues(
    int port,
    int id,
    Pointer<Utf8> key,
    Pointer<Utf8> subkeys,
    int expiration,
    int count,
  );

  @Native<Void Function(Int64, Uint32, Pointer<Utf8>, Pointer<Utf8>)>(
    symbol: 'routing_context_cancel_dht_watch',
  )
  external static void _routingContextCancelDHTWatch(
    int port,
    int id,
    Pointer<Utf8> key,
    Pointer<Utf8> subkeys,
  );

  @Native<
    Void Function(Int64, Uint32, Pointer<Utf8>, Pointer<Utf8>, Pointer<Utf8>)
  >(symbol: 'routing_context_inspect_dht_record')
  external static void _routingContextInspectDHTRecord(
    int port,
    int id,
    Pointer<Utf8> key,
    Pointer<Utf8> subkeys,
    Pointer<Utf8> scope,
  );

  @Native<Void Function(Int64, Uint32, Pointer<Utf8>, Uint64)>(
    symbol: 'routing_context_flush_dht_record',
  )
  external static void _routingContextFlushDHTRecord(
    int port,
    int id,
    Pointer<Utf8> key,
    int timeoutMs,
  );

  @Native<Void Function(Int64, Pointer<Utf8>)>(symbol: 'generate_member_id')
  external static void _generateMemberId(int port, Pointer<Utf8> writerKey);

  @Native<Void Function(Int64, Pointer<Utf8>, Pointer<Utf8>, Pointer<Utf8>)>(
    symbol: 'get_dht_record_key',
  )
  external static void _getDHTRecordKey(
    int port,
    Pointer<Utf8> schema,
    Pointer<Utf8> owner,
    Pointer<Utf8> encryptionKey,
  );

  @Native<Void Function(Int64, Pointer<Utf8>, Pointer<Utf8>)>(
    symbol: 'transact_dht_records',
  )
  external static void _transactDHTRecords(
    int port,
    Pointer<Utf8> recordKeys,
    Pointer<Utf8> options,
  );

  @Native<Void Function(Int64)>(symbol: 'new_private_route')
  external static void _newPrivateRoute(int port);

  @Native<Void Function(Int64, Pointer<Utf8>)>(
    symbol: 'new_custom_private_route',
  )
  external static void _newCustomPrivateRoute(
    int port,
    Pointer<Utf8> privateSpec,
  );

  @Native<Void Function(Int64, Pointer<Utf8>)>(
    symbol: 'import_remote_private_route',
  )
  external static void _importRemotePrivateRoute(int port, Pointer<Utf8> blob);

  @Native<Void Function(Int64, Pointer<Utf8>)>(symbol: 'release_private_route')
  external static void _releasePrivateRoute(int port, Pointer<Utf8> routeId);

  @Native<Void Function(Int64, Pointer<Utf8>, Pointer<Utf8>)>(
    symbol: 'app_call_reply',
  )
  external static void _appCallReply(
    int port,
    Pointer<Utf8> callId,
    Pointer<Utf8> message,
  );

  @Native<Int32 Function(Uint32)>(symbol: 'release_dht_transaction')
  external static int _releaseDHTTransaction(int id);

  @Native<Void Function(Int64, Uint32)>(symbol: 'dht_transaction_commit')
  external static void _dhtTransactionCommit(int port, int id);

  @Native<Void Function(Int64, Uint32)>(symbol: 'dht_transaction_rollback')
  external static void _dhtTransactionRollback(int port, int id);

  @Native<Void Function(Int64, Uint32, Pointer<Utf8>, Pointer<Utf8>)>(
    symbol: 'dht_transaction_extend',
  )
  external static void _dhtTransactionExtend(
    int port,
    int id,
    Pointer<Utf8> recordKeys,
    Pointer<Utf8> options,
  );

  @Native<Void Function(Int64, Uint32, Pointer<Utf8>, Uint32)>(
    symbol: 'dht_transaction_get',
  )
  external static void _dhtTransactionGet(
    int port,
    int id,
    Pointer<Utf8> key,
    int subkey,
  );

  @Native<
    Void Function(
      Int64,
      Uint32,
      Pointer<Utf8>,
      Uint32,
      Pointer<Utf8>,
      Pointer<Utf8>,
    )
  >(symbol: 'dht_transaction_set')
  external static void _dhtTransactionSet(
    int port,
    int id,
    Pointer<Utf8> key,
    int subkey,
    Pointer<Utf8> data,
    Pointer<Utf8> options,
  );

  @Native<
    Void Function(Int64, Uint32, Pointer<Utf8>, Pointer<Utf8>, Pointer<Utf8>)
  >(symbol: 'dht_transaction_inspect')
  external static void _dhtTransactionInspect(
    int port,
    int id,
    Pointer<Utf8> key,
    Pointer<Utf8> subkeys,
    Pointer<Utf8> scope,
  );

  @Native<Void Function(Int64, Pointer<Utf8>, Uint32)>(symbol: 'open_table_db')
  external static void _openTableDb(
    int port,
    Pointer<Utf8> name,
    int columnCount,
  );

  @Native<Int32 Function(Uint32)>(symbol: 'release_table_db')
  external static int _releaseTableDb(int id);

  @Native<Void Function(Int64, Pointer<Utf8>)>(symbol: 'delete_table_db')
  external static void _deleteTableDb(int port, Pointer<Utf8> name);

  @Native<Void Function(Int64, Pointer<Utf8>, Pointer<Utf8>)>(
    symbol: 'rename_table_db',
  )
  external static void _renameTableDb(
    int port,
    Pointer<Utf8> oldName,
    Pointer<Utf8> newName,
  );

  @Native<Uint32 Function(Uint32)>(symbol: 'table_db_get_column_count')
  external static int _tableDbGetColumnCount(int id);

  @Native<Void Function(Int64, Uint32, Uint32)>(symbol: 'table_db_get_keys')
  external static void _tableDbGetKeys(int port, int id, int col);

  @Native<Void Function(Int64, Uint32, Uint32, Pointer<Utf8>, Pointer<Utf8>)>(
    symbol: 'table_db_store',
  )
  external static void _tableDbStore(
    int port,
    int id,
    int col,
    Pointer<Utf8> key,
    Pointer<Utf8> value,
  );

  @Native<Void Function(Int64, Uint32, Uint32, Pointer<Utf8>)>(
    symbol: 'table_db_load',
  )
  external static void _tableDbLoad(
    int port,
    int id,
    int col,
    Pointer<Utf8> key,
  );

  @Native<Void Function(Int64, Uint32, Uint32, Pointer<Utf8>)>(
    symbol: 'table_db_delete',
  )
  external static void _tableDbDelete(
    int port,
    int id,
    int col,
    Pointer<Utf8> key,
  );

  @Native<Uint32 Function(Uint32)>(symbol: 'table_db_transact')
  external static int _tableDbTransact(int id);

  @Native<Int32 Function(Uint32)>(symbol: 'release_table_db_transaction')
  external static int _releaseTableDbTransaction(int id);

  @Native<Void Function(Int64, Uint32)>(symbol: 'table_db_transaction_commit')
  external static void _tableDbTransactionCommit(int port, int id);

  @Native<Void Function(Int64, Uint32)>(symbol: 'table_db_transaction_rollback')
  external static void _tableDbTransactionRollback(int port, int id);

  @Native<Void Function(Int64, Uint32, Uint32, Pointer<Utf8>, Pointer<Utf8>)>(
    symbol: 'table_db_transaction_store',
  )
  external static void _tableDbTransactionStore(
    int port,
    int id,
    int col,
    Pointer<Utf8> key,
    Pointer<Utf8> value,
  );

  @Native<Void Function(Int64, Uint32, Uint32, Pointer<Utf8>)>(
    symbol: 'table_db_transaction_delete',
  )
  external static void _tableDbTransactionDelete(
    int port,
    int id,
    int col,
    Pointer<Utf8> key,
  );

  @Native<Pointer<Utf8> Function()>(symbol: 'valid_crypto_kinds')
  external static Pointer<Utf8> _validCryptoKinds();

  @Native<Void Function(Int64, Pointer<Utf8>, Pointer<Utf8>, Pointer<Utf8>)>(
    symbol: 'verify_signatures',
  )
  external static void _verifySignatures(
    int port,
    Pointer<Utf8> nodeIds,
    Pointer<Utf8> data,
    Pointer<Utf8> signatures,
  );

  @Native<Void Function(Int64, Pointer<Utf8>, Pointer<Utf8>)>(
    symbol: 'generate_signatures',
  )
  external static void _generateSignatures(
    int port,
    Pointer<Utf8> data,
    Pointer<Utf8> keyPairs,
  );

  @Native<Void Function(Int64, Uint32)>(symbol: 'generate_key_pair')
  external static void _generateKeyPair(int port, int kind);

  @Native<Void Function(Int64, Uint32, Pointer<Utf8>, Pointer<Utf8>)>(
    symbol: 'crypto_cached_dh',
  )
  external static void _cryptoCachedDH(
    int port,
    int kind,
    Pointer<Utf8> key,
    Pointer<Utf8> secret,
  );

  @Native<Void Function(Int64, Uint32, Pointer<Utf8>, Pointer<Utf8>)>(
    symbol: 'crypto_compute_dh',
  )
  external static void _cryptoComputeDH(
    int port,
    int kind,
    Pointer<Utf8> key,
    Pointer<Utf8> secret,
  );

  @Native<
    Void Function(Int64, Uint32, Pointer<Utf8>, Pointer<Utf8>, Pointer<Utf8>)
  >(symbol: 'crypto_generate_shared_secret')
  external static void _cryptoGenerateSharedSecret(
    int port,
    int kind,
    Pointer<Utf8> key,
    Pointer<Utf8> secret,
    Pointer<Utf8> domain,
  );

  @Native<
    Void Function(Int64, Uint32, Pointer<Utf8>, Pointer<Utf8>, Pointer<Utf8>)
  >(symbol: 'crypto_hpke_seal')
  external static void _cryptoHpkeSeal(
    int port,
    int kind,
    Pointer<Utf8> recipient,
    Pointer<Utf8> aad,
    Pointer<Utf8> plaintext,
  );

  @Native<
    Void Function(Int64, Uint32, Pointer<Utf8>, Pointer<Utf8>, Pointer<Utf8>)
  >(symbol: 'crypto_hpke_open')
  external static void _cryptoHpkeOpen(
    int port,
    int kind,
    Pointer<Utf8> secret,
    Pointer<Utf8> aad,
    Pointer<Utf8> sealed,
  );

  @Native<Void Function(Int64, Uint32)>(symbol: 'crypto_shared_secret_length')
  external static void _cryptoSharedSecretLength(int port, int kind);

  @Native<Void Function(Int64, Uint32)>(symbol: 'crypto_nonce_length')
  external static void _cryptoNonceLength(int port, int kind);

  @Native<Void Function(Int64, Uint32)>(symbol: 'crypto_hash_digest_length')
  external static void _cryptoHashDigestLength(int port, int kind);

  @Native<Void Function(Int64, Uint32)>(symbol: 'crypto_public_key_length')
  external static void _cryptoPublicKeyLength(int port, int kind);

  @Native<Void Function(Int64, Uint32)>(symbol: 'crypto_secret_key_length')
  external static void _cryptoSecretKeyLength(int port, int kind);

  @Native<Void Function(Int64, Uint32)>(
    symbol: 'crypto_encapsulation_key_length',
  )
  external static void _cryptoEncapsulationKeyLength(int port, int kind);

  @Native<Void Function(Int64, Uint32)>(
    symbol: 'crypto_decapsulation_key_length',
  )
  external static void _cryptoDecapsulationKeyLength(int port, int kind);

  @Native<Void Function(Int64, Uint32)>(symbol: 'crypto_signature_length')
  external static void _cryptoSignatureLength(int port, int kind);

  @Native<Void Function(Int64, Uint32)>(symbol: 'crypto_default_salt_length')
  external static void _cryptoDefaultSaltLength(int port, int kind);

  @Native<Void Function(Int64, Uint32)>(symbol: 'crypto_aead_overhead')
  external static void _cryptoAeadOverhead(int port, int kind);

  @Native<Void Function(Int64, Uint32, Pointer<Utf8>)>(
    symbol: 'crypto_check_shared_secret',
  )
  external static void _cryptoCheckSharedSecret(
    int port,
    int kind,
    Pointer<Utf8> secret,
  );

  @Native<Void Function(Int64, Uint32, Pointer<Utf8>)>(
    symbol: 'crypto_check_nonce',
  )
  external static void _cryptoCheckNonce(
    int port,
    int kind,
    Pointer<Utf8> nonce,
  );

  @Native<Void Function(Int64, Uint32, Pointer<Utf8>)>(
    symbol: 'crypto_check_hash_digest',
  )
  external static void _cryptoCheckHashDigest(
    int port,
    int kind,
    Pointer<Utf8> digest,
  );

  @Native<Void Function(Int64, Uint32, Pointer<Utf8>)>(
    symbol: 'crypto_check_public_key',
  )
  external static void _cryptoCheckPublicKey(
    int port,
    int kind,
    Pointer<Utf8> key,
  );

  @Native<Void Function(Int64, Uint32, Pointer<Utf8>)>(
    symbol: 'crypto_check_secret_key',
  )
  external static void _cryptoCheckSecretKey(
    int port,
    int kind,
    Pointer<Utf8> key,
  );

  @Native<Void Function(Int64, Uint32, Pointer<Utf8>)>(
    symbol: 'crypto_check_signature',
  )
  external static void _cryptoCheckSignature(
    int port,
    int kind,
    Pointer<Utf8> signature,
  );

  @Native<Void Function(Int64, Uint32, Uint32)>(symbol: 'crypto_random_bytes')
  external static void _cryptoRandomBytes(int port, int kind, int len);

  @Native<Void Function(Int64, Uint32, Pointer<Utf8>, Pointer<Utf8>)>(
    symbol: 'crypto_hash_password',
  )
  external static void _cryptoHashPassword(
    int port,
    int kind,
    Pointer<Utf8> password,
    Pointer<Utf8> salt,
  );

  @Native<Void Function(Int64, Uint32, Pointer<Utf8>, Pointer<Utf8>)>(
    symbol: 'crypto_verify_password',
  )
  external static void _cryptoVerifyPassword(
    int port,
    int kind,
    Pointer<Utf8> password,
    Pointer<Utf8> passwordHash,
  );

  @Native<Void Function(Int64, Uint32, Pointer<Utf8>, Pointer<Utf8>)>(
    symbol: 'crypto_derive_shared_secret',
  )
  external static void _cryptoDeriveSharedSecret(
    int port,
    int kind,
    Pointer<Utf8> password,
    Pointer<Utf8> salt,
  );

  @Native<Void Function(Int64, Uint32)>(symbol: 'crypto_random_nonce')
  external static void _cryptoRandomNonce(int port, int kind);

  @Native<Void Function(Int64, Uint32)>(symbol: 'crypto_random_shared_secret')
  external static void _cryptoRandomSharedSecret(int port, int kind);

  @Native<Void Function(Int64, Uint32)>(symbol: 'crypto_generate_key_pair')
  external static void _cryptoGenerateKeyPair(int port, int kind);

  @Native<Void Function(Int64, Uint32)>(symbol: 'crypto_generate_kem_key_pair')
  external static void _cryptoGenerateKemKeyPair(int port, int kind);

  @Native<Void Function(Int64, Uint32, Pointer<Utf8>)>(
    symbol: 'crypto_encapsulation_key_from_signing_key',
  )
  external static void _cryptoEncapsulationKeyFromSigningKey(
    int port,
    int kind,
    Pointer<Utf8> key,
  );

  @Native<Void Function(Int64, Uint32, Pointer<Utf8>)>(
    symbol: 'crypto_decapsulation_key_from_signing_secret',
  )
  external static void _cryptoDecapsulationKeyFromSigningSecret(
    int port,
    int kind,
    Pointer<Utf8> secret,
  );

  @Native<Void Function(Int64, Uint32, Pointer<Utf8>)>(
    symbol: 'crypto_generate_hash',
  )
  external static void _cryptoGenerateHash(
    int port,
    int kind,
    Pointer<Utf8> data,
  );

  @Native<Void Function(Int64, Uint32, Pointer<Utf8>, Pointer<Utf8>)>(
    symbol: 'crypto_validate_key_pair',
  )
  external static void _cryptoValidateKeyPair(
    int port,
    int kind,
    Pointer<Utf8> key,
    Pointer<Utf8> secret,
  );

  @Native<Void Function(Int64, Uint32, Pointer<Utf8>, Pointer<Utf8>)>(
    symbol: 'crypto_validate_hash',
  )
  external static void _cryptoValidateHash(
    int port,
    int kind,
    Pointer<Utf8> data,
    Pointer<Utf8> hash,
  );

  @Native<
    Void Function(Int64, Uint32, Pointer<Utf8>, Pointer<Utf8>, Pointer<Utf8>)
  >(symbol: 'crypto_sign')
  external static void _cryptoSign(
    int port,
    int kind,
    Pointer<Utf8> key,
    Pointer<Utf8> secret,
    Pointer<Utf8> data,
  );

  @Native<
    Void Function(Int64, Uint32, Pointer<Utf8>, Pointer<Utf8>, Pointer<Utf8>)
  >(symbol: 'crypto_verify')
  external static void _cryptoVerify(
    int port,
    int kind,
    Pointer<Utf8> key,
    Pointer<Utf8> data,
    Pointer<Utf8> signature,
  );

  @Native<
    Void Function(
      Int64,
      Uint32,
      Pointer<Utf8>,
      Pointer<Utf8>,
      Pointer<Utf8>,
      Pointer<Utf8>,
    )
  >(symbol: 'crypto_decrypt_aead')
  external static void _cryptoDecryptAead(
    int port,
    int kind,
    Pointer<Utf8> body,
    Pointer<Utf8> nonce,
    Pointer<Utf8> sharedSecret,
    Pointer<Utf8> associatedData,
  );

  @Native<
    Void Function(
      Int64,
      Uint32,
      Pointer<Utf8>,
      Pointer<Utf8>,
      Pointer<Utf8>,
      Pointer<Utf8>,
    )
  >(symbol: 'crypto_encrypt_aead')
  external static void _cryptoEncryptAead(
    int port,
    int kind,
    Pointer<Utf8> body,
    Pointer<Utf8> nonce,
    Pointer<Utf8> sharedSecret,
    Pointer<Utf8> associatedData,
  );

  @Native<
    Void Function(Int64, Uint32, Pointer<Utf8>, Pointer<Utf8>, Pointer<Utf8>)
  >(symbol: 'crypto_crypt_no_auth')
  external static void _cryptoCryptNoAuth(
    int port,
    int kind,
    Pointer<Utf8> body,
    Pointer<Utf8> nonce,
    Pointer<Utf8> sharedSecret,
  );

  @Native<Uint64 Function()>(symbol: 'now')
  external static int _now();

  @Native<Uint64 Function()>(symbol: 'now_non_decreasing')
  external static int _nowNonDecreasing();

  @Native<Uint64 Function()>(symbol: 'now_increasing')
  external static int _nowIncreasing();

  @Native<Void Function(Int64, Pointer<Utf8>)>(symbol: 'debug')
  external static void _debug(int port, Pointer<Utf8> command);

  @Native<Void Function(Int64, Pointer<Utf8>, Pointer<Utf8>)>(
    symbol: 'save_user_secret',
  )
  external static void _saveUserSecret(
    int port,
    Pointer<Utf8> key,
    Pointer<Utf8> value,
  );

  @Native<Void Function(Int64, Pointer<Utf8>)>(symbol: 'load_user_secret')
  external static void _loadUserSecret(int port, Pointer<Utf8> key);

  @Native<Void Function(Int64, Pointer<Utf8>)>(symbol: 'remove_user_secret')
  external static void _removeUserSecret(int port, Pointer<Utf8> key);

  @Native<Void Function(Int64)>(symbol: 'list_user_secrets')
  external static void _listUserSecrets(int port);

  @Native<Void Function(Int64)>(symbol: 'delete_all_user_secrets')
  external static void _deleteAllUserSecrets(int port);

  @Native<Pointer<Utf8> Function()>(symbol: 'veilid_version_string')
  external static Pointer<Utf8> _veilidVersionString();

  @Native<VeilidVersionFFI Function()>(symbol: 'veilid_version')
  external static VeilidVersionFFI _veilidVersion();

  @Native<Pointer<Utf8> Function()>(symbol: 'default_veilid_config')
  external static Pointer<Utf8> _defaultVeilidConfig();

  @Native<Pointer<Utf8> Function()>(symbol: 'veilid_features')
  external static Pointer<Utf8> _veilidFeatures();

  @Native<Void Function(Pointer<_DartPostCObject>, Pointer<Utf8>)>(
    symbol: 'initialize_veilid_flutter',
  )
  external static void _initializeVeilidFlutter(
    Pointer<_DartPostCObject> dartPostCObject,
    Pointer<Utf8> crashPath,
  );

  VeilidFFI() {
    _eagerResolveNativeSymbols();
    // Allow environment to configure
    // ignore: avoid_redundant_argument_values, do_not_use_environment
    final nativeCrashPath = const String.fromEnvironment('VEILID_CRASH_PATH')
        .toNativeUtf8();
    _initializeVeilidFlutter(NativeApi.postCObject, nativeCrashPath);
    malloc.free(nativeCrashPath);
  }

  @override
  void initializeVeilidCore(Map<String, dynamic> platformConfigJson) {
    final nativePlatformConfig = jsonEncode(platformConfigJson).toNativeUtf8();

    _initializeVeilidCore(nativePlatformConfig);
    malloc.free(nativePlatformConfig);
  }

  @override
  void changeLogLevel(String layer, String directives) {
    final nativeDirectives = directives.toNativeUtf8();
    final nativeLayer = layer.toNativeUtf8();
    final out = _changeLogLevel(nativeLayer, nativeDirectives);
    malloc
      ..free(nativeLayer)
      ..free(nativeDirectives);
    if (out == 1) {
      throw VeilidAPIExceptionParseError('Invalid layer', layer);
    }
    if (out == 2) {
      throw VeilidAPIExceptionParseError('Invalid directives', directives);
    }
  }

  @override
  void changeLogIgnore(String layer, List<String> changes) {
    final nativeChanges = jsonEncode(changes.join(',')).toNativeUtf8();
    final nativeLayer = layer.toNativeUtf8();
    _changeLogIgnore(nativeLayer, nativeChanges);
    malloc
      ..free(nativeLayer)
      ..free(nativeChanges);
  }

  @override
  Future<Stream<VeilidUpdate>> startupVeilidCore(VeilidConfig config) async {
    final nativeConfig = jsonEncode(config).toNativeUtf8();
    final recvStreamPort = ReceivePort('veilid_api_stream');
    final sendStreamPort = recvStreamPort.sendPort;
    final recvPort = ReceivePort('startup_veilid_core');
    final sendPort = recvPort.sendPort;
    _startupVeilidCore(
      sendPort.nativePort,
      sendStreamPort.nativePort,
      nativeConfig,
    );
    malloc.free(nativeConfig);
    try {
      final updateStream = await processFutureStream(
        processStreamJson(VeilidUpdate.fromJson, recvStreamPort),
        recvPort.first,
      );
      _updateStreamPorts.add(sendStreamPort.nativePort);
      return updateStream;
    } on Exception {
      // processStreamJson is lazy: nobody subscribes to a failed startup, so
      // its cleanup never runs and the port would pin the event loop
      recvStreamPort.close();
      rethrow;
    }
  }

  @override
  Future<VeilidState> getVeilidState() {
    final recvPort = ReceivePort('get_veilid_state');
    final sendPort = recvPort.sendPort;
    _getVeilidState(sendPort.nativePort);
    return processFutureJson(VeilidState.fromJson, recvPort.first);
  }

  @override
  Future<bool> isShutdown() {
    final recvPort = ReceivePort('is_shutdown');
    final sendPort = recvPort.sendPort;
    _isShutdown(sendPort.nativePort);
    return processFuturePlain<bool>(recvPort.first);
  }

  @override
  Future<void> attach() {
    final recvPort = ReceivePort('attach');
    final sendPort = recvPort.sendPort;
    _attach(sendPort.nativePort);
    return processFutureVoid(recvPort.first);
  }

  @override
  Future<void> detach() {
    final recvPort = ReceivePort('detach');
    final sendPort = recvPort.sendPort;
    _detach(sendPort.nativePort);
    return processFutureVoid(recvPort.first);
  }

  @override
  Future<void> changeConfig(VeilidConfig config) {
    final nativeConfig = jsonEncode(config).toNativeUtf8();
    final recvPort = ReceivePort('change_config');
    final sendPort = recvPort.sendPort;
    _changeConfig(sendPort.nativePort, nativeConfig);
    malloc.free(nativeConfig);
    return processFutureVoid(recvPort.first);
  }

  @override
  Future<void> reportAppLifecycleState(VeilidAppLifecycleState state) {
    final nativeState = jsonEncode(state).toNativeUtf8();
    final recvPort = ReceivePort('report_app_lifecycle_state');
    final sendPort = recvPort.sendPort;
    _reportAppLifecycleState(sendPort.nativePort, nativeState);
    malloc.free(nativeState);
    return processFutureVoid(recvPort.first);
  }

  @override
  Future<VeilidStateAttachment> waitForAttachmentCondition({
    VeilidStateAttachmentCondition? condition,
    Duration? timeout,
  }) {
    final nativeCondition = condition != null
        ? jsonEncode(condition).toNativeUtf8()
        : nullptr;
    final recvPort = ReceivePort('wait_for_attachment_condition');
    final sendPort = recvPort.sendPort;
    _waitForAttachmentCondition(
      sendPort.nativePort,
      nativeCondition,
      timeout?.inMilliseconds ?? 0,
    );
    if (nativeCondition != nullptr) {
      malloc.free(nativeCondition);
    }
    return processFutureJson(VeilidStateAttachment.fromJson, recvPort.first);
  }

  @override
  Future<void> shutdownVeilidCore() {
    final recvPort = ReceivePort('shutdown_veilid_core');
    final sendPort = recvPort.sendPort;
    // the reply port is one-shot and carries no identity, so the stream port is
    // what tells rust whose reference to release
    final streamPort = _updateStreamPorts.isEmpty
        ? 0
        : _updateStreamPorts.removeLast();
    _shutdownVeilidCore(sendPort.nativePort, streamPort);

    return processFutureVoid(recvPort.first);
  }

  @override
  Future<VeilidRoutingContext> routingContext() async {
    final recvPort = ReceivePort('routing_context');
    final sendPort = recvPort.sendPort;
    _routingContext(sendPort.nativePort);
    final id = await processFuturePlain<int>(recvPort.first);
    return VeilidRoutingContextFFI._(_Ctx(id));
  }

  @override
  Future<MemberId> generateMemberId(PublicKey writerKey) {
    final recvPort = ReceivePort('generate_member_id');
    final sendPort = recvPort.sendPort;
    final nativeWriterKey = jsonEncode(writerKey).toNativeUtf8();
    _generateMemberId(sendPort.nativePort, nativeWriterKey);
    malloc.free(nativeWriterKey);
    return processFutureJson(MemberId.fromJson, recvPort.first);
  }

  @override
  Future<RecordKey> getDHTRecordKey(
    DHTSchema schema,
    PublicKey owner,
    SharedSecret? encryptionKey,
  ) {
    final nativeSchema = jsonEncode(schema).toNativeUtf8();
    final nativeOwner = jsonEncode(owner).toNativeUtf8();
    final nativeEncryptionKey = encryptionKey != null
        ? jsonEncode(encryptionKey).toNativeUtf8()
        : nullptr;
    final recvPort = ReceivePort('routing_context_get_dht_record_key');
    final sendPort = recvPort.sendPort;
    _getDHTRecordKey(
      sendPort.nativePort,
      nativeSchema,
      nativeOwner,
      nativeEncryptionKey,
    );
    malloc.free(nativeSchema);
    malloc.free(nativeOwner);
    if (nativeEncryptionKey != nullptr) {
      malloc.free(nativeEncryptionKey);
    }
    return processFutureJson(RecordKey.fromJson, recvPort.first);
  }

  @override
  Future<VeilidDHTTransaction> transactDHTRecords(
    List<RecordKey> recordKeys, {
    TransactDHTRecordsOptions? options,
  }) async {
    final nativeRecordKeys = jsonEncode(recordKeys).toNativeUtf8();
    final nativeOptions = options != null
        ? jsonEncode(options).toNativeUtf8()
        : nullptr;
    final recvPort = ReceivePort('transact_dht_records');
    final sendPort = recvPort.sendPort;
    _transactDHTRecords(sendPort.nativePort, nativeRecordKeys, nativeOptions);
    malloc.free(nativeRecordKeys);
    if (nativeOptions != nullptr) {
      malloc.free(nativeOptions);
    }

    final id = await processFuturePlain<int>(recvPort.first);
    return VeilidDHTTransactionFFI._(_DTX(id));
  }

  @override
  Future<RouteBlob> newPrivateRoute() {
    final recvPort = ReceivePort('new_private_route');
    final sendPort = recvPort.sendPort;
    _newPrivateRoute(sendPort.nativePort);
    return processFutureJson(RouteBlob.fromJson, recvPort.first);
  }

  @override
  Future<RouteBlob> newCustomPrivateRoute(PrivateSpec privateSpec) {
    final recvPort = ReceivePort('new_custom_private_route');
    final sendPort = recvPort.sendPort;
    final nativePrivateSpec = jsonEncode(privateSpec).toNativeUtf8();
    _newCustomPrivateRoute(sendPort.nativePort, nativePrivateSpec);
    malloc.free(nativePrivateSpec);

    return processFutureJson(RouteBlob.fromJson, recvPort.first);
  }

  @override
  Future<RouteId> importRemotePrivateRoute(Uint8List blob) {
    final nativeEncodedBlob = base64UrlNoPadEncode(blob).toNativeUtf8();

    final recvPort = ReceivePort('import_remote_private_route');
    final sendPort = recvPort.sendPort;
    _importRemotePrivateRoute(sendPort.nativePort, nativeEncodedBlob);
    malloc.free(nativeEncodedBlob);
    return processFutureJson(RouteId.fromJson, recvPort.first);
  }

  @override
  Future<void> releasePrivateRoute(RouteId routeId) {
    final nativeRouteId = jsonEncode(routeId).toNativeUtf8();

    final recvPort = ReceivePort('release_private_route');
    final sendPort = recvPort.sendPort;
    _releasePrivateRoute(sendPort.nativePort, nativeRouteId);
    malloc.free(nativeRouteId);
    return processFutureVoid(recvPort.first);
  }

  @override
  Future<void> appCallReply(String callId, Uint8List message) {
    final nativeCallId = callId.toNativeUtf8();
    final nativeEncodedMessage = base64UrlNoPadEncode(message).toNativeUtf8();
    final recvPort = ReceivePort('app_call_reply');
    final sendPort = recvPort.sendPort;
    _appCallReply(sendPort.nativePort, nativeCallId, nativeEncodedMessage);
    malloc.free(nativeCallId);
    malloc.free(nativeEncodedMessage);
    return processFutureVoid(recvPort.first);
  }

  @override
  Future<VeilidTableDB> openTableDB(String name, int columnCount) async {
    final recvPort = ReceivePort('open_table_db');
    final sendPort = recvPort.sendPort;
    final nativeName = name.toNativeUtf8();
    _openTableDb(sendPort.nativePort, nativeName, columnCount);
    malloc.free(nativeName);
    final id = await processFuturePlain<int>(recvPort.first);
    return VeilidTableDBFFI._(_TDB(id));
  }

  @override
  Future<bool> deleteTableDB(String name) {
    final recvPort = ReceivePort('delete_table_db');
    final sendPort = recvPort.sendPort;
    final nativeName = name.toNativeUtf8();
    _deleteTableDb(sendPort.nativePort, nativeName);
    malloc.free(nativeName);
    return processFuturePlain(recvPort.first);
  }

  @override
  Future<void> renameTableDB(String oldName, String newName) {
    final recvPort = ReceivePort('rename_table_db');
    final sendPort = recvPort.sendPort;
    final nativeOldName = oldName.toNativeUtf8();
    final nativeNewName = newName.toNativeUtf8();
    _renameTableDb(sendPort.nativePort, nativeOldName, nativeNewName);
    malloc.free(nativeOldName);
    malloc.free(nativeNewName);
    return processFutureVoid(recvPort.first);
  }

  @override
  List<CryptoKind> validCryptoKinds() {
    final vckString = _validCryptoKinds();
    final vck = jsonDecode(vckString.toDartString()) as List<dynamic>;
    _freeString(vckString);
    return vck.cast<int>().map(CryptoKind.fromInt).toList();
  }

  @override
  Future<VeilidCryptoSystem> getCryptoSystem(CryptoKind kind) async {
    if (!validCryptoKinds().contains(kind)) {
      throw const VeilidAPIExceptionGeneric('unsupported cryptosystem');
    }
    return VeilidCryptoSystemFFI._(kind.toInt());
  }

  @override
  Future<List<PublicKey>?> verifySignatures(
    List<PublicKey> nodeIds,
    Uint8List data,
    List<Signature> signatures,
  ) {
    final nativeNodeIds = jsonEncode(nodeIds).toNativeUtf8();
    final nativeData = base64UrlNoPadEncode(data).toNativeUtf8();
    final nativeSignatures = jsonEncode(signatures).toNativeUtf8();

    final recvPort = ReceivePort('verify_signatures');
    final sendPort = recvPort.sendPort;
    _verifySignatures(
      sendPort.nativePort,
      nativeNodeIds,
      nativeData,
      nativeSignatures,
    );
    malloc.free(nativeNodeIds);
    malloc.free(nativeData);
    malloc.free(nativeSignatures);
    return processFutureOptJson(
      jsonListConstructor<PublicKey>(PublicKey.fromJson),
      recvPort.first,
    );
  }

  @override
  Future<List<Signature>> generateSignatures(
    Uint8List data,
    List<KeyPair> keyPairs,
  ) {
    final nativeData = base64UrlNoPadEncode(data).toNativeUtf8();
    final nativeKeyPairs = jsonEncode(keyPairs).toNativeUtf8();

    final recvPort = ReceivePort('generate_signatures');
    final sendPort = recvPort.sendPort;
    _generateSignatures(sendPort.nativePort, nativeData, nativeKeyPairs);
    malloc.free(nativeData);
    malloc.free(nativeKeyPairs);
    return processFutureJson(
      jsonListConstructor<Signature>(Signature.fromJson),
      recvPort.first,
    );
  }

  @override
  Timestamp now() {
    final ts = _now();
    return Timestamp(value: BigInt.from(ts));
  }

  @override
  Timestamp nowNonDecreasing() {
    final ts = _nowNonDecreasing();
    return Timestamp(value: BigInt.from(ts));
  }

  @override
  Timestamp nowIncreasing() {
    final ts = _nowIncreasing();
    return Timestamp(value: BigInt.from(ts));
  }

  @override
  Future<KeyPair> generateKeyPair(CryptoKind kind) {
    final recvPort = ReceivePort('generate_key_pair');
    final sendPort = recvPort.sendPort;
    _generateKeyPair(sendPort.nativePort, kind.toInt());
    return processFutureJson(KeyPair.fromJson, recvPort.first);
  }

  @override
  Future<String> debug(String command) {
    final nativeCommand = command.toNativeUtf8();
    final recvPort = ReceivePort('debug');
    final sendPort = recvPort.sendPort;
    _debug(sendPort.nativePort, nativeCommand);
    malloc.free(nativeCommand);
    return processFuturePlain(recvPort.first);
  }

  @override
  Future<bool> saveUserSecret(String key, Uint8List value) {
    final nativeKey = key.toNativeUtf8();
    final nativeValue = base64UrlNoPadEncode(value).toNativeUtf8();
    final recvPort = ReceivePort('save_user_secret');
    final sendPort = recvPort.sendPort;
    _saveUserSecret(sendPort.nativePort, nativeKey, nativeValue);
    malloc.free(nativeKey);
    malloc.free(nativeValue);
    return processFuturePlain(recvPort.first);
  }

  @override
  Future<Uint8List?> loadUserSecret(String key) async {
    final nativeKey = key.toNativeUtf8();
    final recvPort = ReceivePort('load_user_secret');
    final sendPort = recvPort.sendPort;
    _loadUserSecret(sendPort.nativePort, nativeKey);
    malloc.free(nativeKey);
    final out = await processFuturePlain<String?>(recvPort.first);
    if (out == null) {
      return null;
    }
    return base64UrlNoPadDecode(out);
  }

  @override
  Future<bool> removeUserSecret(String key) {
    final nativeKey = key.toNativeUtf8();
    final recvPort = ReceivePort('remove_user_secret');
    final sendPort = recvPort.sendPort;
    _removeUserSecret(sendPort.nativePort, nativeKey);
    malloc.free(nativeKey);
    return processFuturePlain(recvPort.first);
  }

  @override
  Future<List<String>> listUserSecrets() {
    final recvPort = ReceivePort('list_user_secrets');
    final sendPort = recvPort.sendPort;
    _listUserSecrets(sendPort.nativePort);
    return processFutureJson(
      (j) => (j as List<dynamic>).cast<String>(),
      recvPort.first,
    );
  }

  @override
  Future<void> deleteAllUserSecrets() {
    final recvPort = ReceivePort('delete_all_user_secrets');
    final sendPort = recvPort.sendPort;
    _deleteAllUserSecrets(sendPort.nativePort);
    return processFutureVoid(recvPort.first);
  }

  @override
  String veilidVersionString() {
    final versionString = _veilidVersionString();
    final ret = versionString.toDartString();
    _freeString(versionString);
    return ret;
  }

  @override
  VeilidVersion veilidVersion() {
    final version = _veilidVersion();
    return VeilidVersion(version.major, version.minor, version.patch);
  }

  @override
  String defaultVeilidConfig() {
    final ptr = _defaultVeilidConfig();
    final out = ptr.toDartString();
    _freeString(ptr);
    return out;
  }

  @override
  List<String> veilidFeatures() {
    final ptr = _veilidFeatures();
    final str = ptr.toDartString();
    _freeString(ptr);
    final features = jsonDecode(str) as List<dynamic>;
    return features.map((e) => e as String).toList();
  }
}
