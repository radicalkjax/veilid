use super::*;

impl_veilid_log_facility!("rtab");

/// What a round that found this node to be its only bootstrap was judged on. Config records do
/// not change, so only a new TXT answer or a change in our network can give another verdict.
#[derive(Clone, Copy, Debug)]
pub(super) struct BootstrapStandoff {
    /// Our published peer info and online state when the round started
    epoch: LossEpoch,
    /// When the earliest TXT answer the round used leaves the lookup cache
    opt_txt_expiration: Option<Timestamp>,
}

impl BootstrapStandoff {
    fn holds(&self, epoch: &LossEpoch, now: Timestamp) -> bool {
        self.epoch.same_network(epoch) && self.opt_txt_expiration.is_none_or(|exp| now < exp)
    }
}

impl PublicInternetRoutingDomainController {
    #[cfg_attr(feature = "instrument", instrument(level = "trace", skip_all, err, fields(__VEILID_LOG_KEY = self.log_key())))]
    pub async fn bootstrap_task_routine(
        &self,
        stop_token: StopToken,
        _last_ts: Timestamp,
        _cur_ts: Timestamp,
    ) -> EyreResult<()> {
        // Don't bother if the state changed and we don't need to bootstrap
        let nodes_needed = self.nodes_needed();
        if nodes_needed.needs_bootstrap.is_empty() {
            return Ok(());
        }
        let crypto_kinds = nodes_needed.needs_bootstrap;

        // Don't bother if nothing to bootstrap from is configured
        let routing_table = self.routing_table();
        let (bootstraps, has_config_records) = {
            let cfg = self.config();
            let rt = &cfg.network.routing_table;
            (rt.bootstrap.clone(), !rt.bootstrap_records.is_empty())
        };
        if bootstraps.is_empty() && !has_config_records {
            return Ok(());
        }

        // Stand off while this node is the only bootstrap it knows of
        let epoch = routing_table.loss_epoch(None);
        if self
            .bootstrap_standoff
            .lock()
            .is_some_and(|standoff| standoff.holds(&epoch, Timestamp::now()))
        {
            return Ok(());
        }

        // Reset the low water mark for this routing domain
        routing_table.refresh_summaries(RoutingDomain::PublicInternet.into());

        // See if we are specifying a direct dialinfo for bootstrap, if so use the direct mechanism
        // Otherwise treat them as txt names for the normal dns-based bootstrap mechanism
        let dial_info_converter = BootstrapDialInfoConverter::default();

        let mut bootstrap_dialinfos = Vec::<DialInfo>::new();
        let mut bootstrap_txt_names = Vec::<String>::new();
        for bootstrap in bootstraps {
            // A url's hostname resolves on the blocking pool, which a stop cannot interrupt
            let Ok(converted) = dial_info_converter
                .try_vec_from_url(&bootstrap)
                .timeout_at(stop_token.clone())
                .await
            else {
                return Ok(());
            };
            if let Ok(bootstrap_di_vec) = converted {
                for bootstrap_di in bootstrap_di_vec {
                    bootstrap_dialinfos.push(bootstrap_di);
                }
            } else if Resolver::supports_txt() {
                bootstrap_txt_names.push(bootstrap);
            } else {
                // No TXT resolver here, so the bootstrap host is asked directly instead
                for url in self.direct_bootstrap_urls(&bootstrap) {
                    let Ok(converted) = dial_info_converter
                        .try_vec_from_url(&url)
                        .timeout_at(stop_token.clone())
                        .await
                    else {
                        return Ok(());
                    };
                    match converted {
                        Ok(bootstrap_di_vec) => bootstrap_dialinfos.extend(bootstrap_di_vec),
                        Err(e) => {
                            veilid_log!(self debug "Could not resolve direct bootstrap {}: {}", url, e);
                        }
                    }
                }
            }
        }

        // Process bootstraps into peers lists
        let network_manager = self.network_manager();
        let txt_names = bootstrap_txt_names.clone();
        let mut unord = FuturesUnordered::<PinBoxFuture<EyreResult<BootstrapPeers>>>::new();

        // Records supplied by config need no lookup of any kind
        if has_config_records {
            unord.push(pin_dyn_future!(async {
                veilid_log!(self debug "Bootstrap with configured records");
                pin_future!(network_manager.config_bootstrap()).await
            }));
        }

        // Get a peer list from bootstraps to process
        for bsdi in bootstrap_dialinfos {
            // Direct bootstrap
            unord.push(pin_dyn_future!(async {
                let bsdi = bsdi;
                veilid_log!(self debug "Direct bootstrap with: {}", bsdi);
                pin_future!(network_manager.direct_bootstrap(bsdi)).await
            }));
        }
        for bstn in bootstrap_txt_names {
            // TXT bootstrap, then the host directly if the resolver gave no records in
            // time: a tethered phone's DNS relay answers A/AAAA but not TXT
            unord.push(pin_dyn_future!(async {
                let bstn = bstn;
                veilid_log!(self debug "TXT bootstrap with: {}", bstn);
                let txt_timeout = self.routing_table().timings().bootstrap_txt_timeout;
                match pin_future!(network_manager.txt_bootstrap(bstn.clone()))
                    .timeout_duration(txt_timeout)
                    .await
                {
                    Ok(Ok(peers)) if !peers.records.is_empty() => Ok(peers),
                    _ => {
                        veilid_log!(self debug "TXT bootstrap gave nothing, trying direct bootstrap on: {}", bstn);
                        self.direct_bootstrap_by_name(&bstn).await
                    }
                }
            }));
        }

        let mut bootstrapped_peer_id_set = HashSet::<NodeId>::new();
        let mut bootstrapped_peers = vec![];
        let mut bootstrapped_records = vec![];
        let mut bootstrapped_record_strings = vec![];
        let mut own_node_seen = false;
        loop {
            match unord.next().timeout_at(stop_token.clone()).await {
                Ok(Some(res)) => match res {
                    Ok(BootstrapPeers {
                        peers,
                        records,
                        record_strings,
                    }) => {
                        bootstrapped_records.extend(records);
                        bootstrapped_record_strings.extend(record_strings);
                        for peer in peers {
                            let peer_node_ids = peer.node_ids();
                            let routing_domain = peer.routing_domain();
                            if routing_domain != RoutingDomain::PublicInternet {
                                continue;
                            }
                            if !peer_node_ids
                                .iter()
                                .any(|x| bootstrapped_peer_id_set.contains(x))
                            {
                                if routing_table.matches_own_node_id(peer_node_ids) {
                                    veilid_log!(self debug "Ignoring own node in bootstrap list");
                                    own_node_seen = true;
                                } else {
                                    for nid in peer.node_ids().iter().cloned() {
                                        bootstrapped_peer_id_set.insert(nid);
                                    }
                                    bootstrapped_peers.push(peer);
                                }
                            }
                        }
                    }
                    Err(e) => {
                        veilid_log!(self debug "Bootstrap error: {}", e);
                    }
                },
                Ok(None) => {
                    // Done
                    break;
                }
                Err(_) => {
                    // Cancelled
                    return Ok(());
                }
            }
        }

        self.read_dyn().clear_bootstrap_peers();

        // Every string received is cached, including versions this build cannot parse, so a node
        // can pass on what it was told. A receiving client re-verifies signatures against its own
        // keys, so relaying a string it does not trust costs response size and nothing else.
        let bootstrap_cache =
            BootstrapCache::new(bootstrapped_record_strings, bootstrapped_records);

        // A round that resolved anything decides whether this node is one of its own bootstraps
        if own_node_seen || !bootstrap_cache.records().is_empty() {
            let was_own_bootstrap = self.own_bootstrap.swap(own_node_seen, Ordering::AcqRel);
            if own_node_seen && !was_own_bootstrap {
                veilid_log!(self info "This node is one of its own bootstraps and answers direct bootstrap requests");
            }
        }

        // Only this node among the bootstraps: nothing to bootstrap from until other nodes connect
        if own_node_seen && bootstrapped_peers.is_empty() {
            self.record_bootstrap_cache(bootstrap_cache);
            // An unanswered TXT name leaves the verdict open, so the next round asks again
            let opt_txt_expiration = (!txt_names.is_empty()).then(|| {
                network_manager
                    .txt_answers_expiration(&txt_names)
                    .unwrap_or_else(Timestamp::now)
            });
            let standoff = BootstrapStandoff {
                epoch,
                opt_txt_expiration,
            };
            if self.bootstrap_standoff.lock().replace(standoff).is_none() {
                veilid_log!(self info "This node is its only bootstrap, waiting for other nodes to connect");
            }
            return Ok(());
        }
        *self.bootstrap_standoff.lock() = None;

        if self
            .unlocked_inner
            .bootstrap_wait_detector
            .fetch_update(|opt_ts| {
                if opt_ts.is_none() {
                    Some(Some(Timestamp::now()))
                } else {
                    None
                }
            })
            .is_ok()
        {
            veilid_log!(self info "Waiting for bootstrap...");
        }
        let valid_bootstraps = self
            .bootstrap_with_peer_list(bootstrapped_peers.clone(), crypto_kinds.clone(), stop_token)
            .await?;

        self.record_bootstrap_cache(bootstrap_cache);

        if valid_bootstraps.is_empty() {
            veilid_log!(self debug "No external bootstrap peers");

            // Leave bootstrap wait detector set so we don't spam logs
            return Ok(());
        }
        veilid_log!(self debug "External bootstrap peers: {:#?}", valid_bootstraps);

        // Get all node ids that have a peerinfo in this routing domain
        let mut rd_peer_ids = BTreeSet::new();
        for peer in bootstrapped_peers.iter() {
            if peer.routing_domain() == RoutingDomain::PublicInternet {
                for nid in peer.node_ids().iter().cloned() {
                    rd_peer_ids.insert(nid);
                }
            }
        }

        // Add valid bootstrap peers to routing domain
        for bootstrap_peer in valid_bootstraps.iter().cloned() {
            let mut add = false;
            if let Some(pi) = bootstrap_peer.get_peer_info(RoutingDomain::PublicInternet) {
                for nid in pi.node_ids().iter() {
                    if rd_peer_ids.contains(nid) {
                        add = true;
                        break;
                    }
                }
            }
            if add {
                self.read_dyn().add_bootstrap_peer(bootstrap_peer);
            }
        }
        let routing_table = self.routing_table();
        routing_table.flush(SaveReason::Bootstrap).await;

        // Reset the low water mark for this routing domain
        routing_table.refresh_summaries(RoutingDomain::PublicInternet.into());

        if let Some(old_ts) = self.unlocked_inner.bootstrap_wait_detector.swap(None) {
            let end_ts = Timestamp::now();
            let duration = end_ts.duration_since(old_ts);
            veilid_log!(self info "Bootstrap finished in {:#} via {}", duration, valid_bootstraps.iter().map(|v| v.to_string()).collect::<Vec<_>>().join(","));
        }

        Ok(())
    }

