import 'package:equatable/equatable.dart';
import 'package:fixnum/fixnum.dart';
import 'package:meta/meta.dart';

final BigInt _biMicrosPerSec = BigInt.from(1000000);
final BigInt _biMicrosPerMilli = BigInt.from(1000);

/// Days since 1970-01-01 to (year, month, day). Howard Hinnant's
/// `civil_from_days`, ported from veilid-tools `raw_timestamp.rs`.
(int, int, int) _civilFromDays(int days) {
  final z = days + 719468;
  final era = (z >= 0 ? z : z - 146096) ~/ 146097;
  final doe = z - era * 146097;
  final yoe = (doe - doe ~/ 1460 + doe ~/ 36524 - doe ~/ 146096) ~/ 365;
  final y = yoe + era * 400;
  final doy = doe - (365 * yoe + yoe ~/ 4 - yoe ~/ 100);
  final mp = (5 * doy + 2) ~/ 153;
  final d = doy - (153 * mp + 2) ~/ 5 + 1;
  final m = mp < 10 ? mp + 3 : mp - 9;
  return (y + (m <= 2 ? 1 : 0), m, d);
}

/// (year, month, day, hour, minute, second, millisecond) in UTC.
///
/// Done in integer arithmetic rather than through [DateTime] so it matches
/// veilid-core exactly and keeps microsecond precision on the web, where
/// [DateTime] has none.
(int, int, int, int, int, int, int) _utcFields(BigInt micros) {
  final secs = (micros ~/ _biMicrosPerSec).toInt();
  final millis = ((micros % _biMicrosPerSec) ~/ _biMicrosPerMilli).toInt();
  final tod = secs % 86400;
  final (year, month, day) = _civilFromDays(secs ~/ 86400);
  return (year, month, day, tod ~/ 3600, (tod % 3600) ~/ 60, tod % 60, millis);
}

String _pad(int n, int width) => n.toString().padLeft(width, '0');

/// A UTC timestamp with the date fields that match today left off, so a log
/// line carries only what distinguishes it. Matches veilid-tools
/// `human_timestamp`, including its four-digit-padded milliseconds.
String _humanTimestamp(BigInt micros) {
  final (nYear, nMonth, nDay, _, _, _, _) = _utcFields(
    BigInt.from(DateTime.now().microsecondsSinceEpoch),
  );
  final (year, month, day, hour, minute, second, millis) = _utcFields(micros);

  final showYear = nYear != year;
  final showMonth = showYear || nMonth != month;
  final showDate = showMonth || nDay != day;

  return '${showYear ? '${_pad(year, 4)}/' : ''}'
      '${showMonth ? '${_pad(month, 2)}/' : ''}'
      '${showDate ? '${_pad(day, 2)}-' : ''}'
      '${_pad(hour, 2)}:${_pad(minute, 2)}:${_pad(second, 2)}'
      '.${_pad(millis, 4)}';
}

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

  /// Decode a [Timestamp] from its JSON representation: decimal microseconds
  /// since the epoch, as [toJson] writes it. Throws [TypeError] if [json] is
  /// not a string, or [FormatException] if it is not a valid integer.
  ///
  /// There is deliberately no `fromString`: [toString] is a lossy human form
  /// with no inverse, and a decoder named after it would invite round trips
  /// that silently lose precision.
  factory Timestamp.fromJson(dynamic json) =>
      Timestamp(value: BigInt.parse(json as String));

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

  /// A UTC clock time for a human reading a log, eliding the date fields that
  /// match today: `14:04:21.0547`, or `08/21-14:04:21.0547` on another day.
  /// Matches veilid-core's `{:#}` form.
  ///
  /// **Debug output only.** It is lossy and it is not parseable, so anything
  /// stored, parsed, sent over the FFI or compared uses [toJson] or [value],
  /// and anything shown to a user gets formatted for their locale instead.
  @override
  String toString() => _humanTimestamp(value);

  /// Encode this [Timestamp] to its JSON representation: microseconds since
  /// the epoch, in decimal. The machine-readable form, and the one [fromJson]
  /// reads back.
  String toJson() => value.toString();

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

  /// Decode a [TimestampDuration] from its JSON representation: decimal
  /// microseconds, as [toJson] writes it. Throws [TypeError] if [json] is not
  /// a string, or [FormatException] if it is not a valid integer.
  ///
  /// There is deliberately no `fromString`, for the same reason as
  /// [Timestamp.fromJson]: [toString] is a lossy human form with no inverse.
  factory TimestampDuration.fromJson(dynamic json) =>
      TimestampDuration(value: BigInt.parse(json as String));

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

  /// A span for a human reading a log: `3.385ms`, `1d2h3m4.005s`. Matches
  /// veilid-tools `human_duration`.
  ///
  /// **Debug output only**, on the same terms as [Timestamp.toString]: use
  /// [toJson] or [value] for anything machine-read, and locale formatting for
  /// anything a user sees.
  @override
  String toString() {
    final biDay = _biMicrosPerSec * BigInt.from(60 * 60 * 24);
    final biHour = _biMicrosPerSec * BigInt.from(60 * 60);
    final biMin = _biMicrosPerSec * BigInt.from(60);
    final biSec = _biMicrosPerSec;
    final biMsec = _biMicrosPerMilli;

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
