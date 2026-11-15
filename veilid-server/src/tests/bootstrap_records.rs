//! Whether a node answers direct (B01T) bootstrap requests is decided by veilid-server's own
//! config mapping, so it is tested here and not only in veilid-core.

use crate::settings::*;
use crate::tools::block_on;
use std::ffi::OsString;
use std::io::{Read as _, Write as _};
use std::net::{IpAddr, TcpListener, TcpStream, UdpSocket};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Arc, OnceLock};
use std::time::Duration;
use tracing_subscriber::prelude::*;
use veilid_core::tools::sleep;
use veilid_core::*;

const B01T_MAGIC: &[u8; 4] = b"B01T";
const NETWORK_START_TIMEOUT_MS: u32 = 30_000;
const BOOTSTRAP_RECORDS_WAIT_MS: u32 = 90_000;
const POLL_MS: u32 = 250;
const PROBE_TIMEOUT: Duration = Duration::from_secs(5);
/// RFC 2606: never resolves, so a path that must not do DNS fails loudly if it ever tries
const UNRESOLVABLE_HOST: &str = "bootstrap.invalid";

static TEST_SEQ: AtomicU32 = AtomicU32::new(0);

fn test_dir(label: &str) -> PathBuf {
    let seq = TEST_SEQ.fetch_add(1, Ordering::AcqRel);
    let dir = std::env::temp_dir().join(format!(
        "veilid-server-test-{}-{}-{}",
        std::process::id(),
        label,
        seq
    ));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("should make test dir");
    dir
}

/// A config file shaped like a real one, so what is under test is the production mapping
fn write_config(dir: &Path, network: &str) -> PathBuf {
    write_config_with_state(dir, network, true)
}

/// `delete_state` false keeps the table store, which is how a restart is tested
fn write_config_with_state(dir: &Path, network: &str, delete_state: bool) -> PathBuf {
    let conf = format!(
        r#"
daemon:
    enabled: false
client_api:
    ipc_enabled: false
    network_enabled: false
auto_attach: false
logging:
    system:
        enabled: false
    terminal:
        enabled: false
    file:
        enabled: false
    api:
        enabled: false
core:
    protected_store:
        allow_insecure_fallback: true
        always_use_insecure_storage: true
        directory: '{dir}/protected_store'
        delete: {delete_state}
    table_store:
        directory: '{dir}/table_store'
        delete: {delete_state}
    block_store:
        directory: '{dir}/block_store'
        delete: {delete_state}
    network:
        upnp: false
        detect_address_changes: false
{network}
"#,
        dir = dir.display(),
        delete_state = delete_state,
    );
    let path = dir.join("veilid-server.conf");
    std::fs::write(&path, conf).expect("should write config");
    path
}

fn settings_for(path: &Path) -> Settings {
    Settings::try_new(Some(&OsString::from(path))).expect("should load settings")
}

async fn start(path: &Path) -> VeilidAPI {
    // Before the first node, so a later timeout has a log to show
    let _ = core_log_path();
    let settings = settings_for(path);
    assert!(
        settings.unknown_keys.is_empty(),
        "test config has unrecognized keys: {:?}",
        settings.unknown_keys
    );
    let core_config = settings
        .get_core_config(0, 0)
        .expect("should map to a core config");
    let update_callback: UpdateCallback = Arc::new(|_| {});
    api_startup(update_callback, core_config)
        .await
        .expect("should start")
}

async fn wait_for_network(api: &VeilidAPI, port: u16) {
    let mut waited_ms = 0;
    while !api
        .get_state()
        .await
        .expect("should get state")
        .network
        .started
    {
        assert!(
            waited_ms < NETWORK_START_TIMEOUT_MS,
            "network never started on port {} after {} ms\nudp bindable now: {}, tcp bindable now: {}\n--- core log tail ---\n{}",
            port,
            waited_ms,
            UdpSocket::bind(("0.0.0.0", port)).is_ok(),
            TcpListener::bind(("0.0.0.0", port)).is_ok(),
            core_log_tail(60),
        );
        sleep(POLL_MS).await;
        waited_ms += POLL_MS;
    }
}

