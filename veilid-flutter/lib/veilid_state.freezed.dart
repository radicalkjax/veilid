// GENERATED CODE - DO NOT MODIFY BY HAND
// coverage:ignore-file
// ignore_for_file: type=lint
// ignore_for_file: unused_element, deprecated_member_use, deprecated_member_use_from_same_package, use_function_type_syntax_for_parameters, unnecessary_const, avoid_init_to_null, invalid_override_different_default_values_named, prefer_expression_function_bodies, annotate_overrides, invalid_annotation_target, unnecessary_question_mark

part of 'veilid_state.dart';

// **************************************************************************
// FreezedGenerator
// **************************************************************************

// dart format off
T _$identity<T>(T value) => value;

/// @nodoc
mixin _$LatencyStats {

 TimestampDuration get fastest; TimestampDuration get average; TimestampDuration get slowest; TimestampDuration get tm90; TimestampDuration get tm75; TimestampDuration get p90; TimestampDuration get p75;
/// Create a copy of LatencyStats
/// with the given fields replaced by the non-null parameter values.
@JsonKey(includeFromJson: false, includeToJson: false)
@pragma('vm:prefer-inline')
$LatencyStatsCopyWith<LatencyStats> get copyWith => _$LatencyStatsCopyWithImpl<LatencyStats>(this as LatencyStats, _$identity);

  /// Serializes this LatencyStats to a JSON map.
  Map<String, dynamic> toJson();


@override
bool operator ==(Object other) {
  return identical(this, other) || (other.runtimeType == runtimeType&&other is LatencyStats&&(identical(other.fastest, fastest) || other.fastest == fastest)&&(identical(other.average, average) || other.average == average)&&(identical(other.slowest, slowest) || other.slowest == slowest)&&(identical(other.tm90, tm90) || other.tm90 == tm90)&&(identical(other.tm75, tm75) || other.tm75 == tm75)&&(identical(other.p90, p90) || other.p90 == p90)&&(identical(other.p75, p75) || other.p75 == p75));
}

@JsonKey(includeFromJson: false, includeToJson: false)
@override
int get hashCode => Object.hash(runtimeType,fastest,average,slowest,tm90,tm75,p90,p75);

@override
String toString() {
  return 'LatencyStats(fastest: $fastest, average: $average, slowest: $slowest, tm90: $tm90, tm75: $tm75, p90: $p90, p75: $p75)';
}


}

/// @nodoc
abstract mixin class $LatencyStatsCopyWith<$Res>  {
  factory $LatencyStatsCopyWith(LatencyStats value, $Res Function(LatencyStats) _then) = _$LatencyStatsCopyWithImpl;
@useResult
$Res call({
 TimestampDuration fastest, TimestampDuration average, TimestampDuration slowest, TimestampDuration tm90, TimestampDuration tm75, TimestampDuration p90, TimestampDuration p75
});




}
/// @nodoc
class _$LatencyStatsCopyWithImpl<$Res>
    implements $LatencyStatsCopyWith<$Res> {
  _$LatencyStatsCopyWithImpl(this._self, this._then);

  final LatencyStats _self;
  final $Res Function(LatencyStats) _then;

/// Create a copy of LatencyStats
/// with the given fields replaced by the non-null parameter values.
@pragma('vm:prefer-inline') @override $Res call({Object? fastest = null,Object? average = null,Object? slowest = null,Object? tm90 = null,Object? tm75 = null,Object? p90 = null,Object? p75 = null,}) {
  return _then(_self.copyWith(
fastest: null == fastest ? _self.fastest : fastest // ignore: cast_nullable_to_non_nullable
as TimestampDuration,average: null == average ? _self.average : average // ignore: cast_nullable_to_non_nullable
as TimestampDuration,slowest: null == slowest ? _self.slowest : slowest // ignore: cast_nullable_to_non_nullable
as TimestampDuration,tm90: null == tm90 ? _self.tm90 : tm90 // ignore: cast_nullable_to_non_nullable
as TimestampDuration,tm75: null == tm75 ? _self.tm75 : tm75 // ignore: cast_nullable_to_non_nullable
as TimestampDuration,p90: null == p90 ? _self.p90 : p90 // ignore: cast_nullable_to_non_nullable
as TimestampDuration,p75: null == p75 ? _self.p75 : p75 // ignore: cast_nullable_to_non_nullable
as TimestampDuration,
  ));
}

}


/// Adds pattern-matching-related methods to [LatencyStats].
extension LatencyStatsPatterns on LatencyStats {
/// A variant of `map` that fallback to returning `orElse`.
///
/// It is equivalent to doing:
/// ```dart
/// switch (sealedClass) {
///   case final Subclass value:
///     return ...;
///   case _:
///     return orElse();
/// }
/// ```

@optionalTypeArgs TResult maybeMap<TResult extends Object?>(TResult Function( _LatencyStats value)?  $default,{required TResult orElse(),}){
final _that = this;
switch (_that) {
case _LatencyStats() when $default != null:
return $default(_that);case _:
  return orElse();

}
}
/// A `switch`-like method, using callbacks.
///
/// Callbacks receives the raw object, upcasted.
/// It is equivalent to doing:
/// ```dart
/// switch (sealedClass) {
///   case final Subclass value:
///     return ...;
///   case final Subclass2 value:
///     return ...;
/// }
/// ```

@optionalTypeArgs TResult map<TResult extends Object?>(TResult Function( _LatencyStats value)  $default,){
final _that = this;
switch (_that) {
case _LatencyStats():
return $default(_that);}
}
/// A variant of `map` that fallback to returning `null`.
///
/// It is equivalent to doing:
/// ```dart
/// switch (sealedClass) {
///   case final Subclass value:
///     return ...;
///   case _:
///     return null;
/// }
/// ```

@optionalTypeArgs TResult? mapOrNull<TResult extends Object?>(TResult? Function( _LatencyStats value)?  $default,){
final _that = this;
switch (_that) {
case _LatencyStats() when $default != null:
return $default(_that);case _:
  return null;

}
}
/// A variant of `when` that fallback to an `orElse` callback.
///
/// It is equivalent to doing:
/// ```dart
/// switch (sealedClass) {
///   case Subclass(:final field):
///     return ...;
///   case _:
///     return orElse();
/// }
/// ```

@optionalTypeArgs TResult maybeWhen<TResult extends Object?>(TResult Function( TimestampDuration fastest,  TimestampDuration average,  TimestampDuration slowest,  TimestampDuration tm90,  TimestampDuration tm75,  TimestampDuration p90,  TimestampDuration p75)?  $default,{required TResult orElse(),}) {final _that = this;
switch (_that) {
case _LatencyStats() when $default != null:
return $default(_that.fastest,_that.average,_that.slowest,_that.tm90,_that.tm75,_that.p90,_that.p75);case _:
  return orElse();

}
}
/// A `switch`-like method, using callbacks.
///
/// As opposed to `map`, this offers destructuring.
/// It is equivalent to doing:
/// ```dart
/// switch (sealedClass) {
///   case Subclass(:final field):
///     return ...;
///   case Subclass2(:final field2):
///     return ...;
/// }
/// ```

@optionalTypeArgs TResult when<TResult extends Object?>(TResult Function( TimestampDuration fastest,  TimestampDuration average,  TimestampDuration slowest,  TimestampDuration tm90,  TimestampDuration tm75,  TimestampDuration p90,  TimestampDuration p75)  $default,) {final _that = this;
switch (_that) {
case _LatencyStats():
return $default(_that.fastest,_that.average,_that.slowest,_that.tm90,_that.tm75,_that.p90,_that.p75);}
}
/// A variant of `when` that fallback to returning `null`
///
/// It is equivalent to doing:
/// ```dart
/// switch (sealedClass) {
///   case Subclass(:final field):
///     return ...;
///   case _:
///     return null;
/// }
/// ```

@optionalTypeArgs TResult? whenOrNull<TResult extends Object?>(TResult? Function( TimestampDuration fastest,  TimestampDuration average,  TimestampDuration slowest,  TimestampDuration tm90,  TimestampDuration tm75,  TimestampDuration p90,  TimestampDuration p75)?  $default,) {final _that = this;
switch (_that) {
case _LatencyStats() when $default != null:
return $default(_that.fastest,_that.average,_that.slowest,_that.tm90,_that.tm75,_that.p90,_that.p75);case _:
  return null;

}
}

}

/// @nodoc
@JsonSerializable()

class _LatencyStats implements LatencyStats {
  const _LatencyStats({required this.fastest, required this.average, required this.slowest, required this.tm90, required this.tm75, required this.p90, required this.p75});
  factory _LatencyStats.fromJson(Map<String, dynamic> json) => _$LatencyStatsFromJson(json);

@override final  TimestampDuration fastest;
@override final  TimestampDuration average;
@override final  TimestampDuration slowest;
@override final  TimestampDuration tm90;
@override final  TimestampDuration tm75;
@override final  TimestampDuration p90;
@override final  TimestampDuration p75;

/// Create a copy of LatencyStats
/// with the given fields replaced by the non-null parameter values.
@override @JsonKey(includeFromJson: false, includeToJson: false)
@pragma('vm:prefer-inline')
_$LatencyStatsCopyWith<_LatencyStats> get copyWith => __$LatencyStatsCopyWithImpl<_LatencyStats>(this, _$identity);

@override
Map<String, dynamic> toJson() {
  return _$LatencyStatsToJson(this, );
}

@override
bool operator ==(Object other) {
  return identical(this, other) || (other.runtimeType == runtimeType&&other is _LatencyStats&&(identical(other.fastest, fastest) || other.fastest == fastest)&&(identical(other.average, average) || other.average == average)&&(identical(other.slowest, slowest) || other.slowest == slowest)&&(identical(other.tm90, tm90) || other.tm90 == tm90)&&(identical(other.tm75, tm75) || other.tm75 == tm75)&&(identical(other.p90, p90) || other.p90 == p90)&&(identical(other.p75, p75) || other.p75 == p75));
}

@JsonKey(includeFromJson: false, includeToJson: false)
@override
int get hashCode => Object.hash(runtimeType,fastest,average,slowest,tm90,tm75,p90,p75);

@override
String toString() {
  return 'LatencyStats(fastest: $fastest, average: $average, slowest: $slowest, tm90: $tm90, tm75: $tm75, p90: $p90, p75: $p75)';
}


}

/// @nodoc
abstract mixin class _$LatencyStatsCopyWith<$Res> implements $LatencyStatsCopyWith<$Res> {
  factory _$LatencyStatsCopyWith(_LatencyStats value, $Res Function(_LatencyStats) _then) = __$LatencyStatsCopyWithImpl;
@override @useResult
$Res call({
 TimestampDuration fastest, TimestampDuration average, TimestampDuration slowest, TimestampDuration tm90, TimestampDuration tm75, TimestampDuration p90, TimestampDuration p75
});




}
/// @nodoc
class __$LatencyStatsCopyWithImpl<$Res>
    implements _$LatencyStatsCopyWith<$Res> {
  __$LatencyStatsCopyWithImpl(this._self, this._then);

  final _LatencyStats _self;
  final $Res Function(_LatencyStats) _then;

/// Create a copy of LatencyStats
/// with the given fields replaced by the non-null parameter values.
@override @pragma('vm:prefer-inline') $Res call({Object? fastest = null,Object? average = null,Object? slowest = null,Object? tm90 = null,Object? tm75 = null,Object? p90 = null,Object? p75 = null,}) {
  return _then(_LatencyStats(
fastest: null == fastest ? _self.fastest : fastest // ignore: cast_nullable_to_non_nullable
as TimestampDuration,average: null == average ? _self.average : average // ignore: cast_nullable_to_non_nullable
as TimestampDuration,slowest: null == slowest ? _self.slowest : slowest // ignore: cast_nullable_to_non_nullable
as TimestampDuration,tm90: null == tm90 ? _self.tm90 : tm90 // ignore: cast_nullable_to_non_nullable
as TimestampDuration,tm75: null == tm75 ? _self.tm75 : tm75 // ignore: cast_nullable_to_non_nullable
as TimestampDuration,p90: null == p90 ? _self.p90 : p90 // ignore: cast_nullable_to_non_nullable
as TimestampDuration,p75: null == p75 ? _self.p75 : p75 // ignore: cast_nullable_to_non_nullable
as TimestampDuration,
  ));
}


}


/// @nodoc
mixin _$TransferStats {

 BigInt get total; BigInt get maximum; BigInt get average; BigInt get minimum;
/// Create a copy of TransferStats
/// with the given fields replaced by the non-null parameter values.
@JsonKey(includeFromJson: false, includeToJson: false)
@pragma('vm:prefer-inline')
$TransferStatsCopyWith<TransferStats> get copyWith => _$TransferStatsCopyWithImpl<TransferStats>(this as TransferStats, _$identity);

  /// Serializes this TransferStats to a JSON map.
  Map<String, dynamic> toJson();


@override
bool operator ==(Object other) {
  return identical(this, other) || (other.runtimeType == runtimeType&&other is TransferStats&&(identical(other.total, total) || other.total == total)&&(identical(other.maximum, maximum) || other.maximum == maximum)&&(identical(other.average, average) || other.average == average)&&(identical(other.minimum, minimum) || other.minimum == minimum));
}

@JsonKey(includeFromJson: false, includeToJson: false)
@override
int get hashCode => Object.hash(runtimeType,total,maximum,average,minimum);

@override
String toString() {
  return 'TransferStats(total: $total, maximum: $maximum, average: $average, minimum: $minimum)';
}


}

/// @nodoc
abstract mixin class $TransferStatsCopyWith<$Res>  {
  factory $TransferStatsCopyWith(TransferStats value, $Res Function(TransferStats) _then) = _$TransferStatsCopyWithImpl;
@useResult
$Res call({
 BigInt total, BigInt maximum, BigInt average, BigInt minimum
});




}
/// @nodoc
class _$TransferStatsCopyWithImpl<$Res>
    implements $TransferStatsCopyWith<$Res> {
  _$TransferStatsCopyWithImpl(this._self, this._then);

  final TransferStats _self;
  final $Res Function(TransferStats) _then;

/// Create a copy of TransferStats
/// with the given fields replaced by the non-null parameter values.
@pragma('vm:prefer-inline') @override $Res call({Object? total = null,Object? maximum = null,Object? average = null,Object? minimum = null,}) {
  return _then(_self.copyWith(
total: null == total ? _self.total : total // ignore: cast_nullable_to_non_nullable
as BigInt,maximum: null == maximum ? _self.maximum : maximum // ignore: cast_nullable_to_non_nullable
as BigInt,average: null == average ? _self.average : average // ignore: cast_nullable_to_non_nullable
as BigInt,minimum: null == minimum ? _self.minimum : minimum // ignore: cast_nullable_to_non_nullable
as BigInt,
  ));
}

}


/// Adds pattern-matching-related methods to [TransferStats].
extension TransferStatsPatterns on TransferStats {
/// A variant of `map` that fallback to returning `orElse`.
///
/// It is equivalent to doing:
/// ```dart
/// switch (sealedClass) {
///   case final Subclass value:
///     return ...;
///   case _:
///     return orElse();
/// }
/// ```

@optionalTypeArgs TResult maybeMap<TResult extends Object?>(TResult Function( _TransferStats value)?  $default,{required TResult orElse(),}){
final _that = this;
switch (_that) {
case _TransferStats() when $default != null:
return $default(_that);case _:
  return orElse();

}
}
/// A `switch`-like method, using callbacks.
///
/// Callbacks receives the raw object, upcasted.
/// It is equivalent to doing:
/// ```dart
/// switch (sealedClass) {
///   case final Subclass value:
///     return ...;
///   case final Subclass2 value:
///     return ...;
/// }
/// ```

@optionalTypeArgs TResult map<TResult extends Object?>(TResult Function( _TransferStats value)  $default,){
final _that = this;
switch (_that) {
case _TransferStats():
return $default(_that);}
}
/// A variant of `map` that fallback to returning `null`.
///
/// It is equivalent to doing:
/// ```dart
/// switch (sealedClass) {
///   case final Subclass value:
///     return ...;
///   case _:
///     return null;
/// }
/// ```

@optionalTypeArgs TResult? mapOrNull<TResult extends Object?>(TResult? Function( _TransferStats value)?  $default,){
final _that = this;
switch (_that) {
case _TransferStats() when $default != null:
return $default(_that);case _:
  return null;

}
}
/// A variant of `when` that fallback to an `orElse` callback.
///
/// It is equivalent to doing:
/// ```dart
/// switch (sealedClass) {
///   case Subclass(:final field):
///     return ...;
///   case _:
///     return orElse();
/// }
/// ```

@optionalTypeArgs TResult maybeWhen<TResult extends Object?>(TResult Function( BigInt total,  BigInt maximum,  BigInt average,  BigInt minimum)?  $default,{required TResult orElse(),}) {final _that = this;
switch (_that) {
case _TransferStats() when $default != null:
return $default(_that.total,_that.maximum,_that.average,_that.minimum);case _:
  return orElse();

}
}
/// A `switch`-like method, using callbacks.
///
/// As opposed to `map`, this offers destructuring.
/// It is equivalent to doing:
/// ```dart
/// switch (sealedClass) {
///   case Subclass(:final field):
///     return ...;
///   case Subclass2(:final field2):
///     return ...;
/// }
/// ```

@optionalTypeArgs TResult when<TResult extends Object?>(TResult Function( BigInt total,  BigInt maximum,  BigInt average,  BigInt minimum)  $default,) {final _that = this;
switch (_that) {
case _TransferStats():
return $default(_that.total,_that.maximum,_that.average,_that.minimum);}
}
/// A variant of `when` that fallback to returning `null`
///
/// It is equivalent to doing:
/// ```dart
/// switch (sealedClass) {
///   case Subclass(:final field):
///     return ...;
///   case _:
///     return null;
/// }
/// ```

@optionalTypeArgs TResult? whenOrNull<TResult extends Object?>(TResult? Function( BigInt total,  BigInt maximum,  BigInt average,  BigInt minimum)?  $default,) {final _that = this;
switch (_that) {
case _TransferStats() when $default != null:
return $default(_that.total,_that.maximum,_that.average,_that.minimum);case _:
  return null;

}
}

}

