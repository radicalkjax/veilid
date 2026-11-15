//! The loss estimate: whose loss counts, when the window is read, and what holds the decision.

use super::*;

fn ts(secs: u64) -> Timestamp {
    Timestamp::new(secs * 1_000_000)
}

fn timings() -> OnlineDetectorTimings {
    OnlineDetectorTimings::new(TimestampDuration::new_ms(
        VeilidConfigInternalRPC::default().timeout_ms.into(),
    ))
}

const UDP: SequenceOrdering = SequenceOrdering::Unordered;

fn node(n: u8) -> NodeId {
    let mut b = [0u8; 32];
    b[0] = n;
    NodeId::new(CRYPTO_KIND_VLD0, BareNodeId::new(&b))
}

/// Peers 0..count each answer a question sent at `at`, which vouches for them
fn vouch(est: &mut LossEstimator, count: u8, at: u64) {
    for n in 0..count {
        est.record_outcome(&node(n), UDP, ts(at), true, true, ts(at));
    }
}

/// One question to each peer every second over [from, to), answered as `answered` says
fn ask(est: &mut LossEstimator, count: u8, from: u64, to: u64, answered: impl Fn(u8, u64) -> bool) {
    for sec in from..to {
        for n in 0..count {
            let ok = answered(n, sec);
            let outcome_at = if ok { sec } else { sec + 5 };
            est.record_outcome(&node(n), UDP, ts(sec), ok, true, ts(outcome_at));
        }
    }
}

pub fn test_timings() {
    let t = timings();
    assert_eq!(t.outcome_lag, TimestampDuration::new_secs(5));
    assert_eq!(t.loss_attribution_window, TimestampDuration::new_secs(30));
}

pub fn test_unvouched_peers_do_not_count() {
    // A peer that never answered cannot tell our loss from its own
    let mut est = LossEstimator::default();
    ask(&mut est, 20, 101, 111, |_, _| false);
    let e = est.evaluate(&timings(), true, ts(116));
    assert_eq!(e.samples, 0);
    assert!(!e.degraded);
}

pub fn test_eligibility_is_per_ordering() {
    // A peer that answers over datagrams but refuses streams has not vouched for its stream path
    let mut est = LossEstimator::default();
    vouch(&mut est, 20, 100);
    for sec in 101..111 {
        for n in 0..20 {
            est.record_outcome(
                &node(n),
                SequenceOrdering::Ordered,
                ts(sec),
                false,
                true,
                ts(sec + 5),
            );
        }
    }
    let e = est.evaluate(&timings(), true, ts(116));
    assert_eq!(e.samples, 0);
    assert!(!e.degraded);
    // but it is still a peer that answered recently
    assert!(est.answered_since(&node(0), ts(100)));
}

pub fn test_too_few_paths_cannot_attribute() {
    // Three silent peers cannot say it is our link
    let mut est = LossEstimator::default();
    vouch(&mut est, 3, 100);
    ask(&mut est, 3, 101, 111, |_, _| false);
    let e = est.evaluate(&timings(), true, ts(116));
    assert_eq!(e.samples, 30);
    assert!(!e.degraded);
}

pub fn test_uniform_loss_is_ours() {
    let mut est = LossEstimator::default();
    vouch(&mut est, 20, 100);
    ask(&mut est, 20, 101, 111, |n, sec| {
        (u64::from(n) + sec).is_multiple_of(2)
    });
    let e = est.evaluate(&timings(), true, ts(116));
    assert_eq!(e.samples, 200);
    assert!((e.survival - 0.5).abs() < 1e-9);
    assert!(e.degraded);
}

pub fn test_a_few_dead_peers_are_not_ours() {
    // Six of twenty never answer: the pooled fraction is low, the median peer is fine
    let mut est = LossEstimator::default();
    vouch(&mut est, 20, 100);
    ask(&mut est, 20, 101, 111, |n, _| n >= 6);
    let e = est.evaluate(&timings(), true, ts(116));
    assert!(e.survival < 0.81);
    assert!(!e.degraded);
}

