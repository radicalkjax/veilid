//! Debug fault injection, driven by `debug chaos` and compiled with the `debug-api` feature
//! (`chaos_stub.rs` otherwise). Settings are runtime only, never persisted, and all zero by
//! default.
//! - `net.self` is this node's own link, for every packet it sends and receives: random loss, a
//!   size cap, and a rate in bytes and packets per second behind a bounded queue, with capacity
//!   that drifts and collapses that lose most packets. Together they reproduce a cellular link.
//! - `net.ip` adds loss and size caps for remote address ranges.
//! - `rpc.value_changed` discards a share of the ValueChanged statements this node receives, so
//!   pushes can fail while the network works.
//! - `preset` loads named settings measured on a real link: `mobile`, `mobile-bad`,
//!   `mtu-blackhole`.
//!
//! A lost or held send reports success. A lost receive is discarded as it comes off the socket, a
//! held one is delivered once the link has carried it. A stream frame crosses a size cap in TCP
//! segments, and one over the cap stalls its connection in that direction, as a path MTU black
//! hole stalls TCP, until `net.stall_close_ms` closes it.

use super::*;

/// Percent of messages dropped
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct ChaosLoss {
    /// 0 drops nothing, 100 drops every message
    loss: u8,
}

/// One direction of this node's link: what it loses and how fast it carries packets
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct ChaosLink {
    /// Percent of packets lost at random
    loss: u8,
    /// Largest packet carried, in bytes; 0 for no cap
    max_size: u32,
    /// Bytes per second carried; 0 for no limit
    rate: u32,
    /// Packets per second carried; 0 for no limit
    pps: u32,
    /// Queue size, as the milliseconds of traffic it holds at full capacity
    queue_ms: u32,
}

/// How the link changes over time, in both directions
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct ChaosWeather {
    /// Most of the capacity drift takes away, percent; 0 holds capacity steady
    drift: u8,
    /// Seconds between drift redraws
    drift_s: u32,
    /// Mean seconds from the end of one collapse to the start of the next; 0 for none
    collapse_every_s: u32,
    /// Mean seconds a collapse lasts
    collapse_s: u32,
    /// Share of the capacity a collapse takes away, percent
    collapse_cut: u8,
    /// Share of packets lost at random during a collapse, percent
    collapse_loss: u8,
}

/// This node's own link
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct ChaosSelf {
    /// Packets this node sends
    send: ChaosLink,
    /// Packets this node receives
    recv: ChaosLink,
    /// Capacity over time
    weather: ChaosWeather,
}

/// One direction of a remote address range: what it loses
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct ChaosPath {
    /// Percent of packets lost at random
    loss: u8,
    /// Largest packet carried, in bytes; 0 for no cap
    max_size: u32,
}

/// Settings for one remote address range
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct ChaosRange {
    /// Packets this node sends to the range
    send: ChaosPath,
    /// Packets this node receives from the range
    recv: ChaosPath,
}

/// Packet settings
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct ChaosNet {
    /// This node's own link, for every packet whatever its remote address
    #[serde(rename = "self")]
    this_node: ChaosSelf,
    /// Further loss and size caps for remote address ranges, keyed by CIDR
    ip: BTreeMap<String, ChaosRange>,
    /// Milliseconds before a stream stalled by a size cap closes, as its far side gives up; 0 never
    stall_close_ms: u32,
}

/// Message loss by kind, for statements this node receives
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct ChaosRpc {
    /// ValueChanged statements, discarded as if their pushes went missing
    value_changed: ChaosLoss,
}

