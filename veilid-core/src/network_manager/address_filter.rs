use super::*;
use alloc::collections::btree_map::Entry;

impl_veilid_log_facility!("net");

const PUNISHMENT_DURATION_MIN: usize = 60;
const MAX_PUNISHMENTS_BY_NODE_ID: usize = 65536;
const MAX_ENVELOPE_INFO_BY_NODE_ID: usize = 16384;

#[derive(ThisError, Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum AddConnectionError {
    #[error("Count exceeded")]
    CountExceeded,
    #[error("Rate exceeded")]
    RateExceeded,
    #[error("Address is punished")]
    Punished,
}

#[derive(ThisError, Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum TimestampError {
    #[error("Timestamp too far behind")]
    TooFarBehind {
        local_timestamp: Timestamp,
        remote_timestamp: Timestamp,
        adjusted_remote_timestamp: Timestamp,
        timestamp_offset: TimestampOffset,
    },
    #[error("Timestamp too far ahead")]
    TooFarAhead {
        local_timestamp: Timestamp,
        remote_timestamp: Timestamp,
        adjusted_remote_timestamp: Timestamp,
        timestamp_offset: TimestampOffset,
    },
    #[error("Duplicate timestamp")]
    Duplicate {
        local_timestamp: Timestamp,
        last_local_timestamp: Timestamp,
        remote_timestamp: Timestamp,
        adjusted_remote_timestamp: Timestamp,
        timestamp_offset: TimestampOffset,
    },
    #[error("Sent while this node was stalled")]
    SentWhileStalled {
        local_timestamp: Timestamp,
        remote_timestamp: Timestamp,
        adjusted_remote_timestamp: Timestamp,
        stall: StallWindow,
    },
}

/// A stretch of time this node could not run, such as a frozen process
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct StallWindow {
    pub start: Timestamp,
    pub end: Timestamp,
}

impl StallWindow {
    fn contains(&self, ts: Timestamp) -> bool {
        self.start <= ts && ts <= self.end
    }
}

#[derive(ThisError, Debug, Clone, Copy, PartialEq, Eq)]
#[error("Address not in table")]
pub struct AddressNotInTableError {}

struct AddressFilterInner {
    conn_count_by_ip4: BTreeMap<Ipv4Addr, usize>,
    conn_count_by_ip6_prefix: BTreeMap<Ipv6Addr, usize>,
    conn_timestamps_by_ip4: BTreeMap<Ipv4Addr, Vec<Timestamp>>,
    conn_timestamps_by_ip6_prefix: BTreeMap<Ipv6Addr, Vec<Timestamp>>,
    punishments_by_ip4: BTreeMap<Ipv4Addr, Punishment>,
    punishments_by_ip6_prefix: BTreeMap<Ipv6Addr, Punishment>,
    punishments_by_node_id: BTreeMap<NodeId, Punishment>,
    envelope_info_by_node_id: hashlink::LruCache<NodeId, EnvelopeInfo>,
    stall_window: Option<StallWindow>,
}

impl fmt::Debug for AddressFilterInner {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut envelope_by_node_id = hashlink::LruCache::<NodeId, EnvelopeInfo>::new(64);
        for entry in self.envelope_info_by_node_id.iter().skip(
            if f.sign_plus() || self.envelope_info_by_node_id.len() <= 64 {
                0
            } else {
                self.envelope_info_by_node_id.len() - 64
            },
        ) {
            envelope_by_node_id.insert(entry.0.clone(), entry.1.clone());
        }
        let envelope_info_by_node_id_name =
            if envelope_by_node_id.len() < self.envelope_info_by_node_id.len() {
                format!(
                    "envelope_info_by_node_id(truncated from {})",
                    self.envelope_info_by_node_id.len()
                )
            } else {
                "envelope_info_by_node_id".to_string()
            };

