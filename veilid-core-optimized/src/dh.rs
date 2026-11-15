use crate::Error;
use x25519_dalek::{PublicKey, StaticSecret};

/// Length of an x25519 key in bytes
pub const X25519_KEY_LENGTH: usize = 32;

fn key(bytes: &[u8]) -> Result<[u8; X25519_KEY_LENGTH], Error> {
    bytes.try_into().map_err(|_| Error::InvalidLength {
        expected: X25519_KEY_LENGTH,
        actual: bytes.len(),
    })
}

/// The x25519 shared secret of `secret` and `public`. `NotContributory` when the result is
/// the low-order point.
pub fn x25519_diffie_hellman(
    secret: &[u8],
    public: &[u8],
) -> Result<[u8; X25519_KEY_LENGTH], Error> {
    let shared = StaticSecret::from(key(secret)?).diffie_hellman(&PublicKey::from(key(public)?));
    if !shared.was_contributory() {
        return Err(Error::NotContributory);
    }
    Ok(shared.to_bytes())
}
