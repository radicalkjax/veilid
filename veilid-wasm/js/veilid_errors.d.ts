import type { OpaqueRecordKey } from './veilid_wasm.js';

/** Discriminant carried by every {@link VeilidAPIError}, one per veilid-core variant. */
export type VeilidAPIErrorKind =
  | 'NotInitialized'
  | 'AlreadyInitialized'
  | 'Timeout'
  | 'TryAgain'
  | 'InvalidTarget'
  | 'NoConnection'
  | 'Shutdown'
  | 'KeyNotFound'
  | 'Internal'
  | 'Unimplemented'
  | 'ParseError'
  | 'InvalidArgument'
  | 'MissingArgument'
  | 'Generic'
  | 'TransactionNotFound';

/**
 * Base class of every error thrown by veilid-wasm.
 *
 * Catch this to handle any Veilid failure, then narrow on `kind` or with `instanceof`
 * against a subclass.
 */
export declare class VeilidAPIError extends Error {
  readonly kind: VeilidAPIErrorKind;
}

/** The API was used before startup or after shutdown. */
export declare class VeilidAPIErrorNotInitialized extends VeilidAPIError {
  readonly kind: 'NotInitialized';
}
/** Startup was attempted while a node was already running. */
export declare class VeilidAPIErrorAlreadyInitialized extends VeilidAPIError {
  readonly kind: 'AlreadyInitialized';
}
/** The operation did not complete within its time budget. */
export declare class VeilidAPIErrorTimeout extends VeilidAPIError {
  readonly kind: 'Timeout';
}
/** The operation is retryable: retry it later. */
export declare class VeilidAPIErrorTryAgain extends VeilidAPIError {
  readonly kind: 'TryAgain';
}
/** The destination of the operation is not valid. */
export declare class VeilidAPIErrorInvalidTarget extends VeilidAPIError {
  readonly kind: 'InvalidTarget';
}
/** A network connection could not be established. */
export declare class VeilidAPIErrorNoConnection extends VeilidAPIError {
  readonly kind: 'NoConnection';
}
/** The node is shutting down. */
export declare class VeilidAPIErrorShutdown extends VeilidAPIError {
  readonly kind: 'Shutdown';
}
/** The requested DHT record does not exist. */
export declare class VeilidAPIErrorKeyNotFound extends VeilidAPIError {
  readonly kind: 'KeyNotFound';
  readonly key: OpaqueRecordKey;
}
/** An internal invariant was violated. */
export declare class VeilidAPIErrorInternal extends VeilidAPIError {
  readonly kind: 'Internal';
}
/** The operation is not implemented in this build. */
export declare class VeilidAPIErrorUnimplemented extends VeilidAPIError {
  readonly kind: 'Unimplemented';
}
/** A value could not be parsed. */
export declare class VeilidAPIErrorParseError extends VeilidAPIError {
  readonly kind: 'ParseError';
  readonly value: string;
}
/** An argument was malformed or out of range. */
export declare class VeilidAPIErrorInvalidArgument extends VeilidAPIError {
  readonly kind: 'InvalidArgument';
  readonly context: string;
  readonly argument: string;
  readonly value: string;
}
/** A required argument was not supplied. */
export declare class VeilidAPIErrorMissingArgument extends VeilidAPIError {
  readonly kind: 'MissingArgument';
  readonly context: string;
  readonly argument: string;
}
/** A failure that does not fit any more specific category. */
export declare class VeilidAPIErrorGeneric extends VeilidAPIError {
  readonly kind: 'Generic';
}
/** The referenced DHT transaction has expired or was never opened. */
export declare class VeilidAPIErrorTransactionNotFound extends VeilidAPIError {
  readonly kind: 'TransactionNotFound';
}
