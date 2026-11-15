//! How large our packets can be in each address family, and what we advertise we take.
//! - A family starts at the safe size and waits for a probe before it settles
//! - Only an egress move or a change of the egress interface's mtu starts it over
//! - A probe is a full-size StatusQ asking for a full-size answer, sent beside a plain one to the
//!   same peer: the plain answer arriving alone says the big packets were lost
//! - A pass probes its peers at once; the tick hands passes to a processor and reads results

use super::*;
use futures_util::future::{select, Either};
use std::pin::pin;

/// Peers that can answer a probe, gathered per family each time one is due
const MTU_PROBE_CANDIDATES: usize = 8;
/// Peers probed per family in one pass; a black hole needs two losses and no full round trip
const MTU_PROBE_PEERS: usize = 4;
/// Peers an unsettled family remembers as tried, after which it forgets them all
const MTU_PROBE_TRIED_MAX: usize = 256;
/// An answer gets this many of the round trips that predict it
const MTU_PROBE_RTT_FACTOR: u64 = 3;
/// An answer gets at least this percent of the RPC timeout
const MTU_PROBE_GRACE_PERCENT: u32 = 33;

/// A peer a pass probes, over the dial info and at the size it is to carry
pub(super) struct MtuProbeCandidate {
    /// Who answers the probe
    node: NodeRef,
    /// The UDP dial info the probe takes
    dial_info: DialInfo,
    /// The full size the family would carry
    size: u16,
}

/// What a peer's answer gets: a multiple of the round trip that predicts it, floored by a
/// share of the RPC timeout and capped at the timeout
fn probe_grace_ms(rpc_timeout_ms: u32, rtt_ms: u64) -> u32 {
    let floor_ms = u64::from(rpc_timeout_ms * MTU_PROBE_GRACE_PERCENT / 100);
    let grace_ms = rtt_ms
        .saturating_mul(MTU_PROBE_RTT_FACTOR)
        .max(floor_ms)
        .min(u64::from(rpc_timeout_ms));
    u32::try_from(grace_ms).unwrap_or(rpc_timeout_ms)
}

impl NativeNetwork {
    /// Starts over every in-use family whose egress moved or whose interface mtu changed
    pub(super) fn refresh_family_mtus(
        &self,
        interface_address_state: &NetworkInterfaceAddressState,
        family_global: AddressTypeSet,
        moved: AddressTypeSet,
    ) {
        let egress_mtus = family_global
            .iter()
            .map(|address_type| {
                let opt_egress = match address_type {
                    AddressType::IPV4 => interface_address_state.egress_v4.as_ref(),
                    AddressType::IPV6 => interface_address_state.egress_v6.as_ref(),
                };
                let interface_mtu = opt_egress.and_then(|egress| {
                    self.interfaces.with_interfaces(|interfaces| {
                        interfaces
                            .get(&egress.interface)
                            .and_then(|intf| intf.mtu())
                    })
                });
                (address_type, interface_mtu)
            })
            .collect::<Vec<_>>();

        let generation = self.interface_address_generation.load(Ordering::Relaxed);
        {
            let mut inner = self.inner.lock();
            inner
                .family_mtus
                .retain(|address_type, _| family_global.contains(*address_type));
            for (address_type, interface_mtu) in egress_mtus {
                let unchanged = !moved.contains(address_type)
                    && inner
                        .family_mtus
                        .get(&address_type)
                        .is_some_and(|family_mtu| family_mtu.interface_mtu == interface_mtu);
                if unchanged {
                    continue;
                }
                let family_mtu = FamilyMtu::new(interface_mtu, address_type, generation);
                veilid_log!(self debug "{} mtu starts over: interface mtu {:?}, {:?}", address_type, interface_mtu, family_mtu.state);
                inner.family_mtus.insert(address_type, family_mtu);
            }
        }

        self.apply_family_mtus();
    }

