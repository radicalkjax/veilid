//! WASM equivalent of native confirm_dial_info: learn supported address families by
//! probing nodes proven live over another family. A failed cross-family probe is
//! tolerated, like native dial-info discovery.

use super::*;

/// Maximum number of probes per family
const ADDR_FAMILY_MAX_PROBES: usize = 8;
/// Maximum number of failures to tolerate before declaring an address family unsupported
const ADDR_FAMILY_FAILURE_THRESHOLD: usize = 3;

impl WasmNetwork {
    // Reset address family confirmation state
    pub(super) fn reset_address_family_confirm_state(&self, routing_domain: RoutingDomain) -> bool {
        let configured_address_types = match routing_domain {
            RoutingDomain::PublicInternet => self.inner.lock().protocol_config.family_global,
            _ => return false,
        };

        let routing_table = self.routing_table();
        let rdc = routing_table.get_routing_domain_controller(routing_domain);
        let mut network_config = rdc.read_dyn().network_config().clone();
        network_config.address_types = configured_address_types;
        {
            let mut editor = rdc.edit_dyn();
            editor.set_network_config(network_config);
            editor.set_confirmed(false);

            self.inner
                .lock()
                .last_confirm_request_ts
                .insert(routing_domain, Timestamp::now_non_decreasing());

            editor.commit();
        }
        rdc.discard_restore_point();
        self.wake_address_family_confirm_worker();

        veilid_log!(self info "{:#} address families reset: {:?}", routing_domain, configured_address_types);
        true
    }

    /// Run a confirmation pass over the unconfirmed routing domains
    pub(super) fn wake_address_family_confirm_worker(&self) {
        if let Some(wakeup) = &self.inner.lock().address_family_wakeup {
            wakeup.wake();
        }
    }

    /// The routing domains whose address families still await a verdict: unconfirmed ones, and
    /// restored ones not yet probed on this network
    pub(super) fn address_family_domains_to_confirm(&self) -> Vec<RoutingDomain> {
        let routing_table = self.routing_table();
        let verifying = self.inner.lock().verifying_address_families.clone();
        self.routing_domains_detecting_address_changes()
            .into_iter()
            .filter(|routing_domain| {
                verifying.contains(routing_domain)
                    || !routing_table
                        .get_routing_domain_controller(*routing_domain)
                        .read_dyn()
                        .confirmed()
            })
            .collect()
    }

    // Apply address family verdict for routing domain
    fn confirm_address_families(
        &self,
        routing_domain: RoutingDomain,
        confirmed_address_families: AddressTypeSet,
    ) {
        let routing_table = self.routing_table();
        let rdc = routing_table.get_routing_domain_controller(routing_domain);
        let mut network_config = rdc.read_dyn().network_config().clone();
        network_config.address_types = confirmed_address_families;
        {
            let mut editor = rdc.edit_dyn();
            editor.set_network_config(network_config);
            editor.set_confirmed(true);
            editor.commit();
        }
        rdc.publish_peer_info();

        veilid_log!(self info "{:#} address families confirmed: {:?}", routing_domain, confirmed_address_families);
    }

    /// Start the address family confirmation worker and online/offline event listeners
    pub(super) fn start_address_family_confirm_worker(&self) {
        let stop_source = StopSource::new();
        let stop_token = stop_source.token();
        let (wake_sender, wake_receiver) = wakeup();
        // The first pass needs no request
        wake_sender.wake();
        let this = self.clone();
        let jh = spawn("address family confirm", async move {
            this.address_family_confirm_worker(wake_receiver, stop_token)
                .await;
        });

        let opt_online_event_listener_jh = if let Some(online_event_listener) =
            GLOBAL_ONLINE_EVENT.as_ref()
        {
            let this = self.clone();
            let stop_token = stop_source.token();
            Some(spawn("global scope online event listener", async move {
                let online_event_receiver = online_event_listener.receiver();
                while let Ok(Ok(_event)) = online_event_receiver
                    .recv_async()
                    .timeout_at(stop_token.clone())
                    .await
                {
                    veilid_log!(this debug "Global scope online detected");

                    // Request address family reconfirmation
                    this.routing_domain_request_confirm_dial_info(RoutingDomain::PublicInternet);
                }
            }))
        } else {
            veilid_log!(self error "Failed to create global scope online event listener");
            None
        };
        let opt_offline_event_listener_jh = if let Some(offline_event_listener) =
            GLOBAL_OFFLINE_EVENT.as_ref()
        {
            let this = self.clone();
            let stop_token = stop_source.token();
            Some(spawn("global scope offline event listener", async move {
                let offline_event_receiver = offline_event_listener.receiver();
                while let Ok(Ok(_event)) = offline_event_receiver
                    .recv_async()
                    .timeout_at(stop_token.clone())
                    .await
                {
                    veilid_log!(this debug "Global scope offline detected");

                    // Kill off all connections
                    //
                    // Needed for Chrome + Firefox because they send offline/online when
                    // switching networks but don't kill connections immediately when offline
                    //
                    // Safari kills connections when switching networks but doesn't send
                    // online/offline unless no network is available.
                    this.network_manager()
                        .connection_manager()
                        .close_all_connections();

                    // Relay latency measured before the network went away ranks nothing after it
                    this.routing_table()
                        .get_specific_routing_domain_controller::<PublicInternetRoutingDomainController>()
                        .resettle_relays();

                    // Request address family reconfirmation
                    this.routing_domain_request_confirm_dial_info(RoutingDomain::PublicInternet);
                }
            }))
        } else {
            veilid_log!(self error "Failed to create global scope offline event listener");
            None
        };

        let mut inner = self.inner.lock();
        inner.address_family_wakeup = Some(wake_sender);
        inner.address_family_stop_source = Some(stop_source);
        inner.address_family_worker_jh = Some(jh);
        inner.online_event_listener_jh = opt_online_event_listener_jh;
        inner.offline_event_listener_jh = opt_offline_event_listener_jh;
    }

