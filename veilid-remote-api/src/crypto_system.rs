use super::*;

/// A remote crypto operation request, naming the target cryptosystem handle.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct CryptoSystemRequest {
    /// Handle identifying the cryptosystem instance to operate on.
    pub cs_id: u32,
    /// The operation to perform.
    #[serde(flatten)]
    pub cs_op: CryptoSystemRequestOp,
}

/// A response to a [`CryptoSystemRequest`], echoing the cryptosystem handle.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct CryptoSystemResponse {
    /// Handle of the cryptosystem instance the request operated on.
    pub cs_id: u32,
    /// The operation result.
    #[serde(flatten)]
    pub cs_op: CryptoSystemResponseOp,
}

/// A single remote crypto operation, mirroring the [`CryptoSystem`] trait.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "cs_op")]
pub enum CryptoSystemRequestOp {
    /// Release the cryptosystem handle.
    Release,
    /// Get the [`CryptoKind`] fourcc identifying this cryptosystem.
    Kind,
    /// Diffie-Hellman shared secret for the key pair, served from the DH cache.
    CachedDh {
        /// Peer public key.
        #[schemars(with = "String")]
        key: PublicKey,
        /// Own secret key.
        #[schemars(with = "String")]
        secret: SecretKey,
    },
    /// Raw Diffie-Hellman shared secret for the key pair, with no caching.
    ComputeDh {
        /// Peer public key.
        #[schemars(with = "String")]
        key: PublicKey,
        /// Own secret key.
        #[schemars(with = "String")]
        secret: SecretKey,
    },
    /// Domain-separated shared secret derived from a key exchange and `domain`.
    GenerateSharedSecret {
        /// Peer public key.
        #[schemars(with = "String")]
        key: PublicKey,
        /// Own secret key.
        #[schemars(with = "String")]
        secret: SecretKey,
        /// Domain separation tag; distinct values yield independent secrets.
        #[serde(with = "as_human_base64")]
        #[schemars(with = "String")]
        domain: Vec<u8>,
    },
    /// Seal `plaintext` to a recipient KEM encapsulation key with HPKE base mode (RFC 9180),
    /// single-shot. Only the recipient can open the blob; the sealer cannot decrypt what it
    /// just sealed. Callers who already share a symmetric key want `EncryptAead` instead.
    HpkeSeal {
        /// Recipient KEM encapsulation key.
        #[schemars(with = "String")]
        recipient: EncapsulationKey,
        /// Associated data; authenticated but not encrypted.
        #[serde(with = "as_human_base64")]
        #[schemars(with = "String")]
        aad: Vec<u8>,
        /// Plaintext to seal.
        #[serde(with = "as_human_base64")]
        #[schemars(with = "String")]
        plaintext: Vec<u8>,
    },
    /// Open a sealed blob produced by `HpkeSeal` with the recipient KEM decapsulation key.
    /// Only the recipient can open a sealed blob; the sealer cannot.
    HpkeOpen {
        /// Recipient KEM decapsulation key.
        #[schemars(with = "String")]
        secret: DecapsulationKey,
        /// Associated data; must match what was supplied at seal.
        #[serde(with = "as_human_base64")]
        #[schemars(with = "String")]
        aad: Vec<u8>,
        /// Sealed blob.
        #[serde(with = "as_human_base64")]
        #[schemars(with = "String")]
        sealed: Vec<u8>,
    },
    /// Fill `len` bytes from the cryptographic RNG.
    RandomBytes {
        /// Number of bytes to generate.
        len: u32,
    },
    /// Byte length of a shared secret.
    SharedSecretLength,
    /// Byte length of a nonce.
    NonceLength,
    /// Byte length of a hash digest.
    HashDigestLength,
    /// Byte length of a public key.
    PublicKeyLength,
    /// Byte length of a secret key.
    SecretKeyLength,
    /// Byte length of a KEM encapsulation key.
    EncapsulationKeyLength,
    /// Byte length of a KEM decapsulation key.
    DecapsulationKeyLength,
    /// Byte length of a signature.
    SignatureLength,
    /// Default salt length for password hashing and KDF operations.
    DefaultSaltLength,
    /// Bytes an AEAD operation adds to the ciphertext (the tag length).
    AeadOverhead,
    /// Verify a shared secret carries this cryptosystem's kind and length.
    CheckSharedSecret {
        /// Shared secret to check.
        #[schemars(with = "String")]
        secret: SharedSecret,
    },
    /// Verify a nonce has the correct length.
    CheckNonce {
        /// Nonce to check.
        #[schemars(with = "String")]
        nonce: Nonce,
    },
    /// Verify a hash digest carries this cryptosystem's kind and length.
    CheckHashDigest {
        /// Hash digest to check.
        #[schemars(with = "String")]
        digest: HashDigest,
    },
    /// Verify a public key carries this cryptosystem's kind and length.
    CheckPublicKey {
        /// Public key to check.
        #[schemars(with = "String")]
        key: PublicKey,
    },
    /// Verify a secret key carries this cryptosystem's kind and length.
    CheckSecretKey {
        /// Secret key to check.
        #[schemars(with = "String")]
        key: SecretKey,
    },
    /// Verify a signature carries this cryptosystem's kind and length.
    CheckSignature {
        /// Signature to check.
        #[schemars(with = "String")]
        signature: Signature,
    },
    /// Hash a password with a salt, returning a PHC hash string.
    HashPassword {
        /// Password bytes.
        #[serde(with = "as_human_base64")]
        #[schemars(with = "String")]
        password: Vec<u8>,
        /// Salt bytes.
        #[serde(with = "as_human_base64")]
        #[schemars(with = "String")]
        salt: Vec<u8>,
    },
    /// Check a password against a PHC hash string.
    VerifyPassword {
        /// Password bytes.
        #[serde(with = "as_human_base64")]
        #[schemars(with = "String")]
        password: Vec<u8>,
        /// PHC hash string to check against.
        password_hash: String,
    },
    /// Derive a deterministic shared secret from a password and salt via KDF.
    DeriveSharedSecret {
        /// Password bytes.
        #[serde(with = "as_human_base64")]
        #[schemars(with = "String")]
        password: Vec<u8>,
        /// Salt bytes.
        #[serde(with = "as_human_base64")]
        #[schemars(with = "String")]
        salt: Vec<u8>,
    },
    /// A fresh random nonce.
    RandomNonce,
    /// A fresh random shared secret.
    RandomSharedSecret,
    /// Generate a fresh random signing key pair.
    GenerateKeyPair,
    /// Generate a fresh random KEM key pair for this cryptosystem.
    GenerateKemKeyPair,
    /// Derive the KEM encapsulation key corresponding to a signing public key (VLD0-only bridge).
    EncapsulationKeyFromSigningKey {
        /// Signing public key.
        #[schemars(with = "String")]
        key: PublicKey,
    },
    /// Derive the KEM decapsulation key corresponding to a signing secret key (VLD0-only bridge).
    DecapsulationKeyFromSigningSecret {
        /// Signing secret key.
        #[schemars(with = "String")]
        secret: SecretKey,
    },
    /// Hash a byte slice, returning a digest tagged with this kind.
    GenerateHash {
        /// Data to hash.
        #[serde(with = "as_human_base64")]
        #[schemars(with = "String")]
        data: Vec<u8>,
    },
    /// Check that a public and secret key form a usable signing pair.
    ValidateKeyPair {
        /// Public key.
        #[schemars(with = "String")]
        key: PublicKey,
        /// Secret key.
        #[schemars(with = "String")]
        secret: SecretKey,
    },
    /// Recompute the hash of `data` and compare it against `hash_digest`.
    ValidateHash {
        /// Data to hash.
        #[serde(with = "as_human_base64")]
        #[schemars(with = "String")]
        data: Vec<u8>,
        /// Expected digest.
        #[schemars(with = "String")]
        hash_digest: HashDigest,
    },
    /// Sign `data` with the key pair, returning a detached signature.
    Sign {
        /// Public key.
        #[schemars(with = "String")]
        key: PublicKey,
        /// Secret key.
        #[schemars(with = "String")]
        secret: SecretKey,
        /// Data to sign.
        #[serde(with = "as_human_base64")]
        #[schemars(with = "String")]
        data: Vec<u8>,
    },
    /// Verify a detached `signature` over `data` for `key`.
    Verify {
        /// Public key.
        #[schemars(with = "String")]
        key: PublicKey,
        /// Signed data.
        #[serde(with = "as_human_base64")]
        #[schemars(with = "String")]
        data: Vec<u8>,
        /// Detached signature to verify.
        #[schemars(with = "String")]
        signature: Signature,
    },
    /// Decrypt and authenticate `body`, returning the plaintext.
    DecryptAead {
        /// Ciphertext with appended authentication tag.
        #[serde(with = "as_human_base64")]
        #[schemars(with = "String")]
        body: Vec<u8>,
        /// Nonce.
        #[schemars(with = "String")]
        nonce: Nonce,
        /// Shared secret.
        #[schemars(with = "String")]
        shared_secret: SharedSecret,
        /// Associated data; must match what was supplied at encryption.
        #[serde(with = "as_human_opt_base64")]
        #[schemars(with = "Option<String>")]
        associated_data: Option<Vec<u8>>,
    },
    /// Encrypt and authenticate `body`, returning the ciphertext with tag.
    EncryptAead {
        /// Plaintext to encrypt.
        #[serde(with = "as_human_base64")]
        #[schemars(with = "String")]
        body: Vec<u8>,
        /// Nonce; never reuse the same nonce with the same shared secret.
        #[schemars(with = "String")]
        nonce: Nonce,
        /// Shared secret.
        #[schemars(with = "String")]
        shared_secret: SharedSecret,
        /// Associated data; authenticated but not encrypted.
        #[serde(with = "as_human_opt_base64")]
        #[schemars(with = "Option<String>")]
        associated_data: Option<Vec<u8>>,
    },
    /// Stream-cipher `body`, without authentication (confidentiality only).
    CryptNoAuth {
        /// Data to encrypt or decrypt.
        #[serde(with = "as_human_base64")]
        #[schemars(with = "String")]
        body: Vec<u8>,
        /// Nonce.
        #[schemars(with = "String")]
        nonce: Nonce,
        /// Shared secret.
        #[schemars(with = "String")]
        shared_secret: SharedSecret,
    },
}
/// The result of a [`CryptoSystemRequestOp`].
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "cs_op")]
pub enum CryptoSystemResponseOp {
    /// The request named a cryptosystem handle that does not exist.
    InvalidId,
    /// The handle was released.
    Release,
    /// The [`CryptoKind`] fourcc identifying this cryptosystem.
    Kind {
        /// Cryptosystem kind.
        #[schemars(with = "String")]
        value: CryptoKind,
    },
    /// Cached Diffie-Hellman shared secret.
    CachedDh {
        /// Shared secret, or an error.
        #[serde(flatten)]
        #[schemars(with = "ApiResult<String>")]
        result: ApiResultWithString<SharedSecret>,
    },
    /// Raw Diffie-Hellman shared secret.
    ComputeDh {
        /// Shared secret, or an error.
        #[serde(flatten)]
        #[schemars(with = "ApiResult<String>")]
        result: ApiResultWithString<SharedSecret>,
    },
    /// Domain-separated shared secret.
    GenerateSharedSecret {
        /// Shared secret, or an error.
        #[serde(flatten)]
        #[schemars(with = "ApiResult<String>")]
        result: ApiResultWithString<SharedSecret>,
    },
    /// HPKE sealed blob.
    HpkeSeal {
        /// Sealed blob, or an error.
        #[serde(flatten)]
        #[schemars(with = "ApiResult<String>")]
        result: ApiResultWithVecU8,
    },
    /// HPKE opened plaintext.
    HpkeOpen {
        /// Plaintext, or an error if the blob is malformed or decryption fails.
        #[serde(flatten)]
        #[schemars(with = "ApiResult<String>")]
        result: ApiResultWithVecU8,
    },
    /// Random bytes.
    RandomBytes {
        /// Generated bytes.
        #[serde(with = "as_human_base64")]
        #[schemars(with = "String")]
        value: Vec<u8>,
    },
    /// Byte length of a shared secret.
    SharedSecretLength {
        /// Length in bytes.
        value: u32,
    },
    /// Byte length of a nonce.
    NonceLength {
        /// Length in bytes.
        value: u32,
    },
    /// Byte length of a hash digest.
    HashDigestLength {
        /// Length in bytes.
        value: u32,
    },
    /// Byte length of a public key.
    PublicKeyLength {
        /// Length in bytes.
        value: u32,
    },
    /// Byte length of a secret key.
    SecretKeyLength {
        /// Length in bytes.
        value: u32,
    },
    /// Byte length of a KEM encapsulation key.
    EncapsulationKeyLength {
        /// Length in bytes.
        value: u32,
    },
    /// Byte length of a KEM decapsulation key.
    DecapsulationKeyLength {
        /// Length in bytes.
        value: u32,
    },
    /// Byte length of a signature.
    SignatureLength {
        /// Length in bytes.
        value: u32,
    },
    /// Default salt length for password hashing and KDF operations.
    DefaultSaltLength {
        /// Length in bytes.
        value: u32,
    },
    /// Bytes an AEAD operation adds to the ciphertext.
    AeadOverhead {
        /// Length in bytes.
        value: u32,
    },
    /// Shared secret kind and length check.
    CheckSharedSecret {
        /// Ok if valid, or an error describing the mismatch.
        #[serde(flatten)]
        result: ApiResult<()>,
    },
    /// Nonce length check.
    CheckNonce {
        /// Ok if valid, or an error describing the mismatch.
        #[serde(flatten)]
        result: ApiResult<()>,
    },
    /// Hash digest kind and length check.
    CheckHashDigest {
        /// Ok if valid, or an error describing the mismatch.
        #[serde(flatten)]
        result: ApiResult<()>,
    },
    /// Public key kind and length check.
    CheckPublicKey {
        /// Ok if valid, or an error describing the mismatch.
        #[serde(flatten)]
        result: ApiResult<()>,
    },
    /// Secret key kind and length check.
    CheckSecretKey {
        /// Ok if valid, or an error describing the mismatch.
        #[serde(flatten)]
        result: ApiResult<()>,
    },
    /// Signature kind and length check.
    CheckSignature {
        /// Ok if valid, or an error describing the mismatch.
        #[serde(flatten)]
        result: ApiResult<()>,
    },
    /// Password hash result.
    HashPassword {
        /// PHC hash string, or an error.
        #[serde(flatten)]
        result: ApiResult<String>,
    },
    /// Password verification result.
    VerifyPassword {
        /// True if the password matches the hash, false otherwise.
        #[serde(flatten)]
        result: ApiResult<bool>,
    },
    /// KDF-derived shared secret.
    DeriveSharedSecret {
        /// Shared secret, or an error.
        #[serde(flatten)]
        #[schemars(with = "ApiResult<String>")]
        result: ApiResultWithString<SharedSecret>,
    },
    /// Random nonce.
    RandomNonce {
        /// Generated nonce.
        #[schemars(with = "String")]
        value: Nonce,
    },
    /// Random shared secret.
    RandomSharedSecret {
        /// Generated shared secret.
        #[schemars(with = "String")]
        value: SharedSecret,
    },
    /// Generated signing key pair.
    GenerateKeyPair {
        /// Generated key pair.
        #[schemars(with = "String")]
        value: KeyPair,
    },
    /// Generated KEM key pair.
    GenerateKemKeyPair {
        /// Generated KEM key pair.
        #[schemars(with = "String")]
        value: KemKeyPair,
    },
    /// KEM encapsulation key derived from a signing public key.
    EncapsulationKeyFromSigningKey {
        /// Encapsulation key, or an error.
        #[serde(flatten)]
        #[schemars(with = "ApiResult<String>")]
        result: ApiResultWithString<EncapsulationKey>,
    },
    /// KEM decapsulation key derived from a signing secret key.
    DecapsulationKeyFromSigningSecret {
        /// Decapsulation key, or an error.
        #[serde(flatten)]
        #[schemars(with = "ApiResult<String>")]
        result: ApiResultWithString<DecapsulationKey>,
    },
    /// Hash digest of the data.
    GenerateHash {
        /// Digest tagged with this cryptosystem's kind.
        #[schemars(with = "String")]
        value: HashDigest,
    },
    /// Key pair validation result.
    ValidateKeyPair {
        /// True if the keys form a usable signing pair, false otherwise.
        #[serde(flatten)]
        result: ApiResult<bool>,
    },
    /// Hash validation result.
    ValidateHash {
        /// True if the data hashes to the expected digest, false otherwise.
        #[serde(flatten)]
        result: ApiResult<bool>,
    },
    /// Detached signature over the data.
    Sign {
        /// Signature, or an error.
        #[serde(flatten)]
        #[schemars(with = "ApiResult<String>")]
        result: ApiResultWithString<Signature>,
    },
    /// Signature verification result.
    Verify {
        /// True if the signature is valid, false otherwise.
        #[serde(flatten)]
        result: ApiResult<bool>,
    },
    /// AEAD-decrypted plaintext.
    DecryptAead {
        /// Plaintext, or an error if authentication fails.
        #[serde(flatten)]
        #[schemars(with = "ApiResult<String>")]
        result: ApiResultWithVecU8,
    },
    /// AEAD-encrypted ciphertext with appended tag.
    EncryptAead {
        /// Ciphertext, or an error.
        #[serde(flatten)]
        #[schemars(with = "ApiResult<String>")]
        result: ApiResultWithVecU8,
    },
    /// Stream-cipher output (unauthenticated).
    CryptNoAuth {
        /// Encrypted or decrypted bytes, or an error.
        #[serde(flatten)]
        #[schemars(with = "ApiResult<String>")]
        result: ApiResultWithVecU8,
    },
}