        f.debug_struct("AddressFilterInner")
            .field("conn_count_by_ip4", &self.conn_count_by_ip4)
            .field("conn_count_by_ip6_prefix", &self.conn_count_by_ip6_prefix)
            .field("conn_timestamps_by_ip4", &self.conn_timestamps_by_ip4)
            .field(
                "conn_timestamps_by_ip6_prefix",
                &self.conn_timestamps_by_ip6_prefix,
            )
            .field("punishments_by_ip4", &self.punishments_by_ip4)
            .field("punishments_by_ip6_prefix", &self.punishments_by_ip6_prefix)
            .field("punishments_by_node_id", &self.punishments_by_node_id)
            .field(&envelope_info_by_node_id_name, &envelope_by_node_id)
            .finish()
    }
}

pub(crate) struct AddressFilter {
    registry: VeilidComponentRegistry,
    inner: Mutex<AddressFilterInner>,
    max_connections_per_ip4: usize,
    max_connections_per_ip6_prefix: usize,
    max_connections_per_ip6_prefix_size: usize,
    max_connection_frequency_per_min: usize,
    punishment_duration_min: usize,
    opt_max_timestamp_ahead: Option<TimestampDuration>,
    opt_max_timestamp_behind: Option<TimestampDuration>,
    oldest_timestamp_fallback: TimestampDuration,
}

impl fmt::Debug for AddressFilter {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("AddressFilter")
            //.field("registry", &self.registry)
            .field("inner", &self.inner)
            .field("max_connections_per_ip4", &self.max_connections_per_ip4)
            .field(
                "max_connections_per_ip6_prefix",
                &self.max_connections_per_ip6_prefix,
            )
            .field(
                "max_connections_per_ip6_prefix_size",
                &self.max_connections_per_ip6_prefix_size,
            )
            .field(
                "max_connection_frequency_per_min",
                &self.max_connection_frequency_per_min,
            )
            .field("punishment_duration_min", &self.punishment_duration_min)
            .field("opt_max_timestamp_ahead", &self.opt_max_timestamp_ahead)
            .field("opt_max_timestamp_behind", &self.opt_max_timestamp_behind)
            .field("oldest_timestamp_fallback", &self.oldest_timestamp_fallback)
            .finish()
    }
}

impl_veilid_component_accessors!(AddressFilter);

impl AddressFilter {
    pub fn new(registry: VeilidComponentRegistry) -> Self {
        let config = registry.config();

        let opt_max_timestamp_behind = config
            .internal()
            .network
            .rpc
            .max_timestamp_behind_ms
            .map(ms_to_us)
            .map(TimestampDuration::new);
        let opt_max_timestamp_ahead = config
            .internal()
            .network
            .rpc
            .max_timestamp_ahead_ms
            .map(ms_to_us)
            .map(TimestampDuration::new);

        Self {
            registry,
            inner: Mutex::new(AddressFilterInner {
                conn_count_by_ip4: BTreeMap::new(),
                conn_count_by_ip6_prefix: BTreeMap::new(),
                conn_timestamps_by_ip4: BTreeMap::new(),
                conn_timestamps_by_ip6_prefix: BTreeMap::new(),
                punishments_by_ip4: BTreeMap::new(),
                punishments_by_ip6_prefix: BTreeMap::new(),
                punishments_by_node_id: BTreeMap::new(),
                envelope_info_by_node_id: hashlink::LruCache::new(MAX_ENVELOPE_INFO_BY_NODE_ID),
                stall_window: None,
            }),
            max_connections_per_ip4: config.internal().network.max_connections_per_ip4 as usize,
            max_connections_per_ip6_prefix: config.internal().network.max_connections_per_ip6_prefix
                as usize,
            max_connections_per_ip6_prefix_size: config
                .internal()
                .network
                .max_connections_per_ip6_prefix_size
                as usize,
            max_connection_frequency_per_min: config
                .internal()
                .network
                .max_connection_frequency_per_min
                as usize,
            punishment_duration_min: PUNISHMENT_DURATION_MIN,
            opt_max_timestamp_ahead,
            opt_max_timestamp_behind,
            // Two RPC timeouts when the configured window is off
            oldest_timestamp_fallback: config.rpc_timeout().saturating_mul(2),
        }
    }

