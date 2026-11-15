//! Envelope timestamp checks: what the per-sender offset accepts, and what a frozen process
//! does to it.
//!
//! Scenarios are in multiples of the configured limits, which the fixture sets differently
//! with and without `footgun-config`.

use super::*;
use crate::tests::*;

const T0: u64 = 1_700_000_000_000_000;
/// Network delay of the steady traffic
const DELAY: u64 = 1000;

struct Limits {
    behind: u64,
    ahead: u64,
}

fn limits(registry: &VeilidComponentRegistry) -> Limits {
    let config = registry.config();
    let rpc = &config.internal().network.rpc;
    Limits {
        behind: ms_to_us(rpc.max_timestamp_behind_ms.expect("behind limit")),
        ahead: ms_to_us(rpc.max_timestamp_ahead_ms.expect("ahead limit")),
    }
}

fn node(n: u8) -> NodeId {
    NodeId::new(CRYPTO_KIND_VLD0, BareNodeId::new(&[n; 32]))
}

fn ts(us: u64) -> Timestamp {
    Timestamp::new(us)
}

fn check(af: &AddressFilter, n: u8, local: u64, remote: u64) -> Result<(), TimestampError> {
    af.check_envelope_timestamp(node(n), ts(local), ts(remote))
}

/// Accepted envelopes every `step` from `from` up to `to`, each sent [DELAY] before arrival
fn steady(af: &AddressFilter, n: u8, from: u64, to: u64, step: u64) {
    let mut local = from;
    while local <= to {
        assert_eq!(
            check(af, n, local, local - DELAY),
            Ok(()),
            "steady at {}",
            local
        );
        local += step;
    }
}

/// A sender whose clock is far off is accepted from its first envelope on
fn test_first_contact_seeds_a_skewed_sender(registry: &VeilidComponentRegistry) {
    let l = limits(registry);
    let af = AddressFilter::new(registry.clone());
    let skew = 3 * l.ahead.max(l.behind);
    let step = l.behind / 4;
    for i in 0..8 {
        let local = T0 + i * step;
        assert_eq!(check(&af, 1, local, local + skew), Ok(()), "envelope {}", i);
    }
}

/// A resent envelope is a duplicate whether or not the original passed
fn test_duplicates_are_caught_accepted_or_not(registry: &VeilidComponentRegistry) {
    let l = limits(registry);
    let af = AddressFilter::new(registry.clone());
    steady(&af, 1, T0, T0 + l.behind, l.behind / 4);

    let local = T0 + l.behind + 1000;
    assert_eq!(check(&af, 1, local, local - DELAY), Ok(()));
    assert!(matches!(
        check(&af, 1, local + 10, local - DELAY),
        Err(TimestampError::Duplicate { .. })
    ));

    let stale = local - 3 * l.behind;
    assert!(matches!(
        check(&af, 1, local + 20, stale),
        Err(TimestampError::TooFarBehind { .. })
    ));
    assert!(matches!(
        check(&af, 1, local + 30, stale),
        Err(TimestampError::Duplicate { .. })
    ));
}

/// Replays and future-dated envelopes are rejected
fn test_replay_and_future_are_rejected(registry: &VeilidComponentRegistry) {
    let l = limits(registry);
    let af = AddressFilter::new(registry.clone());
    let end = T0 + 2 * l.behind;
    steady(&af, 1, T0, end, l.behind / 4);

    assert!(matches!(
        check(&af, 1, end + 1, end - 3 * l.behind),
        Err(TimestampError::TooFarBehind { .. })
    ));
    assert!(matches!(
        check(&af, 1, end + 2, end + 3 * l.ahead),
        Err(TimestampError::TooFarAhead { .. })
    ));
}

/// A rejected envelope does not move the offset, so the next genuine one still passes
fn test_rejected_envelopes_leave_the_offset_alone(registry: &VeilidComponentRegistry) {
    let l = limits(registry);
    let af = AddressFilter::new(registry.clone());
    let end = T0 + 2 * l.behind;
    steady(&af, 1, T0, end, l.behind / 4);

    assert!(matches!(
        check(&af, 1, end + 1, end - 30 * l.behind),
        Err(TimestampError::TooFarBehind { .. })
    ));
    steady(&af, 1, end + 2 * DELAY, end + l.behind, l.behind / 8);
}