/// Every chaos setting
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct ChaosSettings {
    /// Packets
    net: ChaosNet,
    /// Messages
    rpc: ChaosRpc,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ChaosDirection {
    Send,
    Recv,
}

impl ChaosSelf {
    fn link(&self, direction: ChaosDirection) -> &ChaosLink {
        match direction {
            ChaosDirection::Send => &self.send,
            ChaosDirection::Recv => &self.recv,
        }
    }
}

impl ChaosRange {
    fn path(&self, direction: ChaosDirection) -> &ChaosPath {
        match direction {
            ChaosDirection::Send => &self.send,
            ChaosDirection::Recv => &self.recv,
        }
    }
}

/// What chaos does with one packet or stream frame
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[must_use]
pub(crate) enum ChaosVerdict {
    /// Send or deliver it now
    Pass,
    /// Send or deliver it after this many milliseconds, once the link has carried it
    Hold(u32),
    /// Lose it
    Drop,
    /// Lose it as too big for the path; a stream stalls for good
    Oversize,
}

/// An address range: an address with its host bits cleared, and a prefix length
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct IpCidr {
    /// First address of the range
    addr: IpAddr,
    /// Leading bits an address must share to be in the range
    prefix: u8,
}

impl IpCidr {
    /// `a.b.c.d`, `a.b.c.d/n` or the IPv6 forms; a bare address is a single host
    fn parse(text: &str) -> Option<Self> {
        let (addr_text, prefix_text) = match text.split_once('/') {
            Some((addr, prefix)) => (addr, Some(prefix)),
            None => (text, None),
        };
        let addr = canonical_ip(addr_text.trim().parse::<IpAddr>().ok()?);
        let max_prefix = if addr.is_ipv4() { 32 } else { 128 };
        let prefix = match prefix_text {
            Some(prefix) => prefix
                .trim()
                .parse::<u8>()
                .ok()
                .filter(|p| *p <= max_prefix)?,
            None => max_prefix,
        };
        Some(Self {
            addr: mask_ip(addr, prefix),
            prefix,
        })
    }

    fn contains(&self, ip: IpAddr) -> bool {
        let ip = canonical_ip(ip);
        ip.is_ipv4() == self.addr.is_ipv4() && mask_ip(ip, self.prefix) == self.addr
    }
}

impl fmt::Display for IpCidr {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}/{}", self.addr, self.prefix)
    }
}

/// Dual-stack sockets report an IPv4 peer as IPv4-mapped IPv6
fn canonical_ip(ip: IpAddr) -> IpAddr {
    match ip {
        IpAddr::V6(v6) => v6.to_ipv4_mapped().map_or(ip, IpAddr::V4),
        IpAddr::V4(_) => ip,
    }
}

fn mask_ip(ip: IpAddr, prefix: u8) -> IpAddr {
    match ip {
        IpAddr::V4(v4) => {
            let mask = u32::MAX.checked_shl(32 - u32::from(prefix)).unwrap_or(0);
            IpAddr::V4(Ipv4Addr::from(u32::from(v4) & mask))
        }
        IpAddr::V6(v6) => {
            let mask = u128::MAX.checked_shl(128 - u32::from(prefix)).unwrap_or(0);
            IpAddr::V6(Ipv6Addr::from(u128::from(v6) & mask))
        }
    }
}

/// Settings compiled for the packet path
#[derive(Debug, Default)]
struct ChaosRules {
    /// This node's own link
    this_node: ChaosSelf,
    /// Address range rules, longest prefix first
    ip: Vec<(IpCidr, ChaosRange)>,
}

impl ChaosRules {
    fn compile(settings: &ChaosSettings) -> Self {
        let mut ip: Vec<(IpCidr, ChaosRange)> = settings
            .net
            .ip
            .iter()
            .filter_map(|(key, range)| Some((IpCidr::parse(key)?, *range)))
            .collect();
        ip.sort_by_key(|(cidr, _)| std::cmp::Reverse(cidr.prefix));
        Self {
            this_node: settings.net.this_node,
            ip,
        }
    }

    fn is_empty(&self) -> bool {
        self.this_node == ChaosSelf::default()
            && self
                .ip
                .iter()
                .all(|(_, range)| *range == ChaosRange::default())
    }
}

/// One direction of the emulated link: a queue the link drains at its rate
#[derive(Debug, Default)]
struct LinkQueue {
    /// When the link has carried everything queued so far
    free_ts: Timestamp,
}

impl LinkQueue {
    /// When a packet of `size` bytes offered at `now` has been carried, or None if the queue is
    /// full. `capacity` scales the link's rates, percent, and the queue holds the same traffic
    /// however slowly it drains.
    fn offer(
        &mut self,
        link: &ChaosLink,
        capacity: u8,
        now: Timestamp,
        size: usize,
    ) -> Option<Timestamp> {
        if capacity == 0 {
            return None;
        }
        let start = self.free_ts.max(now);
        let longest_wait = u64::from(link.queue_ms) * 100 / u64::from(capacity);
        if start.duration_since(now) > TimestampDuration::new_ms(longest_wait) {
            return None;
        }
        // Microseconds per second, times 100 for capacity in percent
        let scaled = |per_second: u32| u64::from(per_second) * u64::from(capacity);
        let bytes_us = (link.rate > 0).then(|| size as u64 * 100_000_000 / scaled(link.rate));
        let packet_us = (link.pps > 0).then(|| 100_000_000 / scaled(link.pps));
        let carry = TimestampDuration::new(bytes_us.max(packet_us).unwrap_or_default());
        self.free_ts = start.later(carry);
        Some(self.free_ts)
    }
}

