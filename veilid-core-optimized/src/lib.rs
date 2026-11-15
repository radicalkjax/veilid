//! Internal crate for veilid-core, do not use directly.
//!
//! Monomorphic entry points for the hot dependencies of veilid-core. Internal to veilid-core:
//! applications use the `veilid-core` API, never this crate directly.
//!
//! A Cargo profile override optimizes a dependency's own code, but generic and `#[inline]`
//! code is compiled in the crate that instantiates it, at that crate's opt-level. Every
//! function this crate exports is monomorphic and takes no caller-supplied type, so the
//! cipher, curve, hash, KDF, HPKE and LZ4 code it reaches is instantiated here, where the
//! workspace profiles keep it at `opt-level = 3` in every build mode. The `size` profile the
//! WASM builds use keeps veilid-core size-optimized and still runs its crypto at full speed.
//!
//! Randomness comes in as a fill callback rather than an RNG type, so no `rand_core` version
//! binds this crate to its callers.

#![deny(warnings)]
#![warn(missing_docs)]

mod buffer;
mod compress;
mod error;

#[cfg(any(feature = "enable-crypto-vld0", feature = "enable-crypto-none"))]
mod hash;
#[cfg(any(feature = "enable-crypto-vld0", feature = "enable-crypto-none"))]
mod kdf;
#[cfg(any(feature = "enable-crypto-vld0", feature = "enable-crypto-none"))]
mod rng;
#[cfg(any(feature = "enable-crypto-vld0", feature = "enable-crypto-none"))]
mod seal;

#[cfg(feature = "enable-crypto-vld0")]
mod cipher;
#[cfg(feature = "enable-crypto-vld0")]
mod dh;
#[cfg(feature = "enable-crypto-vld0")]
mod sign;

#[cfg(feature = "enable-crypto-none")]
mod none_kem;

pub use buffer::*;
pub use compress::*;
pub use error::*;

#[cfg(any(feature = "enable-crypto-vld0", feature = "enable-crypto-none"))]
pub use hash::*;
#[cfg(any(feature = "enable-crypto-vld0", feature = "enable-crypto-none"))]
pub use kdf::*;
#[cfg(any(feature = "enable-crypto-vld0", feature = "enable-crypto-none"))]
pub use seal::*;

#[cfg(feature = "enable-crypto-vld0")]
pub use cipher::*;
#[cfg(feature = "enable-crypto-vld0")]
pub use dh::*;
#[cfg(feature = "enable-crypto-vld0")]
pub use sign::*;

/// The caller's source of cryptographically secure random bytes: fills the whole slice
pub type FillBytes<'a> = &'a mut dyn FnMut(&mut [u8]);