/// @nodoc
@JsonSerializable()

class _TransferStats implements TransferStats {
  const _TransferStats({required this.total, required this.maximum, required this.average, required this.minimum});
  factory _TransferStats.fromJson(Map<String, dynamic> json) => _$TransferStatsFromJson(json);

@override final  BigInt total;
@override final  BigInt maximum;
@override final  BigInt average;
@override final  BigInt minimum;

/// Create a copy of TransferStats
/// with the given fields replaced by the non-null parameter values.
@override @JsonKey(includeFromJson: false, includeToJson: false)
@pragma('vm:prefer-inline')
_$TransferStatsCopyWith<_TransferStats> get copyWith => __$TransferStatsCopyWithImpl<_TransferStats>(this, _$identity);

@override
Map<String, dynamic> toJson() {
  return _$TransferStatsToJson(this, );
}

@override
bool operator ==(Object other) {
  return identical(this, other) || (other.runtimeType == runtimeType&&other is _TransferStats&&(identical(other.total, total) || other.total == total)&&(identical(other.maximum, maximum) || other.maximum == maximum)&&(identical(other.average, average) || other.average == average)&&(identical(other.minimum, minimum) || other.minimum == minimum));
}

@JsonKey(includeFromJson: false, includeToJson: false)
@override
int get hashCode => Object.hash(runtimeType,total,maximum,average,minimum);

@override
String toString() {
  return 'TransferStats(total: $total, maximum: $maximum, average: $average, minimum: $minimum)';
}


}

/// @nodoc
abstract mixin class _$TransferStatsCopyWith<$Res> implements $TransferStatsCopyWith<$Res> {
  factory _$TransferStatsCopyWith(_TransferStats value, $Res Function(_TransferStats) _then) = __$TransferStatsCopyWithImpl;
@override @useResult
$Res call({
 BigInt total, BigInt maximum, BigInt average, BigInt minimum
});




}
/// @nodoc
class __$TransferStatsCopyWithImpl<$Res>
    implements _$TransferStatsCopyWith<$Res> {
  __$TransferStatsCopyWithImpl(this._self, this._then);

  final _TransferStats _self;
  final $Res Function(_TransferStats) _then;

/// Create a copy of TransferStats
/// with the given fields replaced by the non-null parameter values.
@override @pragma('vm:prefer-inline') $Res call({Object? total = null,Object? maximum = null,Object? average = null,Object? minimum = null,}) {
  return _then(_TransferStats(
total: null == total ? _self.total : total // ignore: cast_nullable_to_non_nullable
as BigInt,maximum: null == maximum ? _self.maximum : maximum // ignore: cast_nullable_to_non_nullable
as BigInt,average: null == average ? _self.average : average // ignore: cast_nullable_to_non_nullable
as BigInt,minimum: null == minimum ? _self.minimum : minimum // ignore: cast_nullable_to_non_nullable
as BigInt,
  ));
}


}


/// @nodoc
mixin _$TransferStatsDownUp {

 TransferStats get down; TransferStats get up;
/// Create a copy of TransferStatsDownUp
/// with the given fields replaced by the non-null parameter values.
@JsonKey(includeFromJson: false, includeToJson: false)
@pragma('vm:prefer-inline')
$TransferStatsDownUpCopyWith<TransferStatsDownUp> get copyWith => _$TransferStatsDownUpCopyWithImpl<TransferStatsDownUp>(this as TransferStatsDownUp, _$identity);

  /// Serializes this TransferStatsDownUp to a JSON map.
  Map<String, dynamic> toJson();


@override
bool operator ==(Object other) {
  return identical(this, other) || (other.runtimeType == runtimeType&&other is TransferStatsDownUp&&(identical(other.down, down) || other.down == down)&&(identical(other.up, up) || other.up == up));
}

@JsonKey(includeFromJson: false, includeToJson: false)
@override
int get hashCode => Object.hash(runtimeType,down,up);

@override
String toString() {
  return 'TransferStatsDownUp(down: $down, up: $up)';
}


}

/// @nodoc
abstract mixin class $TransferStatsDownUpCopyWith<$Res>  {
  factory $TransferStatsDownUpCopyWith(TransferStatsDownUp value, $Res Function(TransferStatsDownUp) _then) = _$TransferStatsDownUpCopyWithImpl;
@useResult
$Res call({
 TransferStats down, TransferStats up
});


$TransferStatsCopyWith<$Res> get down;$TransferStatsCopyWith<$Res> get up;

}
/// @nodoc
class _$TransferStatsDownUpCopyWithImpl<$Res>
    implements $TransferStatsDownUpCopyWith<$Res> {
  _$TransferStatsDownUpCopyWithImpl(this._self, this._then);

  final TransferStatsDownUp _self;
  final $Res Function(TransferStatsDownUp) _then;

/// Create a copy of TransferStatsDownUp
/// with the given fields replaced by the non-null parameter values.
@pragma('vm:prefer-inline') @override $Res call({Object? down = null,Object? up = null,}) {
  return _then(_self.copyWith(
down: null == down ? _self.down : down // ignore: cast_nullable_to_non_nullable
as TransferStats,up: null == up ? _self.up : up // ignore: cast_nullable_to_non_nullable
as TransferStats,
  ));
}
/// Create a copy of TransferStatsDownUp
/// with the given fields replaced by the non-null parameter values.
@override
@pragma('vm:prefer-inline')
$TransferStatsCopyWith<$Res> get down {
  
  return $TransferStatsCopyWith<$Res>(_self.down, (value) {
    return _then(_self.copyWith(down: value));
  });
}/// Create a copy of TransferStatsDownUp
/// with the given fields replaced by the non-null parameter values.
@override
@pragma('vm:prefer-inline')
$TransferStatsCopyWith<$Res> get up {
  
  return $TransferStatsCopyWith<$Res>(_self.up, (value) {
    return _then(_self.copyWith(up: value));
  });
}
}


/// Adds pattern-matching-related methods to [TransferStatsDownUp].
extension TransferStatsDownUpPatterns on TransferStatsDownUp {
/// A variant of `map` that fallback to returning `orElse`.
///
/// It is equivalent to doing:
/// ```dart
/// switch (sealedClass) {
///   case final Subclass value:
///     return ...;
///   case _:
///     return orElse();
/// }
/// ```

@optionalTypeArgs TResult maybeMap<TResult extends Object?>(TResult Function( _TransferStatsDownUp value)?  $default,{required TResult orElse(),}){
final _that = this;
switch (_that) {
case _TransferStatsDownUp() when $default != null:
return $default(_that);case _:
  return orElse();

}
}
/// A `switch`-like method, using callbacks.
///
/// Callbacks receives the raw object, upcasted.
/// It is equivalent to doing:
/// ```dart
/// switch (sealedClass) {
///   case final Subclass value:
///     return ...;
///   case final Subclass2 value:
///     return ...;
/// }
/// ```

@optionalTypeArgs TResult map<TResult extends Object?>(TResult Function( _TransferStatsDownUp value)  $default,){
final _that = this;
switch (_that) {
case _TransferStatsDownUp():
return $default(_that);}
}
/// A variant of `map` that fallback to returning `null`.
///
/// It is equivalent to doing:
/// ```dart
/// switch (sealedClass) {
///   case final Subclass value:
///     return ...;
///   case _:
///     return null;
/// }
/// ```

@optionalTypeArgs TResult? mapOrNull<TResult extends Object?>(TResult? Function( _TransferStatsDownUp value)?  $default,){
final _that = this;
switch (_that) {
case _TransferStatsDownUp() when $default != null:
return $default(_that);case _:
  return null;

}
}
/// A variant of `when` that fallback to an `orElse` callback.
///
/// It is equivalent to doing:
/// ```dart
/// switch (sealedClass) {
///   case Subclass(:final field):
///     return ...;
///   case _:
///     return orElse();
/// }
/// ```

@optionalTypeArgs TResult maybeWhen<TResult extends Object?>(TResult Function( TransferStats down,  TransferStats up)?  $default,{required TResult orElse(),}) {final _that = this;
switch (_that) {
case _TransferStatsDownUp() when $default != null:
return $default(_that.down,_that.up);case _:
  return orElse();

}
}
/// A `switch`-like method, using callbacks.
///
/// As opposed to `map`, this offers destructuring.
/// It is equivalent to doing:
/// ```dart
/// switch (sealedClass) {
///   case Subclass(:final field):
///     return ...;
///   case Subclass2(:final field2):
///     return ...;
/// }
/// ```

@optionalTypeArgs TResult when<TResult extends Object?>(TResult Function( TransferStats down,  TransferStats up)  $default,) {final _that = this;
switch (_that) {
case _TransferStatsDownUp():
return $default(_that.down,_that.up);}
}
/// A variant of `when` that fallback to returning `null`
///
/// It is equivalent to doing:
/// ```dart
/// switch (sealedClass) {
///   case Subclass(:final field):
///     return ...;
///   case _:
///     return null;
/// }
/// ```

@optionalTypeArgs TResult? whenOrNull<TResult extends Object?>(TResult? Function( TransferStats down,  TransferStats up)?  $default,) {final _that = this;
switch (_that) {
case _TransferStatsDownUp() when $default != null:
return $default(_that.down,_that.up);case _:
  return null;

}
}

}

/// @nodoc
@JsonSerializable()

class _TransferStatsDownUp implements TransferStatsDownUp {
  const _TransferStatsDownUp({required this.down, required this.up});
  factory _TransferStatsDownUp.fromJson(Map<String, dynamic> json) => _$TransferStatsDownUpFromJson(json);

@override final  TransferStats down;
@override final  TransferStats up;

/// Create a copy of TransferStatsDownUp
/// with the given fields replaced by the non-null parameter values.
@override @JsonKey(includeFromJson: false, includeToJson: false)
@pragma('vm:prefer-inline')
_$TransferStatsDownUpCopyWith<_TransferStatsDownUp> get copyWith => __$TransferStatsDownUpCopyWithImpl<_TransferStatsDownUp>(this, _$identity);

@override
Map<String, dynamic> toJson() {
  return _$TransferStatsDownUpToJson(this, );
}

@override
bool operator ==(Object other) {
  return identical(this, other) || (other.runtimeType == runtimeType&&other is _TransferStatsDownUp&&(identical(other.down, down) || other.down == down)&&(identical(other.up, up) || other.up == up));
}

@JsonKey(includeFromJson: false, includeToJson: false)
@override
int get hashCode => Object.hash(runtimeType,down,up);

@override
String toString() {
  return 'TransferStatsDownUp(down: $down, up: $up)';
}


}

/// @nodoc
abstract mixin class _$TransferStatsDownUpCopyWith<$Res> implements $TransferStatsDownUpCopyWith<$Res> {
  factory _$TransferStatsDownUpCopyWith(_TransferStatsDownUp value, $Res Function(_TransferStatsDownUp) _then) = __$TransferStatsDownUpCopyWithImpl;
@override @useResult
$Res call({
 TransferStats down, TransferStats up
});


@override $TransferStatsCopyWith<$Res> get down;@override $TransferStatsCopyWith<$Res> get up;

}
/// @nodoc
class __$TransferStatsDownUpCopyWithImpl<$Res>
    implements _$TransferStatsDownUpCopyWith<$Res> {
  __$TransferStatsDownUpCopyWithImpl(this._self, this._then);

  final _TransferStatsDownUp _self;
  final $Res Function(_TransferStatsDownUp) _then;

/// Create a copy of TransferStatsDownUp
/// with the given fields replaced by the non-null parameter values.
@override @pragma('vm:prefer-inline') $Res call({Object? down = null,Object? up = null,}) {
  return _then(_TransferStatsDownUp(
down: null == down ? _self.down : down // ignore: cast_nullable_to_non_nullable
as TransferStats,up: null == up ? _self.up : up // ignore: cast_nullable_to_non_nullable
as TransferStats,
  ));
}

/// Create a copy of TransferStatsDownUp
/// with the given fields replaced by the non-null parameter values.
@override
@pragma('vm:prefer-inline')
$TransferStatsCopyWith<$Res> get down {
  
  return $TransferStatsCopyWith<$Res>(_self.down, (value) {
    return _then(_self.copyWith(down: value));
  });
}/// Create a copy of TransferStatsDownUp
/// with the given fields replaced by the non-null parameter values.
@override
@pragma('vm:prefer-inline')
$TransferStatsCopyWith<$Res> get up {
  
  return $TransferStatsCopyWith<$Res>(_self.up, (value) {
    return _then(_self.copyWith(up: value));
  });
}
}


/// @nodoc
mixin _$PeerStats {

 TransferStatsDownUp get transfer; LatencyStats? get latency;
/// Create a copy of PeerStats
/// with the given fields replaced by the non-null parameter values.
@JsonKey(includeFromJson: false, includeToJson: false)
@pragma('vm:prefer-inline')
$PeerStatsCopyWith<PeerStats> get copyWith => _$PeerStatsCopyWithImpl<PeerStats>(this as PeerStats, _$identity);

  /// Serializes this PeerStats to a JSON map.
  Map<String, dynamic> toJson();


@override
bool operator ==(Object other) {
  return identical(this, other) || (other.runtimeType == runtimeType&&other is PeerStats&&(identical(other.transfer, transfer) || other.transfer == transfer)&&(identical(other.latency, latency) || other.latency == latency));
}

@JsonKey(includeFromJson: false, includeToJson: false)
@override
int get hashCode => Object.hash(runtimeType,transfer,latency);

@override
String toString() {
  return 'PeerStats(transfer: $transfer, latency: $latency)';
}


}

/// @nodoc
abstract mixin class $PeerStatsCopyWith<$Res>  {
  factory $PeerStatsCopyWith(PeerStats value, $Res Function(PeerStats) _then) = _$PeerStatsCopyWithImpl;
@useResult
$Res call({
 TransferStatsDownUp transfer, LatencyStats? latency
});


$TransferStatsDownUpCopyWith<$Res> get transfer;$LatencyStatsCopyWith<$Res>? get latency;

}
/// @nodoc
class _$PeerStatsCopyWithImpl<$Res>
    implements $PeerStatsCopyWith<$Res> {
  _$PeerStatsCopyWithImpl(this._self, this._then);

  final PeerStats _self;
  final $Res Function(PeerStats) _then;

/// Create a copy of PeerStats
/// with the given fields replaced by the non-null parameter values.
@pragma('vm:prefer-inline') @override $Res call({Object? transfer = null,Object? latency = freezed,}) {
  return _then(_self.copyWith(
transfer: null == transfer ? _self.transfer : transfer // ignore: cast_nullable_to_non_nullable
as TransferStatsDownUp,latency: freezed == latency ? _self.latency : latency // ignore: cast_nullable_to_non_nullable
as LatencyStats?,
  ));
}
/// Create a copy of PeerStats
/// with the given fields replaced by the non-null parameter values.
@override
@pragma('vm:prefer-inline')
$TransferStatsDownUpCopyWith<$Res> get transfer {
  
  return $TransferStatsDownUpCopyWith<$Res>(_self.transfer, (value) {
    return _then(_self.copyWith(transfer: value));
  });
}/// Create a copy of PeerStats
/// with the given fields replaced by the non-null parameter values.
@override
@pragma('vm:prefer-inline')
$LatencyStatsCopyWith<$Res>? get latency {
    if (_self.latency == null) {
    return null;
  }

  return $LatencyStatsCopyWith<$Res>(_self.latency!, (value) {
    return _then(_self.copyWith(latency: value));
  });
}
}


/// Adds pattern-matching-related methods to [PeerStats].
extension PeerStatsPatterns on PeerStats {
/// A variant of `map` that fallback to returning `orElse`.
///
/// It is equivalent to doing:
/// ```dart
/// switch (sealedClass) {
///   case final Subclass value:
///     return ...;
///   case _:
///     return orElse();
/// }
/// ```

@optionalTypeArgs TResult maybeMap<TResult extends Object?>(TResult Function( _PeerStats value)?  $default,{required TResult orElse(),}){
final _that = this;
switch (_that) {
case _PeerStats() when $default != null:
return $default(_that);case _:
  return orElse();

}
}
/// A `switch`-like method, using callbacks.
///
/// Callbacks receives the raw object, upcasted.
/// It is equivalent to doing:
/// ```dart
/// switch (sealedClass) {
///   case final Subclass value:
///     return ...;
///   case final Subclass2 value:
///     return ...;
/// }
/// ```

@optionalTypeArgs TResult map<TResult extends Object?>(TResult Function( _PeerStats value)  $default,){
final _that = this;
switch (_that) {
case _PeerStats():
return $default(_that);}
}
/// A variant of `map` that fallback to returning `null`.
///
/// It is equivalent to doing:
/// ```dart
/// switch (sealedClass) {
///   case final Subclass value:
///     return ...;
///   case _:
///     return null;
/// }
/// ```

@optionalTypeArgs TResult? mapOrNull<TResult extends Object?>(TResult? Function( _PeerStats value)?  $default,){
final _that = this;
switch (_that) {
case _PeerStats() when $default != null:
return $default(_that);case _:
  return null;

}
}
/// A variant of `when` that fallback to an `orElse` callback.
///
/// It is equivalent to doing:
/// ```dart
/// switch (sealedClass) {
///   case Subclass(:final field):
///     return ...;
///   case _:
///     return orElse();
/// }
/// ```

@optionalTypeArgs TResult maybeWhen<TResult extends Object?>(TResult Function( TransferStatsDownUp transfer,  LatencyStats? latency)?  $default,{required TResult orElse(),}) {final _that = this;
switch (_that) {
case _PeerStats() when $default != null:
return $default(_that.transfer,_that.latency);case _:
  return orElse();

}
}
/// A `switch`-like method, using callbacks.
///
/// As opposed to `map`, this offers destructuring.
/// It is equivalent to doing:
/// ```dart
/// switch (sealedClass) {
///   case Subclass(:final field):
///     return ...;
///   case Subclass2(:final field2):
///     return ...;
/// }
/// ```

@optionalTypeArgs TResult when<TResult extends Object?>(TResult Function( TransferStatsDownUp transfer,  LatencyStats? latency)  $default,) {final _that = this;
switch (_that) {
case _PeerStats():
return $default(_that.transfer,_that.latency);}
}
/// A variant of `when` that fallback to returning `null`
///
/// It is equivalent to doing:
/// ```dart
/// switch (sealedClass) {
///   case Subclass(:final field):
///     return ...;
///   case _:
///     return null;
/// }
/// ```

@optionalTypeArgs TResult? whenOrNull<TResult extends Object?>(TResult? Function( TransferStatsDownUp transfer,  LatencyStats? latency)?  $default,) {final _that = this;
switch (_that) {
case _PeerStats() when $default != null:
return $default(_that.transfer,_that.latency);case _:
  return null;

}
}

}

