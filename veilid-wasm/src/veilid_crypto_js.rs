#![allow(non_snake_case)]
use super::*;

/// A handle to a single cryptosystem, selected by [`CryptoKind`], exposing its
/// crypto operations to JavaScript.
#[wasm_bindgen(js_name = veilidCrypto)]
pub struct VeilidCrypto {
    pub(crate) kind: CryptoKind,
}

#[wasm_bindgen(js_class = veilidCrypto)]
impl VeilidCrypto {
    // --------------------------------
    // Constants
    // (written as getters since wasm_bindgen doesn't support export of const)
    // --------------------------------

    /// The VLD0 crypto kind
    #[cfg(feature = "enable-crypto-vld0")]
    #[wasm_bindgen(getter, unchecked_return_type = "CryptoKind")]
    #[must_use]
    pub fn CRYPTO_KIND_VLD0() -> JsValue {
        crate::CRYPTO_KIND_VLD0.into()
    }

    /// The NONE crypto kind
    #[cfg(feature = "enable-crypto-none")]
    #[wasm_bindgen(getter, unchecked_return_type = "CryptoKind")]
    #[must_use]
    pub fn CRYPTO_KIND_NONE() -> JsValue {
        crate::CRYPTO_KIND_NONE.into()
    }

    // /// The VLD1 crypto kind
    // #[cfg(feature = "enable-crypto-vld1")]
    // #[wasm_bindgen(getter)]
    // #[must_use]
    // pub fn CRYPTO_KIND_VLD1() -> CryptoKind {
    //     CRYPTO_KIND_VLD1
    // }

    /// All crypto kinds supported by this configuration of Veilid
    #[wasm_bindgen(getter, unchecked_return_type = "CryptoKind[]")]
    #[must_use]
    pub fn VALID_CRYPTO_KINDS() -> JsValue {
        js_sys::Array::from_iter(
            crate::VALID_CRYPTO_KINDS
                .iter()
                .map(|x| JsValue::from(x.to_string())),
        )
        .into()
    }

    ////////////////////////////////////////////////////////////////////////////////

    /// The [`CryptoKind`] fourcc identifying this cryptosystem.
    #[wasm_bindgen(getter, unchecked_return_type = "CryptoKind")]
    #[must_use]
    pub fn kind(&self) -> JsValue {
        self.kind.into()
    }

