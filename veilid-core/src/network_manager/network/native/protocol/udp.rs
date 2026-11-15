use super::*;
use stop_token::{StopSource, StopToken};

impl_veilid_log_facility!("net");

/// Consecutive failed receives before this handler gives up and reports an error upward.
///
/// `into_network_result` classifies host-unreachable, not-connected and friends as conditions
/// rather than errors, which is right for one packet and wrong for a socket that returns one
/// every time: an interface torn down under a bound socket leaves `recv_from` failing instantly
/// and permanently. Only an `Err` out of here reaches the listener's own consecutive-error
/// breaker and the network restart behind it.
const UDP_RECV_FAIL_AFTER_CONSECUTIVE_ERRORS: usize = 16;

/// A message held by debug chaos: its sender, and the datagram it arrived whole in, if one
type HeldMessage = (Bytes, SocketAddr, Option<u16>);

/// Received messages debug chaos holds until its emulated link has carried them
#[derive(Debug, Default)]
struct HeldMessages {
    /// Release time and message, in the order the link delivers them
    queue: Mutex<VecDeque<(Timestamp, HeldMessage)>>,
}

impl HeldMessages {
    fn push(&self, wait_ms: u32, held_message: HeldMessage) {
        let release_ts =
            Timestamp::now_non_decreasing().later(TimestampDuration::new_ms(wait_ms.into()));
        self.queue.lock().push_back((release_ts, held_message));
    }

    /// The oldest message, once it is due
    fn pop_due(&self) -> Option<HeldMessage> {
        let mut queue = self.queue.lock();
        if queue.front()?.0 > Timestamp::now_non_decreasing() {
            return None;
        }
        queue.pop_front().map(|(_, held_message)| held_message)
    }

    /// Milliseconds until the oldest message is due
    fn wait_ms(&self) -> Option<u32> {
        let release_ts = self.queue.lock().front()?.0;
        Some(
            release_ts
                .duration_since(Timestamp::now_non_decreasing())
                .millis_u32()
                .unwrap_or(u32::MAX),
        )
    }
}

#[derive(Clone)]
pub struct RawUdpProtocolHandler {
    /// Component registry for logging and access to other subsystems
    registry: VeilidComponentRegistry,
    /// Low-TTL sends for `socket`, declared before it so its twin closes first
    ttl_sender: Arc<UdpTtlSender>,
    /// Async UDP socket this handler reads from and writes to
    socket: Arc<UdpSocket>,
    /// Reassembly buffer for fragmented UDP messages
    assembly_buffer: AssemblyBuffer,
    /// Received messages debug chaos holds back, shared by every recv-loop on `socket`
    held: Arc<HeldMessages>,
    /// Bound on one fragmented send; a stalled interface parks send_to forever otherwise
    send_timeout_ms: u32,
    /// Pause between repeated receive failures; the first is ordinary ICMP noise and costs nothing
    recv_error_backoff_ms: u32,
    /// Drop to signal cancellation; taken out by `stop()`
    stop_source: Arc<Mutex<Option<StopSource>>>,
    /// Cloned by recv-loops that follow this handler's stop
    stop_token: StopToken,
}

impl_veilid_component_accessors!(RawUdpProtocolHandler);

impl RawUdpProtocolHandler {
    pub fn new(
        registry: VeilidComponentRegistry,
        socket: Arc<UdpSocket>,
        timings: &NativeNetworkTimings,
    ) -> Self {
        let stop_source = StopSource::new();
        let stop_token = stop_source.token();

        Self {
            registry,
            ttl_sender: Arc::new(UdpTtlSender::new()),
            socket,
            assembly_buffer: AssemblyBuffer::new(),
            held: Arc::new(HeldMessages::default()),
            send_timeout_ms: timings.send_timeout_ms,
            recv_error_backoff_ms: timings.socket_error_backoff_ms,
            stop_source: Arc::new(Mutex::new(Some(stop_source))),
            stop_token,
        }
    }

    /// Stop token for this handler's recv-loop
    pub fn stop_token(&self) -> StopToken {
        self.stop_token.clone()
    }

    /// Signal cancellation for this handler. Idempotent.
    pub fn stop(&self) {
        *self.stop_source.lock() = None;
    }