/// Core's own log, so a timeout assert can say what the node was doing. One file per test process
/// (nextest gives each test its own), opened on first use and kept only if the test fails.
fn core_log_path() -> &'static Path {
    static CORE_LOG: OnceLock<PathBuf> = OnceLock::new();
    CORE_LOG.get_or_init(|| {
        let path =
            std::env::temp_dir().join(format!("veilid-server-test-{}.log", std::process::id()));
        let file = std::fs::File::create(&path).expect("should make a log file");
        let (writer, guard) = tracing_appender::non_blocking(file);
        // The appender flushes on drop, so its guard has to outlive every node in the process
        Box::leak(Box::new(guard));
        let filter = VeilidLayerFilter::new_with_config(
            VeilidLayerFilterConfig::new()
                .with_common_log_level(VeilidConfigLogLevel::Info)
                .try_with_directives_string("net=debug,attach=debug,rtab=debug")
                .expect("should parse directives"),
        );
        let layer = tracing_subscriber::fmt::Layer::new()
            .with_ansi(false)
            .with_writer(writer)
            .with_filter(filter);
        let _ = tracing_subscriber::registry().with(layer).try_init();
        path
    })
}

/// How long the node took to bind, or None if it never did within the budget
async fn network_start_delay(api: &VeilidAPI, budget_ms: u32) -> Option<u32> {
    let mut waited_ms = 0;
    loop {
        if api
            .get_state()
            .await
            .expect("should get state")
            .network
            .started
        {
            return Some(waited_ms);
        }
        if waited_ms >= budget_ms {
            return None;
        }
        sleep(POLL_MS).await;
        waited_ms += POLL_MS;
    }
}

/// Nothing failed, so the log has nothing to say
fn discard_core_log() {
    let _ = std::fs::remove_file(core_log_path());
}

fn core_log_tail(lines: usize) -> String {
    let Ok(text) = std::fs::read_to_string(core_log_path()) else {
        return "(no core log)".to_owned();
    };
    let all: Vec<&str> = text.lines().collect();
    all[all.len().saturating_sub(lines)..].join("\n")
}

/// A port free on BOTH protocols right now, because the node binds it for UDP and TCP a moment
/// later, and below every platform's ephemeral range (Linux 32768+, macOS and Windows 49152+):
/// an ephemeral pick can be handed to any process's next outbound connection between this probe
/// and the node's bind and then stays taken on TCP for that connection's life, while a node that
/// loses its bind retries only every `network.bind_retry_ms`, longer than this test's timeout.
fn free_port() -> u16 {
    let micros = Timestamp::now().as_u64();
    let base = 10_000 + ((u64::from(std::process::id()) ^ micros) % 22_000) as u16;
    for i in 0..64 {
        let port = base + i;
        let Ok(_udp) = UdpSocket::bind(("0.0.0.0", port)) else {
            continue;
        };
        // Held together: releasing the udp socket first would let another process take the port
        if TcpListener::bind(("0.0.0.0", port)).is_ok() {
            return port;
        }
    }
    panic!("no port free on both udp and tcp");
}

/// This host's own LAN address. A loopback flow belongs to no routing domain, so that is the
/// address a bootstrapping node would reach this one on. Routing only, nothing is sent.
fn local_address() -> IpAddr {
    let socket = UdpSocket::bind("0.0.0.0:0").expect("should bind");
    socket
        .connect(("192.0.2.1", 9))
        .expect("should have a route");
    let addr = socket
        .local_addr()
        .expect("should have a local address")
        .ip();
    assert!(!addr.is_loopback(), "no non-loopback address to test with");
    addr
}

/// Mint a signed v1 bootstrap record the way an operator does with --dump-txt-record
fn mint_record(api: &VeilidAPI, hostname: &str, port: u16) -> (PublicKey, String) {
    let keypair = Crypto::generate_keypair(CRYPTO_KIND_VLD0).expect("should make a keypair");
    let unsigned = format!(
        "1|0|{}|{}|T{},U{}|{}|",
        keypair.key(),
        hostname,
        port,
        port,
        Timestamp::now().as_u64() / 1_000_000u64
    );
    let crypto = api.crypto().expect("should have crypto");
    let signature = crypto
        .generate_signatures(
            unsigned.as_bytes(),
            std::slice::from_ref(&keypair),
            |_kp, sig| sig.to_string(),
        )
        .expect("should sign")
        .remove(0);
    (keypair.key(), format!("{}{}", unsigned, signature))
}

