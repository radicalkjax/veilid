//! `debug chaos`: the settings tree it prints and edits, and what it then does to packets.

use super::*;

fn run(chaos: &Chaos, line: &str) -> Result<serde_json::Value, String> {
    let args: Vec<String> = line.split_whitespace().map(str::to_owned).collect();
    let out = chaos.command(&args)?;
    Ok(serde_json::from_str(&out).expect("chaos prints JSON"))
}

fn ip(text: &str) -> IpAddr {
    text.parse().expect("test address")
}

fn lost_send(chaos: &Chaos, addr: IpAddr) -> bool {
    chaos.send_verdict(addr, 100) == ChaosVerdict::Drop
}

fn lost_recv(chaos: &Chaos, addr: IpAddr) -> bool {
    chaos.recv_verdict(addr, 100) == ChaosVerdict::Drop
}

/// Drops out of `trials` sends to `addr`
fn count_send_drops(chaos: &Chaos, addr: IpAddr, trials: usize) -> usize {
    (0..trials).filter(|_| lost_send(chaos, addr)).count()
}

/// Defaults print in full, and a path prints just its subtree
fn test_print_defaults_and_subtrees() {
    let chaos = Chaos::new();
    assert_eq!(
        run(&chaos, "").unwrap(),
        serde_json::json!({
            "net": {
                "self": {
                    "send": { "loss": 0, "max_size": 0, "rate": 0, "pps": 0, "queue_ms": 0 },
                    "recv": { "loss": 0, "max_size": 0, "rate": 0, "pps": 0, "queue_ms": 0 },
                    "weather": {
                        "drift": 0,
                        "drift_s": 0,
                        "collapse_every_s": 0,
                        "collapse_s": 0,
                        "collapse_cut": 0,
                        "collapse_loss": 0
                    }
                },
                "ip": {},
                "stall_close_ms": 0
            },
            "rpc": { "value_changed": { "loss": 0 } }
        })
    );
    assert_eq!(
        run(&chaos, "net.self.recv.loss").unwrap(),
        serde_json::json!(0)
    );
    assert!(
        run(&chaos, "net.nope").is_err(),
        "an unknown path is an error"
    );
    assert_eq!(
        chaos.send_verdict(ip("1.2.3.4"), 65_000),
        ChaosVerdict::Pass
    );
    assert_eq!(
        chaos.recv_verdict(ip("1.2.3.4"), 65_000),
        ChaosVerdict::Pass
    );
}

/// Node-wide loss applies to every address and only in its direction
fn test_node_wide_loss_and_reset() {
    let chaos = Chaos::new();
    run(&chaos, "net.self.send.loss=100").unwrap();
    assert_eq!(count_send_drops(&chaos, ip("1.2.3.4"), 50), 50);
    assert_eq!(count_send_drops(&chaos, ip("2001:db8::1"), 50), 50);
    assert!(
        !lost_recv(&chaos, ip("1.2.3.4")),
        "send loss leaves receives alone"
    );

    run(&chaos, "net.self.recv.loss=100").unwrap();
    run(&chaos, "reset net.self.send").unwrap();
    assert!(!lost_send(&chaos, ip("1.2.3.4")));
    assert!(lost_recv(&chaos, ip("1.2.3.4")));

    let all = run(&chaos, "reset").unwrap();
    assert_eq!(all["net"]["self"]["recv"]["loss"], 0);
    assert!(!lost_recv(&chaos, ip("1.2.3.4")));
}

