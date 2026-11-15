use crate::*;

cfg_if! {
    if #[cfg(feature="rt-async-std")] {
        use async_std::net::{TcpListener, TcpStream};
    } else if #[cfg(feature="rt-tokio")] {
        use tokio::net::{TcpListener, TcpStream};
    }
}

/// Linger and other option changes reach the socket through a borrowed view that never
/// closes it, so the stream stays usable afterwards
pub async fn test_borrowed_socket_operations() {
    info!("-- test_borrowed_socket_operations");
    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .expect("listener bind");
    let addr = listener.local_addr().expect("local addr");
    let (client, server) =
        futures_util::future::join(TcpStream::connect(addr), listener.accept()).await;
    let client = client.expect("connect");
    let (_server, _) = server.expect("accept");

    set_tcp_stream_linger(&client, Some(core::time::Duration::from_secs(0))).expect("linger on");
    let linger = socket2_operation(&client, |s| s.linger()).expect("read linger");
    assert_eq!(linger, Some(core::time::Duration::from_secs(0)));
    set_tcp_stream_linger(&client, None).expect("linger off");
    assert_eq!(
        socket2_operation(&client, |s| s.linger()).expect("read linger"),
        None
    );

    // The stream still works, so nothing above closed its descriptor
    assert!(client.local_addr().is_ok());
    assert!(client.peer_addr().is_ok());
}

/// A loopback connection, both ends, made with `mss` on the listener, the connect and the accepted
/// stream
async fn mss_pair(mss: Option<u32>) -> (TcpStream, TcpStream) {
    let any = SocketAddr::V4(SocketAddrV4::new(Ipv4Addr::LOCALHOST, 0));
    let listener = bind_async_tcp_listener(any, mss)
        .expect("bind should not error")
        .expect("bind should succeed");
    let addr = listener.local_addr().expect("local addr");
    let (client, server) = futures_util::future::join(
        connect_async_tcp_stream(None, addr, 1000, mss),
        listener.accept(),
    )
    .await;
    let TimeoutOr::Value(client) = client.expect("connect") else {
        panic!("connect timed out");
    };
    let (server, _) = server.expect("accept");
    if let Some(mss) = mss {
        limit_tcp_stream_mss(&server, mss).expect("limit accepted stream");
    }
    (client, server)
}

/// A clamp keeps both ends' segments within it, where loopback alone would carry far larger ones
pub async fn test_tcp_mss_clamp() {
    info!("-- test_tcp_mss_clamp");
    let (client, server) = mss_pair(None).await;
    let plain = tcp_stream_mss(&client).expect("client mss");
    assert!(plain > 1200, "loopback segments of {}", plain);
    drop((client, server));

    let (client, server) = mss_pair(Some(1200)).await;
    for (end, stream) in [("client", &client), ("server", &server)] {
        let mss = tcp_stream_mss(stream).expect("mss");
        assert!(mss <= 1200, "{} sends segments of {}", end, mss);
    }
}

/// A lowered segment limit cuts a large write into pieces that arrive whole and in order
pub async fn test_segment_limited_stream() {
    info!("-- test_segment_limited_stream");
    use futures_util::{AsyncReadExt, AsyncWriteExt};

    let (client, server) = mss_pair(None).await;
    let (mut client, segment_limit) = SegmentLimitedTcpStream::new(client);
    assert_eq!(segment_limit.get(), None);
    segment_limit.lower(1200);
    segment_limit.lower(1400);
    assert_eq!(segment_limit.get(), Some(1200), "a limit only ever lowers");

    let message: Vec<u8> = (0..65536u32).map(|n| n as u8).collect();
    let (mut server, _) = SegmentLimitedTcpStream::new(server);
    // Read exactly the message: async-std's close does not shut the stream down
    let (write_res, read_res) = futures_util::future::join(
        async {
            client.write_all(&message).await?;
            client.flush().await
        },
        async {
            let mut received = vec![0u8; message.len()];
            server.read_exact(&mut received).await.map(|_| received)
        },
    )
    .await;
    write_res.expect("write");
    assert_eq!(read_res.expect("read"), message);
}

/// A port unreachable for one of our sends stays out of the next receive, as on other platforms
#[cfg(windows)]
pub async fn test_udp_icmp_errors_stay_out_of_recv() {
    info!("-- test_udp_icmp_errors_stay_out_of_recv");
    let socket = bind_async_udp_socket(SocketAddr::V4(SocketAddrV4::new(Ipv4Addr::LOCALHOST, 0)))
        .expect("bind should not error")
        .expect("bind should succeed");
    // A loopback port nobody listens on
    let closed = std::net::UdpSocket::bind("127.0.0.1:0")
        .expect("probe bind")
        .local_addr()
        .expect("probe addr");
    socket.send_to(b"x", closed).await.expect("send");
    let mut buf = [0u8; 16];
    let res = timeout(500, socket.recv_from(&mut buf)).await;
    assert!(res.is_err(), "receive should wait, not fail: {:?}", res);
}

/// IPv4 datagrams leave without DF, so a router on a smaller-MTU path fragments them
#[cfg(any(target_os = "linux", target_os = "android"))]
pub fn test_udp_ipv4_fragmentation_allowed() {
    use std::os::fd::AsRawFd;

    info!("-- test_udp_ipv4_fragmentation_allowed");
    let socket = bind_async_udp_socket(SocketAddr::V4(SocketAddrV4::new(Ipv4Addr::LOCALHOST, 0)))
        .expect("bind should not error")
        .expect("bind should succeed");
    let mut value: libc::c_int = -1;
    let mut len = core::mem::size_of::<libc::c_int>() as libc::socklen_t;
    // SAFETY: the socket is open, and `value` and `len` outlive this synchronous call
    let res = unsafe {
        libc::getsockopt(
            socket.as_raw_fd(),
            libc::IPPROTO_IP,
            libc::IP_MTU_DISCOVER,
            (&mut value as *mut libc::c_int).cast(),
            &mut len,
        )
    };
    assert_eq!(res, 0, "getsockopt: {}", std::io::Error::last_os_error());
    assert_eq!(value, libc::IP_PMTUDISC_DONT);
}

pub async fn test_all() {
    test_borrowed_socket_operations().await;
    test_tcp_mss_clamp().await;
    test_segment_limited_stream().await;
    #[cfg(windows)]
    test_udp_icmp_errors_stay_out_of_recv().await;
    #[cfg(any(target_os = "linux", target_os = "android"))]
    test_udp_ipv4_fragmentation_allowed();
}
