use super::*;
#[cfg(feature = "rt-async-std")]
use async_io::Async;
use std::io;
use std::sync::atomic::AtomicU32;

cfg_if! {
    if #[cfg(feature="rt-async-std")] {
        pub use async_std::net::{TcpStream, TcpListener, UdpSocket};
    } else if #[cfg(feature="rt-tokio")] {
        pub use tokio::net::{TcpStream, TcpListener, UdpSocket};
        pub use tokio_util::compat::*;
    } else {
        compile_error!("needs executor implementation");
    }
}

use socket2::{Domain, Protocol, SockAddr, Socket, Type};

//////////////////////////////////////////////////////////////////////////////////////////

/// Bind a UDP socket to `local_address` with `SO_REUSEADDR`/`SO_REUSEPORT` set. On Linux and
/// Android an IPv4 socket's datagrams go out without DF, so routers may fragment them, as on other
/// platforms.
///
/// Returns `Ok(None)` if the address could not be bound (a bind failure is not an error).
/// Errors only on socket creation or option failures (`Socket::new`, reuse-address/port, and the tokio non-blocking conversion).
pub fn bind_async_udp_socket(local_address: SocketAddr) -> io::Result<Option<UdpSocket>> {
    let Some(socket) = new_bound_shared_socket2_udp(local_address)? else {
        return Ok(None);
    };

    // Make an async UdpSocket from the socket2 socket
    let std_udp_socket: std::net::UdpSocket = socket.into();
    cfg_if! {
        if #[cfg(feature="rt-async-std")] {
            let udp_socket = UdpSocket::from(std_udp_socket);
        } else if #[cfg(feature="rt-tokio")] {
            std_udp_socket.set_nonblocking(true)?;
            let udp_socket = UdpSocket::from_std(std_udp_socket)?;
        } else {
            compile_error!("needs executor implementation");
        }
    }
    Ok(Some(udp_socket))
}

/// Bind a UDP socket to `local_address`, probing the port with a non-shared
/// socket first so an occupied port fails instead of silently reuse-binding
/// alongside another process (which starves both of inbound packets). Use for
/// inbound listeners found by port search; use [`bind_async_udp_socket`] for
/// sockets that intentionally share the listener's port.
///
/// Returns `Ok(None)` if the address could not be bound (a bind failure is not an error).
/// Errors only on socket creation or option failures.
pub fn bind_async_udp_socket_probed(local_address: SocketAddr) -> io::Result<Option<UdpSocket>> {
    // Create a default non-shared socket and bind it to assert the port is free
    let Some(socket) = new_bound_default_socket2_udp(local_address)? else {
        return Ok(None);
    };

    // Drop the probe so we can make a shared socket in its place
    drop(socket);

    // The shared socket keeps outbound source binding on this port possible
    bind_async_udp_socket(local_address)
}

/// Bind a TCP listener to `local_address`.
///
/// Probes the port with a non-shared socket first, then binds a shared
/// (`SO_REUSEADDR`/`SO_REUSEPORT`) socket once the port is known free. With `mss`, accepted
/// connections ask their peers for segments no larger than that where the platform advertises it
/// (Linux, Android); elsewhere clamp each accepted stream with [`limit_tcp_stream_mss`].
/// Returns `Ok(None)` if the address could not be bound or listened on (these are not errors).
/// Errors only on socket creation or option failures (`Socket::new`, linger, nodelay, reuse-address/port, and the tokio non-blocking conversion).
pub fn bind_async_tcp_listener(
    local_address: SocketAddr,
    mss: Option<u32>,
) -> io::Result<Option<TcpListener>> {
    // Create a default non-shared socket and bind it
    let Some(socket) = new_bound_default_socket2_tcp(local_address)? else {
        return Ok(None);
    };

    // Drop the socket so we can make another shared socket in its place
    drop(socket);

    // Create a shared socket and bind it now we have determined the port is free
    let Some(socket) = new_bound_shared_socket2_tcp(local_address)? else {
        return Ok(None);
    };
    if let Some(mss) = mss {
        request_socket_mss(&socket, mss);
    }

    // Listen on the socket
    if socket.listen(128).is_err() {
        return Ok(None);
    }

    // Make an async tcplistener from the socket2 socket
    let std_listener: std::net::TcpListener = socket.into();
    cfg_if! {
        if #[cfg(feature="rt-async-std")] {
            let listener = TcpListener::from(std_listener);
        } else if #[cfg(feature="rt-tokio")] {
            std_listener.set_nonblocking(true)?;
            let listener = TcpListener::from_std(std_listener)?;
        } else {
            compile_error!("needs executor implementation");
        }
    }
    Ok(Some(listener))
}

