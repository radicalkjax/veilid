use super::*;

use core::fmt::{Debug, Display};

/// Whether a network operation was performed at all.
///
/// Sibling to [`NetworkResult`], one layer out: every `NetworkResult` variant
/// asserts something about the remote, so declining to act needs a variant of its
/// own. `Result<NetworkAttempt<T>, E>` covers all three cases: `Err` for our own
/// fault, `Skipped` for a deliberate choice not to act, `Performed` for anything
/// the remote had a part in.
pub enum NetworkAttempt<T> {
    /// The operation happened; the result describes how it went
    Performed(NetworkResult<T>),
    /// Deliberately not performed. Carries no information about the remote: it must not
    /// count against their contact methods, their liveness, or any route or relay built on them.
    Skipped,
}

impl<T> NetworkAttempt<T> {
    /// A performed attempt that succeeded
    pub fn value(v: T) -> Self {
        Self::Performed(NetworkResult::Value(v))
    }
}

impl<T: Debug> Debug for NetworkAttempt<T> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::Performed(nr) => f.debug_tuple("Performed").field(nr).finish(),
            Self::Skipped => write!(f, "Skipped"),
        }
    }
}

impl<T> Display for NetworkAttempt<T> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::Performed(nr) => write!(f, "Performed({})", nr),
            Self::Skipped => write!(f, "Skipped"),
        }
    }
}
