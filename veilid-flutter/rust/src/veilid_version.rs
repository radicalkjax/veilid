use super::*;

#[derive(Debug, Clone, Serialize)]
pub struct VeilidVersion {
    pub major: u32,
    pub minor: u32,
    pub patch: u32,
}