/// @nodoc
@JsonSerializable()

class _PeerStats implements PeerStats {
  const _PeerStats({required this.transfer, this.latency});
  factory _PeerStats.fromJson(Map<String, dynamic> json) => _$PeerStatsFromJson(json);

@override final  TransferStatsDownUp transfer;
@override final  LatencyStats? latency;

/// Create a copy of PeerStats
/// with the given fields replaced by the non-null parameter values.
@override @JsonKey(includeFromJson: false, includeToJson: false)
@pragma('vm:prefer-inline')
_$PeerStatsCopyWith<_PeerStats> get copyWith => __$PeerStatsCopyWithImpl<_PeerStats>(this, _$identity);

@override
Map<String, dynamic> toJson() {
  return _$PeerStatsToJson(this, );
}

@override
bool operator ==(Object other) {
  return identical(this, other) || (other.runtimeType == runtimeType&&other is _PeerStats&&(identical(other.transfer, transfer) || other.transfer == transfer)&&(identical(other.latency, latency) || other.latency == latency));
}

@JsonKey(includeFromJson: false, includeToJson: false)
@override
int get hashCode => Object.hash(runtimeType,transfer,latency);

@override
String toString() {
  return 'PeerStats(transfer: $transfer, latency: $latency)';
}


}

/// @nodoc
abstract mixin class _$PeerStatsCopyWith<$Res> implements $PeerStatsCopyWith<$Res> {
  factory _$PeerStatsCopyWith(_PeerStats value, $Res Function(_PeerStats) _then) = __$PeerStatsCopyWithImpl;
@override @useResult
$Res call({
 TransferStatsDownUp transfer, LatencyStats? latency
});


@override $TransferStatsDownUpCopyWith<$Res> get transfer;@override $LatencyStatsCopyWith<$Res>? get latency;

}
/// @nodoc
class __$PeerStatsCopyWithImpl<$Res>
    implements _$PeerStatsCopyWith<$Res> {
  __$PeerStatsCopyWithImpl(this._self, this._then);

  final _PeerStats _self;
  final $Res Function(_PeerStats) _then;

/// Create a copy of PeerStats
/// with the given fields replaced by the non-null parameter values.
@override @pragma('vm:prefer-inline') $Res call({Object? transfer = null,Object? latency = freezed,}) {
  return _then(_PeerStats(
transfer: null == transfer ? _self.transfer : transfer // ignore: cast_nullable_to_non_nullable
as TransferStatsDownUp,latency: freezed == latency ? _self.latency : latency // ignore: cast_nullable_to_non_nullable
as LatencyStats?,
  ));
}

/// Create a copy of PeerStats
/// with the given fields replaced by the non-null parameter values.
@override
@pragma('vm:prefer-inline')
$TransferStatsDownUpCopyWith<$Res> get transfer {
  
  return $TransferStatsDownUpCopyWith<$Res>(_self.transfer, (value) {
    return _then(_self.copyWith(transfer: value));
  });
}/// Create a copy of PeerStats
/// with the given fields replaced by the non-null parameter values.
@override
@pragma('vm:prefer-inline')
$LatencyStatsCopyWith<$Res>? get latency {
    if (_self.latency == null) {
    return null;
  }

  return $LatencyStatsCopyWith<$Res>(_self.latency!, (value) {
    return _then(_self.copyWith(latency: value));
  });
}
}


/// @nodoc
mixin _$PeerTableData {

 List<NodeId> get nodeIds; String get peerAddress; PeerStats get peerStats;
/// Create a copy of PeerTableData
/// with the given fields replaced by the non-null parameter values.
@JsonKey(includeFromJson: false, includeToJson: false)
@pragma('vm:prefer-inline')
$PeerTableDataCopyWith<PeerTableData> get copyWith => _$PeerTableDataCopyWithImpl<PeerTableData>(this as PeerTableData, _$identity);

  /// Serializes this PeerTableData to a JSON map.
  Map<String, dynamic> toJson();


@override
bool operator ==(Object other) {
  return identical(this, other) || (other.runtimeType == runtimeType&&other is PeerTableData&&const DeepCollectionEquality().equals(other.nodeIds, nodeIds)&&(identical(other.peerAddress, peerAddress) || other.peerAddress == peerAddress)&&(identical(other.peerStats, peerStats) || other.peerStats == peerStats));
}

@JsonKey(includeFromJson: false, includeToJson: false)
@override
int get hashCode => Object.hash(runtimeType,const DeepCollectionEquality().hash(nodeIds),peerAddress,peerStats);

@override
String toString() {
  return 'PeerTableData(nodeIds: $nodeIds, peerAddress: $peerAddress, peerStats: $peerStats)';
}


}

/// @nodoc
abstract mixin class $PeerTableDataCopyWith<$Res>  {
  factory $PeerTableDataCopyWith(PeerTableData value, $Res Function(PeerTableData) _then) = _$PeerTableDataCopyWithImpl;
@useResult
$Res call({
 List<NodeId> nodeIds, String peerAddress, PeerStats peerStats
});


$PeerStatsCopyWith<$Res> get peerStats;

}
/// @nodoc
class _$PeerTableDataCopyWithImpl<$Res>
    implements $PeerTableDataCopyWith<$Res> {
  _$PeerTableDataCopyWithImpl(this._self, this._then);

  final PeerTableData _self;
  final $Res Function(PeerTableData) _then;

/// Create a copy of PeerTableData
/// with the given fields replaced by the non-null parameter values.
@pragma('vm:prefer-inline') @override $Res call({Object? nodeIds = null,Object? peerAddress = null,Object? peerStats = null,}) {
  return _then(_self.copyWith(
nodeIds: null == nodeIds ? _self.nodeIds : nodeIds // ignore: cast_nullable_to_non_nullable
as List<NodeId>,peerAddress: null == peerAddress ? _self.peerAddress : peerAddress // ignore: cast_nullable_to_non_nullable
as String,peerStats: null == peerStats ? _self.peerStats : peerStats // ignore: cast_nullable_to_non_nullable
as PeerStats,
  ));
}
/// Create a copy of PeerTableData
/// with the given fields replaced by the non-null parameter values.
@override
@pragma('vm:prefer-inline')
$PeerStatsCopyWith<$Res> get peerStats {
  
  return $PeerStatsCopyWith<$Res>(_self.peerStats, (value) {
    return _then(_self.copyWith(peerStats: value));
  });
}
}


/// Adds pattern-matching-related methods to [PeerTableData].
extension PeerTableDataPatterns on PeerTableData {
/// A variant of `map` that fallback to returning `orElse`.
///
/// It is equivalent to doing:
/// ```dart
/// switch (sealedClass) {
///   case final Subclass value:
///     return ...;
///   case _:
///     return orElse();
/// }
/// ```

@optionalTypeArgs TResult maybeMap<TResult extends Object?>(TResult Function( _PeerTableData value)?  $default,{required TResult orElse(),}){
final _that = this;
switch (_that) {
case _PeerTableData() when $default != null:
return $default(_that);case _:
  return orElse();

}
}
/// A `switch`-like method, using callbacks.
///
/// Callbacks receives the raw object, upcasted.
/// It is equivalent to doing:
/// ```dart
/// switch (sealedClass) {
///   case final Subclass value:
///     return ...;
///   case final Subclass2 value:
///     return ...;
/// }
/// ```

@optionalTypeArgs TResult map<TResult extends Object?>(TResult Function( _PeerTableData value)  $default,){
final _that = this;
switch (_that) {
case _PeerTableData():
return $default(_that);}
}
/// A variant of `map` that fallback to returning `null`.
///
/// It is equivalent to doing:
/// ```dart
/// switch (sealedClass) {
///   case final Subclass value:
///     return ...;
///   case _:
///     return null;
/// }
/// ```

@optionalTypeArgs TResult? mapOrNull<TResult extends Object?>(TResult? Function( _PeerTableData value)?  $default,){
final _that = this;
switch (_that) {
case _PeerTableData() when $default != null:
return $default(_that);case _:
  return null;

}
}
/// A variant of `when` that fallback to an `orElse` callback.
///
/// It is equivalent to doing:
/// ```dart
/// switch (sealedClass) {
///   case Subclass(:final field):
///     return ...;
///   case _:
///     return orElse();
/// }
/// ```

@optionalTypeArgs TResult maybeWhen<TResult extends Object?>(TResult Function( List<NodeId> nodeIds,  String peerAddress,  PeerStats peerStats)?  $default,{required TResult orElse(),}) {final _that = this;
switch (_that) {
case _PeerTableData() when $default != null:
return $default(_that.nodeIds,_that.peerAddress,_that.peerStats);case _:
  return orElse();

}
}
/// A `switch`-like method, using callbacks.
///
/// As opposed to `map`, this offers destructuring.
/// It is equivalent to doing:
/// ```dart
/// switch (sealedClass) {
///   case Subclass(:final field):
///     return ...;
///   case Subclass2(:final field2):
///     return ...;
/// }
/// ```

@optionalTypeArgs TResult when<TResult extends Object?>(TResult Function( List<NodeId> nodeIds,  String peerAddress,  PeerStats peerStats)  $default,) {final _that = this;
switch (_that) {
case _PeerTableData():
return $default(_that.nodeIds,_that.peerAddress,_that.peerStats);}
}
/// A variant of `when` that fallback to returning `null`
///
/// It is equivalent to doing:
/// ```dart
/// switch (sealedClass) {
///   case Subclass(:final field):
///     return ...;
///   case _:
///     return null;
/// }
/// ```

@optionalTypeArgs TResult? whenOrNull<TResult extends Object?>(TResult? Function( List<NodeId> nodeIds,  String peerAddress,  PeerStats peerStats)?  $default,) {final _that = this;
switch (_that) {
case _PeerTableData() when $default != null:
return $default(_that.nodeIds,_that.peerAddress,_that.peerStats);case _:
  return null;

}
}

}

/// @nodoc
@JsonSerializable()

class _PeerTableData implements PeerTableData {
  const _PeerTableData({required final  List<NodeId> nodeIds, required this.peerAddress, required this.peerStats}): _nodeIds = nodeIds;
  factory _PeerTableData.fromJson(Map<String, dynamic> json) => _$PeerTableDataFromJson(json);

 final  List<NodeId> _nodeIds;
@override List<NodeId> get nodeIds {
  if (_nodeIds is EqualUnmodifiableListView) return _nodeIds;
  // ignore: implicit_dynamic_type
  return EqualUnmodifiableListView(_nodeIds);
}

@override final  String peerAddress;
@override final  PeerStats peerStats;

/// Create a copy of PeerTableData
/// with the given fields replaced by the non-null parameter values.
@override @JsonKey(includeFromJson: false, includeToJson: false)
@pragma('vm:prefer-inline')
_$PeerTableDataCopyWith<_PeerTableData> get copyWith => __$PeerTableDataCopyWithImpl<_PeerTableData>(this, _$identity);

@override
Map<String, dynamic> toJson() {
  return _$PeerTableDataToJson(this, );
}

@override
bool operator ==(Object other) {
  return identical(this, other) || (other.runtimeType == runtimeType&&other is _PeerTableData&&const DeepCollectionEquality().equals(other._nodeIds, _nodeIds)&&(identical(other.peerAddress, peerAddress) || other.peerAddress == peerAddress)&&(identical(other.peerStats, peerStats) || other.peerStats == peerStats));
}

@JsonKey(includeFromJson: false, includeToJson: false)
@override
int get hashCode => Object.hash(runtimeType,const DeepCollectionEquality().hash(_nodeIds),peerAddress,peerStats);

@override
String toString() {
  return 'PeerTableData(nodeIds: $nodeIds, peerAddress: $peerAddress, peerStats: $peerStats)';
}


}

/// @nodoc
abstract mixin class _$PeerTableDataCopyWith<$Res> implements $PeerTableDataCopyWith<$Res> {
  factory _$PeerTableDataCopyWith(_PeerTableData value, $Res Function(_PeerTableData) _then) = __$PeerTableDataCopyWithImpl;
@override @useResult
$Res call({
 List<NodeId> nodeIds, String peerAddress, PeerStats peerStats
});


@override $PeerStatsCopyWith<$Res> get peerStats;

}
/// @nodoc
class __$PeerTableDataCopyWithImpl<$Res>
    implements _$PeerTableDataCopyWith<$Res> {
  __$PeerTableDataCopyWithImpl(this._self, this._then);

  final _PeerTableData _self;
  final $Res Function(_PeerTableData) _then;

/// Create a copy of PeerTableData
/// with the given fields replaced by the non-null parameter values.
@override @pragma('vm:prefer-inline') $Res call({Object? nodeIds = null,Object? peerAddress = null,Object? peerStats = null,}) {
  return _then(_PeerTableData(
nodeIds: null == nodeIds ? _self._nodeIds : nodeIds // ignore: cast_nullable_to_non_nullable
as List<NodeId>,peerAddress: null == peerAddress ? _self.peerAddress : peerAddress // ignore: cast_nullable_to_non_nullable
as String,peerStats: null == peerStats ? _self.peerStats : peerStats // ignore: cast_nullable_to_non_nullable
as PeerStats,
  ));
}

/// Create a copy of PeerTableData
/// with the given fields replaced by the non-null parameter values.
@override
@pragma('vm:prefer-inline')
$PeerStatsCopyWith<$Res> get peerStats {
  
  return $PeerStatsCopyWith<$Res>(_self.peerStats, (value) {
    return _then(_self.copyWith(peerStats: value));
  });
}
}

VeilidUpdate _$VeilidUpdateFromJson(
  Map<String, dynamic> json
) {
        switch (json['kind']) {
                  case 'Log':
          return VeilidLog.fromJson(
            json
          );
                case 'AppMessage':
          return VeilidAppMessage.fromJson(
            json
          );
                case 'AppCall':
          return VeilidAppCall.fromJson(
            json
          );
                case 'Attachment':
          return VeilidUpdateAttachment.fromJson(
            json
          );
                case 'Network':
          return VeilidUpdateNetwork.fromJson(
            json
          );
                case 'Config':
          return VeilidUpdateConfig.fromJson(
            json
          );
                case 'RouteChange':
          return VeilidUpdateRouteChange.fromJson(
            json
          );
                case 'ValueChange':
          return VeilidUpdateValueChange.fromJson(
            json
          );
        
          default:
            throw CheckedFromJsonException(
  json,
  'kind',
  'VeilidUpdate',
  'Invalid union type "${json['kind']}"!'
);
        }
      
}

/// @nodoc
mixin _$VeilidUpdate {



  /// Serializes this VeilidUpdate to a JSON map.
  Map<String, dynamic> toJson();


@override
bool operator ==(Object other) {
  return identical(this, other) || (other.runtimeType == runtimeType&&other is VeilidUpdate);
}

@JsonKey(includeFromJson: false, includeToJson: false)
@override
int get hashCode => runtimeType.hashCode;

@override
String toString() {
  return 'VeilidUpdate()';
}


}

/// @nodoc
class $VeilidUpdateCopyWith<$Res>  {
$VeilidUpdateCopyWith(VeilidUpdate _, $Res Function(VeilidUpdate) __);
}