/// Ask a running node for its bootstrap records over UDP, exactly as a bootstrapping node does
fn probe_udp(addr: IpAddr, port: u16) -> Vec<String> {
    let message = udp_exchange(addr, port).expect("no B01T answer over udp");
    records_from_response(&message)
}

/// Like probe_udp, but silence is an answer of its own while a bootstrap is still running
fn try_probe_udp(addr: IpAddr, port: u16) -> Vec<String> {
    udp_exchange(addr, port)
        .map(|message| records_from_response(&message))
        .unwrap_or_default()
}

fn udp_exchange(addr: IpAddr, port: u16) -> Option<Vec<u8>> {
    let socket = UdpSocket::bind("0.0.0.0:0").expect("should bind");
    socket
        .set_read_timeout(Some(PROBE_TIMEOUT))
        .expect("should set timeout");
    socket
        .send_to(&udp_frame(B01T_MAGIC), (addr, port))
        .expect("should send");

    // A bootstrap server's answer runs to several kilobytes, so it arrives in pieces
    let mut message = Vec::<u8>::new();
    let mut filled = 0usize;
    let mut buf = vec![0u8; 65536];
    loop {
        let (len, _) = socket.recv_from(&mut buf).ok()?;
        let frame = &buf[..len];
        assert!(frame.len() > 8, "short udp frame: {} bytes", frame.len());
        assert_eq!(frame[0], 1, "unknown udp frame version");
        let off = u16::from_be_bytes([frame[4], frame[5]]) as usize;
        let total = u16::from_be_bytes([frame[6], frame[7]]) as usize;
        let chunk = &frame[8..];

        if message.is_empty() {
            message = vec![0u8; total];
        }
        assert_eq!(message.len(), total, "answer changed length mid-message");
        assert!(
            off + chunk.len() <= total,
            "chunk past the end of the answer"
        );
        message[off..off + chunk.len()].copy_from_slice(chunk);
        filled += chunk.len();
        if filled >= total {
            return Some(message);
        }
    }
}

/// UDP carries every message in an 8 byte reassembly header: version, reserved, sequence,
/// this chunk's offset, and the whole message's length, all big endian
fn udp_frame(message: &[u8]) -> Vec<u8> {
    let len = message.len() as u16;
    let mut framed = vec![1u8, 0, 0, 0, 0, 0];
    framed.extend_from_slice(&len.to_be_bytes());
    framed.extend_from_slice(message);
    framed
}

/// The same request over TCP, which carries every message in a 'VL' length frame
fn probe_tcp(addr: IpAddr, port: u16) -> Vec<String> {
    let mut stream = TcpStream::connect((addr, port)).expect("should connect");
    stream
        .set_read_timeout(Some(PROBE_TIMEOUT))
        .expect("should set timeout");
    let len = B01T_MAGIC.len();
    let mut request = vec![b'V', b'L', len as u8, (len >> 8) as u8];
    request.extend_from_slice(B01T_MAGIC);
    stream.write_all(&request).expect("should send");

    let mut header = [0u8; 4];
    stream
        .read_exact(&mut header)
        .expect("no B01T answer over tcp");
    assert_eq!(&header[0..2], b"VL", "bad frame header: {:02x?}", header);
    let len = ((header[3] as usize) << 8) | (header[2] as usize);
    let mut body = vec![0u8; len];
    stream.read_exact(&mut body).expect("short B01T answer");
    records_from_response(&body)
}

fn records_from_response(data: &[u8]) -> Vec<String> {
    let text = std::str::from_utf8(data).expect("B01T answer should be utf8");
    let parsed = json::parse(text).expect("B01T answer should be json");
    parsed["records"]
        .members()
        .map(|x| x.as_str().expect("record should be a string").to_owned())
        .collect()
}

fn network_block(port: u16, routing_table: &str) -> String {
    format!(
        r#"        routing_table:
{routing_table}
        protocol:
            udp:
                enabled: true
                listen_address: ':{port}'
            tcp:
                connect: true
                listen: true
                listen_address: ':{port}'
            ws:
                connect: false
                listen: false"#
    )
}