/// The link's conditions at one moment
#[derive(Clone, Copy, Debug)]
struct LinkConditions {
    /// Capacity left, percent
    capacity: u8,
    /// Share of packets lost at random on top of each direction's own loss, percent
    loss: u8,
    /// Whether a collapse is under way
    collapsed: bool,
}

/// Where the weather stands, advanced as packets pass
#[derive(Debug)]
struct WeatherState {
    /// Capacity drift leaves, percent
    drift_capacity: u8,
    /// When drift next redraws it
    next_drift_ts: Timestamp,
    /// When the current or next collapse starts
    collapse_start_ts: Timestamp,
    /// When that collapse ends
    collapse_end_ts: Timestamp,
    /// Collapses scheduled so far, the next one included
    collapses_scheduled: u64,
}

impl Default for WeatherState {
    fn default() -> Self {
        Self {
            drift_capacity: 100,
            next_drift_ts: Timestamp::default(),
            collapse_start_ts: Timestamp::default(),
            collapse_end_ts: Timestamp::default(),
            collapses_scheduled: 0,
        }
    }
}

impl WeatherState {
    /// The link's conditions at `now`, redrawing drift and scheduling collapses as they come due
    fn conditions(&mut self, weather: &ChaosWeather, now: Timestamp) -> LinkConditions {
        if weather.drift == 0 || weather.drift_s == 0 {
            self.drift_capacity = 100;
        } else if now >= self.next_drift_ts {
            let lost = get_random_u32() % (u32::from(weather.drift) + 1);
            self.drift_capacity = u8::try_from(100u32.saturating_sub(lost)).unwrap_or(100);
            self.next_drift_ts = now.later(TimestampDuration::new_secs(weather.drift_s));
        }
        let calm = LinkConditions {
            capacity: self.drift_capacity,
            loss: 0,
            collapsed: false,
        };
        if weather.collapse_every_s == 0 || weather.collapse_s == 0 {
            return calm;
        }
        if now >= self.collapse_end_ts {
            self.collapse_start_ts = now.later(jittered(weather.collapse_every_s));
            self.collapse_end_ts = self.collapse_start_ts.later(jittered(weather.collapse_s));
            self.collapses_scheduled += 1;
        }
        if now < self.collapse_start_ts {
            return calm;
        }
        let kept = 100u16.saturating_sub(u16::from(weather.collapse_cut));
        LinkConditions {
            capacity: u8::try_from(u16::from(self.drift_capacity) * kept / 100).unwrap_or(0),
            loss: weather.collapse_loss,
            collapsed: true,
        }
    }

    /// Collapses begun by `now`
    fn collapses(&self, now: Timestamp) -> u64 {
        let pending = u64::from(now < self.collapse_start_ts);
        self.collapses_scheduled.saturating_sub(pending)
    }
}

/// Uniform from half to one and a half times `mean_s`
fn jittered(mean_s: u32) -> TimestampDuration {
    let mean = TimestampDuration::new_secs(mean_s).as_u64();
    TimestampDuration::new(mean / 2 + get_random_u64() % (mean + 1))
}

/// The emulated link: a queue each way, and the weather both share
#[derive(Debug, Default)]
struct LinkState {
    /// Packets this node sends
    send: LinkQueue,
    /// Packets this node receives
    recv: LinkQueue,
    /// Capacity over time
    weather: WeatherState,
}

impl LinkState {
    fn queue(&mut self, direction: ChaosDirection) -> &mut LinkQueue {
        match direction {
            ChaosDirection::Send => &mut self.send,
            ChaosDirection::Recv => &mut self.recv,
        }
    }
}

/// What became of one direction's packets
#[derive(Debug, Default)]
struct ChaosCounters {
    /// Lost at random
    lost: AtomicU64,
    /// Lost as too big
    oversize: AtomicU64,
    /// Dropped by a full queue
    overflowed: AtomicU64,
    /// Held while the link carried them
    held: AtomicU64,
}

impl ChaosCounters {
    fn to_json(&self, backlog: TimestampDuration) -> serde_json::Value {
        serde_json::json!({
            "lost": self.lost.load(Ordering::Relaxed),
            "oversize": self.oversize.load(Ordering::Relaxed),
            "overflowed": self.overflowed.load(Ordering::Relaxed),
            "held": self.held.load(Ordering::Relaxed),
            "backlog_ms": backlog.as_u64() / 1000,
        })
    }
}

