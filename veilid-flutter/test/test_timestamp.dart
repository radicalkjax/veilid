import 'package:flutter_test/flutter_test.dart';
import 'package:veilid/veilid.dart';

/// Microseconds for a UTC calendar instant, computed independently of the
/// code under test so the calendar conversion is actually checked.
BigInt _utc(int y, int mo, int d, int h, int mi, int s, [int ms = 0]) =>
    BigInt.from(DateTime.utc(y, mo, d, h, mi, s, ms).millisecondsSinceEpoch) *
    BigInt.from(1000);

Future<void> testTimestampHumanForm() async {
  final now = DateTime.now().toUtc();

  // Same day: only the clock time, milliseconds padded to four digits the way
  // veilid-tools human_timestamp does
  final today = _utc(now.year, now.month, now.day, 14, 4, 21, 547);
  expect(Timestamp(value: today).toString(), '14:04:21.0547');

  // A different day in the same month adds the day
  final otherDay = now.day == 1 ? 2 : 1;
  final sameMonth = _utc(now.year, now.month, otherDay, 1, 2, 3, 4);
  expect(
    Timestamp(value: sameMonth).toString(),
    '${otherDay.toString().padLeft(2, '0')}-01:02:03.0004',
  );

  // A different year shows everything
  expect(
    Timestamp(value: _utc(now.year - 1, 3, 9, 23, 59, 59, 999)).toString(),
    '${now.year - 1}/03/09-23:59:59.0999',
  );

  // Sub-millisecond precision is truncated, not rounded, and survives on the
  // web where DateTime has no microseconds
  expect(
    Timestamp(value: today + BigInt.from(999)).toString(),
    '14:04:21.0547',
  );
}

Future<void> testTimestampMachineForm() async {
  const raw = '1788012261547803';
  final ts = Timestamp.fromJson(raw);

  // toJson is the wire form and round trips; toString is not parseable
  expect(ts.toJson(), raw);
  expect(Timestamp.fromJson(ts.toJson()), ts);
  expect(ts.value.toString(), raw);
  expect(() => Timestamp.fromJson(ts.toString()), throwsFormatException);

  expect(Timestamp.zero().toJson(), '0');
  expect(Timestamp.fromInt64(ts.toInt64()), ts);
}

Future<void> testTimestampDurationHumanForm() async {
  TimestampDuration us(int n) => TimestampDuration(value: BigInt.from(n));

  // Under a second reads in milliseconds, matching veilid-tools human_duration
  expect(us(0).toString(), '0.000ms');
  expect(us(1).toString(), '0.001ms');
  expect(us(3385).toString(), '3.385ms');
  expect(us(999999).toString(), '999.999ms');

  // A second or more switches to the d/h/m/s form, empty parts omitted
  expect(us(1000000).toString(), '1.000s');
  expect(us(62500000).toString(), '1m2.500s');
  expect(us(3600000000).toString(), '1h0.000s');
  expect(us(90061005000).toString(), '1d1h1m1.005s');
}

Future<void> testTimestampDurationMachineForm() async {
  final dur = TimestampDuration.fromJson('62500000');

  expect(dur.toJson(), '62500000');
  expect(TimestampDuration.fromJson(dur.toJson()), dur);
  expect(
    () => TimestampDuration.fromJson(dur.toString()),
    throwsFormatException,
  );
  expect(TimestampDuration.fromInt64(dur.toInt64()), dur);

  expect(TimestampDuration.fromMillis(1500).toJson(), '1500000');
  expect(
    TimestampDuration.fromDuration(const Duration(milliseconds: 1500)).toJson(),
    '1500000',
  );
}