/// Address ranges, quoted or not, print as canonical CIDR and match by longest prefix
fn test_address_ranges() {
    let chaos = Chaos::new();
    run(&chaos, "net.ip['10.0.0.0/8'].recv.loss=100").unwrap();
    // The debug command line strips quotes before this sees the path
    let all = run(
        &chaos,
        "net.ip[10.1.0.0/16].recv.loss=0 net.ip[10.1.2.3].send.loss=100",
    )
    .unwrap();
    let ranges = all["net"]["ip"].as_object().unwrap();
    let keys: Vec<&String> = ranges.keys().collect();
    assert_eq!(keys, ["10.0.0.0/8", "10.1.0.0/16", "10.1.2.3/32"]);

    assert!(lost_recv(&chaos, ip("10.9.9.9")), "inside the /8");
    assert!(
        !lost_recv(&chaos, ip("10.1.9.9")),
        "a lossless /16 exempts its addresses"
    );
    assert!(!lost_recv(&chaos, ip("11.0.0.1")), "outside every range");
    assert!(lost_send(&chaos, ip("10.1.2.3")));
    assert!(!lost_send(&chaos, ip("10.1.2.4")));

    // Dual-stack sockets report IPv4 peers as IPv4-mapped IPv6
    assert!(lost_recv(&chaos, ip("::ffff:10.9.9.9")));

    run(&chaos, "net.ip['2001:db8:abcd:0001::/64'].recv.loss=100").unwrap();
    assert_eq!(
        run(&chaos, "net.ip['2001:db8:abcd:1::/64']").unwrap(),
        serde_json::json!({
            "send": { "loss": 0, "max_size": 0 },
            "recv": { "loss": 100, "max_size": 0 }
        })
    );
    assert!(lost_recv(&chaos, ip("2001:db8:abcd:1::5")));
    assert!(!lost_recv(&chaos, ip("2001:db8:abcd:2::5")));

    run(&chaos, "reset net.ip['10.0.0.0/8']").unwrap();
    assert!(
        !lost_recv(&chaos, ip("10.9.9.9")),
        "resetting a range removes it"
    );
    run(&chaos, "reset net.ip").unwrap();
    assert!(run(&chaos, "net.ip")
        .unwrap()
        .as_object()
        .unwrap()
        .is_empty());
}

/// A bad command changes nothing
fn test_rejected_input() {
    let chaos = Chaos::new();
    run(&chaos, "net.self.send.loss=20").unwrap();
    let before = run(&chaos, "").unwrap();
    for line in [
        "net.self.send.loss=101",
        "net.self.sned.loss=1",
        "net.self.send.loss=abc",
        "net.self.send.loss=-1",
        "net.ip['300.1.1.1'].send.loss=1",
        "net.ip['1.2.3.4/33'].send.loss=1",
        "net.ip['1.2.3.4'.send.loss=1",
        "net.self.recv.loss=5 net.self.send.loss=200",
        "net.self.send.rate=-1",
        "net.self.weather.drift=101",
        "net.self.weather.collapse_cut=101",
        "net.self.weather.collapse_loss=101",
        "net.ip['1.2.3.4'].send.rate=1000",
        "reset net.self net.ip",
    ] {
        assert!(run(&chaos, line).is_err(), "'{}' should be rejected", line);
        assert_eq!(
            run(&chaos, "").unwrap(),
            before,
            "'{}' changed settings",
            line
        );
    }
}

/// Partial loss drops about that share, and node-wide and range losses roll independently
fn test_loss_rates() {
    let chaos = Chaos::new();
    let trials = 4000;
    run(&chaos, "net.self.send.loss=50").unwrap();
    let half = count_send_drops(&chaos, ip("1.2.3.4"), trials);
    assert!(
        (1700..=2300).contains(&half),
        "50% dropped {} of {}",
        half,
        trials
    );

    run(&chaos, "net.ip['1.2.3.0/24'].send.loss=50").unwrap();
    let both = count_send_drops(&chaos, ip("1.2.3.4"), trials);
    assert!(
        (2700..=3300).contains(&both),
        "50% then 50% dropped {} of {}",
        both,
        trials
    );

    let stats = run(&chaos, "stats").unwrap();
    assert_eq!(stats["send"]["lost"], (half + both) as u64);
    assert_eq!(stats["recv"]["lost"], 0);
}

