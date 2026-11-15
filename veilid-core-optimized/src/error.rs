use core::fmt;

/// Why an operation refused or failed. Callers map these onto their own error types; the
/// string variants carry the underlying crate's message.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Error {
    /// An input has the wrong length
    InvalidLength {
        /// Bytes required
        expected: usize,
        /// Bytes given
        actual: usize,
    },
    /// A key is not valid for its algorithm, or a keypair's halves do not match
    InvalidKey,
    /// Two buffers that must match in length do not
    LengthMismatch,
    /// Diffie-Hellman landed on a low-order point
    NotContributory,
    /// Signing failed
    Signature(String),
    /// AEAD authentication failed
    Aead,
    /// HPKE seal or open failed
    Hpke(String),
    /// Argon2 refused its parameters or failed
    Argon2(String),
    /// A password hash string is not in PHC format
    InvalidPasswordHash(String),
    /// An LZ4 block could not be decoded
    Decompress(String),
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidLength { expected, actual } => {
                write!(f, "invalid length: expected {} got {}", expected, actual)
            }
            Self::InvalidKey => write!(f, "invalid key"),
            Self::LengthMismatch => write!(f, "buffer lengths differ"),
            Self::NotContributory => write!(f, "dh was not contributory"),
            Self::Signature(e) => write!(f, "signing failed: {}", e),
            Self::Aead => write!(f, "aead error"),
            Self::Hpke(e) => write!(f, "hpke error: {}", e),
            Self::Argon2(e) => write!(f, "argon2 error: {}", e),
            Self::InvalidPasswordHash(e) => write!(f, "invalid password hash: {}", e),
            Self::Decompress(e) => write!(f, "decompression failed: {}", e),
        }
    }
}

impl std::error::Error for Error {}
