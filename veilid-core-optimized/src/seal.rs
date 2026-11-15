//! HPKE (RFC 9180) base mode with HKDF-SHA256 and ChaCha20-Poly1305. The KEM is the only
//! thing that varies between cryptosystems, so each gets a pair of entry points.

use crate::{rng::FillRng, Error, FillBytes};
use hpke::{Deserializable, Kem as KemTrait, OpModeR, OpModeS, Serializable};

type Kdf = hpke::kdf::HkdfSha256;
type Aead = hpke::aead::ChaCha20Poly1305;

// Instantiated here for every KEM, never in a caller
fn seal<Kem: KemTrait>(
    recipient: &[u8],
    info: &[u8],
    associated_data: &[u8],
    plaintext: &[u8],
    fill: FillBytes<'_>,
) -> Result<(Vec<u8>, Vec<u8>), Error> {
    let recipient = Kem::PublicKey::from_bytes(recipient).map_err(|_| Error::InvalidKey)?;
    let (enc, ciphertext) = hpke::single_shot_seal_with_rng::<Aead, Kdf, Kem>(
        &OpModeS::Base,
        &recipient,
        info,
        plaintext,
        associated_data,
        &mut FillRng(fill),
    )
    .map_err(|e| Error::Hpke(e.to_string()))?;
    Ok((enc.to_bytes().to_vec(), ciphertext))
}

fn open<Kem: KemTrait>(
    secret: &[u8],
    enc: &[u8],
    info: &[u8],
    associated_data: &[u8],
    ciphertext: &[u8],
) -> Result<Vec<u8>, Error> {
    let secret = Kem::PrivateKey::from_bytes(secret).map_err(|_| Error::InvalidKey)?;
    // enc comes from the blob; a bad one stays indistinguishable from a failed open
    let enc = Kem::EncappedKey::from_bytes(enc).map_err(|e| Error::Hpke(e.to_string()))?;
    hpke::single_shot_open::<Aead, Kdf, Kem>(
        &OpModeR::Base,
        &secret,
        &enc,
        info,
        ciphertext,
        associated_data,
    )
    .map_err(|e| Error::Hpke(e.to_string()))
}

#[cfg(feature = "enable-crypto-vld0")]
mod x25519 {
    use super::*;

    type Kem = hpke::kem::X25519HkdfSha256;

    /// Length of a DHKEM(X25519) key, and of the `enc` a seal produces, in bytes
    pub const HPKE_X25519_KEY_LENGTH: usize = 32;

    fn key_bytes(key: &impl Serializable) -> [u8; HPKE_X25519_KEY_LENGTH] {
        let mut out = [0u8; HPKE_X25519_KEY_LENGTH];
        key.write_exact(&mut out);
        out
    }

    /// A fresh DHKEM(X25519) keypair as `(secret, public)`, via RFC 9180 DeriveKeyPair over
    /// random input keying material
    pub fn hpke_x25519_generate_keypair(
        fill: FillBytes<'_>,
    ) -> ([u8; HPKE_X25519_KEY_LENGTH], [u8; HPKE_X25519_KEY_LENGTH]) {
        let (sk, pk) = Kem::gen_keypair_with_rng(&mut FillRng(fill));
        (key_bytes(&sk), key_bytes(&pk))
    }

    /// RFC 9180 DeriveKeyPair for DHKEM(X25519), as `(secret, public)`
    #[must_use]
    pub fn hpke_x25519_derive_keypair(
        ikm: &[u8],
    ) -> ([u8; HPKE_X25519_KEY_LENGTH], [u8; HPKE_X25519_KEY_LENGTH]) {
        let (sk, pk) = Kem::derive_keypair(ikm);
        (key_bytes(&sk), key_bytes(&pk))
    }

    /// The public key of a DHKEM(X25519) secret key
    pub fn hpke_x25519_public_from_secret(
        secret: &[u8],
    ) -> Result<[u8; HPKE_X25519_KEY_LENGTH], Error> {
        let sk =
            <Kem as KemTrait>::PrivateKey::from_bytes(secret).map_err(|_| Error::InvalidKey)?;
        Ok(key_bytes(&Kem::sk_to_pk(&sk)))
    }

    /// Seal `plaintext` to `recipient`, returning `(enc, ciphertext)`. `InvalidKey` when
    /// `recipient` is not a DHKEM(X25519) public key.
    pub fn hpke_x25519_seal(
        recipient: &[u8],
        info: &[u8],
        associated_data: &[u8],
        plaintext: &[u8],
        fill: FillBytes<'_>,
    ) -> Result<(Vec<u8>, Vec<u8>), Error> {
        seal::<Kem>(recipient, info, associated_data, plaintext, fill)
    }

    /// Open `ciphertext` sealed to `secret` with `enc`. `InvalidKey` when `secret` is not a
    /// DHKEM(X25519) secret key; `Hpke` for anything wrong with the blob.
    pub fn hpke_x25519_open(
        secret: &[u8],
        enc: &[u8],
        info: &[u8],
        associated_data: &[u8],
        ciphertext: &[u8],
    ) -> Result<Vec<u8>, Error> {
        open::<Kem>(secret, enc, info, associated_data, ciphertext)
    }
}

#[cfg(feature = "enable-crypto-vld0")]
pub use x25519::*;

#[cfg(feature = "enable-crypto-none")]
mod none {
    use super::*;
    use crate::none_kem::NoneKem;

    /// Length of a NONE KEM key, and of the `enc` a seal produces, in bytes
    pub const HPKE_NONE_KEY_LENGTH: usize = crate::none_kem::NONE_KEM_KEY_LENGTH;

    /// Seal `plaintext` to `recipient` under the NONE KEM, returning `(enc, ciphertext)`
    pub fn hpke_none_seal(
        recipient: &[u8],
        info: &[u8],
        associated_data: &[u8],
        plaintext: &[u8],
        fill: FillBytes<'_>,
    ) -> Result<(Vec<u8>, Vec<u8>), Error> {
        seal::<NoneKem>(recipient, info, associated_data, plaintext, fill)
    }

    /// Open `ciphertext` sealed to `secret` with `enc` under the NONE KEM
    pub fn hpke_none_open(
        secret: &[u8],
        enc: &[u8],
        info: &[u8],
        associated_data: &[u8],
        ciphertext: &[u8],
    ) -> Result<Vec<u8>, Error> {
        open::<NoneKem>(secret, enc, info, associated_data, ciphertext)
    }
}

#[cfg(feature = "enable-crypto-none")]
pub use none::*;
