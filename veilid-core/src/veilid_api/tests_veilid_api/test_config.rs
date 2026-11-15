use super::*;
use crate::tests::*;

// The `internal()` accessor must ignore configured "footgun" tuning unless the
// `footgun-config` feature is enabled. This is the off-build runtime gate: a config
// carrying internal overrides still deserializes, but defaults are used.
pub fn test_internal_footgun_gate() {
    // Build a config whose internal tuning differs from the default.
    let mut tuned = VeilidConfigInternal::default();
    tuned.network.rpc.timeout_ms = tuned.network.rpc.timeout_ms.wrapping_add(1);
    assert_ne!(tuned, VeilidConfigInternal::default());

    let mut cfg = fake_veilid_config();
    cfg.internal = Some(tuned.clone());

    #[cfg(feature = "footgun-config")]
    {
        // With the feature, the configured tuning is honored.
        assert_eq!(*cfg.internal(), tuned);
    }
    #[cfg(not(feature = "footgun-config"))]
    {
        // Without the feature, footgun tuning is ignored and built-in defaults are used.
        assert_eq!(*cfg.internal(), VeilidConfigInternal::default());
    }
}

// A config saved before `network.bind_retry_ms` existed still deserializes, with the default
pub fn test_bind_retry_ms_defaults_when_absent() {
    let mut json = serde_json::to_value(fake_veilid_config()).expect("serialize");
    let network = json
        .get_mut("network")
        .and_then(|n| n.as_object_mut())
        .expect("network object");
    let removed = network
        .remove("bind_retry_ms")
        .or_else(|| network.remove("bindRetryMs"));
    assert!(removed.is_some(), "the field is serialized");

    let cfg: VeilidConfig = serde_json::from_value(json).expect("deserialize");
    assert_eq!(cfg.network.bind_retry_ms, 10_000);
}