    /// Direct bootstrap from every address a bootstrap name resolves to, every
    /// protocol at once, results merged
    async fn direct_bootstrap_by_name(&self, hostname: &str) -> EyreResult<BootstrapPeers> {
        let dial_info_converter = BootstrapDialInfoConverter::default();
        let mut unord = FuturesUnordered::new();
        for url in self.direct_bootstrap_urls(hostname) {
            match dial_info_converter.try_vec_from_url(&url).await {
                Ok(dial_infos) => {
                    for dial_info in dial_infos {
                        let registry = self.registry();
                        unord.push(async move {
                            let network_manager = registry.network_manager();
                            let res =
                                pin_future!(network_manager.direct_bootstrap(dial_info.clone()))
                                    .await;
                            (dial_info, res)
                        });
                    }
                }
                Err(e) => {
                    veilid_log!(self debug "Could not resolve direct bootstrap {}: {}", url, e);
                }
            }
        }
        let mut out = BootstrapPeers::default();
        while let Some((dial_info, res)) = unord.next().await {
            match res {
                Ok(peers) => {
                    out.peers.extend(peers.peers);
                    out.records.extend(peers.records);
                    out.record_strings.extend(peers.record_strings);
                }
                Err(e) => {
                    veilid_log!(self debug "Direct bootstrap {} failed: {}", dial_info, e);
                }
            }
        }
        Ok(out)
    }