    fn with_crypto_system<
        T,
        F: FnOnce(&(dyn CryptoSystem + Send + Sync + 'static)) -> VeilidAPIResult<T>,
    >(
        &self,
        closure: F,
    ) -> VeilidAPIResult<T> {
        let veilid_api = get_veilid_api()?;
        let crypto = veilid_api.crypto()?;
        let crypto_system = crypto.get(self.kind).ok_or_else(|| {
            VeilidAPIError::invalid_argument("with_crypto_system", "kind", self.kind.to_string())
        })?;
        closure(crypto_system.deref())
    }

    /// Diffie-Hellman shared secret for the given public/secret key pair, memoized in the
    /// DH cache to avoid recomputing the same exchange. See `computeDh`.
    ///
    /// Throws `Generic` if `key` or `secret` has the wrong kind or length, and on a cache miss the same errors as `computeDh`. Every method on this object also throws `InvalidArgument` if this cryptosystem kind is unavailable and `NotInitialized` if the node is not started.
    pub fn cachedDh(&self, key: &PublicKey, secret: &SecretKey) -> VeilidAPIResult<SharedSecret> {
        self.with_crypto_system(|crypto_system| {
            let out = crypto_system.cached_dh(key, secret)?;
            Ok(out)
        })
    }

    /// Raw Diffie-Hellman shared secret for the given public/secret key pair, with no caching.
    ///
    /// Throws `Internal` if `key` is not a valid curve point, and `Generic` if the exchange is non-contributory (low-order public key).
    pub fn computeDh(&self, key: &PublicKey, secret: &SecretKey) -> VeilidAPIResult<SharedSecret> {
        self.with_crypto_system(|crypto_system| crypto_system.compute_dh(key, secret))
    }

    /// Derive a domain-separated shared secret from a key exchange: computes the DH secret, then
    /// hashes it together with `domain` and the Veilid API domain tag. Distinct `domain` values
    /// yield independent secrets from the same key pair.
    ///
    /// Throws the `computeDh` errors if the key exchange fails.
    pub fn generateSharedSecret(
        &self,
        key: &PublicKey,
        secret: &SecretKey,
        domain: Box<[u8]>,
    ) -> VeilidAPIResult<SharedSecret> {
        self.with_crypto_system(|crypto_system| {
            crypto_system.generate_shared_secret(key, secret, &domain)
        })
    }

    /// Seal `plaintext` to `recipient` with HPKE base mode (RFC 9180), single-shot. `aad` is
    /// authenticated but not encrypted. Returns a self-describing sealed blob.
    ///
    /// Sealing is one-way: only the holder of the recipient `DecapsulationKey` can open the
    /// blob, and the sealer cannot decrypt what it just sealed. This differs from the DH
    /// pattern, where the shared secret let the encrypting party decrypt its own blobs. A
    /// sealer that needs to re-read stored blobs must also seal them to its own key. Callers
    /// who already share a symmetric key want `encryptAead` instead; HPKE is for encrypting to
    /// a recipient's key when no shared secret exists.
    ///
    /// Throws `InvalidArgument` if `recipient` is not a valid key, and `Generic` if encapsulation
    /// fails (including a low-order key).
    pub fn hpkeSeal(
        &self,
        recipient: &EncapsulationKey,
        aad: Box<[u8]>,
        plaintext: Box<[u8]>,
    ) -> VeilidAPIResult<Box<[u8]>> {
        self.with_crypto_system(|crypto_system| {
            let out = crypto_system.hpke_seal(recipient, &aad, &plaintext)?;
            Ok(out.into_boxed_slice())
        })
    }

    /// Open a sealed blob produced by `hpkeSeal` with the recipient `secret`. `aad` must match
    /// what was supplied at seal. Only the recipient can open a sealed blob; the sealer cannot.
    ///
    /// Throws `ParseError` if the blob is truncated or its version is unknown, `InvalidArgument`
    /// if the blob's kind is not this cryptosystem's kind or `secret` is not a valid key, and
    /// `Generic` if decryption fails.
    pub fn hpkeOpen(
        &self,
        secret: &DecapsulationKey,
        aad: Box<[u8]>,
        sealed: Box<[u8]>,
    ) -> VeilidAPIResult<Box<[u8]>> {
        self.with_crypto_system(|crypto_system| {
            let out = crypto_system.hpke_open(secret, &aad, &sealed)?;
            Ok(out.into_boxed_slice())
        })
    }

    /// Fill a new buffer of `len` bytes from the cryptographic RNG.
    pub fn randomBytes(&self, len: usize) -> VeilidAPIResult<Box<[u8]>> {
        self.with_crypto_system(|crypto_system| {
            let out = crypto_system.random_bytes(len);
            let out = out.into_boxed_slice();
            Ok(out)
        })
    }

    /// Byte length of a shared secret.
    pub fn sharedSecretLength(&self) -> VeilidAPIResult<usize> {
        self.with_crypto_system(|crypto_system| {
            let out = crypto_system.shared_secret_length();
            Ok(out)
        })
    }

    /// Byte length of a nonce.
    pub fn nonceLength(&self) -> VeilidAPIResult<usize> {
        self.with_crypto_system(|crypto_system| {
            let out = crypto_system.nonce_length();
            Ok(out)
        })
    }

    /// Byte length of a hash digest.
    pub fn hashDigestLength(&self) -> VeilidAPIResult<usize> {
        self.with_crypto_system(|crypto_system| {
            let out = crypto_system.hash_digest_length();
            Ok(out)
        })
    }

    /// Byte length of a public key.
    pub fn publicKeyLength(&self) -> VeilidAPIResult<usize> {
        self.with_crypto_system(|crypto_system| {
            let out = crypto_system.public_key_length();
            Ok(out)
        })
    }

    /// Byte length of a secret key.
    pub fn secretKeyLength(&self) -> VeilidAPIResult<usize> {
        self.with_crypto_system(|crypto_system| {
            let out = crypto_system.secret_key_length();
            Ok(out)
        })
    }

    /// Byte length of a KEM encapsulation key.
    pub fn encapsulationKeyLength(&self) -> VeilidAPIResult<usize> {
        self.with_crypto_system(|crypto_system| {
            let out = crypto_system.encapsulation_key_length();
            Ok(out)
        })
    }

    /// Byte length of a KEM decapsulation key.
    pub fn decapsulationKeyLength(&self) -> VeilidAPIResult<usize> {
        self.with_crypto_system(|crypto_system| {
            let out = crypto_system.decapsulation_key_length();
            Ok(out)
        })
    }

    /// Byte length of a signature.
    pub fn signatureLength(&self) -> VeilidAPIResult<usize> {
        self.with_crypto_system(|crypto_system| {
            let out = crypto_system.signature_length();
            Ok(out)
        })
    }

    /// Default salt length in bytes for password hashing and KDF operations.
    pub fn defaultSaltLength(&self) -> VeilidAPIResult<usize> {
        self.with_crypto_system(|crypto_system| {
            let out = crypto_system.default_salt_length();
            Ok(out)
        })
    }

    /// Bytes an AEAD operation adds to the ciphertext (the authentication tag length).
    pub fn aeadOverhead(&self) -> VeilidAPIResult<usize> {
        self.with_crypto_system(|crypto_system| {
            let out = crypto_system.aead_overhead();
            Ok(out)
        })
    }

    /// Verify a shared secret carries this cryptosystem's kind and the correct length.
    ///
    /// Throws `Generic` if `secret` has the wrong kind or length.
    pub fn checkSharedSecret(&self, secret: &SharedSecret) -> VeilidAPIResult<()> {
        self.with_crypto_system(|crypto_system| crypto_system.check_shared_secret(secret))
    }

    /// Verify a nonce has the correct length.
    ///
    /// Throws `Generic` if `nonce` has the wrong length.
    pub fn checkNonce(&self, nonce: &Nonce) -> VeilidAPIResult<()> {
        self.with_crypto_system(|crypto_system| crypto_system.check_nonce(nonce))
    }

    /// Verify a hash digest carries this cryptosystem's kind and the correct length.
    ///
    /// Throws `Generic` if `digest` has the wrong kind or length.
    pub fn checkHashDigest(&self, digest: &HashDigest) -> VeilidAPIResult<()> {
        self.with_crypto_system(|crypto_system| crypto_system.check_hash_digest(digest))
    }

    /// Verify a public key carries this cryptosystem's kind and the correct length.
    ///
    /// Throws `Generic` if `key` has the wrong kind or length.
    pub fn checkPublicKey(&self, key: &PublicKey) -> VeilidAPIResult<()> {
        self.with_crypto_system(|crypto_system| crypto_system.check_public_key(key))
    }

    /// Verify a secret key carries this cryptosystem's kind and the correct length.
    ///
    /// Throws `Generic` if `key` has the wrong kind or length.
    pub fn checkSecretKey(&self, key: &SecretKey) -> VeilidAPIResult<()> {
        self.with_crypto_system(|crypto_system| crypto_system.check_secret_key(key))
    }

    /// Verify a signature carries this cryptosystem's kind and the correct length.
    ///
    /// Throws `Generic` if `signature` has the wrong kind or length.
    pub fn checkSignature(&self, signature: &Signature) -> VeilidAPIResult<()> {
        self.with_crypto_system(|crypto_system| crypto_system.check_signature(signature))
    }

    /// Hash a password with the given salt, returning a self-describing PHC hash string suitable
    /// for storage and later `verifyPassword`.
    ///
    /// Throws `Generic` if `salt` length is outside the Argon2 bounds or the KDF fails, and `ParseError` if the salt fails base64 encoding.
    pub fn hashPassword(&self, password: Box<[u8]>, salt: Box<[u8]>) -> VeilidAPIResult<String> {
        self.with_crypto_system(|crypto_system| crypto_system.hash_password(&password, &salt))
    }

    /// Check a password against a PHC hash string produced by `hashPassword`.
    /// Returns `false` on mismatch; errors only on a malformed hash string.
    ///
    /// Throws `ParseError` if `password_hash` is not a valid PHC string.
    pub fn verifyPassword(
        &self,
        password: Box<[u8]>,
        password_hash: String,
    ) -> VeilidAPIResult<bool> {
        self.with_crypto_system(|crypto_system| {
            crypto_system.verify_password(&password, &password_hash)
        })
    }

    /// Derive a shared secret from a password and salt via a password-hashing KDF. Deterministic:
    /// the same password and salt always yield the same secret. Distinct from
    /// `generateSharedSecret`, which uses key exchange.
    ///
    /// Throws `Generic` if `salt` length is outside the Argon2 bounds or the KDF fails.
    pub fn deriveSharedSecret(
        &self,
        password: Box<[u8]>,
        salt: Box<[u8]>,
    ) -> VeilidAPIResult<SharedSecret> {
        self.with_crypto_system(|crypto_system| {
            crypto_system.derive_shared_secret(&password, &salt)
        })
    }

    /// A fresh random nonce of `nonceLength` bytes.
    pub fn randomNonce(&self) -> VeilidAPIResult<Nonce> {
        self.with_crypto_system(|crypto_system| {
            let out = crypto_system.random_nonce();
            Ok(out)
        })
    }

    /// A fresh random shared secret of `sharedSecretLength` bytes.
    pub fn randomSharedSecret(&self) -> VeilidAPIResult<SharedSecret> {
        self.with_crypto_system(|crypto_system| {
            let out = crypto_system.random_shared_secret();
            Ok(out)
        })
    }

    /// Generate a fresh random signing key pair for this cryptosystem.
    pub fn generateKeyPair(&self) -> VeilidAPIResult<KeyPair> {
        self.with_crypto_system(|crypto_system| {
            let out = crypto_system.generate_keypair();
            Ok(out)
        })
    }

    /// Generate a fresh random KEM key pair for this cryptosystem.
    pub fn generateKemKeyPair(&self) -> VeilidAPIResult<KemKeyPair> {
        self.with_crypto_system(|crypto_system| {
            let out = crypto_system.generate_kem_keypair();
            Ok(out)
        })
    }

    /// Derive the KEM encapsulation key corresponding to a signing public key.
    ///
    /// VLD0-only bridge (ed25519 to x25519): kinds whose signing and KEM keys are unrelated
    /// throw `Unimplemented`. Throws `InvalidArgument` if `key` is not a valid signing public key.
    pub fn encapsulationKeyFromSigningKey(
        &self,
        key: &PublicKey,
    ) -> VeilidAPIResult<EncapsulationKey> {
        self.with_crypto_system(|crypto_system| {
            crypto_system.encapsulation_key_from_signing_key(key)
        })
    }

    /// Derive the KEM decapsulation key corresponding to a signing secret key.
    ///
    /// VLD0-only bridge (ed25519 to x25519): kinds whose signing and KEM keys are unrelated
    /// throw `Unimplemented`. Throws `InvalidArgument` if `secret` is not a valid signing secret key.
    pub fn decapsulationKeyFromSigningSecret(
        &self,
        secret: &SecretKey,
    ) -> VeilidAPIResult<DecapsulationKey> {
        self.with_crypto_system(|crypto_system| {
            crypto_system.decapsulation_key_from_signing_secret(secret)
        })
    }

    /// Hash a byte slice, returning a digest tagged with this cryptosystem's kind.
    pub fn generateHash(&self, data: Box<[u8]>) -> VeilidAPIResult<HashDigest> {
        self.with_crypto_system(|crypto_system| {
            let out = crypto_system.generate_hash(&data);
            Ok(out)
        })
    }

    /// Check that a public and secret key form a usable signing pair by signing test data and
    /// verifying it. Returns `false` if they do not match; errors only on a malformed key.
    ///
    /// Throws `Generic` if `key` or `secret` has the wrong kind or length.
    pub fn validateKeyPair(&self, key: &PublicKey, secret: &SecretKey) -> VeilidAPIResult<bool> {
        self.with_crypto_system(|crypto_system| crypto_system.validate_keypair(key, secret))
    }

    /// Recompute the hash of `data` and compare it against `hash`. Returns `true` on match.
    ///
    /// Throws `Generic` if `hash` has the wrong kind or length.
    pub fn validateHash(&self, data: Box<[u8]>, hash: &HashDigest) -> VeilidAPIResult<bool> {
        self.with_crypto_system(|crypto_system| crypto_system.validate_hash(&data, hash))
    }

    /// Sign `data` with the given key pair, returning a detached signature.
    ///
    /// Throws `Generic` if `key` or `secret` has the wrong kind or length, `ParseError` if they do not form a valid ed25519 keypair, and `Internal` if signing fails.
    pub fn sign(
        &self,
        key: &PublicKey,
        secret: &SecretKey,
        data: Box<[u8]>,
    ) -> VeilidAPIResult<Signature> {
        self.with_crypto_system(|crypto_system| crypto_system.sign(key, secret, &data))
    }

    /// Verify a detached `signature` over `data` for `key`. Returns `true` if valid,
    /// `false` if not; errors only on a malformed key or signature.
    ///
    /// Throws `Generic` if `key` or `signature` has the wrong kind or length, and `ParseError` or `Internal` if `key` is not a valid ed25519 point. A signature that does not match returns `false`, not an error.
    pub fn verify(
        &self,
        key: &PublicKey,
        data: Box<[u8]>,
        signature: &Signature,
    ) -> VeilidAPIResult<bool> {
        self.with_crypto_system(|crypto_system| crypto_system.verify(key, &data, signature))
    }

    /// Decrypt and authenticate `body`, returning the plaintext. `associatedData` must match what
    /// was supplied at encryption. Errors if authentication fails (tampered ciphertext, wrong
    /// key/nonce, or mismatched associated data).
    ///
    /// Throws `Generic` if `nonce` or `shared_secret` has the wrong kind or length or if authentication fails, and `Internal` on an internal length conversion failure.
    pub fn decryptAead(
        &self,
        body: Box<[u8]>,
        nonce: &Nonce,
        shared_secret: &SharedSecret,
        associated_data: Option<Box<[u8]>>,
    ) -> VeilidAPIResult<Box<[u8]>> {
        self.with_crypto_system(|crypto_system| {
            let out = crypto_system.decrypt_aead(
                &body,
                nonce,
                shared_secret,
                match &associated_data {
                    Some(ad) => Some(ad),
                    None => None,
                },
            )?;
            let out = out.into_boxed_slice();
            Ok(out)
        })
    }

    /// Encrypt and authenticate `body`, returning the ciphertext with appended tag. `associatedData`
    /// is authenticated but not encrypted, and must be supplied again at decryption. The same nonce
    /// must never be reused with the same shared secret.
    ///
    /// Throws `Generic` if `nonce` or `shared_secret` has the wrong kind or length, and `Internal` on an internal length conversion failure.
    pub fn encryptAead(
        &self,
        body: Box<[u8]>,
        nonce: &Nonce,
        shared_secret: &SharedSecret,
        associated_data: Option<Box<[u8]>>,
    ) -> VeilidAPIResult<Box<[u8]>> {
        self.with_crypto_system(|crypto_system| {
            let out = crypto_system.encrypt_aead(
                &body,
                nonce,
                shared_secret,
                match &associated_data {
                    Some(ad) => Some(ad),
                    None => None,
                },
            )?;
            Ok(out.into_boxed_slice())
        })
    }

    /// Apply the stream cipher to `body`, without authentication. Same operation for both
    /// directions: re-applying with the same nonce and secret reverses it. Provides confidentiality
    /// only, no integrity; callers needing tamper detection must use the AEAD variants.
    ///
    /// Throws `Generic` if `nonce` or `shared_secret` has the wrong kind or length, and `Internal` on an internal length conversion failure.
    pub fn cryptNoAuth(
        &self,
        mut body: Box<[u8]>,
        nonce: &Nonce,
        shared_secret: &SharedSecret,
    ) -> VeilidAPIResult<Box<[u8]>> {
        self.with_crypto_system(|crypto_system| {
            crypto_system.crypt_in_place_no_auth(&mut body, nonce, shared_secret)?;
            Ok(body)
        })
    }
}
