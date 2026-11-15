use crate::*;

async fn test_send_with_ttl(local: SocketAddr) {
    info!("-- test_send_with_ttl {}", local);

    let receiver = bind_async_udp_socket(local)
        .expect("receiver bind should not error")
        .expect("receiver bind should succeed");
    let recv_addr = receiver.local_addr().expect("should have local addr");
    let sender = bind_async_udp_socket(local)
        .expect("sender bind should not error")
        .expect("sender bind should succeed");
    let ttl_sender = UdpTtlSender::new();

    let read_ttl = || {
        socket2_operation(&sender, |s| {
            if recv_addr.is_ipv6() {
                s.unicast_hops_v6()
            } else {
                s.ttl_v4()
            }
        })
        .expect("should get ttl")
    };

    let ttl_before = read_ttl();

    let sent = ttl_sender
        .send_to(&sender, &[], recv_addr, 3)
        .expect("zero-length send with ttl");
    assert_eq!(sent, 0);
    let sent = ttl_sender
        .send_to(&sender, b"punch", recv_addr, 3)
        .expect("payload send with ttl");
    assert_eq!(sent, 5);

    // Receive both datagrams; delivery order is not guaranteed
    let mut lens = Vec::new();
    let mut buf = [0u8; 64];
    for _ in 0..2 {
        let (n, _) = timeout(5000, receiver.recv_from(&mut buf))
            .await
            .expect("should not time out")
            .expect("should receive");
        if n == 5 {
            assert_eq!(&buf[..5], b"punch");
        }
        lens.push(n);
    }
    lens.sort();
    assert_eq!(lens, vec![0, 5]);

    let ttl_after = read_ttl();
    assert_eq!(ttl_before, ttl_after);

    // Out-of-range TTLs are refused before any syscall, on both address families
    for bad in [0u32, 256, u32::MAX] {
        let err = ttl_sender
            .send_to(&sender, b"x", recv_addr, bad)
            .expect_err("ttl out of range should fail");
        assert_eq!(err.kind(), std::io::ErrorKind::InvalidInput);
    }
    let sent = ttl_sender
        .send_to(&sender, b"max", recv_addr, 255)
        .expect("ttl 255");
    assert_eq!(sent, 3);
    let (n, _) = timeout(5000, receiver.recv_from(&mut buf))
        .await
        .expect("should not time out")
        .expect("should receive");
    assert_eq!(&buf[..n], b"max");
}

/// Have `socket` report each datagram's TTL or hop limit
#[cfg(any(
    target_os = "linux",
    target_os = "android",
    target_os = "macos",
    target_os = "ios"
))]
fn enable_recv_ttl(socket: &std::net::UdpSocket, v6: bool) {
    use std::os::fd::AsRawFd;
    let (level, name) = if v6 {
        (libc::IPPROTO_IPV6, libc::IPV6_RECVHOPLIMIT)
    } else {
        (libc::IPPROTO_IP, libc::IP_RECVTTL)
    };
    let on: libc::c_int = 1;
    // SAFETY: the fd is open for the call and `on` outlives it
    let res = unsafe {
        libc::setsockopt(
            socket.as_raw_fd(),
            level,
            name,
            (&on as *const libc::c_int).cast(),
            std::mem::size_of::<libc::c_int>() as libc::socklen_t,
        )
    };
    assert_eq!(
        res,
        0,
        "enable ttl reporting: {}",
        std::io::Error::last_os_error()
    );
}

/// Receive one datagram and the TTL or hop limit it arrived with
#[cfg(any(
    target_os = "linux",
    target_os = "android",
    target_os = "macos",
    target_os = "ios"
))]
fn recv_with_ttl(socket: &std::net::UdpSocket) -> (Vec<u8>, i32) {
    use std::os::fd::AsRawFd;
    let mut data = [0u8; 64];
    let mut iov = libc::iovec {
        iov_base: data.as_mut_ptr().cast(),
        iov_len: data.len(),
    };
    let mut cmsg_buf = [0u64; 16];
    // SAFETY: msg points at iov, data and cmsg_buf, which outlive the call, and only the
    // first cmsg the kernel wrote is read
    let (len, ttl) = unsafe {
        let mut msg: libc::msghdr = std::mem::zeroed();
        msg.msg_iov = &mut iov;
        msg.msg_iovlen = 1;
        msg.msg_control = cmsg_buf.as_mut_ptr().cast();
        msg.msg_controllen = std::mem::size_of_val(&cmsg_buf) as _;
        let len = libc::recvmsg(socket.as_raw_fd(), &mut msg, 0);
        assert!(len >= 0, "recvmsg: {}", std::io::Error::last_os_error());
        let cmsg = libc::CMSG_FIRSTHDR(&msg);
        assert!(!cmsg.is_null(), "datagram should carry its ttl");
        let value = libc::CMSG_DATA(cmsg);
        let ttl = match ((*cmsg).cmsg_level, (*cmsg).cmsg_type) {
            (libc::IPPROTO_IPV6, libc::IPV6_HOPLIMIT) => {
                std::ptr::read_unaligned(value.cast::<libc::c_int>())
            }
            // Linux reports an int under IP_TTL, xnu a byte under IP_RECVTTL
            #[cfg(any(target_os = "linux", target_os = "android"))]
            (libc::IPPROTO_IP, libc::IP_TTL) => {
                std::ptr::read_unaligned(value.cast::<libc::c_int>())
            }
            #[cfg(any(target_os = "macos", target_os = "ios"))]
            (libc::IPPROTO_IP, libc::IP_RECVTTL) => *value as libc::c_int,
            other => panic!("unexpected cmsg {:?}", other),
        };
        (len as usize, ttl)
    };
    (data[..len].to_vec(), ttl)
}

