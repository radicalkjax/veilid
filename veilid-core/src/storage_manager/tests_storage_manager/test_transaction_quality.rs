//! Subkey confirmation coverage is per subkey over the nodes at a stage: a record
//! is durable only if every desired subkey is confirmed by at least the required
//! count, however many nodes voted. End goes only to nodes that are complete and
//! have nothing in flight.

use super::*;
use crate::storage_manager::outbound_transaction_manager::{
    min_subkey_coverage, select_end_voters, EndCandidate, LocalNodeTransactionId,
};
use crate::storage_manager::transaction_command::exits_at_consensus;

const DESIRED: [ValueSubkey; 8] = [0, 1, 2, 3, 4, 5, 6, 7];

fn confirmed(subkeys: &[ValueSubkey]) -> BTreeSet<ValueSubkey> {
    subkeys.iter().copied().collect()
}

/// The five End voters of a 16x8 concurrent bulk store whose commit returned Ok
/// with subkeys 2 and 5 held by only two of them
fn torn_end_voters() -> Vec<BTreeSet<ValueSubkey>> {
    vec![
        confirmed(&[0, 1, 3, 4, 6, 7]),
        confirmed(&[1, 2, 4, 5, 7]),
        confirmed(&[0, 1, 3, 4, 6, 7]),
        confirmed(&[0, 2, 3, 5, 6]),
        confirmed(&[0, 1, 3, 4, 6, 7]),
    ]
}

fn candidates(nodes: &[(u64, &[ValueSubkey], u32)]) -> Vec<EndCandidate> {
    nodes
        .iter()
        .map(|(id, subkeys, outstanding)| EndCandidate {
            lnxid: LocalNodeTransactionId::new(*id),
            confirmed: confirmed(subkeys),
            outstanding_sets: *outstanding,
        })
        .collect()
}

fn ids(list: &[u64]) -> Vec<LocalNodeTransactionId> {
    list.iter()
        .map(|id| LocalNodeTransactionId::new(*id))
        .collect()
}

pub fn test_select_end_voters() {
    info!("--- test_select_end_voters ---");
    // The torn record's Begin set: nobody complete, so nobody votes yet and every
    // quiescent node gets exactly its missing subkeys re-sent
    let sel = select_end_voters(
        &candidates(&[
            (1, &[0, 1, 3, 4, 6, 7], 0),
            (2, &[1, 2, 4, 5, 7], 0),
            (3, &[0, 2, 3, 5, 6], 1),
        ]),
        &DESIRED,
        2,
    );
    assert!(sel.voters.is_empty());
    assert_eq!(
        sel.completion,
        vec![
            (LocalNodeTransactionId::new(1), vec![2, 5]),
            (LocalNodeTransactionId::new(2), vec![0, 3, 6]),
        ]
    );
    assert_eq!(sel.outstanding, 1);
    assert!(sel.short);

    // Once confirmed, complete quiescent nodes vote; an in-flight one does not
    let sel = select_end_voters(
        &candidates(&[(1, &DESIRED, 0), (2, &DESIRED, 0), (3, &DESIRED, 1)]),
        &DESIRED,
        2,
    );
    assert_eq!(sel.voters, ids(&[1, 2]));
    assert!(sel.completion.is_empty());
    assert_eq!(sel.outstanding, 1);
    assert!(!sel.short);

    // A read-only transaction desires nothing, so every quiescent node votes
    let sel = select_end_voters(&candidates(&[(1, &[], 0), (2, &[], 0)]), &[], 2);
    assert_eq!(sel.voters, ids(&[1, 2]));
    assert!(!sel.short);
}

pub fn test_min_subkey_coverage_is_per_subkey() {
    info!("--- test_min_subkey_coverage_is_per_subkey ---");
    let voters = torn_end_voters();
    let nodes: Vec<&BTreeSet<ValueSubkey>> = voters.iter().collect();
    // Five voters, none complete, and subkeys 2 and 5 reach only two of them
    assert_eq!(min_subkey_coverage(&DESIRED, &nodes), Some(2));
    assert_eq!(min_subkey_coverage(&[0, 1, 3, 4, 6, 7], &nodes), Some(4));

    let full = vec![confirmed(&DESIRED); 5];
    let nodes: Vec<&BTreeSet<ValueSubkey>> = full.iter().collect();
    assert_eq!(min_subkey_coverage(&DESIRED, &nodes), Some(5));

    // Nothing desired means nothing to cover
    assert_eq!(min_subkey_coverage(&[], &nodes), None);
    // Desired but no nodes means zero coverage
    assert_eq!(min_subkey_coverage(&DESIRED, &[]), Some(0));
}

#[cfg(feature = "stats")]
pub fn test_end_coverage_accounting() {
    info!("--- test_end_coverage_accounting ---");
    let mut q = TransactionQualityAccounting::default();
    q.record_end_coverage(None, 5);
    assert_eq!(q.end_coverage.coverage_records, 0);

    q.record_end_coverage(Some(5), 5);
    q.record_end_coverage(Some(2), 5);
    q.record_end_coverage(Some(7), 5);
    assert_eq!(q.end_coverage.records_below_required_coverage, 1);
    assert_eq!(q.end_coverage.coverage_records, 3);
    assert_eq!(q.end_coverage.coverage_sum, 14);
    assert_eq!(q.end_coverage.coverage_min, 2);
}

pub fn test_end_collects_every_voter() {
    info!("--- test_end_collects_every_voter ---");
    assert!(!exits_at_consensus(TransactCommand::End));
    for command in [
        TransactCommand::Set,
        TransactCommand::Commit,
        TransactCommand::Get,
    ] {
        assert!(exits_at_consensus(command));
    }
}

pub fn test_all() {
    test_select_end_voters();
    test_min_subkey_coverage_is_per_subkey();
    #[cfg(feature = "stats")]
    test_end_coverage_accounting();
    test_end_collects_every_voter();
}