    async fn address_family_confirm_worker(&self, wakeup: WakeReceiver, stop_token: StopToken) {
        while wakeup.wait(None, stop_token.clone()).await == WakeOutcome::Woken {
            for routing_domain in self.address_family_domains_to_confirm() {
                if let Err(e) = self
                    .perform_address_family_confirmation_pass(routing_domain, &stop_token)
                    .await
                {
                    veilid_log!(self error "Failed to confirm address families for routing domain {:?}: {}", routing_domain, e);
                }
            }
        }
    }

    /// One confirmation pass: probe each allowed family, drop conclusively-unreachable ones,
    /// then set confirmed=true. A deferred outcome leaves the domain unconfirmed so the
    /// worker retries next tick.
    async fn perform_address_family_confirmation_pass(
        &self,
        routing_domain: RoutingDomain,
        stop_token: &StopToken,
    ) -> EyreResult<()> {
        // Collect candidate node refs before awaiting
        let address_family_confirm_candidates =
            self.collect_family_probe_candidates(routing_domain)?;

        let outcome = match address_family_confirm_candidates.state() {
            AddressFamilyConfirmState::NoConfiguredAddressTypes => {
                veilid_log!(self debug "no address types configured for routing domain {:#}; confirming no address types", routing_domain);
                ConfirmationPassOutcome::Confirm(AddressTypeSet::empty())
            }
            AddressFamilyConfirmState::AllConfiguredConfirmed => {
                veilid_log!(self debug "all address types confirmed for routing domain {:#}; enabling all configured address types", routing_domain);
                ConfirmationPassOutcome::Confirm(
                    address_family_confirm_candidates.confirmed_address_types,
                )
            }
            AddressFamilyConfirmState::NotEnoughCandidates => {
                veilid_log!(self debug "not enough candidates to confirm address types for routing domain {:#}; skipping confirmation", routing_domain);
                ConfirmationPassOutcome::Deferred
            }
            AddressFamilyConfirmState::ConfirmCandidates => {
                veilid_log!(self debug "Need to confirm address types {:?} for routing domain {:#}", address_family_confirm_candidates.candidates.keys(), routing_domain);

                let seen_live = address_family_confirm_candidates.confirmed_address_types;
                let mut dispositions = Vec::new();
                for (at, cands) in address_family_confirm_candidates.candidates {
                    let disposition = self
                        .probe_address_family(routing_domain, at, cands, stop_token)
                        .await;
                    if matches!(disposition, FamilyProbeDisposition::Inconclusive) {
                        // Unreachable when state() guaranteed enough candidates
                        veilid_log!(self error "inconclusive result confirming address type {:#} for routing domain {:#}", at, routing_domain);
                    }
                    let cancelled = matches!(disposition, FamilyProbeDisposition::Cancelled);
                    dispositions.push((at, disposition));
                    if cancelled {
                        break;
                    }
                }
                resolve_probe_dispositions(seen_live, &dispositions)
            }
        };

        if let ConfirmationPassOutcome::Confirm(confirmed_address_types) = outcome {
            self.inner
                .lock()
                .verifying_address_families
                .remove(&routing_domain);
            self.confirm_address_families(routing_domain, confirmed_address_types);
        }

        Ok(())
    }

