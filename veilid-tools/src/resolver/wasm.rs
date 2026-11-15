use super::*;

/// Cross-platform DNS resolver. The wasm backend is a stub; lookups are unsupported.
pub struct Resolver {}

impl Resolver {
    /// Whether this platform can resolve TXT records. Always false on wasm.
    #[must_use]
    pub fn supports_txt() -> bool {
        false
    }

    /// Resolve the TXT records for `host`. Always errors with [`ResolverError::Unsupported`] on wasm.
    #[expect(clippy::unused_async)]
    pub async fn txt_lookup<S: AsRef<str>>(_host: S) -> Result<Vec<String>, ResolverError> {
        Err(ResolverError::Unsupported)
    }

    /// Reverse-resolve `ip_addr` to a hostname. Always errors with [`ResolverError::Unsupported`] on wasm.
    #[expect(clippy::unused_async)]
    pub async fn ptr_lookup(_ip_addr: IpAddr) -> Result<String, ResolverError> {
        Err(ResolverError::Unsupported)
    }
}