/// A config written for an earlier build has no bootstrap_records key at all.
/// It has to keep mapping exactly as it does today.
pub fn test_config_without_bootstrap_records() {
    let dir = test_dir("compat");
    let path = write_config(
        &dir,
        &network_block(
            5150,
            "            bootstrap: ['bootstrap-v1.veilid.net']\n            bootstrap_keys: ['VLD0:Vj0lKDdUQXmQ5Ol1SZdlvXkBHUccBcQvGLN9vbLSI7k']",
        ),
    );
    let settings = settings_for(&path);
    assert!(
        settings.unknown_keys.is_empty(),
        "unrecognized keys: {:?}",
        settings.unknown_keys
    );

    let core_config = settings.get_core_config(0, 0).expect("should map");
    let rt = &core_config.network.routing_table;
    assert_eq!(rt.bootstrap, vec!["bootstrap-v1.veilid.net".to_owned()]);
    assert_eq!(rt.bootstrap_keys.len(), 1);
    assert!(rt.bootstrap_records.is_empty());

    let _ = std::fs::remove_dir_all(&dir);
}

/// Records in the config file reach veilid-core verbatim
pub fn test_config_with_bootstrap_records() {
    let dir = test_dir("roundtrip");
    let record = "1|0|VLD0:Vj0lKDdUQXmQ5Ol1SZdlvXkBHUccBcQvGLN9vbLSI7k|bootstrap.invalid|T5150,U5150|1746492253|VLD0:NntMU4LfAVlOVXWh9zjGCnwPDPFnTBPI0q2KuYd0-zO5WvVG7YvQu1h_MdetcjM09zdvEUxJIIU-SaJAWgF1BQ";
    let path = write_config(
        &dir,
        &network_block(
            5150,
            &format!(
                "            bootstrap: []\n            bootstrap_records:\n                - '{}'",
                record
            ),
        ),
    );
    let settings = settings_for(&path);
    assert!(
        settings.unknown_keys.is_empty(),
        "unrecognized keys: {:?}",
        settings.unknown_keys
    );

    let core_config = settings.get_core_config(0, 0).expect("should map");
    assert_eq!(
        core_config.network.routing_table.bootstrap_records,
        vec![record.to_owned()]
    );

    // The same key is settable at runtime, as 'debug config set' does
    settings
        .set(
            "core.network.routing_table.bootstrap_records",
            "[\"one\",\"two\"]",
        )
        .expect("should set");
    let core_config = settings.get_core_config(0, 0).expect("should map");
    assert_eq!(
        core_config.network.routing_table.bootstrap_records,
        vec!["one".to_owned(), "two".to_owned()]
    );

    let _ = std::fs::remove_dir_all(&dir);
}

/// A node configured with bootstrap records answers B01T from them alone: no bootstrap ran,
/// nothing was resolved, and the bootstrap host named in the record does not exist.
async fn serves_configured_records(bootstrap: &str) {
    // Mint the record with a throwaway node, as an operator would with --dump-txt-record
    let mint_dir = test_dir("mint");
    let mint_path = write_config(
        &mint_dir,
        &network_block(free_port(), "            bootstrap: []"),
    );
    let minter = start(&mint_path).await;
    let port = free_port();
    let (signing_key, record) = mint_record(&minter, UNRESOLVABLE_HOST, port);
    minter.shutdown().await;
    let _ = std::fs::remove_dir_all(&mint_dir);

    let dir = test_dir("serve");
    let path = write_config(
        &dir,
        &network_block(
            port,
            &format!(
                "{}\n            bootstrap_keys: ['{}']\n            bootstrap_records:\n                - '{}'",
                bootstrap, signing_key, record
            ),
        ),
    );

    let api = start(&path).await;
    api.attach().await.expect("should attach");
    wait_for_network(&api, port).await;

    let addr = local_address();
    assert_eq!(probe_udp(addr, port), vec![record.clone()]);
    assert_eq!(probe_tcp(addr, port), vec![record]);

    api.shutdown().await;
    let _ = std::fs::remove_dir_all(&dir);
    discard_core_log();
}

pub fn test_serves_configured_records() {
    block_on(async {
        serves_configured_records("            bootstrap: []").await;
    });
}

/// With records configured, the bootstrap list is never consulted, so a bootstrap host that
/// cannot be resolved changes nothing
pub fn test_serves_configured_records_bypassing_bootstrap() {
    block_on(async {
        serves_configured_records(&format!("            bootstrap: ['{}']", UNRESOLVABLE_HOST))
            .await;
    });
}

