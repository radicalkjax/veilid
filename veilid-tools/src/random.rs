use rand::prelude::*;

/// Cryptographically secure RNG that draws from the thread-local random source.
///
/// Implements [`RngCore`] and [`CryptoRng`] so it can be passed wherever a `rand` generator is expected.
#[derive(Clone, Copy, Debug, Default)]
pub struct VeilidRng;

impl CryptoRng for VeilidRng {}

impl RngCore for VeilidRng {
    fn next_u32(&mut self) -> u32 {
        get_random_u32()
    }

    fn next_u64(&mut self) -> u64 {
        get_random_u64()
    }

    fn fill_bytes(&mut self, dest: &mut [u8]) {
        random_bytes(dest);
    }

    fn try_fill_bytes(&mut self, dest: &mut [u8]) -> Result<(), rand::Error> {
        random_bytes(dest);
        Ok(())
    }
}

// rand_core 0.10 trait generation, for the RustCrypto/dalek crates
impl rand_core::TryRng for VeilidRng {
    type Error = core::convert::Infallible;

    fn try_next_u32(&mut self) -> Result<u32, Self::Error> {
        Ok(get_random_u32())
    }

    fn try_next_u64(&mut self) -> Result<u64, Self::Error> {
        Ok(get_random_u64())
    }

    fn try_fill_bytes(&mut self, dest: &mut [u8]) -> Result<(), Self::Error> {
        random_bytes(dest);
        Ok(())
    }
}

impl rand_core::TryCryptoRng for VeilidRng {}

/// Fill `dest` with cryptographically secure random bytes.
pub fn random_bytes(dest: &mut [u8]) {
    let mut rng = rand::thread_rng();
    rng.fill_bytes(dest);
}

/// Return a cryptographically secure random `u32`.
#[must_use]
pub fn get_random_u32() -> u32 {
    let mut rng = rand::thread_rng();
    rng.next_u32()
}

/// Return a cryptographically secure random `u64`.
#[must_use]
pub fn get_random_u64() -> u64 {
    let mut rng = rand::thread_rng();
    rng.next_u64()
}