/// Connect a TCP stream to `remote_address`, optionally bound to `local_address`.
///
/// When `local_address` is given the connecting socket is shared
/// (`SO_REUSEADDR`/`SO_REUSEPORT`); otherwise a default socket is used. The
/// connect is non-blocking and bounded by `timeout_ms`. With `mss`, the stream sends segments no
/// larger than that, and asks its peer for the same where the platform advertises it (Linux,
/// Android); failing to set it leaves a working stream with the platform's segment size.
///
/// A timeout yields `Ok(TimeoutOr::timeout())`, not an error. Errors with `AddrInUse` when `local_address` cannot be bound, and propagates the connect failure (e.g. `ConnectionRefused`, `HostUnreachable`) along with any socket creation or option io error.
pub async fn connect_async_tcp_stream(
    local_address: Option<SocketAddr>,
    remote_address: SocketAddr,
    timeout_ms: u32,
    mss: Option<u32>,
) -> io::Result<TimeoutOr<TcpStream>> {
    let socket = match local_address {
        Some(a) => {
            new_bound_shared_socket2_tcp(a)?.ok_or(io::Error::from(io::ErrorKind::AddrInUse))?
        }
        None => new_default_socket2_tcp(domain_for_address(remote_address))?,
    };
    if let Some(mss) = mss {
        request_socket_mss(&socket, mss);
    }

    // Non-blocking connect to remote address
    let res = nonblocking_connect(socket, remote_address, timeout_ms).await?;
    if let (Some(mss), TimeoutOr::Value(tcp_stream)) = (mss, &res) {
        // Best effort, like the request before connect
        let _ = limit_tcp_stream_mss(tcp_stream, mss);
    }
    Ok(res)
}

/// Lower a connected stream's TCP segment size to at most `mss`. It limits only what this end
/// sends: the peer's segments follow what was advertised when the connection opened. Windows
/// cannot set TCP_MAXSEG, so there it caps the socket's IP MTU, which IPv6 keeps at 1280 or more.
///
/// Errors with the io error from the underlying `getsockopt`/`setsockopt`.
pub fn limit_tcp_stream_mss(tcp_stream: &TcpStream, mss: u32) -> io::Result<()> {
    let socket = socket2::SockRef::from(tcp_stream);
    cfg_if! {
        if #[cfg(unix)] {
            // XNU rejects a value above the current one
            if socket.tcp_mss()? > mss {
                socket.set_tcp_mss(mss)?;
            }
        } else {
            if windows_tcp_mss(&socket)? > mss {
                windows_cap_ip_mtu(&socket, mss)?;
            }
        }
    }
    Ok(())
}

/// The TCP segment size a connected stream sends.
///
/// Errors with the io error from the underlying `getsockopt`; Windows can read it from Windows 10.
pub fn tcp_stream_mss(tcp_stream: &TcpStream) -> io::Result<u32> {
    let socket = socket2::SockRef::from(tcp_stream);
    cfg_if! {
        if #[cfg(unix)] {
            socket.tcp_mss()
        } else {
            windows_tcp_mss(&socket)
        }
    }
}

/// Lowers the largest segment a [`SegmentLimitedTcpStream`] sends.
#[derive(Clone, Debug)]
pub struct TcpSegmentLimit(Arc<AtomicU32>);

impl TcpSegmentLimit {
    /// Lowers the limit to `limit` bytes; it never rises again.
    pub fn lower(&self, limit: u32) {
        self.0.fetch_min(limit.max(1), Ordering::Relaxed);
    }

    /// The limit, once one was set.
    #[must_use]
    pub fn get(&self) -> Option<u32> {
        let limit = self.0.load(Ordering::Relaxed);
        (limit != u32::MAX).then_some(limit)
    }
}

cfg_if! {
    if #[cfg(feature="rt-async-std")] {
        type SegmentLimitedInner = TcpStream;
    } else if #[cfg(feature="rt-tokio")] {
        type SegmentLimitedInner = Compat<TcpStream>;
    } else {
        compile_error!("needs executor implementation");
    }
}

/// A TCP stream whose segments stay within a limit that can be lowered while it is open:
/// - Linux and Android under tokio write at most the limit at a time and end each write with
///   `MSG_EOR`, which stops the kernel appending to it or merging it, retransmits included
/// - macOS, iOS and Windows lower the socket's own segment size
/// - elsewhere writes stay within the limit, but the kernel may merge them
pub struct SegmentLimitedTcpStream {
    stream: SegmentLimitedInner,
    limit: TcpSegmentLimit,
    applied: u32,
}

impl SegmentLimitedTcpStream {
    /// Wrap `tcp_stream`, returning the handle that lowers its limit; until then it is unlimited.
    #[must_use]
    pub fn new(tcp_stream: TcpStream) -> (Self, TcpSegmentLimit) {
        let limit = TcpSegmentLimit(Arc::new(AtomicU32::new(u32::MAX)));
        cfg_if! {
            if #[cfg(feature="rt-async-std")] {
                let stream = tcp_stream;
            } else if #[cfg(feature="rt-tokio")] {
                let stream = tcp_stream.compat();
            } else {
                compile_error!("needs executor implementation");
            }
        }
        (
            Self {
                stream,
                limit: limit.clone(),
                applied: u32::MAX,
            },
            limit,
        )
    }

    #[cfg(any(
        feature = "rt-tokio",
        not(any(target_os = "linux", target_os = "android"))
    ))]
    fn tcp_stream(&self) -> &TcpStream {
        cfg_if! {
            if #[cfg(feature="rt-async-std")] {
                &self.stream
            } else if #[cfg(feature="rt-tokio")] {
                self.stream.get_ref()
            } else {
                compile_error!("needs executor implementation");
            }
        }
    }
}

impl AsyncRead for SegmentLimitedTcpStream {
    fn poll_read(
        mut self: Pin<&mut Self>,
        cx: &mut task::Context<'_>,
        buf: &mut [u8],
    ) -> task::Poll<io::Result<usize>> {
        Pin::new(&mut self.stream).poll_read(cx, buf)
    }
}