/// Records learned from a TXT bootstrap have to survive a restart, because nothing but a
/// bootstrap ever fetches them and a node that restarts with a warm routing table never
/// bootstraps again. Reaches the real network: excluded from the offline gate.
pub fn test_txt_records_survive_restart() {
    block_on(async {
        let dir = test_dir("restart");
        let port = free_port();
        let path = write_config_with_state(
            &dir,
            &network_block(port, "            bootstrap: ['bootstrap-v1.veilid.net']"),
            false,
        );
        let addr = local_address();

        let api = start(&path).await;
        api.attach().await.expect("should attach");
        wait_for_network(&api, port).await;

        let mut records = Vec::new();
        let mut waited_ms = 0;
        while records.is_empty() {
            assert!(
                waited_ms < BOOTSTRAP_RECORDS_WAIT_MS,
                "TXT bootstrap never produced records to serve"
            );
            records = try_probe_udp(addr, port);
            if records.is_empty() {
                sleep(POLL_MS).await;
                waited_ms += POLL_MS;
            }
        }
        api.shutdown().await;

        // The same node, same table store, answering before it could have bootstrapped again
        let api = start(&path).await;
        api.attach().await.expect("should attach");
        wait_for_network(&api, port).await;
        assert_eq!(
            probe_udp(addr, port),
            records,
            "restarted node lost the records it had cached"
        );
        api.shutdown().await;

        let _ = std::fs::remove_dir_all(&dir);
        discard_core_log();
    });
}

/// The failure `free_port` exists to prevent, on demand. A node that loses either bind retries
/// only every `network.bind_retry_ms` (10 s by default), so a port held for a whole test run
/// shows up as `network never started` and nothing else. Holding UDP and holding TCP are separate
/// cases because an ephemeral port the OS hands out for one says nothing about the other.
async fn run_port_held_case(label: &str, hold_udp: bool, release_after_ms: u32) -> Option<u32> {
    let dir = test_dir(label);
    let port = free_port();
    let path = write_config(&dir, &network_block(port, "            bootstrap: []"));

    let mut udp_hold =
        hold_udp.then(|| UdpSocket::bind(("0.0.0.0", port)).expect("should hold udp"));
    let mut tcp_hold =
        (!hold_udp).then(|| TcpListener::bind(("0.0.0.0", port)).expect("should hold tcp"));

    let api = start(&path).await;
    api.attach().await.expect("should attach");

    let delay = if release_after_ms == u32::MAX {
        network_start_delay(&api, NETWORK_START_TIMEOUT_MS).await
    } else {
        // Nothing should have bound while the port was held
        assert_eq!(network_start_delay(&api, release_after_ms).await, None);
        udp_hold.take();
        tcp_hold.take();
        network_start_delay(&api, NETWORK_START_TIMEOUT_MS).await
    };

    api.shutdown().await;
    let _ = std::fs::remove_dir_all(&dir);
    discard_core_log();
    delay
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn config_without_bootstrap_records() {
        test_config_without_bootstrap_records();
    }

    #[test]
    fn config_with_bootstrap_records() {
        test_config_with_bootstrap_records();
    }

    #[test]
    fn serves_configured_records() {
        test_serves_configured_records();
    }

    #[test]
    fn serves_configured_records_bypassing_bootstrap() {
        test_serves_configured_records_bypassing_bootstrap();
    }

    #[test]
    fn txt_records_survive_restart() {
        test_txt_records_survive_restart();
    }

    /// Not a gate: it spends 30 s per held case waiting for something that must not happen.
    /// Run it with `cargo nextest run -p veilid-server --run-ignored all -E 'test(port_held)'`.
    #[test]
    #[ignore = "reproduction; ~90s of deliberate waiting"]
    fn port_held_stalls_network_start() {
        block_on(async {
            assert!(
                run_port_held_case("held-udp", true, u32::MAX)
                    .await
                    .is_none(),
                "a held udp port should stall the bind for the whole budget"
            );
            assert!(
                run_port_held_case("held-tcp", false, u32::MAX)
                    .await
                    .is_none(),
                "a held tcp port should stall the bind for the whole budget"
            );
            // Recovery waits for the next retry, so a longer-lived holder is fatal rather than slow
            let delay = run_port_held_case("released-tcp", false, 5_000)
                .await
                .expect("should bind once the port is free");
            assert!(delay >= 5_000, "bound too early to have waited for a retry");
        });
    }
}
