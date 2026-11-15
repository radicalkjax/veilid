use super::*;

// Only the backends that receive wire-format answers decode them; the rest get parsed records
#[cfg_attr(
    any(target_os = "macos", target_os = "ios", target_os = "windows"),
    allow(
        dead_code,
        reason = "decoder is used by the Android and libresolv backends"
    )
)]
pub(crate) mod dns;

cfg_if! {
    if #[cfg(target_os = "android")] {
        pub(crate) mod android;
        mod fd_ready;
        mod ptr_unix;
        use android as txt;
        use ptr_unix as ptr;
    } else if #[cfg(any(target_os = "macos", target_os = "ios"))] {
        pub(crate) mod apple;
        mod fd_ready;
        mod ptr_unix;
        use apple as txt;
        use ptr_unix as ptr;
    } else if #[cfg(target_os = "windows")] {
        pub(crate) mod windows;
        use windows as txt;
        use windows as ptr;
    } else {
        pub(crate) mod resolv;
        mod ptr_unix;
        use resolv as txt;
        use ptr_unix as ptr;
    }
}

impl From<dns::DnsError> for ResolverError {
    fn from(e: dns::DnsError) -> Self {
        match e {
            dns::DnsError::Unsupported => ResolverError::Unsupported,
            e => ResolverError::Generic(e.to_string()),
        }
    }
}

/// Cross-platform DNS resolver backed by the platform's system resolver.
pub struct Resolver {}

impl Resolver {
    /// Whether this platform can resolve TXT records. False on Android below API 29.
    #[must_use]
    pub fn supports_txt() -> bool {
        cfg_if! {
            if #[cfg(target_os = "android")] {
                android::available()
            } else {
                true
            }
        }
    }

    /// Resolve the TXT records for `host`, returning each record's text.
    ///
    /// Errors with [`ResolverError::Unsupported`] when the platform has no TXT resolver, and
    /// [`ResolverError::Generic`] when the lookup fails or returns no records.
    pub async fn txt_lookup<S: AsRef<str>>(host: S) -> Result<Vec<String>, ResolverError> {
        Ok(txt::txt_lookup(host.as_ref()).await?)
    }

    /// Reverse-resolve `ip_addr` to a hostname via its PTR record.
    ///
    /// Errors with [`ResolverError::Generic`] when the lookup fails or returns no name.
    pub async fn ptr_lookup(ip_addr: IpAddr) -> Result<String, ResolverError> {
        Ok(ptr::ptr_lookup(ip_addr).await?)
    }
}
