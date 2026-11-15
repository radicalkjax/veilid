/// Length of a BLAKE3 digest in bytes
pub const BLAKE3_HASH_LENGTH: usize = 32;

/// BLAKE3 digest of `data`
#[must_use]
pub fn blake3_hash(data: &[u8]) -> [u8; BLAKE3_HASH_LENGTH] {
    *blake3::hash(data).as_bytes()
}

/// The first `out.len()` bytes of BLAKE3's extendable output over `data`
pub fn blake3_hash_xof(data: &[u8], out: &mut [u8]) {
    let mut hasher = blake3::Hasher::new();
    hasher.update(data);
    hasher.finalize_xof().fill(out);
}

/// Incremental BLAKE3. Also a [`std::io::Write`] sink, so a reader hashes with
/// [`std::io::copy`].
#[derive(Debug, Clone, Default)]
pub struct Blake3Hasher(blake3::Hasher);

impl Blake3Hasher {
    /// An empty hasher
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Absorb `data`
    pub fn update(&mut self, data: &[u8]) {
        self.0.update(data);
    }

    /// Digest of everything absorbed so far
    #[must_use]
    pub fn finalize(&self) -> [u8; BLAKE3_HASH_LENGTH] {
        *self.0.finalize().as_bytes()
    }
}

impl std::io::Write for Blake3Hasher {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        self.0.update(buf);
        Ok(buf.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}
