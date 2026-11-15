//! `MeteredCryptoSystem`: a `CryptoSystem` decorator that counts and times each
//! operation, installed once in `Crypto::new` so every sync guard, async guard,
//! and `Crypto`-level helper funnels through it. It wraps the trait call itself,
//! below the async guard's executor offload, so the measured latency is the pure
//! crypto CPU cost, not the rayon queue wait.
//!
//! Compiled only with `stats`; when off, `Crypto` stores the bare cryptosystems
//! and this decorator does not exist.

use std::ops::Range;

use super::*;

/// One instrumented crypto operation class. Related trait methods collapse into
/// one bucket (both sign variants -> `Sign`, all four stream ciphers -> `CryptNoAuth`).
#[derive(Copy, Clone)]
#[repr(usize)]
enum CryptoOp {
    CachedDh,
    ComputeDh,
    GenerateSharedSecret,
    Sign,
    Verify,
    ValidateKeypair,
    GenerateHash,
    GenerateKeypair,
    GenerateKemKeypair,
    KemBridge,
    Random,
    EncryptAead,
    DecryptAead,
    CryptNoAuth,
    HpkeSeal,
    HpkeOpen,
    HashPassword,
    VerifyPassword,
    DeriveSharedSecret,
}

impl CryptoOp {
    const ALL: [CryptoOp; CRYPTO_OP_COUNT] = [
        CryptoOp::CachedDh,
        CryptoOp::ComputeDh,
        CryptoOp::GenerateSharedSecret,
        CryptoOp::Sign,
        CryptoOp::Verify,
        CryptoOp::ValidateKeypair,
        CryptoOp::GenerateHash,
        CryptoOp::GenerateKeypair,
        CryptoOp::GenerateKemKeypair,
        CryptoOp::KemBridge,
        CryptoOp::Random,
        CryptoOp::EncryptAead,
        CryptoOp::DecryptAead,
        CryptoOp::CryptNoAuth,
        CryptoOp::HpkeSeal,
        CryptoOp::HpkeOpen,
        CryptoOp::HashPassword,
        CryptoOp::VerifyPassword,
        CryptoOp::DeriveSharedSecret,
    ];

    fn name(self) -> &'static str {
        match self {
            CryptoOp::CachedDh => "cached_dh",
            CryptoOp::ComputeDh => "compute_dh",
            CryptoOp::GenerateSharedSecret => "generate_shared_secret",
            CryptoOp::Sign => "sign",
            CryptoOp::Verify => "verify",
            CryptoOp::ValidateKeypair => "validate_keypair",
            CryptoOp::GenerateHash => "generate_hash",
            CryptoOp::GenerateKeypair => "generate_keypair",
            CryptoOp::GenerateKemKeypair => "generate_kem_keypair",
            CryptoOp::KemBridge => "kem_bridge",
            CryptoOp::Random => "random",
            CryptoOp::EncryptAead => "encrypt_aead",
            CryptoOp::DecryptAead => "decrypt_aead",
            CryptoOp::CryptNoAuth => "crypt_no_auth",
            CryptoOp::HpkeSeal => "hpke_seal",
            CryptoOp::HpkeOpen => "hpke_open",
            CryptoOp::HashPassword => "hash_password",
            CryptoOp::VerifyPassword => "verify_password",
            CryptoOp::DeriveSharedSecret => "derive_shared_secret",
        }
    }
}

const CRYPTO_OP_COUNT: usize = 19;

#[derive(Default)]
struct CryptoOpAccounting {
    count: u64,
    accounting: LatencyStatsAccounting,
}

/// One op's telemetry, read by the crypto stats snapshot
pub(super) struct CryptoOpSnapshot {
    pub kind: &'static str,
    pub count: u64,
    pub latency: LatencyStats,
}

/// Per-op count + rolling latency. Each op has its own lock so concurrent
/// crypto on rayon workers contends only within an op class, never globally.
pub(crate) struct CryptoStatsAccounting {
    ops: [Mutex<CryptoOpAccounting>; CRYPTO_OP_COUNT],
}

