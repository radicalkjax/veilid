mod sizes;

use super::*;

use super::hpke::{build_sealed, hpke_info, parse_sealed};
use veilid_core_optimized as opt;

const VLD0_DOMAIN_SIGN: &[u8] = b"VLD0_SIGN";
const VLD0_DOMAIN_CRYPT: &[u8] = b"VLD0_CRYPT";

const VLD0_AEAD_OVERHEAD: usize = opt::XCHACHA20POLY1305_TAG_LENGTH;
/// [`CryptoKind`] fourcc identifying the VLD0 cryptosystem.
pub const CRYPTO_KIND_VLD0: CryptoKind = CryptoKind::new(*b"VLD0");
/// The VLD0 fourcc as a big-endian `u32`.
pub const CRYPTO_KIND_VLD0_FOURCC: u32 = u32::from_be_bytes(*b"VLD0");
pub use sizes::*;

fn public_to_x25519_pk(public: &PublicKey) -> VeilidAPIResult<[u8; VLD0_PUBLIC_KEY_LENGTH]> {
    opt::ed25519_public_to_x25519(public.ref_value()).map_err(VeilidAPIError::internal)
}
fn secret_to_x25519_sk(secret: &SecretKey) -> VeilidAPIResult<[u8; VLD0_SECRET_KEY_LENGTH]> {
    opt::ed25519_secret_to_x25519(secret.ref_value()).map_err(VeilidAPIError::internal)
}

pub(crate) fn vld0_generate_keypair() -> KeyPair {
    let (public_key, secret_key) = opt::ed25519_generate_keypair(&mut random_bytes);
    KeyPair::new(
        CRYPTO_KIND_VLD0,
        BareKeyPair::new(
            BarePublicKey::new(&public_key),
            BareSecretKey::new(&secret_key),
        ),
    )
}

pub(crate) fn vld0_generate_kem_keypair() -> KemKeyPair {
    // RFC 9180 DeriveKeyPair(random ikm), the same path seal/open use
    let (sk, pk) = opt::hpke_x25519_generate_keypair(&mut random_bytes);
    KemKeyPair::new(
        CRYPTO_KIND_VLD0,
        BareKemKeyPair::new(
            BareEncapsulationKey::new(&pk),
            BareDecapsulationKey::new(&sk),
        ),
    )
}

/// V0 CryptoSystem
pub(crate) struct CryptoSystemVLD0 {
    registry: VeilidComponentRegistry,
}

impl CryptoSystemVLD0 {
    #[must_use]
    pub(crate) fn new(registry: VeilidComponentRegistry) -> Self {
        Self { registry }
    }
}

impl CryptoSystem for CryptoSystemVLD0 {
    // Accessors
    fn kind(&self) -> CryptoKind {
        CRYPTO_KIND_VLD0
    }