impl AsyncWrite for SegmentLimitedTcpStream {
    fn poll_write(
        mut self: Pin<&mut Self>,
        cx: &mut task::Context<'_>,
        buf: &[u8],
    ) -> task::Poll<io::Result<usize>> {
        let Some(limit) = self.limit.get() else {
            return Pin::new(&mut self.stream).poll_write(cx, buf);
        };
        if limit < self.applied {
            self.applied = limit;
            // Best effort: where the socket keeps its size, the bounded writes below still apply
            #[cfg(not(any(target_os = "linux", target_os = "android")))]
            let _ = limit_tcp_stream_mss(self.tcp_stream(), limit);
        }
        let buf = &buf[..buf.len().min(limit as usize)];
        cfg_if! {
            if #[cfg(all(feature = "rt-tokio", any(target_os = "linux", target_os = "android")))] {
                poll_write_end_of_record(self.tcp_stream(), cx, buf)
            } else {
                Pin::new(&mut self.stream).poll_write(cx, buf)
            }
        }
    }
    fn poll_flush(
        mut self: Pin<&mut Self>,
        cx: &mut task::Context<'_>,
    ) -> task::Poll<io::Result<()>> {
        Pin::new(&mut self.stream).poll_flush(cx)
    }
    fn poll_close(
        mut self: Pin<&mut Self>,
        cx: &mut task::Context<'_>,
    ) -> task::Poll<io::Result<()>> {
        Pin::new(&mut self.stream).poll_close(cx)
    }
}

/// Writes `buf` as one record the kernel will not merge with another, retransmits included
#[cfg(all(feature = "rt-tokio", any(target_os = "linux", target_os = "android")))]
fn poll_write_end_of_record(
    tcp_stream: &TcpStream,
    cx: &mut task::Context<'_>,
    buf: &[u8],
) -> task::Poll<io::Result<usize>> {
    loop {
        task::ready!(tcp_stream.poll_write_ready(cx))?;
        match tcp_stream.try_io(tokio::io::Interest::WRITABLE, || {
            socket2::SockRef::from(tcp_stream).send_with_flags(buf, libc::MSG_EOR)
        }) {
            Err(e) if e.kind() == io::ErrorKind::WouldBlock => continue,
            res => return task::Poll::Ready(res),
        }
    }
}

/// Changes the segment size a listening socket advertises to the connections it accepts from
/// now on. Only Linux advertises it; elsewhere this changes nothing.
#[derive(Debug)]
pub struct TcpListenerMssHandle(Socket);

impl TcpListenerMssHandle {
    /// A handle on `listener`'s socket, which stays usable after the listener moves on.
    ///
    /// Errors with the io error from duplicating the socket.
    pub fn try_new(listener: &TcpListener) -> io::Result<Self> {
        Ok(Self(socket2::SockRef::from(listener).try_clone()?))
    }

    /// Ask the listener to advertise `mss`; best effort.
    pub fn set_mss(&self, mss: u32) {
        request_socket_mss(&self.0, mss);
    }
}

/// ws2ipdef.h; the winapi crate has none of these
#[cfg(windows)]
const TCP_MAXSEG: core::ffi::c_int = 4;
#[cfg(windows)]
const IP_USER_MTU: core::ffi::c_int = 76;
#[cfg(windows)]
const IPV6_USER_MTU: core::ffi::c_int = 76;

/// The segment size a connected Windows TCP socket sends
#[cfg(windows)]
fn windows_tcp_mss(socket: &Socket) -> io::Result<u32> {
    use std::os::windows::io::AsRawSocket;
    use winapi::shared::ws2def::IPPROTO_TCP;
    use winapi::um::winsock2::{getsockopt, SOCKET, SOCKET_ERROR};

    let mut mss: u32 = 0;
    let mut len = core::mem::size_of::<u32>() as core::ffi::c_int;
    // SAFETY: the socket is open, and `mss` and `len` outlive this synchronous call
    let res = unsafe {
        getsockopt(
            socket.as_raw_socket() as SOCKET,
            IPPROTO_TCP as core::ffi::c_int,
            TCP_MAXSEG,
            (&mut mss as *mut u32).cast(),
            &mut len,
        )
    };
    if res == SOCKET_ERROR {
        return Err(io::Error::last_os_error());
    }
    Ok(mss)
}

/// Caps a Windows socket's IP MTU so its TCP segments are at most `mss`. Only what this end sends
/// shrinks, and IPv6 refuses an MTU below 1280
#[cfg(windows)]
fn windows_cap_ip_mtu(socket: &Socket, mss: u32) -> io::Result<()> {
    use std::os::windows::io::AsRawSocket;
    use winapi::shared::ws2def::{IPPROTO_IP, IPPROTO_IPV6};
    use winapi::um::winsock2::{setsockopt, SOCKET, SOCKET_ERROR};

    let (level, option, mtu) = if socket.local_addr()?.is_ipv6() {
        (
            IPPROTO_IPV6 as core::ffi::c_int,
            IPV6_USER_MTU,
            mss.saturating_add(60).max(1280),
        )
    } else {
        (IPPROTO_IP, IP_USER_MTU, mss.saturating_add(40))
    };
    // SAFETY: the socket is open, and `mtu` outlives this synchronous call
    let res = unsafe {
        setsockopt(
            socket.as_raw_socket() as SOCKET,
            level,
            option,
            (&mtu as *const u32).cast(),
            core::mem::size_of::<u32>() as core::ffi::c_int,
        )
    };
    if res == SOCKET_ERROR {
        return Err(io::Error::last_os_error());
    }
    Ok(())
}