/// Freeze `freeze` long after steady traffic: what the sender queued meanwhile, as
/// (claimed, received), all arriving together at the thaw
fn queued_during(start: u64, freeze: u64, count: u64) -> Vec<(u64, u64)> {
    let thaw = start + freeze;
    (0..count)
        .map(|k| (start + k * (freeze / count), thaw + k))
        .collect()
}

/// Envelopes queued through a freeze arrive before the stall is known. They must not drag
/// the offset: fresh envelopes right after them still pass.
fn test_thaw_before_the_stall_is_known(registry: &VeilidComponentRegistry) {
    let l = limits(registry);
    let af = AddressFilter::new(registry.clone());
    let start = T0 + 2 * l.behind;
    steady(&af, 1, T0, start, l.behind / 4);

    let freeze = 22 * l.behind;
    for (claimed, received) in queued_during(start, freeze, 15) {
        let res = check(&af, 1, received, claimed);
        if received - claimed > l.behind {
            assert!(
                matches!(res, Err(TimestampError::TooFarBehind { .. })),
                "stale envelope sent {}us before arrival: {:?}",
                received - claimed,
                res
            );
        }
    }
    let thaw = start + freeze;
    steady(&af, 1, thaw + 2 * DELAY, thaw + 2 * l.behind, l.behind / 8);
}

/// Once the stall is known, everything sent during it is dropped, even what would pass the
/// behind limit
fn test_sent_while_stalled_is_dropped(registry: &VeilidComponentRegistry) {
    let l = limits(registry);
    let af = AddressFilter::new(registry.clone());
    let start = T0 + 2 * l.behind;
    steady(&af, 1, T0, start, l.behind / 4);

    let freeze = 22 * l.behind;
    let thaw = start + freeze;
    af.record_stall(ts(start), ts(thaw));
    for (claimed, received) in queued_during(start, freeze, 15) {
        assert!(matches!(
            check(&af, 1, received, claimed),
            Err(TimestampError::SentWhileStalled { .. })
        ));
    }
    let inside_limit = thaw - l.behind / 2;
    assert!(matches!(
        check(&af, 1, thaw + 50, inside_limit),
        Err(TimestampError::SentWhileStalled { .. })
    ));
    steady(&af, 1, thaw + 2 * DELAY, thaw + 2 * l.behind, l.behind / 8);
}

/// A sender first heard from during the stall is not seeded from it. Proven by its next,
/// fresh envelope being first contact: accepted at any skew.
fn test_first_contact_sent_while_stalled_is_dropped(registry: &VeilidComponentRegistry) {
    let l = limits(registry);
    let af = AddressFilter::new(registry.clone());
    let start = T0;
    let thaw = start + 22 * l.behind;
    af.record_stall(ts(start), ts(thaw));

    assert!(matches!(
        check(&af, 2, thaw + 10, start + l.behind),
        Err(TimestampError::SentWhileStalled { .. })
    ));
    let skew = 5 * l.ahead;
    assert_eq!(check(&af, 2, thaw + 20, thaw + 20 + skew), Ok(()));
    assert_eq!(check(&af, 2, thaw + 40, thaw + 40 + skew), Ok(()));
}

/// A sender seeded from a queued envelope before the stall was known is forgotten when it
/// is, so its next fresh envelope seeds it again
fn test_stale_seed_is_forgotten_when_the_stall_is_known(registry: &VeilidComponentRegistry) {
    let l = limits(registry);
    let af = AddressFilter::new(registry.clone());
    let start = T0;
    let freeze = 22 * l.behind;
    let thaw = start + freeze;

    assert_eq!(check(&af, 3, thaw, start + freeze / 2), Ok(()));
    af.record_stall(ts(start), ts(thaw));
    steady(&af, 3, thaw + 2 * DELAY, thaw + 2 * l.behind, l.behind / 8);
}