/// Adds pattern-matching-related methods to [VeilidUpdate].
extension VeilidUpdatePatterns on VeilidUpdate {
/// A variant of `map` that fallback to returning `orElse`.
///
/// It is equivalent to doing:
/// ```dart
/// switch (sealedClass) {
///   case final Subclass value:
///     return ...;
///   case _:
///     return orElse();
/// }
/// ```

@optionalTypeArgs TResult maybeMap<TResult extends Object?>({TResult Function( VeilidLog value)?  log,TResult Function( VeilidAppMessage value)?  appMessage,TResult Function( VeilidAppCall value)?  appCall,TResult Function( VeilidUpdateAttachment value)?  attachment,TResult Function( VeilidUpdateNetwork value)?  network,TResult Function( VeilidUpdateConfig value)?  config,TResult Function( VeilidUpdateRouteChange value)?  routeChange,TResult Function( VeilidUpdateValueChange value)?  valueChange,required TResult orElse(),}){
final _that = this;
switch (_that) {
case VeilidLog() when log != null:
return log(_that);case VeilidAppMessage() when appMessage != null:
return appMessage(_that);case VeilidAppCall() when appCall != null:
return appCall(_that);case VeilidUpdateAttachment() when attachment != null:
return attachment(_that);case VeilidUpdateNetwork() when network != null:
return network(_that);case VeilidUpdateConfig() when config != null:
return config(_that);case VeilidUpdateRouteChange() when routeChange != null:
return routeChange(_that);case VeilidUpdateValueChange() when valueChange != null:
return valueChange(_that);case _:
  return orElse();

}
}
/// A `switch`-like method, using callbacks.
///
/// Callbacks receives the raw object, upcasted.
/// It is equivalent to doing:
/// ```dart
/// switch (sealedClass) {
///   case final Subclass value:
///     return ...;
///   case final Subclass2 value:
///     return ...;
/// }
/// ```

@optionalTypeArgs TResult map<TResult extends Object?>({required TResult Function( VeilidLog value)  log,required TResult Function( VeilidAppMessage value)  appMessage,required TResult Function( VeilidAppCall value)  appCall,required TResult Function( VeilidUpdateAttachment value)  attachment,required TResult Function( VeilidUpdateNetwork value)  network,required TResult Function( VeilidUpdateConfig value)  config,required TResult Function( VeilidUpdateRouteChange value)  routeChange,required TResult Function( VeilidUpdateValueChange value)  valueChange,}){
final _that = this;
switch (_that) {
case VeilidLog():
return log(_that);case VeilidAppMessage():
return appMessage(_that);case VeilidAppCall():
return appCall(_that);case VeilidUpdateAttachment():
return attachment(_that);case VeilidUpdateNetwork():
return network(_that);case VeilidUpdateConfig():
return config(_that);case VeilidUpdateRouteChange():
return routeChange(_that);case VeilidUpdateValueChange():
return valueChange(_that);}
}
/// A variant of `map` that fallback to returning `null`.
///
/// It is equivalent to doing:
/// ```dart
/// switch (sealedClass) {
///   case final Subclass value:
///     return ...;
///   case _:
///     return null;
/// }
/// ```

@optionalTypeArgs TResult? mapOrNull<TResult extends Object?>({TResult? Function( VeilidLog value)?  log,TResult? Function( VeilidAppMessage value)?  appMessage,TResult? Function( VeilidAppCall value)?  appCall,TResult? Function( VeilidUpdateAttachment value)?  attachment,TResult? Function( VeilidUpdateNetwork value)?  network,TResult? Function( VeilidUpdateConfig value)?  config,TResult? Function( VeilidUpdateRouteChange value)?  routeChange,TResult? Function( VeilidUpdateValueChange value)?  valueChange,}){
final _that = this;
switch (_that) {
case VeilidLog() when log != null:
return log(_that);case VeilidAppMessage() when appMessage != null:
return appMessage(_that);case VeilidAppCall() when appCall != null:
return appCall(_that);case VeilidUpdateAttachment() when attachment != null:
return attachment(_that);case VeilidUpdateNetwork() when network != null:
return network(_that);case VeilidUpdateConfig() when config != null:
return config(_that);case VeilidUpdateRouteChange() when routeChange != null:
return routeChange(_that);case VeilidUpdateValueChange() when valueChange != null:
return valueChange(_that);case _:
  return null;

}
}
/// A variant of `when` that fallback to an `orElse` callback.
///
/// It is equivalent to doing:
/// ```dart
/// switch (sealedClass) {
///   case Subclass(:final field):
///     return ...;
///   case _:
///     return orElse();
/// }
/// ```

@optionalTypeArgs TResult maybeWhen<TResult extends Object?>({TResult Function( VeilidLogLevel logLevel,  String message,  String? backtrace)?  log,TResult Function(@Uint8ListJsonConverter.jsIsArray()  Uint8List message,  PublicKey? sender,  String? routeId)?  appMessage,TResult Function(@Uint8ListJsonConverter.jsIsArray()  Uint8List message,  String callId,  PublicKey? sender,  String? routeId)?  appCall,TResult Function( AttachmentState state,  bool publicInternetReady,  bool localNetworkReady,  TimestampDuration uptime,  TimestampDuration? attachedUptime,  BigInt reliablePeerCount,  BigInt livePeerCount,  BigInt estimatedNetworkSize,  TimestampDuration? medianLatency,  BigInt overAttachedNodes)?  attachment,TResult Function( bool started,  BigInt bpsDown,  BigInt bpsUp,  List<PeerTableData> peers,  List<NodeId> nodeIds)?  network,TResult Function( VeilidConfig config)?  config,TResult Function( List<String> deadRoutes,  List<String> deadRemoteRoutes)?  routeChange,TResult Function( RecordKey key,  List<ValueSubkeyRange> subkeys,  int count,  ValueData? value)?  valueChange,required TResult orElse(),}) {final _that = this;
switch (_that) {
case VeilidLog() when log != null:
return log(_that.logLevel,_that.message,_that.backtrace);case VeilidAppMessage() when appMessage != null:
return appMessage(_that.message,_that.sender,_that.routeId);case VeilidAppCall() when appCall != null:
return appCall(_that.message,_that.callId,_that.sender,_that.routeId);case VeilidUpdateAttachment() when attachment != null:
return attachment(_that.state,_that.publicInternetReady,_that.localNetworkReady,_that.uptime,_that.attachedUptime,_that.reliablePeerCount,_that.livePeerCount,_that.estimatedNetworkSize,_that.medianLatency,_that.overAttachedNodes);case VeilidUpdateNetwork() when network != null:
return network(_that.started,_that.bpsDown,_that.bpsUp,_that.peers,_that.nodeIds);case VeilidUpdateConfig() when config != null:
return config(_that.config);case VeilidUpdateRouteChange() when routeChange != null:
return routeChange(_that.deadRoutes,_that.deadRemoteRoutes);case VeilidUpdateValueChange() when valueChange != null:
return valueChange(_that.key,_that.subkeys,_that.count,_that.value);case _:
  return orElse();

}
}
/// A `switch`-like method, using callbacks.
///
/// As opposed to `map`, this offers destructuring.
/// It is equivalent to doing:
/// ```dart
/// switch (sealedClass) {
///   case Subclass(:final field):
///     return ...;
///   case Subclass2(:final field2):
///     return ...;
/// }
/// ```

@optionalTypeArgs TResult when<TResult extends Object?>({required TResult Function( VeilidLogLevel logLevel,  String message,  String? backtrace)  log,required TResult Function(@Uint8ListJsonConverter.jsIsArray()  Uint8List message,  PublicKey? sender,  String? routeId)  appMessage,required TResult Function(@Uint8ListJsonConverter.jsIsArray()  Uint8List message,  String callId,  PublicKey? sender,  String? routeId)  appCall,required TResult Function( AttachmentState state,  bool publicInternetReady,  bool localNetworkReady,  TimestampDuration uptime,  TimestampDuration? attachedUptime,  BigInt reliablePeerCount,  BigInt livePeerCount,  BigInt estimatedNetworkSize,  TimestampDuration? medianLatency,  BigInt overAttachedNodes)  attachment,required TResult Function( bool started,  BigInt bpsDown,  BigInt bpsUp,  List<PeerTableData> peers,  List<NodeId> nodeIds)  network,required TResult Function( VeilidConfig config)  config,required TResult Function( List<String> deadRoutes,  List<String> deadRemoteRoutes)  routeChange,required TResult Function( RecordKey key,  List<ValueSubkeyRange> subkeys,  int count,  ValueData? value)  valueChange,}) {final _that = this;
switch (_that) {
case VeilidLog():
return log(_that.logLevel,_that.message,_that.backtrace);case VeilidAppMessage():
return appMessage(_that.message,_that.sender,_that.routeId);case VeilidAppCall():
return appCall(_that.message,_that.callId,_that.sender,_that.routeId);case VeilidUpdateAttachment():
return attachment(_that.state,_that.publicInternetReady,_that.localNetworkReady,_that.uptime,_that.attachedUptime,_that.reliablePeerCount,_that.livePeerCount,_that.estimatedNetworkSize,_that.medianLatency,_that.overAttachedNodes);case VeilidUpdateNetwork():
return network(_that.started,_that.bpsDown,_that.bpsUp,_that.peers,_that.nodeIds);case VeilidUpdateConfig():
return config(_that.config);case VeilidUpdateRouteChange():
return routeChange(_that.deadRoutes,_that.deadRemoteRoutes);case VeilidUpdateValueChange():
return valueChange(_that.key,_that.subkeys,_that.count,_that.value);}
}
/// A variant of `when` that fallback to returning `null`
///
/// It is equivalent to doing:
/// ```dart
/// switch (sealedClass) {
///   case Subclass(:final field):
///     return ...;
///   case _:
///     return null;
/// }
/// ```

@optionalTypeArgs TResult? whenOrNull<TResult extends Object?>({TResult? Function( VeilidLogLevel logLevel,  String message,  String? backtrace)?  log,TResult? Function(@Uint8ListJsonConverter.jsIsArray()  Uint8List message,  PublicKey? sender,  String? routeId)?  appMessage,TResult? Function(@Uint8ListJsonConverter.jsIsArray()  Uint8List message,  String callId,  PublicKey? sender,  String? routeId)?  appCall,TResult? Function( AttachmentState state,  bool publicInternetReady,  bool localNetworkReady,  TimestampDuration uptime,  TimestampDuration? attachedUptime,  BigInt reliablePeerCount,  BigInt livePeerCount,  BigInt estimatedNetworkSize,  TimestampDuration? medianLatency,  BigInt overAttachedNodes)?  attachment,TResult? Function( bool started,  BigInt bpsDown,  BigInt bpsUp,  List<PeerTableData> peers,  List<NodeId> nodeIds)?  network,TResult? Function( VeilidConfig config)?  config,TResult? Function( List<String> deadRoutes,  List<String> deadRemoteRoutes)?  routeChange,TResult? Function( RecordKey key,  List<ValueSubkeyRange> subkeys,  int count,  ValueData? value)?  valueChange,}) {final _that = this;
switch (_that) {
case VeilidLog() when log != null:
return log(_that.logLevel,_that.message,_that.backtrace);case VeilidAppMessage() when appMessage != null:
return appMessage(_that.message,_that.sender,_that.routeId);case VeilidAppCall() when appCall != null:
return appCall(_that.message,_that.callId,_that.sender,_that.routeId);case VeilidUpdateAttachment() when attachment != null:
return attachment(_that.state,_that.publicInternetReady,_that.localNetworkReady,_that.uptime,_that.attachedUptime,_that.reliablePeerCount,_that.livePeerCount,_that.estimatedNetworkSize,_that.medianLatency,_that.overAttachedNodes);case VeilidUpdateNetwork() when network != null:
return network(_that.started,_that.bpsDown,_that.bpsUp,_that.peers,_that.nodeIds);case VeilidUpdateConfig() when config != null:
return config(_that.config);case VeilidUpdateRouteChange() when routeChange != null:
return routeChange(_that.deadRoutes,_that.deadRemoteRoutes);case VeilidUpdateValueChange() when valueChange != null:
return valueChange(_that.key,_that.subkeys,_that.count,_that.value);case _:
  return null;

}
}

}

/// @nodoc
@JsonSerializable()

class VeilidLog implements VeilidUpdate {
  const VeilidLog({required this.logLevel, required this.message, this.backtrace, final  String? $type}): $type = $type ?? 'Log';
  factory VeilidLog.fromJson(Map<String, dynamic> json) => _$VeilidLogFromJson(json);

 final  VeilidLogLevel logLevel;
 final  String message;
 final  String? backtrace;

@JsonKey(name: 'kind')
final String $type;


/// Create a copy of VeilidUpdate
/// with the given fields replaced by the non-null parameter values.
@JsonKey(includeFromJson: false, includeToJson: false)
@pragma('vm:prefer-inline')
$VeilidLogCopyWith<VeilidLog> get copyWith => _$VeilidLogCopyWithImpl<VeilidLog>(this, _$identity);

@override
Map<String, dynamic> toJson() {
  return _$VeilidLogToJson(this, );
}

@override
bool operator ==(Object other) {
  return identical(this, other) || (other.runtimeType == runtimeType&&other is VeilidLog&&(identical(other.logLevel, logLevel) || other.logLevel == logLevel)&&(identical(other.message, message) || other.message == message)&&(identical(other.backtrace, backtrace) || other.backtrace == backtrace));
}

@JsonKey(includeFromJson: false, includeToJson: false)
@override
int get hashCode => Object.hash(runtimeType,logLevel,message,backtrace);

@override
String toString() {
  return 'VeilidUpdate.log(logLevel: $logLevel, message: $message, backtrace: $backtrace)';
}


}

/// @nodoc
abstract mixin class $VeilidLogCopyWith<$Res> implements $VeilidUpdateCopyWith<$Res> {
  factory $VeilidLogCopyWith(VeilidLog value, $Res Function(VeilidLog) _then) = _$VeilidLogCopyWithImpl;
@useResult
$Res call({
 VeilidLogLevel logLevel, String message, String? backtrace
});




}
/// @nodoc
class _$VeilidLogCopyWithImpl<$Res>
    implements $VeilidLogCopyWith<$Res> {
  _$VeilidLogCopyWithImpl(this._self, this._then);

  final VeilidLog _self;
  final $Res Function(VeilidLog) _then;

/// Create a copy of VeilidUpdate
/// with the given fields replaced by the non-null parameter values.
@pragma('vm:prefer-inline') $Res call({Object? logLevel = null,Object? message = null,Object? backtrace = freezed,}) {
  return _then(VeilidLog(
logLevel: null == logLevel ? _self.logLevel : logLevel // ignore: cast_nullable_to_non_nullable
as VeilidLogLevel,message: null == message ? _self.message : message // ignore: cast_nullable_to_non_nullable
as String,backtrace: freezed == backtrace ? _self.backtrace : backtrace // ignore: cast_nullable_to_non_nullable
as String?,
  ));
}


}

/// @nodoc
@JsonSerializable()

class VeilidAppMessage implements VeilidUpdate {
  const VeilidAppMessage({@Uint8ListJsonConverter.jsIsArray() required this.message, this.sender, this.routeId, final  String? $type}): $type = $type ?? 'AppMessage';
  factory VeilidAppMessage.fromJson(Map<String, dynamic> json) => _$VeilidAppMessageFromJson(json);

@Uint8ListJsonConverter.jsIsArray() final  Uint8List message;
 final  PublicKey? sender;
 final  String? routeId;

@JsonKey(name: 'kind')
final String $type;


/// Create a copy of VeilidUpdate
/// with the given fields replaced by the non-null parameter values.
@JsonKey(includeFromJson: false, includeToJson: false)
@pragma('vm:prefer-inline')
$VeilidAppMessageCopyWith<VeilidAppMessage> get copyWith => _$VeilidAppMessageCopyWithImpl<VeilidAppMessage>(this, _$identity);

@override
Map<String, dynamic> toJson() {
  return _$VeilidAppMessageToJson(this, );
}

@override
bool operator ==(Object other) {
  return identical(this, other) || (other.runtimeType == runtimeType&&other is VeilidAppMessage&&const DeepCollectionEquality().equals(other.message, message)&&(identical(other.sender, sender) || other.sender == sender)&&(identical(other.routeId, routeId) || other.routeId == routeId));
}

@JsonKey(includeFromJson: false, includeToJson: false)
@override
int get hashCode => Object.hash(runtimeType,const DeepCollectionEquality().hash(message),sender,routeId);

@override
String toString() {
  return 'VeilidUpdate.appMessage(message: $message, sender: $sender, routeId: $routeId)';
}


}

/// @nodoc
abstract mixin class $VeilidAppMessageCopyWith<$Res> implements $VeilidUpdateCopyWith<$Res> {
  factory $VeilidAppMessageCopyWith(VeilidAppMessage value, $Res Function(VeilidAppMessage) _then) = _$VeilidAppMessageCopyWithImpl;
@useResult
$Res call({
@Uint8ListJsonConverter.jsIsArray() Uint8List message, PublicKey? sender, String? routeId
});




}
/// @nodoc
class _$VeilidAppMessageCopyWithImpl<$Res>
    implements $VeilidAppMessageCopyWith<$Res> {
  _$VeilidAppMessageCopyWithImpl(this._self, this._then);

  final VeilidAppMessage _self;
  final $Res Function(VeilidAppMessage) _then;

/// Create a copy of VeilidUpdate
/// with the given fields replaced by the non-null parameter values.
@pragma('vm:prefer-inline') $Res call({Object? message = null,Object? sender = freezed,Object? routeId = freezed,}) {
  return _then(VeilidAppMessage(
message: null == message ? _self.message : message // ignore: cast_nullable_to_non_nullable
as Uint8List,sender: freezed == sender ? _self.sender : sender // ignore: cast_nullable_to_non_nullable
as PublicKey?,routeId: freezed == routeId ? _self.routeId : routeId // ignore: cast_nullable_to_non_nullable
as String?,
  ));
}


}

/// @nodoc
@JsonSerializable()

