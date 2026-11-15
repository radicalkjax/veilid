use super::*;

pub fn test_fanout_result_congested() {
    use GatedOperationKind as Op;
    for (op, kind, consensus_count, expected) in [
        (Op::Set, FanoutResultKind::Timeout, 0, true),
        (Op::Get, FanoutResultKind::Timeout, 3, true),
        (Op::Set, FanoutResultKind::Consensus, 5, false),
        // A set with nothing accepted: per-RPC timeout collapse
        (Op::Set, FanoutResultKind::Exhausted, 0, true),
        (Op::TransactSet, FanoutResultKind::Exhausted, 0, true),
        // Partial consensus: a candidate condition, not congestion
        (Op::Set, FanoutResultKind::Exhausted, 2, false),
        // A get on a nonexistent record exhausts with zero consensus normally
        (Op::Get, FanoutResultKind::Exhausted, 0, false),
        (Op::TransactGet, FanoutResultKind::Exhausted, 0, false),
        (Op::Set, FanoutResultKind::Incomplete, 0, false),
    ] {
        assert_eq!(
            fanout_congested(op, kind, consensus_count),
            expected,
            "{} {:?} consensus={}",
            op,
            kind,
            consensus_count
        );
    }
}

pub fn test_transact_counts_congested() {
    // Consensus reached; post-consensus early-exit skips are not congestion
    assert!(!transact_counts_congested(5, 4, 9, 5));
    // A single dead node is not congestion
    assert!(!transact_counts_congested(4, 1, 5, 5));
    // Majority silent without consensus
    assert!(transact_counts_congested(3, 6, 9, 5));
    // Exactly half silent without consensus
    assert!(transact_counts_congested(4, 4, 8, 5));
    // All answered Invalid: a state problem, not transport trouble
    assert!(!transact_counts_congested(2, 0, 8, 5));
    // Empty result
    assert!(!transact_counts_congested(0, 0, 0, 5));
}

pub fn test_budget_sizing_defaults() {
    let config = VeilidConfig::default();

    let weight = max_gated_operation_weight(&config);
    assert_eq!(weight, (MAX_SUBKEY_SIZE as u64) * (5 + 6) / 5);

    let initial = gated_operation_budget_initial(&config);
    assert_eq!(initial, weight * 16);

    let floor = gated_operation_budget_floor(&config);
    assert_eq!(floor, weight * 2);
    assert!(floor <= initial);

    let params = gated_operation_budget_params(&config);
    assert_eq!(params.md_cooldown_us, 5_000_000);
}

pub fn test_budget_mirror() {
    let config = VeilidConfig::default();
    let initial = gated_operation_budget_initial(&config);
    let budget = GatedOperationBandwidthBudget::new("up", &config);
    assert_eq!(budget.limit(), initial);
    assert_eq!(budget.enforced_limit(), initial);

    budget.record_outcome(true);
    budget.record_outcome(true);
    let step = budget.step(1_000_000, 0);
    assert!(matches!(step.decision, AimdDecision::Lower(_)));
    assert_eq!(budget.limit(), initial / 2);
    assert_eq!(budget.enforced_limit(), budget.limit());

    budget.reset();
    assert_eq!(budget.limit(), initial);
    assert_eq!(budget.enforced_limit(), initial);
}

pub fn test_all() {
    test_fanout_result_congested();
    test_transact_counts_congested();
    test_budget_sizing_defaults();
    test_budget_mirror();
}
