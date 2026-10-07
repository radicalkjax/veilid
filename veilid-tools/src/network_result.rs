use super::*;

use core::fmt::{Debug, Display};
use core::result::Result;
use std::error::Error;
use std::io;

//////////////////////////////////////////////////////////////////
// Non-fallible network results conversions

/// Folds a `TimeoutError` outcome into a [`NetworkResult`].
pub trait NetworkResultExt<T> {
    /// `Ok(v)` becomes `Value(v)`; a timeout becomes [`NetworkResult::Timeout`].
    fn into_network_result(self) -> NetworkResult<T>;
}

impl<T> NetworkResultExt<T> for Result<T, TimeoutError> {
    fn into_network_result(self) -> NetworkResult<T> {
        self.ok()
            .map(|v| NetworkResult::<T>::Value(v))
            .unwrap_or(NetworkResult::<T>::Timeout)
    }
}

/// Folds an [`io::Result`] into a [`NetworkResult`], routing benign socket errors to
/// the matching network condition.
pub trait IoNetworkResultExt<T> {
    /// `Ok(v)` becomes `Value(v)`. Unreachable-host, timeout, connection-reset,
    /// already-in-use and bad-data errors map to the corresponding non-error variant;
    /// any other error stays an `Err`.
    fn into_network_result(self) -> io::Result<NetworkResult<T>>;
}

fn io_error_kind_from_error<T>(e: io::Error) -> io::Result<NetworkResult<T>> {
    #[cfg(not(all(target_arch = "wasm32", target_os = "unknown")))]
    if let Some(os_err) = e.raw_os_error() {
        if os_err == libc::EHOSTUNREACH || os_err == libc::ENETUNREACH {
            return Ok(NetworkResult::NoConnection(e));
        }
    }
    #[cfg(windows)]
    if let Some(os_err) = e.raw_os_error() {
        if os_err == winapi::um::winsock2::WSAENETRESET
            || os_err == winapi::um::winsock2::WSAENETUNREACH
        {
            return Ok(NetworkResult::NoConnection(e));
        }
    }
    match e.kind() {
        io::ErrorKind::TimedOut => Ok(NetworkResult::Timeout),
        io::ErrorKind::UnexpectedEof
        | io::ErrorKind::NotConnected
        | io::ErrorKind::BrokenPipe
        | io::ErrorKind::ConnectionAborted
        | io::ErrorKind::ConnectionRefused
        | io::ErrorKind::ConnectionReset
        | io::ErrorKind::InvalidInput => Ok(NetworkResult::NoConnection(e)),
        io::ErrorKind::InvalidData => Ok(NetworkResult::InvalidMessage(e.to_string())),
        io::ErrorKind::AddrNotAvailable | io::ErrorKind::AddrInUse => {
            Ok(NetworkResult::AlreadyExists(e))
        }
        _ => Err(e),
    }
}

impl<T> IoNetworkResultExt<T> for io::Result<T> {
    fn into_network_result(self) -> io::Result<NetworkResult<T>> {
        match self {
            Ok(v) => Ok(NetworkResult::Value(v)),
            Err(e) => io_error_kind_from_error(e),
        }
    }
}

/// Swaps the nesting of a [`NetworkResult`] wrapping a [`Result`].
pub trait NetworkResultResultExt<T, E> {
    /// Turns `NetworkResult<Result<T, E>>` into `Result<NetworkResult<T>, E>`: the inner
    /// `Err(e)` becomes the outer `Err(e)`, everything else stays a `NetworkResult`.
    fn into_result_network_result(self) -> Result<NetworkResult<T>, E>;
}

impl<T, E> NetworkResultResultExt<T, E> for NetworkResult<Result<T, E>> {
    fn into_result_network_result(self) -> Result<NetworkResult<T>, E> {
        match self {
            NetworkResult::Timeout => Ok(NetworkResult::<T>::Timeout),
            NetworkResult::ServiceUnavailable(s) => Ok(NetworkResult::<T>::ServiceUnavailable(s)),
            NetworkResult::NoConnection(e) => Ok(NetworkResult::<T>::NoConnection(e)),
            NetworkResult::AlreadyExists(e) => Ok(NetworkResult::<T>::AlreadyExists(e)),
            NetworkResult::InvalidMessage(s) => Ok(NetworkResult::<T>::InvalidMessage(s)),
            NetworkResult::Value(Ok(v)) => Ok(NetworkResult::<T>::Value(v)),
            NetworkResult::Value(Err(e)) => Err(e),
        }
    }
}

/// Folds a timeout-or-io outcome into a single [`NetworkResult`].
pub trait FoldedNetworkResultExt<T> {
    /// Collapses a [`TimeoutOr`] or nested [`NetworkResult`] together with any io error
    /// into one `io::Result<NetworkResult<T>>`, routing benign socket errors as
    /// [`IoNetworkResultExt::into_network_result`] does. Any io error not routed to a benign
    /// variant stays an `Err`.
    fn folded(self) -> io::Result<NetworkResult<T>>;
}

