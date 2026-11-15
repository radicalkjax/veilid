//! A destination names its target apart from how the target is reached.

use super::*;
use crate::routing_table::{PrivateRoute, RouteNode};

fn route_destination(tag: u8, safety_selection: SafetySelection) -> Destination {
    let ck = CRYPTO_KIND_VLD0;
    Destination::private_route(
        Arc::new(PrivateRoute::new_stub(
            PublicKey::new(ck, BarePublicKey::new(&[tag; 32])),
            RouteNode::NodeId(NodeId::new(ck, BareNodeId::new(&[tag; 32]))),
        )),
        safety_selection,
    )
}

/// Questions over one reply route can ask to be answered differently: still one target
pub fn test_same_target_ignores_how_it_is_reached() {
    let ordered = route_destination(1, SafetySelection::Unsafe(Sequencing::PreferOrdered));
    let ensured = route_destination(1, SafetySelection::Unsafe(Sequencing::EnsureOrdered));
    let other = route_destination(2, SafetySelection::Unsafe(Sequencing::PreferOrdered));
    assert!(ordered != ensured);
    assert!(ordered.same_target(&ensured));
    assert!(!ordered.same_target(&other));
}
