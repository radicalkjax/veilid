import 'package:freezed_annotation/freezed_annotation.dart';

//////////////////////////////////////
/// VeilidAPIException

/// Base type for all exceptions thrown by the Veilid API. Each subtype
/// corresponds to a variant of the Rust `VeilidAPIError`.
@immutable
abstract class VeilidAPIException implements Exception {
  /// Build the concrete exception subtype from its serialized `kind` tag.
  ///
  /// Throws [VeilidAPIExceptionInternal] if `kind` is not a recognized variant.
  factory VeilidAPIException.fromJson(dynamic j) {
    final json = j as Map<String, dynamic>;
    switch (json['kind']! as String) {
      case 'NotInitialized':
        {
          return VeilidAPIExceptionNotInitialized();
        }
      case 'AlreadyInitialized':
        {
          return VeilidAPIExceptionAlreadyInitialized();
        }
      case 'Timeout':
        {
          return VeilidAPIExceptionTimeout();
        }
      case 'TryAgain':
        {
          return VeilidAPIExceptionTryAgain(json['message']! as String);
        }
      case 'Shutdown':
        {
          return VeilidAPIExceptionShutdown();
        }
      case 'InvalidTarget':
        {
          return VeilidAPIExceptionInvalidTarget(json['message']! as String);
        }
      case 'NoConnection':
        {
          return VeilidAPIExceptionNoConnection(json['message']! as String);
        }
      case 'KeyNotFound':
        {
          return VeilidAPIExceptionKeyNotFound(json['key']! as String);
        }
      case 'Internal':
        {
          return VeilidAPIExceptionInternal(json['message']! as String);
        }
      case 'Unimplemented':
        {
          return VeilidAPIExceptionUnimplemented(
              json['unimplemented']! as String);
        }
      case 'ParseError':
        {
          return VeilidAPIExceptionParseError(
              json['message']! as String, json['value']! as String);
        }
      case 'InvalidArgument':
        {
          return VeilidAPIExceptionInvalidArgument(json['context']! as String,
              json['argument']! as String, json['value']! as String);
        }
      case 'MissingArgument':
        {
          return VeilidAPIExceptionMissingArgument(
              json['context']! as String, json['argument']! as String);
        }
      case 'Generic':
        {
          return VeilidAPIExceptionGeneric(json['message']! as String);
        }
      case 'TransactionNotFound':
        {
          return VeilidAPIExceptionTransactionNotFound(
              json['message']! as String);
        }
      default:
        {
          throw VeilidAPIExceptionInternal(
              "Invalid VeilidAPIException type: ${json['kind']! as String}");
        }
    }
  }

  /// Human-readable message suitable for display to an end user.
  String toDisplayError();
}

/// Thrown when the API is used before Veilid was started or after it detached.
@immutable
class VeilidAPIExceptionNotInitialized implements VeilidAPIException {
  @override
  String toString() => 'VeilidAPIException: NotInitialized';

  @override
  String toDisplayError() => 'Not initialized';
}

/// Thrown when Veilid is initialized while it is already running.
@immutable
class VeilidAPIExceptionAlreadyInitialized implements VeilidAPIException {
  @override
  String toString() => 'VeilidAPIException: AlreadyInitialized';

  @override
  String toDisplayError() => 'Already initialized';
}

/// Thrown when an operation did not complete within its time budget.
@immutable
class VeilidAPIExceptionTimeout implements VeilidAPIException {
  @override
  String toString() => 'VeilidAPIException: Timeout';

  @override
  String toDisplayError() => 'Timeout';
}

/// Thrown when an operation could not complete yet and should be retried later.
@immutable
class VeilidAPIExceptionTryAgain implements VeilidAPIException {
  /// Why the operation could not complete this time.
  final String message;

  /// Construct with the reason the operation could not complete.
  const VeilidAPIExceptionTryAgain(this.message);

  @override
  String toString() => 'VeilidAPIException: TryAgain (message: $message)';

  @override
  String toDisplayError() => 'Try again: (message: $message)';
}

/// Thrown when the API is shutting down and can no longer service requests.
@immutable
class VeilidAPIExceptionShutdown implements VeilidAPIException {
  @override
  String toString() => 'VeilidAPIException: Shutdown';

  @override
  String toDisplayError() => 'Currently shut down';
}

/// Thrown when the destination for an operation is unreachable or malformed.
@immutable
class VeilidAPIExceptionInvalidTarget implements VeilidAPIException {
  /// Details about the unreachable or malformed target.
  final String message;

  /// Construct with details about the bad target.
  const VeilidAPIExceptionInvalidTarget(this.message);

