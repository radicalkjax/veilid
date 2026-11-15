use super::connection_manager::{ProtectedDropAction, ProtectedDropTracker};
use super::*;

/// The default RPC timeout, below which a dropped connection died young
const FAST_DROP: TimestampDuration = TimestampDuration::new_secs(5);
const YOUNG: TimestampDuration = TimestampDuration::new_ms(1);
const LIVED: TimestampDuration = TimestampDuration::new_secs(60);

/// A peer that accepts a handshake and then drops us must not be able to drive reconnects
/// indefinitely: connections that die young trip the breaker once and it stays tripped.
pub fn test_reconnect_storm_is_bounded() {
    let mut tracker = ProtectedDropTracker::new(FAST_DROP);

    let mut reconnects = 0usize;
    let mut trips = 0usize;
    for _ in 0..1000 {
        match tracker.record_drop(YOUNG) {
            ProtectedDropAction::Reconnect => reconnects += 1,
            ProtectedDropAction::HoldOff { tripped_now } => {
                if tripped_now {
                    trips += 1;
                }
            }
        }
    }

    assert_eq!(
        reconnects, 2,
        "young drops must allow at most PROTECTED_CONNECTION_DROP_COUNT-1 reconnects"
    );
    assert_eq!(
        trips, 1,
        "the hold-off should be reported once, not per drop"
    );
}

/// A connection that stayed up past the threshold and then dropped is not a storm, whether or
/// not it carried anything, and must still reconnect, however often; it also clears the young
/// drops before it.
pub fn test_drops_after_a_long_life_reconnect() {
    let mut tracker = ProtectedDropTracker::new(FAST_DROP);

    for i in 0..50 {
        assert_eq!(
            tracker.record_drop(LIVED),
            ProtectedDropAction::Reconnect,
            "a drop after a long life must reconnect (drop {i})"
        );
        assert_eq!(tracker.fast_drops(), 0);
    }

    assert_eq!(tracker.record_drop(YOUNG), ProtectedDropAction::Reconnect);
    assert_eq!(tracker.record_drop(YOUNG), ProtectedDropAction::Reconnect);
    assert_eq!(
        tracker.record_drop(FAST_DROP),
        ProtectedDropAction::Reconnect
    );
    assert_eq!(tracker.record_drop(YOUNG), ProtectedDropAction::Reconnect);
    assert_eq!(tracker.fast_drops(), 1);
}

/// A tripped breaker holds through more young drops and ends at the first connection that
/// outlived the threshold.
pub fn test_a_connection_that_lives_ends_the_hold_off() {
    let mut tracker = ProtectedDropTracker::new(FAST_DROP);

    assert_eq!(tracker.record_drop(YOUNG), ProtectedDropAction::Reconnect);
    assert_eq!(tracker.record_drop(YOUNG), ProtectedDropAction::Reconnect);
    assert_eq!(
        tracker.record_drop(YOUNG),
        ProtectedDropAction::HoldOff { tripped_now: true }
    );
    assert_eq!(
        tracker.record_drop(YOUNG),
        ProtectedDropAction::HoldOff { tripped_now: false }
    );

    assert_eq!(tracker.record_drop(LIVED), ProtectedDropAction::Reconnect);
    assert_eq!(tracker.record_drop(YOUNG), ProtectedDropAction::Reconnect);
}

pub fn test_all() {
    test_reconnect_storm_is_bounded();
    test_drops_after_a_long_life_reconnect();
    test_a_connection_that_lives_ends_the_hold_off();
}