/// What the packet path reads, shared with the connections it wraps
#[derive(Debug, Default)]
struct ChaosState {
    /// Whether any packet setting is set, so the packet path can skip the rules
    enabled: AtomicBool,
    /// Settings as last set
    settings: Mutex<ChaosSettings>,
    /// Settings compiled for the packet path
    rules: RwLock<ChaosRules>,
    /// The emulated link
    link: Mutex<LinkState>,
    /// Packets sent
    send: ChaosCounters,
    /// Packets received
    recv: ChaosCounters,
    /// Share of received ValueChanged statements to discard, percent
    value_changed_loss: AtomicU64,
    /// ValueChanged statements discarded
    dropped_value_changed: AtomicU64,
    /// Milliseconds before a stalled stream closes; 0 never
    stall_close_ms: AtomicU64,
}

impl ChaosState {
    /// Size caps on the `largest` packet first, then independent loss rolls for the link and the
    /// longest matching address range, then the link's queue for all `size` bytes
    fn verdict(
        &self,
        direction: ChaosDirection,
        ip: IpAddr,
        size: usize,
        largest: usize,
    ) -> ChaosVerdict {
        if !self.enabled.load(Ordering::Relaxed) {
            return ChaosVerdict::Pass;
        }
        let (link, path, weather) = {
            let rules = self.rules.read();
            let path = rules
                .ip
                .iter()
                .find(|(cidr, _)| cidr.contains(ip))
                .map(|(_, range)| *range.path(direction))
                .unwrap_or_default();
            (
                *rules.this_node.link(direction),
                path,
                rules.this_node.weather,
            )
        };
        let counters = match direction {
            ChaosDirection::Send => &self.send,
            ChaosDirection::Recv => &self.recv,
        };
        let max_size = [link.max_size, path.max_size]
            .into_iter()
            .filter(|max_size| *max_size > 0)
            .min();
        if max_size.is_some_and(|max_size| largest > max_size as usize) {
            counters.oversize.fetch_add(1, Ordering::Relaxed);
            return ChaosVerdict::Oversize;
        }
        if roll_loss(link.loss) || roll_loss(path.loss) {
            counters.lost.fetch_add(1, Ordering::Relaxed);
            return ChaosVerdict::Drop;
        }
        if weather == ChaosWeather::default() && link.rate == 0 && link.pps == 0 {
            return ChaosVerdict::Pass;
        }
        let now = Timestamp::now_non_decreasing();
        let carried = {
            let mut state = self.link.lock();
            let conditions = state.weather.conditions(&weather, now);
            if roll_loss(conditions.loss) {
                counters.lost.fetch_add(1, Ordering::Relaxed);
                return ChaosVerdict::Drop;
            }
            if link.rate == 0 && link.pps == 0 {
                return ChaosVerdict::Pass;
            }
            state
                .queue(direction)
                .offer(&link, conditions.capacity, now, size)
        };
        let Some(carried) = carried else {
            counters.overflowed.fetch_add(1, Ordering::Relaxed);
            return ChaosVerdict::Drop;
        };
        // Sleeps take whole milliseconds
        match carried.duration_since(now).millis_u32() {
            Ok(0) => ChaosVerdict::Pass,
            wait_ms => {
                counters.held.fetch_add(1, Ordering::Relaxed);
                ChaosVerdict::Hold(wait_ms.unwrap_or(u32::MAX))
            }
        }
    }
}

fn roll_loss(loss: u8) -> bool {
    loss >= 100 || (loss > 0 && get_random_u32() % 100 < u32::from(loss))
}

/// Debug fault injection
#[derive(Clone, Debug, Default)]
pub(crate) struct Chaos {
    /// Settings and counters, shared with wrapped connections
    state: Arc<ChaosState>,
}

impl Chaos {
    pub fn new() -> Self {
        Self::default()
    }