  @override
  String toString() => 'VeilidAPIException: InvalidTarget (message: $message)';

  @override
  String toDisplayError() => 'Invalid target: (message: $message)';
}

/// Thrown when no network connection could be established for the operation.
@immutable
class VeilidAPIExceptionNoConnection implements VeilidAPIException {
  /// Details about the connection failure.
  final String message;

  /// Construct with details about the connection failure.
  const VeilidAPIExceptionNoConnection(this.message);

  @override
  String toString() => 'VeilidAPIException: NoConnection (message: $message)';

  @override
  String toDisplayError() => 'No connection: $message';
}

/// Thrown when a requested DHT record key is not present in local storage.
@immutable
class VeilidAPIExceptionKeyNotFound implements VeilidAPIException {
  /// The record key that was not found.
  final String key;

  /// Construct with the record key that was not found.
  const VeilidAPIExceptionKeyNotFound(this.key);

  @override
  String toString() => 'VeilidAPIException: KeyNotFound (key: $key)';

  @override
  String toDisplayError() => 'Key not found: $key';
}

/// Thrown when an internal invariant was violated, indicating a bug in Veilid.
@immutable
class VeilidAPIExceptionInternal implements VeilidAPIException {
  /// Details about the internal failure.
  final String message;

  /// Construct with details about the internal failure.
  const VeilidAPIExceptionInternal(this.message);

  @override
  String toString() => 'VeilidAPIException: Internal ($message)';

  @override
  String toDisplayError() => 'Internal error: $message';
}

/// Thrown when a feature exists in the API surface but is not implemented on
/// this platform or build.
@immutable
class VeilidAPIExceptionUnimplemented implements VeilidAPIException {
  /// Which functionality is unimplemented.
  final String message;

  /// Construct with the name of the unimplemented functionality.
  const VeilidAPIExceptionUnimplemented(this.message);

  @override
  String toString() => 'VeilidAPIException: Unimplemented ($message)';

  @override
  String toDisplayError() => 'Unimplemented: $message';
}

/// Thrown when a value could not be parsed into its expected form.
@immutable
class VeilidAPIExceptionParseError implements VeilidAPIException {
  /// What went wrong while parsing.
  final String message;

  /// The input value that failed to parse.
  final String value;

  /// Construct with the parse failure reason and the offending value.
  const VeilidAPIExceptionParseError(this.message, this.value);

  @override
  String toString() =>
      'VeilidAPIException: ParseError ($message)\n    value: $value';

  @override
  String toDisplayError() => 'Parse error: $message';
}

/// Thrown when an argument was supplied but its value was rejected.
@immutable
class VeilidAPIExceptionInvalidArgument implements VeilidAPIException {
  /// The calling context that rejected the argument.
  final String context;

  /// The name of the offending argument.
  final String argument;

  /// The rejected value.
  final String value;

  /// Construct with the calling context, argument name, and rejected value.
  const VeilidAPIExceptionInvalidArgument(
      this.context, this.argument, this.value);

  @override
  String toString() => 'VeilidAPIException: InvalidArgument'
      ' ($context:$argument)\n    value: $value';

  @override
  String toDisplayError() => 'Invalid argument for $context: $argument';
}

/// Thrown when a required argument was not supplied.
@immutable
class VeilidAPIExceptionMissingArgument implements VeilidAPIException {
  /// The calling context that required the argument.
  final String context;

  /// The name of the missing argument.
  final String argument;

  /// Construct with the calling context and the name of the missing argument.
  const VeilidAPIExceptionMissingArgument(this.context, this.argument);

  @override
  String toString() =>
      'VeilidAPIException: MissingArgument ($context:$argument)';

  @override
  String toDisplayError() => 'Missing argument for $context: $argument';
}

/// Thrown for a failure that does not fit any more specific category.
@immutable
class VeilidAPIExceptionGeneric implements VeilidAPIException {
  /// Details about the failure.
  final String message;

  /// Construct with details about the failure.
  const VeilidAPIExceptionGeneric(this.message);

  @override
  String toString() => 'VeilidAPIException: Generic (message: $message)';

  @override
  String toDisplayError() => message;
}

/// Thrown when a referenced DHT transaction does not exist, having expired or
/// never been opened.
@immutable
class VeilidAPIExceptionTransactionNotFound implements VeilidAPIException {
  /// Details about the missing transaction.
  final String message;

  /// Construct with details about the missing transaction.
  const VeilidAPIExceptionTransactionNotFound(this.message);

  @override
  String toString() =>
      'VeilidAPIException: TransactionNotFound (message: $message)';

  @override
  String toDisplayError() => 'Transaction not found: $message';
}