impl<T> FoldedNetworkResultExt<T> for io::Result<TimeoutOr<T>> {
    fn folded(self) -> io::Result<NetworkResult<T>> {
        match self {
            Ok(TimeoutOr::Timeout) => Ok(NetworkResult::Timeout),
            Ok(TimeoutOr::Value(v)) => Ok(NetworkResult::Value(v)),
            Err(e) => io_error_kind_from_error(e),
        }
    }
}

impl<T> FoldedNetworkResultExt<T> for io::Result<NetworkResult<T>> {
    fn folded(self) -> io::Result<NetworkResult<T>> {
        match self {
            Ok(v) => Ok(v),
            Err(e) => io_error_kind_from_error(e),
        }
    }
}

//////////////////////////////////////////////////////////////////
// Non-fallible network result

/// Outcome of a network operation that separates a real `Value` from benign network
/// conditions which should not be treated as hard errors.
///
/// Used throughout veilid so callers can distinguish "the peer answered" from the many
/// ways a request can fail to produce an answer without anything being wrong locally.
#[must_use]
pub enum NetworkResult<T> {
    /// The operation did not complete in time.
    Timeout,
    /// The remote refused to service the request; the string carries the reason.
    ServiceUnavailable(String),
    /// No connection could be established or the existing one dropped.
    NoConnection(io::Error),
    /// The remote reports the thing being created already exists, or the local address is
    /// already bound.
    AlreadyExists(io::Error),
    /// A message arrived but failed to parse or validate; the string carries the reason.
    InvalidMessage(String),
    /// The operation succeeded, carrying its result.
    Value(T),
}

impl<T> NetworkResult<T> {
    /// Constructs a [`NetworkResult::Timeout`].
    pub fn timeout() -> Self {
        Self::Timeout
    }
    /// Constructs a [`NetworkResult::ServiceUnavailable`] with the given reason.
    pub fn service_unavailable<S: ToString>(s: S) -> Self {
        Self::ServiceUnavailable(s.to_string())
    }
    /// Constructs a [`NetworkResult::NoConnection`] from an io error.
    pub fn no_connection(e: io::Error) -> Self {
        Self::NoConnection(e)
    }
    /// Constructs a [`NetworkResult::NoConnection`] from a message, wrapping it in an
    /// `io::Error::other`.
    pub fn no_connection_other<S: ToString>(s: S) -> Self {
        Self::NoConnection(io::Error::other(s.to_string()))
    }
    /// Constructs a [`NetworkResult::InvalidMessage`] with the given reason.
    pub fn invalid_message<S: ToString>(s: S) -> Self {
        Self::InvalidMessage(s.to_string())
    }
    /// Constructs a [`NetworkResult::AlreadyExists`] from an io error.
    pub fn already_exists(e: io::Error) -> Self {
        Self::AlreadyExists(e)
    }
    /// Constructs a [`NetworkResult::Value`].
    pub fn value(value: T) -> Self {
        Self::Value(value)
    }

    /// True if this is a [`NetworkResult::Timeout`].
    pub fn is_timeout(&self) -> bool {
        matches!(self, Self::Timeout)
    }
    /// True if this is a [`NetworkResult::NoConnection`].
    pub fn is_no_connection(&self) -> bool {
        matches!(self, Self::NoConnection(_))
    }
    /// True if this is a [`NetworkResult::AlreadyExists`].
    pub fn is_already_exists(&self) -> bool {
        matches!(self, Self::AlreadyExists(_))
    }
    /// True if this is a [`NetworkResult::InvalidMessage`].
    pub fn is_invalid_message(&self) -> bool {
        matches!(self, Self::InvalidMessage(_))
    }
    /// True if this is a [`NetworkResult::Value`].
    pub fn is_value(&self) -> bool {
        matches!(self, Self::Value(_))
    }
    /// Maps the contained `Value` through `f`, passing every other variant unchanged.
    pub fn map<X, F: Fn(T) -> X>(self, f: F) -> NetworkResult<X> {
        match self {
            Self::Timeout => NetworkResult::<X>::Timeout,
            Self::ServiceUnavailable(s) => NetworkResult::<X>::ServiceUnavailable(s),
            Self::NoConnection(e) => NetworkResult::<X>::NoConnection(e),
            Self::AlreadyExists(e) => NetworkResult::<X>::AlreadyExists(e),
            Self::InvalidMessage(s) => NetworkResult::<X>::InvalidMessage(s),
            Self::Value(v) => NetworkResult::<X>::Value(f(v)),
        }
    }
    /// Collapses into an [`io::Result`], turning each non-value variant into the io error
    /// it stands for.
    pub fn into_io_result(self) -> Result<T, io::Error> {
        match self {
            Self::Timeout => Err(io::Error::new(io::ErrorKind::TimedOut, "Timed out")),
            Self::ServiceUnavailable(s) => Err(io::Error::new(
                io::ErrorKind::NotFound,
                format!("Service unavailable: {}", s),
            )),
            Self::NoConnection(e) => Err(e),
            Self::AlreadyExists(e) => Err(e),
            Self::InvalidMessage(s) => Err(io::Error::new(
                io::ErrorKind::InvalidData,
                format!("Invalid message: {}", s),
            )),
            Self::Value(v) => Ok(v),
        }
    }
}

