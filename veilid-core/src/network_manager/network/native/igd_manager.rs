use super::*;
use igd::*;
use std::net::UdpSocket;
use stop_token::future::FutureExt as StopFutureExt;

impl_veilid_log_facility!("net");

const UPNP_GATEWAY_DETECT_TIMEOUT_MS: u32 = 3_000;
const UPNP_MAPPING_ATTEMPTS: u32 = 3;
const UPNP_MAPPING_LIFETIME: TimestampDuration = TimestampDuration::new_ms(120_000);

/// What one renewal pass did
#[derive(Clone, Copy, Debug, Default)]
pub struct IgdTickOutcome {
    /// a mapping could not be renewed at all; the network must restart
    pub needs_restart: bool,
    /// a mapping came back on a different external port, so the dial info moved
    pub remapped: bool,
    /// a mapping has no gateway left to renew it through, so the dial info using it is gone
    pub lost: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub(super) struct PortMapKey {
    protocol_type: IGDProtocolType,
    address_type: IGDAddressType,
    local_port: u16,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub(super) struct PortMapValue {
    ext_ip: IpAddr,
    mapped_port: u16,
    timestamp: Timestamp,
    renewal_lifetime: TimestampDuration,
    renewal_attempts: u32,
}

/// Command for the background worker
enum IGDCommand {
    Probe(IGDAddressType),
}

/// Never locked across a router request: shutdown and callers on the executor lock it too
struct IGDManagerInner {
    /// Routed local IP per address type; `None` value = probed, no routable IP
    local_ip_addrs: BTreeMap<IGDAddressType, Option<IpAddr>>,
    /// IGD gateway per local IP; `None` value = probed, no gateway
    gateways: BTreeMap<IpAddr, Option<Arc<Gateway>>>,
    /// External IP per local IP; `None` value = probed, gateway gave no external IP
    external_ips: BTreeMap<IpAddr, Option<IpAddr>>,
    /// Active port mappings
    port_maps: BTreeMap<PortMapKey, PortMapValue>,
    /// Worker holds during `do_probe`; cache readers lock to wait for fresh cache
    probe_locks: BTreeMap<IGDAddressType, Arc<AsyncMutex<()>>>,
    /// Probes triggered and not yet finished, per address type
    pending_probes: BTreeMap<IGDAddressType, usize>,
    /// Address types a detection skipped mapping for because their probe had not finished
    deferred_maps: BTreeSet<IGDAddressType>,
    /// Bumped by `clear_caches`, so a mapping made against the old caches is not stored
    cache_generation: u64,
    /// Worker command sender
    cmd_tx: Option<flume::Sender<IGDCommand>>,
    /// Worker stop source
    stop_source: Option<StopSource>,
    /// Worker join handle
    worker_jh: Option<MustJoinHandle<()>>,
}

impl Drop for IGDManagerInner {
    fn drop(&mut self) {
        // Detach so MustJoinHandle::drop doesn't panic if shutdown() didn't run
        if let Some(jh) = self.worker_jh.take() {
            jh.detach();
        }
    }
}

#[derive(Clone)]
pub struct IGDManager {
    registry: VeilidComponentRegistry,
    inner: Arc<Mutex<IGDManagerInner>>,
}

impl_veilid_component_accessors!(IGDManager);

fn convert_protocol_type(igdpt: IGDProtocolType) -> PortMappingProtocol {
    match igdpt {
        IGDProtocolType::UDP => PortMappingProtocol::UDP,
        IGDProtocolType::TCP => PortMappingProtocol::TCP,
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum IGDAddressType {
    IPV6,
    IPV4,
}

impl From<AddressType> for IGDAddressType {
    fn from(address_type: AddressType) -> Self {
        match address_type {
            AddressType::IPV4 => IGDAddressType::IPV4,
            AddressType::IPV6 => IGDAddressType::IPV6,
        }
    }
}

impl From<IpAddr> for IGDAddressType {
    fn from(ip: IpAddr) -> Self {
        match ip {
            IpAddr::V4(_) => IGDAddressType::IPV4,
            IpAddr::V6(_) => IGDAddressType::IPV6,
        }
    }
}

impl fmt::Display for IGDAddressType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            IGDAddressType::IPV6 => write!(f, "IPV6"),
            IGDAddressType::IPV4 => write!(f, "IPV4"),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum IGDProtocolType {
    UDP,
    TCP,
}

impl fmt::Display for IGDProtocolType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            IGDProtocolType::UDP => write!(f, "UDP"),
            IGDProtocolType::TCP => write!(f, "TCP"),
        }
    }
}

impl IGDManager {
    /////////////////////////////////////////////////////////////////////
    // Public Interface