pub fn test_window_waits_for_outcomes() {
    // Questions sent within the last RPC timeout may still be answered, so the window ends before them
    let mut est = LossEstimator::default();
    vouch(&mut est, 20, 100);
    ask(&mut est, 20, 112, 116, |_, _| false);
    let e = est.evaluate(&timings(), true, ts(116));
    assert_eq!(e.samples, 0);
    assert!(!e.degraded);
}

pub fn test_thin_traffic_holds_the_decision() {
    let mut est = LossEstimator::default();
    vouch(&mut est, 20, 100);
    ask(&mut est, 20, 101, 111, |n, sec| {
        (u64::from(n) + sec).is_multiple_of(2)
    });
    assert!(est.evaluate(&timings(), true, ts(116)).degraded);

    // Two answers cannot bound survival above the threshold
    ask(&mut est, 2, 121, 122, |_, _| true);
    let e = est.evaluate(&timings(), true, ts(127));
    assert_eq!(e.samples, 2);
    assert!(e.degraded);

    // Forty can
    ask(&mut est, 20, 131, 133, |_, _| true);
    let e = est.evaluate(&timings(), true, ts(138));
    assert_eq!(e.samples, 40);
    assert!(!e.degraded);
}

pub fn test_baseline_learns_only_while_healthy() {
    let mut est = LossEstimator::default();
    vouch(&mut est, 20, 100);
    // Every peer misses one question in ten: a healthy network's residue, under the threshold
    let residue = |n: u8, sec: u64| !(u64::from(n) + sec).is_multiple_of(10);
    ask(&mut est, 20, 101, 400, residue);
    let mut e = LossEstimate::default();
    for sec in 116..405 {
        e = est.evaluate(&timings(), true, ts(sec));
    }
    assert!(!e.degraded);
    assert!(e.survival > 0.98, "survival {}", e.survival);

    // A window evaluated without learning leaves the baseline alone
    let mut est = LossEstimator::default();
    vouch(&mut est, 20, 100);
    ask(&mut est, 20, 101, 400, residue);
    for sec in 116..405 {
        e = est.evaluate(&timings(), false, ts(sec));
    }
    assert!((e.survival - 0.9).abs() < 1e-9);
}

pub fn test_clear_window_keeps_the_decision() {
    let mut est = LossEstimator::default();
    vouch(&mut est, 20, 100);
    ask(&mut est, 20, 101, 111, |_, _| false);
    assert!(est.evaluate(&timings(), true, ts(116)).degraded);
    est.clear_window();
    let e = est.evaluate(&timings(), true, ts(117));
    assert_eq!(e.samples, 0);
    assert!(e.degraded, "an empty window holds the decision");
    // Peers vouched for before the clear still count, and their answers move it
    assert!(est.answered_since(&node(3), ts(100)));
    ask(&mut est, 20, 121, 131, |_, _| true);
    assert!(!est.evaluate(&timings(), true, ts(136)).degraded);
}

pub fn test_loss_holds_degraded() {
    // The estimate alone holds Degraded, and the exit does not blame local stalls
    let mut pd = OnlineDetectionPerDomain::default();
    vouch(&mut pd.loss_estimator, 20, 100);
    ask(&mut pd.loss_estimator, 20, 101, 111, |_, _| false);
    let entry = OnlineDetector::commit_effective(&timings(), &mut pd, None, ts(116))
        .expect("loss degrades");
    assert_eq!(entry.new_state, OnlineState::Degraded);
    assert!(entry.loss.degraded);

    ask(&mut pd.loss_estimator, 20, 121, 131, |_, _| true);
    let exit = OnlineDetector::commit_effective(&timings(), &mut pd, None, ts(136))
        .expect("answers recover");
    assert_eq!(exit.new_state, OnlineState::Online);
    assert!(!exit.local_stall_only);
}

pub fn test_failure_backoff_curve() {
    let secs = TimestampDuration::new_secs;
    let curve = |survival| {
        (0..6)
            .map(|attempt| failure_backoff(secs(5), secs(60), attempt, survival))
            .collect::<Vec<_>>()
    };
    let some = |v: &[u32]| v.iter().map(|s| Some(secs(*s))).collect::<Vec<_>>();
    assert_eq!(curve(Some(1.0)), some(&[5, 10, 20, 40, 60, 60]));
    assert_eq!(curve(Some(0.25)), some(&[20, 40, 60, 60, 60, 60]));
    assert_eq!(curve(Some(0.0)), some(&[60, 60, 60, 60, 60, 60]));
    assert_eq!(curve(None), vec![None; 6]);
}

