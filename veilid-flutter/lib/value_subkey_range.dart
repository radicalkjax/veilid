import 'dart:math';

import 'package:equatable/equatable.dart';
import 'package:freezed_annotation/freezed_annotation.dart';

/// An inclusive range of DHT subkeys from [low] to [high].
@immutable
class ValueSubkeyRange extends Equatable {
  /// The lowest subkey in the range, inclusive.
  final int low;

  /// The highest subkey in the range, inclusive.
  final int high;

  /// A range covering `low..=high`. In debug builds, throws [AssertionError] if
  /// `low < 0` or `low > high`.
  const ValueSubkeyRange({required this.low, required this.high})
    : assert(low >= 0 && low <= high, 'range is invalid');

  /// A range containing one subkey.
  factory ValueSubkeyRange.single(int val) =>
      ValueSubkeyRange(low: val, high: val);

  /// A range covering `low..=high`.
  factory ValueSubkeyRange.make(int low, int high) =>
      ValueSubkeyRange(low: low, high: high);

  /// A range from a `(low, high)` tuple.
  factory ValueSubkeyRange.fromIntPair((int, int) pair) =>
      ValueSubkeyRange(low: pair.$1, high: pair.$2);

  /// A range from a two-element `[low, high]` list. In debug builds, throws
  /// [AssertionError] if [intlist] does not have exactly two items, or if
  /// `low < 0` or `low > high`.
  factory ValueSubkeyRange.fromIntList(List<int> intlist) {
    assert(intlist.length == 2, 'range must be a two item list');
    return ValueSubkeyRange(low: intlist[0], high: intlist[1]);
  }

  /// Decodes a range from its JSON `[low, high]` form. Throws [TypeError] if
  /// [json] is not a list or holds non-int elements; in debug builds also throws
  /// [AssertionError] if the list is not two items, or if `low < 0` or
  /// `low > high`.
  factory ValueSubkeyRange.fromJson(dynamic json) =>
      ValueSubkeyRange.fromIntList((json as List<dynamic>).cast<int>());

  /// Encodes the range as a `[low, high]` list.
  List<int> toJson() => <int>[low, high];

  @override
  List<Object> get props => [low, high];
}

/// Set-like operations on a single [ValueSubkeyRange].
extension ValueSubkeyRangeExt on ValueSubkeyRange {
  /// True if subkey [v] falls within the range.
  bool contains(int v) => low <= v && v <= high;

  /// The range with subkey [v] removed, split into zero, one, or two ranges.
  List<ValueSubkeyRange> remove(int v) {
    if (v < low || v > high) {
      return [ValueSubkeyRange(low: low, high: high)];
    }
    if (v == low) {
      if (v == high) {
        return [];
      } else {
        return [ValueSubkeyRange(low: v + 1, high: high)];
      }
    } else if (v == high) {
      return [ValueSubkeyRange(low: low, high: v - 1)];
    } else {
      return [
        ValueSubkeyRange(low: low, high: v - 1),
        ValueSubkeyRange(low: v + 1, high: high),
      ];
    }
  }

  /// The overlap with [other], or null if the ranges are disjoint.
  ValueSubkeyRange? intersect(ValueSubkeyRange other) {
    if (high < other.low || low > other.high) {
      return null;
    }
    return ValueSubkeyRange(
      low: max(low, other.low),
      high: min(high, other.high),
    );
  }

  /// The merged range with [other], or null if the ranges are not adjacent or
  /// overlapping.
  ValueSubkeyRange? union(ValueSubkeyRange other) {
    if (high < (other.low - 1) || low > (other.high + 1)) {
      return null;
    }
    return ValueSubkeyRange(
      low: min(low, other.low),
      high: max(high, other.high),
    );
  }
}

/// Operations on an ordered, disjoint list of [ValueSubkeyRange], mirroring the
/// Rust `ValueSubkeyRangeSet`.
extension ListValueSubkeyRangeExt on List<ValueSubkeyRange> {
  /// Builds a list of ranges from a list of `(low, high)` tuples.
  static List<ValueSubkeyRange> fromIntPairs(List<(int, int)> x) =>
      x.map(ValueSubkeyRange.fromIntPair).toList();

  /// Asserts the ranges are sorted and disjoint. In debug builds, throws
  /// [AssertionError] if any range is out of order or overlaps the previous one.
  void validate() {
    int? lastHigh;
    for (final r in this) {
      assert(
        lastHigh == null || r.low > lastHigh,
        'subrange not in order or disjoint',
      );
      lastHigh = r.high;
    }
  }

  /// True if subkey [v] is present in any range.
  bool containsSubkey(int v) => indexWhere((e) => e.contains(v)) != -1;

  /// The list with subkey [v] removed.
  List<ValueSubkeyRange> removeSubkey(int v) {
    for (var i = 0; i < length; i++) {
      if (this[i].contains(v)) {
        return [...sublist(0, i), ...this[i].remove(v), ...sublist(i + 1)];
      }
    }
    return toList();
  }

  /// The list with subkey [v] inserted, merging adjacent ranges.
  List<ValueSubkeyRange> insertSubkey(int v) =>
      unionSubkeys([ValueSubkeyRange.single(v)]);

  /// The lowest subkey across all ranges, or null if empty.
  int? get firstSubkey => isNotEmpty ? first.low : null;

  /// The subkeys present in both this list and [other].
  List<ValueSubkeyRange> intersectSubkeys(List<ValueSubkeyRange> other) {
    final out = <ValueSubkeyRange>[];
    for (var i = 0, j = 0; i < length && j < other.length;) {
      final vsrThis = this[i];
      final vsrOther = other[j];
      if (vsrThis.high < vsrOther.low) {
        i++;
        continue;
      }
      if (vsrOther.high < vsrThis.low) {
        j++;
        continue;
      }

      // Otherwise we intersect
      out.add(vsrThis.intersect(vsrOther)!);

      // Iterate whichever has a lower high
      // If they both have the same high then both ranges are exhausted
      // and should be iterated
      if (vsrThis.high < vsrOther.high) {
        // Iterate this because other could still have some overlaps
        i++;
      } else if (vsrThis.high == vsrOther.high) {
        // Iterate both because both ranges are exhausted
        i++;
        j++;
      } else {
        // Iterate other because this could still have some overlaps
        j++;
      }
    }
    return out;
  }

  /// The subkeys present in either this list or [other], merging adjacent
  /// ranges.
  List<ValueSubkeyRange> unionSubkeys(List<ValueSubkeyRange> other) {
    final out = <ValueSubkeyRange>[];
    ValueSubkeyRange? current;
    for (var i = 0, j = 0; i < length || j < other.length;) {
      if (i == length) {
        current = other[j];
        j++;
      } else if (j == other.length) {
        current = this[i];
        i++;
      } else if (this[i].low < other[j].low) {
        current = this[i];
        i++;
      } else {
        current = other[j];
        j++;
      }

      if (out.isNotEmpty && out.last.high >= (current.low - 1)) {
        out[out.length - 1] = ValueSubkeyRange(
          low: out.last.low,
          high: max(out.last.high, current.high),
        );
      } else {
        out.add(current);
      }
    }
    return out;
  }
}