/// A stall shorter than the behind limit buffers nothing the limit would drop, and some of
/// it is answers still being waited for, so it changes nothing
fn test_short_stall_is_ignored(registry: &VeilidComponentRegistry) {
    let l = limits(registry);
    let af = AddressFilter::new(registry.clone());
    let start = T0 + 2 * l.behind;
    steady(&af, 1, T0, start, l.behind / 4);

    let end = start + l.behind / 2;
    af.record_stall(ts(start), ts(end));
    assert_eq!(check(&af, 1, end + 10, start + l.behind / 4), Ok(()));
}

/// The stall stops dropping once it is as old as the behind limit, which covers the rest
fn test_stall_window_expires(registry: &VeilidComponentRegistry) {
    let l = limits(registry);
    let af = AddressFilter::new(registry.clone());
    let start = T0 + 2 * l.behind;
    steady(&af, 1, T0, start, l.behind / 4);
    let thaw = start + 22 * l.behind;
    af.record_stall(ts(start), ts(thaw));

    assert!(matches!(
        check(&af, 1, thaw + l.behind / 2, thaw - l.behind / 4),
        Err(TimestampError::SentWhileStalled { .. })
    ));
    assert!(matches!(
        check(&af, 1, thaw + l.behind + 1000, thaw - 1000),
        Err(TimestampError::TooFarBehind { .. })
    ));
}

/// A sender that steps its clock is rejected for one window, then its offset starts over
fn test_clock_step_heals_after_a_window(registry: &VeilidComponentRegistry) {
    let l = limits(registry);
    let af = AddressFilter::new(registry.clone());
    let start = T0 + 2 * l.behind;
    steady(&af, 1, T0, start, l.behind / 4);

    let step = 5 * l.ahead;
    let every = l.behind / 8;
    let first = start + every;
    let mut local = first;
    let healed_at = loop {
        match check(&af, 1, local, local + step) {
            Ok(()) => break local,
            Err(TimestampError::TooFarAhead { .. }) => {}
            Err(e) => panic!("unexpected {:?}", e),
        }
        local += every;
        assert!(local < first + 3 * l.behind, "never healed");
    };
    assert!(
        healed_at - first >= l.behind,
        "healed before a window passed"
    );
    assert!(healed_at - first <= l.behind + every, "healed late");
    for i in 1..8 {
        let local = healed_at + i * every;
        assert_eq!(check(&af, 1, local, local + step), Ok(()));
    }
}

/// Known limit: a new sender whose clock is behind ours by `lag` reads as sent during a
/// stall that just ended, until `lag` past the thaw
fn test_new_sender_behind_us_waits_out_the_stall(registry: &VeilidComponentRegistry) {
    let l = limits(registry);
    let af = AddressFilter::new(registry.clone());
    let start = T0;
    let thaw = start + 22 * l.behind;
    af.record_stall(ts(start), ts(thaw));

    let lag = l.behind / 2;
    let early = thaw + lag / 2;
    assert!(matches!(
        check(&af, 4, early, early - lag),
        Err(TimestampError::SentWhileStalled { .. })
    ));
    let later = thaw + lag + 1000;
    assert_eq!(check(&af, 4, later, later - lag), Ok(()));
}

pub async fn test_all() {
    let registry = mock_registry::init("envelope_timestamps").await;

    test_first_contact_seeds_a_skewed_sender(&registry);
    test_duplicates_are_caught_accepted_or_not(&registry);
    test_replay_and_future_are_rejected(&registry);
    test_rejected_envelopes_leave_the_offset_alone(&registry);
    test_thaw_before_the_stall_is_known(&registry);
    test_sent_while_stalled_is_dropped(&registry);
    test_first_contact_sent_while_stalled_is_dropped(&registry);
    test_stale_seed_is_forgotten_when_the_stall_is_known(&registry);
    test_short_stall_is_ignored(&registry);
    test_stall_window_expires(&registry);
    test_clock_step_heals_after_a_window(&registry);
    test_new_sender_behind_us_waits_out_the_stall(&registry);

    mock_registry::terminate(registry).await;
}
