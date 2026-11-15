//! Debug fault injection stub, compiled when the `debug-api` feature is off. Nothing can set chaos
//! without `debug chaos`, so every packet passes and connections pass through unwrapped.

use super::*;

/// What chaos does with one packet or stream frame; always `Pass` here
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[must_use]
#[cfg_attr(
    all(target_arch = "wasm32", target_os = "unknown"),
    expect(dead_code, reason = "web has no UDP, the only caller")
)]
pub(crate) enum ChaosVerdict {
    /// Send or deliver it now
    Pass,
    /// Send or deliver it after this many milliseconds, once the link has carried it
    #[cfg_attr(
        not(all(target_arch = "wasm32", target_os = "unknown")),
        expect(dead_code, reason = "only debug chaos holds packets")
    )]
    Hold(u32),
    /// Lose it
    #[cfg_attr(
        not(all(target_arch = "wasm32", target_os = "unknown")),
        expect(dead_code, reason = "only debug chaos loses packets")
    )]
    Drop,
    /// Lose it as too big for the path; a stream stalls for good
    #[cfg_attr(
        not(all(target_arch = "wasm32", target_os = "unknown")),
        expect(dead_code, reason = "only debug chaos loses packets")
    )]
    Oversize,
}

/// Debug fault injection; inert without the `debug-api` feature
#[derive(Clone, Debug, Default)]
pub(crate) struct Chaos;

impl Chaos {
    pub fn new() -> Self {
        Self
    }

    #[cfg_attr(
        all(target_arch = "wasm32", target_os = "unknown"),
        expect(dead_code, reason = "web has no UDP, the only caller")
    )]
    pub fn send_verdict(&self, _ip: IpAddr, _size: usize) -> ChaosVerdict {
        ChaosVerdict::Pass
    }

    #[cfg_attr(
        all(target_arch = "wasm32", target_os = "unknown"),
        expect(dead_code, reason = "web has no UDP, the only caller")
    )]
    pub fn recv_verdict(&self, _ip: IpAddr, _size: usize) -> ChaosVerdict {
        ChaosVerdict::Pass
    }

    pub fn drop_value_changed(&self) -> bool {
        false
    }

    pub fn wrap_connection(
        &self,
        inner: ProtocolNetworkConnection,
        _segment_size: Option<u32>,
    ) -> ProtocolNetworkConnection {
        inner
    }
}
