use super::*;

impl_veilid_log_facility!("rtab");

/// Interval between loopback keepalive ticks on allocated routes; suppressed when real traffic
/// or a recent route test has already bumped `last_known_valid_ts` within the same window.
const SR_LOOPBACK_KEEPALIVE_INTERVAL: TimestampDuration = TimestampDuration::new_secs(10);

/// The one ordering a pool route carries; None for legacy routes that carried both
fn single_ordering(orderings: SequenceOrderingSet) -> Option<SequenceOrdering> {
    (orderings.len() == 1)
        .then(|| orderings.iter().next())
        .flatten()
}

impl PublicInternetRoutingDomainController {
    /// Fastest routes sort
    fn route_sort_latency_fn(
        a: &(AllocatedRouteSetId, u64),
        b: &(AllocatedRouteSetId, u64),
    ) -> cmp::Ordering {
        let mut al = a.1;
        let mut bl = b.1;
        // Treat zero latency as uncalculated
        if al == 0 {
            al = u64::MAX;
        }
        if bl == 0 {
            bl = u64::MAX;
        }
        // Less is better
        let c = al.cmp(&bl);
        if c != cmp::Ordering::Equal {
            return c;
        }

        // Otherwise, just sort by route id
        a.0.cmp(&b.0)
    }

    /// Determine if we have enough tested automatic safety routes to perform routing operations
    pub(in crate::routing_table) fn safety_routes_ready(&self, outbound_was_ready: bool) -> bool {
        let routing_table = self.routing_table();
        let rss = routing_table.route_spec_store();

        let mut safe_route_count = 0;
        let mut unsafe_route_count = 0;

        rss.list_allocated_routes(|_k, v| {
            let is_known_valid = v.with_stats(|stats| stats.last_known_valid_ts().is_some());
            // Ordered routes gate readiness; unordered ones are best effort
            let is_ordered = single_ordering(v.orderings()) == Some(SequenceOrdering::Ordered);
            // A dead route awaiting release can carry nothing, though it stays listed while locked
            if is_known_valid && v.is_automatic() && is_ordered && !v.is_marked_for_release() {
                if v.hop_count() == rss.get_default_route_hop_count_safe() {
                    safe_route_count += 1;
                } else if v.hop_count() == rss.get_default_route_hop_count_unsafe() {
                    unsafe_route_count += 1;
                }
            }
            Option::<()>::None
        });

        safety_routes_ready_for(outbound_was_ready, safe_route_count, unsafe_route_count)
    }