    /// Debug availability override: a suppressed transport must fail like the network broke it
    fn is_transport_disabled(&self, remote_addr: SocketAddr) -> bool {
        let address_type = if remote_addr.is_ipv6() {
            AddressType::IPV6
        } else {
            AddressType::IPV4
        };
        self.network_manager()
            .transport_stats()
            .is_transport_disabled(LowLevelProtocolType::UDP, address_type)
    }

    /// Receives one message, and the length of the datagram it arrived whole in, if it did
    #[cfg_attr(feature = "instrument", instrument(level = "trace", target = "protocol", err, skip(self, data), fields(self.socket = ?self.socket, data.len = data.len(), ret.len, ret.flow)))]
    pub async fn recv_message(&self, data: &mut [u8]) -> io::Result<(usize, Flow, Option<u16>)> {
        let mut consecutive_errors: usize = 0;
        let (message_len, remote_addr, opt_datagram_len) = loop {
            if let Some((message, remote_addr, opt_datagram_len)) = self.held.pop_due() {
                data[0..message.len()].copy_from_slice(&message);
                break (message.len(), remote_addr, opt_datagram_len);
            }

            // Get a packet, or wake when a held message is due
            let nres = match self.held.wait_ms() {
                None => self.socket.recv_from(data).await,
                Some(wait_ms) => match timeout(wait_ms, self.socket.recv_from(data)).await {
                    Ok(res) => res,
                    Err(_) => continue,
                },
            }
            .into_network_result()?;
            let failure = (!matches!(nres, NetworkResult::Value(_))).then(|| nres.to_string());
            let (size, remote_addr) = network_result_value_or_log!(self nres => {
                consecutive_errors += 1;
                if consecutive_errors >= UDP_RECV_FAIL_AFTER_CONSECUTIVE_ERRORS {
                    return Err(io::Error::other(format!(
                        "udp recv failed {} times consecutively: {}",
                        consecutive_errors,
                        failure.unwrap_or_default()
                    )));
                }
                // Yield unconditionally: a socket failing instantly would otherwise spin inside a
                // single poll, which pins a core and leaves the stop token unobserved, so the task
                // cannot be cancelled either
                sleep(if consecutive_errors > 1 { self.recv_error_backoff_ms } else { 0 }).await;
                continue;
            });
            consecutive_errors = 0;

            // Check to see if it is punished
            if self
                .network_manager()
                .address_filter()
                .is_ip_addr_punished(remote_addr.ip())
            {
                continue;
            }

            // Debug chaos: lost here, or held until its link has carried it
            let hold_ms = match self
                .network_manager()
                .chaos()
                .recv_verdict(remote_addr.ip(), size)
            {
                ChaosVerdict::Pass => None,
                ChaosVerdict::Hold(wait_ms) => Some(wait_ms),
                ChaosVerdict::Drop | ChaosVerdict::Oversize => continue,
            };

            let opt_datagram_len = AssemblyBuffer::is_whole_message_frame(&data[0..size])
                .then(|| u16::try_from(size).ok())
                .flatten();

            // Insert into assembly buffer
            let message = match self
                .assembly_buffer
                .insert_frame(&data[0..size], remote_addr)
            {
                NetworkResult::Value(Some(v)) => v,
                NetworkResult::Value(None) => {
                    continue;
                }
                nres => {
                    veilid_log!(self debug throttle: PROTOCOL_LOG_THROTTLE_MS, target: "network_result",
                        "UDP::recv_message insert_frame failed: {:?} <= size={} remote_addr={}",
                        nres,
                        size,
                        remote_addr
                    );
                    continue;
                }
            };

            // Check length of reassembled message (same for all protocols)
            if message.len() > MAX_MESSAGE_SIZE {
                veilid_log!(self debug throttle: PROTOCOL_LOG_THROTTLE_MS, "{}({}) at {}@{}:{}", "Invalid message", "received too large UDP message", file!(), line!(), column!());
                continue;
            }

            // The message arrives with its last fragment
            if let Some(wait_ms) = hold_ms {
                self.held
                    .push(wait_ms, (message, remote_addr, opt_datagram_len));
                continue;
            }

            // Copy assemble message out if we got one
            data[0..message.len()].copy_from_slice(&message);
            break (message.len(), remote_addr, opt_datagram_len);
        };

        // Return a flow and the amount of data in the message
        let peer_addr = PeerAddress::new(
            SocketAddress::from_socket_addr(remote_addr),
            ProtocolType::UDP,
        );
        let local_socket_addr = self.socket.local_addr()?;
        let flow = Flow::new(
            peer_addr,
            SocketAddress::from_socket_addr(local_socket_addr),
        );

        #[cfg(feature = "verbose-tracing")]
        tracing::Span::current().record("ret.len", message_len);
        #[cfg(feature = "verbose-tracing")]
        tracing::Span::current().record("ret.flow", format!("{:?}", flow).as_str());
        Ok((message_len, flow, opt_datagram_len))
    }

