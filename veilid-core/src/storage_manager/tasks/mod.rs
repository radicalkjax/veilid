pub mod check_inbound_transactions;
pub mod check_inbound_watches;
pub mod check_outbound_transactions;
pub mod check_outbound_watches;
pub mod flush_record_stores;
pub mod offline_subkey_writes;
pub mod rehydrate_records;
pub mod save_metadata;
pub mod send_value_changes;

use super::*;

impl StorageManager {
    pub(super) fn setup_tasks(&self) {
        // Set flush records tick task
        veilid_log!(self debug "starting flush record stores task");
        impl_setup_task_async!(
            self,
            Self,
            flush_record_stores_task,
            flush_record_stores_task_routine
        );

        // Set save metadata task
        veilid_log!(self debug "starting save metadata task");
        impl_setup_task_async!(self, Self, save_metadata_task, save_metadata_task_routine);

        // Set offline subkey writes tick task
        veilid_log!(self debug "starting offline subkey writes task");
        impl_setup_task_async!(
            self,
            Self,
            offline_subkey_writes_task,
            offline_subkey_writes_task_routine
        );

        // Set send value changes tick task
        veilid_log!(self debug "starting send value changes task");
        impl_setup_task_async!(
            self,
            Self,
            send_value_changes_task,
            send_value_changes_task_routine
        );

        // Set check active watches tick task
        veilid_log!(self debug "starting check outbound watches task");
        impl_setup_task!(
            self,
            Self,
            check_outbound_watches_task,
            check_outbound_watches_task_routine
        );

        // Set rehydrate records tick task
        veilid_log!(self debug "starting rehydrate records task");
        impl_setup_task_async!(
            self,
            Self,
            rehydrate_records_task,
            rehydrate_records_task_routine
        );
    }

    /// Start the deadline timers, once the record stores are open
    pub(super) fn start_deadline_timers(&self) {
        veilid_log!(self debug "starting deadline timers");
        self.start_expiry_timer(
            &self.check_inbound_watches_timer,
            Self::check_inbound_watches_timer_routine,
        );
        self.start_expiry_timer(
            &self.check_inbound_transactions_timer,
            Self::check_inbound_transactions_timer_routine,
        );
        self.start_expiry_timer(
            &self.check_outbound_transactions_timer,
            Self::check_outbound_transactions_timer_routine,
        );
        let registry = self.registry();
        self.background_quiet_timer
            .start(move |cur_ts, _stop_token| {
                let registry = registry.clone();
                Box::pin(async move {
                    registry
                        .storage_manager()
                        .background_quiet_timer_routine(Timestamp::new(cur_ts))
                        .await
                        .map(|ts| ts.as_u64())
                })
            });
    }

    /// Restart background work deferred for foreground transactions once the quiet window ends,
    /// rather than at the next tick
    async fn background_quiet_timer_routine(&self, cur_ts: Timestamp) -> Option<Timestamp> {
        let quiet_until = self.background_quiet_until();
        if cur_ts < quiet_until {
            return Some(quiet_until);
        }
        // An open foreground transaction defers it further, and its end arms the timer again
        if !self.background_transactions_allowed() || !self.dht_is_online() {
            return None;
        }
        if self.has_rehydration_requests() {
            if let Err(e) = self.rehydrate_records_task.try_tick_now().await {
                veilid_log!(self debug "rehydrate_records_task not kicked: {}", e);
            }
        }
        if let Err(e) = self.check_outbound_watches_task.try_tick_now().await {
            veilid_log!(self debug "check_outbound_watches_task not kicked: {}", e);
        }
        None
    }

    fn start_expiry_timer(
        &self,
        timer: &DeadlineTimer,
        routine: fn(&StorageManager, Timestamp) -> Option<Timestamp>,
    ) {
        let registry = self.registry();
        timer.start(move |cur_ts, _stop_token| {
            let opt_next = routine(&registry.storage_manager(), Timestamp::new(cur_ts));
            Box::pin(async move { opt_next.map(|ts| ts.as_u64()) })
        });
    }

    #[cfg_attr(feature = "instrument", instrument(parent = None, level = "trace", target = "stor", name = "StorageManager::tick", skip_all, err))]
    pub async fn tick(&self) -> EyreResult<()> {
        let Ok(_startup_guard) = self.startup_lock.enter() else {
            return Ok(());
        };

        // Run the flush stores task
        self.flush_record_stores_task.tick().await?;

        // Run the flush stores task
        self.save_metadata_task.tick().await?;

        // Run online-only tasks
        if self.dht_is_online() {
            // Check active watches
            self.check_outbound_watches_task.tick().await?;

            // Run offline subkey writes task if there's work to be done
            if self.has_offline_subkey_writes() {
                self.offline_subkey_writes_task.tick().await?;
            }

            // Do requested rehydrations, yielding to foreground transaction activity
            if self.has_rehydration_requests() && self.background_transactions_allowed() {
                self.rehydrate_records_task.tick().await?;
            }

            // Send value changed notifications
            self.send_value_changes_task.tick().await?;
        }

        Ok(())
    }

    #[cfg_attr(
        feature = "instrument",
        instrument(level = "trace", target = "stor", skip_all)
    )]
    pub(super) async fn cancel_tasks(&self) {
        veilid_log!(self debug "stopping deadline timers");
        self.background_quiet_timer.stop().await;
        self.check_inbound_transactions_timer.stop().await;
        self.check_outbound_transactions_timer.stop().await;
        self.check_inbound_watches_timer.stop().await;
        veilid_log!(self debug "stopping check outbound watches task");
        if let Err(e) = self.check_outbound_watches_task.stop().await {
            veilid_log!(self warn "check_outbound_watches_task not stopped: {}", e);
        }
        veilid_log!(self debug "stopping send value changes task");
        if let Err(e) = self.send_value_changes_task.stop().await {
            veilid_log!(self warn "send_value_changes_task not stopped: {}", e);
        }
        veilid_log!(self debug "stopping flush record stores task");
        if let Err(e) = self.flush_record_stores_task.stop().await {
            veilid_log!(self warn "flush_record_stores_task not stopped: {}", e);
        }
        veilid_log!(self debug "stopping offline subkey writes task");
        if let Err(e) = self.offline_subkey_writes_task.stop().await {
            veilid_log!(self warn "offline_subkey_writes_task not stopped: {}", e);
        }
        veilid_log!(self debug "stopping save metadata task");
        if let Err(e) = self.save_metadata_task.stop().await {
            veilid_log!(self warn "save_metadata_task not stopped: {}", e);
        }
        veilid_log!(self debug "stopping record rehydration task");
        if let Err(e) = self.rehydrate_records_task.stop().await {
            veilid_log!(self warn "rehydrate_records_task not stopped: {}", e);
        }
    }
}