    /// Get the list of routes to test and drop
    ///
    /// Allocated routes to test:
    /// * if a route 'needs_testing'
    ///   . all published allocated routes
    ///   . routes that have never been tested
    ///   . the fastest 0..N automatic safety routes of safe default length
    ///   . the fastest 0..N automatic safety routes of unsafe default length
    ///
    /// Routes to drop:
    /// * if a route 'needs_testing'
    ///   . the remaining automatic safety routes
    ///   . the rest of the allocated unpublished routes
    ///
    /// If a route doesn't 'need_testing', then we neither test nor drop it
    #[cfg_attr(feature = "instrument", instrument(level = "trace", skip(self), ret, fields(__VEILID_LOG_KEY = self.log_key())))]
    fn get_allocated_routes_to_test(&self, cur_ts: Timestamp) -> Vec<AllocatedRouteSetId> {
        let routing_table = self.routing_table();
        let rss = routing_table.route_spec_store();
        let default_route_hop_count_safe = rss.get_default_route_hop_count_safe();
        let default_route_hop_count_unsafe = rss.get_default_route_hop_count_unsafe();

        let mut must_test_routes = Vec::<AllocatedRouteSetId>::new();
        // Pool routes by (hop count, ordering) cell, with latency for the speed sort
        let mut pool =
            BTreeMap::<(usize, SequenceOrdering), Vec<(AllocatedRouteSetId, u64)>>::new();
        let mut expired_routes = Vec::<AllocatedRouteSetId>::new();
        rss.list_allocated_routes(|k, v| {
            // A route a test confirmed dead is marked for release; release it once its refcount
            // drains to zero (in-flight RPCs finished). The background pool reallocates a
            // replacement; selection already excludes it the whole time.
            if v.is_marked_for_release() {
                if !v.is_locked() {
                    expired_routes.push(k.clone());
                }
                return Option::<()>::None;
            }
            let route_orderings = v.orderings();
            // Pool routes carry one ordering each; a legacy route that carried both makes way
            let opt_ordering = single_ordering(route_orderings);
            if v.is_automatic() && opt_ordering.is_none() {
                expired_routes.push(k.clone());
                return Option::<()>::None;
            }
            v.with_stats(|stats| {
                // Ignore routes that don't need or want testing
                if !stats.needs_testing(route_orderings, cur_ts) {
                    return Option::<()>::None;
                }
                // If this has been published, always test if we need it
                // Also if the route has never been tested, test it at least once
                if v.is_published() || stats.last_known_valid_ts().is_none() {
                    must_test_routes.push(k.clone());
                }
                // Pool routes of a default length are kept alive per cell
                else if v.is_automatic()
                    && (v.hop_count() == default_route_hop_count_safe
                        || v.hop_count() == default_route_hop_count_unsafe)
                {
                    if let Some(ordering) = opt_ordering {
                        pool.entry((v.hop_count(), ordering))
                            .or_default()
                            .push((k.clone(), stats.latency.average.as_u64()));
                    }
                }
                // Else tear down only automatic (pool-managed) routes; manual routes are caller-owned
                else if v.is_automatic() {
                    expired_routes.push(k.clone());
                }
                Option::<()>::None
            })
        });

        // Per cell: keep and test the fastest N, kill off the rest rather than testing them
        for routes in pool.values_mut() {
            routes.sort_unstable_by(Self::route_sort_latency_fn);
            let keep = usize::min(DESIRED_BACKGROUND_SAFETY_ROUTE_COUNT, routes.len());
            for route in routes.iter().take(keep) {
                must_test_routes.push(route.0.clone());
            }
            for route in routes.iter().skip(keep) {
                expired_routes.push(route.0.clone());
            }
        }

        // Process dead routes
        for r in expired_routes {
            veilid_log!(self debug "Expired route: {}", r);
            rss.release_route(r.into());
        }

        // return routes to test
        must_test_routes
    }

    /// Build the keepalive schedule for allocated routes: one request per route, with the
    /// orderings whose last loopback keepalive is older than SR_LOOPBACK_KEEPALIVE_INTERVAL.
    /// Per-ordering keying ensures a TCP-busy route doesn't starve UDP keepalives or vice versa.
    fn get_allocated_routes_to_loopback_keepalive(
        &self,
        cur_ts: Timestamp,
    ) -> Vec<RoutePingValidationRequest> {
        let routing_table = self.routing_table();
        let rss = routing_table.route_spec_store();

        let mut out = Vec::new();
        rss.list_allocated_routes(|k, v| {
            // Don't keepalive-test a route marked for release; it's slated for release and a
            // test would re-hold it and defer that.
            if v.is_marked_for_release() {
                return Option::<()>::None;
            }
            let route_orderings = v.orderings();
            let mut needed = SequenceOrderingSet::empty();
            v.with_stats(|stats| {
                for ordering in route_orderings {
                    let stale = stats
                        .last_loopback_keepalive_ts(ordering)
                        .map(|ts| cur_ts.duration_since(ts) >= SR_LOOPBACK_KEEPALIVE_INTERVAL)
                        .unwrap_or(true);
                    if stale {
                        needed.insert(ordering);
                    }
                }
            });
            if !needed.is_empty() {
                out.push(RoutePingValidationRequest {
                    route_id: k.clone(),
                    orderings: needed,
                    purpose: RoutePingValidationPurpose::Keepalive,
                });
            }
            Option::<()>::None
        });
        out
    }