impl CryptoStatsAccounting {
    pub(crate) fn new() -> Self {
        Self {
            ops: std::array::from_fn(|_| Mutex::new(CryptoOpAccounting::default())),
        }
    }

    fn record(&self, op: CryptoOp, dur: TimestampDuration) {
        let mut a = self.ops[op as usize].lock();
        a.count += 1;
        a.accounting.record_latency(dur);
    }

    pub(super) fn snapshot(&self) -> Vec<CryptoOpSnapshot> {
        CryptoOp::ALL
            .iter()
            .filter_map(|&op| {
                let a = self.ops[op as usize].lock();
                (a.count > 0).then(|| CryptoOpSnapshot {
                    kind: op.name(),
                    count: a.count,
                    latency: a.accounting.latency_stats().unwrap_or_default(),
                })
            })
            .collect()
    }
}

/// Times `$e` and records it under `$op`. The clock stops before the record, so
/// the accounting cost never inflates the measured latency.
macro_rules! timed {
    ($self:ident, $op:expr, $e:expr) => {{
        let start = Timestamp::now_non_decreasing();
        let r = $e;
        $self
            .stats
            .record($op, TimestampDuration::since_non_decreasing(start));
        r
    }};
}

pub(crate) struct MeteredCryptoSystem {
    inner: Arc<dyn CryptoSystem + Send + Sync>,
    stats: Arc<CryptoStatsAccounting>,
}

impl MeteredCryptoSystem {
    pub(crate) fn new(
        inner: Arc<dyn CryptoSystem + Send + Sync>,
        stats: Arc<CryptoStatsAccounting>,
    ) -> Self {
        Self { inner, stats }
    }
}