    /// Sends a fragment once debug chaos's emulated link has carried it; a socket closed by then
    /// loses it, as the link would
    fn send_held(&self, framed_chunk: Bytes, remote_addr: SocketAddr, wait_ms: u32) {
        let socket = Arc::downgrade(&self.socket);
        spawn_detached("chaos held udp send", async move {
            sleep(wait_ms).await;
            if let Some(socket) = socket.upgrade() {
                let _ = socket.send_to(&framed_chunk, remote_addr).await;
            }
        });
    }

    /// Fragment and send a message. `purpose` names the call site in the send-timeout log.
    #[cfg_attr(feature = "instrument", instrument(level = "trace", target = "protocol", err, skip(self, data), fields(self.socket = ?self.socket, data.len = data.len(), ret.flow)))]
    /// Sends one message in datagrams of at most `mtu` bytes
    pub async fn send_message(
        &self,
        data: Bytes,
        remote_addr: SocketAddr,
        mtu: u16,
        purpose: &'static str,
    ) -> io::Result<NetworkResult<Flow>> {
        if data.len() > MAX_MESSAGE_SIZE {
            bail_io_error_other!("sending too large UDP message");
        }

        // Check to see if it is punished
        if self
            .network_manager()
            .address_filter()
            .is_ip_addr_punished(remote_addr.ip())
        {
            return Ok(NetworkResult::no_connection_other("punished"));
        }

        if self.is_transport_disabled(remote_addr) {
            return Ok(NetworkResult::no_connection_other("transport disabled"));
        }

        let sender = |framed_chunk: Bytes, remote_addr: SocketAddr| async move {
            // Debug chaos: a lost or held fragment counts as sent
            match self
                .network_manager()
                .chaos()
                .send_verdict(remote_addr.ip(), framed_chunk.len())
            {
                ChaosVerdict::Pass => {}
                ChaosVerdict::Hold(wait_ms) => {
                    self.send_held(framed_chunk, remote_addr, wait_ms);
                    return Ok(NetworkResult::value(()));
                }
                ChaosVerdict::Drop | ChaosVerdict::Oversize => {
                    return Ok(NetworkResult::value(()));
                }
            }
            let len = network_result_try!(self
                .socket
                .send_to(&framed_chunk, remote_addr)
                .await
                .into_network_result()?);
            if len != framed_chunk.len() {
                bail_io_error_other!("UDP partial send")
            }

            veilid_log!(self trace "udp::send_message:chunk(len={}) {:?}", len, remote_addr);
            Ok(NetworkResult::value(()))
        };

        // A stalled interface parks send_to forever; bound the fragmented send so it fails
        network_result_try!(match timeout(
            self.send_timeout_ms,
            self.assembly_buffer
                .split_message(data, remote_addr, mtu.into(), sender)
        )
        .await
        {
            Ok(res) => res?,
            Err(_) => {
                veilid_log!(self debug throttle: PROTOCOL_LOG_THROTTLE_MS, "UDP send timed out: purpose={} remote_addr={:?}", purpose, remote_addr);
                return Ok(NetworkResult::timeout());
            }
        });

        // Return a flow for the sent message
        let peer_addr = PeerAddress::new(
            SocketAddress::from_socket_addr(remote_addr),
            ProtocolType::UDP,
        );
        let local_socket_addr = self.socket.local_addr()?;

        let flow = Flow::new(
            peer_addr,
            SocketAddress::from_socket_addr(local_socket_addr),
        );

        veilid_log!(self trace "udp::send_message: {:?}", flow);

        #[cfg(feature = "verbose-tracing")]
        tracing::Span::current().record("ret.flow", format!("{:?}", flow).as_str());
        Ok(NetworkResult::value(flow))
    }