    /// Puts each family's sizes where the send path reads them, and where TCP listeners announce
    /// them
    fn apply_family_mtus(&self) {
        let family_mtus = self.inner.lock().family_mtus.clone();
        let network_manager = self.network_manager();
        let own_mtus = network_manager.own_mtus();
        for address_type in AddressTypeSet::all() {
            let safe = (SAFE_TRANSPORT_MTU, SAFE_TRANSPORT_MTU);
            let ((udp, tcp), (udp_limit, tcp_limit)) = family_mtus
                .get(&address_type)
                .map(|family_mtu| {
                    (
                        family_mtu.mtus(address_type),
                        family_mtu.limits(address_type),
                    )
                })
                .unwrap_or((safe, safe));
            own_mtus.set(
                LowLevelTransportType {
                    low_level_protocol_type: LowLevelProtocolType::UDP,
                    address_type,
                },
                udp,
                udp_limit,
            );
            own_mtus.set(
                LowLevelTransportType {
                    low_level_protocol_type: LowLevelProtocolType::TCP,
                    address_type,
                },
                tcp,
                tcp_limit,
            );
        }

        let inner = self.inner.lock();
        for (addr, listener_mss_handle) in &inner.listener_mss_handles {
            listener_mss_handle.set_mss(own_tcp_mss(&self.registry(), *addr));
        }
    }

    /// What our peer info says we take; None while any family is still detecting
    pub(super) fn receive_mtus(&self) -> Option<TransportMtus> {
        let inner = self.inner.lock();
        if inner
            .family_mtus
            .values()
            .any(|family_mtu| family_mtu.state == FamilyMtuState::Detecting)
        {
            return None;
        }
        let mut receive_mtus = TransportMtus::default();
        for (address_type, family_mtu) in &inner.family_mtus {
            let (udp, tcp) = family_mtu.mtus(*address_type);
            receive_mtus.set(
                LowLevelTransportType {
                    low_level_protocol_type: LowLevelProtocolType::UDP,
                    address_type: *address_type,
                },
                udp,
            );
            receive_mtus.set(
                LowLevelTransportType {
                    low_level_protocol_type: LowLevelProtocolType::TCP,
                    address_type: *address_type,
                },
                tcp,
            );
        }
        Some(receive_mtus)
    }

    /// Spawns the pass processor for this network start; the tick feeds it and reads its results
    pub(super) fn start_mtu_probe_processor(&self) {
        let (pass_tx, pass_rx) = flume::unbounded();
        let (result_tx, result_rx) = flume::unbounded();
        let this = self.clone();
        let prober: MtuProber<MtuProbeCandidate> = Arc::new(move |candidate| {
            let this = this.clone();
            Box::pin(async move { Box::pin(this.mtu_probe(candidate)).await })
        });

        let mut inner = self.inner.lock();
        let Some(stop_token) = inner.stop_source.as_ref().map(|s| s.token()) else {
            veilid_log!(self error "mtu probe processor needs the network's stop source");
            return;
        };
        let jh = spawn(
            "mtu probe processor",
            mtu_probe_processor(prober, stop_token, pass_rx, result_tx),
        );
        inner.mtu_probe_passes = Some(pass_tx);
        inner.mtu_probe_results = Some(result_rx);
        inner.join_handles.push(jh);
    }

    /// Puts what finished passes showed into effect, and starts a pass for every family a probe
    /// can still settle
    pub(super) fn mtu_detection_task_routine(
        &self,
        _stop_token: StopToken,
        _last_ts: Timestamp,
        _cur_ts: Timestamp,
    ) -> EyreResult<()> {
        let mut changed = false;

        // Finished passes first, so a family they leave unsettled can start another
        let results = {
            let inner = self.inner.lock();
            inner
                .mtu_probe_results
                .as_ref()
                .map(|results| results.try_iter().collect::<Vec<_>>())
                .unwrap_or_default()
        };
        for result in results {
            // A pass that settles nothing leaves a detecting family at the safe size
            let verdict = probe_verdict(&result.outcomes).unwrap_or(FamilyMtuState::Unprobed);
            changed |= self.settle_family_mtu(result.address_type, result.generation, verdict);
        }

        let pending = self
            .inner
            .lock()
            .family_mtus
            .iter()
            .filter(|(_, family_mtu)| {
                !family_mtu.probing
                    && matches!(
                        family_mtu.state,
                        FamilyMtuState::Detecting | FamilyMtuState::Unprobed
                    )
            })
            .map(|(address_type, family_mtu)| (*address_type, family_mtu.clone()))
            .collect::<Vec<_>>();

        for (address_type, family_mtu) in pending {
            let (full_udp, _) = family_mtu.limits(address_type);
            let candidates = self
                .mtu_probe_candidates(address_type, full_udp, &family_mtu.tried)
                .into_iter()
                .take(MTU_PROBE_PEERS)
                .collect::<Vec<_>>();
            if candidates.is_empty() {
                // No peer to ask: a detecting family settles safe rather than hold up publication
                if family_mtu.state == FamilyMtuState::Detecting
                    && self.public_internet_waits_on_mtus()
                {
                    changed |= self.settle_family_mtu(
                        address_type,
                        family_mtu.generation,
                        FamilyMtuState::Unprobed,
                    );
                }
                continue;
            }
            self.start_family_probe(address_type, family_mtu.generation, candidates);
        }

        if changed {
            self.commit_family_mtus();
        }
        Ok(())
    }

