mod discovery_context;
mod igd_manager;
mod low_level_protocol_tcp;
mod low_level_protocol_udp;
mod mtu_detection;
mod network_state;
mod protocol;
mod restore;
mod start_protocols;
mod tasks;

pub(super) use protocol::*;

use super::*;
use discovery_context::*;
use low_level_protocol_tcp::*;
use low_level_protocol_udp::UdpListenerCommand;
use mtu_detection::*;
use network_state::*;
use protocol::tcp::RawTcpProtocolHandler;
use protocol::udp::RawUdpProtocolHandler;
use protocol::ws::WebsocketProtocolHandler;
use start_protocols::*;

use futures_rustls::{
    pki_types::{
        pem::PemObject as _, CertificateDer, PrivateKeyDer, PrivatePkcs1KeyDer, PrivatePkcs8KeyDer,
    },
    rustls::server::ServerConfig,
    TlsAcceptor,
};
use futures_util::StreamExt;
use std::fs::File;
use std::io;
use std::io::BufReader;
use std::path::{Path, PathBuf};

/////////////////////////////////////////////////////////////////

impl_veilid_log_facility!("net");

pub const MAX_DIAL_INFO_CONFIRM_FAILURE_COUNT: usize = 3;
pub const UPDATE_DIAL_INFO_TASK_TICK_PERIOD_SECS: u32 = 1;
pub const NETWORK_INTERFACES_TASK_TICK_PERIOD_SECS: u32 = 1;
pub const UPNP_TASK_TICK_PERIOD_SECS: u32 = 1;
pub const MTU_DETECTION_TASK_TICK_PERIOD_SECS: u32 = 1;

/// Native network durations derived from config
#[derive(Clone, Copy, Debug)]
pub(crate) struct NativeNetworkTimings {
    /// Bound on one send, in milliseconds: the smaller of the RPC and single-send timeouts
    pub send_timeout_ms: u32,
    /// Pause after a failed accept or receive, in milliseconds, so a broken socket costs a timer and not a core: 2% of the RPC timeout
    pub socket_error_backoff_ms: u32,
    /// Time to deliver a dial info validation request before the receipt wait: 75% of a send
    pub validate_dial_info_send_budget: TimestampDuration,
    /// Backstop on a detection pass, whose steps carry their own timeouts: twelve RPC timeouts
    pub dial_info_detection_timeout: TimestampDuration,
}

impl NativeNetworkTimings {
    pub fn new(config: &VeilidConfig) -> Self {
        let rpc_timeout_ms = config.internal().network.rpc.timeout_ms;
        let single_send_timeout_ms = config.internal().network.single_send_timeout_ms;
        let single_send_timeout = TimestampDuration::new_ms(single_send_timeout_ms.into());
        Self {
            send_timeout_ms: rpc_timeout_ms.min(single_send_timeout_ms),
            socket_error_backoff_ms: rpc_timeout_ms.saturating_mul(2) / 100,
            validate_dial_info_send_budget: single_send_timeout.saturating_mul(75).div(100),
            dial_info_detection_timeout: config.rpc_timeout().saturating_mul(12),
        }
    }
}
pub const PEEK_DETECT_LEN: usize = 64;

/// Our TCP size in `addr`'s family, which only Linux announces in the handshake
fn own_tcp_mss(registry: &VeilidComponentRegistry, addr: SocketAddr) -> u32 {
    let address_type = if addr.is_ipv4() {
        AddressType::IPV4
    } else {
        AddressType::IPV6
    };
    registry
        .network_manager()
        .own_mtus()
        .get(LowLevelTransportType {
            low_level_protocol_type: LowLevelProtocolType::TCP,
            address_type,
        })
        .into()
}

/////////////////////////////////////////////////////////////////

struct NetworkInner {
    /// set if the network needs to be restarted due to a low level configuration change
    /// such as dhcp release or change of address or interfaces being added or removed
    network_needs_restart: bool,
    /// the number of consecutive dial info confirmation failures per routing domain,
    /// used to avoid unpublishing peer info after a single transient discovery failure
    dial_info_confirm_failure_count: BTreeMap<RoutingDomain, usize>,
    /// validator pool size (capped at the full quorum) seen at the last public internet
    /// dial info confirmation attempt; a below-quorum confirmation re-tests as this grows
    confirmed_validator_count: Option<usize>,
    /// external addresses and unexamined transports of the last public internet detection
    last_dial_info_detection: Option<(BTreeSet<Address>, BTreeSet<TransportType>)>,
    /// result of resolving 'auto'/None detect_address_changes mode
    routing_domain_detect_address_changes: BTreeSet<RoutingDomain>,
    /// join handles for all the low level network background tasks
    join_handles: Vec<MustJoinHandle<()>>,
    /// stop source for shutting down the low level network background tasks
    stop_source: Option<StopSource>,
    /// Actual bound addresses per protocol
    bound_address_per_protocol: BTreeMap<ProtocolType, Vec<SocketAddr>>,
    /// mapping of protocol handlers to accept or send messages from a set of bound socket addresses
    udp_protocol_handlers: BTreeMap<SocketAddr, RawUdpProtocolHandler>,
    /// One sender per UDP listener task, used to add dynamically-bound handlers at runtime
    udp_listener_command_txs: Vec<flume::Sender<UdpListenerCommand>>,
    /// TLS handling socket controller
    tls_acceptor: Option<TlsAcceptor>,
    /// Multiplexer record for protocols on low level TCP sockets
    listener_states: BTreeMap<SocketAddr, Arc<RwLock<ListenerState>>>,
    /// Preferred local addresses for transports for outgoing connections
    preferred_local_addresses: BTreeMap<TransportType, SocketAddr>,
    /// UPnP mapper, present while the network is up with `network.upnp` on
    igd_manager: Option<igd_manager::IGDManager>,
    /// Network state
    network_state: Option<Arc<NetworkState>>,
    /// where each in-use address family's packet size stands
    family_mtus: BTreeMap<AddressType, FamilyMtu>,
    /// passes of mtu probes handed to the processor while the network is up
    mtu_probe_passes: Option<flume::Sender<MtuProbePass<MtuProbeCandidate>>>,
    /// what finished passes showed, read by the tick
    mtu_probe_results: Option<flume::Receiver<MtuProbePassResult>>,
    /// handles on the TCP listeners, to change the segment size they announce
    listener_mss_handles: BTreeMap<SocketAddr, TcpListenerMssHandle>,
}

pub(super) struct NetworkUnlockedInner {
    // Startup lock
    startup_lock: StartupLock,

