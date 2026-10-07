import 'dart:convert';

import 'package:equatable/equatable.dart';
import 'package:flutter/foundation.dart';
import 'package:freezed_annotation/freezed_annotation.dart';

import 'veilid_stub.dart'
    if (dart.library.io) 'veilid_ffi.dart'
    if (dart.library.js) 'veilid_js.dart';

//////////////////////////////////////
/// JSON Encode Helper

/// Lifts a per-element JSON constructor into one that decodes a JSON list.
List<T> Function(dynamic) jsonListConstructor<T>(
  T Function(dynamic) jsonConstructor,
) =>
    (dynamic j) => (j as List<dynamic>).map(jsonConstructor).toList();

/// Lifts a per-element JSON constructor into one that decodes a JSON list, or
/// null when the input is null.
List<T>? Function(dynamic) optJsonListConstructor<T>(
  T Function(dynamic) jsonConstructor,
) =>
    (dynamic j) =>
        j == null ? null : (j as List<dynamic>).map(jsonConstructor).toList();

//////////////////////////////////////
/// Base64 URL No-Pad Encode/Decode

/// Encodes [bytes] as unpadded base64url.
String base64UrlNoPadEncode(List<int> bytes) {
  var x = base64Url.encode(bytes);
  while (x.endsWith('=')) {
    x = x.substring(0, x.length - 1);
  }
  return x;
}

/// Decodes unpadded base64url [source], restoring padding first.
///
/// Throws [FormatException] if [source] is not valid base64url.
Uint8List base64UrlNoPadDecode(String source) {
  source = base64Url.normalize(source);
  return base64Url.decode(source);
}

/// [base64UrlNoPadDecode] over a dynamic value cast to [String].
///
/// Throws [TypeError] if [source] is not a [String], or [FormatException] if it
/// is not valid base64url.
Uint8List base64UrlNoPadDecodeDynamic(dynamic source) =>
    base64UrlNoPadDecode(source as String);

/// JSON converter for [Uint8List], encoding as unpadded base64url, or as a
/// JS-native byte array on web when constructed with [Uint8ListJsonConverter.jsIsArray].
class Uint8ListJsonConverter implements JsonConverter<Uint8List, dynamic> {
  /// Converts bytes to and from base64url strings.
  const Uint8ListJsonConverter() : _jsIsArray = false;

  /// On web, converts bytes to and from a JS-native byte array instead of a
  /// base64url string.
  const Uint8ListJsonConverter.jsIsArray() : _jsIsArray = true;

  final bool _jsIsArray;

  @override
  Uint8List fromJson(dynamic json) => kIsWeb && _jsIsArray
      ? convertUint8ListFromJson(json)
      : base64UrlNoPadDecode(json as String);

  @override
  dynamic toJson(Uint8List data) => kIsWeb && _jsIsArray
      ? convertUint8ListToJson(data)
      : base64UrlNoPadEncode(data);
}

/// Base class for fixed-purpose byte values carried as unpadded base64url
/// strings. Subclasses are the bare crypto types ([BarePublicKey], [Nonce],
/// etc).
@immutable
sealed class EncodedString extends Equatable {
  ////////////////////////////////////////////////////////////////////////////

  /// The unpadded base64url encoding of the value.
  final String contents;

  EncodedString._fromBytes(Uint8List bytes)
    : contents = base64UrlNoPadEncode(bytes);

  EncodedString._fromString(String s) : contents = s {
    // Ensure things can be decoded, will throw an exception if it fails
    base64UrlNoPadDecode(contents);
  }

  EncodedString._fromJson(dynamic json) : contents = json as String {
    // Ensure things can be decoded, will throw an exception if it fails
    base64UrlNoPadDecode(contents);
  }

  /// Encodes the value as its base64url JSON string.
  String toJson() => toString();

  /// Decodes the value back to raw bytes.
  Uint8List toBytes() => base64UrlNoPadDecode(contents);

  @override
  String toString() => contents;

  ////////////////////////////////////////////////////////////////////////////

  /// Constructs the [EncodedString] subclass [T] from raw [bytes].
  ///
  /// Throws [UnimplementedError] if [T] is not a known [EncodedString] subtype.
  static T fromBytes<T extends EncodedString>(Uint8List bytes) {
    switch (T) {
      case const (BarePublicKey):
        return BarePublicKey.fromBytes(bytes) as T;
      case const (BareSignature):
        return BareSignature.fromBytes(bytes) as T;
      case const (Nonce):
        return Nonce.fromBytes(bytes) as T;
      case const (BareSecretKey):
        return BareSecretKey.fromBytes(bytes) as T;
      case const (BareEncapsulationKey):
        return BareEncapsulationKey.fromBytes(bytes) as T;
      case const (BareDecapsulationKey):
        return BareDecapsulationKey.fromBytes(bytes) as T;
      case const (BareHashDigest):
        return BareHashDigest.fromBytes(bytes) as T;
      case const (BareOpaqueRecordKey):
        return BareOpaqueRecordKey.fromBytes(bytes) as T;
      case const (BareSharedSecret):
        return BareSharedSecret.fromBytes(bytes) as T;
      case const (BareRouteId):
        return BareRouteId.fromBytes(bytes) as T;
      case const (BareNodeId):
        return BareNodeId.fromBytes(bytes) as T;
      case const (BareMemberId):
        return BareMemberId.fromBytes(bytes) as T;
      default:
        throw UnimplementedError();
    }
  }