/// Set the `SO_LINGER` option on a TCP stream.
///
/// Works around runtimes that do not expose linger directly by reaching the
/// underlying socket via the raw fd/handle.
///
/// Errors with the io error from the underlying `setsockopt`.
pub fn set_tcp_stream_linger(
    tcp_stream: &TcpStream,
    linger: Option<core::time::Duration>,
) -> io::Result<()> {
    // The borrowed view never closes the stream's descriptor
    socket2::SockRef::from(tcp_stream).set_linger(linger)
}

cfg_if! {
    if #[cfg(feature="rt-async-std")] {
        /// Owned read half of a split [`TcpStream`].
        pub type ReadHalf = futures_util::io::ReadHalf<TcpStream>;
        /// Owned write half of a split [`TcpStream`].
        pub type WriteHalf = futures_util::io::WriteHalf<TcpStream>;
    } else if #[cfg(feature="rt-tokio")] {
        /// Owned read half of a split [`TcpStream`].
        pub type ReadHalf = tokio::net::tcp::OwnedReadHalf;
        /// Owned write half of a split [`TcpStream`].
        pub type WriteHalf = tokio::net::tcp::OwnedWriteHalf;
    } else {
        compile_error!("needs executor implementation");
    }
}

/// Wrap a [`TcpListener`] as a stream of accepted [`TcpStream`] connections.
#[must_use]
pub fn async_tcp_listener_incoming(
    tcp_listener: TcpListener,
) -> Pin<Box<impl futures_util::stream::Stream<Item = std::io::Result<TcpStream>> + Send>> {
    cfg_if! {
        if #[cfg(feature="rt-async-std")] {
            Box::pin(tcp_listener.into_incoming())
        } else if #[cfg(feature="rt-tokio")] {
            Box::pin(tokio_stream::wrappers::TcpListenerStream::new(tcp_listener))
        } else {
            compile_error!("needs executor implementation");
        }
    }
}

/// Split a [`TcpStream`] into owned read and write halves.
#[must_use]
pub fn split_async_tcp_stream(tcp_stream: TcpStream) -> (ReadHalf, WriteHalf) {
    cfg_if! {
        if #[cfg(feature="rt-async-std")] {
            use futures_util::AsyncReadExt;
            tcp_stream.split()
        } else if #[cfg(feature="rt-tokio")] {
            tcp_stream.into_split()
        } else {
            compile_error!("needs executor implementation");
        }
    }
}

//////////////////////////////////////////////////////////////////////////////////////////

fn new_shared_socket2_udp(domain: core::ffi::c_int) -> io::Result<Socket> {
    let domain = Domain::from(domain);
    let socket = Socket::new(domain, Type::DGRAM, Some(Protocol::UDP))?;
    if domain == Domain::IPV6 {
        socket.set_only_v6(true)?;
    }
    // SO_REUSEADDR + SO_REUSEPORT so a wildcard listener and a specific-source
    // handler can coexist on the same port (used for outbound source binding)
    socket.set_reuse_address(true)?;
    cfg_if! {
        if #[cfg(unix)] {
            socket.set_reuse_port(true)?;
        } else if #[cfg(windows)] {
            // Best effort: where it fails, the receive path still treats these errors as transient
            let _ = disable_udp_icmp_errors(&socket);
        }
    }
    #[cfg(any(target_os = "linux", target_os = "android"))]
    if domain == Domain::IPV4 {
        // Best effort: where it fails, a router that must fragment our datagrams drops them instead
        let _ = allow_ipv4_fragmentation(&socket);
    }
    Ok(socket)
}

/// Let routers fragment an IPv4 socket's datagrams, as other platforms do; Linux and Android
/// otherwise set DF on every datagram that fits the cached path MTU
#[cfg(any(target_os = "linux", target_os = "android"))]
fn allow_ipv4_fragmentation(socket: &Socket) -> io::Result<()> {
    use std::os::fd::AsRawFd;

    let value: libc::c_int = libc::IP_PMTUDISC_DONT;
    // SAFETY: the socket is open, and `value` outlives this synchronous call
    let res = unsafe {
        libc::setsockopt(
            socket.as_raw_fd(),
            libc::IPPROTO_IP,
            libc::IP_MTU_DISCOVER,
            (&value as *const libc::c_int).cast(),
            core::mem::size_of::<libc::c_int>() as libc::socklen_t,
        )
    };
    if res != 0 {
        return Err(io::Error::last_os_error());
    }
    Ok(())
}

/// Keep ICMP port unreachable and TTL expired out of recv, as on other platforms; Windows
/// otherwise fails the next receive on the port with WSAECONNRESET or WSAENETRESET
#[cfg(windows)]
fn disable_udp_icmp_errors(socket: &Socket) -> io::Result<()> {
    use std::os::windows::io::AsRawSocket;
    use winapi::shared::minwindef::{BOOL, DWORD, FALSE};
    use winapi::um::mswsock::{SIO_UDP_CONNRESET, SIO_UDP_NETRESET};
    use winapi::um::winsock2::{WSAIoctl, SOCKET, SOCKET_ERROR};

    for code in [SIO_UDP_CONNRESET, SIO_UDP_NETRESET] {
        let mut report: BOOL = FALSE;
        let mut returned: DWORD = 0;
        // SAFETY: the socket is open, and `report` and `returned` outlive this synchronous call
        let res = unsafe {
            WSAIoctl(
                socket.as_raw_socket() as SOCKET,
                code,
                (&mut report as *mut BOOL).cast(),
                std::mem::size_of::<BOOL>() as DWORD,
                std::ptr::null_mut(),
                0,
                &mut returned,
                std::ptr::null_mut(),
                None,
            )
        };
        if res == SOCKET_ERROR {
            return Err(io::Error::last_os_error());
        }
    }
    Ok(())
}