/// A size cap loses larger packets only, and the smaller of the node's and a range's cap applies
fn test_size_caps() {
    let chaos = Chaos::new();
    run(
        &chaos,
        "net.self.send.max_size=1400 net.ip['0.0.0.0/0'].send.max_size=1200",
    )
    .unwrap();
    let v4 = ip("192.0.2.9");
    let v6 = ip("2001:db8::9");
    assert_eq!(chaos.send_verdict(v4, 1200), ChaosVerdict::Pass);
    assert_eq!(chaos.send_verdict(v4, 1201), ChaosVerdict::Oversize);
    assert_eq!(
        chaos.send_verdict(v6, 1400),
        ChaosVerdict::Pass,
        "the IPv4 range leaves IPv6 alone"
    );
    assert_eq!(chaos.send_verdict(v6, 1401), ChaosVerdict::Oversize);
    assert_eq!(
        chaos.recv_verdict(v4, 9000),
        ChaosVerdict::Pass,
        "caps are per direction"
    );
    assert_eq!(run(&chaos, "stats").unwrap()["send"]["oversize"], 2);
}

/// Back-to-back packets wait their turn on a rate-limited link, and the queue loses any that would
/// wait longer than it holds
fn test_link_queue() {
    let chaos = Chaos::new();
    // 1000 bytes take 10 ms; the queue holds 100 ms of waiting
    run(
        &chaos,
        "net.self.send.rate=100000 net.self.send.queue_ms=100",
    )
    .unwrap();
    let verdicts: Vec<ChaosVerdict> = (0..20)
        .map(|_| chaos.send_verdict(ip("192.0.2.9"), 1000))
        .collect();
    let holds: Vec<u32> = verdicts
        .iter()
        .filter_map(|verdict| match verdict {
            ChaosVerdict::Hold(wait_ms) => Some(*wait_ms),
            _ => None,
        })
        .collect();
    assert_eq!(holds.len(), 11, "11 fit in the queue: {:?}", verdicts);
    assert!(
        holds.windows(2).all(|pair| pair[0] < pair[1]),
        "each waits longer than the last: {:?}",
        holds
    );
    assert!((9..=10).contains(&holds[0]), "first {:?}", holds);
    assert!((109..=110).contains(&holds[10]), "last {:?}", holds);
    assert!(verdicts[11..].iter().all(|v| *v == ChaosVerdict::Drop));
    assert_eq!(
        chaos.recv_verdict(ip("192.0.2.9"), 1000),
        ChaosVerdict::Pass,
        "the receive side has its own link"
    );

    let stats = run(&chaos, "stats").unwrap();
    assert_eq!(stats["send"]["held"], 11);
    assert_eq!(stats["send"]["overflowed"], 9);
    assert!(stats["send"]["backlog_ms"].as_u64().unwrap() > 100);

    // Packets a second bound the link too: each takes 10 ms however small
    run(&chaos, "reset net.self.send").unwrap();
    run(&chaos, "net.self.recv.pps=100 net.self.recv.queue_ms=50").unwrap();
    let held = (0..10)
        .filter(|_| {
            matches!(
                chaos.recv_verdict(ip("192.0.2.9"), 1),
                ChaosVerdict::Hold(_)
            )
        })
        .count();
    assert_eq!(held, 6);
}

