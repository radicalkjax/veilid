use super::*;

impl_veilid_log_facility!("rtab");

impl LocalNetworkRoutingDomainController {
    pub fn setup_tasks(&self) {
        // Set ping validator LocalNetwork tick task
        impl_setup_task_clone!(
            self,
            ping_validator_local_network_task,
            ping_validator_local_network_task_routine
        );
    }

    pub async fn tick(&self) -> EyreResult<()> {
        let state = self.state();

        // Suppression enforcement is armed only once outbound is ready
        self.network_manager()
            .transport_stats()
            .set_enforcing(RoutingDomain::LocalNetwork, state.is_ready_outbound);

        match state.outbound_stage {
            RoutingDomainOutboundStage::Invalid | RoutingDomainOutboundStage::NeedsBootstrap => {
                // Do nothing
            }
            RoutingDomainOutboundStage::NeedsPublishedPeerInfo
            | RoutingDomainOutboundStage::NeedsSafetyRoutes
            | RoutingDomainOutboundStage::NeedsMoreTestedNodes
            | RoutingDomainOutboundStage::ReadyToOperate => {
                self.ping_validator_local_network_task.tick().await?;
            }
        }

        Ok(())
    }
    pub async fn cancel_tasks(&self) {
        veilid_log!(self debug "stopping LocalNetwork ping_validator tasks");

        if let Err(e) = self.ping_validator_local_network_task.stop().await {
            veilid_log!(self warn "ping_validator_local_network_task not stopped: {}", e);
        }
    }

    // Task routine for LocalNetwork status pings
    #[cfg_attr(feature = "instrument", instrument(level = "trace", skip(self), err))]
    pub fn ping_validator_local_network_task_routine(
        &self,
        _stop_token: StopToken,
        _last_ts: Timestamp,
        cur_ts: Timestamp,
    ) -> EyreResult<()> {
        self.routing_table()
            .add_reliability_ping_validations(cur_ts, RoutingDomain::LocalNetwork)?;

        Ok(())
    }
}