class VeilidAppCall implements VeilidUpdate {
  const VeilidAppCall({@Uint8ListJsonConverter.jsIsArray() required this.message, required this.callId, this.sender, this.routeId, final  String? $type}): $type = $type ?? 'AppCall';
  factory VeilidAppCall.fromJson(Map<String, dynamic> json) => _$VeilidAppCallFromJson(json);

@Uint8ListJsonConverter.jsIsArray() final  Uint8List message;
 final  String callId;
 final  PublicKey? sender;
 final  String? routeId;

@JsonKey(name: 'kind')
final String $type;


/// Create a copy of VeilidUpdate
/// with the given fields replaced by the non-null parameter values.
@JsonKey(includeFromJson: false, includeToJson: false)
@pragma('vm:prefer-inline')
$VeilidAppCallCopyWith<VeilidAppCall> get copyWith => _$VeilidAppCallCopyWithImpl<VeilidAppCall>(this, _$identity);

@override
Map<String, dynamic> toJson() {
  return _$VeilidAppCallToJson(this, );
}

@override
bool operator ==(Object other) {
  return identical(this, other) || (other.runtimeType == runtimeType&&other is VeilidAppCall&&const DeepCollectionEquality().equals(other.message, message)&&(identical(other.callId, callId) || other.callId == callId)&&(identical(other.sender, sender) || other.sender == sender)&&(identical(other.routeId, routeId) || other.routeId == routeId));
}

@JsonKey(includeFromJson: false, includeToJson: false)
@override
int get hashCode => Object.hash(runtimeType,const DeepCollectionEquality().hash(message),callId,sender,routeId);

@override
String toString() {
  return 'VeilidUpdate.appCall(message: $message, callId: $callId, sender: $sender, routeId: $routeId)';
}


}

/// @nodoc
abstract mixin class $VeilidAppCallCopyWith<$Res> implements $VeilidUpdateCopyWith<$Res> {
  factory $VeilidAppCallCopyWith(VeilidAppCall value, $Res Function(VeilidAppCall) _then) = _$VeilidAppCallCopyWithImpl;
@useResult
$Res call({
@Uint8ListJsonConverter.jsIsArray() Uint8List message, String callId, PublicKey? sender, String? routeId
});




}
/// @nodoc
class _$VeilidAppCallCopyWithImpl<$Res>
    implements $VeilidAppCallCopyWith<$Res> {
  _$VeilidAppCallCopyWithImpl(this._self, this._then);

  final VeilidAppCall _self;
  final $Res Function(VeilidAppCall) _then;

/// Create a copy of VeilidUpdate
/// with the given fields replaced by the non-null parameter values.
@pragma('vm:prefer-inline') $Res call({Object? message = null,Object? callId = null,Object? sender = freezed,Object? routeId = freezed,}) {
  return _then(VeilidAppCall(
message: null == message ? _self.message : message // ignore: cast_nullable_to_non_nullable
as Uint8List,callId: null == callId ? _self.callId : callId // ignore: cast_nullable_to_non_nullable
as String,sender: freezed == sender ? _self.sender : sender // ignore: cast_nullable_to_non_nullable
as PublicKey?,routeId: freezed == routeId ? _self.routeId : routeId // ignore: cast_nullable_to_non_nullable
as String?,
  ));
}


}

/// @nodoc
@JsonSerializable()

class VeilidUpdateAttachment implements VeilidUpdate {
  const VeilidUpdateAttachment({required this.state, required this.publicInternetReady, required this.localNetworkReady, required this.uptime, required this.attachedUptime, required this.reliablePeerCount, required this.livePeerCount, required this.estimatedNetworkSize, required this.medianLatency, required this.overAttachedNodes, final  String? $type}): $type = $type ?? 'Attachment';
  factory VeilidUpdateAttachment.fromJson(Map<String, dynamic> json) => _$VeilidUpdateAttachmentFromJson(json);

 final  AttachmentState state;
 final  bool publicInternetReady;
 final  bool localNetworkReady;
 final  TimestampDuration uptime;
 final  TimestampDuration? attachedUptime;
 final  BigInt reliablePeerCount;
 final  BigInt livePeerCount;
 final  BigInt estimatedNetworkSize;
 final  TimestampDuration? medianLatency;
 final  BigInt overAttachedNodes;

@JsonKey(name: 'kind')
final String $type;


/// Create a copy of VeilidUpdate
/// with the given fields replaced by the non-null parameter values.
@JsonKey(includeFromJson: false, includeToJson: false)
@pragma('vm:prefer-inline')
$VeilidUpdateAttachmentCopyWith<VeilidUpdateAttachment> get copyWith => _$VeilidUpdateAttachmentCopyWithImpl<VeilidUpdateAttachment>(this, _$identity);

@override
Map<String, dynamic> toJson() {
  return _$VeilidUpdateAttachmentToJson(this, );
}

@override
bool operator ==(Object other) {
  return identical(this, other) || (other.runtimeType == runtimeType&&other is VeilidUpdateAttachment&&(identical(other.state, state) || other.state == state)&&(identical(other.publicInternetReady, publicInternetReady) || other.publicInternetReady == publicInternetReady)&&(identical(other.localNetworkReady, localNetworkReady) || other.localNetworkReady == localNetworkReady)&&(identical(other.uptime, uptime) || other.uptime == uptime)&&(identical(other.attachedUptime, attachedUptime) || other.attachedUptime == attachedUptime)&&(identical(other.reliablePeerCount, reliablePeerCount) || other.reliablePeerCount == reliablePeerCount)&&(identical(other.livePeerCount, livePeerCount) || other.livePeerCount == livePeerCount)&&(identical(other.estimatedNetworkSize, estimatedNetworkSize) || other.estimatedNetworkSize == estimatedNetworkSize)&&(identical(other.medianLatency, medianLatency) || other.medianLatency == medianLatency)&&(identical(other.overAttachedNodes, overAttachedNodes) || other.overAttachedNodes == overAttachedNodes));
}

@JsonKey(includeFromJson: false, includeToJson: false)
@override
int get hashCode => Object.hash(runtimeType,state,publicInternetReady,localNetworkReady,uptime,attachedUptime,reliablePeerCount,livePeerCount,estimatedNetworkSize,medianLatency,overAttachedNodes);

@override
String toString() {
  return 'VeilidUpdate.attachment(state: $state, publicInternetReady: $publicInternetReady, localNetworkReady: $localNetworkReady, uptime: $uptime, attachedUptime: $attachedUptime, reliablePeerCount: $reliablePeerCount, livePeerCount: $livePeerCount, estimatedNetworkSize: $estimatedNetworkSize, medianLatency: $medianLatency, overAttachedNodes: $overAttachedNodes)';
}


}

/// @nodoc
abstract mixin class $VeilidUpdateAttachmentCopyWith<$Res> implements $VeilidUpdateCopyWith<$Res> {
  factory $VeilidUpdateAttachmentCopyWith(VeilidUpdateAttachment value, $Res Function(VeilidUpdateAttachment) _then) = _$VeilidUpdateAttachmentCopyWithImpl;
@useResult
$Res call({
 AttachmentState state, bool publicInternetReady, bool localNetworkReady, TimestampDuration uptime, TimestampDuration? attachedUptime, BigInt reliablePeerCount, BigInt livePeerCount, BigInt estimatedNetworkSize, TimestampDuration? medianLatency, BigInt overAttachedNodes
});




}
/// @nodoc
class _$VeilidUpdateAttachmentCopyWithImpl<$Res>
    implements $VeilidUpdateAttachmentCopyWith<$Res> {
  _$VeilidUpdateAttachmentCopyWithImpl(this._self, this._then);

  final VeilidUpdateAttachment _self;
  final $Res Function(VeilidUpdateAttachment) _then;

/// Create a copy of VeilidUpdate
/// with the given fields replaced by the non-null parameter values.
@pragma('vm:prefer-inline') $Res call({Object? state = null,Object? publicInternetReady = null,Object? localNetworkReady = null,Object? uptime = null,Object? attachedUptime = freezed,Object? reliablePeerCount = null,Object? livePeerCount = null,Object? estimatedNetworkSize = null,Object? medianLatency = freezed,Object? overAttachedNodes = null,}) {
  return _then(VeilidUpdateAttachment(
state: null == state ? _self.state : state // ignore: cast_nullable_to_non_nullable
as AttachmentState,publicInternetReady: null == publicInternetReady ? _self.publicInternetReady : publicInternetReady // ignore: cast_nullable_to_non_nullable
as bool,localNetworkReady: null == localNetworkReady ? _self.localNetworkReady : localNetworkReady // ignore: cast_nullable_to_non_nullable
as bool,uptime: null == uptime ? _self.uptime : uptime // ignore: cast_nullable_to_non_nullable
as TimestampDuration,attachedUptime: freezed == attachedUptime ? _self.attachedUptime : attachedUptime // ignore: cast_nullable_to_non_nullable
as TimestampDuration?,reliablePeerCount: null == reliablePeerCount ? _self.reliablePeerCount : reliablePeerCount // ignore: cast_nullable_to_non_nullable
as BigInt,livePeerCount: null == livePeerCount ? _self.livePeerCount : livePeerCount // ignore: cast_nullable_to_non_nullable
as BigInt,estimatedNetworkSize: null == estimatedNetworkSize ? _self.estimatedNetworkSize : estimatedNetworkSize // ignore: cast_nullable_to_non_nullable
as BigInt,medianLatency: freezed == medianLatency ? _self.medianLatency : medianLatency // ignore: cast_nullable_to_non_nullable
as TimestampDuration?,overAttachedNodes: null == overAttachedNodes ? _self.overAttachedNodes : overAttachedNodes // ignore: cast_nullable_to_non_nullable
as BigInt,
  ));
}


}

/// @nodoc
@JsonSerializable()

class VeilidUpdateNetwork implements VeilidUpdate {
  const VeilidUpdateNetwork({required this.started, required this.bpsDown, required this.bpsUp, required final  List<PeerTableData> peers, required final  List<NodeId> nodeIds, final  String? $type}): _peers = peers,_nodeIds = nodeIds,$type = $type ?? 'Network';
  factory VeilidUpdateNetwork.fromJson(Map<String, dynamic> json) => _$VeilidUpdateNetworkFromJson(json);

 final  bool started;
 final  BigInt bpsDown;
 final  BigInt bpsUp;
 final  List<PeerTableData> _peers;
 List<PeerTableData> get peers {
  if (_peers is EqualUnmodifiableListView) return _peers;
  // ignore: implicit_dynamic_type
  return EqualUnmodifiableListView(_peers);
}

 final  List<NodeId> _nodeIds;
 List<NodeId> get nodeIds {
  if (_nodeIds is EqualUnmodifiableListView) return _nodeIds;
  // ignore: implicit_dynamic_type
  return EqualUnmodifiableListView(_nodeIds);
}


@JsonKey(name: 'kind')
final String $type;


/// Create a copy of VeilidUpdate
/// with the given fields replaced by the non-null parameter values.
@JsonKey(includeFromJson: false, includeToJson: false)
@pragma('vm:prefer-inline')
$VeilidUpdateNetworkCopyWith<VeilidUpdateNetwork> get copyWith => _$VeilidUpdateNetworkCopyWithImpl<VeilidUpdateNetwork>(this, _$identity);

@override
Map<String, dynamic> toJson() {
  return _$VeilidUpdateNetworkToJson(this, );
}

@override
bool operator ==(Object other) {
  return identical(this, other) || (other.runtimeType == runtimeType&&other is VeilidUpdateNetwork&&(identical(other.started, started) || other.started == started)&&(identical(other.bpsDown, bpsDown) || other.bpsDown == bpsDown)&&(identical(other.bpsUp, bpsUp) || other.bpsUp == bpsUp)&&const DeepCollectionEquality().equals(other._peers, _peers)&&const DeepCollectionEquality().equals(other._nodeIds, _nodeIds));
}

@JsonKey(includeFromJson: false, includeToJson: false)
@override
int get hashCode => Object.hash(runtimeType,started,bpsDown,bpsUp,const DeepCollectionEquality().hash(_peers),const DeepCollectionEquality().hash(_nodeIds));

@override
String toString() {
  return 'VeilidUpdate.network(started: $started, bpsDown: $bpsDown, bpsUp: $bpsUp, peers: $peers, nodeIds: $nodeIds)';
}


}

/// @nodoc
abstract mixin class $VeilidUpdateNetworkCopyWith<$Res> implements $VeilidUpdateCopyWith<$Res> {
  factory $VeilidUpdateNetworkCopyWith(VeilidUpdateNetwork value, $Res Function(VeilidUpdateNetwork) _then) = _$VeilidUpdateNetworkCopyWithImpl;
@useResult
$Res call({
 bool started, BigInt bpsDown, BigInt bpsUp, List<PeerTableData> peers, List<NodeId> nodeIds
});




}
/// @nodoc
class _$VeilidUpdateNetworkCopyWithImpl<$Res>
    implements $VeilidUpdateNetworkCopyWith<$Res> {
  _$VeilidUpdateNetworkCopyWithImpl(this._self, this._then);

  final VeilidUpdateNetwork _self;
  final $Res Function(VeilidUpdateNetwork) _then;

/// Create a copy of VeilidUpdate
/// with the given fields replaced by the non-null parameter values.
@pragma('vm:prefer-inline') $Res call({Object? started = null,Object? bpsDown = null,Object? bpsUp = null,Object? peers = null,Object? nodeIds = null,}) {
  return _then(VeilidUpdateNetwork(
started: null == started ? _self.started : started // ignore: cast_nullable_to_non_nullable
as bool,bpsDown: null == bpsDown ? _self.bpsDown : bpsDown // ignore: cast_nullable_to_non_nullable
as BigInt,bpsUp: null == bpsUp ? _self.bpsUp : bpsUp // ignore: cast_nullable_to_non_nullable
as BigInt,peers: null == peers ? _self._peers : peers // ignore: cast_nullable_to_non_nullable
as List<PeerTableData>,nodeIds: null == nodeIds ? _self._nodeIds : nodeIds // ignore: cast_nullable_to_non_nullable
as List<NodeId>,
  ));
}


}

/// @nodoc
@JsonSerializable()

class VeilidUpdateConfig implements VeilidUpdate {
  const VeilidUpdateConfig({required this.config, final  String? $type}): $type = $type ?? 'Config';
  factory VeilidUpdateConfig.fromJson(Map<String, dynamic> json) => _$VeilidUpdateConfigFromJson(json);

 final  VeilidConfig config;

@JsonKey(name: 'kind')
final String $type;


/// Create a copy of VeilidUpdate
/// with the given fields replaced by the non-null parameter values.
@JsonKey(includeFromJson: false, includeToJson: false)
@pragma('vm:prefer-inline')
$VeilidUpdateConfigCopyWith<VeilidUpdateConfig> get copyWith => _$VeilidUpdateConfigCopyWithImpl<VeilidUpdateConfig>(this, _$identity);

@override
Map<String, dynamic> toJson() {
  return _$VeilidUpdateConfigToJson(this, );
}

@override
bool operator ==(Object other) {
  return identical(this, other) || (other.runtimeType == runtimeType&&other is VeilidUpdateConfig&&(identical(other.config, config) || other.config == config));
}

@JsonKey(includeFromJson: false, includeToJson: false)
@override
int get hashCode => Object.hash(runtimeType,config);

@override
String toString() {
  return 'VeilidUpdate.config(config: $config)';
}


}

/// @nodoc
abstract mixin class $VeilidUpdateConfigCopyWith<$Res> implements $VeilidUpdateCopyWith<$Res> {
  factory $VeilidUpdateConfigCopyWith(VeilidUpdateConfig value, $Res Function(VeilidUpdateConfig) _then) = _$VeilidUpdateConfigCopyWithImpl;
@useResult
$Res call({
 VeilidConfig config
});


$VeilidConfigCopyWith<$Res> get config;

}
/// @nodoc
class _$VeilidUpdateConfigCopyWithImpl<$Res>
    implements $VeilidUpdateConfigCopyWith<$Res> {
  _$VeilidUpdateConfigCopyWithImpl(this._self, this._then);

  final VeilidUpdateConfig _self;
  final $Res Function(VeilidUpdateConfig) _then;

/// Create a copy of VeilidUpdate
/// with the given fields replaced by the non-null parameter values.
@pragma('vm:prefer-inline') $Res call({Object? config = null,}) {
  return _then(VeilidUpdateConfig(
config: null == config ? _self.config : config // ignore: cast_nullable_to_non_nullable
as VeilidConfig,
  ));
}

/// Create a copy of VeilidUpdate
/// with the given fields replaced by the non-null parameter values.
@override
@pragma('vm:prefer-inline')
$VeilidConfigCopyWith<$Res> get config {
  
  return $VeilidConfigCopyWith<$Res>(_self.config, (value) {
    return _then(_self.copyWith(config: value));
  });
}
}

/// @nodoc
@JsonSerializable()

class VeilidUpdateRouteChange implements VeilidUpdate {
  const VeilidUpdateRouteChange({required final  List<String> deadRoutes, required final  List<String> deadRemoteRoutes, final  String? $type}): _deadRoutes = deadRoutes,_deadRemoteRoutes = deadRemoteRoutes,$type = $type ?? 'RouteChange';
  factory VeilidUpdateRouteChange.fromJson(Map<String, dynamic> json) => _$VeilidUpdateRouteChangeFromJson(json);

 final  List<String> _deadRoutes;
 List<String> get deadRoutes {
  if (_deadRoutes is EqualUnmodifiableListView) return _deadRoutes;
  // ignore: implicit_dynamic_type
  return EqualUnmodifiableListView(_deadRoutes);
}