pub fn test_degraded_time_carries_its_blame_weight() {
    // Loss at half survival through a degraded period: half of it stays blamed
    let mut pd = OnlineDetectionPerDomain::default();
    vouch(&mut pd.loss_estimator, 20, 100);
    ask(&mut pd.loss_estimator, 20, 101, 111, |n, sec| {
        (u64::from(n) + sec).is_multiple_of(2)
    });
    let entry = OnlineDetector::commit_effective(&timings(), &mut pd, None, ts(116))
        .expect("loss degrades");
    assert!((entry.loss.survival - 0.5).abs() < 1e-9);
    ask(&mut pd.loss_estimator, 20, 121, 131, |_, _| true);
    let exit = OnlineDetector::commit_effective(&timings(), &mut pd, None, ts(136))
        .expect("answers recover");
    assert_eq!(exit.new_state, OnlineState::Online);
    let (start, end, weight) = *pd.recent_degraded_intervals.back().expect("recorded");
    assert_eq!((start, end), (ts(116), ts(136)));
    assert!((weight - 0.5).abs() < 1e-9, "weight {}", weight);

    // A stall holds the domain degraded with nobody to blame
    let mut pd = OnlineDetectionPerDomain::default();
    let stall = Some(ts(10));
    assert!(OnlineDetector::commit_effective(&timings(), &mut pd, stall, ts(10)).is_some());
    pd.last_success_ts = Some(ts(12));
    assert!(OnlineDetector::commit_effective(&timings(), &mut pd, stall, ts(16)).is_some());
    let (_, _, weight) = *pd.recent_degraded_intervals.back().expect("recorded");
    assert_eq!(weight, 0.0);
}

/// Survival falling partway through a degraded period never takes blamed time back, so a dead peer
/// past its dead span cannot come back to life
pub fn test_open_degraded_credit_only_grows() {
    let mut pd = OnlineDetectionPerDomain::default();
    vouch(&mut pd.loss_estimator, 20, 100);
    ask(&mut pd.loss_estimator, 20, 101, 111, |n, sec| {
        (u64::from(n) + sec).is_multiple_of(2)
    });
    OnlineDetector::commit_effective(&timings(), &mut pd, None, ts(116)).expect("loss degrades");
    let from = ts(120);
    let blamed = |od: &OverlapData, at: Timestamp| {
        at.duration_since(from).as_u64() - od.overlap(from, at, true).as_u64()
    };
    let before = OverlapData::of(&pd);
    assert_eq!(blamed(&before, ts(136)), 8_000_000, "half of 16 s");

    // Survival falls to a quarter
    ask(&mut pd.loss_estimator, 20, 121, 131, |n, sec| {
        (u64::from(n) + sec).is_multiple_of(4)
    });
    assert!(OnlineDetector::commit_effective(&timings(), &mut pd, None, ts(136)).is_none());
    let estimate = pd.loss_estimator.estimate();
    assert!(estimate.degraded && (estimate.survival - 0.25).abs() < 1e-9);
    let after = OverlapData::of(&pd);
    assert_eq!(blamed(&after, ts(136)), blamed(&before, ts(136)));
    let mut last = 0;
    for sec in 136..200 {
        let now = blamed(&after, ts(sec));
        assert!(
            now >= last,
            "blamed time fell at {} s: {} < {}",
            sec,
            now,
            last
        );
        last = now;
    }
}

pub fn test_all() {
    test_degraded_time_carries_its_blame_weight();
    test_open_degraded_credit_only_grows();
    test_failure_backoff_curve();
    test_timings();
    test_unvouched_peers_do_not_count();
    test_eligibility_is_per_ordering();
    test_too_few_paths_cannot_attribute();
    test_uniform_loss_is_ours();
    test_a_few_dead_peers_are_not_ours();
    test_window_waits_for_outcomes();
    test_thin_traffic_holds_the_decision();
    test_baseline_learns_only_while_healthy();
    test_clear_window_keeps_the_decision();
    test_loss_holds_degraded();
}