    // When the network restarts, some of the address filter can be cleared
    pub fn restart(&self) {
        let mut inner = self.inner.lock();
        inner.conn_count_by_ip4.clear();
        inner.conn_count_by_ip6_prefix.clear();
    }

    fn purge_old_connection_timestamps_inner(
        &self,
        inner: &mut AddressFilterInner,
        cur_ts: Timestamp,
    ) {
        // v4
        {
            let mut dead_keys = Vec::<Ipv4Addr>::new();
            for (key, value) in &mut inner.conn_timestamps_by_ip4 {
                value.retain(|v| {
                    // keep timestamps that are less than a minute away
                    cur_ts.duration_since(*v) < TimestampDuration::new_secs(60)
                });
                if value.is_empty() {
                    dead_keys.push(*key);
                }
            }
            for key in dead_keys {
                inner.conn_timestamps_by_ip4.remove(&key);
            }
        }
        // v6
        {
            let mut dead_keys = Vec::<Ipv6Addr>::new();
            for (key, value) in &mut inner.conn_timestamps_by_ip6_prefix {
                value.retain(|v| {
                    // keep timestamps that are less than a minute away
                    cur_ts.duration_since(*v) < TimestampDuration::new_secs(60)
                });
                if value.is_empty() {
                    dead_keys.push(*key);
                }
            }
            for key in dead_keys {
                inner.conn_timestamps_by_ip6_prefix.remove(&key);
            }
        }
    }

    fn purge_old_punishments(&self, inner: &mut AddressFilterInner, cur_ts: Timestamp) {
        // v4
        {
            let mut dead_keys = Vec::<Ipv4Addr>::new();
            for (key, value) in &mut inner.punishments_by_ip4 {
                // Drop punishments older than the punishment duration
                if cur_ts.as_u64().saturating_sub(value.timestamp.as_u64())
                    > self.punishment_duration_min as u64 * 60_000_000u64
                {
                    dead_keys.push(*key);
                }
            }
            for key in dead_keys {
                self.forgive_ip_addr_inner(inner, IpAddr::V4(key));
            }
        }
        // v6
        {
            let mut dead_keys = Vec::<Ipv6Addr>::new();
            for (key, value) in &mut inner.punishments_by_ip6_prefix {
                // Drop punishments older than the punishment duration
                if cur_ts.as_u64().saturating_sub(value.timestamp.as_u64())
                    > self.punishment_duration_min as u64 * 60_000_000u64
                {
                    dead_keys.push(*key);
                }
            }
            for key in dead_keys {
                self.forgive_ip_addr_inner(inner, IpAddr::V6(key));
            }
        }
        // node id
        {
            let mut dead_keys = Vec::<NodeId>::new();
            for (key, value) in &mut inner.punishments_by_node_id {
                // Drop punishments older than the punishment duration
                if cur_ts.as_u64().saturating_sub(value.timestamp.as_u64())
                    > self.punishment_duration_min as u64 * 60_000_000u64
                {
                    dead_keys.push(key.clone());
                }
            }
            for key in dead_keys {
                self.forgive_node_id_inner(inner, key);
            }
        }
    }

    fn is_ip_addr_punished_inner(&self, inner: &AddressFilterInner, ipblock: IpAddr) -> bool {
        match ipblock {
            IpAddr::V4(v4) => {
                if inner.punishments_by_ip4.contains_key(&v4) {
                    return true;
                }
            }
            IpAddr::V6(v6) => {
                if inner.punishments_by_ip6_prefix.contains_key(&v6) {
                    return true;
                }
            }
        }
        false
    }

    pub fn is_ip_addr_punished(&self, addr: IpAddr) -> bool {
        let inner = self.inner.lock();
        let ipblock = ip_to_ipblock(self.max_connections_per_ip6_prefix_size, addr);
        self.is_ip_addr_punished_inner(&inner, ipblock)
    }

    #[cfg_attr(not(feature = "debug-api"), expect(dead_code))]
    pub fn clear_punishments(&self) {
        let mut inner = self.inner.lock();
        inner.punishments_by_ip4.clear();
        inner.punishments_by_ip6_prefix.clear();
        inner.punishments_by_node_id.clear();

        self.routing_table().clear_punishments();
    }