  /// Constructs the [EncodedString] subclass [T] from base64url string [s].
  ///
  /// Throws [UnimplementedError] if [T] is not a known [EncodedString] subtype,
  /// or [FormatException] if [s] is not valid base64url.
  static T fromString<T extends EncodedString>(String s) {
    switch (T) {
      case const (BarePublicKey):
        return BarePublicKey.fromString(s) as T;
      case const (BareSignature):
        return BareSignature.fromString(s) as T;
      case const (Nonce):
        return Nonce.fromString(s) as T;
      case const (BareSecretKey):
        return BareSecretKey.fromString(s) as T;
      case const (BareEncapsulationKey):
        return BareEncapsulationKey.fromString(s) as T;
      case const (BareDecapsulationKey):
        return BareDecapsulationKey.fromString(s) as T;
      case const (BareHashDigest):
        return BareHashDigest.fromString(s) as T;
      case const (BareOpaqueRecordKey):
        return BareOpaqueRecordKey.fromString(s) as T;
      case const (BareSharedSecret):
        return BareSharedSecret.fromString(s) as T;
      case const (BareRouteId):
        return BareRouteId.fromString(s) as T;
      case const (BareNodeId):
        return BareNodeId.fromString(s) as T;
      case const (BareMemberId):
        return BareMemberId.fromString(s) as T;
      default:
        throw UnimplementedError();
    }
  }

  /// Constructs the [EncodedString] subclass [T] from its JSON form.
  ///
  /// Throws [UnimplementedError] if [T] is not a known [EncodedString] subtype,
  /// [TypeError] if [json] is not a [String], or [FormatException] if it is not
  /// valid base64url.
  static T fromJson<T extends EncodedString>(dynamic json) {
    switch (T) {
      case const (BarePublicKey):
        return BarePublicKey.fromJson(json) as T;
      case const (BareSignature):
        return BareSignature.fromJson(json) as T;
      case const (Nonce):
        return Nonce.fromJson(json) as T;
      case const (BareSecretKey):
        return BareSecretKey.fromJson(json) as T;
      case const (BareEncapsulationKey):
        return BareEncapsulationKey.fromJson(json) as T;
      case const (BareDecapsulationKey):
        return BareDecapsulationKey.fromJson(json) as T;
      case const (BareHashDigest):
        return BareHashDigest.fromJson(json) as T;
      case const (BareOpaqueRecordKey):
        return BareOpaqueRecordKey.fromJson(json) as T;
      case const (BareSharedSecret):
        return BareSharedSecret.fromJson(json) as T;
      case const (BareRouteId):
        return BareRouteId.fromJson(json) as T;
      case const (BareNodeId):
        return BareNodeId.fromJson(json) as T;
      case const (BareMemberId):
        return BareMemberId.fromJson(json) as T;
      default:
        throw UnimplementedError();
    }
  }

  @override
  List<Object> get props => [contents];
}

/// A crypto-kind-agnostic public key.
class BarePublicKey extends EncodedString {
  /// Constructs from raw bytes.
  BarePublicKey.fromBytes(super.bytes) : super._fromBytes();

  /// Constructs from a base64url string. Throws [FormatException] if not valid
  /// base64url.
  BarePublicKey.fromString(super.s) : super._fromString();

  /// Constructs from its JSON form. Throws [TypeError] if not a [String], or
  /// [FormatException] if not valid base64url.
  BarePublicKey.fromJson(super.json) : super._fromJson();
}

/// A crypto-kind-agnostic signature.
class BareSignature extends EncodedString {
  /// Constructs from raw bytes.
  BareSignature.fromBytes(super.bytes) : super._fromBytes();

  /// Constructs from a base64url string. Throws [FormatException] if not valid
  /// base64url.
  BareSignature.fromString(super.s) : super._fromString();

  /// Constructs from its JSON form. Throws [TypeError] if not a [String], or
  /// [FormatException] if not valid base64url.
  BareSignature.fromJson(super.json) : super._fromJson();
}

/// A single-use nonce.
class Nonce extends EncodedString {
  /// Constructs from raw bytes.
  Nonce.fromBytes(super.bytes) : super._fromBytes();

  /// Constructs from a base64url string. Throws [FormatException] if not valid
  /// base64url.
  Nonce.fromString(super.s) : super._fromString();

  /// Constructs from its JSON form. Throws [TypeError] if not a [String], or
  /// [FormatException] if not valid base64url.
  Nonce.fromJson(super.json) : super._fromJson();
}

/// A crypto-kind-agnostic secret key.
class BareSecretKey extends EncodedString {
  /// Constructs from raw bytes.
  BareSecretKey.fromBytes(super.bytes) : super._fromBytes();

