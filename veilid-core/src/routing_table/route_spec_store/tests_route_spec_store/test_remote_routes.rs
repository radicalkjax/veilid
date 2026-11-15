//! A remote route the app imported is tested as one unit: until it is proved, then once per idle
//! time, and again while a failure stands.

use super::*;

pub fn test_remote_route_testing_cadence() {
    let t0 = Timestamp::new(1_000_000_000);
    let idle = TimestampDuration::new_ms(ROUTE_MIN_IDLE_TIME_MS as u64);
    let mut stats = RouteStats::new(t0);
    assert!(stats.remote_needs_testing(t0), "never proved");

    stats.record_remote_test_pass(SequenceOrdering::Ordered, t0);
    assert!(
        !stats.remote_needs_testing(t0),
        "an ordering the route was never seen on does not make it stale"
    );
    assert!(!stats.remote_needs_testing(t0.later(idle)));
    assert!(stats.remote_needs_testing(t0.later(idle).later(TimestampDuration::new(1))));

    // A failure on another ordering asks for a test, and a pass over any ordering clears it
    stats.record_send_failed(SequenceOrdering::Unordered);
    assert!(stats.remote_needs_testing(t0));
    stats.record_remote_test_pass(SequenceOrdering::Ordered, t0);
    assert!(!stats.remote_needs_testing(t0));
}