/// Drift takes capacity away. A collapse cuts capacity, which slows the link and stretches the
/// waits its queue holds, and loses packets at random, for as long as it lasts.
async fn test_weather() {
    let drifting = Chaos::new();
    run(
        &drifting,
        "net.self.weather.drift=50 net.self.weather.drift_s=3600",
    )
    .unwrap();
    let capacity = run(&drifting, "stats").unwrap()["capacity"]
        .as_u64()
        .unwrap();
    assert!((50..=100).contains(&capacity), "drift to {}", capacity);

    // A first collapse starts 0.5 to 1.5 s after the first packet or stats, and outlasts the test
    let collapse = "net.self.weather.collapse_every_s=1 net.self.weather.collapse_s=3600";
    let halved = Chaos::new();
    run(
        &halved,
        &format!("{} net.self.weather.collapse_cut=50", collapse),
    )
    .unwrap();
    // 1000 bytes take 10 ms at full capacity, and the queue holds 100 ms of them
    run(
        &halved,
        "net.self.send.rate=100000 net.self.send.queue_ms=100",
    )
    .unwrap();
    let lossy = Chaos::new();
    run(
        &lossy,
        &format!("{} net.self.weather.collapse_loss=100", collapse),
    )
    .unwrap();
    for chaos in [&halved, &lossy] {
        let before = run(chaos, "stats").unwrap();
        assert_eq!(before["capacity"], 100);
        assert_eq!(before["collapsed"], false);
        assert_eq!(before["collapses"], 0);
    }
    assert!(!lost_send(&lossy, ip("192.0.2.9")));

    sleep(1600).await;
    let during = run(&halved, "stats").unwrap();
    assert_eq!(during["capacity"], 50);
    assert_eq!(during["collapsed"], true);
    assert_eq!(during["collapses"], 1);
    let holds: Vec<u32> = (0..30)
        .filter_map(|_| match halved.send_verdict(ip("192.0.2.9"), 1000) {
            ChaosVerdict::Hold(wait_ms) => Some(wait_ms),
            _ => None,
        })
        .collect();
    assert_eq!(
        holds.len(),
        11,
        "the queue holds 200 ms at half capacity: {:?}",
        holds
    );
    assert!((19..=20).contains(&holds[0]), "first {:?}", holds);
    assert!((219..=220).contains(&holds[10]), "last {:?}", holds);

    assert!(
        (0..20).all(|_| lost_send(&lossy, ip("192.0.2.9"))),
        "a collapse loses packets without any rate set"
    );
}

/// Stands in for a TCP or WS connection: records sends, replays an inbox, then reports the
/// connection closed or waits forever
#[derive(Debug)]
struct FakeConnection {
    flow: Flow,
    sent: Arc<Mutex<Vec<Bytes>>>,
    inbox: Mutex<VecDeque<Bytes>>,
    hang_when_empty: bool,
}

impl PlatformProtocolNetworkConnection for FakeConnection {
    fn flow(&self) -> Flow {
        self.flow
    }
    fn direction(&self) -> Direction {
        Direction::Out
    }
    fn close(&self) -> PinBoxFuture<'_, std::io::Result<NetworkResult<()>>> {
        Box::pin(async { Ok(NetworkResult::value(())) })
    }
    fn send(
        &self,
        message: Bytes,
        _mtu: u16,
    ) -> PinBoxFuture<'_, std::io::Result<NetworkResult<()>>> {
        self.sent.lock().push(message);
        Box::pin(async { Ok(NetworkResult::value(())) })
    }
    fn recv(&self) -> PinBoxFuture<'_, std::io::Result<NetworkResult<Bytes>>> {
        let next = self.inbox.lock().pop_front();
        if next.is_none() && self.hang_when_empty {
            return Box::pin(futures_util::future::pending());
        }
        Box::pin(async move {
            Ok(match next {
                Some(message) => NetworkResult::value(message),
                None => NetworkResult::no_connection_other("closed"),
            })
        })
    }
}