 final  List<String> _deadRemoteRoutes;
 List<String> get deadRemoteRoutes {
  if (_deadRemoteRoutes is EqualUnmodifiableListView) return _deadRemoteRoutes;
  // ignore: implicit_dynamic_type
  return EqualUnmodifiableListView(_deadRemoteRoutes);
}


@JsonKey(name: 'kind')
final String $type;


/// Create a copy of VeilidUpdate
/// with the given fields replaced by the non-null parameter values.
@JsonKey(includeFromJson: false, includeToJson: false)
@pragma('vm:prefer-inline')
$VeilidUpdateRouteChangeCopyWith<VeilidUpdateRouteChange> get copyWith => _$VeilidUpdateRouteChangeCopyWithImpl<VeilidUpdateRouteChange>(this, _$identity);

@override
Map<String, dynamic> toJson() {
  return _$VeilidUpdateRouteChangeToJson(this, );
}

@override
bool operator ==(Object other) {
  return identical(this, other) || (other.runtimeType == runtimeType&&other is VeilidUpdateRouteChange&&const DeepCollectionEquality().equals(other._deadRoutes, _deadRoutes)&&const DeepCollectionEquality().equals(other._deadRemoteRoutes, _deadRemoteRoutes));
}

@JsonKey(includeFromJson: false, includeToJson: false)
@override
int get hashCode => Object.hash(runtimeType,const DeepCollectionEquality().hash(_deadRoutes),const DeepCollectionEquality().hash(_deadRemoteRoutes));

@override
String toString() {
  return 'VeilidUpdate.routeChange(deadRoutes: $deadRoutes, deadRemoteRoutes: $deadRemoteRoutes)';
}


}

/// @nodoc
abstract mixin class $VeilidUpdateRouteChangeCopyWith<$Res> implements $VeilidUpdateCopyWith<$Res> {
  factory $VeilidUpdateRouteChangeCopyWith(VeilidUpdateRouteChange value, $Res Function(VeilidUpdateRouteChange) _then) = _$VeilidUpdateRouteChangeCopyWithImpl;
@useResult
$Res call({
 List<String> deadRoutes, List<String> deadRemoteRoutes
});




}
/// @nodoc
class _$VeilidUpdateRouteChangeCopyWithImpl<$Res>
    implements $VeilidUpdateRouteChangeCopyWith<$Res> {
  _$VeilidUpdateRouteChangeCopyWithImpl(this._self, this._then);

  final VeilidUpdateRouteChange _self;
  final $Res Function(VeilidUpdateRouteChange) _then;

/// Create a copy of VeilidUpdate
/// with the given fields replaced by the non-null parameter values.
@pragma('vm:prefer-inline') $Res call({Object? deadRoutes = null,Object? deadRemoteRoutes = null,}) {
  return _then(VeilidUpdateRouteChange(
deadRoutes: null == deadRoutes ? _self._deadRoutes : deadRoutes // ignore: cast_nullable_to_non_nullable
as List<String>,deadRemoteRoutes: null == deadRemoteRoutes ? _self._deadRemoteRoutes : deadRemoteRoutes // ignore: cast_nullable_to_non_nullable
as List<String>,
  ));
}


}

/// @nodoc
@JsonSerializable()

class VeilidUpdateValueChange implements VeilidUpdate {
  const VeilidUpdateValueChange({required this.key, required final  List<ValueSubkeyRange> subkeys, required this.count, required this.value, final  String? $type}): _subkeys = subkeys,$type = $type ?? 'ValueChange';
  factory VeilidUpdateValueChange.fromJson(Map<String, dynamic> json) => _$VeilidUpdateValueChangeFromJson(json);

 final  RecordKey key;
 final  List<ValueSubkeyRange> _subkeys;
 List<ValueSubkeyRange> get subkeys {
  if (_subkeys is EqualUnmodifiableListView) return _subkeys;
  // ignore: implicit_dynamic_type
  return EqualUnmodifiableListView(_subkeys);
}

 final  int count;
 final  ValueData? value;

@JsonKey(name: 'kind')
final String $type;


/// Create a copy of VeilidUpdate
/// with the given fields replaced by the non-null parameter values.
@JsonKey(includeFromJson: false, includeToJson: false)
@pragma('vm:prefer-inline')
$VeilidUpdateValueChangeCopyWith<VeilidUpdateValueChange> get copyWith => _$VeilidUpdateValueChangeCopyWithImpl<VeilidUpdateValueChange>(this, _$identity);

@override
Map<String, dynamic> toJson() {
  return _$VeilidUpdateValueChangeToJson(this, );
}

@override
bool operator ==(Object other) {
  return identical(this, other) || (other.runtimeType == runtimeType&&other is VeilidUpdateValueChange&&(identical(other.key, key) || other.key == key)&&const DeepCollectionEquality().equals(other._subkeys, _subkeys)&&(identical(other.count, count) || other.count == count)&&(identical(other.value, value) || other.value == value));
}

@JsonKey(includeFromJson: false, includeToJson: false)
@override
int get hashCode => Object.hash(runtimeType,key,const DeepCollectionEquality().hash(_subkeys),count,value);

@override
String toString() {
  return 'VeilidUpdate.valueChange(key: $key, subkeys: $subkeys, count: $count, value: $value)';
}


}

/// @nodoc
abstract mixin class $VeilidUpdateValueChangeCopyWith<$Res> implements $VeilidUpdateCopyWith<$Res> {
  factory $VeilidUpdateValueChangeCopyWith(VeilidUpdateValueChange value, $Res Function(VeilidUpdateValueChange) _then) = _$VeilidUpdateValueChangeCopyWithImpl;
@useResult
$Res call({
 RecordKey key, List<ValueSubkeyRange> subkeys, int count, ValueData? value
});


$ValueDataCopyWith<$Res>? get value;

}
/// @nodoc
class _$VeilidUpdateValueChangeCopyWithImpl<$Res>
    implements $VeilidUpdateValueChangeCopyWith<$Res> {
  _$VeilidUpdateValueChangeCopyWithImpl(this._self, this._then);

  final VeilidUpdateValueChange _self;
  final $Res Function(VeilidUpdateValueChange) _then;

/// Create a copy of VeilidUpdate
/// with the given fields replaced by the non-null parameter values.
@pragma('vm:prefer-inline') $Res call({Object? key = null,Object? subkeys = null,Object? count = null,Object? value = freezed,}) {
  return _then(VeilidUpdateValueChange(
key: null == key ? _self.key : key // ignore: cast_nullable_to_non_nullable
as RecordKey,subkeys: null == subkeys ? _self._subkeys : subkeys // ignore: cast_nullable_to_non_nullable
as List<ValueSubkeyRange>,count: null == count ? _self.count : count // ignore: cast_nullable_to_non_nullable
as int,value: freezed == value ? _self.value : value // ignore: cast_nullable_to_non_nullable
as ValueData?,
  ));
}

/// Create a copy of VeilidUpdate
/// with the given fields replaced by the non-null parameter values.
@override
@pragma('vm:prefer-inline')
$ValueDataCopyWith<$Res>? get value {
    if (_self.value == null) {
    return null;
  }

  return $ValueDataCopyWith<$Res>(_self.value!, (value) {
    return _then(_self.copyWith(value: value));
  });
}
}


/// @nodoc
mixin _$VeilidStateAttachment {

 AttachmentState get state; bool get publicInternetReady; bool get localNetworkReady; TimestampDuration get uptime; TimestampDuration? get attachedUptime; BigInt get reliablePeerCount; BigInt get livePeerCount; BigInt get estimatedNetworkSize; TimestampDuration? get medianLatency; BigInt get overAttachedNodes;
/// Create a copy of VeilidStateAttachment
/// with the given fields replaced by the non-null parameter values.
@JsonKey(includeFromJson: false, includeToJson: false)
@pragma('vm:prefer-inline')
$VeilidStateAttachmentCopyWith<VeilidStateAttachment> get copyWith => _$VeilidStateAttachmentCopyWithImpl<VeilidStateAttachment>(this as VeilidStateAttachment, _$identity);

  /// Serializes this VeilidStateAttachment to a JSON map.
  Map<String, dynamic> toJson();


@override
bool operator ==(Object other) {
  return identical(this, other) || (other.runtimeType == runtimeType&&other is VeilidStateAttachment&&(identical(other.state, state) || other.state == state)&&(identical(other.publicInternetReady, publicInternetReady) || other.publicInternetReady == publicInternetReady)&&(identical(other.localNetworkReady, localNetworkReady) || other.localNetworkReady == localNetworkReady)&&(identical(other.uptime, uptime) || other.uptime == uptime)&&(identical(other.attachedUptime, attachedUptime) || other.attachedUptime == attachedUptime)&&(identical(other.reliablePeerCount, reliablePeerCount) || other.reliablePeerCount == reliablePeerCount)&&(identical(other.livePeerCount, livePeerCount) || other.livePeerCount == livePeerCount)&&(identical(other.estimatedNetworkSize, estimatedNetworkSize) || other.estimatedNetworkSize == estimatedNetworkSize)&&(identical(other.medianLatency, medianLatency) || other.medianLatency == medianLatency)&&(identical(other.overAttachedNodes, overAttachedNodes) || other.overAttachedNodes == overAttachedNodes));
}

@JsonKey(includeFromJson: false, includeToJson: false)
@override
int get hashCode => Object.hash(runtimeType,state,publicInternetReady,localNetworkReady,uptime,attachedUptime,reliablePeerCount,livePeerCount,estimatedNetworkSize,medianLatency,overAttachedNodes);

@override
String toString() {
  return 'VeilidStateAttachment(state: $state, publicInternetReady: $publicInternetReady, localNetworkReady: $localNetworkReady, uptime: $uptime, attachedUptime: $attachedUptime, reliablePeerCount: $reliablePeerCount, livePeerCount: $livePeerCount, estimatedNetworkSize: $estimatedNetworkSize, medianLatency: $medianLatency, overAttachedNodes: $overAttachedNodes)';
}


}

/// @nodoc
abstract mixin class $VeilidStateAttachmentCopyWith<$Res>  {
  factory $VeilidStateAttachmentCopyWith(VeilidStateAttachment value, $Res Function(VeilidStateAttachment) _then) = _$VeilidStateAttachmentCopyWithImpl;
@useResult
$Res call({
 AttachmentState state, bool publicInternetReady, bool localNetworkReady, TimestampDuration uptime, TimestampDuration? attachedUptime, BigInt reliablePeerCount, BigInt livePeerCount, BigInt estimatedNetworkSize, TimestampDuration? medianLatency, BigInt overAttachedNodes
});




}
/// @nodoc
class _$VeilidStateAttachmentCopyWithImpl<$Res>
    implements $VeilidStateAttachmentCopyWith<$Res> {
  _$VeilidStateAttachmentCopyWithImpl(this._self, this._then);

  final VeilidStateAttachment _self;
  final $Res Function(VeilidStateAttachment) _then;

/// Create a copy of VeilidStateAttachment
/// with the given fields replaced by the non-null parameter values.
@pragma('vm:prefer-inline') @override $Res call({Object? state = null,Object? publicInternetReady = null,Object? localNetworkReady = null,Object? uptime = null,Object? attachedUptime = freezed,Object? reliablePeerCount = null,Object? livePeerCount = null,Object? estimatedNetworkSize = null,Object? medianLatency = freezed,Object? overAttachedNodes = null,}) {
  return _then(_self.copyWith(
state: null == state ? _self.state : state // ignore: cast_nullable_to_non_nullable
as AttachmentState,publicInternetReady: null == publicInternetReady ? _self.publicInternetReady : publicInternetReady // ignore: cast_nullable_to_non_nullable
as bool,localNetworkReady: null == localNetworkReady ? _self.localNetworkReady : localNetworkReady // ignore: cast_nullable_to_non_nullable
as bool,uptime: null == uptime ? _self.uptime : uptime // ignore: cast_nullable_to_non_nullable
as TimestampDuration,attachedUptime: freezed == attachedUptime ? _self.attachedUptime : attachedUptime // ignore: cast_nullable_to_non_nullable
as TimestampDuration?,reliablePeerCount: null == reliablePeerCount ? _self.reliablePeerCount : reliablePeerCount // ignore: cast_nullable_to_non_nullable
as BigInt,livePeerCount: null == livePeerCount ? _self.livePeerCount : livePeerCount // ignore: cast_nullable_to_non_nullable
as BigInt,estimatedNetworkSize: null == estimatedNetworkSize ? _self.estimatedNetworkSize : estimatedNetworkSize // ignore: cast_nullable_to_non_nullable
as BigInt,medianLatency: freezed == medianLatency ? _self.medianLatency : medianLatency // ignore: cast_nullable_to_non_nullable
as TimestampDuration?,overAttachedNodes: null == overAttachedNodes ? _self.overAttachedNodes : overAttachedNodes // ignore: cast_nullable_to_non_nullable
as BigInt,
  ));
}

}


/// Adds pattern-matching-related methods to [VeilidStateAttachment].
extension VeilidStateAttachmentPatterns on VeilidStateAttachment {
/// A variant of `map` that fallback to returning `orElse`.
///
/// It is equivalent to doing:
/// ```dart
/// switch (sealedClass) {
///   case final Subclass value:
///     return ...;
///   case _:
///     return orElse();
/// }
/// ```

@optionalTypeArgs TResult maybeMap<TResult extends Object?>(TResult Function( _VeilidStateAttachment value)?  $default,{required TResult orElse(),}){
final _that = this;
switch (_that) {
case _VeilidStateAttachment() when $default != null:
return $default(_that);case _:
  return orElse();

}
}
/// A `switch`-like method, using callbacks.
///
/// Callbacks receives the raw object, upcasted.
/// It is equivalent to doing:
/// ```dart
/// switch (sealedClass) {
///   case final Subclass value:
///     return ...;
///   case final Subclass2 value:
///     return ...;
/// }
/// ```

@optionalTypeArgs TResult map<TResult extends Object?>(TResult Function( _VeilidStateAttachment value)  $default,){
final _that = this;
switch (_that) {
case _VeilidStateAttachment():
return $default(_that);}
}
/// A variant of `map` that fallback to returning `null`.
///
/// It is equivalent to doing:
/// ```dart
/// switch (sealedClass) {
///   case final Subclass value:
///     return ...;
///   case _:
///     return null;
/// }
/// ```

@optionalTypeArgs TResult? mapOrNull<TResult extends Object?>(TResult? Function( _VeilidStateAttachment value)?  $default,){
final _that = this;
switch (_that) {
case _VeilidStateAttachment() when $default != null:
return $default(_that);case _:
  return null;

}
}
/// A variant of `when` that fallback to an `orElse` callback.
///
/// It is equivalent to doing:
/// ```dart
/// switch (sealedClass) {
///   case Subclass(:final field):
///     return ...;
///   case _:
///     return orElse();
/// }
/// ```

@optionalTypeArgs TResult maybeWhen<TResult extends Object?>(TResult Function( AttachmentState state,  bool publicInternetReady,  bool localNetworkReady,  TimestampDuration uptime,  TimestampDuration? attachedUptime,  BigInt reliablePeerCount,  BigInt livePeerCount,  BigInt estimatedNetworkSize,  TimestampDuration? medianLatency,  BigInt overAttachedNodes)?  $default,{required TResult orElse(),}) {final _that = this;
switch (_that) {
case _VeilidStateAttachment() when $default != null:
return $default(_that.state,_that.publicInternetReady,_that.localNetworkReady,_that.uptime,_that.attachedUptime,_that.reliablePeerCount,_that.livePeerCount,_that.estimatedNetworkSize,_that.medianLatency,_that.overAttachedNodes);case _:
  return orElse();

}
}
/// A `switch`-like method, using callbacks.
///
/// As opposed to `map`, this offers destructuring.
/// It is equivalent to doing:
/// ```dart
/// switch (sealedClass) {
///   case Subclass(:final field):
///     return ...;
///   case Subclass2(:final field2):
///     return ...;
/// }
/// ```

@optionalTypeArgs TResult when<TResult extends Object?>(TResult Function( AttachmentState state,  bool publicInternetReady,  bool localNetworkReady,  TimestampDuration uptime,  TimestampDuration? attachedUptime,  BigInt reliablePeerCount,  BigInt livePeerCount,  BigInt estimatedNetworkSize,  TimestampDuration? medianLatency,  BigInt overAttachedNodes)  $default,) {final _that = this;
switch (_that) {
case _VeilidStateAttachment():
return $default(_that.state,_that.publicInternetReady,_that.localNetworkReady,_that.uptime,_that.attachedUptime,_that.reliablePeerCount,_that.livePeerCount,_that.estimatedNetworkSize,_that.medianLatency,_that.overAttachedNodes);}
}
/// A variant of `when` that fallback to returning `null`
///
/// It is equivalent to doing:
/// ```dart
/// switch (sealedClass) {
///   case Subclass(:final field):
///     return ...;
///   case _:
///     return null;
/// }
/// ```

@optionalTypeArgs TResult? whenOrNull<TResult extends Object?>(TResult? Function( AttachmentState state,  bool publicInternetReady,  bool localNetworkReady,  TimestampDuration uptime,  TimestampDuration? attachedUptime,  BigInt reliablePeerCount,  BigInt livePeerCount,  BigInt estimatedNetworkSize,  TimestampDuration? medianLatency,  BigInt overAttachedNodes)?  $default,) {final _that = this;
switch (_that) {
case _VeilidStateAttachment() when $default != null:
return $default(_that.state,_that.publicInternetReady,_that.localNetworkReady,_that.uptime,_that.attachedUptime,_that.reliablePeerCount,_that.livePeerCount,_that.estimatedNetworkSize,_that.medianLatency,_that.overAttachedNodes);case _:
  return null;

}
}

}

/// @nodoc
@JsonSerializable()

