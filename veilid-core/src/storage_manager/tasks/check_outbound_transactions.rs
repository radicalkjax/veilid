use super::*;

impl StorageManager {
    /// Drop the client-side transactions that have expired, returning when the next one expires.
    /// Held while the DHT is offline; going online runs it again.
    #[cfg_attr(
        feature = "instrument",
        instrument(level = "trace", target = "stor", skip_all)
    )]
    pub(super) fn check_outbound_transactions_timer_routine(
        &self,
        cur_ts: Timestamp,
    ) -> Option<Timestamp> {
        if !self.dht_is_online() {
            return None;
        }

        let (expired_transactions, opt_next_expiry) = self
            .inner
            .lock()
            .outbound_transaction_manager
            .mark_expired_transactions(cur_ts);

        for (transaction_handle, transaction_keys) in expired_transactions {
            self.drop_expired_transaction(transaction_handle, transaction_keys);
        }

        opt_next_expiry
    }

    /// Drop an expired transaction once its records are free, if it is still expired then
    fn drop_expired_transaction(
        &self,
        transaction_handle: OutboundTransactionHandle,
        transaction_keys: Vec<OpaqueRecordKey>,
    ) {
        let registry = self.registry();
        self.background_operation_processor.add_future(async move {
            let this = registry.storage_manager();

            let records_lock = this
                .record_lock_table
                .lock_records(
                    transaction_keys,
                    StorageManagerRecordLockPurpose::TransactDrop,
                )
                .await;

            let opt_state = this
                .inner
                .lock()
                .outbound_transaction_manager
                .drop_if_expired(transaction_handle, Timestamp::now());
            if let Some(state) = opt_state {
                veilid_log!(this debug "Dropping expired transaction: {}", transaction_handle);
                state.into_transaction_cleanup().await;
            }

            drop(records_lock);
        });
    }
}