/// A wrapped TCP connection to 192.0.2.7 that will receive `inbox`, and what it sends
fn wrapped_fake(
    chaos: &Chaos,
    inbox: &[&'static [u8]],
    segment_size: Option<u32>,
    hang_when_empty: bool,
) -> (ProtocolNetworkConnection, Arc<Mutex<Vec<Bytes>>>) {
    let sent = Arc::new(Mutex::new(Vec::new()));
    let fake = FakeConnection {
        flow: Flow::new(
            PeerAddress::new(
                SocketAddress::from_socket_addr("192.0.2.7:5150".parse().unwrap()),
                ProtocolType::TCP,
            ),
            SocketAddress::from_socket_addr("192.0.2.1:5150".parse().unwrap()),
        ),
        sent: sent.clone(),
        inbox: Mutex::new(inbox.iter().map(|m| Bytes::from_static(m)).collect()),
        hang_when_empty,
    };
    (chaos.wrap_connection(Box::new(fake), segment_size), sent)
}

/// A wrapped connection's dropped sends report success without reaching the socket, and its
/// dropped receives are skipped
async fn test_wrapped_connection() {
    let chaos = Chaos::new();
    let (conn, sent) = wrapped_fake(&chaos, &[b"one", b"two", b"three"], None, false);

    run(&chaos, "net.ip['192.0.2.7'].send.loss=100").unwrap();
    let res = conn
        .send(Bytes::from_static(b"lost"), u16::MAX)
        .await
        .unwrap();
    assert!(
        matches!(res, NetworkResult::Value(())),
        "a dropped send reports success"
    );
    assert!(sent.lock().is_empty(), "and never reaches the socket");
    let res = conn
        .send(Bytes::from_static(b"kept"), u16::MAX)
        .await
        .unwrap();
    assert!(matches!(res, NetworkResult::Value(())));
    assert_eq!(sent.lock().len(), 0, "the range still drops it");

    run(&chaos, "reset").unwrap();
    let res = conn
        .send(Bytes::from_static(b"kept"), u16::MAX)
        .await
        .unwrap();
    assert!(matches!(res, NetworkResult::Value(())));
    assert_eq!(sent.lock().len(), 1, "with no loss the send goes out");
    assert!(matches!(conn.recv().await.unwrap(), NetworkResult::Value(m) if m.as_ref() == b"one"));

    run(&chaos, "net.self.recv.loss=100").unwrap();
    let res = conn.recv().await.unwrap();
    assert!(
        res.is_no_connection(),
        "every remaining frame was discarded"
    );

    let stats = run(&chaos, "stats").unwrap();
    assert_eq!(stats["send"]["lost"], 2);
    assert_eq!(stats["recv"]["lost"], 2);
}

/// A held frame goes out once the link has carried it; a frame over a size cap stalls its
/// direction of the connection for good
async fn test_wrapped_connection_link() {
    let chaos = Chaos::new();
    let (conn, sent) = wrapped_fake(&chaos, &[b"small", b"far too big", b"ok"], None, false);

    // 1000 bytes a second: 50 bytes take 50 ms
    run(
        &chaos,
        "net.self.send.rate=1000 net.self.send.queue_ms=1000",
    )
    .unwrap();
    let start = Timestamp::now();
    let res = conn
        .send(Bytes::from(vec![0u8; 50]), u16::MAX)
        .await
        .unwrap();
    assert!(matches!(res, NetworkResult::Value(())));
    let took = TimestampDuration::since(start);
    assert!(took >= TimestampDuration::new_ms(45), "held {}", took);
    assert_eq!(sent.lock().len(), 1);

    run(&chaos, "reset net.self.send").unwrap();
    run(
        &chaos,
        "net.self.send.max_size=10 net.self.recv.max_size=10",
    )
    .unwrap();
    for size in [11, 1] {
        let res = conn
            .send(Bytes::from(vec![0u8; size]), u16::MAX)
            .await
            .unwrap();
        assert!(
            matches!(res, NetworkResult::Value(())),
            "a stalled send reports success"
        );
    }
    assert_eq!(
        sent.lock().len(),
        1,
        "a stalled connection sends nothing more"
    );

    assert!(
        matches!(conn.recv().await.unwrap(), NetworkResult::Value(m) if m.as_ref() == b"small")
    );
    assert!(
        conn.recv().await.unwrap().is_no_connection(),
        "the oversize frame and every frame after it are lost"
    );

    let stats = run(&chaos, "stats").unwrap();
    assert_eq!(stats["send"]["oversize"], 1);
    assert_eq!(stats["recv"]["oversize"], 1);
}

/// A frame crosses a size cap in its connection's TCP segments: a clamped stream carries any frame,
/// and an unclamped one stalls on its first big frame
async fn test_wrapped_connection_segments() {
    let chaos = Chaos::new();
    run(&chaos, "preset mtu-blackhole").unwrap();
    let (clamped, clamped_sent) = wrapped_fake(&chaos, &[], Some(1200), false);
    let (plain, plain_sent) = wrapped_fake(&chaos, &[], Some(1460), false);
    for conn in [&clamped, &plain] {
        let res = conn
            .send(Bytes::from(vec![0u8; 16000]), u16::MAX)
            .await
            .unwrap();
        assert!(matches!(res, NetworkResult::Value(())));
    }
    assert_eq!(
        clamped_sent.lock().len(),
        1,
        "1200-byte segments pass a 1380-byte cap"
    );
    assert!(plain_sent.lock().is_empty(), "1460-byte segments do not");
}

/// A stalled stream closes once `net.stall_close_ms` has passed: a receive as soon as it is due,
/// a send at the first send after it
async fn test_wrapped_connection_stall_close() {
    let chaos = Chaos::new();
    run(
        &chaos,
        "net.self.send.max_size=10 net.self.recv.max_size=10 net.stall_close_ms=50",
    )
    .unwrap();

    let (conn, _) = wrapped_fake(&chaos, &[b"far too big"], None, true);
    let start = Timestamp::now();
    assert!(conn.recv().await.unwrap().is_no_connection());
    let took = TimestampDuration::since(start);
    assert!(
        took >= TimestampDuration::new_ms(45),
        "closed after {}",
        took
    );

    let (conn, sent) = wrapped_fake(&chaos, &[], None, true);
    for message in [vec![0u8; 11], b"ok".to_vec()] {
        let res = conn.send(Bytes::from(message), u16::MAX).await.unwrap();
        assert!(
            matches!(res, NetworkResult::Value(())),
            "lost until the deadline"
        );
    }
    sleep(60).await;
    assert!(conn
        .send(Bytes::from_static(b"ok"), u16::MAX)
        .await
        .unwrap()
        .is_no_connection());
    assert!(sent.lock().is_empty());
}

/// Presets load over defaults in the order given, assignments override them, and an unknown name
/// changes nothing
fn test_presets() {
    let chaos = Chaos::new();
    let list = run(&chaos, "preset").unwrap();
    for name in ["mobile", "mobile-bad", "mtu-blackhole"] {
        assert!(list[name]["about"].is_string(), "{} is listed", name);
        run(&chaos, &format!("preset {}", name)).unwrap();
    }

    let all = run(
        &chaos,
        "preset mobile-bad mtu-blackhole net.self.send.rate=1000",
    )
    .unwrap();
    assert_eq!(all["net"]["self"]["send"]["rate"], 1000, "assignments win");
    assert_eq!(all["net"]["self"]["recv"]["rate"], 600000);
    assert_eq!(all["net"]["ip"]["0.0.0.0/0"]["send"]["max_size"], 1380);
    assert_eq!(all["net"]["stall_close_ms"], 70000);

    let before = run(&chaos, "").unwrap();
    assert!(run(&chaos, "preset mobile nope").is_err());
    assert_eq!(
        run(&chaos, "").unwrap(),
        before,
        "a bad preset changes nothing"
    );

    run(&chaos, "rpc.value_changed.loss=50").unwrap();
    assert_eq!(
        run(&chaos, "preset mobile").unwrap()["rpc"]["value_changed"]["loss"],
        0,
        "a preset starts from defaults"
    );
}

/// ValueChanged loss discards pushes on their own, leaving packets alone, and counts them
fn test_value_changed_loss() {
    let chaos = Chaos::new();
    assert!(!chaos.drop_value_changed());
    run(&chaos, "rpc.value_changed.loss=100").unwrap();
    assert!((0..20).all(|_| chaos.drop_value_changed()));
    assert!(!lost_send(&chaos, ip("1.2.3.4")) && !lost_recv(&chaos, ip("1.2.3.4")));
    assert_eq!(
        run(&chaos, "stats").unwrap()["value_changed"]["lost"],
        serde_json::json!(20)
    );
    assert!(run(&chaos, "rpc.value_changed.loss=101").is_err());
    run(&chaos, "reset rpc").unwrap();
    assert!(!chaos.drop_value_changed());
}

pub async fn test_all() {
    test_value_changed_loss();
    test_print_defaults_and_subtrees();
    test_node_wide_loss_and_reset();
    test_address_ranges();
    test_rejected_input();
    test_loss_rates();
    test_size_caps();
    test_link_queue();
    test_weather().await;
    test_presets();
    test_wrapped_connection().await;
    test_wrapped_connection_link().await;
    test_wrapped_connection_segments().await;
    test_wrapped_connection_stall_close().await;
}
