use super::*;

/// Cross-platform DNS resolver. The wasm backend is a stub; lookups are unsupported.
pub struct Resolver {}

impl Resolver {
    /// Resolve the TXT records for `host`. Always errors with [`ResolverError::Generic`] on wasm.
    #[expect(clippy::unused_async)]
    pub async fn txt_lookup<S: AsRef<str>>(_host: S) -> Result<Vec<String>, ResolverError> {
        Err(ResolverError::Generic(
            "wasm does not support txt lookup".to_owned(),
        ))
    }

    /// Reverse-resolve `ip_addr` to a hostname. Always errors with [`ResolverError::Generic`] on wasm.
    #[expect(clippy::unused_async)]
    pub async fn ptr_lookup(_ip_addr: IpAddr) -> Result<String, ResolverError> {
        Err(ResolverError::Generic(
            "wasm does not support ptr lookup".to_owned(),
        ))
    }
}