    /// Peers not yet tried that can answer a probe over UDP in a family, those already
    /// advertising our full size first
    fn mtu_probe_candidates(
        &self,
        address_type: AddressType,
        full_udp: u16,
        tried: &BTreeSet<NodeId>,
    ) -> Vec<MtuProbeCandidate> {
        let routing_table = self.routing_table();
        let routing_domain = RoutingDomain::PublicInternet;
        let dial_info_filter = DialInfoFilter::all()
            .with_protocol_type(ProtocolType::UDP)
            .with_address_type(address_type);
        let advertises_mtus: RoutingTableEntryFilter =
            Box::new(|opt_snapshot: &Option<BucketEntrySnapshot>, _cur_ts| {
                opt_snapshot
                    .as_ref()
                    .and_then(|snapshot| snapshot.public_internet.peer_info.as_ref())
                    .is_some_and(|peer_info| !peer_info.node_info().receive_mtus().is_empty())
            });
        let filters = VecDeque::from([
            routing_table.make_inbound_dial_info_entry_filter(routing_domain, dial_info_filter),
            advertises_mtus,
        ]);
        let nodes = routing_table.find_fast_non_local_nodes_filtered(
            routing_domain,
            MTU_PROBE_CANDIDATES,
            filters,
        );

        let udp = LowLevelTransportType {
            low_level_protocol_type: LowLevelProtocolType::UDP,
            address_type,
        };
        let mut candidates = nodes
            .into_iter()
            .filter(|node| !tried.contains(&node.best_node_id()))
            .filter_map(|node| {
                let node_info = node.node_info(routing_domain)?;
                let dial_info = node_info
                    .dial_info_detail_list()
                    .iter()
                    .find(|did| {
                        !did.class.requires_signal()
                            && did.dial_info.matches_filter(&dial_info_filter)
                    })?
                    .dial_info
                    .clone();
                let at_full = node_info
                    .receive_mtus()
                    .get(udp)
                    .is_some_and(|mtu| mtu >= full_udp);
                Some((at_full, node, dial_info))
            })
            .collect::<Vec<_>>();
        candidates.sort_by_key(|(at_full, _, _)| !*at_full);
        candidates
            .into_iter()
            .map(|(_, node, dial_info)| MtuProbeCandidate {
                node,
                dial_info,
                size: full_udp,
            })
            .collect()
    }

    /// Sends one peer a full-size probe and a plain StatusQ together over a UDP dial info. A
    /// peer that says nothing within what its round trips predict settles nothing
    async fn mtu_probe(&self, candidate: MtuProbeCandidate) -> MtuProbeOutcome {
        let MtuProbeCandidate {
            node,
            dial_info,
            size,
        } = candidate;
        let rpc_processor = self.rpc_processor();
        let rpc_timeout_ms = self.config().internal().network.rpc.timeout_ms;
        let known_rtt_ms = node
            .peer_stats()
            .latency
            .map_or(0, |latency| latency.average.as_u64() / 1000);
        let first_word_ms = probe_grace_ms(rpc_timeout_ms, known_rtt_ms);
        let destination = Destination::dial_info(dial_info, node);
        // Only the datagram the answer arrived in says what the path carried
        let filled = size.saturating_sub(MTU_PROBE_FILL_SLACK);
        let answered_full = |res: &Result<StatusResult, RPCError>| {
            matches!(res, Ok(StatusResult::Sent(answer))
                if answer.opt_mtu_probe_datagram_len.is_some_and(|len| len >= filled))
        };
        // The probe went out and nothing came back
        let probe_lost = |res: &Result<StatusResult, RPCError>| {
            matches!(res, Ok(StatusResult::Failed(sdr))
                if sdr.unique_flow().is_some())
        };

        let probe = pin!(rpc_processor.rpc_call_status(destination.clone(), Some(size)));
        let plain = pin!(rpc_processor.rpc_call_status(destination, None));
        let Ok(first) = timeout(first_word_ms, select(probe, plain)).await else {
            return MtuProbeOutcome::Inconclusive;
        };
        match first {
            Either::Left((probe_res, plain)) => {
                if answered_full(&probe_res) {
                    return MtuProbeOutcome::Full;
                }
                let lost = probe_lost(&probe_res);
                match timeout(first_word_ms, plain).await {
                    Ok(Ok(StatusResult::Sent(_))) if lost => MtuProbeOutcome::Lost,
                    _ => MtuProbeOutcome::Inconclusive,
                }
            }
            Either::Right((plain_res, probe)) => {
                let Ok(StatusResult::Sent(plain_answer)) = plain_res else {
                    return match timeout(first_word_ms, probe).await {
                        Ok(probe_res) if answered_full(&probe_res) => MtuProbeOutcome::Full,
                        _ => MtuProbeOutcome::Inconclusive,
                    };
                };
                let rtt_ms = plain_answer.answer.answer_context.latency.as_u64() / 1000;
                let grace_ms = probe_grace_ms(rpc_timeout_ms, rtt_ms);
                match timeout(grace_ms, probe).await {
                    Ok(probe_res) if answered_full(&probe_res) => MtuProbeOutcome::Full,
                    Ok(probe_res) if probe_lost(&probe_res) => MtuProbeOutcome::Lost,
                    // The plain answer is in and the probe's never came within its grace
                    Err(_) => MtuProbeOutcome::Lost,
                    Ok(_) => MtuProbeOutcome::Inconclusive,
                }
            }
        }
    }