    /// Per family, nodes with a WS/WSS dial info in that family that are live over another
    /// family (so a probe failure is our egress, not a dead node). Returns map + node count.
    fn collect_family_probe_candidates(
        &self,
        routing_domain: RoutingDomain,
    ) -> EyreResult<AddressFamilyConfirmCandidates<NodeRef>> {
        // All possible configured address types we can confirm
        let configured_address_types = configured_address_type_set(&self.config());

        // List of candidates that can be pinged for positive confirmation of missing address types
        let mut candidates: BTreeMap<AddressType, Vec<NodeRef>> = BTreeMap::new();

        // All address types we have positively seen since the last confirm request
        let mut confirmed_address_types = AddressTypeSet::empty();

        // Only families reachable over a protocol we can actually dial outbound
        let (outbound_protocols, last_confirm_request_ts) = {
            let inner = self.inner.lock();

            let outbound_protocols = inner.protocol_config.outbound;
            let Some(last_confirm_request_ts) =
                inner.last_confirm_request_ts.get(&routing_domain).copied()
            else {
                // Should have had a request timestamp for this routing domain
                bail!(
                    "No last confirm request timestamp for routing domain {:?}",
                    routing_domain
                );
            };

            (outbound_protocols, last_confirm_request_ts)
        };

        // Make snapshot of all nodes in the routing table that have been seen consistently
        let cur_ts = Timestamp::now();
        let snapshot = self
            .routing_table()
            .snapshot_entries(cur_ts, BucketEntryState::Unreliable);

        for snap in snapshot.entries() {
            let Some(pi) = snap.get_peer_info(routing_domain) else {
                continue;
            };

            // Families we've received answers over since the last confirm request
            let mut snap_live_families = AddressTypeSet::empty();
            for (tt, st) in snap.per_transport_stats.iter() {
                // Skip any address types that are not configured for this routing domain
                if !configured_address_types.contains(tt.address_type()) {
                    continue;
                }
                if let Some(last_seen_ts) = st.last_seen_ts.as_ref().copied() {
                    if last_seen_ts > last_confirm_request_ts {
                        // Record some node as having positive confirmation of this address type
                        confirmed_address_types.insert(tt.address_type());

                        // Also record for this node specifically
                        snap_live_families.insert(tt.address_type());
                    }
                }
            }

            let ni = pi.node_info();

            for at in configured_address_types {
                // Skip address type if no directly dialable address for it
                let has_dialable_in_at = ni.dial_info_detail_list().iter().any(|did| {
                    !did.class.requires_signal()
                        && did.dial_info.address_type() == at
                        && outbound_protocols.contains(did.dial_info.protocol_type())
                });
                if !has_dialable_in_at {
                    continue;
                }

                // If this node supports this address type but has already been seen live for it
                // then we can skip this address type
                if snap_live_families.contains(at) {
                    continue;
                }

                // Record ping candidate for this address type
                candidates
                    .entry(at)
                    .or_default()
                    .push(snap.node_ref.clone());
            }
        }

        // Remove any candidate address types that have already been confirmed
        for at in confirmed_address_types.iter() {
            candidates.remove(&at);
        }

        // Return the candidates
        Ok(AddressFamilyConfirmCandidates {
            configured_address_types,
            confirmed_address_types,
            candidates,
        })
    }

    /// >=1 answer => Supported; N controlled failures => Unsupported; else Inconclusive
    async fn probe_address_family(
        &self,
        routing_domain: RoutingDomain,
        at: AddressType,
        candidates: Vec<NodeRef>,
        stop_token: &StopToken,
    ) -> FamilyProbeDisposition {
        let mut failures = 0usize;
        for nr in candidates.into_iter().take(ADDR_FAMILY_MAX_PROBES) {
            let fnr = nr.custom_filtered(
                NodeRefFilter::new()
                    .with_routing_domain(routing_domain)
                    .with_address_type(at),
            );
            let res = match self
                .rpc_processor()
                .rpc_call_status(Destination::direct(fnr, None), None)
                .timeout_at(stop_token.clone())
                .await
            {
                Ok(res) => res,
                Err(_) => return FamilyProbeDisposition::Cancelled,
            };
            match res {
                Ok(StatusResult::Sent(_)) => return FamilyProbeDisposition::Supported,
                _ => {
                    failures += 1;
                    if failures >= ADDR_FAMILY_FAILURE_THRESHOLD {
                        return FamilyProbeDisposition::Unsupported;
                    }
                }
            }
        }
        FamilyProbeDisposition::Inconclusive
    }
}