fn new_bound_shared_socket2_udp(local_address: SocketAddr) -> io::Result<Option<Socket>> {
    let domain = domain_for_address(local_address);
    let socket = new_shared_socket2_udp(domain)?;
    let socket2_addr = SockAddr::from(local_address);

    if socket.bind(&socket2_addr).is_err() {
        return Ok(None);
    }

    Ok(Some(socket))
}

fn new_default_socket2_udp(domain: core::ffi::c_int) -> io::Result<Socket> {
    let domain = Domain::from(domain);
    let socket = Socket::new(domain, Type::DGRAM, Some(Protocol::UDP))?;
    if domain == Domain::IPV6 {
        socket.set_only_v6(true)?;
    }
    Ok(socket)
}

fn new_bound_default_socket2_udp(local_address: SocketAddr) -> io::Result<Option<Socket>> {
    let domain = domain_for_address(local_address);
    let socket = new_default_socket2_udp(domain)?;
    let socket2_addr = SockAddr::from(local_address);
    if socket.bind(&socket2_addr).is_err() {
        return Ok(None);
    }

    Ok(Some(socket))
}

/// Create a non-shared TCP [`Socket`] for `domain` with zero linger and `TCP_NODELAY` set.
///
/// Errors with the io error from socket creation or any of the option calls (linger, nodelay, IPv6-only).
pub fn new_default_socket2_tcp(domain: core::ffi::c_int) -> io::Result<Socket> {
    let domain = Domain::from(domain);
    let socket = Socket::new(domain, Type::STREAM, Some(Protocol::TCP))?;
    socket.set_linger(Some(core::time::Duration::from_secs(0)))?;
    socket.set_tcp_nodelay(true)?;
    if domain == Domain::IPV6 {
        socket.set_only_v6(true)?;
    }
    Ok(socket)
}

fn new_shared_socket2_tcp(domain: core::ffi::c_int) -> io::Result<Socket> {
    let domain = Domain::from(domain);
    let socket = Socket::new(domain, Type::STREAM, Some(Protocol::TCP))?;
    socket.set_linger(Some(core::time::Duration::from_secs(0)))?;
    socket.set_tcp_nodelay(true)?;
    if domain == Domain::IPV6 {
        socket.set_only_v6(true)?;
    }
    socket.set_reuse_address(true)?;
    cfg_if! {
        if #[cfg(unix)] {
            socket.set_reuse_port(true)?;
        }
    }

    Ok(socket)
}

fn new_bound_default_socket2_tcp(local_address: SocketAddr) -> io::Result<Option<Socket>> {
    let domain = domain_for_address(local_address);
    let socket = new_default_socket2_tcp(domain)?;
    let socket2_addr = SockAddr::from(local_address);
    if socket.bind(&socket2_addr).is_err() {
        return Ok(None);
    }

    Ok(Some(socket))
}

/// Asks for `mss` before connecting or listening. Linux also advertises it in the handshake, so
/// the peer's segments shrink too; XNU ignores it there and rejects values above its 512-byte
/// default, so a failure is expected and its connected streams are clamped instead, as on Windows.
fn request_socket_mss(socket: &Socket, mss: u32) {
    cfg_if! {
        if #[cfg(unix)] {
            let _ = socket.set_tcp_mss(mss);
        } else {
            let _ = (socket, mss);
        }
    }
}

fn new_bound_shared_socket2_tcp(local_address: SocketAddr) -> io::Result<Option<Socket>> {
    // Create the reuseaddr/reuseport socket now that we've asserted the port is free
    let domain = domain_for_address(local_address);
    let socket = new_shared_socket2_tcp(domain)?;
    let socket2_addr = SockAddr::from(local_address);
    if socket.bind(&socket2_addr).is_err() {
        return Ok(None);
    }

    Ok(Some(socket))
}

// Non-blocking connect is tricky when you want to start with a prepared socket
// Errors should not be logged as they are valid conditions for this function
async fn nonblocking_connect(
    socket: Socket,
    addr: SocketAddr,
    timeout_ms: u32,
) -> io::Result<TimeoutOr<TcpStream>> {
    // Set for non blocking connect
    socket.set_nonblocking(true)?;

    // Make socket2 SockAddr
    let socket2_addr = socket2::SockAddr::from(addr);

    // Connect to the remote address (non-blocking; completion signaled by writability)
    match socket.connect(&socket2_addr) {
        Ok(()) => Ok(()),
        #[cfg(unix)]
        Err(err) if err.raw_os_error() == Some(libc::EINPROGRESS) => Ok(()),
        Err(err) if err.kind() == std::io::ErrorKind::WouldBlock => Ok(()),
        Err(e) => Err(e),
    }?;
    let std_stream = std::net::TcpStream::from(socket);

    // Wait for writability (connect complete) on the executor's own reactor. async-io's reactor
    // is never driven under tokio, so its wait and timeout would hang forever there.
    cfg_if! {
        if #[cfg(feature="rt-async-std")] {
            let async_stream = Async::new(std_stream)?;
            timeout_or_try!(timeout(timeout_ms, async_stream.writable())
                .await
                .into_timeout_or()
                .into_result()?);
            if let Some(err) = async_stream.get_ref().take_error()? {
                return Err(err);
            }
            Ok(TimeoutOr::value(TcpStream::from(async_stream.into_inner()?)))
        } else if #[cfg(feature="rt-tokio")] {
            let stream = TcpStream::from_std(std_stream)?;
            timeout_or_try!(timeout(timeout_ms, stream.writable())
                .await
                .into_timeout_or()
                .into_result()?);
            if let Some(err) = stream.take_error()? {
                return Err(err);
            }
            Ok(TimeoutOr::value(stream))
        } else {
            compile_error!("needs executor implementation");
        }
    }
}

