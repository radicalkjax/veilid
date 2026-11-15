use super::*;

mod test_change_inspect_trigger;
mod test_commit_rounds;
mod test_encrypted_value_data;
mod test_freeze_recovery;
#[cfg(feature = "aimd-dht")]
mod test_gated_operation;
mod test_get_value_store;
mod test_inbound_set_value_signature;
mod test_inbound_transaction_set_validation;
mod test_inspect_fanout_done;
mod test_metadata_round_trip;
mod test_metadata_save_schedule;
mod test_min_seqnum;
mod test_record_lock_preemption;
mod test_record_report_seqs;
mod test_rehydration_failure_runs;
mod test_subkey_lock;
mod test_transaction_quality;
mod test_transaction_stage_consensus;
mod test_unanswered_inspect;
mod test_value_split;
mod test_watch_bookkeeping;
mod test_watch_fallback_backoff;
mod test_watch_node_cap;
mod test_watch_route_pinning;
mod test_watch_subkey_filter;

pub async fn test_all() {
    test_commit_rounds::test_all();
    test_encrypted_value_data::test_all().await;
    test_freeze_recovery::test_all();
    #[cfg(feature = "aimd-dht")]
    test_gated_operation::test_all();
    test_inbound_set_value_signature::test_all().await;
    test_inbound_transaction_set_validation::test_all().await;
    test_inspect_fanout_done::test_all();
    test_metadata_round_trip::test_metadata_round_trip().await;
    test_metadata_save_schedule::test_all().await;
    test_min_seqnum::test_all().await;
    test_get_value_store::test_all().await;
    test_subkey_lock::test_all().await;
    test_record_lock_preemption::test_all().await;
    test_record_report_seqs::test_all();
    test_rehydration_failure_runs::test_all();
    test_transaction_quality::test_all();
    test_transaction_stage_consensus::test_all();
    test_unanswered_inspect::test_all().await;
    test_watch_bookkeeping::test_all().await;
    test_change_inspect_trigger::test_all();
    test_watch_fallback_backoff::test_all();
    test_watch_node_cap::test_all();
    test_watch_route_pinning::test_all().await;
    test_watch_subkey_filter::test_all().await;
    record_store::tests::test_record_store().await;
    test_value_split::test_all().await;
}
