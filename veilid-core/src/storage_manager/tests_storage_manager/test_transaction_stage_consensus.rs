//! A record's stage consensus is the one live stage enough node transactions share; a node
//! that never answered is marked Failed for bookkeeping and must not outvote that stage.

use super::*;
use crate::storage_manager::outbound_transaction_manager::{
    record_consensus_stage, LocalNodeTransactionId, OutboundTransactionStage,
};

fn ids(range: std::ops::Range<u64>) -> BTreeSet<LocalNodeTransactionId> {
    range.map(LocalNodeTransactionId::new).collect()
}

pub fn test_failed_nodes_do_not_outvote_a_live_stage() {
    info!("--- test_failed_nodes_do_not_outvote_a_live_stage ---");
    let mut map = HashMap::new();
    map.insert(OutboundTransactionStage::End, ids(0..5));
    map.insert(OutboundTransactionStage::Failed, ids(5..10));
    assert_eq!(
        record_consensus_stage(&map, 5),
        OutboundTransactionStage::End
    );

    let mut map = HashMap::new();
    map.insert(OutboundTransactionStage::Failed, ids(0..10));
    assert_eq!(
        record_consensus_stage(&map, 5),
        OutboundTransactionStage::Failed
    );
}

pub fn test_two_live_stages_or_too_few_is_failed() {
    info!("--- test_two_live_stages_or_too_few_is_failed ---");
    let mut map = HashMap::new();
    map.insert(OutboundTransactionStage::End, ids(0..5));
    map.insert(OutboundTransactionStage::Commit, ids(5..10));
    assert_eq!(
        record_consensus_stage(&map, 5),
        OutboundTransactionStage::Failed
    );

    let mut map = HashMap::new();
    map.insert(OutboundTransactionStage::End, ids(0..4));
    assert_eq!(
        record_consensus_stage(&map, 5),
        OutboundTransactionStage::Failed
    );
}

pub fn test_all() {
    test_failed_nodes_do_not_outvote_a_live_stage();
    test_two_live_stages_or_too_few_is_failed();
}