/// The socket address family (`AF_INET`/`AF_INET6`) for `address`.
#[must_use]
pub fn domain_for_address(address: SocketAddr) -> core::ffi::c_int {
    socket2::Domain::for_address(address).into()
}

/// Local IPv4 the kernel would pick as the source for outbound traffic.
///
/// UDP-connects to an unallocated probe; no packets are sent. `None` if no
/// default IPv4 route exists.
#[must_use]
pub fn primary_source_ipv4() -> Option<Ipv4Addr> {
    let ipv4addr = "1.2.3.4".parse::<Ipv4Addr>().unwrap();
    let sockaddrv4 = SocketAddr::V4(SocketAddrV4::new(ipv4addr, 5155));
    let socket = std::net::UdpSocket::bind("0.0.0.0:0").ok()?;
    socket.connect(sockaddrv4).ok()?;
    if let Ok(SocketAddr::V4(local)) = socket.local_addr() {
        Some(*local.ip())
    } else {
        None
    }
}

/// Local IPv6 the kernel would pick as the source for outbound traffic.
///
/// UDP-connects to an unallocated probe; no packets are sent. `None` if no
/// default IPv6 route exists.
#[must_use]
pub fn primary_source_ipv6() -> Option<Ipv6Addr> {
    let ipv6addr = "2222::1".parse::<Ipv6Addr>().unwrap();
    let sockaddrv6 = SocketAddr::V6(SocketAddrV6::new(ipv6addr, 5155, 0, 0));
    let socket = std::net::UdpSocket::bind("[::]:0").ok()?;
    socket.connect(sockaddrv6).ok()?;
    if let Ok(SocketAddr::V6(local)) = socket.local_addr() {
        Some(*local.ip())
    } else {
        None
    }
}

/// Sends single UDP datagrams at a chosen TTL for one shared socket, never touching that
/// socket's TTL option. Keep one beside each shared socket and drop it with that socket: on
/// macOS and iOS it holds a twin per TTL bound to the socket's address and port, and a twin left
/// open after its socket closes takes the port's inbound traffic.
#[derive(Debug, Default)]
pub struct UdpTtlSender {
    /// One per TTL asked for, bound on its first send and rebuilt after a failed one
    #[cfg(any(target_os = "macos", target_os = "ios"))]
    twins: Mutex<Vec<Arc<TtlTwin>>>,
}

impl UdpTtlSender {
    /// A sender that binds no twins until its first send
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }
}

