// Error classes thrown across the veilid-wasm boundary.
//
// Ships at the package root and is re-exported from the package entry point, while the
// wasm glue reaches it through js/veilid_errors_bridge.js. Both routes resolve to this
// same module, which is what makes `instanceof` hold for errors raised inside wasm.
//
// The variants mirror veilid_core::VeilidAPIError; adding one there means adding a class
// here. An unrecognized kind still throws a VeilidAPIError, just not a subclass of one.

/// Base class of every error thrown by veilid-wasm.
export class VeilidAPIError extends Error {
  constructor(message, fields) {
    super(message);
    this.name = new.target.name;
    // `message` is already on Error, and non-enumerable there by spec
    const { message: _message, ...rest } = fields ?? {};
    Object.assign(this, rest);
    // Restores the prototype for consumers transpiling to ES5, where extending a
    // builtin otherwise yields a plain Error
    Object.setPrototypeOf(this, new.target.prototype);
    if (Error.captureStackTrace) {
      Error.captureStackTrace(this, VeilidAPIError);
    }
  }
}

/// The API was used before startup or after shutdown.
export class VeilidAPIErrorNotInitialized extends VeilidAPIError {}
/// Startup was attempted while a node was already running.
export class VeilidAPIErrorAlreadyInitialized extends VeilidAPIError {}
/// The operation did not complete within its time budget.
export class VeilidAPIErrorTimeout extends VeilidAPIError {}
/// The operation is retryable: retry it later.
export class VeilidAPIErrorTryAgain extends VeilidAPIError {}
/// The destination of the operation is not valid.
export class VeilidAPIErrorInvalidTarget extends VeilidAPIError {}
/// A network connection could not be established.
export class VeilidAPIErrorNoConnection extends VeilidAPIError {}
/// The node is shutting down.
export class VeilidAPIErrorShutdown extends VeilidAPIError {}
/// The requested DHT record does not exist.
export class VeilidAPIErrorKeyNotFound extends VeilidAPIError {}
/// An internal invariant was violated.
export class VeilidAPIErrorInternal extends VeilidAPIError {}
/// The operation is not implemented in this build.
export class VeilidAPIErrorUnimplemented extends VeilidAPIError {}
/// A value could not be parsed.
export class VeilidAPIErrorParseError extends VeilidAPIError {}
/// An argument was malformed or out of range.
export class VeilidAPIErrorInvalidArgument extends VeilidAPIError {}
/// A required argument was not supplied.
export class VeilidAPIErrorMissingArgument extends VeilidAPIError {}
/// A failure that does not fit any more specific category.
export class VeilidAPIErrorGeneric extends VeilidAPIError {}
/// The referenced DHT transaction has expired or was never opened.
export class VeilidAPIErrorTransactionNotFound extends VeilidAPIError {}

const ERROR_CLASSES = {
  NotInitialized: VeilidAPIErrorNotInitialized,
  AlreadyInitialized: VeilidAPIErrorAlreadyInitialized,
  Timeout: VeilidAPIErrorTimeout,
  TryAgain: VeilidAPIErrorTryAgain,
  InvalidTarget: VeilidAPIErrorInvalidTarget,
  NoConnection: VeilidAPIErrorNoConnection,
  Shutdown: VeilidAPIErrorShutdown,
  KeyNotFound: VeilidAPIErrorKeyNotFound,
  Internal: VeilidAPIErrorInternal,
  Unimplemented: VeilidAPIErrorUnimplemented,
  ParseError: VeilidAPIErrorParseError,
  InvalidArgument: VeilidAPIErrorInvalidArgument,
  MissingArgument: VeilidAPIErrorMissingArgument,
  Generic: VeilidAPIErrorGeneric,
  TransactionNotFound: VeilidAPIErrorTransactionNotFound,
};

/// Build the error instance for one serialized VeilidAPIError. Called from Rust, with
/// `message` holding the Rust Display text.
///
/// Variants carrying a `message` field keep it verbatim, which is the property the
/// pre-class plain object exposed; the rest take the Display text, which is the only
/// readable sentence they have.
export function newVeilidAPIError(data, message) {
  const cls = ERROR_CLASSES[data?.kind] ?? VeilidAPIError;
  return new cls(data?.message ?? message, data);
}
