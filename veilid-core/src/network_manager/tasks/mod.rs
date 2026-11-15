pub mod rolling_transfers;
use super::*;

impl NetworkManager {
    pub fn setup_tasks(&self) {
        // Set rolling transfers tick task
        impl_setup_task_async!(
            self,
            Self,
            rolling_transfers_task,
            rolling_transfers_task_routine
        );

        // Set transport stats health log task
        impl_setup_task!(
            self,
            Self,
            transport_stats_log_task,
            transport_stats_log_task_routine
        );

        // Set address filter task
        impl_setup_task_async!(self, Self, address_filter_task, address_filter_task_routine);
    }

    fn transport_stats_log_task_routine(
        &self,
        _stop_token: StopToken,
        _last_ts: Timestamp,
        _cur_ts: Timestamp,
    ) -> EyreResult<()> {
        self.transport_stats().log_health();
        Ok(())
    }

    async fn address_filter_task_routine(
        &self,
        stop_token: StopToken,
        last_ts: Timestamp,
        cur_ts: Timestamp,
    ) -> EyreResult<()> {
        self.address_filter()
            .address_filter_task_routine(stop_token, last_ts, cur_ts)
            .await
    }

    pub async fn tick_event_handler(&self, _evt: Arc<TickEvent>) {
        if let Err(e) = self.tick().await {
            error!("Error in network manager tick: {}", e);
        }
    }

    #[cfg_attr(
        feature = "instrument",
        instrument(level = "trace", name = "NetworkManager::tick", skip_all, err, fields(__VEILID_LOG_KEY = self.log_key()))
    )]
    async fn tick(&self) -> EyreResult<()> {
        let Ok(_startup_guard) = self.startup_context.startup_lock.enter() else {
            return Ok(());
        };

        let net = self.net();

        // Run the rolling transfers task
        self.rolling_transfers_task.tick().await?;

        // Run the address filter task
        self.address_filter_task.tick().await?;

        // Log our own per-transport send health
        self.transport_stats_log_task.tick().await?;

        // Run the low level network tick
        net.tick().await?;

        // Purge the client allowlist
        self.purge_client_allowlist();

        Ok(())
    }

    pub async fn cancel_tasks(&self) {
        veilid_log!(self debug "stopping receipt manager tasks");
        let receipt_manager = self.receipt_manager();
        receipt_manager.cancel_tasks().await;

        let net = self.net();
        net.cancel_tasks().await;

        veilid_log!(self debug "stopping rolling transfers task");
        if let Err(e) = self.rolling_transfers_task.stop().await {
            veilid_log!(self warn "rolling_transfers_task not stopped: {}", e);
        }

        veilid_log!(self debug "stopping transport stats log task");
        if let Err(e) = self.transport_stats_log_task.stop().await {
            veilid_log!(self warn "transport_stats_log_task not stopped: {}", e);
        }

        veilid_log!(self debug "stopping address filter task");
        if let Err(e) = self.address_filter_task.stop().await {
            veilid_log!(self warn "address_filter_task not stopped: {}", e);
        }
    }
}
