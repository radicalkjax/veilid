use super::*;

/////////////////////////////////////////////////////////////////////////////////
// Resolver
//
// Every lookup goes through the platform's own resolver, so the system's DNS privacy
// settings (private DNS, encrypted DNS profiles, DoH) apply without veilid knowing about them.
// veilid never sends a DNS query of its own; the wire codec here only decodes answers.

cfg_if! {
    if #[cfg(all(target_arch = "wasm32", target_os = "unknown"))] {
        mod wasm;
        pub use wasm::*;
    } else {
        pub(crate) mod native;
        pub use native::*;
    }
}

/// Error returned by DNS resolution operations.
#[derive(Debug, Clone, Eq, PartialEq, ThisError)]
pub enum ResolverError {
    /// Resolution failed; the string carries a backend-specific description.
    #[error("Generic resolver error: {0}")]
    Generic(String),
    /// This platform has no system resolver for the record type.
    #[error("Lookup not supported on this platform")]
    Unsupported,
}