// Accessors, lengths and the defaulted `check_*` validators pass through untimed; the defaulted
// `generate_shared_secret` is overridden so its dh + hash count once, as one bucket
impl CryptoSystem for MeteredCryptoSystem {
    fn kind(&self) -> CryptoKind {
        self.inner.kind()
    }
    fn crypto(&self) -> VeilidComponentGuard<'_, Crypto> {
        self.inner.crypto()
    }

    fn cached_dh(&self, key: &PublicKey, secret: &SecretKey) -> VeilidAPIResult<SharedSecret> {
        timed!(self, CryptoOp::CachedDh, self.inner.cached_dh(key, secret))
    }

    fn random_bytes(&self, len: usize) -> Vec<u8> {
        timed!(self, CryptoOp::Random, self.inner.random_bytes(len))
    }
    fn hash_password(&self, password: &[u8], salt: &[u8]) -> VeilidAPIResult<String> {
        timed!(
            self,
            CryptoOp::HashPassword,
            self.inner.hash_password(password, salt)
        )
    }
    fn verify_password(&self, password: &[u8], password_hash: &str) -> VeilidAPIResult<bool> {
        timed!(
            self,
            CryptoOp::VerifyPassword,
            self.inner.verify_password(password, password_hash)
        )
    }
    fn derive_shared_secret(&self, password: &[u8], salt: &[u8]) -> VeilidAPIResult<SharedSecret> {
        timed!(
            self,
            CryptoOp::DeriveSharedSecret,
            self.inner.derive_shared_secret(password, salt)
        )
    }
    fn random_nonce(&self) -> Nonce {
        timed!(self, CryptoOp::Random, self.inner.random_nonce())
    }
    fn random_shared_secret(&self) -> SharedSecret {
        timed!(self, CryptoOp::Random, self.inner.random_shared_secret())
    }
    fn compute_dh(&self, key: &PublicKey, secret: &SecretKey) -> VeilidAPIResult<SharedSecret> {
        timed!(
            self,
            CryptoOp::ComputeDh,
            self.inner.compute_dh(key, secret)
        )
    }
    fn generate_shared_secret(
        &self,
        key: &PublicKey,
        secret: &SecretKey,
        domain: &[u8],
    ) -> VeilidAPIResult<SharedSecret> {
        timed!(
            self,
            CryptoOp::GenerateSharedSecret,
            self.inner.generate_shared_secret(key, secret, domain)
        )
    }
    fn hpke_seal(
        &self,
        recipient: &EncapsulationKey,
        aad: &[u8],
        plaintext: &[u8],
    ) -> VeilidAPIResult<Vec<u8>> {
        timed!(
            self,
            CryptoOp::HpkeSeal,
            self.inner.hpke_seal(recipient, aad, plaintext)
        )
    }
    fn hpke_open(
        &self,
        secret: &DecapsulationKey,
        aad: &[u8],
        sealed: &[u8],
    ) -> VeilidAPIResult<Vec<u8>> {
        timed!(
            self,
            CryptoOp::HpkeOpen,
            self.inner.hpke_open(secret, aad, sealed)
        )
    }
    fn generate_keypair(&self) -> KeyPair {
        timed!(
            self,
            CryptoOp::GenerateKeypair,
            self.inner.generate_keypair()
        )
    }
    fn generate_kem_keypair(&self) -> KemKeyPair {
        timed!(
            self,
            CryptoOp::GenerateKemKeypair,
            self.inner.generate_kem_keypair()
        )
    }
    fn encapsulation_key_from_signing_key(
        &self,
        key: &PublicKey,
    ) -> VeilidAPIResult<EncapsulationKey> {
        timed!(
            self,
            CryptoOp::KemBridge,
            self.inner.encapsulation_key_from_signing_key(key)
        )
    }
    fn decapsulation_key_from_signing_secret(
        &self,
        secret: &SecretKey,
    ) -> VeilidAPIResult<DecapsulationKey> {
        timed!(
            self,
            CryptoOp::KemBridge,
            self.inner.decapsulation_key_from_signing_secret(secret)
        )
    }
    fn generate_hash(&self, data: &[u8]) -> HashDigest {
        timed!(self, CryptoOp::GenerateHash, self.inner.generate_hash(data))
    }
    fn generate_hash_reader(&self, reader: &mut dyn std::io::Read) -> VeilidAPIResult<PublicKey> {
        timed!(
            self,
            CryptoOp::GenerateHash,
            self.inner.generate_hash_reader(reader)
        )
    }

    fn shared_secret_length(&self) -> usize {
        self.inner.shared_secret_length()
    }
    fn nonce_length(&self) -> usize {
        self.inner.nonce_length()
    }
    fn hash_digest_length(&self) -> usize {
        self.inner.hash_digest_length()
    }
    fn public_key_length(&self) -> usize {
        self.inner.public_key_length()
    }
    fn secret_key_length(&self) -> usize {
        self.inner.secret_key_length()
    }
    fn encapsulation_key_length(&self) -> usize {
        self.inner.encapsulation_key_length()
    }
    fn decapsulation_key_length(&self) -> usize {
        self.inner.decapsulation_key_length()
    }
    fn signature_length(&self) -> usize {
        self.inner.signature_length()
    }
    fn default_salt_length(&self) -> usize {
        self.inner.default_salt_length()
    }
    fn aead_overhead(&self) -> usize {
        self.inner.aead_overhead()
    }

    fn validate_keypair(&self, key: &PublicKey, secret: &SecretKey) -> VeilidAPIResult<bool> {
        timed!(
            self,
            CryptoOp::ValidateKeypair,
            self.inner.validate_keypair(key, secret)
        )
    }
    fn validate_hash(&self, data: &[u8], hash: &HashDigest) -> VeilidAPIResult<bool> {
        timed!(
            self,
            CryptoOp::GenerateHash,
            self.inner.validate_hash(data, hash)
        )
    }
    fn validate_hash_reader(
        &self,
        reader: &mut dyn std::io::Read,
        hash: &HashDigest,
    ) -> VeilidAPIResult<bool> {
        timed!(
            self,
            CryptoOp::GenerateHash,
            self.inner.validate_hash_reader(reader, hash)
        )
    }

    fn sign(
        &self,
        public_key: &PublicKey,
        secret: &SecretKey,
        data: &[u8],
    ) -> VeilidAPIResult<Signature> {
        timed!(
            self,
            CryptoOp::Sign,
            self.inner.sign(public_key, secret, data)
        )
    }
    fn sign_in_place(
        &self,
        public_key: &PublicKey,
        secret: &SecretKey,
        data: &mut [u8],
        range: Range<usize>,
        sig_idx: usize,
    ) -> VeilidAPIResult<()> {
        timed!(
            self,
            CryptoOp::Sign,
            self.inner
                .sign_in_place(public_key, secret, data, range, sig_idx)
        )
    }
    fn verify(
        &self,
        public_key: &PublicKey,
        data: &[u8],
        signature: &Signature,
    ) -> VeilidAPIResult<bool> {
        timed!(
            self,
            CryptoOp::Verify,
            self.inner.verify(public_key, data, signature)
        )
    }
    fn verify_in_place(
        &self,
        public_key: &PublicKey,
        data: &[u8],
        range: Range<usize>,
        sig_idx: usize,
    ) -> VeilidAPIResult<bool> {
        timed!(
            self,
            CryptoOp::Verify,
            self.inner.verify_in_place(public_key, data, range, sig_idx)
        )
    }

    fn decrypt_in_place_aead(
        &self,
        body: &mut dyn CryptoSystemBuffer,
        nonce: &Nonce,
        shared_secret: &SharedSecret,
        associated_data: Option<&[u8]>,
    ) -> VeilidAPIResult<()> {
        timed!(
            self,
            CryptoOp::DecryptAead,
            self.inner
                .decrypt_in_place_aead(body, nonce, shared_secret, associated_data)
        )
    }
    fn decrypt_aead(
        &self,
        body: &[u8],
        nonce: &Nonce,
        shared_secret: &SharedSecret,
        associated_data: Option<&[u8]>,
    ) -> VeilidAPIResult<Vec<u8>> {
        timed!(
            self,
            CryptoOp::DecryptAead,
            self.inner
                .decrypt_aead(body, nonce, shared_secret, associated_data)
        )
    }
    fn encrypt_in_place_aead(
        &self,
        body: &mut dyn CryptoSystemBuffer,
        nonce: &Nonce,
        shared_secret: &SharedSecret,
        associated_data: Option<&[u8]>,
    ) -> VeilidAPIResult<()> {
        timed!(
            self,
            CryptoOp::EncryptAead,
            self.inner
                .encrypt_in_place_aead(body, nonce, shared_secret, associated_data)
        )
    }
    fn encrypt_aead(
        &self,
        body: &[u8],
        nonce: &Nonce,
        shared_secret: &SharedSecret,
        associated_data: Option<&[u8]>,
    ) -> VeilidAPIResult<Vec<u8>> {
        timed!(
            self,
            CryptoOp::EncryptAead,
            self.inner
                .encrypt_aead(body, nonce, shared_secret, associated_data)
        )
    }

    fn crypt_in_place_no_auth(
        &self,
        body: &mut [u8],
        nonce: &Nonce,
        shared_secret: &SharedSecret,
    ) -> VeilidAPIResult<()> {
        timed!(
            self,
            CryptoOp::CryptNoAuth,
            self.inner
                .crypt_in_place_no_auth(body, nonce, shared_secret)
        )
    }
    fn crypt_b2b_no_auth(
        &self,
        in_buf: &[u8],
        out_buf: &mut [u8],
        nonce: &Nonce,
        shared_secret: &SharedSecret,
    ) -> VeilidAPIResult<()> {
        timed!(
            self,
            CryptoOp::CryptNoAuth,
            self.inner
                .crypt_b2b_no_auth(in_buf, out_buf, nonce, shared_secret)
        )
    }
    fn crypt_no_auth_aligned_8(
        &self,
        body: &[u8],
        nonce: &Nonce,
        shared_secret: &SharedSecret,
    ) -> VeilidAPIResult<Vec<u8>> {
        timed!(
            self,
            CryptoOp::CryptNoAuth,
            self.inner
                .crypt_no_auth_aligned_8(body, nonce, shared_secret)
        )
    }
    fn crypt_no_auth_unaligned(
        &self,
        body: &[u8],
        nonce: &Nonce,
        shared_secret: &SharedSecret,
    ) -> VeilidAPIResult<Vec<u8>> {
        timed!(
            self,
            CryptoOp::CryptNoAuth,
            self.inner
                .crypt_no_auth_unaligned(body, nonce, shared_secret)
        )
    }
}
