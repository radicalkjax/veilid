use super::*;

impl_veilid_log_facility!("rtab");

/// Protocols that carry a given sequence ordering
// XXX: WebTransport will offer both orderings over UDP; derive ordering support from
// dialinfo rather than protocol when it lands
fn protocols_for_ordering(ordering: SequenceOrdering) -> ProtocolTypeSet {
    let mut protocols = ProtocolTypeSet::new();
    for p in ProtocolTypeSet::all().iter() {
        if p.sequence_ordering() == ordering {
            protocols.insert(p);
        }
    }
    protocols
}

impl RoutingTable {
    /// Recompute entries' contact-method viability when a peerinfo changes on
    /// either side; the per-entry basis check skips unchanged edges. Applies
    /// evidence retention pruning on the same edge.
    #[cfg_attr(feature = "instrument", instrument(level = "trace", skip(self), err, fields(__VEILID_LOG_KEY = self.log_key())))]
    pub fn update_cm_viability_task_routine(
        &self,
        _stop_token: StopToken,
        _last_ts: Timestamp,
        _cur_ts: Timestamp,
    ) -> EyreResult<()> {
        // Consume the flag first; changes landing during this pass re-set it
        self.take_cm_viability_dirty();

        struct DomainContext {
            routing_domain: RoutingDomain,
            peer_a: Arc<PeerInfo>,
            peer_a_published: bool,
        }
        let domain_contexts: Vec<DomainContext> =
            [RoutingDomain::PublicInternet, RoutingDomain::LocalNetwork]
                .into_iter()
                .map(|routing_domain| {
                    let peer_a = self.get_current_peer_info(routing_domain);
                    let peer_a_published = self
                        .get_published_peer_info(routing_domain)
                        .map(|p| p.node_info().timestamp())
                        == Some(peer_a.node_info().timestamp());
                    DomainContext {
                        routing_domain,
                        peer_a,
                        peer_a_published,
                    }
                })
                .collect();

        let all_entries: Vec<Arc<BucketEntry>> = self.inner.read().all_entries.iter().collect();
        for entry in all_entries {
            // Snapshot target peer infos and the basis the current cache was computed against
            let (peer_bs, old_basis) = entry.with(|e| {
                let peer_bs: Vec<(&DomainContext, Arc<PeerInfo>)> = domain_contexts
                    .iter()
                    .filter_map(|dc| e.get_peer_info(dc.routing_domain).map(|pi| (dc, pi)))
                    .collect();
                let old_basis = e.cm_viability().map(|v| v.basis.clone());
                (peer_bs, old_basis)
            });

            if peer_bs.is_empty() {
                entry.with_mut(|e| e.set_cm_viability(None));
                continue;
            }

            let new_basis: BTreeMap<RoutingDomain, (Timestamp, Timestamp)> = peer_bs
                .iter()
                .map(|(dc, peer_b)| {
                    (
                        dc.routing_domain,
                        (
                            dc.peer_a.node_info().timestamp(),
                            peer_b.node_info().timestamp(),
                        ),
                    )
                })
                .collect();
            if old_basis.as_ref() == Some(&new_basis) {
                continue;
            }

            // Compute contact methods per ordering, merged across routing domains
            let mut per_ordering = BTreeMap::<SequenceOrdering, OrderingReachability>::new();
            let mut all_cms = Vec::<ContactMethod>::new();
            for (dc, peer_b) in &peer_bs {
                for ordering in [SequenceOrdering::Ordered, SequenceOrdering::Unordered] {
                    let protocols = protocols_for_ordering(ordering);
                    let dial_info_filter = DialInfoFilter::all()
                        .with_protocol_type_set(
                            protocols & dc.peer_a.node_info().outbound_protocols(),
                        )
                        .with_address_type_set(dc.peer_a.node_info().address_types());
                    let cms = self.get_contact_methods(
                        dc.routing_domain,
                        ContactMethodRequest {
                            peer_a: dc.peer_a.clone(),
                            peer_a_published: dc.peer_a_published,
                            peer_b: peer_b.clone(),
                            dial_info_filter,
                            sequencing: Sequencing::PreferUnordered,
                        },
                    );
                    if cms.is_empty() {
                        continue;
                    }
                    let reach = per_ordering.entry(ordering).or_default();
                    for cm in &cms {
                        match cm {
                            ContactMethod::Direct { target_di } => {
                                reach
                                    .direct_transports
                                    .insert(TransportType::from(target_di));
                            }
                            _ => {
                                reach.has_indirect = true;
                            }
                        }
                    }
                    all_cms.extend(cms);
                }
            }

            entry.with_mut(|e| {
                // The edge changed; keep evidence only for contact methods still offered
                if old_basis.is_some() {
                    e.prune_evidence_to_contact_methods(&all_cms);
                }
                e.set_cm_viability(Some(CMViability {
                    per_ordering,
                    basis: new_basis,
                }));
            });
        }
        Ok(())
    }
}