    // Network
    interfaces: NetworkInterfaces,
    // Bumped whenever the interface address state changes
    interface_address_generation: AtomicU64,
    // Generation the public internet dial info was last confirmed on; 0 = never
    confirmed_interface_address_generation: AtomicU64,
    // Generation an emptied connection table last asked to confirm
    empty_table_requested_generation: AtomicU64,

    // Background processes
    confirm_dial_info_tasks: BTreeMap<RoutingDomain, TickTask<EyreReport>>,
    network_interfaces_task: TickTask<EyreReport>,
    upnp_task: TickTask<EyreReport>,
    mtu_detection_task: TickTask<EyreReport>,
    network_task_lock: AsyncRwLock<()>,

    // Durations derived from config
    timings: NativeNetworkTimings,
}

#[derive(Clone)]
pub(super) struct NativeNetwork {
    registry: VeilidComponentRegistry,
    inner: Arc<Mutex<NetworkInner>>,
    unlocked_inner: Arc<NetworkUnlockedInner>,
}

impl_veilid_component_accessors!(NativeNetwork);

impl core::ops::Deref for NativeNetwork {
    type Target = NetworkUnlockedInner;

    fn deref(&self) -> &Self::Target {
        &self.unlocked_inner
    }
}

impl PlatformNetwork for NativeNetwork {
    #[cfg_attr(feature = "instrument", instrument(level = "debug", err, skip_all, fields(__VEILID_LOG_KEY = self.log_key())))]
    fn startup(&self) -> PinBoxFuture<'_, EyreResult<StartupDisposition>> {
        Box::pin(async move {
            let guard = self.startup_lock.startup()?;

            match self.startup_internal().await {
                Ok(StartupDisposition::Success) => {
                    veilid_log!(self debug "Network started");
                    guard.success();

                    // Warn if we have no public dialinfo, because we're not going to magically find some
                    // with detect_address_changes turned off. Skip the warning if require_inbound_relay is
                    // enabled, this option intentionally disables publishing any dialinfo.
                    let network_state = self.last_network_state().unwrap_or_log();
                    if !self
                        .routing_domains_detecting_address_changes()
                        .contains(&RoutingDomain::PublicInternet)
                        && !network_state
                            .has_any_static_dial_info_details(RoutingDomain::PublicInternet)
                        && !network_state
                            .has_any_interface_dial_info_details(RoutingDomain::PublicInternet)
                        && !self.config().network.privacy.require_inbound_relay
                    {
                        veilid_log!(self warn
                            "This node has no valid PublicInternet dial info. Configure this node with a static public IP address and correct firewall rules."
                        );
                    }

                    Ok(StartupDisposition::Success)
                }
                Ok(StartupDisposition::BindRetry) => {
                    debug!("network bind retry");
                    self.shutdown_internal().await;
                    Ok(StartupDisposition::BindRetry)
                }
                Err(e) => {
                    debug!("network failed to start");
                    self.shutdown_internal().await;
                    Err(e)
                }
            }
        })
    }

    fn needs_restart(&self) -> bool {
        self.inner.lock().network_needs_restart
    }

    fn is_started(&self) -> bool {
        self.startup_lock.is_started()
    }

    #[cfg_attr(feature = "instrument", instrument(level = "debug", skip_all, fields(__VEILID_LOG_KEY = self.log_key())))]
    fn restart_network(&self) {
        veilid_log!(self debug "restart_network() called externally, triggering network restart");
        self.inner.lock().network_needs_restart = true;
    }

    #[cfg_attr(feature = "instrument", instrument(level = "debug", skip_all, fields(__VEILID_LOG_KEY = self.log_key())))]
    fn shutdown(&self) -> PinBoxFuture<'_, ()> {
        Box::pin(async move {
            veilid_log!(self debug "starting low level network shutdown");
            let Ok(guard) = self.startup_lock.shutdown().await else {
                // Startup never reached the success state (e.g. bind retry tore it down already)
                veilid_log!(self debug "low level network was not started, nothing to shut down");
                return;
            };

            self.shutdown_internal().await;

            guard.success();
            veilid_log!(self debug "finished low level network shutdown");
        })
    }

    fn tick(&self) -> PinBoxFuture<'_, EyreResult<()>> {
        Box::pin(NativeNetwork::tick(self))
    }

    fn cancel_tasks(&self) -> PinBoxFuture<'_, ()> {
        Box::pin(NativeNetwork::cancel_tasks(self))
    }

    fn connect(
        &self,
        local_address: Option<SocketAddr>,
        dial_info: DialInfo,
        timeout_ms: u32,
    ) -> PinBoxFuture<'_, EyreResult<NetworkResult<ProtocolNetworkConnection>>> {
        Box::pin(async move {
            let network_manager = self.network_manager();
            let address_filter = network_manager.address_filter();
            if address_filter.is_ip_addr_punished(dial_info.address().ip_addr()) {
                return Ok(NetworkResult::no_connection_other("punished"));
            }
            // Debug availability override: fail like the network broke it
            if network_manager.transport_stats().is_transport_disabled(
                dial_info.protocol_type().low_level_protocol_type(),
                dial_info.address_type(),
            ) {
                return Ok(NetworkResult::no_connection_other("transport disabled"));
            }
            NativeProtocolNetworkConnection::connect(
                self.registry(),
                local_address,
                dial_info,
                timeout_ms,
            )
            .await
            .wrap_err("io error in connect")
        })
    }

    #[cfg_attr(feature = "instrument", instrument(level="trace", target="net", err, skip(self, data), fields(data.len = data.len())))]
    fn send_data_unbound_to_dial_info(
        &self,
        dial_info: DialInfo,
        data: Bytes,
        mtu: u16,
    ) -> PinBoxFuture<'_, EyreResult<NetworkAttempt<()>>> {
        Box::pin(async move {
            let _guard = self.startup_lock.enter()?;

            let data_len = data.len();
            let connect_timeout_ms = self
                .config()
                .internal()
                .network
                .connection_initial_timeout_ms;

            let ip_addr = dial_info.address().ip_addr();

            if self
                .network_manager()
                .address_filter()
                .is_ip_addr_punished(ip_addr)
            {
                return Ok(NetworkAttempt::Performed(
                    NetworkResult::no_connection_other("punished"),
                ));
            }

            // Unbound sends have no contact method selection to steer around a suppressed transport
            if self
                .network_manager()
                .transport_stats()
                .unbound_send_deprioritized(&dial_info)
            {
                return Ok(NetworkAttempt::Skipped);
            }

            // The address names the routing domain, so nodeless traffic still counts
            let stats_dial_info = dial_info.clone();
            let send_result: EyreResult<NetworkResult<()>> = async {
                match dial_info.protocol_type() {
                    ProtocolType::UDP => {
                        let peer_socket_addr = dial_info.to_socket_addr();
                        let h = RawUdpProtocolHandler::try_new_unspecified_bound_handler(
                            self.registry(),
                            &peer_socket_addr,
                            self.timings(),
                        )
                        .wrap_err("create socket failure")?;
                        network_result_try!(h
                            .send_message(data, peer_socket_addr, mtu, "unbound")
                            .await
                            .wrap_err("send message failure")?);
                    }
                    ProtocolType::TCP => {
                        let peer_socket_addr = dial_info.to_socket_addr();
                        let pnc = network_result_try!(RawTcpProtocolHandler::connect(
                            self.registry(),
                            None,
                            peer_socket_addr,
                            connect_timeout_ms
                        )
                        .await
                        .wrap_err("connect failure")?);
                        network_result_try!(pnc.send(data, mtu).await.wrap_err("send failure")?);
                    }
                    ProtocolType::WS => {
                        let pnc = network_result_try!(WebsocketProtocolHandler::connect(
                            self.registry(),
                            None,
                            dial_info,
                            connect_timeout_ms
                        )
                        .await
                        .wrap_err("connect failure")?);
                        network_result_try!(pnc.send(data, mtu).await.wrap_err("send failure")?);
                    }
                    #[cfg(feature = "enable-protocol-wss")]
                    ProtocolType::WSS => {
                        let pnc = network_result_try!(WebsocketProtocolHandler::connect(
                            self.registry(),
                            None,
                            dial_info,
                            connect_timeout_ms
                        )
                        .await
                        .wrap_err("connect failure")?);
                        network_result_try!(pnc.send(data, mtu).await.wrap_err("send failure")?);
                    }
                }
                Ok(NetworkResult::Value(()))
            }
            .await;

            if let Some(outcome) = send_result
                .as_ref()
                .ok()
                .and_then(TransportSendOutcome::from_network_result)
            {
                self.network_manager()
                    .transport_stats()
                    .record_unbound_send(&stats_dial_info, outcome);
            }
            let nr = send_result?;
            if !matches!(nr, NetworkResult::Value(_)) {
                return Ok(NetworkAttempt::Performed(nr));
            }

            // Network accounting
            self.network_manager()
                .stats_packet_sent(ip_addr, ByteCount::new(data_len as u64));

            Ok(NetworkAttempt::Performed(NetworkResult::Value(())))
        })
    }

    #[cfg_attr(feature = "instrument", instrument(level="trace", target="net", err, skip(self, data), fields(data.len = data.len())))]
    fn send_recv_data_unbound_to_dial_info(
        &self,
        dial_info: DialInfo,
        data: Bytes,
        mtu: u16,
        timeout_ms: u32,
    ) -> PinBoxFuture<'_, EyreResult<NetworkAttempt<Bytes>>> {
        Box::pin(async move {
            let _guard = self.startup_lock.enter()?;

            let data_len = data.len();
            let network_manager = self.network_manager();
            let connect_timeout_ms = self
                .config()
                .internal()
                .network
                .connection_initial_timeout_ms;

            let ip_addr = dial_info.address().ip_addr();
            if network_manager
                .address_filter()
                .is_ip_addr_punished(ip_addr)
            {
                return Ok(NetworkAttempt::Performed(
                    NetworkResult::no_connection_other("punished"),
                ));
            }

            // Unbound sends have no contact method selection to steer around a suppressed transport
            if network_manager
                .transport_stats()
                .unbound_send_deprioritized(&dial_info)
            {
                return Ok(NetworkAttempt::Skipped);
            }

            let result: EyreResult<NetworkResult<Bytes>> = async {
                match dial_info.protocol_type() {
                    // Connectionless protocols
                    ProtocolType::UDP => {
                        let peer_socket_addr = dial_info.to_socket_addr();
                        let h = RawUdpProtocolHandler::try_new_unspecified_bound_handler(
                            self.registry(),
                            &peer_socket_addr,
                            self.timings(),
                        )
                        .wrap_err("create socket failure")?;
                        // Attribute the send only; a missing reply is the peer's silence
                        let send_res = h
                            .send_message(data, peer_socket_addr, mtu, "unbound_recv")
                            .await
                            .wrap_err("send message failure")?;
                        if let Some(outcome) = TransportSendOutcome::from_network_result(&send_res)
                        {
                            network_manager
                                .transport_stats()
                                .record_unbound_send(&dial_info, outcome);
                        }
                        network_result_try!(send_res);
                        network_manager.stats_packet_sent(ip_addr, ByteCount::new(data_len as u64));

                        // receive single response
                        let mut out = BytesMut::zeroed(MAX_MESSAGE_SIZE);
                        let (recv_len, recv_addr, _) = network_result_try!(timeout(
                            timeout_ms,
                            h.recv_message(&mut out).in_current_span()
                        )
                        .await
                        .into_network_result())
                        .wrap_err("recv_message failure")?;

                        let recv_socket_addr = recv_addr.remote_address().socket_addr();
                        network_manager.stats_packet_rcvd(
                            recv_socket_addr.ip(),
                            ByteCount::new(recv_len as u64),
                        );

                        // if the from address is not the same as the one we sent to, then drop this
                        if recv_socket_addr != peer_socket_addr {
                            bail!("wrong address");
                        }
                        out.resize(recv_len, 0u8);
                        Ok(NetworkResult::Value(out.into()))
                    }
                    // Connection-oriented protocols
                    _ => {
                        let stats_dial_info = dial_info.clone();
                        let connect_res = match dial_info.protocol_type() {
                            ProtocolType::UDP => {
                                bail!("UDP handled by connectionless arm");
                            }
                            ProtocolType::TCP => {
                                let peer_socket_addr = dial_info.to_socket_addr();
                                RawTcpProtocolHandler::connect(
                                    self.registry(),
                                    None,
                                    peer_socket_addr,
                                    connect_timeout_ms,
                                )
                                .await
                                .wrap_err("connect failure")?
                            }
                            ProtocolType::WS => WebsocketProtocolHandler::connect(
                                self.registry(),
                                None,
                                dial_info,
                                connect_timeout_ms,
                            )
                            .await
                            .wrap_err("connect failure")?,
                            #[cfg(feature = "enable-protocol-wss")]
                            ProtocolType::WSS => WebsocketProtocolHandler::connect(
                                self.registry(),
                                None,
                                dial_info,
                                connect_timeout_ms,
                            )
                            .await
                            .wrap_err("connect failure")?,
                        };
                        // Attribute connect+send only; a missing reply is the peer's silence
                        let pnc = match connect_res {
                            NetworkResult::Value(pnc) => pnc,
                            nr => {
                                if let Some(outcome) =
                                    TransportSendOutcome::from_network_result(&nr)
                                {
                                    network_manager
                                        .transport_stats()
                                        .record_unbound_send(&stats_dial_info, outcome);
                                }
                                // PANIC-CENSUS: provably-dead the non-Value arm carries no value to map
                                return Ok(nr.map(|_| unreachable!()));
                            }
                        };
                        let send_res = pnc.send(data, mtu).await.wrap_err("send failure")?;
                        if let Some(outcome) = TransportSendOutcome::from_network_result(&send_res)
                        {
                            network_manager
                                .transport_stats()
                                .record_unbound_send(&stats_dial_info, outcome);
                        }
                        network_result_try!(send_res);
                        network_manager.stats_packet_sent(ip_addr, ByteCount::new(data_len as u64));

                        let out = network_result_try!(network_result_try!(timeout(
                            timeout_ms,
                            pnc.recv().in_current_span()
                        )
                        .await
                        .into_network_result())
                        .wrap_err("recv failure")?);

                        network_manager
                            .stats_packet_rcvd(ip_addr, ByteCount::new(out.len() as u64));

                        Ok(NetworkResult::Value(out))
                    }
                }
            }
            .await;

            Ok(NetworkAttempt::Performed(result?))
        })
    }

    #[cfg_attr(feature = "instrument", instrument(level="trace", target="net", err, skip(self, data), fields(data.len = data.len())))]
    fn send_data_to_existing_flow(
        &self,
        flow: Flow,
        data: Bytes,
        mtu: u16,
    ) -> PinBoxFuture<'_, EyreResult<NetworkAttempt<UniqueFlow>>> {
        Box::pin(async move {
            let _guard = self.startup_lock.enter()?;
            let network_manager = self.network_manager();

            let data_len = data.len();

            // Handle connectionless protocol
            if flow.protocol_type() == ProtocolType::UDP {
                // send over the best udp socket we have bound since UDP is not connection oriented
                let peer_socket_addr = flow.remote().socket_addr();
                if let Some(ph) = self.find_best_udp_protocol_handler(
                    &peer_socket_addr,
                    &flow.local().map(|sa| sa.socket_addr()),
                ) {
                    match ph
                        .clone()
                        .send_message(data.clone(), peer_socket_addr, mtu, "existing_flow")
                        .await
                        .wrap_err("sending data to existing flow")?
                    {
                        NetworkResult::Value(_) => {}
                        // Wedged transport; dialing would repeat the identical send
                        NetworkResult::Timeout => {
                            return Ok(NetworkAttempt::Performed(NetworkResult::Timeout));
                        }
                        nres => {
                            veilid_log!(self debug throttle: PROTOCOL_LOG_THROTTLE_MS, target:"network_result",
                                "send_data_to_existing_flow: {:?}: data.len={}, flow={:?}",
                                nres, data.len(), flow
                            );
                            return Ok(NetworkAttempt::Skipped);
                        }
                    }

                    // Network accounting
                    network_manager
                        .stats_packet_sent(peer_socket_addr.ip(), ByteCount::new(data_len as u64));

                    // Data was consumed; UDP is connectionless so this is a dial
                    let unique_flow = UniqueFlow {
                        flow,
                        connection_id: None,
                        dialed: true,
                    };
                    return Ok(NetworkAttempt::value(unique_flow));
                }
            }

            // Handle connection-oriented protocols

            // Try to send to the exact existing connection if one exists
            if let Some(conn) = network_manager.connection_manager().get_connection(flow) {
                match timeout(self.single_send_timeout_ms(), conn.send_async(data, mtu)).await {
                    Ok(ConnectionHandleSendResult::Sent) => {
                        // Network accounting
                        network_manager.stats_packet_sent(
                            flow.remote().socket_addr().ip(),
                            ByteCount::new(data_len as u64),
                        );

                        // Data was consumed
                        return Ok(NetworkAttempt::value(conn.unique_flow()));
                    }
                    Ok(ConnectionHandleSendResult::NotSent) => {
                        return Ok(NetworkAttempt::Skipped);
                    }
                    Err(_) => {
                        veilid_log!(self debug throttle: PROTOCOL_LOG_THROTTLE_MS, "connection send timed out: flow={:?}", flow);
                        return Ok(NetworkAttempt::Performed(NetworkResult::Timeout));
                    }
                }
            }
            // Connection didn't exist
            Ok(NetworkAttempt::Skipped)
        })
    }

    #[cfg_attr(feature = "instrument", instrument(level="trace", target="net", err, skip(self, data), fields(data.len = data.len())))]
    fn send_data_to_dial_info(
        &self,
        dial_info: DialInfo,
        data: Bytes,
        mtu: u16,
    ) -> PinBoxFuture<'_, EyreResult<NetworkResult<UniqueFlow>>> {
        Box::pin(async move {
            let _guard = self.startup_lock.enter()?;

            let data_len = data.len();
            let network_manager = self.network_manager();

            let ip_addr = dial_info.address().ip_addr();

            let unique_flow = if dial_info.protocol_type() == ProtocolType::UDP {
                // Never send to one of our own addresses; it loops back to our own socket
                if network_manager
                    .routing_table()
                    .is_own_direct_dial_info_address(&dial_info.socket_address())
                {
                    return Ok(NetworkResult::no_connection_other(
                        "refusing to send to our own address",
                    ));
                }
                // Handle connectionless protocol
                let peer_socket_addr = dial_info.to_socket_addr();
                let preferred_local = self.preferred_outbound_source_addr(&dial_info);
                let ph = match self
                    .find_best_udp_protocol_handler(&peer_socket_addr, &preferred_local)
                {
                    Some(ph) => ph,
                    None => {
                        return Ok(NetworkResult::no_connection_other(
                            "no appropriate UDP protocol handler for dial_info",
                        ));
                    }
                };
                let flow = network_result_try!(ph
                    .send_message(data, peer_socket_addr, mtu, "dial_info")
                    .await
                    .wrap_err("failed to send data to dial info")?);
                UniqueFlow {
                    flow,
                    connection_id: None,
                    dialed: true,
                }
            } else {
                // Handle connection-oriented protocols
                let connmgr = network_manager.connection_manager();
                let conn = network_result_try!(connmgr.get_or_create_connection(dial_info).await?);

                match timeout(self.single_send_timeout_ms(), conn.send_async(data, mtu)).await {
                    Ok(ConnectionHandleSendResult::Sent) => {}
                    Ok(ConnectionHandleSendResult::NotSent) => {
                        return Ok(NetworkResult::NoConnection(io::Error::new(
                            io::ErrorKind::ConnectionReset,
                            "failed to send",
                        )));
                    }
                    Err(_) => {
                        veilid_log!(self debug throttle: PROTOCOL_LOG_THROTTLE_MS, "connection send timed out: dial_info={:?}", conn.unique_flow().flow);
                        return Ok(NetworkResult::timeout());
                    }
                }
                conn.unique_flow()
            };

            // Network accounting
            network_manager.stats_packet_sent(ip_addr, ByteCount::new(data_len as u64));

            Ok(NetworkResult::value(unique_flow))
        })
    }

    #[cfg_attr(
        feature = "instrument",
        instrument(level = "trace", target = "net", err, skip(self), fields(__VEILID_LOG_KEY = self.log_key()))
    )]
    fn send_hole_punch(
        &self,
        dial_info: DialInfo,
    ) -> PinBoxFuture<'_, EyreResult<NetworkResult<UniqueFlow>>> {
        Box::pin(async move {
            let _guard = self.startup_lock.enter()?;

            let network_manager = self.network_manager();
            let ip_addr = dial_info.address().ip_addr();

            if dial_info.protocol_type().low_level_protocol_type() != LowLevelProtocolType::UDP {
                return Ok(NetworkResult::ServiceUnavailable(
                    "unimplemented for this protocol".to_owned(),
                ));
            }

            // Handle connectionless protocol
            let peer_socket_addr = dial_info.to_socket_addr();
            // Prefer a source-bound handler for PublicInternet sends
            let preferred_local = self.preferred_outbound_source_addr(&dial_info);
            let ph = match self.find_best_udp_protocol_handler(&peer_socket_addr, &preferred_local)
            {
                Some(ph) => ph,
                None => {
                    return Ok(NetworkResult::no_connection_other(
                        "no appropriate UDP protocol handler for dial_info",
                    ));
                }
            };
            let ttl = hole_punch_ttl(
                &self.config().internal().network,
                &self.interfaces.interface_address_state(),
                dial_info.address_type(),
            );
            let flow = network_result_try!(ph
                .send_hole_punch(peer_socket_addr, ttl)
                .wrap_err("failed to send hole punch to dial info")?);
            let unique_flow = UniqueFlow {
                flow,
                connection_id: None,
                dialed: true,
            };

            // Network accounting
            network_manager.stats_packet_sent(ip_addr, ByteCount::new(0));

            Ok(NetworkResult::value(unique_flow))
        })
    }

    fn get_preferred_local_address(&self, dial_info: &DialInfo) -> Option<SocketAddr> {
        let inner = self.inner.lock();
        inner
            .preferred_local_addresses
            .get(&dial_info.transport_type())
            .copied()
    }

    fn get_preferred_local_address_by_key(&self, tt: TransportType) -> Option<SocketAddr> {
        let inner = self.inner.lock();
        inner.preferred_local_addresses.get(&tt).copied()
    }

    fn preferred_outbound_source_addr(&self, dial_info: &DialInfo) -> Option<SocketAddr> {
        // Only PublicInternet uses source-address binding
        let routing_domain = self
            .routing_table()
            .routing_domain_for_address(dial_info.address())?;
        if routing_domain != RoutingDomain::PublicInternet {
            return None;
        }

        let configured = self.get_preferred_local_address_by_key(dial_info.transport_type())?;

        // User pinned a specific listen address via config; use it as the outbound source
        if !configured.ip().is_unspecified() {
            return Some(configured);
        }

        // Default unspecified listener: pair the first publicly-routable interface
        // address of the matching family with the bound listen port
        let listen_port = configured.port();
        let interface_address_state = self.interfaces.interface_address_state();
        for ifaddr in interface_address_state.interface_addresses.iter() {
            let ip = ifaddr.ip();
            let matches_family = matches!(
                (ip, dial_info.address_type()),
                (IpAddr::V4(_), AddressType::IPV4) | (IpAddr::V6(_), AddressType::IPV6)
            );
            if !matches_family {
                continue;
            }
            if !Address::from_ip_addr(ip).is_global() {
                continue;
            }
            return Some(SocketAddr::new(ip, listen_port));
        }
        None
    }

    fn routing_domains_detecting_address_changes(&self) -> BTreeSet<RoutingDomain> {
        let inner = self.inner.lock();
        if inner.network_state.is_none() {
            veilid_log!(self debug "ignoring 'routing_domain_detect_address_changes' due to not started up or initialized yet");
            return BTreeSet::new();
        };

        inner.routing_domain_detect_address_changes.clone()
    }

    fn routing_domain_request_confirm_dial_info(&self, routing_domain: RoutingDomain) -> bool {
        let Ok(_guard) = self.startup_lock.enter() else {
            veilid_log!(self debug "ignoring 'routing_domain_request_confirm_dial_info' due to not started up");
            return false;
        };

        let routing_table = self.routing_table();
        let rdc = routing_table.get_routing_domain_controller(routing_domain);
        // Already unconfirmed; a detection cycle is already pending or in progress
        if !rdc.read_dyn().confirmed() {
            return false;
        }
        let mut editor = rdc.edit_dyn();
        editor.set_confirmed(false);
        editor.commit();
        rdc.discard_restore_point();
        true
    }

    fn request_confirm_after_empty_table(&self) -> bool {
        let generation = self.interface_address_generation.load(Ordering::Relaxed);
        if generation
            == self
                .confirmed_interface_address_generation
                .load(Ordering::Relaxed)
        {
            return false;
        }
        // A failed detection keeps the generation unconfirmed, so remember the one asked for
        if self
            .empty_table_requested_generation
            .swap(generation, Ordering::Relaxed)
            == generation
        {
            return false;
        }
        self.routing_domain_request_confirm_dial_info(RoutingDomain::PublicInternet)
    }

    fn restore_state(&self, routing_domain: RoutingDomain) -> Option<PlatformRestoreState> {
        self.routing_domain_restore_state(routing_domain)
    }

    fn last_dial_info_detection(&self) -> Option<(BTreeSet<Address>, BTreeSet<TransportType>)> {
        self.inner.lock().last_dial_info_detection.clone()
    }
}

