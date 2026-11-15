use super::*;

impl_veilid_log_facility!("rtab");

impl RoutingTable {
    // Save routing table to disk when its schedule says
    #[cfg_attr(feature = "instrument", instrument(level = "trace", skip(self), err, fields(__VEILID_LOG_KEY = self.log_key())))]
    pub async fn flush_task_routine(
        &self,
        _stop_token: StopToken,
        _last_ts: Timestamp,
        cur_ts: Timestamp,
    ) -> EyreResult<()> {
        let due = self.save_schedule.lock().due(cur_ts);
        if let Some(reason) = due {
            self.flush(reason).await;
        }

        Ok(())
    }
}