class _VeilidStateAttachment implements VeilidStateAttachment {
  const _VeilidStateAttachment({required this.state, required this.publicInternetReady, required this.localNetworkReady, required this.uptime, required this.attachedUptime, required this.reliablePeerCount, required this.livePeerCount, required this.estimatedNetworkSize, required this.medianLatency, required this.overAttachedNodes});
  factory _VeilidStateAttachment.fromJson(Map<String, dynamic> json) => _$VeilidStateAttachmentFromJson(json);

@override final  AttachmentState state;
@override final  bool publicInternetReady;
@override final  bool localNetworkReady;
@override final  TimestampDuration uptime;
@override final  TimestampDuration? attachedUptime;
@override final  BigInt reliablePeerCount;
@override final  BigInt livePeerCount;
@override final  BigInt estimatedNetworkSize;
@override final  TimestampDuration? medianLatency;
@override final  BigInt overAttachedNodes;

/// Create a copy of VeilidStateAttachment
/// with the given fields replaced by the non-null parameter values.
@override @JsonKey(includeFromJson: false, includeToJson: false)
@pragma('vm:prefer-inline')
_$VeilidStateAttachmentCopyWith<_VeilidStateAttachment> get copyWith => __$VeilidStateAttachmentCopyWithImpl<_VeilidStateAttachment>(this, _$identity);

@override
Map<String, dynamic> toJson() {
  return _$VeilidStateAttachmentToJson(this, );
}

@override
bool operator ==(Object other) {
  return identical(this, other) || (other.runtimeType == runtimeType&&other is _VeilidStateAttachment&&(identical(other.state, state) || other.state == state)&&(identical(other.publicInternetReady, publicInternetReady) || other.publicInternetReady == publicInternetReady)&&(identical(other.localNetworkReady, localNetworkReady) || other.localNetworkReady == localNetworkReady)&&(identical(other.uptime, uptime) || other.uptime == uptime)&&(identical(other.attachedUptime, attachedUptime) || other.attachedUptime == attachedUptime)&&(identical(other.reliablePeerCount, reliablePeerCount) || other.reliablePeerCount == reliablePeerCount)&&(identical(other.livePeerCount, livePeerCount) || other.livePeerCount == livePeerCount)&&(identical(other.estimatedNetworkSize, estimatedNetworkSize) || other.estimatedNetworkSize == estimatedNetworkSize)&&(identical(other.medianLatency, medianLatency) || other.medianLatency == medianLatency)&&(identical(other.overAttachedNodes, overAttachedNodes) || other.overAttachedNodes == overAttachedNodes));
}

@JsonKey(includeFromJson: false, includeToJson: false)
@override
int get hashCode => Object.hash(runtimeType,state,publicInternetReady,localNetworkReady,uptime,attachedUptime,reliablePeerCount,livePeerCount,estimatedNetworkSize,medianLatency,overAttachedNodes);

@override
String toString() {
  return 'VeilidStateAttachment(state: $state, publicInternetReady: $publicInternetReady, localNetworkReady: $localNetworkReady, uptime: $uptime, attachedUptime: $attachedUptime, reliablePeerCount: $reliablePeerCount, livePeerCount: $livePeerCount, estimatedNetworkSize: $estimatedNetworkSize, medianLatency: $medianLatency, overAttachedNodes: $overAttachedNodes)';
}


}

/// @nodoc
abstract mixin class _$VeilidStateAttachmentCopyWith<$Res> implements $VeilidStateAttachmentCopyWith<$Res> {
  factory _$VeilidStateAttachmentCopyWith(_VeilidStateAttachment value, $Res Function(_VeilidStateAttachment) _then) = __$VeilidStateAttachmentCopyWithImpl;
@override @useResult
$Res call({
 AttachmentState state, bool publicInternetReady, bool localNetworkReady, TimestampDuration uptime, TimestampDuration? attachedUptime, BigInt reliablePeerCount, BigInt livePeerCount, BigInt estimatedNetworkSize, TimestampDuration? medianLatency, BigInt overAttachedNodes
});




}
/// @nodoc
class __$VeilidStateAttachmentCopyWithImpl<$Res>
    implements _$VeilidStateAttachmentCopyWith<$Res> {
  __$VeilidStateAttachmentCopyWithImpl(this._self, this._then);

  final _VeilidStateAttachment _self;
  final $Res Function(_VeilidStateAttachment) _then;

/// Create a copy of VeilidStateAttachment
/// with the given fields replaced by the non-null parameter values.
@override @pragma('vm:prefer-inline') $Res call({Object? state = null,Object? publicInternetReady = null,Object? localNetworkReady = null,Object? uptime = null,Object? attachedUptime = freezed,Object? reliablePeerCount = null,Object? livePeerCount = null,Object? estimatedNetworkSize = null,Object? medianLatency = freezed,Object? overAttachedNodes = null,}) {
  return _then(_VeilidStateAttachment(
state: null == state ? _self.state : state // ignore: cast_nullable_to_non_nullable
as AttachmentState,publicInternetReady: null == publicInternetReady ? _self.publicInternetReady : publicInternetReady // ignore: cast_nullable_to_non_nullable
as bool,localNetworkReady: null == localNetworkReady ? _self.localNetworkReady : localNetworkReady // ignore: cast_nullable_to_non_nullable
as bool,uptime: null == uptime ? _self.uptime : uptime // ignore: cast_nullable_to_non_nullable
as TimestampDuration,attachedUptime: freezed == attachedUptime ? _self.attachedUptime : attachedUptime // ignore: cast_nullable_to_non_nullable
as TimestampDuration?,reliablePeerCount: null == reliablePeerCount ? _self.reliablePeerCount : reliablePeerCount // ignore: cast_nullable_to_non_nullable
as BigInt,livePeerCount: null == livePeerCount ? _self.livePeerCount : livePeerCount // ignore: cast_nullable_to_non_nullable
as BigInt,estimatedNetworkSize: null == estimatedNetworkSize ? _self.estimatedNetworkSize : estimatedNetworkSize // ignore: cast_nullable_to_non_nullable
as BigInt,medianLatency: freezed == medianLatency ? _self.medianLatency : medianLatency // ignore: cast_nullable_to_non_nullable
as TimestampDuration?,overAttachedNodes: null == overAttachedNodes ? _self.overAttachedNodes : overAttachedNodes // ignore: cast_nullable_to_non_nullable
as BigInt,
  ));
}


}


/// @nodoc
mixin _$VeilidStateNetwork {

 bool get started; BigInt get bpsDown; BigInt get bpsUp; List<PeerTableData> get peers;
/// Create a copy of VeilidStateNetwork
/// with the given fields replaced by the non-null parameter values.
@JsonKey(includeFromJson: false, includeToJson: false)
@pragma('vm:prefer-inline')
$VeilidStateNetworkCopyWith<VeilidStateNetwork> get copyWith => _$VeilidStateNetworkCopyWithImpl<VeilidStateNetwork>(this as VeilidStateNetwork, _$identity);

  /// Serializes this VeilidStateNetwork to a JSON map.
  Map<String, dynamic> toJson();


@override
bool operator ==(Object other) {
  return identical(this, other) || (other.runtimeType == runtimeType&&other is VeilidStateNetwork&&(identical(other.started, started) || other.started == started)&&(identical(other.bpsDown, bpsDown) || other.bpsDown == bpsDown)&&(identical(other.bpsUp, bpsUp) || other.bpsUp == bpsUp)&&const DeepCollectionEquality().equals(other.peers, peers));
}

@JsonKey(includeFromJson: false, includeToJson: false)
@override
int get hashCode => Object.hash(runtimeType,started,bpsDown,bpsUp,const DeepCollectionEquality().hash(peers));

@override
String toString() {
  return 'VeilidStateNetwork(started: $started, bpsDown: $bpsDown, bpsUp: $bpsUp, peers: $peers)';
}


}

/// @nodoc
abstract mixin class $VeilidStateNetworkCopyWith<$Res>  {
  factory $VeilidStateNetworkCopyWith(VeilidStateNetwork value, $Res Function(VeilidStateNetwork) _then) = _$VeilidStateNetworkCopyWithImpl;
@useResult
$Res call({
 bool started, BigInt bpsDown, BigInt bpsUp, List<PeerTableData> peers
});




}
/// @nodoc
class _$VeilidStateNetworkCopyWithImpl<$Res>
    implements $VeilidStateNetworkCopyWith<$Res> {
  _$VeilidStateNetworkCopyWithImpl(this._self, this._then);

  final VeilidStateNetwork _self;
  final $Res Function(VeilidStateNetwork) _then;

/// Create a copy of VeilidStateNetwork
/// with the given fields replaced by the non-null parameter values.
@pragma('vm:prefer-inline') @override $Res call({Object? started = null,Object? bpsDown = null,Object? bpsUp = null,Object? peers = null,}) {
  return _then(_self.copyWith(
started: null == started ? _self.started : started // ignore: cast_nullable_to_non_nullable
as bool,bpsDown: null == bpsDown ? _self.bpsDown : bpsDown // ignore: cast_nullable_to_non_nullable
as BigInt,bpsUp: null == bpsUp ? _self.bpsUp : bpsUp // ignore: cast_nullable_to_non_nullable
as BigInt,peers: null == peers ? _self.peers : peers // ignore: cast_nullable_to_non_nullable
as List<PeerTableData>,
  ));
}

}


/// Adds pattern-matching-related methods to [VeilidStateNetwork].
extension VeilidStateNetworkPatterns on VeilidStateNetwork {
/// A variant of `map` that fallback to returning `orElse`.
///
/// It is equivalent to doing:
/// ```dart
/// switch (sealedClass) {
///   case final Subclass value:
///     return ...;
///   case _:
///     return orElse();
/// }
/// ```

@optionalTypeArgs TResult maybeMap<TResult extends Object?>(TResult Function( _VeilidStateNetwork value)?  $default,{required TResult orElse(),}){
final _that = this;
switch (_that) {
case _VeilidStateNetwork() when $default != null:
return $default(_that);case _:
  return orElse();

}
}
/// A `switch`-like method, using callbacks.
///
/// Callbacks receives the raw object, upcasted.
/// It is equivalent to doing:
/// ```dart
/// switch (sealedClass) {
///   case final Subclass value:
///     return ...;
///   case final Subclass2 value:
///     return ...;
/// }
/// ```

@optionalTypeArgs TResult map<TResult extends Object?>(TResult Function( _VeilidStateNetwork value)  $default,){
final _that = this;
switch (_that) {
case _VeilidStateNetwork():
return $default(_that);}
}
/// A variant of `map` that fallback to returning `null`.
///
/// It is equivalent to doing:
/// ```dart
/// switch (sealedClass) {
///   case final Subclass value:
///     return ...;
///   case _:
///     return null;
/// }
/// ```

@optionalTypeArgs TResult? mapOrNull<TResult extends Object?>(TResult? Function( _VeilidStateNetwork value)?  $default,){
final _that = this;
switch (_that) {
case _VeilidStateNetwork() when $default != null:
return $default(_that);case _:
  return null;

}
}
/// A variant of `when` that fallback to an `orElse` callback.
///
/// It is equivalent to doing:
/// ```dart
/// switch (sealedClass) {
///   case Subclass(:final field):
///     return ...;
///   case _:
///     return orElse();
/// }
/// ```

@optionalTypeArgs TResult maybeWhen<TResult extends Object?>(TResult Function( bool started,  BigInt bpsDown,  BigInt bpsUp,  List<PeerTableData> peers)?  $default,{required TResult orElse(),}) {final _that = this;
switch (_that) {
case _VeilidStateNetwork() when $default != null:
return $default(_that.started,_that.bpsDown,_that.bpsUp,_that.peers);case _:
  return orElse();

}
}
/// A `switch`-like method, using callbacks.
///
/// As opposed to `map`, this offers destructuring.
/// It is equivalent to doing:
/// ```dart
/// switch (sealedClass) {
///   case Subclass(:final field):
///     return ...;
///   case Subclass2(:final field2):
///     return ...;
/// }
/// ```

@optionalTypeArgs TResult when<TResult extends Object?>(TResult Function( bool started,  BigInt bpsDown,  BigInt bpsUp,  List<PeerTableData> peers)  $default,) {final _that = this;
switch (_that) {
case _VeilidStateNetwork():
return $default(_that.started,_that.bpsDown,_that.bpsUp,_that.peers);}
}
/// A variant of `when` that fallback to returning `null`
///
/// It is equivalent to doing:
/// ```dart
/// switch (sealedClass) {
///   case Subclass(:final field):
///     return ...;
///   case _:
///     return null;
/// }
/// ```

@optionalTypeArgs TResult? whenOrNull<TResult extends Object?>(TResult? Function( bool started,  BigInt bpsDown,  BigInt bpsUp,  List<PeerTableData> peers)?  $default,) {final _that = this;
switch (_that) {
case _VeilidStateNetwork() when $default != null:
return $default(_that.started,_that.bpsDown,_that.bpsUp,_that.peers);case _:
  return null;

}
}

}

/// @nodoc
@JsonSerializable()

class _VeilidStateNetwork implements VeilidStateNetwork {
  const _VeilidStateNetwork({required this.started, required this.bpsDown, required this.bpsUp, required final  List<PeerTableData> peers}): _peers = peers;
  factory _VeilidStateNetwork.fromJson(Map<String, dynamic> json) => _$VeilidStateNetworkFromJson(json);

@override final  bool started;
@override final  BigInt bpsDown;
@override final  BigInt bpsUp;
 final  List<PeerTableData> _peers;
@override List<PeerTableData> get peers {
  if (_peers is EqualUnmodifiableListView) return _peers;
  // ignore: implicit_dynamic_type
  return EqualUnmodifiableListView(_peers);
}


/// Create a copy of VeilidStateNetwork
/// with the given fields replaced by the non-null parameter values.
@override @JsonKey(includeFromJson: false, includeToJson: false)
@pragma('vm:prefer-inline')
_$VeilidStateNetworkCopyWith<_VeilidStateNetwork> get copyWith => __$VeilidStateNetworkCopyWithImpl<_VeilidStateNetwork>(this, _$identity);

@override
Map<String, dynamic> toJson() {
  return _$VeilidStateNetworkToJson(this, );
}

@override
bool operator ==(Object other) {
  return identical(this, other) || (other.runtimeType == runtimeType&&other is _VeilidStateNetwork&&(identical(other.started, started) || other.started == started)&&(identical(other.bpsDown, bpsDown) || other.bpsDown == bpsDown)&&(identical(other.bpsUp, bpsUp) || other.bpsUp == bpsUp)&&const DeepCollectionEquality().equals(other._peers, _peers));
}

@JsonKey(includeFromJson: false, includeToJson: false)
@override
int get hashCode => Object.hash(runtimeType,started,bpsDown,bpsUp,const DeepCollectionEquality().hash(_peers));

@override
String toString() {
  return 'VeilidStateNetwork(started: $started, bpsDown: $bpsDown, bpsUp: $bpsUp, peers: $peers)';
}


}

/// @nodoc
abstract mixin class _$VeilidStateNetworkCopyWith<$Res> implements $VeilidStateNetworkCopyWith<$Res> {
  factory _$VeilidStateNetworkCopyWith(_VeilidStateNetwork value, $Res Function(_VeilidStateNetwork) _then) = __$VeilidStateNetworkCopyWithImpl;
@override @useResult
$Res call({
 bool started, BigInt bpsDown, BigInt bpsUp, List<PeerTableData> peers
});




}
/// @nodoc
class __$VeilidStateNetworkCopyWithImpl<$Res>
    implements _$VeilidStateNetworkCopyWith<$Res> {
  __$VeilidStateNetworkCopyWithImpl(this._self, this._then);

  final _VeilidStateNetwork _self;
  final $Res Function(_VeilidStateNetwork) _then;

/// Create a copy of VeilidStateNetwork
/// with the given fields replaced by the non-null parameter values.
@override @pragma('vm:prefer-inline') $Res call({Object? started = null,Object? bpsDown = null,Object? bpsUp = null,Object? peers = null,}) {
  return _then(_VeilidStateNetwork(
started: null == started ? _self.started : started // ignore: cast_nullable_to_non_nullable
as bool,bpsDown: null == bpsDown ? _self.bpsDown : bpsDown // ignore: cast_nullable_to_non_nullable
as BigInt,bpsUp: null == bpsUp ? _self.bpsUp : bpsUp // ignore: cast_nullable_to_non_nullable
as BigInt,peers: null == peers ? _self._peers : peers // ignore: cast_nullable_to_non_nullable
as List<PeerTableData>,
  ));
}


}


/// @nodoc
mixin _$VeilidStateConfig {

 VeilidConfig get config;
/// Create a copy of VeilidStateConfig
/// with the given fields replaced by the non-null parameter values.
@JsonKey(includeFromJson: false, includeToJson: false)
@pragma('vm:prefer-inline')
$VeilidStateConfigCopyWith<VeilidStateConfig> get copyWith => _$VeilidStateConfigCopyWithImpl<VeilidStateConfig>(this as VeilidStateConfig, _$identity);

  /// Serializes this VeilidStateConfig to a JSON map.
  Map<String, dynamic> toJson();


@override
bool operator ==(Object other) {
  return identical(this, other) || (other.runtimeType == runtimeType&&other is VeilidStateConfig&&(identical(other.config, config) || other.config == config));
}

@JsonKey(includeFromJson: false, includeToJson: false)
@override
int get hashCode => Object.hash(runtimeType,config);

@override
String toString() {
  return 'VeilidStateConfig(config: $config)';
}


}