    fn crypto(&self) -> VeilidComponentGuard<'_, Crypto> {
        self.registry.lookup::<Crypto>().unwrap_or_log()
    }

    // Cached Operations
    #[cfg_attr(feature = "instrument", instrument(level = "trace", skip_all, fields(__VEILID_LOG_KEY = self.registry.log_key())))]
    fn cached_dh(&self, key: &PublicKey, secret: &SecretKey) -> VeilidAPIResult<SharedSecret> {
        self.crypto()
            .cached_dh_internal::<CryptoSystemVLD0>(self, key, secret)
    }

    // Generation
    #[cfg_attr(
        feature = "instrument",
        instrument(level = "trace", target = "crypto", skip_all, fields(__VEILID_LOG_KEY = self.registry.log_key()))
    )]
    fn random_bytes(&self, len: usize) -> Vec<u8> {
        let mut bytes = zeroed_u8_vec(len);
        random_bytes(bytes.as_mut());
        bytes
    }

    #[cfg_attr(
        feature = "instrument",
        instrument(level = "trace", target = "crypto", skip_all, fields(__VEILID_LOG_KEY = self.registry.log_key()))
    )]
    fn hash_password(&self, password: &[u8], salt: &[u8]) -> VeilidAPIResult<String> {
        // Argon2 with default params (Argon2id v19)
        opt::argon2_hash_password(password, salt).map_err(VeilidAPIError::generic)
    }
    #[cfg_attr(
        feature = "instrument",
        instrument(level = "trace", target = "crypto", skip_all, fields(__VEILID_LOG_KEY = self.registry.log_key()))
    )]
    fn verify_password(&self, password: &[u8], password_hash: &str) -> VeilidAPIResult<bool> {
        opt::argon2_verify_password(password, password_hash).map_err(|e| match e {
            opt::Error::InvalidPasswordHash(msg) => VeilidAPIError::parse_error(msg, password_hash),
            e => VeilidAPIError::generic(e),
        })
    }

    #[cfg_attr(
        feature = "instrument",
        instrument(level = "trace", target = "crypto", skip_all, fields(__VEILID_LOG_KEY = self.registry.log_key()))
    )]
    fn derive_shared_secret(&self, password: &[u8], salt: &[u8]) -> VeilidAPIResult<SharedSecret> {
        // Argon2 with default params (Argon2id v19)
        let mut output_key_material = [0u8; VLD0_SHARED_SECRET_LENGTH];
        opt::argon2_derive_key(password, salt, &mut output_key_material)
            .map_err(VeilidAPIError::generic)?;
        Ok(SharedSecret::new(
            CRYPTO_KIND_VLD0,
            BareSharedSecret::new(&output_key_material),
        ))
    }

    #[cfg_attr(
        feature = "instrument",
        instrument(level = "trace", target = "crypto", skip_all, fields(__VEILID_LOG_KEY = self.registry.log_key()))
    )]
    fn random_nonce(&self) -> Nonce {
        let mut nonce = [0u8; VLD0_NONCE_LENGTH];
        random_bytes(&mut nonce);
        Nonce::new(&nonce)
    }

    #[cfg_attr(
        feature = "instrument",
        instrument(level = "trace", target = "crypto", skip_all, fields(__VEILID_LOG_KEY = self.registry.log_key()))
    )]
    fn random_shared_secret(&self) -> SharedSecret {
        let mut s = [0u8; VLD0_SHARED_SECRET_LENGTH];
        random_bytes(&mut s);
        SharedSecret::new(CRYPTO_KIND_VLD0, BareSharedSecret::new(&s))
    }

    #[cfg_attr(
        feature = "instrument",
        instrument(level = "trace", target = "crypto", skip_all, fields(__VEILID_LOG_KEY = self.registry.log_key()))
    )]
    fn compute_dh(&self, key: &PublicKey, secret: &SecretKey) -> VeilidAPIResult<SharedSecret> {
        let pk_xd = public_to_x25519_pk(key)?;
        let sk_xd = secret_to_x25519_sk(secret)?;

        let dh_bytes = opt::x25519_diffie_hellman(&sk_xd, &pk_xd).map_err(|e| match e {
            opt::Error::NotContributory => VeilidAPIError::generic(e),
            e => VeilidAPIError::internal(e),
        })?;

        let mut hasher = opt::Blake3Hasher::new();
        hasher.update(VLD0_DOMAIN_CRYPT);
        hasher.update(&dh_bytes);
        let output = hasher.finalize();

        Ok(SharedSecret::new(
            CRYPTO_KIND_VLD0,
            BareSharedSecret::new(&output),
        ))
    }

    #[cfg_attr(
        feature = "instrument",
        instrument(level = "trace", target = "crypto", skip_all, fields(__VEILID_LOG_KEY = self.registry.log_key()))
    )]
    fn hpke_seal(
        &self,
        recipient: &EncapsulationKey,
        aad: &[u8],
        plaintext: &[u8],
    ) -> VeilidAPIResult<Vec<u8>> {
        let (enc, ciphertext) = opt::hpke_x25519_seal(
            recipient.ref_value(),
            &hpke_info(CRYPTO_KIND_VLD0),
            aad,
            plaintext,
            &mut random_bytes,
        )
        .map_err(|e| match e {
            opt::Error::InvalidKey => VeilidAPIError::invalid_argument("hpke_seal", "recipient", e),
            e => VeilidAPIError::generic(e),
        })?;
        Ok(build_sealed(CRYPTO_KIND_VLD0, &enc, &ciphertext))
    }

    #[cfg_attr(
        feature = "instrument",
        instrument(level = "trace", target = "crypto", skip_all, fields(__VEILID_LOG_KEY = self.registry.log_key()))
    )]
    fn hpke_open(
        &self,
        secret: &DecapsulationKey,
        aad: &[u8],
        sealed: &[u8],
    ) -> VeilidAPIResult<Vec<u8>> {
        let (enc, ciphertext) = parse_sealed(CRYPTO_KIND_VLD0, VLD0_HPKE_ENC_LENGTH, sealed)?;
        opt::hpke_x25519_open(
            secret.ref_value(),
            enc,
            &hpke_info(CRYPTO_KIND_VLD0),
            aad,
            ciphertext,
        )
        .map_err(|e| match e {
            opt::Error::InvalidKey => VeilidAPIError::invalid_argument("hpke_open", "secret", e),
            e => VeilidAPIError::generic(e),
        })
    }

    #[cfg_attr(
        feature = "instrument",
        instrument(level = "trace", target = "crypto", skip_all, fields(__VEILID_LOG_KEY = self.registry.log_key()))
    )]
    fn generate_keypair(&self) -> KeyPair {
        vld0_generate_keypair()
    }

    #[cfg_attr(
        feature = "instrument",
        instrument(level = "trace", target = "crypto", skip_all, fields(__VEILID_LOG_KEY = self.registry.log_key()))
    )]
    fn generate_kem_keypair(&self) -> KemKeyPair {
        vld0_generate_kem_keypair()
    }

    #[cfg_attr(
        feature = "instrument",
        instrument(level = "trace", target = "crypto", skip_all, fields(__VEILID_LOG_KEY = self.registry.log_key()))
    )]
    fn encapsulation_key_from_signing_key(
        &self,
        key: &PublicKey,
    ) -> VeilidAPIResult<EncapsulationKey> {
        let pk_xd = public_to_x25519_pk(key).map_err(|e| {
            VeilidAPIError::invalid_argument(
                "encapsulation_key_from_signing_key",
                "key",
                map_to_string(e),
            )
        })?;
        Ok(EncapsulationKey::new(
            CRYPTO_KIND_VLD0,
            BareEncapsulationKey::new(&pk_xd),
        ))
    }

    #[cfg_attr(
        feature = "instrument",
        instrument(level = "trace", target = "crypto", skip_all, fields(__VEILID_LOG_KEY = self.registry.log_key()))
    )]
    fn decapsulation_key_from_signing_secret(
        &self,
        secret: &SecretKey,
    ) -> VeilidAPIResult<DecapsulationKey> {
        let sk_xd = secret_to_x25519_sk(secret).map_err(|e| {
            VeilidAPIError::invalid_argument(
                "decapsulation_key_from_signing_secret",
                "secret",
                map_to_string(e),
            )
        })?;
        Ok(DecapsulationKey::new(
            CRYPTO_KIND_VLD0,
            BareDecapsulationKey::new(&sk_xd),
        ))
    }

    #[cfg_attr(
        feature = "instrument",
        instrument(level = "trace", target = "crypto", skip_all, fields(__VEILID_LOG_KEY = self.registry.log_key()))
    )]
    fn generate_hash(&self, data: &[u8]) -> HashDigest {
        HashDigest::new(
            CRYPTO_KIND_VLD0,
            BareHashDigest::new(&opt::blake3_hash(data)),
        )
    }

    #[cfg_attr(
        feature = "instrument",
        instrument(level = "trace", target = "crypto", skip_all, fields(__VEILID_LOG_KEY = self.registry.log_key()))
    )]
    fn generate_hash_reader(&self, reader: &mut dyn std::io::Read) -> VeilidAPIResult<PublicKey> {
        let mut hasher = opt::Blake3Hasher::new();
        std::io::copy(reader, &mut hasher).map_err(|e| VeilidAPIError::generic(e.to_string()))?;
        Ok(PublicKey::new(
            CRYPTO_KIND_VLD0,
            BarePublicKey::new(&hasher.finalize()),
        ))
    }

    // Validation
    fn shared_secret_length(&self) -> usize {
        VLD0_SHARED_SECRET_LENGTH
    }
    fn nonce_length(&self) -> usize {
        VLD0_NONCE_LENGTH
    }
    fn hash_digest_length(&self) -> usize {
        VLD0_HASH_DIGEST_LENGTH
    }
    fn public_key_length(&self) -> usize {
        VLD0_PUBLIC_KEY_LENGTH
    }
    fn secret_key_length(&self) -> usize {
        VLD0_SECRET_KEY_LENGTH
    }
    fn encapsulation_key_length(&self) -> usize {
        VLD0_ENCAPSULATION_KEY_LENGTH
    }
    fn decapsulation_key_length(&self) -> usize {
        VLD0_DECAPSULATION_KEY_LENGTH
    }
    fn signature_length(&self) -> usize {
        VLD0_SIGNATURE_LENGTH
    }
    fn default_salt_length(&self) -> usize {
        16
    }
    fn aead_overhead(&self) -> usize {
        VLD0_AEAD_OVERHEAD
    }

    #[cfg_attr(
        feature = "instrument",
        instrument(level = "trace", target = "crypto", skip_all, fields(__VEILID_LOG_KEY = self.registry.log_key()))
    )]
    fn validate_keypair(
        &self,
        public_key: &PublicKey,
        secret_key: &SecretKey,
    ) -> VeilidAPIResult<bool> {
        self.check_public_key(public_key)?;
        self.check_secret_key(secret_key)?;

        let data = vec![0u8; 512];
        let Ok(sig) = self.sign(public_key, secret_key, &data) else {
            return Ok(false);
        };
        let Ok(v) = self.verify(public_key, &data, &sig) else {
            return Ok(false);
        };
        Ok(v)
    }

    #[cfg_attr(
        feature = "instrument",
        instrument(level = "trace", target = "crypto", skip_all, fields(__VEILID_LOG_KEY = self.registry.log_key()))
    )]
    fn validate_hash(&self, data: &[u8], hash_digest: &HashDigest) -> VeilidAPIResult<bool> {
        self.check_hash_digest(hash_digest)?;

        let bytes = opt::blake3_hash(data);

        Ok(bytes == *hash_digest.ref_value().bytes())
    }

    #[cfg_attr(
        feature = "instrument",
        instrument(level = "trace", target = "crypto", skip_all, fields(__VEILID_LOG_KEY = self.registry.log_key()))
    )]
    fn validate_hash_reader(
        &self,
        reader: &mut dyn std::io::Read,
        hash_digest: &HashDigest,
    ) -> VeilidAPIResult<bool> {
        self.check_hash_digest(hash_digest)?;

        let mut hasher = opt::Blake3Hasher::new();
        std::io::copy(reader, &mut hasher).map_err(|e| VeilidAPIError::generic(e.to_string()))?;
        let bytes = hasher.finalize();
        Ok(bytes == *hash_digest.ref_value().bytes())
    }

    // Authentication
    #[cfg_attr(
        feature = "instrument",
        instrument(level = "trace", target = "crypto", skip_all, fields(__VEILID_LOG_KEY = self.registry.log_key()))
    )]
    fn sign(
        &self,
        public_key: &PublicKey,
        secret_key: &SecretKey,
        data: &[u8],
    ) -> VeilidAPIResult<Signature> {
        self.check_public_key(public_key)?;
        self.check_secret_key(secret_key)?;

        let sig_bytes = opt::ed25519_sign_prehashed(
            public_key.ref_value(),
            secret_key.ref_value(),
            data,
            VLD0_DOMAIN_SIGN,
        )
        .map_err(|e| match e {
            opt::Error::InvalidKey => VeilidAPIError::parse_error("Keypair is invalid", e),
            e => VeilidAPIError::internal(e),
        })?;

        Ok(Signature::new(
            CRYPTO_KIND_VLD0,
            BareSignature::new(&sig_bytes),
        ))
    }

    fn sign_in_place(
        &self,
        public_key: &PublicKey,
        secret_key: &SecretKey,
        data: &mut [u8],
        range: Range<usize>,
        sig_idx: usize,
    ) -> VeilidAPIResult<()> {
        self.check_public_key(public_key)?;
        self.check_secret_key(secret_key)?;

        let message = data.get(range.clone()).ok_or_else(|| {
            VeilidAPIError::invalid_argument("range is out of bounds", "range", range)
        })?;

        let sig_bytes = opt::ed25519_sign_prehashed(
            public_key.ref_value(),
            secret_key.ref_value(),
            message,
            VLD0_DOMAIN_SIGN,
        )
        .map_err(|e| match e {
            opt::Error::InvalidKey => VeilidAPIError::parse_error("Keypair is invalid", e),
            e => VeilidAPIError::internal(e),
        })?;

        data.get_mut(sig_idx..sig_idx + VLD0_SIGNATURE_LENGTH)
            .ok_or_else(|| {
                VeilidAPIError::parse_error("signature index is out of bounds", sig_idx)
            })?
            .copy_from_slice(&sig_bytes);

        Ok(())
    }

    #[cfg_attr(
        feature = "instrument",
        instrument(level = "trace", target = "crypto", skip_all, fields(__VEILID_LOG_KEY = self.registry.log_key()))
    )]
    fn verify(
        &self,
        public_key: &PublicKey,
        data: &[u8],
        signature: &Signature,
    ) -> VeilidAPIResult<bool> {
        self.check_public_key(public_key)?;
        self.check_signature(signature)?;

        opt::ed25519_verify_prehashed(
            public_key.ref_value(),
            data,
            VLD0_DOMAIN_SIGN,
            signature.ref_value(),
        )
        .map_err(|e| match e {
            opt::Error::InvalidKey => {
                VeilidAPIError::parse_error("Public key is invalid", public_key.clone())
            }
            e => VeilidAPIError::internal(e),
        })
    }

    fn verify_in_place(
        &self,
        public_key: &PublicKey,
        data: &[u8],
        range: Range<usize>,
        sig_idx: usize,
    ) -> VeilidAPIResult<bool> {
        self.check_public_key(public_key)?;

        let message = data
            .get(range)
            .ok_or_else(|| VeilidAPIError::internal("range is out of bounds"))?;
        let sig = data
            .get(sig_idx..sig_idx + VLD0_SIGNATURE_LENGTH)
            .ok_or_else(|| VeilidAPIError::internal("signature index is out of bounds"))?;

        opt::ed25519_verify_prehashed(public_key.ref_value(), message, VLD0_DOMAIN_SIGN, sig)
            .map_err(|e| match e {
                opt::Error::InvalidKey => {
                    VeilidAPIError::parse_error("Public key is invalid", public_key.clone())
                }
                e => VeilidAPIError::internal(e),
            })
    }

    // AEAD Encrypt/Decrypt
    #[cfg_attr(
        feature = "instrument",
        instrument(level = "trace", target = "crypto", skip_all, fields(__VEILID_LOG_KEY = self.registry.log_key()))
    )]
    fn decrypt_in_place_aead(
        &self,
        body: &mut dyn CryptoSystemBuffer,
        nonce: &Nonce,
        shared_secret: &SharedSecret,
        associated_data: Option<&[u8]>,
    ) -> VeilidAPIResult<()> {
        self.check_shared_secret(shared_secret)?;

        let mut buffer = BufferWrapper::new(body);
        opt::xchacha20poly1305_decrypt_in_place(
            shared_secret.ref_value(),
            nonce.as_ref(),
            associated_data.unwrap_or(b""),
            &mut buffer,
        )
        .map_err(VeilidAPIError::generic)
    }

    #[cfg_attr(
        feature = "instrument",
        instrument(level = "trace", target = "crypto", skip_all, fields(__VEILID_LOG_KEY = self.registry.log_key()))
    )]
    fn decrypt_aead(
        &self,
        body: &[u8],
        nonce: &Nonce,
        shared_secret: &SharedSecret,
        associated_data: Option<&[u8]>,
    ) -> VeilidAPIResult<Vec<u8>> {
        self.check_nonce(nonce)?;
        self.check_shared_secret(shared_secret)?;

        let mut out = body.to_vec();
        self.decrypt_in_place_aead(&mut out, nonce, shared_secret, associated_data)
            .map_err(map_to_string)
            .map_err(VeilidAPIError::generic)?;
        Ok(out)
    }

    #[cfg_attr(
        feature = "instrument",
        instrument(level = "trace", target = "crypto", skip_all, fields(__VEILID_LOG_KEY = self.registry.log_key()))
    )]
    fn encrypt_in_place_aead(
        &self,
        body: &mut dyn CryptoSystemBuffer,
        nonce: &Nonce,
        shared_secret: &SharedSecret,
        associated_data: Option<&[u8]>,
    ) -> VeilidAPIResult<()> {
        self.check_nonce(nonce)?;
        self.check_shared_secret(shared_secret)?;

        let mut buffer = BufferWrapper::new(body);
        opt::xchacha20poly1305_encrypt_in_place(
            shared_secret.ref_value(),
            nonce.as_ref(),
            associated_data.unwrap_or(b""),
            &mut buffer,
        )
        .map_err(VeilidAPIError::generic)
    }

    #[cfg_attr(
        feature = "instrument",
        instrument(level = "trace", target = "crypto", skip_all, fields(__VEILID_LOG_KEY = self.registry.log_key()))
    )]
    fn encrypt_aead(
        &self,
        body: &[u8],
        nonce: &Nonce,
        shared_secret: &SharedSecret,
        associated_data: Option<&[u8]>,
    ) -> VeilidAPIResult<Vec<u8>> {
        self.check_nonce(nonce)?;
        self.check_shared_secret(shared_secret)?;

        let mut out = body.to_vec();
        self.encrypt_in_place_aead(&mut out, nonce, shared_secret, associated_data)
            .map_err(map_to_string)
            .map_err(VeilidAPIError::generic)?;
        Ok(out)
    }

    // NoAuth Encrypt/Decrypt
    #[cfg_attr(
        feature = "instrument",
        instrument(level = "trace", target = "crypto", skip_all, fields(__VEILID_LOG_KEY = self.registry.log_key()))
    )]
    fn crypt_in_place_no_auth(
        &self,
        body: &mut [u8],
        nonce: &Nonce,
        shared_secret: &SharedSecret,
    ) -> VeilidAPIResult<()> {
        self.check_nonce(nonce)?;
        self.check_shared_secret(shared_secret)?;

        opt::xchacha20_apply_keystream(shared_secret.ref_value(), nonce.as_ref(), body)
            .map_err(VeilidAPIError::internal)
    }

    #[cfg_attr(
        feature = "instrument",
        instrument(level = "trace", target = "crypto", skip_all, fields(__VEILID_LOG_KEY = self.registry.log_key()))
    )]
    fn crypt_b2b_no_auth(
        &self,
        in_buf: &[u8],
        out_buf: &mut [u8],
        nonce: &Nonce,
        shared_secret: &SharedSecret,
    ) -> VeilidAPIResult<()> {
        self.check_nonce(nonce)?;
        self.check_shared_secret(shared_secret)?;

        opt::xchacha20_apply_keystream_b2b(
            shared_secret.ref_value(),
            nonce.as_ref(),
            in_buf,
            out_buf,
        )
        .map_err(VeilidAPIError::internal)
    }

    #[cfg_attr(
        feature = "instrument",
        instrument(level = "trace", target = "crypto", skip_all, fields(__VEILID_LOG_KEY = self.registry.log_key()))
    )]
    fn crypt_no_auth_aligned_8(
        &self,
        in_buf: &[u8],
        nonce: &Nonce,
        shared_secret: &SharedSecret,
    ) -> VeilidAPIResult<Vec<u8>> {
        self.check_nonce(nonce)?;
        self.check_shared_secret(shared_secret)?;

        let mut out_buf = zeroed_u8_vec(in_buf.len());
        self.crypt_b2b_no_auth(in_buf, &mut out_buf, nonce, shared_secret)?;
        Ok(out_buf)
    }

    #[cfg_attr(
        feature = "instrument",
        instrument(level = "trace", target = "crypto", skip_all, fields(__VEILID_LOG_KEY = self.registry.log_key()))
    )]
    fn crypt_no_auth_unaligned(
        &self,
        in_buf: &[u8],
        nonce: &Nonce,
        shared_secret: &SharedSecret,
    ) -> VeilidAPIResult<Vec<u8>> {
        self.check_nonce(nonce)?;
        self.check_shared_secret(shared_secret)?;

        let mut out_buf = zeroed_u8_vec(in_buf.len());
        self.crypt_b2b_no_auth(in_buf, &mut out_buf, nonce, shared_secret)?;
        Ok(out_buf)
    }
}
