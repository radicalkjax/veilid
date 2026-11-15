use crate::{AeadBuffer, Error};
use chacha20::cipher::{KeyIvInit, StreamCipher};
use chacha20::XChaCha20;
use chacha20poly1305::{aead::AeadInOut as _, KeyInit as _, XChaCha20Poly1305};

/// Length of an XChaCha20 key in bytes
pub const XCHACHA20_KEY_LENGTH: usize = 32;
/// Length of an XChaCha20 nonce in bytes
pub const XCHACHA20_NONCE_LENGTH: usize = 24;
/// Bytes the Poly1305 tag adds to a sealed buffer
pub const XCHACHA20POLY1305_TAG_LENGTH: usize = 16;

fn sized<const N: usize>(bytes: &[u8]) -> Result<[u8; N], Error> {
    bytes.try_into().map_err(|_| Error::InvalidLength {
        expected: N,
        actual: bytes.len(),
    })
}

type KeyNonce = (chacha20poly1305::Key, chacha20poly1305::XNonce);

fn key_nonce(key: &[u8], nonce: &[u8]) -> Result<KeyNonce, Error> {
    Ok((
        chacha20poly1305::Key::from(sized::<XCHACHA20_KEY_LENGTH>(key)?),
        chacha20poly1305::XNonce::from(sized::<XCHACHA20_NONCE_LENGTH>(nonce)?),
    ))
}

/// XOR `buf` with the XChaCha20 keystream for `key` and `nonce`
pub fn xchacha20_apply_keystream(key: &[u8], nonce: &[u8], buf: &mut [u8]) -> Result<(), Error> {
    let (key, nonce) = key_nonce(key, nonce)?;
    XChaCha20::new(&key, &nonce).apply_keystream(buf);
    Ok(())
}

/// Write `input` XOR the XChaCha20 keystream for `key` and `nonce` into `output`, which
/// must be the same length
pub fn xchacha20_apply_keystream_b2b(
    key: &[u8],
    nonce: &[u8],
    input: &[u8],
    output: &mut [u8],
) -> Result<(), Error> {
    if input.len() != output.len() {
        return Err(Error::LengthMismatch);
    }
    let (key, nonce) = key_nonce(key, nonce)?;
    XChaCha20::new(&key, &nonce).apply_keystream_b2b(input, output);
    Ok(())
}

/// Seal `buffer` in place with XChaCha20-Poly1305, appending the tag
pub fn xchacha20poly1305_encrypt_in_place(
    key: &[u8],
    nonce: &[u8],
    associated_data: &[u8],
    buffer: &mut dyn AeadBuffer,
) -> Result<(), Error> {
    let (key, nonce) = key_nonce(key, nonce)?;
    XChaCha20Poly1305::new(&key)
        .encrypt_in_place(&nonce, associated_data, &mut Adapter(buffer))
        .map_err(|_| Error::Aead)
}

/// Open `buffer` in place with XChaCha20-Poly1305, removing the tag. `Aead` when the tag
/// does not authenticate.
pub fn xchacha20poly1305_decrypt_in_place(
    key: &[u8],
    nonce: &[u8],
    associated_data: &[u8],
    buffer: &mut dyn AeadBuffer,
) -> Result<(), Error> {
    let (key, nonce) = key_nonce(key, nonce)?;
    XChaCha20Poly1305::new(&key)
        .decrypt_in_place(&nonce, associated_data, &mut Adapter(buffer))
        .map_err(|_| Error::Aead)
}

// The AEAD's growable-buffer trait, over the caller's object-safe one
struct Adapter<'a>(&'a mut dyn AeadBuffer);

impl AsRef<[u8]> for Adapter<'_> {
    fn as_ref(&self) -> &[u8] {
        self.0.as_ref()
    }
}

impl AsMut<[u8]> for Adapter<'_> {
    fn as_mut(&mut self) -> &mut [u8] {
        self.0.as_mut()
    }
}

impl chacha20poly1305::aead::Buffer for Adapter<'_> {
    fn len(&self) -> usize {
        self.0.len()
    }

    fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    fn extend_from_slice(&mut self, other: &[u8]) -> chacha20poly1305::aead::Result<()> {
        self.0.extend_from_slice(other);
        Ok(())
    }

    fn truncate(&mut self, len: usize) {
        self.0.truncate(len);
    }
}