impl NativeNetwork {
    /// Dial info is known good on the current interface state
    fn record_confirmed_interface_generation(&self) {
        self.confirmed_interface_address_generation.store(
            self.interface_address_generation.load(Ordering::Relaxed),
            Ordering::Relaxed,
        );
    }

    /// The UPnP mapper, while the network is up with `network.upnp` on
    fn igd_manager(&self) -> Option<igd_manager::IGDManager> {
        self.inner.lock().igd_manager.clone()
    }

    fn new_inner() -> NetworkInner {
        NetworkInner {
            network_needs_restart: false,
            dial_info_confirm_failure_count: BTreeMap::new(),
            confirmed_validator_count: None,
            last_dial_info_detection: None,
            routing_domain_detect_address_changes: BTreeSet::new(),
            join_handles: Vec::new(),
            stop_source: None,
            bound_address_per_protocol: BTreeMap::new(),
            udp_protocol_handlers: BTreeMap::new(),
            udp_listener_command_txs: Vec::new(),
            tls_acceptor: None,
            listener_states: BTreeMap::new(),
            preferred_local_addresses: BTreeMap::new(),
            igd_manager: None,
            network_state: None,
            family_mtus: BTreeMap::new(),
            mtu_probe_passes: None,
            mtu_probe_results: None,
            listener_mss_handles: BTreeMap::new(),
        }
    }