impl<T> From<NetworkResult<T>> for Option<T> {
    fn from(t: NetworkResult<T>) -> Self {
        match t {
            NetworkResult::Value(v) => Some(v),
            _ => None,
        }
    }
}

impl<T: Debug> Debug for NetworkResult<T> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::Timeout => write!(f, "Timeout"),
            Self::ServiceUnavailable(s) => f.debug_tuple("ServiceUnavailable").field(s).finish(),
            Self::NoConnection(e) => f.debug_tuple("NoConnection").field(e).finish(),
            Self::AlreadyExists(e) => f.debug_tuple("AlreadyExists").field(e).finish(),
            Self::InvalidMessage(s) => f.debug_tuple("InvalidMessage").field(s).finish(),
            Self::Value(v) => f.debug_tuple("Value").field(v).finish(),
        }
    }
}

impl<T> Display for NetworkResult<T> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::Timeout => write!(f, "Timeout"),
            Self::ServiceUnavailable(s) => write!(f, "ServiceUnavailable({})", s),
            Self::NoConnection(e) => write!(f, "NoConnection({})", e.kind()),
            Self::AlreadyExists(e) => write!(f, "AlreadyExists({})", e.kind()),
            Self::InvalidMessage(s) => write!(f, "InvalidMessage({})", s),
            Self::Value(_) => write!(f, "Value"),
        }
    }
}

impl<T: Debug + Display> Error for NetworkResult<T> {}

//////////////////////////////////////////////////////////////////
// Non-fallible network result macros

/// Returns early from the enclosing `Result<NetworkResult<_>, _>`-returning function,
/// re-raising a non-value [`NetworkResult`] under a new value type. Panics on `Value`.
#[macro_export]
macro_rules! network_result_raise {
    ($r: expr) => {
        match $r {
            NetworkResult::Timeout => return Ok(NetworkResult::Timeout),
            NetworkResult::ServiceUnavailable(s) => return Ok(NetworkResult::ServiceUnavailable(s)),
            NetworkResult::NoConnection(e) => return Ok(NetworkResult::NoConnection(e)),
            NetworkResult::AlreadyExists(e) => return Ok(NetworkResult::AlreadyExists(e)),
            NetworkResult::InvalidMessage(s) => return Ok(NetworkResult::InvalidMessage(s)),
            NetworkResult::Value(_) => panic!("Can not raise value"),
        }
    };
}

/// Unwraps the `Value` of a [`NetworkResult`], or returns the non-value variant early
/// from the enclosing `Result<NetworkResult<_>, _>`-returning function. The `=> $f`
/// form runs `$f` before returning, for cleanup on the early-exit path.
#[macro_export]
macro_rules! network_result_try {
    ($r: expr) => {
        match $r {
            NetworkResult::Timeout => return Ok(NetworkResult::Timeout),
            NetworkResult::ServiceUnavailable(s) => return Ok(NetworkResult::ServiceUnavailable(s)),
            NetworkResult::NoConnection(e) => return Ok(NetworkResult::NoConnection(e)),
            NetworkResult::AlreadyExists(e) => return Ok(NetworkResult::AlreadyExists(e)),
            NetworkResult::InvalidMessage(s) => return Ok(NetworkResult::InvalidMessage(s)),
            NetworkResult::Value(v) => v,
        }
    };
    ($r:expr => $f:tt) => {
        match $r {
            NetworkResult::Timeout => {
                $f;
                return Ok(NetworkResult::Timeout);
            }
            NetworkResult::ServiceUnavailable(s) => {
                $f;
                return Ok(NetworkResult::ServiceUnavailable(s));
            }
            NetworkResult::NoConnection(e) => {
                $f;
                return Ok(NetworkResult::NoConnection(e));
            }
            NetworkResult::AlreadyExists(e) => {
                $f;
                return Ok(NetworkResult::AlreadyExists(e));
            }
            NetworkResult::InvalidMessage(s) => {
                $f;
                return Ok(NetworkResult::InvalidMessage(s));
            }
            NetworkResult::Value(v) => v,
        }
    };
}
