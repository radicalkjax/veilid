use crate::FillBytes;
use core::convert::Infallible;

/// The caller's fill callback, seen by the dalek and RustCrypto crates as a CSPRNG
pub(crate) struct FillRng<'a>(pub(crate) FillBytes<'a>);

impl rand_core::TryRng for FillRng<'_> {
    type Error = Infallible;

    fn try_next_u32(&mut self) -> Result<u32, Self::Error> {
        let mut b = [0u8; 4];
        (self.0)(&mut b);
        Ok(u32::from_le_bytes(b))
    }

    fn try_next_u64(&mut self) -> Result<u64, Self::Error> {
        let mut b = [0u8; 8];
        (self.0)(&mut b);
        Ok(u64::from_le_bytes(b))
    }

    fn try_fill_bytes(&mut self, dst: &mut [u8]) -> Result<(), Self::Error> {
        (self.0)(dst);
        Ok(())
    }
}

impl rand_core::TryCryptoRng for FillRng<'_> {}
