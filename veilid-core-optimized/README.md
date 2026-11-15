<!-- DO NOT EDIT BELOW - content within cargo-sync-readme blocks is generated -->
<!-- cargo-sync-readme start -->

Internal crate for veilid-core, do not use directly.

Monomorphic entry points for the hot dependencies of veilid-core. Internal to veilid-core:
applications use the `veilid-core` API, never this crate directly.

A Cargo profile override optimizes a dependency's own code, but generic and `#[inline]`
code is compiled in the crate that instantiates it, at that crate's opt-level. Every
function this crate exports is monomorphic and takes no caller-supplied type, so the
cipher, curve, hash, KDF, HPKE and LZ4 code it reaches is instantiated here, where the
workspace profiles keep it at `opt-level = 3` in every build mode. The `size` profile the
WASM builds use keeps veilid-core size-optimized and still runs its crypto at full speed.

Randomness comes in as a fill callback rather than an RNG type, so no `rand_core` version
binds this crate to its callers.

<!-- cargo-sync-readme end -->
