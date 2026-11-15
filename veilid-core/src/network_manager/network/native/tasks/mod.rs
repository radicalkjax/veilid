mod confirm_dial_info;
mod network_interfaces_task;
mod upnp_task;

use super::*;

impl NativeNetwork {
    pub fn setup_tasks(&self) {
        // Set update network class tick task
        for (routing_domain, task) in self.confirm_dial_info_tasks.iter() {
            let this = self.clone();
            let routing_domain = *routing_domain;

            if matches!(routing_domain, RoutingDomain::PublicInternet) {
                task.set_routine(move |s, l, t| {
                    let this = this.clone();
                    Box::pin(async move {
                        this.confirm_public_internet_dial_info_task_routine(
                            s,
                            Timestamp::new(l),
                            Timestamp::new(t),
                        )
                        .await
                    })
                });
            } else {
                task.set_routine(move |s, l, t| {
                    let this = this.clone();
                    Box::pin(async move {
                        this.confirm_generic_dial_info_task_routine(
                            s,
                            routing_domain,
                            Timestamp::new(l),
                            Timestamp::new(t),
                        )
                        .await
                    })
                });
            }
        }

        // Set network interfaces tick task
        impl_setup_task_async_clone!(
            self,
            network_interfaces_task,
            network_interfaces_task_routine
        );

        // Set mtu detection tick task
        impl_setup_task_clone!(self, mtu_detection_task, mtu_detection_task_routine);

        // Set upnp tick task
        impl_setup_task_async_clone!(self, upnp_task, upnp_task_routine);
    }

    #[cfg_attr(
        feature = "instrument",
        instrument(level = "trace", target = "net", name = "Network::tick", skip_all, err, fields(__VEILID_LOG_KEY = self.log_key()))
    )]
    pub async fn tick(&self) -> EyreResult<()> {
        let Ok(_guard) = self.startup_lock.enter() else {
            veilid_log!(self debug "ignoring 'Network::tick' due to not started up");
            return Ok(());
        };

        // Ignore this tick if we need to restart
        if self.needs_restart() {
            return Ok(());
        }

        // If we need to figure out our network class, tick the task for it
        let needs_detect_address_changes = self.routing_domains_detecting_address_changes();

        if !needs_detect_address_changes.is_empty() {
            // Check our network interfaces to see if they have changed
            self.network_interfaces_task.tick().await?;

            // Check each routing domain's dial info confirmation task
            let needs_confirm_dial_info = self.routing_domains_needing_confirm_dial_info();
            for (routing_domain, task) in self.unlocked_inner.confirm_dial_info_tasks.iter() {
                if needs_confirm_dial_info.contains(*routing_domain) {
                    task.tick().await?;
                }
            }
        }

        // Probes settle each address family's packet size
        self.mtu_detection_task.tick().await?;

        // Tick the upnp renewal task only when UPnP is enabled (igd manager present)
        if self.igd_manager().is_some() {
            self.upnp_task.tick().await?;
        }

        Ok(())
    }

    pub async fn cancel_tasks(&self) {
        veilid_log!(self debug "stopping mtu detection task");
        if let Err(e) = self.mtu_detection_task.stop().await {
            veilid_log!(self warn "mtu_detection_task not stopped: {}", e);
        }
        veilid_log!(self debug "stopping upnp task");
        if let Err(e) = self.upnp_task.stop().await {
            veilid_log!(self warn "upnp_task not stopped: {}", e);
        }
        veilid_log!(self debug "stopping network interfaces task");
        if let Err(e) = self.network_interfaces_task.stop().await {
            veilid_log!(self warn "network_interfaces_task not stopped: {}", e);
        }
        for (routing_domain, task) in self.unlocked_inner.confirm_dial_info_tasks.iter() {
            veilid_log!(self debug "stopping confirm dial info task for routing domain: {}", routing_domain);
            if let Err(e) = task.stop().await {
                veilid_log!(self warn "confirm_dial_info_task for routing domain {} not stopped: {}", routing_domain, e);
            }
        }
    }
}