    fn new_unlocked_inner(registry: VeilidComponentRegistry) -> NetworkUnlockedInner {
        // Make a tick task for each routing domain
        let mut confirm_dial_info_tasks = BTreeMap::new();
        for routing_domain in RoutingDomain::all() {
            confirm_dial_info_tasks.insert(
                routing_domain,
                TickTask::new(
                    format!("confirm_dial_info_task_{}", routing_domain).to_static_str(),
                    UPDATE_DIAL_INFO_TASK_TICK_PERIOD_SECS,
                ),
            );
        }

        let config = registry.config();

        NetworkUnlockedInner {
            startup_lock: StartupLock::new(),
            interfaces: NetworkInterfaces::new(),
            interface_address_generation: AtomicU64::new(0),
            confirmed_interface_address_generation: AtomicU64::new(0),
            empty_table_requested_generation: AtomicU64::new(0),
            confirm_dial_info_tasks,
            network_interfaces_task: TickTask::new(
                "network_interfaces_task",
                NETWORK_INTERFACES_TASK_TICK_PERIOD_SECS,
            ),
            upnp_task: TickTask::new("upnp_task", UPNP_TASK_TICK_PERIOD_SECS),
            mtu_detection_task: TickTask::new(
                "mtu_detection_task",
                MTU_DETECTION_TASK_TICK_PERIOD_SECS,
            ),
            network_task_lock: AsyncRwLock::new(()),
            timings: NativeNetworkTimings::new(&config),
        }
    }

