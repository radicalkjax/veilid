//! A commit is re-sent to the End-stage nodes still owed an answer while their End
//! windows are open, and a commit that landed somewhere is never a TryAgain: its short
//! records are rehydrated instead.

use super::*;
use crate::storage_manager::outbound_transaction_manager::{
    commit_outcome, plan_commit_round, CommitCandidate, CommitCoverage, CommitOutcome, CommitRound,
    LocalNodeTransactionId,
};

fn candidate(id: u64, opt_expiration: Option<u64>) -> CommitCandidate {
    CommitCandidate {
        lnxid: LocalNodeTransactionId::new(id),
        opt_expiration: opt_expiration.map(Timestamp::new),
    }
}

fn coverage(n: u8, coverage: usize, required: usize) -> CommitCoverage {
    CommitCoverage {
        opaque_record_key: OpaqueRecordKey::new(
            CRYPTO_KIND_VLD0,
            BareOpaqueRecordKey::new(&[n; 32]),
        ),
        subkeys: ValueSubkeyRangeSet::single(0),
        coverage,
        required,
    }
}

pub fn test_commit_round_targets_open_windows() {
    info!("--- test_commit_round_targets_open_windows ---");
    let now = Timestamp::new(1_000);
    // A closed window and a node the server let go are not worth a Commit; the
    // round waits until the last open window closes
    let round = plan_commit_round(
        &[
            candidate(1, Some(1_500)),
            candidate(2, Some(900)),
            candidate(3, None),
            candidate(4, Some(2_000)),
        ],
        now,
    );
    assert_eq!(
        round,
        CommitRound {
            lnxids: vec![
                LocalNodeTransactionId::new(1),
                LocalNodeTransactionId::new(4)
            ],
            opt_deadline: Some(Timestamp::new(2_000)),
        }
    );

    // A window closing right now is closed
    assert_eq!(
        plan_commit_round(&[candidate(1, Some(1_000))], now),
        CommitRound::default()
    );
}

pub fn test_commit_outcome() {
    info!("--- test_commit_outcome ---");
    assert_eq!(commit_outcome(vec![]), CommitOutcome::Reached);
    assert_eq!(
        commit_outcome(vec![coverage(1, 5, 5), coverage(2, 7, 5)]),
        CommitOutcome::Reached
    );

    // Nothing landed anywhere: the caller may try again
    assert_eq!(
        commit_outcome(vec![coverage(1, 0, 5), coverage(2, 0, 5)]),
        CommitOutcome::Missed(coverage(1, 0, 5))
    );

    // Held somewhere: it cannot be undone, so the short records are rehydrated
    assert_eq!(
        commit_outcome(vec![coverage(1, 3, 5)]),
        CommitOutcome::Partial(vec![coverage(1, 3, 5)])
    );
    assert_eq!(
        commit_outcome(vec![coverage(1, 5, 5), coverage(2, 0, 5)]),
        CommitOutcome::Partial(vec![coverage(2, 0, 5)])
    );
}

pub fn test_all() {
    test_commit_round_targets_open_windows();
    test_commit_outcome();
}