// Send a single UDP datagram with a per-packet TTL override
cfg_if! {
    if #[cfg(unix)] {
        cfg_if! {
            if #[cfg(target_os = "openbsd")] {
                /// netinet6/in6.h; the libc crate has no OpenBSD binding for it
                const IPV6_HOPLIMIT: libc::c_int = 47;
            } else {
                const IPV6_HOPLIMIT: libc::c_int = libc::IPV6_HOPLIMIT;
            }
        }

        /// The IPv4 low-TTL sender for a wildcard-bound shared socket on macOS and iOS, where
        /// xnu ignores an `IP_TTL` cmsg. The shared socket keeps all its traffic while the twin
        /// lives inside its lifetime: xnu gives inbound unicast to the oldest wildcard socket on
        /// a port and IPv4 ICMP errors only to connected sockets.
        #[cfg(any(target_os = "macos", target_os = "ios"))]
        #[derive(Debug)]
        struct TtlTwin {
            /// Bound to `local_address` after the shared socket
            socket: Socket,
            /// The wildcard address and port the twin shares
            local_address: SocketAddr,
            /// TTL every datagram from `socket` carries
            ttl: u32,
        }

        #[cfg(any(target_os = "macos", target_os = "ios"))]
        impl TtlTwin {
            fn try_new(local_address: SocketAddr, ttl: u32) -> io::Result<Self> {
                let socket = new_shared_socket2_udp(domain_for_address(local_address))?;
                socket.set_ttl_v4(ttl)?;
                socket.set_nonblocking(true)?;
                // It never reads, and LAN broadcasts to the port reach it as well
                socket.set_recv_buffer_size(1)?;
                socket.bind(&SockAddr::from(local_address))?;
                Ok(Self {
                    socket,
                    local_address,
                    ttl,
                })
            }
        }

        impl UdpTtlSender {
            /// Send one UDP datagram on `socket` with a best-effort per-packet TTL (IPv4) or hop
            /// limit (IPv6):
            /// - IPv6: `IPV6_HOPLIMIT` cmsg (RFC 3542)
            /// - IPv4 on Linux/Android: `IP_TTL` cmsg
            /// - IPv4 on macOS/iOS from a wildcard-bound socket: from the twin for that TTL
            /// - IPv4 elsewhere: at the socket's default TTL; a setsockopt swap would race other
            ///   senders on the shared socket
            ///
            /// Single nonblocking attempt; a full send buffer surfaces as `WouldBlock`.
            pub fn send_to<S: std::os::fd::AsFd>(
                &self,
                socket: &S,
                data: &[u8],
                remote_addr: SocketAddr,
                ttl: u32,
            ) -> io::Result<usize> {
                use std::os::fd::AsRawFd;
                let ttl = ttl_option_value(ttl)?;
                cfg_if! {
                    if #[cfg(any(target_os = "linux", target_os = "android"))] {
                        let (cmsg_level, cmsg_type) = match remote_addr {
                            SocketAddr::V4(_) => (libc::IPPROTO_IP, libc::IP_TTL),
                            SocketAddr::V6(_) => (libc::IPPROTO_IPV6, IPV6_HOPLIMIT),
                        };
                        sendmsg_with_ttl_cmsg(socket.as_fd().as_raw_fd(), data, remote_addr, cmsg_level, cmsg_type, ttl)
                    } else if #[cfg(any(target_os = "macos", target_os = "ios"))] {
                        match remote_addr {
                            SocketAddr::V4(_) => self.send_from_twin(socket, data, remote_addr, ttl as u32),
                            SocketAddr::V6(_) => sendmsg_with_ttl_cmsg(socket.as_fd().as_raw_fd(), data, remote_addr, libc::IPPROTO_IPV6, IPV6_HOPLIMIT, ttl),
                        }
                    } else {
                        match remote_addr {
                            SocketAddr::V4(_) => socket2_operation(socket, |s| s.send_to(data, &remote_addr.into())),
                            SocketAddr::V6(_) => sendmsg_with_ttl_cmsg(socket.as_fd().as_raw_fd(), data, remote_addr, libc::IPPROTO_IPV6, IPV6_HOPLIMIT, ttl),
                        }
                    }
                }
            }

            /// Send from the twin for `ttl`; a socket bound to a specific address, or a twin that
            /// cannot bind, sends at the socket's TTL
            #[cfg(any(target_os = "macos", target_os = "ios"))]
            fn send_from_twin<S: std::os::fd::AsFd>(
                &self,
                socket: &S,
                data: &[u8],
                remote_addr: SocketAddr,
                ttl: u32,
            ) -> io::Result<usize> {
                let send_shared = || socket2_operation(socket, |s| s.send_to(data, &remote_addr.into()));
                // A specific-address twin would take the shared socket's inbound traffic
                let Some(local_address) = socket2_operation(socket, |s| s.local_addr())
                    .ok()
                    .and_then(|sa| sa.as_socket())
                    .filter(|sa| sa.ip().is_unspecified())
                else {
                    return send_shared();
                };
                let opt_twin = {
                    let mut twins = self.twins.lock();
                    twins.retain(|twin| twin.local_address == local_address);
                    match twins.iter().find(|twin| twin.ttl == ttl) {
                        Some(twin) => Some(twin.clone()),
                        None => TtlTwin::try_new(local_address, ttl).ok().map(|twin| {
                            let twin = Arc::new(twin);
                            twins.push(twin.clone());
                            twin
                        }),
                    }
                };
                let Some(twin) = opt_twin else {
                    return send_shared();
                };
                let res = twin.socket.send_to(data, &remote_addr.into());
                // A failed twin, such as one iOS defuncted, is rebuilt by the next send
                if matches!(&res, Err(e) if e.kind() != io::ErrorKind::WouldBlock) {
                    self.twins.lock().retain(|cached| !Arc::ptr_eq(cached, &twin));
                }
                res
            }
        }
    } else if #[cfg(windows)] {
        /// The IPv4 per-packet TTL cmsg (ws2ipdef.h), which winapi lacks; an `IP_TTL` cmsg
        /// fails with WSAEINVAL
        const IP_HOPLIMIT: i32 = 21;

        impl UdpTtlSender {
            /// Send one UDP datagram on `socket` with a best-effort per-packet TTL (IPv4) or hop
            /// limit (IPv6):
            /// - IPv4: `IP_HOPLIMIT` cmsg via `WSASendMsg`, or the socket's default TTL where
            ///   Windows refuses that cmsg
            /// - IPv6: `IPV6_HOPLIMIT` cmsg via `WSASendMsg`
            ///
            /// Single nonblocking attempt; a full send buffer surfaces as `WouldBlock`.
            pub fn send_to<S: std::os::windows::io::AsSocket>(
                &self,
                socket: &S,
                data: &[u8],
                remote_addr: SocketAddr,
                ttl: u32,
            ) -> io::Result<usize> {
                use winapi::shared::ws2def::{IPPROTO_IP, IPPROTO_IPV6};
                use winapi::shared::ws2ipdef::IPV6_HOPLIMIT;
                use winapi::um::winsock2::WSAEINVAL;

                let ttl = ttl_option_value(ttl)?;
                match remote_addr {
                    SocketAddr::V4(_) => {
                        match wsasendmsg_with_ttl_cmsg(socket, data, remote_addr, IPPROTO_IP, IP_HOPLIMIT, ttl) {
                            Err(e) if e.raw_os_error() == Some(WSAEINVAL) => {
                                socket2_operation(socket, |s| s.send_to(data, &remote_addr.into()))
                            }
                            res => res,
                        }
                    }
                    SocketAddr::V6(_) => wsasendmsg_with_ttl_cmsg(socket, data, remote_addr, IPPROTO_IPV6 as i32, IPV6_HOPLIMIT, ttl),
                }
            }
        }
    }
}