    /// Whether PublicInternet publication waits on nothing but our mtus
    fn public_internet_waits_on_mtus(&self) -> bool {
        matches!(
            self.routing_table()
                .get_specific_routing_domain_controller::<PublicInternetRoutingDomainController>()
                .state()
                .inbound_stage,
            RoutingDomainInboundStage::NeedsTransportMtus
        )
    }

    /// Hands a pass to the processor and records its peers as tried, unless the family started
    /// over meanwhile
    fn start_family_probe(
        &self,
        address_type: AddressType,
        generation: u64,
        candidates: Vec<MtuProbeCandidate>,
    ) {
        let mut inner = self.inner.lock();
        let Some(passes) = inner.mtu_probe_passes.clone() else {
            return;
        };
        let Some(family_mtu) = inner.family_mtus.get_mut(&address_type) else {
            return;
        };
        if family_mtu.generation != generation {
            return;
        }
        family_mtu.tried.extend(
            candidates
                .iter()
                .map(|candidate| candidate.node.best_node_id()),
        );
        if family_mtu.tried.len() > MTU_PROBE_TRIED_MAX {
            family_mtu.tried.clear();
        }
        veilid_log!(self debug "{} mtu: probing {} peers at {}", address_type, candidates.len(), candidates[0].size);
        let pass = MtuProbePass {
            address_type,
            generation,
            candidates,
        };
        family_mtu.probing = passes.send(pass).is_ok();
    }

    /// Records a verdict and ends the family's pass, unless the family started over meanwhile
    fn settle_family_mtu(
        &self,
        address_type: AddressType,
        generation: u64,
        verdict: FamilyMtuState,
    ) -> bool {
        let mut inner = self.inner.lock();
        let Some(family_mtu) = inner.family_mtus.get_mut(&address_type) else {
            return false;
        };
        if family_mtu.generation != generation {
            return false;
        }
        family_mtu.probing = false;
        if family_mtu.state == verdict {
            return false;
        }
        veilid_log!(self debug "{} mtu: {:?} -> {:?}", address_type, family_mtu.state, verdict);
        family_mtu.state = verdict;
        // A settled family probes no more, so the peers it tried no longer matter
        if verdict != FamilyMtuState::Unprobed {
            family_mtu.tried.clear();
        }
        true
    }

    /// Puts settled sizes into effect, then into both routing domains' peer info
    fn commit_family_mtus(&self) {
        self.apply_family_mtus();
        let receive_mtus = self.receive_mtus();
        let routing_table = self.routing_table();

        let public_internet = routing_table
            .get_specific_routing_domain_controller::<PublicInternetRoutingDomainController>();
        let mut editor = public_internet.edit();
        editor.set_receive_mtus(receive_mtus);
        editor.commit();
        public_internet.publish_peer_info();

        let local_network = routing_table
            .get_specific_routing_domain_controller::<LocalNetworkRoutingDomainController>();
        let mut editor = local_network.edit();
        editor.set_receive_mtus(receive_mtus);
        editor.commit();
        local_network.publish_peer_info();
    }
}