    pub fn punish_ip_addr(&self, addr: IpAddr, reason: PunishmentReason) {
        veilid_log!(self debugwarn "Punished: {} for {:?}", addr, reason);
        let timestamp = Timestamp::now();
        let punishment = Punishment { reason, timestamp };

        let ipblock = ip_to_ipblock(self.max_connections_per_ip6_prefix_size, addr);

        let mut inner = self.inner.lock();
        match ipblock {
            IpAddr::V4(v4) => inner
                .punishments_by_ip4
                .entry(v4)
                .and_modify(|v| *v = punishment)
                .or_insert(punishment),
            IpAddr::V6(v6) => inner
                .punishments_by_ip6_prefix
                .entry(v6)
                .and_modify(|v| *v = punishment)
                .or_insert(punishment),
        };
    }

    #[cfg_attr(not(feature = "debug-api"), expect(dead_code))]
    pub fn forgive_ip_addr(&self, addr: IpAddr) {
        let mut inner = self.inner.lock();
        self.forgive_ip_addr_inner(&mut inner, addr);
    }

    fn forgive_ip_addr_inner(&self, inner: &mut AddressFilterInner, addr: IpAddr) {
        veilid_log!(self warn "Forgiving: {}", addr);
        let ipblock = ip_to_ipblock(self.max_connections_per_ip6_prefix_size, addr);
        match ipblock {
            IpAddr::V4(v4) => inner.punishments_by_ip4.remove(&v4),
            IpAddr::V6(v6) => inner.punishments_by_ip6_prefix.remove(&v6),
        };
    }

    fn is_node_id_punished_inner(&self, inner: &AddressFilterInner, node_id: NodeId) -> bool {
        if inner.punishments_by_node_id.contains_key(&node_id) {
            return true;
        }
        false
    }

    pub fn is_node_id_punished(&self, node_id: NodeId) -> bool {
        let inner = self.inner.lock();
        self.is_node_id_punished_inner(&inner, node_id)
    }

    pub fn punish_node_id(&self, node_id: NodeId, reason: PunishmentReason) {
        if let Ok(Some(nr)) = self.routing_table().lookup_node_id(node_id.clone()) {
            // make the entry dead if it's punished
            nr.set_punished(Some(reason));
        }

        let timestamp = Timestamp::now();
        let punishment = Punishment { reason, timestamp };

        let mut inner = self.inner.lock();
        if inner.punishments_by_node_id.len() >= MAX_PUNISHMENTS_BY_NODE_ID {
            veilid_log!(self debugwarn "Punishment table full: {}", node_id);
            return;
        }
        veilid_log!(self debugwarn "Punished: {} for {:?}", node_id, reason);
        inner
            .punishments_by_node_id
            .entry(node_id)
            .and_modify(|v| *v = punishment)
            .or_insert(punishment);
    }

    #[cfg_attr(not(feature = "debug-api"), expect(dead_code))]
    pub fn forgive_node_id(&self, node_id: NodeId) {
        let mut inner = self.inner.lock();
        self.forgive_node_id_inner(&mut inner, node_id)
    }

    fn forgive_node_id_inner(&self, inner: &mut AddressFilterInner, node_id: NodeId) {
        veilid_log!(self warn "Forgiving: {}", node_id);
        inner.punishments_by_node_id.remove(&node_id);
        // make the entry alive again if it's still here
        if let Ok(Some(nr)) = self.routing_table().lookup_node_id(node_id) {
            nr.set_punished(None);
        }
    }

    #[cfg_attr(feature = "instrument", instrument(parent = None, level = "trace", skip_all, err, fields(__VEILID_LOG_KEY = self.log_key())))]
    #[allow(clippy::unused_async)]
    pub async fn address_filter_task_routine(
        &self,
        _stop_token: StopToken,
        _last_ts: Timestamp,
        cur_ts: Timestamp,
    ) -> EyreResult<()> {
        //
        let mut inner = self.inner.lock();
        self.purge_old_connection_timestamps_inner(&mut inner, cur_ts);
        self.purge_old_punishments(&mut inner, cur_ts);

        Ok(())
    }