/// @nodoc
abstract mixin class $VeilidStateConfigCopyWith<$Res>  {
  factory $VeilidStateConfigCopyWith(VeilidStateConfig value, $Res Function(VeilidStateConfig) _then) = _$VeilidStateConfigCopyWithImpl;
@useResult
$Res call({
 VeilidConfig config
});


$VeilidConfigCopyWith<$Res> get config;

}
/// @nodoc
class _$VeilidStateConfigCopyWithImpl<$Res>
    implements $VeilidStateConfigCopyWith<$Res> {
  _$VeilidStateConfigCopyWithImpl(this._self, this._then);

  final VeilidStateConfig _self;
  final $Res Function(VeilidStateConfig) _then;

/// Create a copy of VeilidStateConfig
/// with the given fields replaced by the non-null parameter values.
@pragma('vm:prefer-inline') @override $Res call({Object? config = null,}) {
  return _then(_self.copyWith(
config: null == config ? _self.config : config // ignore: cast_nullable_to_non_nullable
as VeilidConfig,
  ));
}
/// Create a copy of VeilidStateConfig
/// with the given fields replaced by the non-null parameter values.
@override
@pragma('vm:prefer-inline')
$VeilidConfigCopyWith<$Res> get config {
  
  return $VeilidConfigCopyWith<$Res>(_self.config, (value) {
    return _then(_self.copyWith(config: value));
  });
}
}


/// Adds pattern-matching-related methods to [VeilidStateConfig].
extension VeilidStateConfigPatterns on VeilidStateConfig {
/// A variant of `map` that fallback to returning `orElse`.
///
/// It is equivalent to doing:
/// ```dart
/// switch (sealedClass) {
///   case final Subclass value:
///     return ...;
///   case _:
///     return orElse();
/// }
/// ```

@optionalTypeArgs TResult maybeMap<TResult extends Object?>(TResult Function( _VeilidStateConfig value)?  $default,{required TResult orElse(),}){
final _that = this;
switch (_that) {
case _VeilidStateConfig() when $default != null:
return $default(_that);case _:
  return orElse();

}
}
/// A `switch`-like method, using callbacks.
///
/// Callbacks receives the raw object, upcasted.
/// It is equivalent to doing:
/// ```dart
/// switch (sealedClass) {
///   case final Subclass value:
///     return ...;
///   case final Subclass2 value:
///     return ...;
/// }
/// ```

@optionalTypeArgs TResult map<TResult extends Object?>(TResult Function( _VeilidStateConfig value)  $default,){
final _that = this;
switch (_that) {
case _VeilidStateConfig():
return $default(_that);}
}
/// A variant of `map` that fallback to returning `null`.
///
/// It is equivalent to doing:
/// ```dart
/// switch (sealedClass) {
///   case final Subclass value:
///     return ...;
///   case _:
///     return null;
/// }
/// ```

@optionalTypeArgs TResult? mapOrNull<TResult extends Object?>(TResult? Function( _VeilidStateConfig value)?  $default,){
final _that = this;
switch (_that) {
case _VeilidStateConfig() when $default != null:
return $default(_that);case _:
  return null;

}
}
/// A variant of `when` that fallback to an `orElse` callback.
///
/// It is equivalent to doing:
/// ```dart
/// switch (sealedClass) {
///   case Subclass(:final field):
///     return ...;
///   case _:
///     return orElse();
/// }
/// ```

@optionalTypeArgs TResult maybeWhen<TResult extends Object?>(TResult Function( VeilidConfig config)?  $default,{required TResult orElse(),}) {final _that = this;
switch (_that) {
case _VeilidStateConfig() when $default != null:
return $default(_that.config);case _:
  return orElse();

}
}
/// A `switch`-like method, using callbacks.
///
/// As opposed to `map`, this offers destructuring.
/// It is equivalent to doing:
/// ```dart
/// switch (sealedClass) {
///   case Subclass(:final field):
///     return ...;
///   case Subclass2(:final field2):
///     return ...;
/// }
/// ```

@optionalTypeArgs TResult when<TResult extends Object?>(TResult Function( VeilidConfig config)  $default,) {final _that = this;
switch (_that) {
case _VeilidStateConfig():
return $default(_that.config);}
}
/// A variant of `when` that fallback to returning `null`
///
/// It is equivalent to doing:
/// ```dart
/// switch (sealedClass) {
///   case Subclass(:final field):
///     return ...;
///   case _:
///     return null;
/// }
/// ```

@optionalTypeArgs TResult? whenOrNull<TResult extends Object?>(TResult? Function( VeilidConfig config)?  $default,) {final _that = this;
switch (_that) {
case _VeilidStateConfig() when $default != null:
return $default(_that.config);case _:
  return null;

}
}

}

/// @nodoc
@JsonSerializable()

class _VeilidStateConfig implements VeilidStateConfig {
  const _VeilidStateConfig({required this.config});
  factory _VeilidStateConfig.fromJson(Map<String, dynamic> json) => _$VeilidStateConfigFromJson(json);

@override final  VeilidConfig config;

/// Create a copy of VeilidStateConfig
/// with the given fields replaced by the non-null parameter values.
@override @JsonKey(includeFromJson: false, includeToJson: false)
@pragma('vm:prefer-inline')
_$VeilidStateConfigCopyWith<_VeilidStateConfig> get copyWith => __$VeilidStateConfigCopyWithImpl<_VeilidStateConfig>(this, _$identity);

@override
Map<String, dynamic> toJson() {
  return _$VeilidStateConfigToJson(this, );
}

@override
bool operator ==(Object other) {
  return identical(this, other) || (other.runtimeType == runtimeType&&other is _VeilidStateConfig&&(identical(other.config, config) || other.config == config));
}

@JsonKey(includeFromJson: false, includeToJson: false)
@override
int get hashCode => Object.hash(runtimeType,config);

@override
String toString() {
  return 'VeilidStateConfig(config: $config)';
}


}

/// @nodoc
abstract mixin class _$VeilidStateConfigCopyWith<$Res> implements $VeilidStateConfigCopyWith<$Res> {
  factory _$VeilidStateConfigCopyWith(_VeilidStateConfig value, $Res Function(_VeilidStateConfig) _then) = __$VeilidStateConfigCopyWithImpl;
@override @useResult
$Res call({
 VeilidConfig config
});


@override $VeilidConfigCopyWith<$Res> get config;

}
/// @nodoc
class __$VeilidStateConfigCopyWithImpl<$Res>
    implements _$VeilidStateConfigCopyWith<$Res> {
  __$VeilidStateConfigCopyWithImpl(this._self, this._then);

  final _VeilidStateConfig _self;
  final $Res Function(_VeilidStateConfig) _then;

/// Create a copy of VeilidStateConfig
/// with the given fields replaced by the non-null parameter values.
@override @pragma('vm:prefer-inline') $Res call({Object? config = null,}) {
  return _then(_VeilidStateConfig(
config: null == config ? _self.config : config // ignore: cast_nullable_to_non_nullable
as VeilidConfig,
  ));
}

/// Create a copy of VeilidStateConfig
/// with the given fields replaced by the non-null parameter values.
@override
@pragma('vm:prefer-inline')
$VeilidConfigCopyWith<$Res> get config {
  
  return $VeilidConfigCopyWith<$Res>(_self.config, (value) {
    return _then(_self.copyWith(config: value));
  });
}
}


/// @nodoc
mixin _$VeilidState {

 VeilidStateAttachment get attachment; VeilidStateNetwork get network; VeilidStateConfig get config;
/// Create a copy of VeilidState
/// with the given fields replaced by the non-null parameter values.
@JsonKey(includeFromJson: false, includeToJson: false)
@pragma('vm:prefer-inline')
$VeilidStateCopyWith<VeilidState> get copyWith => _$VeilidStateCopyWithImpl<VeilidState>(this as VeilidState, _$identity);

  /// Serializes this VeilidState to a JSON map.
  Map<String, dynamic> toJson();


@override
bool operator ==(Object other) {
  return identical(this, other) || (other.runtimeType == runtimeType&&other is VeilidState&&(identical(other.attachment, attachment) || other.attachment == attachment)&&(identical(other.network, network) || other.network == network)&&(identical(other.config, config) || other.config == config));
}

@JsonKey(includeFromJson: false, includeToJson: false)
@override
int get hashCode => Object.hash(runtimeType,attachment,network,config);

@override
String toString() {
  return 'VeilidState(attachment: $attachment, network: $network, config: $config)';
}


}

/// @nodoc
abstract mixin class $VeilidStateCopyWith<$Res>  {
  factory $VeilidStateCopyWith(VeilidState value, $Res Function(VeilidState) _then) = _$VeilidStateCopyWithImpl;
@useResult
$Res call({
 VeilidStateAttachment attachment, VeilidStateNetwork network, VeilidStateConfig config
});


$VeilidStateAttachmentCopyWith<$Res> get attachment;$VeilidStateNetworkCopyWith<$Res> get network;$VeilidStateConfigCopyWith<$Res> get config;

}
/// @nodoc
class _$VeilidStateCopyWithImpl<$Res>
    implements $VeilidStateCopyWith<$Res> {
  _$VeilidStateCopyWithImpl(this._self, this._then);

  final VeilidState _self;
  final $Res Function(VeilidState) _then;

/// Create a copy of VeilidState
/// with the given fields replaced by the non-null parameter values.
@pragma('vm:prefer-inline') @override $Res call({Object? attachment = null,Object? network = null,Object? config = null,}) {
  return _then(_self.copyWith(
attachment: null == attachment ? _self.attachment : attachment // ignore: cast_nullable_to_non_nullable
as VeilidStateAttachment,network: null == network ? _self.network : network // ignore: cast_nullable_to_non_nullable
as VeilidStateNetwork,config: null == config ? _self.config : config // ignore: cast_nullable_to_non_nullable
as VeilidStateConfig,
  ));
}
/// Create a copy of VeilidState
/// with the given fields replaced by the non-null parameter values.
@override
@pragma('vm:prefer-inline')
$VeilidStateAttachmentCopyWith<$Res> get attachment {
  
  return $VeilidStateAttachmentCopyWith<$Res>(_self.attachment, (value) {
    return _then(_self.copyWith(attachment: value));
  });
}/// Create a copy of VeilidState
/// with the given fields replaced by the non-null parameter values.
@override
@pragma('vm:prefer-inline')
$VeilidStateNetworkCopyWith<$Res> get network {
  
  return $VeilidStateNetworkCopyWith<$Res>(_self.network, (value) {
    return _then(_self.copyWith(network: value));
  });
}/// Create a copy of VeilidState
/// with the given fields replaced by the non-null parameter values.
@override
@pragma('vm:prefer-inline')
$VeilidStateConfigCopyWith<$Res> get config {
  
  return $VeilidStateConfigCopyWith<$Res>(_self.config, (value) {
    return _then(_self.copyWith(config: value));
  });
}
}


/// Adds pattern-matching-related methods to [VeilidState].
extension VeilidStatePatterns on VeilidState {
/// A variant of `map` that fallback to returning `orElse`.
///
/// It is equivalent to doing:
/// ```dart
/// switch (sealedClass) {
///   case final Subclass value:
///     return ...;
///   case _:
///     return orElse();
/// }
/// ```

@optionalTypeArgs TResult maybeMap<TResult extends Object?>(TResult Function( _VeilidState value)?  $default,{required TResult orElse(),}){
final _that = this;
switch (_that) {
case _VeilidState() when $default != null:
return $default(_that);case _:
  return orElse();

}
}
/// A `switch`-like method, using callbacks.
///
/// Callbacks receives the raw object, upcasted.
/// It is equivalent to doing:
/// ```dart
/// switch (sealedClass) {
///   case final Subclass value:
///     return ...;
///   case final Subclass2 value:
///     return ...;
/// }
/// ```

@optionalTypeArgs TResult map<TResult extends Object?>(TResult Function( _VeilidState value)  $default,){
final _that = this;
switch (_that) {
case _VeilidState():
return $default(_that);}
}
/// A variant of `map` that fallback to returning `null`.
///
/// It is equivalent to doing:
/// ```dart
/// switch (sealedClass) {
///   case final Subclass value:
///     return ...;
///   case _:
///     return null;
/// }
/// ```

@optionalTypeArgs TResult? mapOrNull<TResult extends Object?>(TResult? Function( _VeilidState value)?  $default,){
final _that = this;
switch (_that) {
case _VeilidState() when $default != null:
return $default(_that);case _:
  return null;

}
}
/// A variant of `when` that fallback to an `orElse` callback.
///
/// It is equivalent to doing:
/// ```dart
/// switch (sealedClass) {
///   case Subclass(:final field):
///     return ...;
///   case _:
///     return orElse();
/// }
/// ```

@optionalTypeArgs TResult maybeWhen<TResult extends Object?>(TResult Function( VeilidStateAttachment attachment,  VeilidStateNetwork network,  VeilidStateConfig config)?  $default,{required TResult orElse(),}) {final _that = this;
switch (_that) {
case _VeilidState() when $default != null:
return $default(_that.attachment,_that.network,_that.config);case _:
  return orElse();

}
}
/// A `switch`-like method, using callbacks.
///
/// As opposed to `map`, this offers destructuring.
/// It is equivalent to doing:
/// ```dart
/// switch (sealedClass) {
///   case Subclass(:final field):
///     return ...;
///   case Subclass2(:final field2):
///     return ...;
/// }
/// ```

@optionalTypeArgs TResult when<TResult extends Object?>(TResult Function( VeilidStateAttachment attachment,  VeilidStateNetwork network,  VeilidStateConfig config)  $default,) {final _that = this;
switch (_that) {
case _VeilidState():
return $default(_that.attachment,_that.network,_that.config);}
}
/// A variant of `when` that fallback to returning `null`
///
/// It is equivalent to doing:
/// ```dart
/// switch (sealedClass) {
///   case Subclass(:final field):
///     return ...;
///   case _:
///     return null;
/// }
/// ```

@optionalTypeArgs TResult? whenOrNull<TResult extends Object?>(TResult? Function( VeilidStateAttachment attachment,  VeilidStateNetwork network,  VeilidStateConfig config)?  $default,) {final _that = this;
switch (_that) {
case _VeilidState() when $default != null:
return $default(_that.attachment,_that.network,_that.config);case _:
  return null;

}
}

}

/// @nodoc
@JsonSerializable()

class _VeilidState implements VeilidState {
  const _VeilidState({required this.attachment, required this.network, required this.config});
  factory _VeilidState.fromJson(Map<String, dynamic> json) => _$VeilidStateFromJson(json);

@override final  VeilidStateAttachment attachment;
@override final  VeilidStateNetwork network;
@override final  VeilidStateConfig config;

/// Create a copy of VeilidState
/// with the given fields replaced by the non-null parameter values.
@override @JsonKey(includeFromJson: false, includeToJson: false)
@pragma('vm:prefer-inline')
_$VeilidStateCopyWith<_VeilidState> get copyWith => __$VeilidStateCopyWithImpl<_VeilidState>(this, _$identity);

@override
Map<String, dynamic> toJson() {
  return _$VeilidStateToJson(this, );
}

@override
bool operator ==(Object other) {
  return identical(this, other) || (other.runtimeType == runtimeType&&other is _VeilidState&&(identical(other.attachment, attachment) || other.attachment == attachment)&&(identical(other.network, network) || other.network == network)&&(identical(other.config, config) || other.config == config));
}

@JsonKey(includeFromJson: false, includeToJson: false)
@override
int get hashCode => Object.hash(runtimeType,attachment,network,config);

@override
String toString() {
  return 'VeilidState(attachment: $attachment, network: $network, config: $config)';
}


}

/// @nodoc
abstract mixin class _$VeilidStateCopyWith<$Res> implements $VeilidStateCopyWith<$Res> {
  factory _$VeilidStateCopyWith(_VeilidState value, $Res Function(_VeilidState) _then) = __$VeilidStateCopyWithImpl;
@override @useResult
$Res call({
 VeilidStateAttachment attachment, VeilidStateNetwork network, VeilidStateConfig config
});


@override $VeilidStateAttachmentCopyWith<$Res> get attachment;@override $VeilidStateNetworkCopyWith<$Res> get network;@override $VeilidStateConfigCopyWith<$Res> get config;

}
/// @nodoc
class __$VeilidStateCopyWithImpl<$Res>
    implements _$VeilidStateCopyWith<$Res> {
  __$VeilidStateCopyWithImpl(this._self, this._then);

  final _VeilidState _self;
  final $Res Function(_VeilidState) _then;

/// Create a copy of VeilidState
/// with the given fields replaced by the non-null parameter values.
@override @pragma('vm:prefer-inline') $Res call({Object? attachment = null,Object? network = null,Object? config = null,}) {
  return _then(_VeilidState(
attachment: null == attachment ? _self.attachment : attachment // ignore: cast_nullable_to_non_nullable
as VeilidStateAttachment,network: null == network ? _self.network : network // ignore: cast_nullable_to_non_nullable
as VeilidStateNetwork,config: null == config ? _self.config : config // ignore: cast_nullable_to_non_nullable
as VeilidStateConfig,
  ));
}

/// Create a copy of VeilidState
/// with the given fields replaced by the non-null parameter values.
@override
@pragma('vm:prefer-inline')
$VeilidStateAttachmentCopyWith<$Res> get attachment {
  
  return $VeilidStateAttachmentCopyWith<$Res>(_self.attachment, (value) {
    return _then(_self.copyWith(attachment: value));
  });
}/// Create a copy of VeilidState
/// with the given fields replaced by the non-null parameter values.
@override
@pragma('vm:prefer-inline')
$VeilidStateNetworkCopyWith<$Res> get network {
  
  return $VeilidStateNetworkCopyWith<$Res>(_self.network, (value) {
    return _then(_self.copyWith(network: value));
  });
}/// Create a copy of VeilidState
/// with the given fields replaced by the non-null parameter values.
@override
@pragma('vm:prefer-inline')
$VeilidStateConfigCopyWith<$Res> get config {
  
  return $VeilidStateConfigCopyWith<$Res>(_self.config, (value) {
    return _then(_self.copyWith(config: value));
  });
}
}

// dart format on