  /// Constructs from a base64url string. Throws [FormatException] if not valid
  /// base64url.
  BareSecretKey.fromString(super.s) : super._fromString();

  /// Constructs from its JSON form. Throws [TypeError] if not a [String], or
  /// [FormatException] if not valid base64url.
  BareSecretKey.fromJson(super.json) : super._fromJson();
}

/// A crypto-kind-agnostic KEM encapsulation key.
class BareEncapsulationKey extends EncodedString {
  /// Constructs from raw bytes.
  BareEncapsulationKey.fromBytes(super.bytes) : super._fromBytes();

  /// Constructs from a base64url string. Throws [FormatException] if not valid
  /// base64url.
  BareEncapsulationKey.fromString(super.s) : super._fromString();

  /// Constructs from its JSON form. Throws [TypeError] if not a [String], or
  /// [FormatException] if not valid base64url.
  BareEncapsulationKey.fromJson(super.json) : super._fromJson();
}

/// A crypto-kind-agnostic KEM decapsulation key.
class BareDecapsulationKey extends EncodedString {
  /// Constructs from raw bytes.
  BareDecapsulationKey.fromBytes(super.bytes) : super._fromBytes();

  /// Constructs from a base64url string. Throws [FormatException] if not valid
  /// base64url.
  BareDecapsulationKey.fromString(super.s) : super._fromString();

  /// Constructs from its JSON form. Throws [TypeError] if not a [String], or
  /// [FormatException] if not valid base64url.
  BareDecapsulationKey.fromJson(super.json) : super._fromJson();
}

/// A crypto-kind-agnostic hash digest.
class BareHashDigest extends EncodedString {
  /// Constructs from raw bytes.
  BareHashDigest.fromBytes(super.bytes) : super._fromBytes();

  /// Constructs from a base64url string. Throws [FormatException] if not valid
  /// base64url.
  BareHashDigest.fromString(super.s) : super._fromString();

  /// Constructs from its JSON form. Throws [TypeError] if not a [String], or
  /// [FormatException] if not valid base64url.
  BareHashDigest.fromJson(super.json) : super._fromJson();
}

/// A crypto-kind-agnostic opaque DHT record key.
class BareOpaqueRecordKey extends EncodedString {
  /// Constructs from raw bytes.
  BareOpaqueRecordKey.fromBytes(super.bytes) : super._fromBytes();

  /// Constructs from a base64url string. Throws [FormatException] if not valid
  /// base64url.
  BareOpaqueRecordKey.fromString(super.s) : super._fromString();

  /// Constructs from its JSON form. Throws [TypeError] if not a [String], or
  /// [FormatException] if not valid base64url.
  BareOpaqueRecordKey.fromJson(super.json) : super._fromJson();
}

/// A crypto-kind-agnostic shared secret.
class BareSharedSecret extends EncodedString {
  /// Constructs from raw bytes.
  BareSharedSecret.fromBytes(super.bytes) : super._fromBytes();

  /// Constructs from a base64url string. Throws [FormatException] if not valid
  /// base64url.
  BareSharedSecret.fromString(super.s) : super._fromString();

  /// Constructs from its JSON form. Throws [TypeError] if not a [String], or
  /// [FormatException] if not valid base64url.
  BareSharedSecret.fromJson(super.json) : super._fromJson();
}

/// A crypto-kind-agnostic private route id.
class BareRouteId extends EncodedString {
  /// Constructs from raw bytes.
  BareRouteId.fromBytes(super.bytes) : super._fromBytes();

  /// Constructs from a base64url string. Throws [FormatException] if not valid
  /// base64url.
  BareRouteId.fromString(super.s) : super._fromString();

  /// Constructs from its JSON form. Throws [TypeError] if not a [String], or
  /// [FormatException] if not valid base64url.
  BareRouteId.fromJson(super.json) : super._fromJson();
}

/// A crypto-kind-agnostic node id.
class BareNodeId extends EncodedString {
  /// Constructs from raw bytes.
  BareNodeId.fromBytes(super.bytes) : super._fromBytes();

  /// Constructs from a base64url string. Throws [FormatException] if not valid
  /// base64url.
  BareNodeId.fromString(super.s) : super._fromString();

  /// Constructs from its JSON form. Throws [TypeError] if not a [String], or
  /// [FormatException] if not valid base64url.
  BareNodeId.fromJson(super.json) : super._fromJson();
}

/// A crypto-kind-agnostic group member id.
class BareMemberId extends EncodedString {
  /// Constructs from raw bytes.
  BareMemberId.fromBytes(super.bytes) : super._fromBytes();

  /// Constructs from a base64url string. Throws [FormatException] if not valid
  /// base64url.
  BareMemberId.fromString(super.s) : super._fromString();

  /// Constructs from its JSON form. Throws [TypeError] if not a [String], or
  /// [FormatException] if not valid base64url.
  BareMemberId.fromJson(super.json) : super._fromJson();
}