    pub fn new(registry: VeilidComponentRegistry) -> Self {
        Self {
            registry,
            inner: Arc::new(Mutex::new(IGDManagerInner {
                local_ip_addrs: BTreeMap::new(),
                gateways: BTreeMap::new(),
                external_ips: BTreeMap::new(),
                port_maps: BTreeMap::new(),
                probe_locks: BTreeMap::new(),
                pending_probes: BTreeMap::new(),
                deferred_maps: BTreeSet::new(),
                cache_generation: 0,
                cmd_tx: None,
                stop_source: None,
                worker_jh: None,
            })),
        }
    }

    #[cfg_attr(
        feature = "instrument",
        instrument(level = "trace", target = "net", skip_all, fields(__VEILID_LOG_KEY = self.log_key()))
    )]
    #[expect(dead_code)]
    pub async fn unmap_port(
        &self,
        protocol_type: IGDProtocolType,
        address_type: IGDAddressType,
        mapped_port: u16,
    ) -> Option<()> {
        // Wait for any in-flight probe for this address type so the cache below is fresh
        let _probe_guard = self.wait_for_probe(address_type).await;

        let gw = {
            let mut inner = self.inner.lock();

            // If we already have this port mapped, just return the existing portmap
            let mut found = None;
            for (pmk, pmv) in &inner.port_maps {
                if pmk.protocol_type == protocol_type
                    && pmk.address_type == address_type
                    && pmv.mapped_port == mapped_port
                {
                    found = Some(*pmk);
                    break;
                }
            }
            let pmk = found?;
            let _pmv = inner
                .port_maps
                .remove(&pmk)
                .expect_or_log("key found but remove failed");

            // Get local ip address
            let local_ip = Self::find_local_ip_inner(&inner, address_type)?;

            // Find gateway
            Self::find_gateway_inner(&inner, local_ip)?
        };

        // Unmap port
        match gw
            .remove_port(convert_protocol_type(protocol_type), mapped_port)
            .await
        {
            Ok(()) => (),
            Err(e) => {
                // Failed to map external port
                veilid_log!(self debug "upnp failed to remove external port: {}", e);
                return None;
            }
        };
        Some(())
    }

    #[cfg_attr(
        feature = "instrument",
        instrument(level = "trace", target = "net", skip_all, fields(__VEILID_LOG_KEY = self.log_key()))
    )]
    pub async fn map_any_port(
        &self,
        protocol_type: IGDProtocolType,
        address_type: IGDAddressType,
        local_port: u16,
        expected_external_address: Option<IpAddr>,
    ) -> Option<SocketAddr> {
        // A search can run for seconds: skip, and `has_mappable_deferred` asks for a new detection
        let probe_lock = {
            let mut inner = self.inner.lock();
            if inner.pending_probes.contains_key(&address_type) {
                inner.deferred_maps.insert(address_type);
                return None;
            }
            inner.deferred_maps.remove(&address_type);
            inner.probe_locks.get(&address_type).cloned()
        };

        // Hold the probe lock across the mapping so a probe can't repopulate the cache mid-mapping
        let _probe_guard = match probe_lock {
            Some(lock) => Some(lock.lock_arc().await),
            None => None,
        };

        let pmkey = PortMapKey {
            protocol_type,
            address_type,
            local_port,
        };
        let (local_ip, gw, ext_ip, cache_generation) = {
            let inner = self.inner.lock();
            if let Some(pmval) = inner.port_maps.get(&pmkey) {
                return Some(SocketAddr::new(pmval.ext_ip, pmval.mapped_port));
            }

            let local_ip = Self::find_local_ip_inner(&inner, address_type)?;
            let gw = Self::find_gateway_inner(&inner, local_ip)?;
            let ext_ip = Self::find_external_ip_inner(&inner, local_ip)?;
            (local_ip, gw, ext_ip, inner.cache_generation)
        };

        // Ensure external IP matches address type
        if ext_ip.is_ipv4() && address_type != IGDAddressType::IPV4 {
            veilid_log!(self debug "mismatched ip address type from igd, wanted v4, got v6");
            return None;
        } else if ext_ip.is_ipv6() && address_type != IGDAddressType::IPV6 {
            veilid_log!(self debug "mismatched ip address type from igd, wanted v6, got v4");
            return None;
        }

        if let Some(expected_external_address) = expected_external_address {
            if ext_ip != expected_external_address {
                veilid_log!(self debug "gateway external address does not match calculated external address: expected={} vs gateway={}", expected_external_address, ext_ip);
                return None;
            }
        }

        // Map any port
        let desc = self.get_description(protocol_type, local_port);
        let mapped_port = match gw
            .add_any_port(
                convert_protocol_type(protocol_type),
                SocketAddr::new(local_ip, local_port),
                UPNP_MAPPING_LIFETIME
                    .millis_u32()
                    .unwrap_or_log()
                    .div_ceil(1000),
                &desc,
            )
            .await
        {
            Ok(mapped_port) => mapped_port,
            Err(e) => {
                // Failed to map external port
                veilid_log!(self debug "upnp failed to map external port: {}", e);
                return None;
            }
        };

        // Add to mapping list to keep alive
        let timestamp = Timestamp::now();
        if !self.store_port_map(
            cache_generation,
            pmkey,
            Some(PortMapValue {
                ext_ip,
                mapped_port,
                timestamp,
                renewal_lifetime: UPNP_MAPPING_LIFETIME.div(2),
                renewal_attempts: 0,
            }),
        ) {
            return None;
        }

        // Succeeded, return the externally mapped port
        Some(SocketAddr::new(ext_ip, mapped_port))
    }

    /// Spawn the background worker. Idempotent.
    pub fn startup(&self) {
        let mut inner = self.inner.lock();
        if inner.cmd_tx.is_some() {
            return;
        }
        let (tx, rx) = flume::unbounded::<IGDCommand>();
        let stop_source = StopSource::new();
        let stop_token = stop_source.token();
        let this = self.clone();
        let jh = spawn("igd worker", async move {
            this.worker_loop(rx, stop_token).await;
        });
        inner.cmd_tx = Some(tx);
        inner.stop_source = Some(stop_source);
        inner.worker_jh = Some(jh);
    }

    /// Stop the background worker and wait for it to finish. Idempotent.
    pub async fn shutdown(&self) {
        let jh = {
            let mut inner = self.inner.lock();
            inner.cmd_tx = None;
            inner.stop_source = None;
            inner.worker_jh.take()
        };
        if let Some(jh) = jh {
            jh.await;
        }
    }

    /// Forget probed gateways, external IPs and mappings, for one address type or all of them
    pub fn clear_caches(&self, address_type: Option<IGDAddressType>) {
        let mut inner = self.inner.lock();
        match address_type {
            None => {
                inner.local_ip_addrs.clear();
                inner.gateways.clear();
                inner.external_ips.clear();
                inner.port_maps.clear();
                inner.deferred_maps.clear();
            }
            Some(at) => {
                inner.local_ip_addrs.remove(&at);
                inner
                    .gateways
                    .retain(|ip, _| IGDAddressType::from(*ip) != at);
                inner
                    .external_ips
                    .retain(|ip, _| IGDAddressType::from(*ip) != at);
                inner.port_maps.retain(|k, _| k.address_type != at);
                inner.deferred_maps.remove(&at);
            }
        }
        inner.cache_generation += 1;
    }

    /// Whether a mapping skipped while its probe ran could now be made. Deferred address types
    /// whose probe found nothing to map through are forgotten.
    pub fn has_mappable_deferred(&self) -> bool {
        let mut inner = self.inner.lock();
        let finished: Vec<IGDAddressType> = inner
            .deferred_maps
            .iter()
            .filter(|at| !inner.pending_probes.contains_key(at))
            .copied()
            .collect();
        let mut mappable = false;
        for address_type in finished {
            if Self::can_map_inner(&inner, address_type) {
                mappable = true;
            } else {
                inner.deferred_maps.remove(&address_type);
            }
        }
        mappable
    }

    /// Forget skipped mappings once a new detection will attempt them
    pub fn clear_deferred(&self) {
        self.inner.lock().deferred_maps.clear();
    }

    /// The port mappings held and their leases
    pub(super) fn port_maps(&self) -> Vec<(PortMapKey, PortMapValue)> {
        self.inner
            .lock()
            .port_maps
            .iter()
            .map(|(k, v)| (*k, *v))
            .collect()
    }

    /// Hold restored port mappings; their lease timestamps say when each is renewed
    pub(super) fn restore_port_maps(&self, port_maps: Vec<(PortMapKey, PortMapValue)>) {
        self.inner.lock().port_maps.extend(port_maps);
    }

    /// Asynchronously probe for the given address type; returns immediately.
    /// No-op before startup
    pub fn trigger_probe(&self, address_type: IGDAddressType) {
        let mut inner = self.inner.lock();
        inner
            .probe_locks
            .entry(address_type)
            .or_insert_with(|| Arc::new(AsyncMutex::new(())));
        let Some(sent) = inner
            .cmd_tx
            .as_ref()
            .map(|tx| tx.send(IGDCommand::Probe(address_type)))
        else {
            return;
        };
        match sent {
            Ok(()) => *inner.pending_probes.entry(address_type).or_default() += 1,
            Err(e) => veilid_log!(self debug "failed to send IGD probe command: {}", e),
        }
    }

    /// Acquire the per-AT probe lock so callers can read caches knowing no probe
    /// will mutate them while the guard is held. `None` if no probe ever triggered.
    async fn wait_for_probe(&self, address_type: IGDAddressType) -> Option<AsyncMutexGuardArc<()>> {
        let lock = self.inner.lock().probe_locks.get(&address_type).cloned();
        match lock {
            Some(lock) => Some(lock.lock_arc().await),
            None => None,
        }
    }

    async fn worker_loop(&self, rx: flume::Receiver<IGDCommand>, stop_token: StopToken) {
        loop {
            let cmd = match rx.recv_async().timeout_at(stop_token.clone()).await {
                Ok(Ok(cmd)) => cmd,
                _ => return,
            };
            match cmd {
                IGDCommand::Probe(address_type) => {
                    let lock = self.inner.lock().probe_locks.get(&address_type).cloned();
                    let Some(lock) = lock else {
                        continue;
                    };
                    // Wrap awaits in stop_token so a stop ends a probe mid-request
                    let Ok(_guard) = lock.lock_arc().timeout_at(stop_token.clone()).await else {
                        return;
                    };
                    if self
                        .do_probe(address_type)
                        .timeout_at(stop_token.clone())
                        .await
                        .is_err()
                    {
                        return;
                    }
                    let mut inner = self.inner.lock();
                    if let Some(pending) = inner.pending_probes.get_mut(&address_type) {
                        *pending -= 1;
                        if *pending == 0 {
                            inner.pending_probes.remove(&address_type);
                        }
                    }
                }
            }
        }
    }

    /// Probe local IP, gateway, and external IP for an address type and cache the result
    async fn do_probe(&self, address_type: IGDAddressType) {
        let local_ip = self.get_routed_local_ip_address(address_type);
        self.inner
            .lock()
            .local_ip_addrs
            .insert(address_type, local_ip);

        let Some(local_ip) = local_ip else {
            veilid_log!(self debug "no routable local ip for address_type={:?}", address_type);
            return;
        };

        let gateway_opt = match local_ip {
            IpAddr::V4(v4) => {
                let mut opts = SearchOptions::new_v4(UPNP_GATEWAY_DETECT_TIMEOUT_MS as u64);
                opts.bind_addr = SocketAddr::V4(SocketAddrV4::new(v4, 0));
                match igd::search_gateway(opts).await {
                    Ok(v) => Some(Arc::new(v)),
                    Err(e) => {
                        veilid_log!(self debug "couldn't find ipv4 igd: {}", e);
                        None
                    }
                }
            }
            IpAddr::V6(v6) => {
                let mut opts = SearchOptions::new_v6(
                    Ipv6SearchScope::LinkLocal,
                    UPNP_GATEWAY_DETECT_TIMEOUT_MS as u64,
                );
                opts.bind_addr = SocketAddr::V6(SocketAddrV6::new(v6, 0, 0, 0));
                match igd::search_gateway(opts).await {
                    Ok(v) => Some(Arc::new(v)),
                    Err(e) => {
                        veilid_log!(self debug "couldn't find ipv6 igd: {}", e);
                        None
                    }
                }
            }
        };

        let ext_ip = match &gateway_opt {
            Some(gw) => match gw.get_external_ip().await {
                Ok(ip) => Some(ip),
                Err(e) => {
                    veilid_log!(self debug "couldn't get external ip from igd: {}", e);
                    None
                }
            },
            None => None,
        };

        let mut inner = self.inner.lock();
        inner.gateways.insert(local_ip, gateway_opt);
        inner.external_ips.insert(local_ip, ext_ip);
    }

    #[cfg_attr(
        feature = "instrument",
        instrument(
            level = "trace",
            target = "net",
            name = "IGDManager::tick",
            skip_all,
            err,
            fields(__VEILID_LOG_KEY = self.log_key())
        )
    )]
    pub async fn tick(&self) -> EyreResult<IgdTickOutcome> {
        // Refresh mappings if we have them
        let mut full_renews: Vec<(PortMapKey, PortMapValue)> = Vec::new();
        let mut renews: Vec<(PortMapKey, PortMapValue)> = Vec::new();
        let cache_generation = {
            let inner = self.inner.lock();
            let now = Timestamp::now();

            for (k, v) in &inner.port_maps {
                let mapping_lifetime = now.duration_since(v.timestamp);
                if mapping_lifetime >= UPNP_MAPPING_LIFETIME
                    || v.renewal_attempts >= UPNP_MAPPING_ATTEMPTS
                {
                    // Past expiration time or tried N times, do a full renew and fail out if we can't
                    full_renews.push((*k, *v));
                } else if mapping_lifetime >= v.renewal_lifetime {
                    // Attempt a normal renewal
                    renews.push((*k, *v));
                }
            }

            // See if any mapping is due
            if full_renews.is_empty() && renews.is_empty() {
                // Just return now since there's nothing to renew
                return Ok(IgdTickOutcome::default());
            }
            inner.cache_generation
        };

        let mut outcome = IgdTickOutcome::default();

        // Process full renewals
        for (k, v) in full_renews {
            let Some((local_ip, gw)) = self.renewal_route(cache_generation, k, &mut outcome) else {
                continue;
            };

            let desc = self.get_description(k.protocol_type, k.local_port);
            let local_addr = SocketAddr::new(local_ip, k.local_port);
            let lease_secs = UPNP_MAPPING_LIFETIME
                .millis_u32()
                .unwrap_or_log()
                .div_ceil(1000);
            // A lapsed lease is usually still free, so ask for the published port first
            let same_port = v.renewal_attempts < UPNP_MAPPING_ATTEMPTS
                && match gw
                    .add_port(
                        convert_protocol_type(k.protocol_type),
                        v.mapped_port,
                        local_addr,
                        lease_secs,
                        &desc,
                    )
                    .await
                {
                    Ok(()) => true,
                    Err(e) => {
                        veilid_log!(self debug "could not re-add mapped port {:?} -> {:?}: {}", v, k, e);
                        false
                    }
                };
            let mapped_port = if same_port {
                v.mapped_port
            } else {
                // Delete the mapping if it exists, ignore any errors here
                let _ = gw
                    .remove_port(convert_protocol_type(k.protocol_type), v.mapped_port)
                    .await;
                self.store_port_map(cache_generation, k, None);
                match gw
                    .add_any_port(
                        convert_protocol_type(k.protocol_type),
                        local_addr,
                        lease_secs,
                        &desc,
                    )
                    .await
                {
                    Ok(mapped_port) => mapped_port,
                    Err(e) => {
                        veilid_log!(self info "failed to full-renew mapped port {:?} -> {:?}: {}", v, k, e);

                        // Must restart network now :(
                        outcome.needs_restart = true;
                        return Ok(outcome);
                    }
                }
            };
            if mapped_port != v.mapped_port {
                outcome.remapped = true;
            }
            veilid_log!(self debug "full-renewed mapped port {:?} -> {:?} on external port {}", v, k, mapped_port);
            self.store_port_map(
                cache_generation,
                k,
                Some(PortMapValue {
                    ext_ip: v.ext_ip,
                    mapped_port,
                    timestamp: Timestamp::now(),
                    renewal_lifetime: UPNP_MAPPING_LIFETIME.div(2),
                    renewal_attempts: 0,
                }),
            );
        }
        // Process normal renewals
        for (k, mut v) in renews {
            let Some((local_ip, gw)) = self.renewal_route(cache_generation, k, &mut outcome) else {
                continue;
            };

            let desc = self.get_description(k.protocol_type, k.local_port);
            match gw
                .add_port(
                    convert_protocol_type(k.protocol_type),
                    v.mapped_port,
                    SocketAddr::new(local_ip, k.local_port),
                    UPNP_MAPPING_LIFETIME
                        .millis_u32()
                        .unwrap_or_log()
                        .div_ceil(1000),
                    &desc,
                )
                .await
            {
                Ok(()) => {
                    veilid_log!(self trace "renewed mapped port {:?} -> {:?}", v, k);

                    self.store_port_map(
                        cache_generation,
                        k,
                        Some(PortMapValue {
                            ext_ip: v.ext_ip,
                            mapped_port: v.mapped_port,
                            timestamp: Timestamp::now(),
                            renewal_lifetime: UPNP_MAPPING_LIFETIME.div(2),
                            renewal_attempts: 0,
                        }),
                    );
                }
                Err(e) => {
                    veilid_log!(self debug "failed to renew mapped port {:?} -> {:?}: {}", v, k, e);

                    // Get closer to the maximum renewal timeline by a factor of two each time
                    v.renewal_lifetime =
                        (v.renewal_lifetime.saturating_add(UPNP_MAPPING_LIFETIME)).div(2);
                    v.renewal_attempts += 1;

                    // Store new value to try again
                    self.store_port_map(cache_generation, k, Some(v));
                }
            };
        }

        Ok(outcome)
    }

    /////////////////////////////////////////////////////////////////////
    // Private Implementation

    #[cfg_attr(
        feature = "instrument",
        instrument(level = "trace", target = "net", skip_all, fields(__VEILID_LOG_KEY = self.log_key()))
    )]
    fn get_routed_local_ip_address(&self, address_type: IGDAddressType) -> Option<IpAddr> {
        let socket = match UdpSocket::bind(match address_type {
            IGDAddressType::IPV4 => SocketAddr::new(IpAddr::V4(Ipv4Addr::UNSPECIFIED), 0),
            IGDAddressType::IPV6 => SocketAddr::new(IpAddr::V6(Ipv6Addr::UNSPECIFIED), 0),
        }) {
            Ok(s) => s,
            Err(e) => {
                veilid_log!(self debug "failed to bind to unspecified address: {}", e);
                return None;
            }
        };

        // can be any routable ip address,
        // this is just to make the system routing table calculate the appropriate local ip address
        // using google's dns, but it wont actually send any packets to it
        socket
            .connect(match address_type {
                IGDAddressType::IPV4 => SocketAddr::new(IpAddr::V4(Ipv4Addr::new(8, 8, 8, 8)), 80),
                IGDAddressType::IPV6 => SocketAddr::new(
                    IpAddr::V6(Ipv6Addr::new(0x2001, 0x4860, 0x4860, 0, 0, 0, 0, 0x8888)),
                    80,
                ),
            })
            .map_err(|e| {
                veilid_log!(self debug "failed to connect to dummy address: {}", e);
                e
            })
            .ok()?;

        Some(socket.local_addr().ok()?.ip())
    }

    /// Read the routed local IP from the cache; `None` if unprobed or no routable IP
    fn find_local_ip_inner(
        inner: &IGDManagerInner,
        address_type: IGDAddressType,
    ) -> Option<IpAddr> {
        inner.local_ip_addrs.get(&address_type).copied().flatten()
    }

    /// Read the IGD gateway from the cache; `None` if unprobed or no gateway
    fn find_gateway_inner(inner: &IGDManagerInner, local_ip: IpAddr) -> Option<Arc<Gateway>> {
        inner.gateways.get(&local_ip).cloned().flatten()
    }

    /// Read the external IP from the cache; `None` if unprobed or no external IP
    fn find_external_ip_inner(inner: &IGDManagerInner, local_ip: IpAddr) -> Option<IpAddr> {
        inner.external_ips.get(&local_ip).copied().flatten()
    }

    /// Whether the cache holds a gateway that reports an external IP of this address type
    fn can_map_inner(inner: &IGDManagerInner, address_type: IGDAddressType) -> bool {
        let Some(local_ip) = Self::find_local_ip_inner(inner, address_type) else {
            return false;
        };
        if Self::find_gateway_inner(inner, local_ip).is_none() {
            return false;
        }
        Self::find_external_ip_inner(inner, local_ip).is_some_and(|ext_ip| match address_type {
            IGDAddressType::IPV4 => ext_ip.is_ipv4(),
            IGDAddressType::IPV6 => ext_ip.is_ipv6(),
        })
    }

    /// The routed local IP and its gateway for an address type, from the cache
    fn find_route(&self, address_type: IGDAddressType) -> EyreResult<(IpAddr, Arc<Gateway>)> {
        let inner = self.inner.lock();
        let local_ip = Self::find_local_ip_inner(&inner, address_type)
            .ok_or_else(|| eyre!("local ip missing for address type"))?;
        let gw = Self::find_gateway_inner(&inner, local_ip)
            .ok_or_else(|| eyre!("gateway missing for interface"))?;
        Ok((local_ip, gw))
    }

    /// The route to renew a mapping through once its gateway search has finished. A mapping
    /// with no gateway found, such as one restored onto a network whose router stopped
    /// answering, is forgotten and reported lost.
    fn renewal_route(
        &self,
        cache_generation: u64,
        key: PortMapKey,
        outcome: &mut IgdTickOutcome,
    ) -> Option<(IpAddr, Arc<Gateway>)> {
        if self
            .inner
            .lock()
            .pending_probes
            .contains_key(&key.address_type)
        {
            return None;
        }
        match self.find_route(key.address_type) {
            Ok(route) => Some(route),
            Err(e) => {
                veilid_log!(self debug "no gateway to renew mapped port {:?}: {}", key, e);
                if self.store_port_map(cache_generation, key, None) {
                    outcome.lost = true;
                }
                None
            }
        }
    }

    /// Store a mapping, or forget it with `None`; false if the caches were cleared since
    /// `cache_generation` was read
    fn store_port_map(
        &self,
        cache_generation: u64,
        key: PortMapKey,
        value: Option<PortMapValue>,
    ) -> bool {
        let mut inner = self.inner.lock();
        if inner.cache_generation != cache_generation {
            return false;
        }
        match value {
            Some(value) => {
                inner.port_maps.insert(key, value);
            }
            None => {
                inner.port_maps.remove(&key);
            }
        }
        true
    }

    fn get_description(&self, protocol_type: IGDProtocolType, local_port: u16) -> String {
        format!(
            "{} map {} for port {}",
            self.registry.program_name(),
            protocol_type,
            local_port
        )
    }
}
