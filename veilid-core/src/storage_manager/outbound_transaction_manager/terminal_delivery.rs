use super::*;

impl_veilid_log_facility!("stor");

/// Retries the terminal command for a node transaction that was dropped without an
/// answer, until the remote answers, the server-side expiration passes (the remote
/// drops it then anyway), or shutdown. Any well-formed answer completes delivery:
/// the remote executed the command, or it no longer knows the transaction.
///
/// A lost terminal RPC leaves the remote holding the transaction until its TTL, and a
/// schema member gets one concurrent write transaction per writer key, so that stale
/// holder blocks the writer's own next begin for the whole TTL.
#[derive(Debug)]
pub(in crate::storage_manager) struct TerminalDelivery {
    /// The record the node transaction is over
    pub opaque_record_key: OpaqueRecordKey,
    /// Server-side transaction id
    pub rnxid: RemoteNodeTransactionId,
    /// The node holding the transaction
    pub node_ref: NodeRef,
    /// Safety selection, applied fresh per attempt so retries survive route rotation
    pub safety_selection: SafetySelection,
    /// Descriptor for the record
    pub descriptor: Arc<SignedValueDescriptor>,
    /// Commit for an ended straggler in a committed transaction, else Rollback
    pub command: TransactCommand,
    /// Server-side expiration, past which the remote has dropped it anyway
    pub expiration_ts: Timestamp,
}

impl TerminalDelivery {
    /// Retry until answered, expired, or shutdown; runs detached in the background
    /// operation processor.
    pub async fn run(self, registry: VeilidComponentRegistry) {
        // Doubles per attempt, clamped to the remaining server-side lifetime
        let mut backoff_ms = u64::from(
            registry
                .storage_manager()
                .timings()
                .terminal_delivery_backoff_start
                .millis_u32()
                .unwrap_or_default(),
        );
        let mut attempts = 0u32;
        loop {
            if Timestamp::now() >= self.expiration_ts {
                veilid_log!(registry debug target: "network_result",
                    "terminal delivery expired after {} attempts: {:?} xid={} node={} record={}",
                    attempts, self.command, self.rnxid, self.node_ref, self.opaque_record_key);
                veilid_stats!(
                    registry
                        .storage_manager()
                        .transaction_quality()
                        .lock()
                        .mitigations
                        .terminal_delivery_expired += 1
                );
                return;
            }

            attempts += 1;
            let dest = Destination::direct(
                self.node_ref
                    .routing_domain_filtered(RoutingDomain::PublicInternet),
                Some(self.safety_selection.clone()),
            );
            let rpc_processor = registry.rpc_processor();
            let res = rpc_processor
                .rpc_call_transact_command(
                    dest,
                    self.opaque_record_key.clone(),
                    self.descriptor.clone(),
                    self.rnxid.xid(),
                    self.command,
                    None,
                    None,
                    None,
                )
                .await;

            // Anything short of a well-formed answer retries; shutdown cancels this
            // future via the background processor's stop token
            if let Ok(NetworkResult::Value(_answer)) = res {
                veilid_log!(registry debug target: "network_result",
                    "terminal delivery complete (attempt {}): {:?} xid={} node={} record={}",
                    attempts, self.command, self.rnxid, self.node_ref, self.opaque_record_key);
                veilid_stats!(
                    registry
                        .storage_manager()
                        .transaction_quality()
                        .lock()
                        .mitigations
                        .terminal_delivery_completed += 1
                );
                return;
            }

            // Never sleep past expiration; the top-of-loop check then exits promptly
            let remaining_ms = self.expiration_ts.duration_since(Timestamp::now()).as_u64() / 1_000;
            sleep(backoff_ms.min(remaining_ms) as u32).await;
            backoff_ms = backoff_ms.saturating_mul(2);
        }
    }
}
