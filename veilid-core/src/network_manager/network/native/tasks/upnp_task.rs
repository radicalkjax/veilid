use super::*;

impl_veilid_log_facility!("net");

impl NativeNetwork {
    #[cfg_attr(feature = "instrument", instrument(parent = None, level = "trace", target = "net", skip_all, err, fields(__VEILID_LOG_KEY = self.log_key())))]
    pub(super) async fn upnp_task_routine(
        &self,
        stop_token: StopToken,
        _l: Timestamp,
        _t: Timestamp,
    ) -> EyreResult<()> {
        // Only present when UPnP is enabled
        let Some(igd_manager) = self.igd_manager() else {
            return Ok(());
        };
        let Ok(outcome) = igd_manager.tick().timeout_at(stop_token).await else {
            return Ok(());
        };
        let outcome = outcome?;
        if outcome.needs_restart {
            veilid_log!(self info "upnp port mapping renew failed, triggering network restart");
            // The published dial info rests on the mapping that could not be renewed
            self.routing_table()
                .get_routing_domain_controller(RoutingDomain::PublicInternet)
                .discard_restore_point();
            let mut inner = self.inner.lock();
            inner.network_needs_restart = true;
        } else if outcome.remapped
            && self.routing_domain_request_confirm_dial_info(RoutingDomain::PublicInternet)
        {
            veilid_log!(self info "upnp port mapping moved, detecting dial info");
        } else if outcome.lost
            && self.routing_domain_request_confirm_dial_info(RoutingDomain::PublicInternet)
        {
            veilid_log!(self info "upnp port mapping lost its gateway, detecting dial info");
        } else if igd_manager.has_mappable_deferred()
            && self.routing_domain_request_confirm_dial_info(RoutingDomain::PublicInternet)
        {
            // Retried every tick until the detection skipping the mapping has finished
            igd_manager.clear_deferred();
            veilid_log!(self info "upnp gateway found after dial info detection, detecting dial info");
        }

        Ok(())
    }
}