    /// Direct bootstrap urls for a TXT bootstrap name, one per enabled outbound protocol
    fn direct_bootstrap_urls(&self, hostname: &str) -> Vec<String> {
        let protocol = &self.config().network.protocol;
        let mut urls = Vec::new();
        if protocol.udp.enabled {
            urls.push(format!("udp://{}:{}", hostname, BOOTSTRAP_DIRECT_PORT));
        }
        if protocol.tcp.connect {
            urls.push(format!("tcp://{}:{}", hostname, BOOTSTRAP_DIRECT_PORT));
        }
        urls
    }

    /// Record what this bootstrap produced, so a restart can serve it without a lookup
    fn record_bootstrap_cache(&self, bootstrap_cache: BootstrapCache) {
        veilid_log!(self debug "Bootstrap records: {:?}", bootstrap_cache.record_strings());
        self.read_dyn().record_bootstrap_cache(bootstrap_cache);
    }

    pub fn bootstrap_with_peer(
        &self,
        crypto_kinds: Vec<CryptoKind>,
        peer_info: Arc<PeerInfo>,
        existing_node_ids: Arc<HashSet<NodeId>>,
        unord: &FuturesUnordered<PinBoxFutureStatic<Option<NodeRef>>>,
    ) {
        veilid_log!(self trace
            "--- bootstrapping {} with {:?}",
            peer_info.node_ids(),
            peer_info.node_info().dial_info_detail_list()
        );

        let routing_domain = peer_info.routing_domain();
        let routing_table = self.routing_table();

        let nr = match routing_table.register_node_with_peer_info(
            Arc::new(UnverifiedPeerInfo::from_verified(&peer_info)),
            true,
        ) {
            Ok(nr) => nr,
            Err(e) => {
                veilid_log!(self error "failed to register bootstrap peer info: {}", e);
                return;
            }
        };

        // Add this our futures to process in parallel
        for crypto_kind in crypto_kinds {
            // Bootstrap this crypto kind
            let nr = nr.unfiltered();
            let existing_node_ids = existing_node_ids.clone();
            unord.push(Box::pin(
                async move {
                    let network_manager = nr.network_manager();
                    let routing_table = nr.routing_table();

                    // Get what contact method would be used for contacting the bootstrap
                    let bsdi = match network_manager.get_node_contact_method(
                        nr.sequencing_filtered(Sequencing::PreferOrdered),
                        ContactMethodIntent::Advisory,
                    ) {
                        Ok(NodeContactMethodResult::Resolved(ncm)) => {
                            let Some(di) = ncm.direct_dial_info() else {
                                veilid_log!(nr debug "invalid contact method for bootstrap, ignoring peer: {}", NodeContactMethodResult::Resolved(ncm));
                                return None;
                            };
                            di
                        }
                        Ok(v) => {
                            veilid_log!(nr debug "invalid contact method for bootstrap, ignoring peer: {}", v);
                            return None;
                        }
                        Err(e) => {
                            veilid_log!(nr warn "unable to bootstrap: {}", e);
                            return None;
                        }
                    };

                    // Need VALID signed peer info, so ask bootstrap to find_node of itself
                    // which will ensure it has the bootstrap's signed peer info as part of the response
                    let _ = Box::pin(routing_table.find_nodes_close_to_node_ref(crypto_kind, nr.sequencing_filtered(Sequencing::PreferOrdered), vec![])).await;

                    // Ensure we got the signed peer info
                    if !nr.peer_info_has_valid_signature(routing_domain) {
                        veilid_log!(nr debug "bootstrap server is not responding for dialinfo: {}", bsdi);
                        None
                    } else {
                        // otherwise this bootstrap is valid, lets ask it to find ourselves now
                        // We should prefer nodes that have relaying, signaling, routing and validation of dial info
                        // for our initial nodes in our routing table because we need these things in order to
                        // properly attach to the network

                        // Find nodes close to ourself
                        let close_node_refs = network_result_value_or_log!(nr match pin_future!(
                                routing_table.find_new_nodes_close_to_self(
                                    crypto_kind,
                                    &existing_node_ids,
                                    nr.routing_domain_filtered(RoutingDomain::PublicInternet).with_sequencing(Sequencing::PreferOrdered),
                                    CONNECTIVITY_CAPABILITIES.to_vec())
                                ).await {
                            Err(e) => {
                                veilid_log!(nr debug "failed to find nodes close to self: {}", e);
                                NetworkResult::value(vec![])
                            }
                            Ok(v) => v,
                        } => {
                            vec![]
                        });
                        veilid_log!(nr debug target:"network_result", "bootstrap: found {} nodes close to self for {} in {} with {:?}", close_node_refs.len(), crypto_kind, RoutingDomain::PublicInternet, CONNECTIVITY_CAPABILITIES);

                        // Now find nodes widely across the network
                        let wide_node_refs = match pin_future!(routing_table.find_new_nodes_wide(
                                crypto_kind,
                                &existing_node_ids,
                                RoutingDomain::PublicInternet,
                                None,
                                CONNECTIVITY_CAPABILITIES.to_vec())
                            ).await {
                            Err(e) => {
                                veilid_log!(nr debug "failed to find nodes wide: {}", e);
                                vec![]
                            }
                            Ok(v) => v,
                        };

                        veilid_log!(nr debug target:"network_result", "bootstrap: found {} nodes wide for {} in {} with {:?}", wide_node_refs.len(), crypto_kind, RoutingDomain::PublicInternet, CONNECTIVITY_CAPABILITIES);

                        veilid_log!(nr debug "Bootstrap of {} successful via {} with {} close nodes and {} wide nodes", crypto_kind, nr, close_node_refs.len(), wide_node_refs.len());
                        Some(nr)
                    }
                }
                .instrument(Span::current()),
            ));
        }
    }