    /// What to do with a packet of `size` bytes this node is sending to `ip`
    #[cfg_attr(
        all(target_arch = "wasm32", target_os = "unknown"),
        expect(dead_code, reason = "web has no UDP, the only caller")
    )]
    pub fn send_verdict(&self, ip: IpAddr, size: usize) -> ChaosVerdict {
        self.state.verdict(ChaosDirection::Send, ip, size, size)
    }

    /// What to do with a packet of `size` bytes this node received from `ip`
    #[cfg_attr(
        all(target_arch = "wasm32", target_os = "unknown"),
        expect(dead_code, reason = "web has no UDP, the only caller")
    )]
    pub fn recv_verdict(&self, ip: IpAddr, size: usize) -> ChaosVerdict {
        self.state.verdict(ChaosDirection::Recv, ip, size, size)
    }

    /// Whether to discard a ValueChanged statement this node received
    pub fn drop_value_changed(&self) -> bool {
        let loss = self.state.value_changed_loss.load(Ordering::Relaxed);
        let dropped = roll_loss(u8::try_from(loss).unwrap_or(100));
        if dropped {
            self.state
                .dropped_value_changed
                .fetch_add(1, Ordering::Relaxed);
        }
        dropped
    }

    /// Puts a connection's frames through the chaos settings; `segment_size` is the TCP segment
    /// size it sends, where the platform reports it
    pub fn wrap_connection(
        &self,
        inner: ProtocolNetworkConnection,
        segment_size: Option<u32>,
    ) -> ProtocolNetworkConnection {
        Box::new(ChaosProtocolNetworkConnection {
            inner,
            chaos: self.clone(),
            segment_size,
            send_stalled: AtomicBool::new(false),
            recv_stalled: AtomicBool::new(false),
            stalled_ts: AtomicOptionTimestamp::none(),
        })
    }

    /// Runs a `debug chaos` command:
    /// - no arguments: every setting, as JSON
    /// - `<path>`: one subtree, such as `net.self.recv`
    /// - `<path>=<value> ...`: set, then every setting
    /// - `reset [<path>]`: everything, or one subtree, back to defaults
    /// - `preset [<name> ...] [<path>=<value> ...]`: defaults, then each preset and assignment in
    ///   order, then every setting; no arguments lists the presets
    /// - `stats`: what each direction lost, held and still has queued, and the link's capacity
    pub fn command(&self, args: &[String]) -> Result<String, String> {
        let mut settings = self.state.settings.lock();
        match args {
            [] => settings_json(&settings, &[]),
            [first] if first == "stats" => Ok(self.stats(&settings).to_string()),
            [first] if first == "preset" => presets_json(),
            [first, rest @ ..] if first == "preset" => {
                let mut next = ChaosSettings::default();
                for arg in rest {
                    if arg.contains('=') {
                        set_assignment(&mut next, arg)?;
                        continue;
                    }
                    let preset = PRESETS
                        .iter()
                        .find(|preset| preset.name == arg.as_str())
                        .ok_or_else(|| format!("no chaos preset '{}'", arg))?;
                    for assignment in preset.assignments.split_whitespace() {
                        set_assignment(&mut next, assignment)?;
                    }
                }
                *settings = next;
                self.apply(&settings);
                settings_json(&settings, &[])
            }
            [first, rest @ ..] if first == "reset" => {
                match rest {
                    [] => *settings = ChaosSettings::default(),
                    [path] => reset_path(&mut settings, &parse_path(path)?)?,
                    _ => return Err("reset takes at most one path".to_owned()),
                }
                self.apply(&settings);
                settings_json(&settings, &[])
            }
            [path] if !path.contains('=') => settings_json(&settings, &parse_path(path)?),
            assignments => {
                let mut next = settings.clone();
                for assignment in assignments {
                    set_assignment(&mut next, assignment)?;
                }
                *settings = next;
                self.apply(&settings);
                settings_json(&settings, &[])
            }
        }
    }

    fn stats(&self, settings: &ChaosSettings) -> serde_json::Value {
        let now = Timestamp::now_non_decreasing();
        let (send_backlog, recv_backlog, conditions, collapses) = {
            let mut link = self.state.link.lock();
            let conditions = link
                .weather
                .conditions(&settings.net.this_node.weather, now);
            (
                link.send.free_ts.duration_since(now),
                link.recv.free_ts.duration_since(now),
                conditions,
                link.weather.collapses(now),
            )
        };
        serde_json::json!({
            "send": self.state.send.to_json(send_backlog),
            "recv": self.state.recv.to_json(recv_backlog),
            "capacity": conditions.capacity,
            "collapsed": conditions.collapsed,
            "collapses": collapses,
            "value_changed": {
                "lost": self.state.dropped_value_changed.load(Ordering::Relaxed),
            },
        })
    }

    fn apply(&self, settings: &ChaosSettings) {
        let rules = ChaosRules::compile(settings);
        let enabled = !rules.is_empty();
        *self.state.rules.write() = rules;
        // Queues and weather start over
        *self.state.link.lock() = LinkState::default();
        self.state.enabled.store(enabled, Ordering::Relaxed);
        self.state.value_changed_loss.store(
            u64::from(settings.rpc.value_changed.loss),
            Ordering::Relaxed,
        );
        self.state
            .stall_close_ms
            .store(u64::from(settings.net.stall_close_ms), Ordering::Relaxed);
    }
}