    pub fn new(registry: VeilidComponentRegistry) -> Self {
        let this = Self {
            inner: Arc::new(Mutex::new(Self::new_inner())),
            unlocked_inner: Arc::new(Self::new_unlocked_inner(registry.clone())),
            registry,
        };

        this.setup_tasks();

        this
    }

    /// Durations derived from config
    pub(super) fn timings(&self) -> &NativeNetworkTimings {
        &self.unlocked_inner.timings
    }

    /// Upper bound on one send; a stalled connection parks channel sends forever otherwise
    fn single_send_timeout_ms(&self) -> u32 {
        self.timings().send_timeout_ms
    }

    fn load_certs(path: &Path) -> io::Result<Vec<CertificateDer<'static>>> {
        let cvec = CertificateDer::<'static>::pem_reader_iter(&mut BufReader::new(
            // nosemgrep: rust.actix.path-traversal.tainted-path.tainted-path
            File::open(path)?,
        ))
        .collect::<Result<Vec<CertificateDer<'static>>, futures_rustls::pki_types::pem::Error>>()
        .map_err(io::Error::other)?;
        Ok(cvec)
    }

    fn load_keys(path: &Path) -> io::Result<Vec<PrivateKeyDer<'static>>> {
        {
            if let Ok(v) = PrivatePkcs1KeyDer::<'static>::pem_reader_iter(&mut BufReader::new(
                // nosemgrep: rust.actix.path-traversal.tainted-path.tainted-path
                File::open(path)?,
            ))
            .collect::<Result<Vec<PrivatePkcs1KeyDer<'static>>, futures_rustls::pki_types::pem::Error>>()
            {
                if !v.is_empty() {
                    return Ok(v
                        .into_iter()
                        .map(PrivateKeyDer::Pkcs1)
                        .collect::<Vec<PrivateKeyDer<'static>>>());
                }
            }
        }
        {
            if let Ok(v) = PrivatePkcs8KeyDer::<'static>::pem_reader_iter(&mut BufReader::new(
                // nosemgrep: rust.actix.path-traversal.tainted-path.tainted-path
                File::open(path)?,
            ))
            .collect::<Result<Vec<PrivatePkcs8KeyDer<'static>>,futures_rustls::pki_types::pem::Error>>()
            {
                if !v.is_empty() {
                    return Ok(v.into_iter().map(PrivateKeyDer::Pkcs8).collect());
                }
            }
        }

        Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "invalid TLS private key",
        ))
    }

    fn load_server_config(&self) -> io::Result<ServerConfig> {
        let config = self.config();
        //
        veilid_log!(self trace
            "loading certificate from {}",
            config.network.tls.certificate_path
        );
        let certs_path = PathBuf::from(&config.network.tls.certificate_path);
        let certs = Self::load_certs(&certs_path)?;
        veilid_log!(self trace "loaded {} certificates", certs.len());
        if certs.is_empty() {
            return Err(io::Error::new(io::ErrorKind::InvalidInput, format!("Certificates at {} could not be loaded.\nEnsure it is in PEM format, beginning with '-----BEGIN CERTIFICATE-----'",config.network.tls.certificate_path)));
        }
        //
        veilid_log!(self trace
            "loading private key from {}",
            config.network.tls.private_key_path
        );
        let keys_path = PathBuf::from(&config.network.tls.private_key_path);
        let mut keys = Self::load_keys(&keys_path)?;
        veilid_log!(self trace "loaded {} keys", keys.len());
        if keys.is_empty() {
            return Err(io::Error::new(io::ErrorKind::InvalidInput, format!("Private key at {} could not be loaded.\nEnsure it is unencrypted and in RSA or PKCS8 format, beginning with '-----BEGIN RSA PRIVATE KEY-----' or '-----BEGIN PRIVATE KEY-----'",config.network.tls.private_key_path)));
        }

        let config = ServerConfig::builder()
            .with_no_client_auth()
            .with_single_cert(certs, keys.remove(0))
            .map_err(|err| io::Error::new(io::ErrorKind::InvalidInput, err))?;

        Ok(config)
    }

    fn add_to_join_handles(&self, jh: MustJoinHandle<()>) {
        let mut inner = self.inner.lock();
        inner.join_handles.push(jh);
    }

    ////////////////////////////////////////////////////////////

    /////////////////////////////////////////////////////////////////

    /// Set up `routing_domain_detect_address_changes` based on the config and the initial network state
    ///
    /// Assigns the routing_domain_detect_address_changes settings and also returns it.
    fn set_up_detect_address_changes(
        &self,
        interface_address_state: &NetworkInterfaceAddressState,
    ) -> BTreeSet<RoutingDomain> {
        let config = self.config();
        // Resolve 'auto'/None config fo detect_address_changes
        let mut inner = self.inner.lock();

        // Get the address config because we'll need that for this autodetection
        let address_config = self.make_address_config();

        // Process the detect_address_changes 'auto' mode
        let detect_address_changes = config.network.detect_address_changes;
        let require_inbound_relay = config.network.privacy.require_inbound_relay;

        let enable_public_internet_detect_address_changes = if require_inbound_relay {
            veilid_log!(self info "Manually-disabled detection of PublicInternet address changes because 'network.private.require_inbound_relay' is 'true'");
            false
        } else if let Some(detect_address_changes) = detect_address_changes {
            if detect_address_changes {
                veilid_log!(self info "Manually-enabled detection of PublicInternet address changes because 'network.detect_address_changes' is 'true'");
            } else {
                veilid_log!(self info "Manually-disabled detection of PublicInternet address changes because 'network.detect_address_changes' is 'false'");
            }
            detect_address_changes
        } else {
            // Check for publicly routable IPv4 and IPv6 addresses on the local interfaces
            let mut global_ipv4_needed = address_config.ipv4_global;
            let mut global_ipv6_needed = address_config.ipv6_global;
            for intf_addr in interface_address_state.interface_addresses.iter() {
                if Address::from_ip_addr(intf_addr.ip()).is_global() {
                    match intf_addr {
                        IfAddr::V4(_) => {
                            global_ipv4_needed = false;
                        }
                        IfAddr::V6(_) => {
                            global_ipv6_needed = false;
                        }
                    }
                }
            }

            // If either IPV4 or IPV6 global static addresses are supported but not provided, then turn on detect_address_changes
            let enable_public_internet_detect_address_changes =
                global_ipv4_needed || global_ipv6_needed;

            // Print log message about this action
            let mut supports = Vec::new();
            if address_config.ipv4_global {
                supports.push("IPv4");
            }
            if address_config.ipv6_global {
                supports.push("IPv6");
            }

            let mut needed = Vec::new();
            if global_ipv4_needed {
                needed.push("IPv4");
            }
            if global_ipv6_needed {
                needed.push("IPv6");
            }

            if enable_public_internet_detect_address_changes {
                veilid_log!(self info "Auto-enabled detection of PublicInternet address changes because this node supports {}, but does not have publicly routable addresses for {}",
                    supports.join(" and "),
                    needed.join(" and "));
            } else {
                veilid_log!(self info "Auto-disabled detection of PublicInternet address changes because this node has globally routable addresses for {}",
                    supports.join(" and "));
            }
            enable_public_internet_detect_address_changes
        };

        // Enable detection of address changes for all routing domains
        for routing_domain in RoutingDomain::all() {
            if routing_domain == RoutingDomain::PublicInternet {
                if enable_public_internet_detect_address_changes {
                    inner
                        .routing_domain_detect_address_changes
                        .insert(routing_domain);
                }
                continue;
            }

            // By default, routing domains are checked for address changes
            inner
                .routing_domain_detect_address_changes
                .insert(routing_domain);
        }

        inner.routing_domain_detect_address_changes.clone()
    }

    fn update_public_internet_routing_domain(
        &self,
        opt_last_network_state: Option<Arc<NetworkState>>,
        new_network_state: Arc<NetworkState>,
    ) -> EyreResult<()> {
        let routing_table = self.routing_table();

        // Get routing domain and editor for PublicInternet-specific fields
        let controller_public_internet = routing_table
            .get_specific_routing_domain_controller::<PublicInternetRoutingDomainController>();
        let mut editor_public_internet = controller_public_internet.edit();

        // Set new interface addresses on routing domain
        let new_interface_addresses = new_network_state
            .interface_address_state
            .interface_addresses
            .clone();
        editor_public_internet.set_interface_addresses(new_interface_addresses);

        // Check if we are detecting address changes for this routing domain
        // Auto-confirm dial info if we are not detecting address changes and won't
        // be running the confirm dial info task for this routing domain
        let auto_confirm_dial_info = !self
            .routing_domains_detecting_address_changes()
            .contains(&RoutingDomain::PublicInternet);

        // Update generic routing domain network state
        self.update_routing_domain_network_state(
            &*controller_public_internet as &dyn RoutingDomainController,
            &mut editor_public_internet as &mut dyn RoutingDomainEditor,
            opt_last_network_state.clone(),
            new_network_state.clone(),
            auto_confirm_dial_info,
        )?;

        editor_public_internet.commit();
        if opt_last_network_state.is_none() {
            self.restore_routing_domain(&*controller_public_internet);
        }
        controller_public_internet.publish_peer_info();

        Ok(())
    }

    fn update_local_network_routing_domain(
        &self,
        opt_last_network_state: Option<Arc<NetworkState>>,
        new_network_state: Arc<NetworkState>,
    ) -> EyreResult<()> {
        let routing_table = self.routing_table();

        // Get routing domain and editor for LocalNetwork-specific fields
        let controller_local_network = routing_table
            .get_specific_routing_domain_controller::<LocalNetworkRoutingDomainController>();
        let mut editor_local_network = controller_local_network.edit();

        // Set interface addresses
        // (all addresses on all interfaces that could possibly participate in the PublicInternet routing domain)
        let new_interface_addresses = new_network_state
            .interface_address_state
            .interface_addresses
            .clone();
        editor_local_network.set_interface_addresses(new_interface_addresses);

        // Update generic routing domain network state
        self.update_routing_domain_network_state(
            &*controller_local_network as &dyn RoutingDomainController,
            &mut editor_local_network as &mut dyn RoutingDomainEditor,
            opt_last_network_state.clone(),
            new_network_state.clone(),
            true,
        )?;

        editor_local_network.commit();
        if opt_last_network_state.is_none() {
            self.restore_routing_domain(&*controller_local_network);
        }
        controller_local_network.publish_peer_info();

        Ok(())
    }

    fn update_routing_domain_network_state(
        &self,
        controller: &dyn RoutingDomainController,
        editor: &mut dyn RoutingDomainEditor,
        opt_last_network_state: Option<Arc<NetworkState>>,
        new_network_state: Arc<NetworkState>,
        auto_confirm_dial_info: bool,
    ) -> EyreResult<()> {
        let routing_domain = controller.routing_domain();

        // Check if routing domain network config has changed
        let opt_last_rd_network_state = if let Some(last_network_state) = &opt_last_network_state {
            let Some(last_rd_network_state) = last_network_state
                .routing_domain_network_states
                .get(&routing_domain)
            else {
                bail!("failed to get last network state routing domain network state");
            };
            Some(last_rd_network_state)
        } else {
            None
        };
        let Some(new_rd_network_state) = new_network_state
            .routing_domain_network_states
            .get(&routing_domain)
        else {
            bail!("failed to get new network state routing domain network state");
        };
        let rd_network_state_changed = opt_last_rd_network_state != Some(new_rd_network_state);

        // Our receive mtus ride every commit, so no publish goes out without them
        editor.set_receive_mtus(self.receive_mtus());

        // Update the routing domain details if the config or interface dialinfo details have changed
        if rd_network_state_changed {
            // Set network config
            editor.set_network_config(new_rd_network_state.routing_domain_network_config.clone());

            // Update routing domain from interface dial info details if they have changed
            if opt_last_rd_network_state.map(|x| &x.interface_dial_info_details)
                != Some(&new_rd_network_state.interface_dial_info_details)
            {
                // Add dialinfo details to routing domain
                editor.clear_dial_info_details(None, None);
                for did in new_rd_network_state.static_dial_info_details.iter() {
                    editor.add_dial_info_detail(did.clone());
                }
                for did in new_rd_network_state.interface_dial_info_details.iter() {
                    editor.add_dial_info_detail(did.clone());
                }
            }

            // Set confirmed dial info state if there is no other detection mechanism for this routing domain
            editor.set_confirmed(auto_confirm_dial_info);

            // What was published rests on the network state that just changed
            if opt_last_rd_network_state.is_some() && !auto_confirm_dial_info {
                controller.discard_restore_point();
            }
        }

        Ok(())
    }

    fn bind_all_listeners(&self) -> EyreResult<StartupDisposition> {
        let config = self.config();

        // Start listeners
        if config.network.protocol.udp.enabled {
            let res = self.bind_udp_protocol_handlers();
            if !matches!(res, Ok(StartupDisposition::Success)) {
                return res;
            }
        }
        if config.network.protocol.ws.listen {
            let res = self.start_ws_listeners();
            if !matches!(res, Ok(StartupDisposition::Success)) {
                return res;
            }
        }

        #[cfg(feature = "enable-protocol-wss")]
        if config.network.protocol.wss.listen {
            let res = self.start_wss_listeners();
            if !matches!(res, Ok(StartupDisposition::Success)) {
                return res;
            }
        }
        if config.network.protocol.tcp.listen {
            let res = self.start_tcp_listeners();
            if !matches!(res, Ok(StartupDisposition::Success)) {
                return res;
            }
        }
        Ok(StartupDisposition::Success)
    }

    pub async fn startup_internal(&self) -> EyreResult<StartupDisposition> {
        // Create the shutdown stop source before we bind the listeners
        {
            let mut inner = self.inner.lock();
            inner.stop_source = Some(StopSource::new());
        }
        self.start_mtu_probe_processor();

        // Made per attach so a config change applies at the next one, and a detached node
        // never emits SSDP; require_inbound_relay implies upnp off
        let igd_manager = {
            let config = self.config();
            (config.network.upnp && !config.network.privacy.require_inbound_relay)
                .then(|| igd_manager::IGDManager::new(self.registry()))
        };
        if let Some(igd_manager) = &igd_manager {
            igd_manager.startup();
        }
        self.inner.lock().igd_manager = igd_manager;

        // Bind all listening network protocols
        let res = self.bind_all_listeners();
        if !matches!(res, Ok(StartupDisposition::Success)) {
            return res;
        }

        // Get the initial network state snapshot (must return Some() the first time)
        // Caution: this -must- happen first because we use unwrap() later
        let Some(network_state) = self.refresh_network_state().await? else {
            veilid_log!(self error "No network state returned from initial refresh");
            bail!("No network state returned from initial refresh");
        };

        // Ensure at least one address mode is supported for PublicInternet routing domain
        if !network_state.address_config.ipv4_global && !network_state.address_config.ipv6_global {
            veilid_log!(self info "PublicInternet routing domain can not be started because neither IPV4 nor IPV6 are enabled");
            bail!("No PublicInternet routing domain support");
        }
        if !network_state.address_config.ipv4_local && !network_state.address_config.ipv6_local {
            veilid_log!(self info "LocalNetwork routing domain can not be started because neither IPV4 nor IPV6 are enabled");
            bail!("No LocalNetwork routing domain support");
        }

        // Set up each routing domain from the network state
        if let Err(e) = self.update_public_internet_routing_domain(None, network_state.clone()) {
            veilid_log!(self error "Failed to update PublicInternet routing domain: {}", e);
        }
        if let Err(e) = self.update_local_network_routing_domain(None, network_state.clone()) {
            veilid_log!(self error "Failed to update LocalNetwork routing domain: {}", e);
        }

        Ok(StartupDisposition::Success)
    }

    #[cfg_attr(feature = "instrument", instrument(level = "debug", skip_all, fields(__VEILID_LOG_KEY = self.log_key())))]
    async fn shutdown_internal(&self) {
        let mut unord = FuturesUnordered::new();
        {
            let mut inner = self.inner.lock();
            // take the join handles out
            for h in inner.join_handles.drain(..) {
                veilid_log!(self trace "joining: {:?}", h);
                unord.push(h);
            }
            // Wake source-bound recv-loops that follow per-handler stops (the global stop
            // doesn't reach them)
            for ph in inner.udp_protocol_handlers.values() {
                ph.stop();
            }
            // Drop senders so listener channel-wait futures observe ChannelClosed
            inner.udp_listener_command_txs.clear();
            drop(inner.stop_source.take());
        }
        veilid_log!(self debug "stopping {} low level network tasks", unord.len());
        while unord.next().await.is_some() {}

        let igd_manager = self.inner.lock().igd_manager.take();
        if let Some(igd_manager) = igd_manager {
            igd_manager.shutdown().await;
        }

        *self.inner.lock() = Self::new_inner();
    }
}
