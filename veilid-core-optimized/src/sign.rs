use crate::{rng::FillRng, Error, FillBytes};
use ed25519_dalek::{Digest as _, Sha512, Signature, SigningKey, VerifyingKey};

/// Length of an ed25519 public key in bytes
pub const ED25519_PUBLIC_KEY_LENGTH: usize = 32;
/// Length of an ed25519 secret key in bytes
pub const ED25519_SECRET_KEY_LENGTH: usize = 32;
/// Length of an ed25519 signature in bytes
pub const ED25519_SIGNATURE_LENGTH: usize = 64;

fn sized<const N: usize>(bytes: &[u8]) -> Result<[u8; N], Error> {
    bytes.try_into().map_err(|_| Error::InvalidLength {
        expected: N,
        actual: bytes.len(),
    })
}

fn verifying_key(public: &[u8]) -> Result<VerifyingKey, Error> {
    VerifyingKey::from_bytes(&sized::<ED25519_PUBLIC_KEY_LENGTH>(public)?)
        .map_err(|_| Error::InvalidKey)
}

fn signing_key(public: &[u8], secret: &[u8]) -> Result<SigningKey, Error> {
    let mut keypair = [0u8; ED25519_SECRET_KEY_LENGTH + ED25519_PUBLIC_KEY_LENGTH];
    keypair[..ED25519_SECRET_KEY_LENGTH]
        .copy_from_slice(&sized::<ED25519_SECRET_KEY_LENGTH>(secret)?);
    keypair[ED25519_SECRET_KEY_LENGTH..]
        .copy_from_slice(&sized::<ED25519_PUBLIC_KEY_LENGTH>(public)?);
    SigningKey::from_keypair_bytes(&keypair).map_err(|_| Error::InvalidKey)
}

fn prehash(data: &[u8]) -> Sha512 {
    let mut digest = Sha512::default();
    digest.update(data);
    digest
}

/// A fresh keypair as `(public, secret)`
pub fn ed25519_generate_keypair(
    fill: FillBytes<'_>,
) -> (
    [u8; ED25519_PUBLIC_KEY_LENGTH],
    [u8; ED25519_SECRET_KEY_LENGTH],
) {
    let signing_key = SigningKey::generate(&mut FillRng(fill));
    (
        signing_key.verifying_key().to_bytes(),
        signing_key.to_bytes(),
    )
}

/// Ed25519ph signature over `data` with `domain` as the context string. `InvalidKey` when
/// `public` and `secret` are not the halves of one keypair.
pub fn ed25519_sign_prehashed(
    public: &[u8],
    secret: &[u8],
    data: &[u8],
    domain: &[u8],
) -> Result<[u8; ED25519_SIGNATURE_LENGTH], Error> {
    signing_key(public, secret)?
        .sign_prehashed(prehash(data), Some(domain))
        .map(|s| s.to_bytes())
        .map_err(|e| Error::Signature(e.to_string()))
}

/// Strict Ed25519ph verification of `signature` over `data` under `domain`. `Ok(false)`
/// when the signature does not match; `InvalidKey` when `public` is not a curve point.
pub fn ed25519_verify_prehashed(
    public: &[u8],
    data: &[u8],
    domain: &[u8],
    signature: &[u8],
) -> Result<bool, Error> {
    let key = verifying_key(public)?;
    let signature = Signature::from_bytes(&sized::<ED25519_SIGNATURE_LENGTH>(signature)?);
    Ok(key
        .verify_prehashed_strict(prehash(data), Some(domain), &signature)
        .is_ok())
}

/// The x25519 public key on the same curve point as an ed25519 public key
pub fn ed25519_public_to_x25519(public: &[u8]) -> Result<[u8; ED25519_PUBLIC_KEY_LENGTH], Error> {
    Ok(verifying_key(public)?.to_montgomery().to_bytes())
}

/// The x25519 secret key an ed25519 secret key expands to
pub fn ed25519_secret_to_x25519(secret: &[u8]) -> Result<[u8; ED25519_SECRET_KEY_LENGTH], Error> {
    let secret = sized::<ED25519_SECRET_KEY_LENGTH>(secret)?;
    // The raw expanded-key half, not SigningKey::to_scalar(), which reduces the scalar (curve25519-dalek#565)
    let hash: [u8; 64] = Sha512::default().chain_update(secret).finalize().into();
    let mut out = [0u8; ED25519_SECRET_KEY_LENGTH];
    out.copy_from_slice(&hash[..ED25519_SECRET_KEY_LENGTH]);
    Ok(out)
}
