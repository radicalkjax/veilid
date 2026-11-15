use super::*;
use crate::tests::*;

/// Contacting a watch node about an EXISTING watch should go over the route its
/// ValueChanged notifications already use. Route selection otherwise round-robins
/// among the top tier (`first_available_route_inner`), and
/// `wants_per_node_watch_update` only notices a route deallocated outright.
///
/// The pin must never be load-bearing: once the route is gone the unpinned selection
/// has to come back unchanged, so that losing it drives a reconcile instead of a
/// silent substitution. These cover those fallbacks; the positive case needs a live
/// allocated route and belongs to the watch battery.
pub async fn test_route_pin_falls_back_cleanly() {
    let (update_callback, config) = fixture_veilid_core();
    let api = api_startup(update_callback, config)
        .await
        .expect("startup failed");
    let registry = api.core_context().unwrap().registry();
    let storage_manager = registry.storage_manager();

    let safe = SafetySelection::Safe(SafetySpec {
        preferred_route: None,
        hop_count: 2,
        stability: Stability::Reliable,
        sequencing: Sequencing::EnsureOrdered,
    });

    // No recorded notification route: nothing to pin to
    assert_eq!(
        storage_manager.pin_value_changed_route(&safe, None),
        safe,
        "a watch with no recorded route keeps its selection"
    );

    // Unallocated route key: substituting another route would hide the loss
    let unknown = PublicKey::new(CRYPTO_KIND_VLD0, BarePublicKey::new(&[7u8; 32]));
    let pinned = storage_manager.pin_value_changed_route(&safe, Some(&unknown));
    assert_eq!(
        pinned, safe,
        "an unallocated route falls back to the unpinned selection"
    );

    // An unsafe selection has no route to pin at all
    let unsafe_sel = SafetySelection::Unsafe(Sequencing::PreferOrdered);
    assert_eq!(
        storage_manager.pin_value_changed_route(&unsafe_sel, Some(&unknown)),
        unsafe_sel
    );

    // The rest of the spec survives, so pinning cannot change hop count,
    // stability or sequencing
    let SafetySelection::Safe(spec) = pinned else {
        panic!("should still be a safe selection");
    };
    assert_eq!(spec.hop_count, 2);
    assert_eq!(spec.stability, Stability::Reliable);
    assert_eq!(spec.sequencing, Sequencing::EnsureOrdered);

    api.shutdown().await;
}

pub async fn test_all() {
    test_route_pin_falls_back_cleanly().await;
}