/// A named set of chaos settings
struct ChaosPreset {
    /// What `debug chaos preset` calls it
    name: &'static str,
    /// What it reproduces
    about: &'static str,
    /// Its settings, as `debug chaos` assignments
    assignments: &'static str,
}

/// Measured on an iPhone tether in 2026-10: rate sweeps, two rounds of veilid nodes, and the path
/// MTU of its 464XLAT
const PRESETS: &[ChaosPreset] = &[
    ChaosPreset {
        name: "mobile",
        about: "a cellular link on a fair day: 4 Mbps up and 6.4 down behind 0.8 s queues, capacity drifting by up to half, and a 45 s collapse about every 10 minutes",
        assignments: "net.self.send.rate=500000 net.self.send.queue_ms=800 \
            net.self.recv.rate=800000 net.self.recv.queue_ms=800 \
            net.self.weather.drift=50 net.self.weather.drift_s=10 \
            net.self.weather.collapse_every_s=600 net.self.weather.collapse_s=45 \
            net.self.weather.collapse_cut=30 net.self.weather.collapse_loss=85",
    },
    ChaosPreset {
        name: "mobile-bad",
        about: "a cellular link on a bad day: 2 Mbps up and 4.8 down behind 0.8 s queues, capacity drifting by up to half, and 100 s collapses about 70 s apart",
        assignments: "net.self.send.rate=250000 net.self.send.queue_ms=800 \
            net.self.recv.rate=600000 net.self.recv.queue_ms=800 \
            net.self.weather.drift=50 net.self.weather.drift_s=10 \
            net.self.weather.collapse_every_s=70 net.self.weather.collapse_s=100 \
            net.self.weather.collapse_cut=30 net.self.weather.collapse_loss=85",
    },
    ChaosPreset {
        name: "mtu-blackhole",
        about: "an IPv4 path that drops packets over 1420 bytes unannounced, as a 464XLAT can; a stalled stream closes after 70 s",
        assignments: "net.ip[0.0.0.0/0].send.max_size=1380 net.ip[0.0.0.0/0].recv.max_size=1380 \
            net.stall_close_ms=70000",
    },
];

fn presets_json() -> Result<String, String> {
    let presets: serde_json::Map<String, serde_json::Value> = PRESETS
        .iter()
        .map(|preset| {
            let settings = preset
                .assignments
                .split_whitespace()
                .collect::<Vec<_>>()
                .join(" ");
            (
                preset.name.to_owned(),
                serde_json::json!({ "about": preset.about, "settings": settings }),
            )
        })
        .collect();
    serde_json::to_string_pretty(&presets).map_err(|e| e.to_string())
}

/// Applies one `<path>=<value>`
fn set_assignment(settings: &mut ChaosSettings, assignment: &str) -> Result<(), String> {
    let Some((path, value)) = assignment.split_once('=') else {
        return Err(format!("expected <path>=<value>, got '{}'", assignment));
    };
    set_path(settings, &parse_path(path)?, value)
}

/// Splits `net.ip['1.2.3.4/24'].send.loss` into keys. Quotes are optional, since the debug
/// command line strips them; an address range comes out as canonical CIDR.
fn parse_path(text: &str) -> Result<Vec<String>, String> {
    let mut keys = Vec::new();
    let mut rest = text.trim();
    while !rest.is_empty() {
        let end = rest.find(['.', '[']).unwrap_or(rest.len());
        if end == 0 {
            return Err(format!("empty key in chaos path '{}'", text));
        }
        keys.push(rest[..end].to_owned());
        rest = &rest[end..];
        if let Some(after) = rest.strip_prefix('[') {
            let close = after
                .find(']')
                .ok_or_else(|| format!("unclosed '[' in chaos path '{}'", text))?;
            let range = after[..close].trim().trim_matches(['\'', '"']);
            let cidr = IpCidr::parse(range)
                .ok_or_else(|| format!("invalid address range '{}' in chaos path", range))?;
            keys.push(cidr.to_string());
            rest = &after[close + 1..];
        }
        if !rest.is_empty() {
            rest = rest
                .strip_prefix('.')
                .ok_or_else(|| format!("expected '.' in chaos path '{}'", text))?;
        }
    }
    Ok(keys)
}