    pub fn add_connection(&self, addr: IpAddr) -> Result<(), AddConnectionError> {
        let inner = &mut *self.inner.lock();

        let ipblock = ip_to_ipblock(self.max_connections_per_ip6_prefix_size, addr);
        if self.is_ip_addr_punished_inner(inner, ipblock) {
            return Err(AddConnectionError::Punished);
        }

        let ts = Timestamp::now_non_decreasing();
        self.purge_old_connection_timestamps_inner(inner, ts);

        match ipblock {
            IpAddr::V4(v4) => {
                // See if we have too many connections from this ip block
                let cnt = inner.conn_count_by_ip4.entry(v4).or_default();
                if *cnt > self.max_connections_per_ip4 {
                    veilid_log!(self error "Address filter count overflow: {:?}", v4);
                }
                if *cnt >= self.max_connections_per_ip4 {
                    veilid_log!(self warn "Address filter count exceeded: {:?}", v4);
                    return Err(AddConnectionError::CountExceeded);
                }
                self.note_frequency(inner.conn_timestamps_by_ip4.entry(v4).or_default(), ts, v4)?;
                *cnt += 1;
            }
            IpAddr::V6(v6) => {
                // See if we have too many connections from this ip block
                let cnt = inner.conn_count_by_ip6_prefix.entry(v6).or_default();
                if *cnt > self.max_connections_per_ip6_prefix {
                    veilid_log!(self error "Address filter count overflow: {:?}", v6);
                }
                if *cnt >= self.max_connections_per_ip6_prefix {
                    veilid_log!(self warn "Address filter count exceeded: {:?}", v6);
                    return Err(AddConnectionError::CountExceeded);
                }
                self.note_frequency(
                    inner.conn_timestamps_by_ip6_prefix.entry(v6).or_default(),
                    ts,
                    v6,
                )?;
                *cnt += 1;
            }
        }
        Ok(())
    }

    /// Counts a direct bootstrap request against its block's connection rate, so a flood of
    /// requests draws no more replies than connections would
    pub fn add_bootstrap_request(&self, addr: IpAddr) -> Result<(), AddConnectionError> {
        let inner = &mut *self.inner.lock();

        let ipblock = ip_to_ipblock(self.max_connections_per_ip6_prefix_size, addr);
        if self.is_ip_addr_punished_inner(inner, ipblock) {
            return Err(AddConnectionError::Punished);
        }

        let ts = Timestamp::now_non_decreasing();
        self.purge_old_connection_timestamps_inner(inner, ts);

        match ipblock {
            IpAddr::V4(v4) => {
                self.note_frequency(inner.conn_timestamps_by_ip4.entry(v4).or_default(), ts, v4)
            }
            IpAddr::V6(v6) => self.note_frequency(
                inner.conn_timestamps_by_ip6_prefix.entry(v6).or_default(),
                ts,
                v6,
            ),
        }
    }

    /// Records one event for the block, or refuses it at the per-minute rate
    fn note_frequency(
        &self,
        tstamps: &mut Vec<Timestamp>,
        ts: Timestamp,
        block: impl fmt::Debug,
    ) -> Result<(), AddConnectionError> {
        tstamps.retain(|v| ts.duration_since(*v) < TimestampDuration::new_secs(60));
        if tstamps.len() > self.max_connection_frequency_per_min {
            veilid_log!(self error "Address filter rate overflow: {:?}", block);
        }
        if tstamps.len() >= self.max_connection_frequency_per_min {
            veilid_log!(self warn "Address filter rate exceeded: {:?}", block);
            return Err(AddConnectionError::RateExceeded);
        }
        tstamps.push(ts);
        Ok(())
    }

