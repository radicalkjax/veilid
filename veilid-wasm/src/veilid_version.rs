use super::*;

#[derive(Debug, Clone, Serialize)]
#[cfg_attr(all(target_arch = "wasm32", target_os = "unknown"), derive(Tsify))]
/// Semantic version of the Veilid core library.
pub struct VeilidVersion {
    /// Major version.
    pub major: u32,
    /// Minor version.
    pub minor: u32,
    /// Patch version.
    pub patch: u32,
}