#[cfg(unix)]
fn sendmsg_with_ttl_cmsg(
    fd: std::os::fd::RawFd,
    data: &[u8],
    remote_addr: SocketAddr,
    cmsg_level: libc::c_int,
    cmsg_type: libc::c_int,
    ttl: libc::c_int,
) -> io::Result<usize> {
    let sockaddr = SockAddr::from(remote_addr);
    let mut iov = libc::iovec {
        iov_base: data.as_ptr() as *mut libc::c_void,
        iov_len: data.len(),
    };
    // u64 array for cmsg alignment; larger than CMSG_SPACE(sizeof(int)) everywhere
    let mut cmsg_buf = [0u64; 8];
    // SAFETY: sockaddr, iov and cmsg_buf outlive the call; msg_controllen is
    // CMSG_SPACE(sizeof(int)), which fits in cmsg_buf, so the cmsg writes stay inside it
    unsafe {
        let mut msg: libc::msghdr = std::mem::zeroed();
        msg.msg_name = sockaddr.as_ptr() as *mut libc::c_void;
        msg.msg_namelen = sockaddr.len();
        msg.msg_iov = &mut iov;
        msg.msg_iovlen = 1;
        msg.msg_control = cmsg_buf.as_mut_ptr() as *mut libc::c_void;
        msg.msg_controllen =
            libc::CMSG_SPACE(std::mem::size_of::<libc::c_int>() as libc::c_uint) as _;
        let cmsg = libc::CMSG_FIRSTHDR(&msg);
        (*cmsg).cmsg_level = cmsg_level;
        (*cmsg).cmsg_type = cmsg_type;
        (*cmsg).cmsg_len = libc::CMSG_LEN(std::mem::size_of::<libc::c_int>() as libc::c_uint) as _;
        std::ptr::write_unaligned(libc::CMSG_DATA(cmsg).cast::<libc::c_int>(), ttl);
        let res = libc::sendmsg(fd, &msg, 0);
        if res < 0 {
            return Err(io::Error::last_os_error());
        }
        Ok(res as usize)
    }
}

#[cfg(windows)]
fn wsasendmsg_with_ttl_cmsg<S: std::os::windows::io::AsSocket>(
    socket: &S,
    data: &[u8],
    remote_addr: SocketAddr,
    cmsg_level: i32,
    cmsg_type: i32,
    ttl: i32,
) -> io::Result<usize> {
    use winapi::shared::minwindef::DWORD;
    use winapi::shared::ws2def::{WSABUF, WSACMSGHDR, WSAMSG};
    use winapi::um::winsock2::{WSASendMsg, SOCKET, SOCKET_ERROR};

    // WSA_CMSG_SPACE(sizeof(int)) layout by hand; winapi has no cmsg macros
    #[repr(C)]
    struct CmsgTtl {
        hdr: WSACMSGHDR,
        value: i32,
    }

    let sockaddr = SockAddr::from(remote_addr);
    let mut wsabuf = WSABUF {
        len: data.len() as u32,
        buf: data.as_ptr() as *mut i8,
    };
    let mut cmsg = CmsgTtl {
        hdr: WSACMSGHDR {
            cmsg_len: std::mem::size_of::<WSACMSGHDR>() + std::mem::size_of::<i32>(),
            cmsg_level,
            cmsg_type,
        },
        value: ttl,
    };
    let mut msg = WSAMSG {
        name: sockaddr.as_ptr() as *mut _,
        namelen: sockaddr.len(),
        lpBuffers: &mut wsabuf,
        dwBufferCount: 1,
        Control: WSABUF {
            len: std::mem::size_of::<CmsgTtl>() as u32,
            buf: (&mut cmsg as *mut CmsgTtl) as *mut i8,
        },
        dwFlags: 0,
    };
    let mut sent: DWORD = 0;
    // SAFETY: msg and everything it points to outlive this synchronous, non-overlapped call
    let res = unsafe {
        WSASendMsg(
            std::os::windows::io::AsRawSocket::as_raw_socket(&socket.as_socket()) as SOCKET,
            &mut msg,
            0,
            &mut sent,
            std::ptr::null_mut(),
            None,
        )
    };
    if res == SOCKET_ERROR {
        return Err(io::Error::last_os_error());
    }
    Ok(sent as usize)
}

/// A TTL or hop limit as the int the socket layer takes; values outside 1..=255 are refused
/// before any syscall
fn ttl_option_value(ttl: u32) -> io::Result<i32> {
    if !(1..=255).contains(&ttl) {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "ttl must be in 1..=255",
        ));
    }
    Ok(ttl as i32)
}

// Run operations on underlying socket
cfg_if! {
    if #[cfg(unix)] {
        /// Run `callback` against a borrowed [`socket2::Socket`] view of `s`; the view never
        /// closes the descriptor, even if `callback` unwinds.
        pub fn socket2_operation<S: std::os::fd::AsFd, F: FnOnce(&socket2::Socket) -> R, R>(
            s: &S,
            callback: F,
        ) -> R {
            callback(&socket2::SockRef::from(s))
        }
    } else if #[cfg(windows)] {
        /// Run `callback` against a borrowed [`socket2::Socket`] view of `s`; the view never
        /// closes the socket, even if `callback` unwinds.
        pub fn socket2_operation<
            S: std::os::windows::io::AsSocket,
            F: FnOnce(&socket2::Socket) -> R,
            R,
        >(
            s: &S,
            callback: F,
        ) -> R {
            callback(&socket2::SockRef::from(s))
        }
    } else {
        #[compile_error("unimplemented")]
    }
}