    pub fn remove_connection(&self, addr: IpAddr) -> Result<(), AddressNotInTableError> {
        let mut inner = self.inner.lock();

        let ipblock = ip_to_ipblock(self.max_connections_per_ip6_prefix_size, addr);

        let ts = Timestamp::now();
        self.purge_old_connection_timestamps_inner(&mut inner, ts);

        match ipblock {
            IpAddr::V4(v4) => {
                match inner.conn_count_by_ip4.entry(v4) {
                    Entry::Vacant(_) => {
                        return Err(AddressNotInTableError {});
                    }
                    Entry::Occupied(mut o) => {
                        let cnt = o.get_mut();
                        if *cnt == 0 {
                            veilid_log!(self error "Address filter count underflow: {:?}", v4);
                            inner.conn_count_by_ip4.remove(&v4);
                            return Err(AddressNotInTableError {});
                        }
                        if *cnt == 1 {
                            inner.conn_count_by_ip4.remove(&v4);
                        } else {
                            *cnt -= 1;
                        }
                    }
                };
            }
            IpAddr::V6(v6) => {
                match inner.conn_count_by_ip6_prefix.entry(v6) {
                    Entry::Vacant(_) => {
                        return Err(AddressNotInTableError {});
                    }
                    Entry::Occupied(mut o) => {
                        let cnt = o.get_mut();
                        if *cnt == 0 {
                            veilid_log!(self error "Address filter count underflow: {:?}", v6);
                            inner.conn_count_by_ip6_prefix.remove(&v6);
                            return Err(AddressNotInTableError {});
                        }
                        if *cnt == 1 {
                            inner.conn_count_by_ip6_prefix.remove(&v6);
                        } else {
                            *cnt -= 1;
                        }
                    }
                };
            }
        }
        Ok(())
    }

    /// Checks an envelope's claimed send time against this node's clock, adjusted by the
    /// sender's usual offset from it.
    ///
    /// - An envelope sent while this node was stalled is dropped before anything is learned
    ///   from it, even from a sender seen for the first time. See [AddressFilter::record_stall].
    /// - The first envelope from a sender is accepted and seeds its offset.
    /// - After that only envelopes that pass move the offset, so stale or replayed ones cannot
    ///   drag it. A sender rejected for a whole window with none passing has stepped its
    ///   clock, and its offset starts over from its latest envelope.
    pub fn check_envelope_timestamp(
        &self,
        node_id: NodeId,
        local_timestamp: Timestamp,
        remote_timestamp: Timestamp,
    ) -> Result<(), TimestampError> {
        // Get oldest timestamp we want to keep
        let oldest_timestamp_duration = self
            .opt_max_timestamp_behind
            .unwrap_or(self.oldest_timestamp_fallback);
        let oldest_local_timestamp = local_timestamp.earlier(oldest_timestamp_duration);

        let mut inner = self.inner.lock();

        if let Some(stall) = self.active_stall_window(&mut inner, local_timestamp) {
            let sent = inner
                .envelope_info_by_node_id
                .peek(&node_id)
                .map(|pi| pi.timestamp_offset.adjust_remote(remote_timestamp))
                .unwrap_or(remote_timestamp);
            if stall.contains(sent) {
                return Err(TimestampError::SentWhileStalled {
                    local_timestamp,
                    remote_timestamp,
                    adjusted_remote_timestamp: sent,
                    stall,
                });
            }
        }

        let Some(pi) = inner.envelope_info_by_node_id.get_mut(&node_id) else {
            inner.envelope_info_by_node_id.insert(
                node_id,
                EnvelopeInfo::new(local_timestamp, remote_timestamp),
            );
            return Ok(());
        };

        // Adjust received envelope timestamp to local offset
        let adjusted_remote_timestamp = pi.timestamp_offset.adjust_remote(remote_timestamp);

        // Drop duplicated remote timestamps
        if let Some(last) = pi.envelope_timestamp_log.get(&remote_timestamp).copied() {
            return Err(TimestampError::Duplicate {
                local_timestamp,
                last_local_timestamp: last.local,
                remote_timestamp,
                adjusted_remote_timestamp,
                timestamp_offset: pi.timestamp_offset,
            });
        };

        let res = self.check_timestamp_range(
            local_timestamp,
            remote_timestamp,
            adjusted_remote_timestamp,
            pi.timestamp_offset,
        );
        if res.is_err() {
            let heal_after = if oldest_timestamp_duration.as_u64() == 0 {
                self.oldest_timestamp_fallback
            } else {
                oldest_timestamp_duration
            };
            if pi
                .rejecting_since
                .is_some_and(|since| local_timestamp.duration_since(since) >= heal_after)
            {
                pi.reseed(local_timestamp, remote_timestamp, oldest_local_timestamp);
                return Ok(());
            }
        }

        // Add to the log
        pi.add_timestamp(
            local_timestamp,
            remote_timestamp,
            res.is_ok(),
            oldest_local_timestamp,
        );

        res
    }