/// A punch from a wildcard-bound socket, like a listener's, arrives with its TTL and from
/// the socket's own port, so it opens the NAT mapping the socket's traffic uses; the
/// socket keeps its inbound traffic while the sender stays alive
#[cfg(any(
    target_os = "linux",
    target_os = "android",
    target_os = "macos",
    target_os = "ios"
))]
async fn test_ttl_on_the_wire(wildcard: SocketAddr, loopback: IpAddr) {
    info!("-- test_ttl_on_the_wire {}", wildcard);

    let receiver = std::net::UdpSocket::bind(SocketAddr::new(loopback, 0))
        .expect("receiver bind should succeed");
    receiver
        .set_read_timeout(Some(std::time::Duration::from_secs(5)))
        .expect("should set read timeout");
    enable_recv_ttl(&receiver, wildcard.is_ipv6());
    let recv_addr = receiver.local_addr().expect("should have local addr");

    let sender = bind_async_udp_socket(wildcard)
        .expect("sender bind should not error")
        .expect("sender bind should succeed");
    let sender_port = sender.local_addr().expect("should have local addr").port();
    let ttl_sender = UdpTtlSender::new();

    let sent = ttl_sender
        .send_to(&sender, b"punch", recv_addr, 3)
        .expect("send with ttl");
    assert_eq!(sent, 5);
    let mut buf = [0u8; 64];
    let (_, from) = receiver
        .peek_from(&mut buf)
        .expect("should receive the punch");
    assert_eq!(from.port(), sender_port);
    assert_eq!(recv_with_ttl(&receiver), (b"punch".to_vec(), 3));

    // A later send asking for another TTL gets it
    ttl_sender
        .send_to(&sender, b"again", recv_addr, 6)
        .expect("send with ttl");
    assert_eq!(recv_with_ttl(&receiver), (b"again".to_vec(), 6));

    // The shared socket keeps its own TTL and still receives on its port
    sender
        .send_to(b"plain", recv_addr)
        .await
        .expect("plain send");
    let (data, ttl) = recv_with_ttl(&receiver);
    assert_eq!(data, b"plain");
    assert_ne!(ttl, 3);
    receiver
        .send_to(b"back", SocketAddr::new(loopback, sender_port))
        .expect("reply send");
    let (n, _) = timeout(5000, sender.recv_from(&mut buf))
        .await
        .expect("should not time out")
        .expect("should receive");
    assert_eq!(&buf[..n], b"back");
}

/// An IPv6 destination on an IPv4 socket fails inside sendmsg, not before it
fn test_send_family_mismatch() {
    info!("-- test_send_family_mismatch");
    let sender = bind_async_udp_socket(SocketAddr::V4(SocketAddrV4::new(Ipv4Addr::LOCALHOST, 0)))
        .expect("bind should not error")
        .expect("bind should succeed");
    let v6 = SocketAddr::V6(SocketAddrV6::new(Ipv6Addr::LOCALHOST, 5150, 0, 0));
    assert!(UdpTtlSender::new().send_to(&sender, b"x", v6, 3).is_err());
}

pub async fn test_all() {
    test_send_family_mismatch();
    test_send_with_ttl(SocketAddr::V4(SocketAddrV4::new(Ipv4Addr::LOCALHOST, 0))).await;
    test_send_with_ttl(SocketAddr::V6(SocketAddrV6::new(
        Ipv6Addr::LOCALHOST,
        0,
        0,
        0,
    )))
    .await;
    #[cfg(any(
        target_os = "linux",
        target_os = "android",
        target_os = "macos",
        target_os = "ios"
    ))]
    {
        test_ttl_on_the_wire(
            SocketAddr::V4(SocketAddrV4::new(Ipv4Addr::UNSPECIFIED, 0)),
            IpAddr::V4(Ipv4Addr::LOCALHOST),
        )
        .await;
        test_ttl_on_the_wire(
            SocketAddr::V6(SocketAddrV6::new(Ipv6Addr::UNSPECIFIED, 0, 0, 0)),
            IpAddr::V6(Ipv6Addr::LOCALHOST),
        )
        .await;
    }
}
