use crate::routing_table::*;

/// Attaining readiness takes two live ordered routes of each default length
pub fn test_safety_routes_ready_attain_needs_the_pool() {
    info!("--- test_safety_routes_ready_attain_needs_the_pool ---");
    assert!(!safety_routes_ready_for(false, 0, 0));
    assert!(!safety_routes_ready_for(false, 1, 2));
    assert!(!safety_routes_ready_for(false, 2, 1));
    assert!(!safety_routes_ready_for(false, 5, 0));
    assert!(safety_routes_ready_for(false, 2, 2));
    assert!(safety_routes_ready_for(false, 3, 2));
}

/// Once attained, any one live ordered route keeps readiness; zero loses it
pub fn test_safety_routes_ready_keeps_on_one_route() {
    info!("--- test_safety_routes_ready_keeps_on_one_route ---");
    assert!(safety_routes_ready_for(true, 1, 0));
    assert!(safety_routes_ready_for(true, 0, 1));
    assert!(safety_routes_ready_for(true, 1, 1));
    assert!(!safety_routes_ready_for(true, 0, 0));
}