    /// Release the automatic unpublished routes and run the management routine now so the
    /// pool refills without waiting for its tick. Returns (released, allocated).
    #[cfg(feature = "debug-api")]
    pub(crate) async fn reallocate_safety_routes(&self) -> EyreResult<(usize, usize)> {
        let routing_table = self.routing_table();
        let rss = routing_table.route_spec_store();
        let pool = rss.list_allocated_routes(|k, v| {
            (v.is_automatic() && !v.is_published()).then(|| k.clone())
        });
        let released = pool.len();
        for id in pool {
            rss.release_route(id.into());
        }
        // A tick already in flight is skipped by try_tick_now; retry until one of ours starts.
        // The routine runs detached, so wait for the pool count to settle rather than for it.
        for _ in 0..20 {
            if self.private_route_management_task.try_tick_now().await? {
                break;
            }
            sleep(100).await;
        }
        let pool_size = || {
            rss.list_allocated_routes(|_k, v| (v.is_automatic() && !v.is_published()).then_some(()))
                .len()
        };
        let mut allocated = pool_size();
        let mut settled = 0;
        for _ in 0..100 {
            sleep(100).await;
            let now = pool_size();
            settled = if now == allocated && now > 0 {
                settled + 1
            } else {
                0
            };
            allocated = now;
            if settled >= 5 {
                break;
            }
        }
        Ok((released, allocated))
    }

