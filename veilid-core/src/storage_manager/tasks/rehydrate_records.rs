use super::*;

impl_veilid_log_facility!("stor");

impl StorageManager {
    /// Process background rehydration requests
    #[cfg_attr(
        feature = "instrument",
        instrument(level = "trace", target = "stor", skip_all, err)
    )]
    pub(super) async fn rehydrate_records_task_routine(
        &self,
        stop_token: StopToken,
        _last_ts: Timestamp,
        _cur_ts: Timestamp,
    ) -> EyreResult<()> {
        // Bounded batch per tick so app-open bursts spread out
        let reqs = {
            let mut inner = self.inner.lock();
            let keys = inner
                .rehydration_requests
                .keys()
                .take(REHYDRATE_REQUESTS_PER_TICK)
                .cloned()
                .collect::<Vec<_>>();
            let reqs = keys
                .into_iter()
                .filter_map(|k| inner.rehydration_requests.remove_entry(&k))
                .collect::<Vec<_>>();
            if !reqs.is_empty() {
                self.mark_metadata_dirty();
            }
            reqs
        };

        let mut futs = Vec::new();
        for req in reqs {
            let stop_token = stop_token.clone();
            futs.push(async move {
                let res = self
                    .rehydrate_record(
                        stop_token,
                        req.0.clone(),
                        req.1.subkeys.clone(),
                        req.1.consensus_count,
                    )
                    .await;

                let _report = match res {
                    Ok(v) => {
                        self.clear_rehydration_failures(&req.0);
                        self.mark_rehydration_checked(req.0);
                        v
                    }
                    Err(e) => {
                        if matches!(e, VeilidAPIError::TryAgain { message: _ }) {
                            veilid_log!(self debug "Rehydration request will retry: {}", e);

                            // Try again later
                            self.add_rehydration_request(
                                req.0,
                                req.1.subkeys,
                                req.1.consensus_count,
                            );
                        } else if matches!(e, VeilidAPIError::KeyNotFound { key: _ }) {
                            veilid_log!(self debug "Rehydration request skipped: {}", e);
                        } else {
                            // One failure is the network on a bad day; a run on one record is not
                            let failures = self.note_rehydration_failure(req.0.clone());
                            if failures >= REHYDRATION_FAILURE_WARN_THRESHOLD {
                                // An empty request is the whole record, which displays as nothing
                                let subkeys = if req.1.subkeys.is_empty() {
                                    "all subkeys".to_string()
                                } else {
                                    format!("subkeys {}", req.1.subkeys)
                                };
                                veilid_log!(self warn "Rehydration of {} {} failed {} times in a row: {}", req.0, subkeys, failures, e);
                            } else {
                                veilid_log!(self debug "Rehydration request failed: {}", e);
                            }
                        }
                        return;
                    }
                };
            });
        }

        process_batched_future_queue_void(futs, REHYDRATE_BATCH_SIZE, stop_token).await;

        Ok(())
    }
}
