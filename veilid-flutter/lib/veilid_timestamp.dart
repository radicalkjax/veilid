import 'package:equatable/equatable.dart';
import 'package:fixnum/fixnum.dart';
import 'package:meta/meta.dart';

//////////////////////////////////////
/// Timestamp
@immutable
class Timestamp extends Equatable implements Comparable<Timestamp> {
  /// Microseconds since the epoch.
  final BigInt value;

  /// Make a timestamp from a raw microseconds-since-epoch [value].
  const Timestamp({required this.value});

  /// The zero timestamp.
  factory Timestamp.zero() => Timestamp(value: BigInt.zero);

  /// Make a timestamp from an unsigned 64-bit microseconds value.
  factory Timestamp.fromInt64(Int64 i64) => Timestamp(
    value:
        (BigInt.from((i64 >> 32).toUnsigned(32).toInt()) << 32) |
        BigInt.from(i64.toUnsigned(32).toInt()),
  );

  /// Parse a timestamp from a decimal microseconds string. Throws
  /// [FormatException] if [s] is not a valid integer.
  factory Timestamp.fromString(String s) => Timestamp(value: BigInt.parse(s));

  /// Decode a [Timestamp] from its JSON representation. Throws [TypeError] if
  /// [json] is not a string, or [FormatException] if it is not a valid integer.
  factory Timestamp.fromJson(dynamic json) =>
      Timestamp.fromString(json as String);

  @override
  List<Object> get props => [value];

  @override
  int compareTo(Timestamp other) => value.compareTo(other.value);

  /// Returns true if this timestamp is earlier than [other].
  bool operator <(Timestamp other) => compareTo(other) < 0;

  /// Returns true if this timestamp is at or before [other].
  bool operator <=(Timestamp other) => compareTo(other) <= 0;

  /// Returns true if this timestamp is later than [other].
  bool operator >(Timestamp other) => compareTo(other) > 0;

  /// Returns true if this timestamp is at or after [other].
  bool operator >=(Timestamp other) => compareTo(other) >= 0;

  @override
  String toString() => value.toString();

  /// Encode this [Timestamp] to its JSON representation.
  String toJson() => toString();

  /// This timestamp as an unsigned 64-bit microseconds value.
  Int64 toInt64() => Int64.fromInts(
    (value >> 32).toUnsigned(32).toInt(),
    value.toUnsigned(32).toInt(),
  );

  /// The duration from [other] to this timestamp.
  TimestampDuration diff(Timestamp other) =>
      TimestampDuration(value: value - other.value);

  /// This timestamp shifted later by [dur].
  Timestamp offset(TimestampDuration dur) =>
      Timestamp(value: value + dur.value);
}

/// A span of time measured in microseconds.
@immutable
class TimestampDuration extends Equatable
    implements Comparable<TimestampDuration> {
  /// The duration in microseconds.
  final BigInt value;

  /// Make a duration from a raw microseconds [value].
  const TimestampDuration({required this.value});

  /// Make a duration from an unsigned 64-bit microseconds value.
  factory TimestampDuration.fromInt64(Int64 i64) => TimestampDuration(
    value:
        (BigInt.from((i64 >> 32).toUnsigned(32).toInt()) << 32) |
        BigInt.from(i64.toUnsigned(32).toInt()),
  );

  /// Make a duration from a number of milliseconds.
  factory TimestampDuration.fromMillis(int millis) =>
      TimestampDuration(value: BigInt.from(millis) * BigInt.from(1000));

  /// Make a duration from a Dart [Duration].
  factory TimestampDuration.fromDuration(Duration d) => TimestampDuration(
    value:
        BigInt.from(d.inSeconds) * BigInt.from(1000000) +
        BigInt.from(d.inMicroseconds % 1000000),
  );

  /// Parse a duration from a decimal microseconds string. Throws
  /// [FormatException] if [s] is not a valid integer.
  factory TimestampDuration.fromString(String s) =>
      TimestampDuration(value: BigInt.parse(s));

  /// Decode a [TimestampDuration] from its JSON representation. Throws
  /// [TypeError] if [json] is not a string, or [FormatException] if it is not a
  /// valid integer.
  factory TimestampDuration.fromJson(dynamic json) =>
      TimestampDuration.fromString(json as String);

  @override
  List<Object> get props => [value];

  @override
  int compareTo(TimestampDuration other) => value.compareTo(other.value);

  /// Returns true if this duration is shorter than [other].
  bool operator <(TimestampDuration other) => compareTo(other) < 0;

  /// Returns true if this duration is at most [other].
  bool operator <=(TimestampDuration other) => compareTo(other) <= 0;

  /// Returns true if this duration is longer than [other].
  bool operator >(TimestampDuration other) => compareTo(other) > 0;

  /// Returns true if this duration is at least [other].
  bool operator >=(TimestampDuration other) => compareTo(other) >= 0;

  @override
  String toString() {
    final biDay = BigInt.from(1000000) * BigInt.from(60 * 60 * 24);
    final biHour = BigInt.from(1000000) * BigInt.from(60 * 60);
    final biMin = BigInt.from(1000000) * BigInt.from(60);
    final biSec = BigInt.from(1000000);
    final biMsec = BigInt.from(1000);

    final days = (value ~/ biDay).toInt();
    final dvalue = value % biDay;
    final hours = (dvalue ~/ biHour).toInt();
    final hvalue = dvalue % biHour;
    final mins = (hvalue ~/ biMin).toInt();
    final mvalue = hvalue % biMin;
    final secs = (mvalue ~/ biSec).toInt();
    final svalue = mvalue % biSec;
    final msecs = (svalue ~/ biMsec).toInt();
    final uvalue = svalue % biMsec;

    if (days == 0 && hours == 0 && mins == 0 && secs == 0) {
      // microseconds format
      return '$msecs.${uvalue.toString().padLeft(3, '0')}ms';
    }
    var out = '';
    if (days != 0) {
      out += '${days}d';
    }
    if (hours != 0) {
      out += '${hours}h';
    }
    if (mins != 0) {
      out += '${mins}m';
    }
    return '$out$secs.${msecs.toString().padLeft(3, '0')}s';
  }

  /// Encode this [TimestampDuration] to its JSON representation.
  String toJson() => value.toString();

  /// This duration as an unsigned 64-bit microseconds value.
  Int64 toInt64() => Int64.fromInts(
    (value >> 32).toUnsigned(32).toInt(),
    value.toUnsigned(32).toInt(),
  );

  /// This duration in milliseconds.
  double toMillis() => value / BigInt.from(1000);

  /// This duration in seconds.
  double toSecs() => value / BigInt.from(1000000);

  /// This duration in microseconds.
  BigInt toMicros() => value;
}