    #[cfg_attr(feature = "instrument", instrument(level = "trace", target = "protocol", err, skip(self), fields(self.socket = ?self.socket, ret.flow)))]
    pub fn send_hole_punch(
        &self,
        remote_addr: SocketAddr,
        ttl: u32,
    ) -> io::Result<NetworkResult<Flow>> {
        // Check to see if it is punished
        if self
            .network_manager()
            .address_filter()
            .is_ip_addr_punished(remote_addr.ip())
        {
            return Ok(NetworkResult::no_connection_other("punished"));
        }

        if self.is_transport_disabled(remote_addr) {
            return Ok(NetworkResult::no_connection_other("transport disabled"));
        }

        // Zero length packet with a best-effort per-packet TTL; debug chaos can lose it, never hold it
        let verdict = self
            .network_manager()
            .chaos()
            .send_verdict(remote_addr.ip(), 0);
        let res = if matches!(verdict, ChaosVerdict::Drop | ChaosVerdict::Oversize) {
            Ok(0)
        } else {
            match self
                .ttl_sender
                .send_to(self.socket.as_ref(), &[], remote_addr, ttl)
            {
                Err(e) if e.kind() == io::ErrorKind::WouldBlock => {
                    veilid_log!(self debug throttle: PROTOCOL_LOG_THROTTLE_MS, "UDP hole punch send would block: remote_addr={:?}", remote_addr);
                    return Ok(NetworkResult::timeout());
                }
                res => res,
            }
        };

        // Check for errors
        let len = network_result_try!(res.into_network_result()?);
        if len != 0 {
            bail_io_error_other!("wrong size send");
        }

        // Return a flow for the sent message
        let peer_addr = PeerAddress::new(
            SocketAddress::from_socket_addr(remote_addr),
            ProtocolType::UDP,
        );
        let local_socket_addr = self.socket.local_addr()?;

        let flow = Flow::new(
            peer_addr,
            SocketAddress::from_socket_addr(local_socket_addr),
        );

        veilid_log!(self trace "udp::send_hole_punch: {:?}", flow);

        #[cfg(feature = "verbose-tracing")]
        tracing::Span::current().record("ret.flow", format!("{:?}", flow).as_str());
        Ok(NetworkResult::value(flow))
    }

    #[cfg_attr(
        feature = "instrument",
        instrument(level = "trace", target = "protocol", skip(registry, timings), err, fields(__VEILID_LOG_KEY = registry.log_key()))
    )]
    pub fn try_new_unspecified_bound_handler(
        registry: VeilidComponentRegistry,
        socket_addr: &SocketAddr,
        timings: &NativeNetworkTimings,
    ) -> io::Result<RawUdpProtocolHandler> {
        // get local wildcard address for bind
        let local_socket_addr = compatible_unspecified_socket_addr(socket_addr);
        let socket = bind_async_udp_socket(local_socket_addr)?
            .ok_or(io::Error::from(io::ErrorKind::AddrInUse))?;
        Ok(RawUdpProtocolHandler::new(
            registry,
            Arc::new(socket),
            timings,
        ))
    }

    /// Bind a UDP socket to a specific local source address
    #[cfg_attr(
        feature = "instrument",
        instrument(level = "trace", target = "protocol", skip(registry, timings), err, fields(__VEILID_LOG_KEY = registry.log_key()))
    )]
    pub fn try_new_specific_bound_handler(
        registry: VeilidComponentRegistry,
        local_socket_addr: SocketAddr,
        timings: &NativeNetworkTimings,
    ) -> io::Result<RawUdpProtocolHandler> {
        let socket = bind_async_udp_socket(local_socket_addr)?
            .ok_or(io::Error::from(io::ErrorKind::AddrInUse))?;
        Ok(RawUdpProtocolHandler::new(
            registry,
            Arc::new(socket),
            timings,
        ))
    }
}