    /// Takes in a list of bootstrap peer info, and attempts bootstrapping
    /// A list of valid bootstrap peer noderefs is returned
    #[cfg_attr(feature = "instrument", instrument(level = "trace", skip(self), err, fields(__VEILID_LOG_KEY = self.log_key())))]
    pub async fn bootstrap_with_peer_list(
        &self,
        bootstrap_peers: Vec<Arc<PeerInfo>>,
        crypto_kinds: Vec<CryptoKind>,
        stop_token: StopToken,
    ) -> EyreResult<Vec<NodeRef>> {
        if bootstrap_peers.is_empty() {
            veilid_log!(self debug "No peers suitable for bootstrap");
            return Ok(vec![]);
        }

        veilid_log!(self debug "Bootstrap peers: {:?}", &bootstrap_peers);

        // Snapshot once and extract existing node ids for dedup filtering
        let existing_node_ids = Arc::new(
            self.routing_table()
                .snapshot_entries(Timestamp::now(), BucketEntryState::Initial)
                .existing_node_ids(None),
        );

        // Run all bootstrap operations concurrently
        let mut unord = FuturesUnordered::<PinBoxFutureStatic<Option<NodeRef>>>::new();
        for peer in bootstrap_peers {
            let mut peer_crypto_kinds = vec![];
            for ck in crypto_kinds.iter().copied() {
                if peer.node_ids().get(ck).is_some() {
                    peer_crypto_kinds.push(ck);
                }
            }
            if peer_crypto_kinds.is_empty() {
                continue;
            }

            veilid_log!(self debug "Attempting bootstrap: {}", peer.node_ids());
            self.bootstrap_with_peer(peer_crypto_kinds, peer, existing_node_ids.clone(), &unord);
        }

        // Wait for all bootstrap operations to complete before we complete the singlefuture
        let mut valid_bootstraps = vec![];
        while let Ok(Some(res)) = unord.next().timeout_at(stop_token.clone()).await {
            if let Some(valid_bootstrap) = res {
                valid_bootstraps.push(valid_bootstrap);
            }
        }
        Ok(valid_bootstraps)
    }
}