fn settings_json(settings: &ChaosSettings, path: &[String]) -> Result<String, String> {
    let root = serde_json::to_value(settings).map_err(|e| e.to_string())?;
    let node = path
        .iter()
        .try_fold(&root, |node, key| node.get(key))
        .ok_or_else(|| format!("no chaos setting at '{}'", path.join(".")))?;
    serde_json::to_string_pretty(node).map_err(|e| e.to_string())
}

/// Whether `path` names an address range entry or something inside one
fn ip_range_key(path: &[String]) -> Option<&String> {
    match path {
        [net, ip, range, ..] if net == "net" && ip == "ip" => Some(range),
        _ => None,
    }
}

fn set_path(settings: &mut ChaosSettings, path: &[String], value: &str) -> Result<(), String> {
    let value: serde_json::Value = serde_json::from_str(value.trim())
        .map_err(|e| format!("invalid chaos value '{}': {}", value, e))?;
    let mut next = settings.clone();
    // An address range not set yet starts from no loss
    if let Some(range) = ip_range_key(path) {
        next.net.ip.entry(range.clone()).or_default();
    }
    let mut root = serde_json::to_value(&next).map_err(|e| e.to_string())?;
    let slot = path
        .iter()
        .try_fold(&mut root, |node, key| node.get_mut(key))
        .ok_or_else(|| format!("no chaos setting at '{}'", path.join(".")))?;
    *slot = value;
    *settings = checked_settings(root)?;
    Ok(())
}

fn reset_path(settings: &mut ChaosSettings, path: &[String]) -> Result<(), String> {
    if let (Some(range), 3) = (ip_range_key(path), path.len()) {
        settings.net.ip.remove(range);
        return Ok(());
    }
    // Inside an address range the defaults are those of a new range
    let (defaults, default_path) = match ip_range_key(path) {
        Some(_) => (
            serde_json::to_value(ChaosRange::default()).map_err(|e| e.to_string())?,
            &path[3..],
        ),
        None => (
            serde_json::to_value(ChaosSettings::default()).map_err(|e| e.to_string())?,
            path,
        ),
    };
    let default = default_path
        .iter()
        .try_fold(&defaults, |node, key| node.get(key))
        .ok_or_else(|| format!("no chaos setting at '{}'", path.join(".")))?
        .clone();
    let mut root = serde_json::to_value(&*settings).map_err(|e| e.to_string())?;
    let slot = path
        .iter()
        .try_fold(&mut root, |node, key| node.get_mut(key))
        .ok_or_else(|| format!("no chaos setting at '{}'", path.join(".")))?;
    *slot = default;
    *settings = checked_settings(root)?;
    Ok(())
}

/// Type-checks an edited tree and bounds every percentage. A range with no loss stays: as the
/// longest match it exempts its addresses from a wider range.
fn checked_settings(root: serde_json::Value) -> Result<ChaosSettings, String> {
    let settings: ChaosSettings =
        serde_json::from_value(root).map_err(|e| format!("invalid chaos settings: {}", e))?;
    for range in settings.net.ip.keys() {
        if IpCidr::parse(range).map(|cidr| cidr.to_string()).as_deref() != Some(range.as_str()) {
            return Err(format!("invalid address range '{}'", range));
        }
    }
    let this_node = &settings.net.this_node;
    let mut losses = [
        this_node.send.loss,
        this_node.recv.loss,
        settings.rpc.value_changed.loss,
    ]
    .into_iter()
    .chain(
        settings
            .net
            .ip
            .values()
            .flat_map(|range| [range.send.loss, range.recv.loss]),
    );
    if losses.any(|loss| loss > 100) {
        return Err("loss is a percentage, 0 to 100".to_owned());
    }
    let weather = &this_node.weather;
    if [weather.drift, weather.collapse_cut, weather.collapse_loss]
        .into_iter()
        .any(|percent| percent > 100)
    {
        return Err("drift, collapse_cut and collapse_loss are percentages, 0 to 100".to_owned());
    }
    Ok(settings)
}

/// A connection whose frames go through the node's chaos settings
#[derive(Debug)]
struct ChaosProtocolNetworkConnection {
    /// The connection being wrapped
    inner: ProtocolNetworkConnection,
    /// The node's settings and counters
    chaos: Chaos,
    /// The TCP segment size this end sends, where the platform reports it
    segment_size: Option<u32>,
    /// Set by a frame sent over a size cap; every later send is lost
    send_stalled: AtomicBool,
    /// Set by a frame received over a size cap; every later receive is lost
    recv_stalled: AtomicBool,
    /// When the connection first stalled
    stalled_ts: AtomicOptionTimestamp,
}

