use crate::*;

const COOLDOWN_US: u64 = 5_000_000;

fn params() -> AimdParams {
    AimdParams {
        initial: 1000,
        floor: 100,
        md_scale_num: 1,
        md_scale_den: 2,
        ai_step: 50,
        congested_num: 1,
        congested_den: 2,
        headroom_factor: 2,
        md_cooldown_us: COOLDOWN_US,
    }
}

fn congested_window(c: &AimdController) {
    c.record_outcome(true);
    c.record_outcome(true);
}

pub fn test_md_and_floor() {
    info!("test_md_and_floor");

    let c = AimdController::new(params());
    assert_eq!(c.limit(), 1000);

    congested_window(&c);
    assert_eq!(c.step(1_000_000, 0).decision, AimdDecision::Lower(500));
    assert_eq!(c.limit(), 500);

    // Past recovery each time: 500 -> 250 -> 125 -> floor
    let mut t = 1_000_000;
    for expected in [250, 125, 100] {
        t += COOLDOWN_US + 1_000_000;
        congested_window(&c);
        assert_eq!(c.step(t, 0).decision, AimdDecision::Lower(expected));
    }

    // At the floor a congested window still reports Lower
    t += COOLDOWN_US + 1_000_000;
    congested_window(&c);
    assert_eq!(c.step(t, 0).decision, AimdDecision::Lower(100));
    assert_eq!(c.limit(), 100);
}

pub fn test_recovery_period() {
    info!("test_recovery_period");

    let c = AimdController::new(params());
    congested_window(&c);
    assert_eq!(c.step(1_000_000, 0).decision, AimdDecision::Lower(500));

    // Congestion inside recovery does not cut again
    congested_window(&c);
    assert_eq!(c.step(2_000_000, 0).decision, AimdDecision::Hold);

    // Demand inside recovery does not raise
    c.record_demand();
    assert_eq!(c.step(3_000_000, 1000).decision, AimdDecision::Hold);

    // Recovery expired
    congested_window(&c);
    assert_eq!(
        c.step(1_000_000 + COOLDOWN_US, 0).decision,
        AimdDecision::Lower(250)
    );
}

pub fn test_ai_and_clamp_boundary() {
    info!("test_ai_and_clamp_boundary");

    let c = AimdController::new(params());
    congested_window(&c);
    assert_eq!(c.step(1_000_000, 0).decision, AimdDecision::Lower(500));

    // clamp = max(achieved*2, initial) = 1000; raise 50 per demand window
    let mut t = 1_000_000 + COOLDOWN_US;
    let mut expected = 500;
    while expected < 1000 {
        expected += 50;
        c.record_demand();
        assert_eq!(c.step(t, 500).decision, AimdDecision::Raise(expected));
        t += 1_000_000;
    }
    assert_eq!(c.limit(), 1000);

    // At the ceiling, demand no longer raises
    c.record_demand();
    assert_eq!(c.step(t, 500).decision, AimdDecision::Hold);

    // An overshooting step lands exactly on the ceiling
    let c2 = AimdController::new(AimdParams {
        ai_step: 64,
        ..params()
    });
    congested_window(&c2);
    assert_eq!(c2.step(1_000_000, 0).decision, AimdDecision::Lower(500));
    let t2 = 1_000_000 + COOLDOWN_US;
    let mut limit = 500;
    while limit < 1000 {
        c2.record_demand();
        limit = (limit + 64).min(1000);
        assert_eq!(c2.step(t2, 500).decision, AimdDecision::Raise(limit));
    }
    assert_eq!(c2.limit(), 1000);
}

pub fn test_hold_when_idle() {
    info!("test_hold_when_idle");

    let c = AimdController::new(params());
    assert_eq!(c.step(1_000_000, 0).decision, AimdDecision::Hold);

    // Successful traffic without demand does not raise
    c.record_outcome(false);
    c.record_outcome(false);
    assert_eq!(c.step(2_000_000, 5000).decision, AimdDecision::Hold);
    assert_eq!(c.limit(), 1000);
}

pub fn test_congested_fraction_threshold() {
    info!("test_congested_fraction_threshold");

    // (congested, total, expect_lower) with threshold 1/2
    for (congested, total, expect_lower) in [
        (0u64, 4u64, false),
        (1, 4, false),
        (2, 4, true),
        (1, 2, true),
        (1, 1, true),
    ] {
        let c = AimdController::new(params());
        for _ in 0..congested {
            c.record_outcome(true);
        }
        for _ in 0..total - congested {
            c.record_outcome(false);
        }
        let decision = c.step(1_000_000, 0).decision;
        if expect_lower {
            assert_eq!(decision, AimdDecision::Lower(500), "{congested}/{total}");
        } else {
            assert_eq!(decision, AimdDecision::Hold, "{congested}/{total}");
        }
    }
}

pub fn test_clamp_never_below_initial() {
    info!("test_clamp_never_below_initial");

    let c = AimdController::new(params());
    // Inflate above initial with demand and high achieved rate
    c.record_demand();
    assert_eq!(c.step(1_000_000, 5000).decision, AimdDecision::Raise(1050));

    // Achieved collapses; limit deflates to the ceiling, which floors at initial
    assert_eq!(c.step(2_000_000, 0).decision, AimdDecision::Clamp(1000));
    assert_eq!(c.step(3_000_000, 0).decision, AimdDecision::Hold);
    assert_eq!(c.limit(), 1000);
}

pub fn test_reset() {
    info!("test_reset");

    let c = AimdController::new(params());
    congested_window(&c);
    assert_eq!(c.step(1_000_000, 0).decision, AimdDecision::Lower(500));
    c.record_outcome(true);
    c.record_demand();

    c.reset();
    assert_eq!(c.limit(), 1000);

    // Window cleared by reset
    assert_eq!(c.step(2_000_000, 0).decision, AimdDecision::Hold);

    // Recovery cleared by reset: a cut is allowed immediately
    congested_window(&c);
    assert_eq!(c.step(2_500_000, 0).decision, AimdDecision::Lower(500));
}

pub fn test_step_consumes_window() {
    info!("test_step_consumes_window");

    let c = AimdController::new(params());
    congested_window(&c);
    assert_eq!(c.step(1_000_000, 0).decision, AimdDecision::Lower(500));
    assert_eq!(
        c.step(1_000_000 + COOLDOWN_US, 0).decision,
        AimdDecision::Hold
    );
}

pub fn test_all() {
    test_md_and_floor();
    test_recovery_period();
    test_ai_and_clamp_boundary();
    test_hold_when_idle();
    test_congested_fraction_threshold();
    test_clamp_never_below_initial();
    test_reset();
    test_step_consumes_window();
}