    /// Keep private routes assigned and accessible
    #[cfg_attr(
        feature = "instrument",
        instrument(level = "trace", skip(self, _stop_token), err, fields(__VEILID_LOG_KEY = self.log_key()))
    )]
    pub async fn private_route_management_task_routine(
        &self,
        _stop_token: StopToken,
        _last_ts: Timestamp,
        cur_ts: Timestamp,
    ) -> EyreResult<()> {
        let routing_table = self.routing_table();
        let rss = routing_table.route_spec_store();

        // Get locally allocated routes needing testing and enqueue them
        let routes_needing_testing = self.get_allocated_routes_to_test(cur_ts);
        if !routes_needing_testing.is_empty() {
            #[cfg(feature = "verbose-tracing")]
            veilid_log!(self debug "Enqueuing {} allocated route tests", routes_needing_testing.len());
            routing_table
                .enqueue_route_tests(
                    cur_ts,
                    routes_needing_testing.into_iter().map(Into::into).collect(),
                    0,
                )
                .await;
        }

        // Retest routes whose failures were deferred during blame suppression, now that it lifted
        if !routing_table
            .network_manager()
            .online_detector()
            .blame_suppressed(RoutingDomain::PublicInternet)
        {
            let deferred_routes = routing_table.take_deferred_route_tests();
            if !deferred_routes.is_empty() {
                veilid_log!(self debug "Retesting {} routes with deferred failures", deferred_routes.len());
                routing_table
                    .enqueue_route_tests(cur_ts, deferred_routes, 0)
                    .await;
            }
        }

        // Get locally allocated routes needing loopback keepalive and enqueue them
        let routes_needing_keepalive = self.get_allocated_routes_to_loopback_keepalive(cur_ts);
        if !routes_needing_keepalive.is_empty() {
            #[cfg(feature = "verbose-tracing")]
            veilid_log!(self debug "Enqueuing loopback keepalives for {} routes", routes_needing_keepalive.len());
            routing_table
                .enqueue_route_loopback_keepalives(cur_ts, routes_needing_keepalive, 0)
                .await;
        }

        // Ensure we have a minimum of N allocated local, unpublished routes per (hop count, ordering)
        // cell, with all our supported crypto kinds: the default length for safe destinations and
        // twice that for direct ones, each for ordered and for unordered traffic.
        let default_route_hop_count_safe = rss.get_default_route_hop_count_safe();
        let default_route_hop_count_unsafe = rss.get_default_route_hop_count_unsafe();

        let mut pool_counts = BTreeMap::<(usize, SequenceOrdering), usize>::new();
        rss.list_allocated_routes(|_k, v| {
            if !v.is_published() && v.is_automatic() {
                if let Some(ordering) = single_ordering(v.orderings()) {
                    *pool_counts.entry((v.hop_count(), ordering)).or_default() += 1;
                }
            }
            Option::<()>::None
        });

        // Ordered cells first (they gate readiness), safe and unsafe lengths interleaved so the
        // first batch of parallel tests covers both lengths
        let network_manager = routing_table.network_manager();
        let transport_stats = network_manager.transport_stats();
        let mut newly_allocated_routes = Vec::new();
        for ordering in [SequenceOrdering::Ordered, SequenceOrdering::Unordered] {
            // No outbound protocol carries it; PreferUnordered traffic rides the ordered routes
            if !transport_stats.ordering_is_available(RoutingDomain::PublicInternet, ordering) {
                continue;
            }
            let mut newly_safe_routes = Vec::new();
            let mut newly_unsafe_routes = Vec::new();
            for (hop_count, is_safe) in [
                (default_route_hop_count_safe, true),
                (default_route_hop_count_unsafe, false),
            ] {
                let route_count = pool_counts
                    .get(&(hop_count, ordering))
                    .copied()
                    .unwrap_or(0);
                if route_count >= DESIRED_BACKGROUND_SAFETY_ROUTE_COUNT {
                    continue;
                }
                let routes_to_allocate = DESIRED_BACKGROUND_SAFETY_ROUTE_COUNT - route_count;

                // Parameters here must be the most inclusive route allocation spec
                // These will be used by test_remote_route as well
                let params = AllocateRouteParams {
                    crypto_kinds: VALID_CRYPTO_KINDS.to_vec(),
                    hop_count,
                    stability: Stability::Reliable,
                    sequencing: ordering.minimum_sequencing(),
                    directions: DirectionSet::all(),
                    avoid_nodes: Vec::new(),
                    automatic: true,
                };
                for _n in 0..routes_to_allocate {
                    match rss.allocate_route(params.clone()).await {
                        Err(VeilidAPIError::TryAgain { message }) => {
                            veilid_log!(self debug "Route allocation unavailable ({:#} {} hops): {}", ordering, hop_count, message);
                            break;
                        }
                        Err(e) => return Err(e.into()),
                        Ok(v) => {
                            if is_safe {
                                newly_safe_routes.push(v.route_id);
                            } else {
                                newly_unsafe_routes.push(v.route_id);
                            }
                        }
                    }
                }
            }
            let max_len = newly_safe_routes.len().max(newly_unsafe_routes.len());
            for i in 0..max_len {
                if let Some(r) = newly_safe_routes.get(i) {
                    newly_allocated_routes.push(r.clone());
                }
                if let Some(r) = newly_unsafe_routes.get(i) {
                    newly_allocated_routes.push(r.clone());
                }
            }
        }

        // Enqueue tests for newly allocated routes
        if !newly_allocated_routes.is_empty() {
            #[cfg(feature = "verbose-tracing")]
            veilid_log!(self debug "Enqueuing {} newly allocated route tests", newly_allocated_routes.len());
            routing_table
                .enqueue_route_tests(
                    cur_ts,
                    newly_allocated_routes.into_iter().map(Into::into).collect(),
                    0,
                )
                .await;
        }

        // Enqueue remote route tests at slightly lower priority; the store holds only routes the
        // app imported, since a reply route is good for its one answer
        let remote_routes_needing_testing = rss.list_remote_routes(|k, v| {
            v.with_stats(|stats| stats.remote_needs_testing(cur_ts))
                .then(|| k.clone())
        });
        if !remote_routes_needing_testing.is_empty() {
            #[cfg(feature = "verbose-tracing")]
            veilid_log!(self debug "Enqueuing {} remote route tests", remote_routes_needing_testing.len());
            routing_table
                .enqueue_route_tests(
                    cur_ts,
                    remote_routes_needing_testing
                        .into_iter()
                        .map(Into::into)
                        .collect(),
                    10,
                )
                .await;
        }

        rss.log_route_test_summary(cur_ts);

        // Send update (also may send updates for released routes done by other parts of the program)
        rss.send_route_update();

        Ok(())
    }
}