impl ChaosProtocolNetworkConnection {
    fn remote_ip(&self) -> IpAddr {
        self.inner.flow().remote().socket_addr().ip()
    }

    /// The largest packet a frame of `len` bytes puts on the path, taking the far end to send
    /// segments as large as ours
    fn largest_packet(&self, len: usize) -> usize {
        self.segment_size
            .map_or(len, |segment_size| len.min(segment_size as usize))
    }

    fn stall(&self, stalled: &AtomicBool) {
        stalled.store(true, Ordering::Relaxed);
        let now = Timestamp::now_non_decreasing();
        let _ = self
            .stalled_ts
            .fetch_update(|ts| ts.is_none().then_some(Some(now)));
    }

    /// Milliseconds until a stalled connection closes; None while it isn't stalled or never closes
    fn close_in_ms(&self) -> Option<u32> {
        let stalled_ts = self.stalled_ts.get()?;
        let close_ms = self.chaos.state.stall_close_ms.load(Ordering::Relaxed);
        if close_ms == 0 {
            return None;
        }
        let close_ts = stalled_ts.later(TimestampDuration::new_ms(close_ms));
        Some(
            close_ts
                .duration_since(Timestamp::now_non_decreasing())
                .millis_u32()
                .unwrap_or(u32::MAX),
        )
    }
}

fn stalled_stream_closed<T>() -> std::io::Result<NetworkResult<T>> {
    Ok(NetworkResult::no_connection_other("stalled stream closed"))
}

impl PlatformProtocolNetworkConnection for ChaosProtocolNetworkConnection {
    fn flow(&self) -> Flow {
        self.inner.flow()
    }

    fn direction(&self) -> Direction {
        self.inner.direction()
    }

    fn close(&self) -> PinBoxFuture<'_, std::io::Result<NetworkResult<()>>> {
        self.inner.close()
    }

    fn send(
        &self,
        message: Bytes,
        mtu: u16,
    ) -> PinBoxFuture<'_, std::io::Result<NetworkResult<()>>> {
        // A connection stalled only on sends closes on its first send after the deadline
        if self.close_in_ms() == Some(0) {
            return Box::pin(async { stalled_stream_closed() });
        }
        let verdict = if self.send_stalled.load(Ordering::Relaxed) {
            ChaosVerdict::Drop
        } else {
            let len = message.len();
            self.chaos.state.verdict(
                ChaosDirection::Send,
                self.remote_ip(),
                len,
                self.largest_packet(len).min(mtu.into()),
            )
        };
        match verdict {
            ChaosVerdict::Pass => self.inner.send(message, mtu),
            ChaosVerdict::Hold(wait_ms) => Box::pin(async move {
                sleep(wait_ms).await;
                self.inner.send(message, mtu).await
            }),
            ChaosVerdict::Oversize => {
                self.stall(&self.send_stalled);
                Box::pin(async { Ok(NetworkResult::value(())) })
            }
            ChaosVerdict::Drop => Box::pin(async { Ok(NetworkResult::value(())) }),
        }
    }

    fn recv(&self) -> PinBoxFuture<'_, std::io::Result<NetworkResult<Bytes>>> {
        Box::pin(async move {
            loop {
                // A read cut off here is lost with the connection it belonged to
                let out = match self.close_in_ms() {
                    Some(0) => return stalled_stream_closed(),
                    Some(wait_ms) => match timeout(wait_ms, self.inner.recv()).await {
                        Ok(res) => res?,
                        Err(_) => continue,
                    },
                    None => self.inner.recv().await?,
                };
                let NetworkResult::Value(frame) = &out else {
                    return Ok(out);
                };
                if self.recv_stalled.load(Ordering::Relaxed) {
                    continue;
                }
                let len = frame.len();
                match self.chaos.state.verdict(
                    ChaosDirection::Recv,
                    self.remote_ip(),
                    len,
                    self.largest_packet(len),
                ) {
                    ChaosVerdict::Pass => return Ok(out),
                    ChaosVerdict::Hold(wait_ms) => {
                        sleep(wait_ms).await;
                        return Ok(out);
                    }
                    ChaosVerdict::Oversize => self.stall(&self.recv_stalled),
                    ChaosVerdict::Drop => {}
                }
            }
        })
    }
}