    /// Drop envelopes that are too old or too new
    fn check_timestamp_range(
        &self,
        local_timestamp: Timestamp,
        remote_timestamp: Timestamp,
        adjusted_remote_timestamp: Timestamp,
        timestamp_offset: TimestampOffset,
    ) -> Result<(), TimestampError> {
        if let Some(max_timestamp_behind) = self.opt_max_timestamp_behind {
            if max_timestamp_behind.as_u64() != 0
                && (local_timestamp > adjusted_remote_timestamp
                    && local_timestamp.duration_since(adjusted_remote_timestamp)
                        > max_timestamp_behind)
            {
                return Err(TimestampError::TooFarBehind {
                    local_timestamp,
                    remote_timestamp,
                    adjusted_remote_timestamp,
                    timestamp_offset,
                });
            }
        }
        if let Some(max_timestamp_ahead) = self.opt_max_timestamp_ahead {
            if max_timestamp_ahead.as_u64() != 0
                && (local_timestamp < adjusted_remote_timestamp
                    && adjusted_remote_timestamp.duration_since(local_timestamp)
                        > max_timestamp_ahead)
            {
                return Err(TimestampError::TooFarAhead {
                    local_timestamp,
                    remote_timestamp,
                    adjusted_remote_timestamp,
                    timestamp_offset,
                });
            }
        }
        Ok(())
    }

    /// Records a stretch this node could not run, such as a frozen process.
    ///
    /// Envelopes sent during it waited out the stall in a socket buffer: answers nobody is
    /// waiting for, and questions whose askers have given up. They are dropped until the
    /// stall is as old as the behind limit, after which the limit drops them anyway. A
    /// sender first seen from one of them, before the stall was known, is forgotten so its
    /// next envelope seeds it afresh. Stalls shorter than the behind limit are ignored: what
    /// they buffer is within it, and some of it is answers still being waited for.
    pub fn record_stall(&self, start: Timestamp, end: Timestamp) {
        let Some(max_timestamp_behind) = self.enabled_max_timestamp_behind() else {
            return;
        };
        if end <= start || end.duration_since(start) < max_timestamp_behind {
            return;
        }

        let mut inner = self.inner.lock();
        let stall = match inner.stall_window {
            Some(w) if w.start <= end && start <= w.end => StallWindow {
                start: w.start.min(start),
                end: w.end.max(end),
            },
            _ => StallWindow { start, end },
        };
        inner.stall_window = Some(stall);
        inner
            .envelope_info_by_node_id
            .retain(|_, pi| !(pi.seed.1 >= stall.start && stall.contains(pi.seed.0)));

        veilid_log!(self debug "dropping envelopes sent while stalled: {:#} for {:#}", stall.start, stall.end.duration_since(stall.start));
    }

    /// The stall whose envelopes are still being dropped, if any
    fn active_stall_window(
        &self,
        inner: &mut AddressFilterInner,
        local_timestamp: Timestamp,
    ) -> Option<StallWindow> {
        let max_timestamp_behind = self.enabled_max_timestamp_behind()?;
        let stall = inner.stall_window?;
        if local_timestamp > stall.end.later(max_timestamp_behind) {
            inner.stall_window = None;
            return None;
        }
        Some(stall)
    }

    fn enabled_max_timestamp_behind(&self) -> Option<TimestampDuration> {
        self.opt_max_timestamp_behind.filter(|d| d.as_u64() != 0)
    }
}
