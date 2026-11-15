////////////////////////////////////////////////////////////////
// Debugging

use super::*;
use clap::{error::ErrorKind, CommandFactory, Parser, Subcommand};
use data_encoding::BASE64URL_NOPAD;
use hashlink::LinkedHashMap;
use network_manager::*;
use routing_table::*;
use std::fmt::Write;

impl_veilid_log_facility!("veilid_debug");

const DEBUG_HELP_TEMPLATE: &str = "{about-with-newline}{all-args}{after-help}";

/// Debugging subcommands
#[derive(Debug, Parser)]
#[command(
    name = "debug",
    disable_help_flag = true,
    disable_version_flag = true,
    help_template = "{subcommands}"
)]
struct DebugCommandParser {
    #[command(subcommand)]
    command: DebugCommand,
}

/// Subcommands supported by top-level debug command text.
#[derive(Debug, Subcommand)]
#[command(help_template = DEBUG_HELP_TEMPLATE)]
enum DebugCommand {
    /// display this node's id values
    Nodeid,
    /// display routing table bucket statistics
    Buckets {
        /// Optional minimum state filter (`punished`, `dead`, `reliable`, `unreliable`).
        #[arg(value_name = "min_state")]
        min_state: Option<String>,
    },
    /// display dialinfo in this node's routing domains
    Dialinfo,
    /// display local peer info
    Peerinfo {
        /// Optional tokens in any order:
        /// - routing domain: `public`, `local`, `pub`, `loc`
        /// - publication mode: `published`, `current`
        #[arg(value_name = "peerinfo_token")]
        args: Vec<String>,
    },
    /// explain contact mechanism for a node
    Contact {
        /// Filtered node reference:
        /// `<node>[+<sequencing>][/<protocoltype>][/<addresstype>][/<routingdomain>]`
        /// where protocol is `udp|tcp|ws|wss`, address is `ipv4|ipv6`,
        /// and routing domain is `pub|loc`.
        #[arg(value_name = "node_ref")]
        node_ref: String,
    },
    /// generate and display a random keypair
    Keypair {
        /// Optional crypto kind (`VLD0`, etc.).
        #[arg(value_name = "cryptokind")]
        cryptokind: Option<String>,
    },
    /// show routing table entry index
    Entries {
        /// Optional filters in any order:
        /// - minimum state: `dead` or `reliable`
        /// - mode: `fastest` or `liveness` (per-entry probe/answer ages and transport health)
        /// - capabilities list: comma-separated four-char codes
        ///   (for example `ROUT,SGNL,RLAY,DIAL,DHTV,APPM`).
        #[arg(value_name = "entries_token")]
        args: Vec<String>,
    },
    /// show details for a routing table entry
    Entry {
        /// Node id or filtered node reference.
        #[arg(value_name = "node")]
        node: String,
    },
    /// list, clear, add, or remove address-filter punishments
    Punish {
        #[command(subcommand)]
        command: Option<DebugPunishSubcommand>,
    },
    /// generate bootstrap TXT record data
    Txtrecord {
        /// Optional signing keypair group.
        #[arg(value_name = "keypairs")]
        keypairs: Option<String>,
    },
    /// change relays in use
    Relay {
        /// Relay configuration tokens:
        /// `[<relay>...] [public|local]`
        ///
        /// `<relay>` accepts node id or filtered node forms.
        #[arg(value_name = "relay_token")]
        args: Vec<String>,
    },
    /// inspect and control low-level transport availability
    Transport {
        /// `stats`: dump per-transport send-health windows and disabled set
        ///
        /// `clear`: purge all transport stats windows
        ///
        /// `enable|disable <selector>...`: set availability. A disabled
        /// transport's low-level sends and connects fail with NoConnection,
        /// exercising the normal evidence/suppression machinery.
        ///
        /// `toggle <selector>... [duration]`: flip availability, reverting
        /// after `duration` if given.
        ///
        /// `<selector>` is `udp|tcp` (both families), `ipv4|ipv6` (both
        /// protocols), or `<proto>/<family>` e.g. `udp/ipv6`. TCP covers
        /// WS/WSS (low-level transport granularity).
        #[arg(value_name = "transport_token")]
        args: Vec<String>,
    },
    /// lose, delay or rate-limit this node's packets, or lose its pushes, to test bad networks
    Chaos {
        /// No arguments prints every setting as JSON, starting from all-zero defaults.
        ///
        /// `<path>`: print one subtree, e.g. `net.self.recv`
        ///
        /// `<path>=<value> ...`: set, e.g. `net.self.send.loss=100` drops every packet sent
        /// (the send still reports success) and `net.self.recv.loss=50` discards half of those
        /// received. `net.ip['1.2.3.4'].send.loss=50` or `net.ip['2001:db8::/64'].recv.loss=25`
        /// adds loss for an address range; the longest matching range applies, on top of
        /// `net.self`. Loss is a percentage, 0 to 100.
        ///
        /// `max_size`, in `net.self` or a range, loses packets larger than that many bytes. A TCP
        /// or WS frame crosses it in the connection's segments, and one over it stalls the
        /// connection, as a path MTU black hole does, until `net.stall_close_ms` closes it.
        ///
        /// `net.self.send.rate=250000 net.self.send.queue_ms=800` makes this node's link carry
        /// 250000 bytes a second (`pps` caps packets a second) behind a queue holding 800 ms of
        /// that traffic: packets wait their turn, and one that finds the queue full is lost.
        /// `net.self.weather` changes the link over time: `drift` (percent) takes up to that much
        /// capacity away, redrawn every `drift_s` seconds, and collapses lasting about
        /// `collapse_s` seconds, about `collapse_every_s` seconds apart, cut `collapse_cut`
        /// percent of the capacity and lose `collapse_loss` percent of packets.
        ///
        /// `rpc.value_changed.loss=100` discards every ValueChanged statement received, as if the
        /// pushes went missing while the network works.
        ///
        /// `reset [<path>]`: restore every setting, or one subtree, to its default
        ///
        /// `preset [<name> ...] [<path>=<value> ...]`: start from defaults, then apply each preset
        /// and assignment in order, e.g. `preset mobile-bad mtu-blackhole` for a bad cellular link
        /// through a 464XLAT. No arguments lists the presets: `mobile`, `mobile-bad`,
        /// `mtu-blackhole`.
        ///
        /// `stats`: what each direction lost, held and still has queued, the link's capacity now
        /// and the collapses so far, and the pushes lost
        #[arg(value_name = "chaos_token")]
        args: Vec<String>,
    },
    /// send a status RPC question to a destination
    Ping {
        /// Destination grammar:
        /// - direct: `<node>[+<safety>][/<protocoltype>][/<addresstype>][/<routingdomain>]`
        /// - relay: `<target>@<relay_dialinfo>`
        /// - private: `#<index_or_route>[+<safety>]`
        #[arg(value_name = "destination")]
        destination: String,
    },
    /// send an app message statement
    Appmessage {
        /// Destination grammar matches `ping`.
        #[arg(value_name = "destination")]
        destination: String,
        /// Data payload grammar:
        /// - single-word string (for example `foobar`)
        /// - shell-quoted string (for example `"foo\nbar\n"`)
        /// - `#` followed by hex bytes (for example `#12AB34CD...`)
        #[arg(value_name = "data")]
        data: Vec<String>,
    },
    /// send an app call question
    Appcall {
        /// Destination grammar matches `ping`.
        #[arg(value_name = "destination")]
        destination: String,
        /// Data payload grammar:
        /// - single-word string (for example `foobar`)
        /// - shell-quoted string (for example `"foo\nbar\n"`)
        /// - `#` followed by hex bytes (for example `#12AB34CD...`)
        #[arg(value_name = "data")]
        data: Vec<String>,
    },
    /// reply to a pending app call
    Appreply {
        /// Optional `#call_id` value from a pending app call.
        #[arg(value_name = "id")]
        id: Option<String>,
        /// Data payload grammar:
        /// - single-word string (for example `foobar`)
        /// - shell-quoted string (for example `"foo\nbar\n"`)
        /// - `#` followed by hex bytes (for example `#12AB34CD...`)
        #[arg(value_name = "data")]
        data: Vec<String>,
    },
    /// resolve a destination across the network
    Resolve {
        /// Destination grammar matches `ping`.
        #[arg(value_name = "destination")]
        destination: String,
    },
    /// display detailed local node information
    Nodeinfo,
    /// purge local routing state
    Purge {
        #[command(subcommand)]
        command: Option<DebugPurgeSubcommand>,
    },
    /// attach this node to the network
    Attach,
    /// detach this node from the network
    Detach,
    /// inspect or mutate config
    Config {
        /// First positional argument:
        /// - `insecure` to read from full config view
        /// - otherwise treated as a config key path.
        #[arg(value_name = "mode_or_key")]
        arg_0: Option<String>,
        /// Optional config key path when `mode_or_key` is `insecure`.
        ///
        /// Config keys use dot-path grammar, for example:
        /// `network.protocol.udp.enabled`.
        #[arg(value_name = "config_key")]
        arg_1: Option<String>,
    },
    /// restart network subsystem or inspect network stats
    Network {
        #[command(subcommand)]
        command: Option<DebugNetworkSubcommand>,
    },
    /// allocate, publish, import, and test routes
    Route {
        #[command(subcommand)]
        command: Option<DebugRouteSubcommand>,
    },
    /// list, open, inspect, and mutate DHT records
    Record {
        #[command(subcommand)]
        command: Option<DebugRecordSubcommand>,
    },
    /// list tables and show table store details
    Table {
        #[command(subcommand)]
        command: Option<DebugTableSubcommand>,
    },
    /// show process and attachment uptime
    Uptime,
    /// show where the host application stands in its lifecycle
    Lifecycle,
    /// print or reset internal optimization stats
    #[cfg(feature = "stats")]
    Stats {
        #[command(subcommand)]
        command: Option<DebugStatsSubcommand>,
    },
    #[cfg(debug_assertions)]
    /// trigger crash behaviors for debug testing
    Die {
        /// Crash mode (`panic`, `unwrap`, `unwrap_or_log`, `expect`, `expect_or_log`, `div0`, `overflow`, `oob`, `nullptr`, `unreachable`).
        #[arg(value_name = "mode")]
        mode: Option<String>,
        /// Optional panic or expect message words.
        #[arg(value_name = "message")]
        message: Vec<String>,
    },
}

/// Subcommands supported by `purge`.
#[derive(Debug, Subcommand)]
#[command(
    help_template = DEBUG_HELP_TEMPLATE,
    disable_help_subcommand = true
)]
enum DebugPurgeSubcommand {
    /// Purge routing table buckets.
    #[command(help_template = DEBUG_HELP_TEMPLATE)]
    Buckets,
    /// Purge recent peer connections.
    #[command(help_template = DEBUG_HELP_TEMPLATE)]
    Connections,
    /// Purge route specifications.
    #[command(help_template = DEBUG_HELP_TEMPLATE)]
    Routes,
}

/// Subcommands supported by `network`.
#[derive(Debug, Subcommand)]
#[command(
    help_template = DEBUG_HELP_TEMPLATE,
    disable_help_subcommand = true
)]
enum DebugNetworkSubcommand {
    /// Restart the low-level network subsystem.
    #[command(help_template = DEBUG_HELP_TEMPLATE)]
    Restart,
    /// Print network manager statistics.
    #[command(help_template = DEBUG_HELP_TEMPLATE)]
    Stats,
}

/// Subcommands supported by `stats`.
#[cfg(feature = "stats")]
#[derive(Debug, Subcommand)]
#[command(
    help_template = DEBUG_HELP_TEMPLATE,
    disable_help_subcommand = true
)]
enum DebugStatsSubcommand {
    /// Print current stats as pretty JSON.
    #[command(help_template = DEBUG_HELP_TEMPLATE)]
    Print,
    /// Reset stats to start a fresh collection window.
    #[command(help_template = DEBUG_HELP_TEMPLATE)]
    Reset,
    /// Start, reconfigure, or stop periodic stats logging.
    #[command(help_template = DEBUG_HELP_TEMPLATE)]
    Log {
        /// Seconds between records (default 10), or `off` to stop.
        #[arg(value_name = "duration")]
        duration: Option<String>,
        /// File to append NDJSON to (native only); omit to log to the "stats" facility.
        #[arg(value_name = "path")]
        path: Option<String>,
    },
}

/// Subcommands supported by `route`.
#[derive(Debug, Subcommand)]
#[command(
    help_template = DEBUG_HELP_TEMPLATE,
    disable_help_subcommand = true
)]
enum DebugRouteSubcommand {
    /// Allocate a new route.
    #[command(help_template = DEBUG_HELP_TEMPLATE)]
    Allocate {
        /// Optional allocation arguments in any order:
        /// - sequencing: `ord`, `ord!`, `uno`
        /// - stability: `rel` or `low`
        /// - hop count: positive integer
        /// - direction: `in` or `out`
        /// - avoid: node id(s) that must not be hops
        #[arg(allow_hyphen_values = true, value_name = "allocate_arg")]
        params: Vec<String>,
    },
    /// Release an allocated route.
    #[command(help_template = DEBUG_HELP_TEMPLATE)]
    Release {
        /// Route id (full or prefix).
        #[arg(value_name = "route_id")]
        route_id: String,
    },
    /// Publish a route blob for import on another node.
    #[command(help_template = DEBUG_HELP_TEMPLATE)]
    Publish {
        /// Route id (full or prefix).
        #[arg(value_name = "route_id")]
        route_id: String,
        /// Include full route details when set to `full`.
        #[arg(value_name = "full")]
        full: Option<String>,
    },
    /// Mark a route as no longer published.
    #[command(help_template = DEBUG_HELP_TEMPLATE)]
    Unpublish {
        /// Route id (full or prefix).
        #[arg(value_name = "route_id")]
        route_id: String,
    },
    /// Show details for a route id or route key.
    #[command(help_template = DEBUG_HELP_TEMPLATE)]
    Print {
        /// Route id or route key (full or prefix).
        #[arg(value_name = "route")]
        route: String,
    },
    /// List allocated and imported routes.
    #[command(help_template = DEBUG_HELP_TEMPLATE)]
    List,
    /// Import a published route blob.
    #[command(help_template = DEBUG_HELP_TEMPLATE)]
    Import {
        /// Base64URL route blob from `route publish`.
        #[arg(value_name = "blob")]
        blob: String,
    },
    /// Test an allocated or imported route.
    #[command(help_template = DEBUG_HELP_TEMPLATE)]
    Test {
        /// Route id (full or prefix).
        #[arg(value_name = "route_id")]
        route_id: String,
    },
    /// Release the automatic safety route pool and allocate a fresh one now.
    #[command(help_template = DEBUG_HELP_TEMPLATE)]
    Reallocate,
    /// Show each hop's dial info and the contact methods between adjacent hops per sequencing.
    #[command(help_template = DEBUG_HELP_TEMPLATE)]
    Diag {
        /// Allocated route id (full or prefix).
        #[arg(value_name = "route_id")]
        route_id: String,
    },
}

/// Subcommands supported by `record`.
#[derive(Debug, Subcommand)]
#[command(
    help_template = DEBUG_HELP_TEMPLATE,
    disable_help_subcommand = true
)]
enum DebugRecordSubcommand {
    /// List records by scope.
    #[command(help_template = DEBUG_HELP_TEMPLATE)]
    List {
        /// Record scope (`local`, `remote`, `opened`, `offline`, `watched`, `transactions`).
        #[arg(value_name = "scope")]
        scope: String,
    },
    /// Purge local or remote records.
    #[command(help_template = DEBUG_HELP_TEMPLATE)]
    Purge {
        /// Scope to purge (`local` or `remote`).
        #[arg(value_name = "scope")]
        scope: String,
        /// Optional byte target to purge down to. Conflicts with `--keys`.
        #[arg(value_name = "bytes", group = "target")]
        bytes: Option<u64>,
        /// Optional comma-delimited list of record keys to purge. No spaces allowed, conflicts with `bytes`.
        #[arg(long, value_name = "keys", value_delimiter = ',', group = "target")]
        keys: Option<Vec<String>>,
    },
    /// Create a new DHT record.
    #[command(help_template = DEBUG_HELP_TEMPLATE)]
    Create {
        /// DHT schema JSON or default subkey count.
        #[arg(value_name = "dht_schema")]
        dht_schema: Option<String>,
        /// Crypto kind (for example `VLD0`).
        #[arg(value_name = "crypto_kind")]
        crypto_kind: Option<String>,
        /// Safety selection expression.
        #[arg(value_name = "safety")]
        safety: Option<String>,
    },
    /// Open an existing DHT record.
    #[command(help_template = DEBUG_HELP_TEMPLATE)]
    Open {
        /// Record key with optional `+safety` suffix.
        #[arg(value_name = "key")]
        key: String,
        /// Optional writer keypair.
        #[arg(value_name = "writer")]
        writer: Option<String>,
    },
    /// Close an opened DHT record.
    #[command(help_template = DEBUG_HELP_TEMPLATE)]
    Close {
        /// Optional key (defaults to most recently opened record).
        #[arg(value_name = "key")]
        key: Option<String>,
    },
    /// Read a value from a DHT record subkey.
    #[command(help_template = DEBUG_HELP_TEMPLATE)]
    Get {
        /// Positional form:
        /// - `[key] <subkey> [force]`
        /// - if `key` is omitted, the most recently opened record is used
        /// - `force` refreshes from the network
        #[arg(allow_hyphen_values = true, value_name = "get_arg")]
        params: Vec<String>,
    },
    /// Write a value to a DHT record subkey.
    #[command(help_template = DEBUG_HELP_TEMPLATE)]
    Set {
        /// Positional form:
        /// - `[key] <subkey> <data> [writer] [offline|online|true|false] [min_seqnum]`
        /// - if `key` is omitted, the most recently opened record is used
        /// - `<data>` may be plain text, quoted text, or `#`-prefixed hex
        /// - `min_seqnum` is `max`, `max-<n>` leaving `n` writes, or a sequence number
        #[arg(allow_hyphen_values = true, value_name = "set_arg")]
        params: Vec<String>,
    },
    /// Delete the local copy of a DHT record.
    #[command(help_template = DEBUG_HELP_TEMPLATE)]
    Delete {
        /// Record key.
        #[arg(value_name = "key")]
        key: String,
    },
    /// Show local and remote record information.
    #[command(help_template = DEBUG_HELP_TEMPLATE)]
    Info {
        /// Record key.
        #[arg(value_name = "key")]
        key: String,
        /// Optional subkey to inspect.
        #[arg(value_name = "subkey")]
        subkey: Option<u32>,
    },
    /// Watch a record for value changes.
    #[command(help_template = DEBUG_HELP_TEMPLATE)]
    Watch {
        /// Optional key (defaults to most recently opened record).
        #[arg(value_name = "key")]
        key: Option<String>,
        /// Optional subkeys or ranges.
        #[arg(value_name = "subkeys")]
        subkeys: Option<String>,
        /// Optional expiration duration.
        #[arg(value_name = "expiration")]
        expiration: Option<String>,
        /// Optional max change count.
        #[arg(value_name = "count")]
        count: Option<u32>,
    },
    /// Cancel a record watch.
    #[command(help_template = DEBUG_HELP_TEMPLATE)]
    Cancel {
        /// Optional key (defaults to most recently opened record).
        #[arg(value_name = "key")]
        key: Option<String>,
        /// Optional subkeys or ranges.
        #[arg(value_name = "subkeys")]
        subkeys: Option<String>,
    },
    /// Inspect DHT record subkey status.
    #[command(help_template = DEBUG_HELP_TEMPLATE)]
    Inspect {
        /// Optional key (defaults to most recently opened record).
        #[arg(value_name = "key")]
        key: Option<String>,
        /// Optional inspect scope (`local|syncget|syncset|updateget|updateset`).
        #[arg(value_name = "scope")]
        scope: Option<String>,
        /// Optional subkeys or ranges.
        #[arg(value_name = "subkeys")]
        subkeys: Option<String>,
    },
    /// Rehydrate expired local record data onto the network.
    #[command(help_template = DEBUG_HELP_TEMPLATE)]
    Rehydrate {
        /// Record key.
        #[arg(value_name = "key")]
        key: String,
        /// Optional subkeys or ranges.
        #[arg(value_name = "subkeys")]
        subkeys: Option<String>,
        /// Optional consensus count.
        #[arg(value_name = "consensus_count")]
        consensus_count: Option<usize>,
    },
}

/// Subcommands supported by `table`.
#[derive(Debug, Subcommand)]
#[command(
    help_template = DEBUG_HELP_TEMPLATE,
    disable_help_subcommand = true
)]
enum DebugTableSubcommand {
    /// List all table store tables.
    #[command(help_template = DEBUG_HELP_TEMPLATE)]
    List,
    /// Show details and IO stats for a table.
    #[command(help_template = DEBUG_HELP_TEMPLATE)]
    Info {
        /// Table name.
        #[arg(value_name = "name")]
        name: String,
    },
}

/// Subcommands supported by `punish`.
#[derive(Debug, Subcommand)]
#[command(
    help_template = DEBUG_HELP_TEMPLATE,
    disable_help_subcommand = true
)]
enum DebugPunishSubcommand {
    /// List all address-filter punishments.
    #[command(help_template = DEBUG_HELP_TEMPLATE)]
    List,
    /// Clear all address-filter punishments.
    #[command(help_template = DEBUG_HELP_TEMPLATE)]
    Clear,
    /// Add a punishment for a node id or IP address.
    #[command(help_template = DEBUG_HELP_TEMPLATE)]
    Add {
        /// Target node id (`VLD0:...`) or IP address.
        #[arg(value_name = "target")]
        target: String,
    },
    /// Remove a punishment for a node id or IP address.
    #[command(help_template = DEBUG_HELP_TEMPLATE)]
    Remove {
        /// Target node id (`VLD0:...`) or IP address.
        #[arg(value_name = "target")]
        target: String,
    },
}

fn render_parser_help<T: CommandFactory>() -> String {
    let mut command = T::command();
    let mut bytes = Vec::new();
    if command.write_long_help(&mut bytes).is_err() {
        return String::new();
    }
    String::from_utf8(bytes).unwrap_or_else(|e| String::from_utf8_lossy(&e.into_bytes()).into())
}

fn join_args(args: Vec<String>) -> String {
    args.join(" ")
}

#[derive(Default)]
pub(crate) struct DebugCache {
    pub imported_routes: Vec<RouteId>,
    pub opened_record_contexts: LinkedHashMap<RecordKey, RoutingContext>,
}

#[must_use]
pub(crate) fn format_opt_ts(ts: Option<TimestampDuration>) -> String {
    let Some(ts) = ts else {
        return "---".to_owned();
    };
    format!("{:#}", ts)
}

#[must_use]
pub(crate) fn format_opt_bps(bps: Option<ByteCount>) -> String {
    let Some(bps) = bps else {
        return "---".to_owned();
    };
    format!("{:#}/s", bps)
}

fn parse_bucket_entry_state(text: &str) -> Result<BucketEntryState, String> {
    match text {
        "punished" => Ok(BucketEntryState::Punished),
        "dead" => Ok(BucketEntryState::Dead),
        "reliable" => Ok(BucketEntryState::Reliable),
        "unreliable" => Ok(BucketEntryState::Unreliable),
        _ => Err(format!("invalid bucket entry state: {text}")),
    }
}

fn get_bucket_entry_state(text: &str) -> Option<BucketEntryState> {
    parse_bucket_entry_state(text).ok()
}

fn get_string(text: &str) -> Option<String> {
    Some(text.to_owned())
}

fn parse_data(text: &str) -> Result<Vec<u8>, String> {
    if let Some(stripped_text) = text.strip_prefix('#') {
        hex::decode(stripped_text).map_err(|e| format!("invalid hex data: {e}"))
    } else if text.starts_with('"') || text.starts_with('\'') {
        serde_json::from_str::<String>(text)
            .map(|x| x.into_bytes())
            .map_err(|e| format!("invalid quoted data: {e}"))
    } else {
        Ok(text.as_bytes().to_vec())
    }
}

fn get_data(text: &str) -> Option<Vec<u8>> {
    parse_data(text).ok()
}

fn parse_subkeys(text: &str) -> Result<ValueSubkeyRangeSet, String> {
    if let Some(n) = get_number::<u32>(text) {
        Ok(ValueSubkeyRangeSet::single(n))
    } else {
        ValueSubkeyRangeSet::from_str(text).map_err(|e| format!("invalid subkeys: {e}"))
    }
}

fn get_subkeys(text: &str) -> Option<ValueSubkeyRangeSet> {
    parse_subkeys(text).ok()
}

fn parse_dht_report_scope(text: &str) -> Result<DHTReportScope, String> {
    match text.to_ascii_lowercase().trim() {
        "local" => Ok(DHTReportScope::Local),
        "syncget" => Ok(DHTReportScope::SyncGet),
        "syncset" => Ok(DHTReportScope::SyncSet),
        "updateget" => Ok(DHTReportScope::UpdateGet),
        "updateset" => Ok(DHTReportScope::UpdateSet),
        _ => Err(format!("invalid dht report scope: {text}")),
    }
}

fn get_dht_report_scope(text: &str) -> Option<DHTReportScope> {
    parse_dht_report_scope(text).ok()
}

fn parse_allow_offline(text: &str) -> Result<AllowOffline, String> {
    match text.to_ascii_lowercase().trim() {
        "online" | "false" => Ok(AllowOffline(false)),
        "offline" | "true" => Ok(AllowOffline(true)),
        _ => Err(format!("invalid allow_offline: {text}")),
    }
}

fn get_allow_offline(text: &str) -> Option<AllowOffline> {
    parse_allow_offline(text).ok()
}

/// Accepts `max`, `max-<n>` (leaving `n` writes), or a bare sequence number.
pub(super) fn parse_value_seq_num(text: &str) -> Result<ValueSeqNum, String> {
    let text = text.to_ascii_lowercase();
    let text = text.trim();
    let Some(remaining) = text.strip_prefix("max") else {
        return text
            .parse::<u32>()
            .map(ValueSeqNum::from)
            .map_err(|e| format!("invalid sequence number '{text}': {e}"));
    };
    if remaining.is_empty() {
        return Ok(ValueSeqNum::MAX);
    }
    let Some(count) = remaining.strip_prefix('-') else {
        return Err(format!("invalid sequence number: {text}"));
    };
    let count = count
        .parse::<u32>()
        .map_err(|e| format!("invalid remaining write count '{count}': {e}"))?;
    let max = u32::from(ValueSeqNum::MAX);
    max.checked_sub(count)
        .map(ValueSeqNum::from)
        .ok_or_else(|| format!("more remaining writes than sequence numbers: {text}"))
}

fn transport_list(transports: &[LowLevelTransportType]) -> String {
    transports
        .iter()
        .map(|t| t.to_string())
        .collect::<Vec<_>>()
        .join(", ")
}

enum PublishedState {
    Published,
    Current,
}

impl PublishedState {
    fn as_bool(&self) -> bool {
        matches!(self, Self::Published)
    }
}

impl FromStr for PublishedState {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_ascii_lowercase().as_str() {
            "published" => Ok(Self::Published),
            "current" => Ok(Self::Current),
            _ => Err(format!("invalid published mode: {s}")),
        }
    }
}

fn get_nested_debug_family_name(arg: Option<&str>) -> Option<String> {
    let top_level = arg?;
    let parser = DebugCommandParser::command();
    let command = parser.find_subcommand(top_level)?;
    if command.get_subcommands().next().is_some() {
        Some(top_level.to_owned())
    } else {
        None
    }
}

fn get_known_debug_command_name(arg: Option<&str>) -> Option<String> {
    let top_level = arg?;
    let parser = DebugCommandParser::command();
    if parser.find_subcommand(top_level).is_some() {
        Some(top_level.to_owned())
    } else {
        None
    }
}

fn get_route_id(
    registry: VeilidComponentRegistry,
    allow_allocated: bool,
    allow_remote: bool,
) -> impl Fn(&str) -> Option<RouteId> {
    move |text: &str| {
        if text.is_empty() {
            return None;
        }
        let routing_table = registry.routing_table();
        let rss = routing_table.route_spec_store();

        match RouteId::from_str(text).ok() {
            Some(key) => {
                if allow_allocated {
                    let routes: Vec<RouteId> =
                        rss.list_allocated_routes(|k, _| Some(k.clone().into()));
                    if routes.contains(&key) {
                        return Some(key);
                    }
                }
                if allow_remote {
                    let rroutes: Vec<RouteId> =
                        rss.list_remote_routes(|k, _| Some(k.clone().into()));
                    if rroutes.contains(&key) {
                        return Some(key);
                    }
                }
            }
            None => {
                if allow_allocated {
                    let routes: Vec<RouteId> =
                        rss.list_allocated_routes(|k, _| Some(k.clone().into()));
                    for r in routes {
                        let rkey = r.to_string();
                        if rkey.starts_with(text)
                            || rkey
                                .split_once(':')
                                .map(|(_, b)| b.starts_with(text))
                                .unwrap_or(false)
                        {
                            return Some(r);
                        }
                    }
                }
                if allow_remote {
                    let routes: Vec<RouteId> =
                        rss.list_remote_routes(|k, _| Some(k.clone().into()));
                    for r in routes {
                        let rkey = r.to_string();
                        if rkey.starts_with(text)
                            || rkey
                                .split_once(':')
                                .map(|(_, b)| b.starts_with(text))
                                .unwrap_or(false)
                        {
                            return Some(r);
                        }
                    }
                }
            }
        }
        None
    }
}

fn get_route_key(
    registry: VeilidComponentRegistry,
    allow_allocated: bool,
    allow_remote: bool,
) -> impl Fn(&str) -> Option<PublicKey> {
    move |text: &str| {
        if text.is_empty() {
            return None;
        }
        let routing_table = registry.routing_table();
        let rss = routing_table.route_spec_store();

        match PublicKey::from_str(text).ok() {
            Some(key) => {
                if allow_allocated {
                    let allocated_route_keys =
                        rss.list_allocated_routes(|_, arce| Some(arce.route_set_keys().to_vec()));
                    if let Some(found_key) = allocated_route_keys
                        .into_iter()
                        .flatten()
                        .find(|x| x == &key)
                    {
                        return Some(found_key);
                    }
                }
                if allow_remote {
                    let remote_route_keys = rss.list_remote_routes(|_, v| {
                        Some(
                            v.get_private_routes()
                                .iter()
                                .map(|x| x.public_key.clone())
                                .collect::<Vec<_>>(),
                        )
                    });
                    if let Some(found_key) =
                        remote_route_keys.into_iter().flatten().find(|x| x == &key)
                    {
                        return Some(found_key);
                    }
                }
            }
            None => {
                if allow_allocated {
                    let allocated_route_keys =
                        rss.list_allocated_routes(|_, arce| Some(arce.route_set_keys().to_vec()));
                    for r in allocated_route_keys.into_iter().flatten() {
                        let rkey = r.to_string();
                        if rkey.starts_with(text)
                            || rkey
                                .split_once(':')
                                .map(|(_, b)| b.starts_with(text))
                                .unwrap_or(false)
                        {
                            return Some(r);
                        }
                    }
                }
                if allow_remote {
                    let remote_route_keys = rss.list_remote_routes(|_, v| {
                        Some(
                            v.get_private_routes()
                                .iter()
                                .map(|x| x.public_key.clone())
                                .collect::<Vec<_>>(),
                        )
                    });
                    for r in remote_route_keys.into_iter().flatten() {
                        let rkey = r.to_string();
                        if rkey.starts_with(text)
                            || rkey
                                .split_once(':')
                                .map(|(_, b)| b.starts_with(text))
                                .unwrap_or(false)
                        {
                            return Some(r);
                        }
                    }
                }
            }
        }
        None
    }
}

fn get_dht_schema(text: &str) -> Option<VeilidAPIResult<DHTSchema>> {
    if text.is_empty() {
        return None;
    }
    if let Ok(n) = u16::from_str(text) {
        return Some(DHTSchema::dflt(n));
    }
    Some(deserialize_json::<DHTSchema>(text))
}

fn get_safety_selection(
    registry: VeilidComponentRegistry,
) -> impl Fn(&str) -> Option<SafetySelection> {
    move |text| {
        let default_route_hop_count =
            registry.config().network.rpc.default_route_hop_count as usize;

        if let Some(text) = text.strip_prefix('-') {
            // Unsafe
            let seq = get_sequencing(text).unwrap_or_default();
            Some(SafetySelection::Unsafe(seq))
        } else {
            // Safe
            let mut preferred_route = None;
            let mut hop_count = default_route_hop_count;
            let mut stability = Stability::Reliable;
            let mut sequencing = Sequencing::PreferUnordered;
            for x in text.split(',') {
                let x = x.trim();
                if let Some(pr) = get_route_id(registry.clone(), true, false)(x) {
                    preferred_route = Some(pr)
                }
                if let Some(n) = get_number(x) {
                    hop_count = n;
                }
                if let Some(s) = get_stability(x) {
                    stability = s;
                }
                if let Some(s) = get_sequencing(x) {
                    sequencing = s;
                }
            }

            let ss = SafetySpec {
                preferred_route,
                hop_count,
                stability,
                sequencing,
            };
            Some(SafetySelection::Safe(ss))
        }
    }
}

fn get_node_ref_modifiers(node_ref: NodeRef) -> impl FnOnce(&str) -> Option<FilteredNodeRef> {
    move |text| {
        let mut node_ref = node_ref.sequencing_filtered(Sequencing::PreferUnordered);
        for m in text.split('/') {
            if let Some(pt) = get_protocol_type(m) {
                node_ref.merge_filter(NodeRefFilter::new().with_protocol_type(pt));
            } else if let Some(at) = get_address_type(m) {
                node_ref.merge_filter(NodeRefFilter::new().with_address_type(at));
            } else {
                let rd = get_routing_domain(m)?;
                node_ref.merge_filter(NodeRefFilter::new().with_routing_domain(rd));
            }
        }
        Some(node_ref)
    }
}

fn get_number<T: num_traits::Num + FromStr>(text: &str) -> Option<T> {
    T::from_str(text).ok()
}

fn get_record_key(text: &str) -> Option<RecordKey> {
    RecordKey::from_str(text).ok()
}
fn get_bare_node_id(text: &str) -> Option<BareNodeId> {
    let bare_node_id = BareNodeId::from_str(text).ok()?;

    // Enforce 32 byte node ids
    if bare_node_id.len() != 32 {
        return None;
    }
    Some(bare_node_id)
}
fn get_node_id(text: &str) -> Option<NodeId> {
    let node_id = NodeId::from_str(text).ok()?;

    // Enforce 32 byte node ids
    if node_id.value().len() != 32 {
        return None;
    }
    Some(node_id)
}
fn get_keypair(text: &str) -> Option<KeyPair> {
    KeyPair::from_str(text).ok()
}
fn get_keypair_group(text: &str) -> Option<KeyPairGroup> {
    KeyPairGroup::from_str(text).ok()
}

fn get_crypto_system_version<'a>(
    crypto: &'a Crypto,
) -> impl FnOnce(&str) -> Option<CryptoSystemGuard<'a>> {
    move |text| {
        let kindstr = get_string(text)?;
        let kind = CryptoKind::from_str(&kindstr).ok()?;
        crypto.get(kind)
    }
}

fn get_dht_key_no_safety(text: &str) -> Option<RecordKey> {
    let key = get_record_key(text)?;

    Some(key)
}

fn get_dht_key(
    registry: VeilidComponentRegistry,
) -> impl FnOnce(&str) -> Option<(RecordKey, Option<SafetySelection>)> {
    move |text| {
        // Safety selection
        let (text, ss) = if let Some((first, second)) = text.split_once('+') {
            let ss = get_safety_selection(registry)(second)?;
            (first, Some(ss))
        } else {
            (text, None)
        };
        if text.is_empty() {
            return None;
        }

        let key = get_record_key(text)?;

        Some((key, ss))
    }
}

fn resolve_node_ref(
    registry: VeilidComponentRegistry,
    safety_selection: SafetySelection,
) -> impl FnOnce(&str) -> PinBoxFutureStatic<Option<NodeRef>> {
    move |text| {
        let text = text.to_owned();
        Box::pin(async move {
            let node_id = get_node_id(&text)?;
            let nr = registry
                .rpc_processor()
                .resolve_node(node_id, safety_selection)
                .await
                .ok()
                .flatten()?;
            Some(nr)
        })
    }
}

fn resolve_filtered_node_ref(
    registry: VeilidComponentRegistry,
    safety_selection: SafetySelection,
) -> impl FnOnce(&str) -> PinBoxFutureStatic<Option<FilteredNodeRef>> {
    move |text| {
        let text = text.to_owned();
        Box::pin(async move {
            let (text, mods) = text
                .split_once('/')
                .map(|x| (x.0, Some(x.1)))
                .unwrap_or((&text, None));

            let node_id = get_node_id(text)?;
            let nr = registry
                .rpc_processor()
                .resolve_node(node_id, safety_selection)
                .await
                .ok()
                .flatten()?;
            if let Some(mods) = mods {
                Some(get_node_ref_modifiers(nr)(mods)?)
            } else {
                Some(nr.sequencing_filtered(Sequencing::PreferUnordered))
            }
        })
    }
}

fn get_node_ref(registry: VeilidComponentRegistry) -> impl FnOnce(&str) -> Option<NodeRef> {
    move |text| {
        let routing_table = registry.routing_table();
        let nr = if let Some(key) = get_bare_node_id(text) {
            routing_table.lookup_bare_node_id(key).ok().flatten()?
        } else {
            let node_id = get_node_id(text)?;
            routing_table.lookup_node_id(node_id).ok().flatten()?
        };
        Some(nr)
    }
}

fn get_filtered_node_ref(
    registry: VeilidComponentRegistry,
) -> impl FnOnce(&str) -> Option<FilteredNodeRef> {
    move |text| {
        let routing_table = registry.routing_table();
        FilteredNodeRef::parse(&routing_table, text).ok().flatten()
    }
}

fn get_protocol_type(text: &str) -> Option<ProtocolType> {
    ProtocolType::from_str(text).ok()
}
fn get_sequencing(text: &str) -> Option<Sequencing> {
    Sequencing::from_str(text).ok()
}
fn get_stability(text: &str) -> Option<Stability> {
    Stability::from_str(text).ok()
}
fn get_direction_set(text: &str) -> Option<DirectionSet> {
    Direction::set_from_str(text).ok()
}

fn get_address_type(text: &str) -> Option<AddressType> {
    AddressType::from_str(text).ok()
}
fn get_routing_domain(text: &str) -> Option<RoutingDomain> {
    RoutingDomain::from_str(text).ok()
}

fn get_ip_addr(text: &str) -> Option<IpAddr> {
    IpAddr::from_str(text).ok()
}

fn get_published(text: &str) -> Option<bool> {
    PublishedState::from_str(text).ok().map(|x| x.as_bool())
}

fn get_debug_argument<T, G: FnOnce(&str) -> Option<T>>(
    value: &str,
    context: &str,
    argument: &str,
    getter: G,
) -> VeilidAPIResult<T> {
    let Some(val) = getter(value) else {
        apibail_invalid_argument!(context, argument, value);
    };
    Ok(val)
}

async fn async_get_debug_argument<T, G: FnOnce(&str) -> PinBoxFutureStatic<Option<T>>>(
    value: &str,
    context: &str,
    argument: &str,
    getter: G,
) -> VeilidAPIResult<T> {
    let Some(val) = getter(value).await else {
        apibail_invalid_argument!(context, argument, value);
    };
    Ok(val)
}

fn get_debug_argument_at<T, G: FnOnce(&str) -> Option<T>>(
    debug_args: &[String],
    pos: usize,
    context: &str,
    argument: &str,
    getter: G,
) -> VeilidAPIResult<T> {
    if pos >= debug_args.len() {
        apibail_missing_argument!(context, argument);
    }
    let value = &debug_args[pos];
    let Some(val) = getter(value) else {
        apibail_invalid_argument!(context, argument, value.clone());
    };
    Ok(val)
}

async fn async_get_debug_argument_at<T, G: FnOnce(&str) -> PinBoxFutureStatic<Option<T>>>(
    debug_args: &[String],
    pos: usize,
    context: &str,
    argument: &str,
    getter: G,
) -> VeilidAPIResult<T> {
    if pos >= debug_args.len() {
        apibail_missing_argument!(context, argument);
    }
    let value = &debug_args[pos];
    let Some(val) = getter(value).await else {
        apibail_invalid_argument!(context, argument, value.clone());
    };
    Ok(val)
}

impl VeilidAPI {
    fn debug_buckets(&self, opt_min_state: Option<String>) -> VeilidAPIResult<String> {
        let mut min_state = BucketEntryState::Unreliable;
        if let Some(min_state_str) = opt_min_state {
            min_state = get_debug_argument(
                &min_state_str,
                "debug_buckets",
                "min_state",
                get_bucket_entry_state,
            )?;
        }
        // Dump routing table bucket info
        let routing_table = self.core_context()?.routing_table();
        Ok(routing_table.debug_info_buckets(min_state))
    }

    fn debug_dialinfo(&self) -> VeilidAPIResult<String> {
        // Dump routing table dialinfo
        let routing_table = self.core_context()?.routing_table();
        Ok(routing_table.debug_info_dialinfo())
    }
    fn debug_peerinfo(&self, args: Vec<String>) -> VeilidAPIResult<String> {
        // Dump routing table peerinfo
        let routing_table = self.core_context()?.routing_table();

        let mut ai = 0;
        let mut opt_routing_domain = None;
        let mut opt_published = None;

        while ai < args.len() {
            if let Ok(routing_domain) = get_debug_argument_at(
                &args,
                ai,
                "debug_peerinfo",
                "routing_domain",
                get_routing_domain,
            ) {
                opt_routing_domain = Some(routing_domain);
            } else if let Ok(published) =
                get_debug_argument_at(&args, ai, "debug_peerinfo", "published", get_published)
            {
                opt_published = Some(published);
            }
            ai += 1;
        }

        let routing_domain = opt_routing_domain.unwrap_or(RoutingDomain::PublicInternet);
        let published = opt_published.unwrap_or(true);

        Ok(routing_table.debug_info_peerinfo(routing_domain, published))
    }

    async fn debug_txtrecord(&self, keypairs: Option<String>) -> VeilidAPIResult<String> {
        // Dump routing table txt record
        let signing_key_pairs = if let Some(keypairs) = keypairs {
            get_debug_argument(
                &keypairs,
                "debug_txtrecord",
                "signing_key_pairs",
                get_keypair_group,
            )?
        } else {
            KeyPairGroup::new()
        };

        let network_manager = self.core_context()?.network_manager();
        Ok(network_manager
            .debug_info_txtrecord(signing_key_pairs)
            .await)
    }

    fn debug_keypair(&self, cryptokind: Option<String>) -> VeilidAPIResult<String> {
        let crypto = self.crypto()?;

        let vcrypto = if let Some(cryptokind) = cryptokind {
            get_debug_argument(
                &cryptokind,
                "debug_keypair",
                "kind",
                get_crypto_system_version(&crypto),
            )?
        } else {
            crypto.best()
        };

        // Generate a keypair
        let out = vcrypto.generate_keypair().to_string();
        Ok(out)
    }

    fn debug_entries(&self, args: Vec<String>) -> VeilidAPIResult<String> {
        let mut min_state = BucketEntryState::Missing;
        let mut capabilities = vec![];
        let mut fastest = false;
        let mut liveness = false;
        for arg in args {
            if let Some(ms) = get_bucket_entry_state(&arg) {
                min_state = ms;
            } else if arg == "fastest" {
                fastest = true;
            } else if arg == "liveness" {
                liveness = true;
            } else {
                for cap in arg.split(',') {
                    if let Ok(cap) = VeilidCapability::from_str(cap) {
                        capabilities.push(cap);
                    } else {
                        apibail_invalid_argument!("debug_entries", "unknown", arg);
                    }
                }
            }
        }

        // Dump routing table entries
        let routing_table = self.core_context()?.routing_table();
        Ok(if liveness {
            routing_table.debug_info_entries_liveness()
        } else if fastest {
            routing_table.debug_info_entries_fastest(min_state, capabilities, 100000)
        } else {
            routing_table.debug_info_entries(min_state, capabilities)
        })
    }

    fn debug_entry(&self, node: String) -> VeilidAPIResult<String> {
        let registry = self.core_context()?.registry();

        let node_ref = get_debug_argument(
            &node,
            "debug_entry",
            "node_id",
            get_node_ref(registry.clone()),
        )?;

        // Dump routing table entry
        Ok(registry.routing_table().debug_info_entry(node_ref))
    }

    /// Expand a transport selector token into low-level transports.
    /// `udp`/`tcp` = both families; `ipv4`/`ipv6` = both protocols; `udp/ipv6` = one.
    fn parse_transport_selector(token: &str) -> Option<Vec<LowLevelTransportType>> {
        fn proto(s: &str) -> Option<LowLevelProtocolType> {
            match s {
                "udp" => Some(LowLevelProtocolType::UDP),
                "tcp" => Some(LowLevelProtocolType::TCP),
                _ => None,
            }
        }
        fn family(s: &str) -> Option<AddressType> {
            match s {
                "ipv4" | "v4" => Some(AddressType::IPV4),
                "ipv6" | "v6" => Some(AddressType::IPV6),
                _ => None,
            }
        }
        let token = token.to_ascii_lowercase();
        let (protos, families) = if let Some((p, f)) = token.split_once('/') {
            (vec![proto(p)?], vec![family(f)?])
        } else if let Some(p) = proto(&token) {
            (vec![p], vec![AddressType::IPV4, AddressType::IPV6])
        } else {
            let f = family(&token)?;
            (
                vec![LowLevelProtocolType::UDP, LowLevelProtocolType::TCP],
                vec![f],
            )
        };
        let mut out = Vec::new();
        for p in &protos {
            for f in &families {
                out.push(LowLevelTransportType {
                    low_level_protocol_type: *p,
                    address_type: *f,
                });
            }
        }
        Some(out)
    }

    fn debug_chaos(&self, args: Vec<String>) -> VeilidAPIResult<String> {
        let registry = self.core_context()?.registry();
        registry
            .network_manager()
            .chaos()
            .command(&args)
            .map_err(VeilidAPIError::generic)
    }

    fn debug_transport(&self, args: Vec<String>) -> VeilidAPIResult<String> {
        let registry = self.core_context()?.registry();
        let network_manager = registry.network_manager();
        let transport_stats = network_manager.transport_stats();

        let Some(subcommand) = args.first() else {
            apibail_missing_argument!("debug_transport", "subcommand");
        };
        match subcommand.as_str() {
            "stats" => Ok(transport_stats.debug_stats()),
            "clear" => {
                transport_stats.clear_all_windows();
                Ok("transport stats cleared".to_owned())
            }
            "enable" | "disable" | "toggle" => {
                let mut selector_args: Vec<&String> = args[1..].iter().collect();
                let mut opt_duration_us: Option<u64> = None;
                if *subcommand == "toggle" {
                    if let Some(last) = selector_args.last() {
                        if let Some(dur) = parse_duration(last) {
                            // A token that parses as both stays a selector
                            if Self::parse_transport_selector(last).is_none() {
                                opt_duration_us = Some(dur);
                                selector_args.pop();
                            }
                        }
                    }
                }
                if selector_args.is_empty() {
                    apibail_missing_argument!("debug_transport", "selector");
                }
                let mut transports: Vec<LowLevelTransportType> = Vec::new();
                for token in &selector_args {
                    let Some(mut tts) = Self::parse_transport_selector(token) else {
                        apibail_invalid_argument!("debug_transport", "selector", token.to_string());
                    };
                    transports.append(&mut tts);
                }
                transports.sort();
                transports.dedup();

                let mut out = String::new();
                match subcommand.as_str() {
                    "enable" | "disable" => {
                        let disable = subcommand.as_str() == "disable";
                        transport_stats.set_transports_disabled(&transports, disable);
                        out += &format!(
                            "{}: {}\n",
                            if disable { "disabled" } else { "enabled" },
                            transport_list(&transports)
                        );
                    }
                    "toggle" => {
                        let currently_disabled = transport_stats.disabled_transports();
                        let to_enable: Vec<LowLevelTransportType> = transports
                            .iter()
                            .filter(|t| currently_disabled.contains(t))
                            .copied()
                            .collect();
                        let to_disable: Vec<LowLevelTransportType> = transports
                            .iter()
                            .filter(|t| !currently_disabled.contains(t))
                            .copied()
                            .collect();
                        let mut gen = 0u64;
                        if !to_enable.is_empty() {
                            gen = transport_stats.set_transports_disabled(&to_enable, false);
                        }
                        if !to_disable.is_empty() {
                            gen = transport_stats.set_transports_disabled(&to_disable, true);
                        }
                        out += &format!(
                            "toggled: enabled [{}] disabled [{}]\n",
                            transport_list(&to_enable),
                            transport_list(&to_disable)
                        );
                        // The generation check makes the revert skip anything changed since
                        if let Some(dur_us) = opt_duration_us {
                            let registry = registry.clone();
                            spawn_detached("transport toggle revert", async move {
                                sleep((dur_us / 1000) as u32).await;
                                let network_manager = registry.network_manager();
                                let ts = network_manager.transport_stats();
                                ts.set_transports_disabled_if_generation(&to_enable, true, gen);
                                ts.set_transports_disabled_if_generation(&to_disable, false, gen);
                            });
                            out += &format!("reverting in {:#}\n", TimestampDuration::new(dur_us));
                        }
                    }
                    // PANIC-CENSUS: provably-dead the command word was matched above
                    _ => unreachable!(),
                }
                let now_disabled = transport_stats.disabled_transports();
                out += &format!(
                    "now disabled: {}\n",
                    if now_disabled.is_empty() {
                        "(none)".to_owned()
                    } else {
                        transport_list(&now_disabled)
                    }
                );
                Ok(out)
            }
            _ => {
                apibail_invalid_argument!("debug_transport", "subcommand", subcommand.to_string())
            }
        }
    }

    async fn debug_relay(&self, args: Vec<String>) -> VeilidAPIResult<String> {
        let registry = self.core_context()?.registry();

        let mut relay_nodes_refs: Vec<NodeRef> = vec![];
        let mut routing_domain = RoutingDomain::PublicInternet;
        for n in 0..args.len() {
            let opt_relay_node = async_get_debug_argument_at(
                &args,
                n,
                "debug_relay",
                "node_id",
                resolve_node_ref(
                    registry.clone(),
                    SafetySelection::Unsafe(Sequencing::PreferUnordered),
                ),
            )
            .await
            .ok();
            if let Some(relay_node) = opt_relay_node {
                relay_nodes_refs.push(relay_node);
                continue;
            }

            let opt_routing_domain = get_debug_argument_at(
                &args,
                n,
                "debug_relay",
                "routing_domain",
                get_routing_domain,
            )
            .ok();
            if let Some(rd) = opt_routing_domain {
                routing_domain = rd;
                break;
            }
        }

        let relays: Vec<_> = relay_nodes_refs
            .iter()
            .cloned()
            .map(|nr| RoutingDomainRelay::new(routing_domain, nr, RelayKind::Inbound))
            .collect();

        // Change routing domain relays
        let routing_table = registry.routing_table();
        {
            let rdc = routing_table.get_routing_domain_controller(routing_domain);

            // Create a relay compilation with these relays
            let opt_relay_compilation = if relays.is_empty() {
                None
            } else {
                let rdd = rdc.read_dyn();
                let relay_requirements = rdd.relay_requirements();
                let mut rc = relay_requirements.make_relay_compiler();
                for relay in relays {
                    rc.apply_relay(relay);
                }

                let c = rc.compile();
                if c.is_none() {
                    return Ok("Relays selected do not meet publication requirements".to_owned());
                }
                c
            };

            let mut editor = rdc.edit_dyn();
            editor.set_relay_compilation(opt_relay_compilation);
            editor.commit();
            rdc.publish_peer_info();
        }

        Ok("Relay changed".to_owned())
    }

    async fn debug_nodeinfo(&self) -> VeilidAPIResult<String> {
        // Dump routing table entry
        let registry = self.core_context()?.registry();
        let nodeinfo_rtab = registry.routing_table().debug_info_nodeinfo();
        let nodeinfo_net = registry.network_manager().debug_info_nodeinfo();
        let nodeinfo_rpc = registry.rpc_processor().debug_info_nodeinfo();
        let nodeinfo_crypto = registry.crypto().debug_info_nodeinfo();
        let nodeinfo_attach = registry.attachment_manager().debug_info_nodeinfo();

        // Dump core state
        let state = self.get_state().await?;

        let mut peertable = Vec::new();
        peertable.push(format!(
            "Recent Peers: {} (max {})",
            state.network.peers.len(),
            RECENT_PEERS_TABLE_SIZE
        ));
        for peer in state.network.peers {
            peertable.push(format!(
                "   {} | {} | {} | {} down | {} up",
                peer.node_ids.first().unwrap_or_log(),
                peer.peer_address,
                format_opt_ts(peer.peer_stats.latency.map(|l| l.average)),
                format_opt_bps(Some(peer.peer_stats.transfer.down.average)),
                format_opt_bps(Some(peer.peer_stats.transfer.up.average)),
            ));
        }

        // Dump connection table
        let connman =
            if let Some(connection_manager) = registry.network_manager().opt_connection_manager() {
                connection_manager.debug_print()
            } else {
                "Connection manager unavailable when detached".to_owned()
            };

        Ok(format!(
            "{}\n{}\n{}\n{}\n{}\n{}\n{}\n",
            nodeinfo_rtab,
            nodeinfo_net,
            nodeinfo_rpc,
            peertable.join("\n"),
            connman,
            nodeinfo_crypto,
            nodeinfo_attach
        ))
    }

    fn debug_nodeid(&self) -> VeilidAPIResult<String> {
        // Dump routing table entry
        let registry = self.core_context()?.registry();
        let nodeid = registry.routing_table().debug_info_nodeid();
        Ok(nodeid)
    }

    #[expect(clippy::unused_async)]
    async fn debug_config(
        &self,
        arg_0: Option<String>,
        arg_1: Option<String>,
    ) -> VeilidAPIResult<String> {
        let mut args = arg_0.as_deref().unwrap_or_default();
        let mut config = self.config()?;
        if !args.starts_with("insecure") {
            if let Some(extra) = arg_1 {
                apibail_invalid_argument!("debug_config", "arg_1", extra);
            }
            config = config.safe();
        } else {
            if args != "insecure" {
                apibail_invalid_argument!("debug_config", "arg_0", args);
            }
            args = arg_1.as_deref().unwrap_or_default();
        }
        let args = args.trim_start();

        if args.is_empty() {
            return config.get_key_json("", true);
        }
        config.get_key_json(args, true)
    }

    async fn debug_network(
        &self,
        command: Option<DebugNetworkSubcommand>,
    ) -> VeilidAPIResult<String> {
        let Some(command) = command else {
            apibail_missing_argument!("debug_network", "arg_0");
        };

        match command {
            DebugNetworkSubcommand::Restart => {
                // Must be attached
                if matches!(
                    self.get_state().await?.attachment.state,
                    AttachmentState::Detached
                ) {
                    apibail_internal!("Must be attached to restart network");
                }

                let registry = self.core_context()?.registry();
                registry.network_manager().restart_network();

                Ok("Network restarted".to_owned())
            }
            DebugNetworkSubcommand::Stats => {
                let registry = self.core_context()?.registry();
                let debug_stats = registry.network_manager().debug();

                Ok(debug_stats)
            }
        }
    }

    async fn debug_purge(&self, command: Option<DebugPurgeSubcommand>) -> VeilidAPIResult<String> {
        let registry = self.core_context()?.registry();

        let Some(command) = command else {
            apibail_missing_argument!("debug_purge", "parameter");
        };

        match command {
            DebugPurgeSubcommand::Buckets => {
                // Must be detached
                if !matches!(
                    self.get_state().await?.attachment.state,
                    AttachmentState::Detached | AttachmentState::Detaching
                ) {
                    apibail_internal!("Must be detached to purge");
                }
                match registry.routing_table().purge_buckets().await {
                    Ok(_) => Ok("Buckets purged".to_owned()),
                    Err(e) => Ok(format!("{}", e)),
                }
            }
            DebugPurgeSubcommand::Connections => {
                // Purge connection table
                let opt_connection_manager = registry.network_manager().opt_connection_manager();

                if let Some(connection_manager) = &opt_connection_manager {
                    connection_manager.shutdown().await;
                }

                // Eliminate last_connections from routing table entries
                registry.routing_table().purge_last_connections();

                if let Some(connection_manager) = &opt_connection_manager {
                    connection_manager
                        .startup()
                        .map_err(VeilidAPIError::internal)?;
                }
                Ok("Connections purged".to_owned())
            }
            DebugPurgeSubcommand::Routes => {
                // Purge route spec store
                self.with_debug_cache(|dc| {
                    dc.imported_routes.clear();
                });
                match registry.routing_table().route_spec_store().purge().await {
                    Ok(_) => Ok("Routes purged".to_owned()),
                    Err(e) => Ok(format!("{}", e)),
                }
            }
        }
    }

    async fn debug_attach(&self) -> VeilidAPIResult<String> {
        self.attach().await?;

        Ok("Attached".to_owned())
    }

    async fn debug_detach(&self) -> VeilidAPIResult<String> {
        self.detach().await?;

        Ok("Detached".to_owned())
    }

    fn debug_contact(&self, node_ref: String) -> VeilidAPIResult<String> {
        let registry = self.core_context()?.registry();

        let node_ref = get_debug_argument(
            &node_ref,
            "debug_contact",
            "node_ref",
            get_filtered_node_ref(registry.clone()),
        )?;

        let cm = registry
            .network_manager()
            .get_node_contact_method(node_ref, ContactMethodIntent::Advisory)
            .map_err(VeilidAPIError::internal)?;

        Ok(format!("{:#?}", cm))
    }

    async fn debug_resolve(&self, destination: String) -> VeilidAPIResult<String> {
        let registry = self.core_context()?.registry();
        if !registry.attachment_manager().is_attached() {
            apibail_internal!("Must be attached first");
        };

        let dest = async_get_debug_argument(
            &destination,
            "debug_resolve",
            "destination",
            self.clone().get_destination(registry.clone()),
        )
        .await?;

        let routing_table = registry.routing_table();
        match &dest {
            Destination::Direct {
                node,
                safety_selection: _,
            } => Ok(format!(
                "Destination: {:#?}\nNode:\n{}\n",
                dest,
                routing_table.debug_info_entry(node.unfiltered())
            )),
            Destination::DialInfo {
                dial_info: relay_di,
                node,
            } => Ok(format!(
                "Destination: {:#?}\nNode:\n{}\nDialInfo:\n{}\n",
                dest,
                routing_table.debug_info_entry(node.clone()),
                relay_di,
            )),
            Destination::PrivateRoute {
                private_route: _,
                safety_selection: _,
            } => Ok(format!("Destination: {:#?}", dest)),
        }
    }

    async fn debug_ping(&self, destination: String) -> VeilidAPIResult<String> {
        let registry = self.core_context()?.registry();
        if !registry.attachment_manager().is_attached() {
            apibail_internal!("Must be attached first");
        };

        let dest = async_get_debug_argument(
            &destination,
            "debug_ping",
            "destination",
            self.clone().get_destination(registry.clone()),
        )
        .await?;

        // Send a StatusQ
        let rpc_processor = registry.rpc_processor();
        let result = Box::pin(rpc_processor.rpc_call_status(dest, None))
            .await
            .map_err(VeilidAPIError::internal)?;
        match result {
            StatusResult::Sent(status_answer) => Ok(format!("{:#?}", status_answer.answer)),
            StatusResult::Failed(sdr) => Ok(format!("send failed: {}", sdr)),
            StatusResult::NotSent(nr) => Ok(format!("not sent: {:?}", nr)),
        }
    }

    async fn debug_app_message(
        &self,
        destination: String,
        data_parts: Vec<String>,
    ) -> VeilidAPIResult<String> {
        let registry = self.core_context()?.registry();
        if !registry.attachment_manager().is_attached() {
            apibail_internal!("Must be attached first");
        };
        let dest = async_get_debug_argument(
            &destination,
            "debug_app_message",
            "destination",
            self.clone().get_destination(registry.clone()),
        )
        .await?;

        let data_text = join_args(data_parts);
        let data = get_debug_argument(&data_text, "debug_app_message", "data", get_data)?;
        let data_len = data.len();

        // Send an AppMessage
        let rpc_processor = registry.rpc_processor();

        let out = match rpc_processor
            .rpc_call_app_message(dest, data.into())
            .await
            .map_err(VeilidAPIError::internal)?
        {
            NetworkResult::Value(_) => format!("Sent {} bytes", data_len),
            r => {
                return Ok(r.to_string());
            }
        };

        Ok(out)
    }

    async fn debug_app_call(
        &self,
        destination: String,
        data_parts: Vec<String>,
    ) -> VeilidAPIResult<String> {
        let registry = self.core_context()?.registry();
        if !registry.attachment_manager().is_attached() {
            apibail_internal!("Must be attached first");
        };
        let dest = async_get_debug_argument(
            &destination,
            "debug_app_call",
            "destination",
            self.clone().get_destination(registry.clone()),
        )
        .await?;

        let data_text = join_args(data_parts);
        let data = get_debug_argument(&data_text, "debug_app_call", "data", get_data)?;
        let data_len = data.len();

        // Send an AppCall
        let rpc_processor = registry.rpc_processor();

        let out = match Box::pin(rpc_processor.rpc_call_app_call(dest, data.into()))
            .await
            .map_err(VeilidAPIError::internal)?
        {
            NetworkResult::Value(v) => format!(
                "Sent {} bytes, received: {}",
                data_len,
                human_byte_data(&v.answer, Some(512))
            ),
            r => {
                return Ok(r.to_string());
            }
        };

        Ok(out)
    }

    async fn debug_app_reply(
        &self,
        id: Option<String>,
        data_parts: Vec<String>,
    ) -> VeilidAPIResult<String> {
        let registry = self.core_context()?.registry();
        if !registry.attachment_manager().is_attached() {
            apibail_internal!("Must be attached first");
        };

        let (call_id, data) = if let Some(id) = id {
            let stripped_id = id.strip_prefix('#').unwrap_or(id.as_str());
            let call_id = OperationId::new(
                u64::from_str_radix(stripped_id, 16)
                    .map_err(|e| VeilidAPIError::parse_error(e.to_string(), stripped_id))?,
            );
            let data_text = join_args(data_parts);
            let data = get_debug_argument(&data_text, "debug_app_reply", "data", get_data)?;
            (call_id, data)
        } else {
            let rpc_processor = registry.rpc_processor();

            let call_id = rpc_processor
                .get_app_call_ids()
                .first()
                .cloned()
                .ok_or_else(|| VeilidAPIError::generic("no app calls waiting"))?;
            let data_text = join_args(data_parts);
            let data = get_debug_argument(&data_text, "debug_app_reply", "data", get_data)?;
            (call_id, data)
        };

        let data_len = data.len();

        // Send a AppCall Reply
        self.app_call_reply(call_id, data)
            .await
            .map_err(VeilidAPIError::internal)?;

        Ok(format!("Replied with {} bytes", data_len))
    }

    async fn debug_route_allocate(&self, args: Vec<String>) -> VeilidAPIResult<String> {
        // [uno|ord|ord!] [rel] [<count>] [in|out] [avoid_node_id]

        let registry = self.core_context()?.registry();
        let routing_table = registry.routing_table();
        let rss = routing_table.route_spec_store();
        let default_route_hop_count = self.config()?.network.rpc.default_route_hop_count as usize;

        let mut ai = 0;
        let mut sequencing = Sequencing::default();
        let mut stability = Stability::default();
        let mut hop_count = default_route_hop_count;
        let mut directions = DirectionSet::all();
        let mut avoid_nodes = Vec::new();

        while ai < args.len() {
            if let Ok(seq) =
                get_debug_argument_at(&args, ai, "debug_route", "sequencing", get_sequencing)
            {
                sequencing = seq;
            } else if let Ok(sta) =
                get_debug_argument_at(&args, ai, "debug_route", "stability", get_stability)
            {
                stability = sta;
            } else if let Ok(hc) =
                get_debug_argument_at(&args, ai, "debug_route", "hop_count", get_number)
            {
                hop_count = hc;
            } else if let Ok(ds) =
                get_debug_argument_at(&args, ai, "debug_route", "direction_set", get_direction_set)
            {
                directions = ds;
            } else if let Ok(node_id) =
                get_debug_argument_at(&args, ai, "debug_route", "avoid_node_id", get_node_id)
            {
                avoid_nodes.push(node_id);
            } else {
                return Ok(format!("Invalid argument specified: {}", args[ai]));
            }
            ai += 1;
        }

        // Allocate route
        let allocate_route_params = AllocateRouteParams {
            crypto_kinds: VALID_CRYPTO_KINDS.to_vec(),
            hop_count,
            stability,
            sequencing,
            directions,
            avoid_nodes,
            automatic: false,
        };
        let out = match rss.allocate_route(allocate_route_params).await {
            Ok(v) => v.route_id.to_string(),
            Err(e) => format!("Route allocation failed: {}", e),
        };

        Ok(out)
    }
    fn debug_route_release(&self, args: Vec<String>) -> VeilidAPIResult<String> {
        // <route id>
        let registry = self.core_context()?.registry();
        let routing_table = registry.routing_table();
        let rss = routing_table.route_spec_store();

        let route_id = get_debug_argument_at(
            &args,
            0,
            "debug_route",
            "route_id",
            get_route_id(registry.clone(), true, true),
        )?;

        // Release route
        let out = match rss.release_route(route_id.clone()) {
            true => {
                // release imported
                self.with_debug_cache(|dc| {
                    for (n, ir) in dc.imported_routes.iter().enumerate() {
                        if *ir == route_id {
                            let _ = dc.imported_routes.remove(n);
                            break;
                        }
                    }
                });
                "Released".to_owned()
            }
            false => "Route does not exist".to_owned(),
        };

        Ok(out)
    }
    async fn debug_route_publish(&self, args: Vec<String>) -> VeilidAPIResult<String> {
        // <route id> [full]
        let registry = self.core_context()?.registry();
        let routing_table = registry.routing_table();
        let rss = routing_table.route_spec_store();

        let route_id = get_debug_argument_at(
            &args,
            0,
            "debug_route",
            "route_id",
            get_route_id(registry.clone(), true, false),
        )?;
        let full = {
            if args.len() > 1 {
                let full_val = get_debug_argument_at(&args, 1, "debug_route", "full", get_string)?
                    .to_ascii_lowercase();
                if full_val == "full" {
                    true
                } else {
                    apibail_invalid_argument!("debug_route", "full", full_val);
                }
            } else {
                false
            }
        };

        // Publish route
        let arsid = AllocatedRouteSetId::from_route_id(route_id.clone());
        let out = match rss.assemble_private_route_set(&arsid, Some(!full)).await {
            Ok(private_routes) => {
                if let Err(e) = rss.mark_route_published(&arsid, true) {
                    return Ok(format!("Couldn't mark route published: {}", e));
                }
                // Convert to blob
                let blob_data = RouteSpecStore::private_routes_to_blob(&private_routes)
                    .map_err(VeilidAPIError::internal)?;
                let out = BASE64URL_NOPAD.encode(&blob_data);
                veilid_log!(registry info
                    "Published route {} as {} bytes:\n{}",
                    route_id,
                    blob_data.len(),
                    out
                );
                format!("Published route {}", route_id)
            }
            Err(e) => {
                format!("Couldn't assemble private route: {}", e)
            }
        };

        Ok(out)
    }
    fn debug_route_unpublish(&self, args: Vec<String>) -> VeilidAPIResult<String> {
        // <route id>
        let registry = self.core_context()?.registry();
        let routing_table = registry.routing_table();
        let rss = routing_table.route_spec_store();

        let route_id = get_debug_argument_at(
            &args,
            0,
            "debug_route",
            "route_id",
            get_route_id(registry.clone(), true, false),
        )?;

        // Unpublish route
        let arsid = AllocatedRouteSetId::from_route_id(route_id);
        let out = if let Err(e) = rss.mark_route_published(&arsid, false) {
            return Ok(format!("Couldn't mark route unpublished: {}", e));
        } else {
            "Route unpublished".to_owned()
        };
        Ok(out)
    }
    fn debug_route_print(&self, args: Vec<String>) -> VeilidAPIResult<String> {
        // <route id>
        let registry = self.core_context()?.registry();
        let routing_table = registry.routing_table();
        let rss = routing_table.route_spec_store();

        if let Ok(route_id) = get_debug_argument_at(
            &args,
            0,
            "debug_route",
            "route_id",
            get_route_id(registry.clone(), true, true),
        ) {
            // Content (persisted spec) view plus the cache view with live route stats
            Ok(format!(
                "{}\n{}",
                rss.debug_route_by_id(&route_id),
                rss.display_route_by_id(&route_id)
            ))
        } else if let Ok(route_key) = get_debug_argument_at(
            &args,
            0,
            "debug_route",
            "route_key",
            get_route_key(registry.clone(), true, true),
        ) {
            Ok(rss.debug_route_by_key(&route_key))
        } else {
            Ok("Route key not found".to_string())
        }
    }
    fn debug_route_list(&self, _args: Vec<String>) -> VeilidAPIResult<String> {
        //
        let registry = self.core_context()?.registry();
        let routing_table = registry.routing_table();
        let rss = routing_table.route_spec_store();

        let routes = rss.list_allocated_routes(|k, v| Some((k.clone(), format!("{:#}", v))));
        let mut out = format!("Allocated Routes: (count = {}):\n", routes.len());
        for (r, s) in routes {
            out.push_str(&format!("{}: {}\n", r, s));
        }

        let remote_routes = rss.list_remote_routes(|k, v| Some((k.clone(), format!("{:#}", v))));
        out.push_str(&format!(
            "Remote Routes: (count = {}):\n",
            remote_routes.len()
        ));
        for (r, s) in remote_routes {
            out.push_str(&format!("{}: {:#}\n", r, s));
        }

        Ok(out)
    }
    fn debug_route_import(&self, args: Vec<String>) -> VeilidAPIResult<String> {
        // <blob>
        let registry = self.core_context()?.registry();
        let routing_table = registry.routing_table();
        let rss = routing_table.route_spec_store();

        let blob = get_debug_argument_at(&args, 0, "debug_route", "blob", get_string)?;
        let blob_dec = BASE64URL_NOPAD
            .decode(blob.as_bytes())
            .map_err(|e| VeilidAPIError::parse_error(e.to_string(), blob))?;

        let route_id = rss.import_remote_route_blob(blob_dec)?;

        let out = self.with_debug_cache(|dc| {
            let n = dc.imported_routes.len();
            let out = format!("Private route #{} imported: {}", n, route_id);
            dc.imported_routes.push(route_id);
            out
        });

        Ok(out)
    }

    async fn debug_route_test(&self, args: Vec<String>) -> VeilidAPIResult<String> {
        // <route id>
        let registry = self.core_context()?.registry();
        let routing_table = registry.routing_table();
        let rss = routing_table.route_spec_store();

        let route_id = get_debug_argument_at(
            &args,
            0,
            "debug_route",
            "route_id",
            get_route_id(registry.clone(), true, true),
        )?;

        let success = rss.test_route(route_id).await?;

        let out = match success {
            Some(true) => "SUCCESS".to_owned(),
            Some(false) => "FAILED".to_owned(),
            None => "UNTESTED".to_owned(),
        };

        Ok(out)
    }

    async fn debug_route(&self, command: Option<DebugRouteSubcommand>) -> VeilidAPIResult<String> {
        let Some(command) = command else {
            apibail_missing_argument!("debug_route", "command");
        };

        match command {
            DebugRouteSubcommand::Allocate { params } => self.debug_route_allocate(params).await,
            DebugRouteSubcommand::Release { route_id } => self.debug_route_release(vec![route_id]),
            DebugRouteSubcommand::Publish { route_id, full } => {
                let mut args = vec![route_id];
                if let Some(full) = full {
                    args.push(full);
                }
                self.debug_route_publish(args).await
            }
            DebugRouteSubcommand::Unpublish { route_id } => {
                self.debug_route_unpublish(vec![route_id])
            }
            DebugRouteSubcommand::Print { route } => self.debug_route_print(vec![route]),
            DebugRouteSubcommand::List => self.debug_route_list(vec![]),
            DebugRouteSubcommand::Import { blob } => self.debug_route_import(vec![blob]),
            DebugRouteSubcommand::Test { route_id } => self.debug_route_test(vec![route_id]).await,
            DebugRouteSubcommand::Diag { route_id } => self.debug_route_diag(vec![route_id]),
            DebugRouteSubcommand::Reallocate => self.debug_route_reallocate().await,
        }
    }

    /// Drop every automatic unpublished route and refill the pool immediately, so a run on a
    /// warmed-up node can be compared against a cold start
    async fn debug_route_reallocate(&self) -> VeilidAPIResult<String> {
        let registry = self.core_context()?.registry();
        let routing_table = registry.routing_table();
        let (released, allocated) = routing_table
            .get_specific_routing_domain_controller::<PublicInternetRoutingDomainController>()
            .reallocate_safety_routes()
            .await
            .map_err(VeilidAPIError::internal)?;
        Ok(format!("released {}, allocated {}", released, allocated))
    }

    /// Per-leg contact methods for an allocated route, computed from the peer info we hold
    fn debug_route_diag(&self, args: Vec<String>) -> VeilidAPIResult<String> {
        // <route id>
        let registry = self.core_context()?.registry();
        let routing_table = registry.routing_table();
        let rss = routing_table.route_spec_store();

        let route_id = get_debug_argument_at(
            &args,
            0,
            "debug_route",
            "route_id",
            get_route_id(registry.clone(), true, false),
        )?;
        let Some(info) = rss
            .get_allocated_route_test_info(&AllocatedRouteSetId::from_route_id(route_id.clone()))
        else {
            return Ok("Route not found".to_owned());
        };
        let Some(our_pi) = routing_table.get_published_peer_info(RoutingDomain::PublicInternet)
        else {
            return Ok("No published PublicInternet peer info".to_owned());
        };

        let mut out = format!(
            "Route {} hops={} orderings={:?}\n",
            route_id,
            info.hops.len(),
            info.orderings
        );

        // Legs run self -> hop1 -> ... -> hopN and back
        let mut legs: Vec<(String, Arc<PeerInfo>)> = vec![("self".to_owned(), our_pi)];
        for (i, hop) in info.hops.iter().enumerate() {
            let name = format!("hop{}", i + 1);
            let Some(pi) = hop.get_peer_info(RoutingDomain::PublicInternet) else {
                out.push_str(&format!(
                    "  {}: {} has no PublicInternet peer info\n",
                    name, hop
                ));
                return Ok(out);
            };
            let dial_info = pi
                .node_info()
                .dial_info_detail_list()
                .iter()
                .map(|did| did.to_string())
                .collect::<Vec<_>>()
                .join(", ");
            let relays = pi
                .node_info()
                .relay_info_list()
                .iter()
                .map(|ri| format!("{}:{:?}", ri.node_ids(), ri.relay_kind()))
                .collect::<Vec<_>>()
                .join(", ");
            out.push_str(&format!(
                "  {}: {} dialinfo=[{}] relays=[{}]\n",
                name,
                hop.best_node_id(),
                dial_info,
                relays
            ));
            legs.push((name, pi));
        }

        let probes = [
            ("ENO", Sequencing::EnsureOrdered, DialInfoFilter::all()),
            ("PRU", Sequencing::PreferUnordered, DialInfoFilter::all()),
            (
                "PRU/udp",
                Sequencing::PreferUnordered,
                DialInfoFilter::ensure_ordering(SequenceOrdering::Unordered),
            ),
        ];
        let mut describe_leg = |a: &(String, Arc<PeerInfo>), b: &(String, Arc<PeerInfo>)| {
            let cms = probes
                .iter()
                .map(|(label, sequencing, dial_info_filter)| {
                    let cm = routing_table.get_best_contact_method(
                        RoutingDomain::PublicInternet,
                        ContactMethodRequest {
                            peer_a: a.1.clone(),
                            peer_a_published: true,
                            peer_b: b.1.clone(),
                            dial_info_filter: *dial_info_filter,
                            sequencing: *sequencing,
                        },
                    );
                    format!(
                        "{}={}",
                        label,
                        cm.map(|cm| cm.to_string())
                            .unwrap_or_else(|| "none".to_owned())
                    )
                })
                .collect::<Vec<_>>()
                .join(" ");
            out.push_str(&format!("  {}->{}: {}\n", a.0, b.0, cms));
        };
        for pair in legs.windows(2) {
            describe_leg(&pair[0], &pair[1]);
        }
        for pair in legs.windows(2).rev() {
            describe_leg(&pair[1], &pair[0]);
        }

        Ok(out)
    }

    fn debug_record_list(&self, args: Vec<String>) -> VeilidAPIResult<String> {
        // <local|remote>
        let registry = self.core_context()?.registry();
        let storage_manager = registry.storage_manager();

        let scope = get_debug_argument_at(&args, 0, "debug_record_list", "scope", get_string)?;
        let out = match scope.as_str() {
            "local" => {
                let mut out = "Local Records:\n".to_string();
                out += &storage_manager.debug_local_records();
                out
            }
            "remote" => {
                let mut out = "Remote Records:\n".to_string();
                out += &storage_manager.debug_remote_records();
                out
            }
            "opened" => {
                let mut out = "Opened Records:\n".to_string();
                out += &storage_manager.debug_opened_records();
                out
            }
            "watched" => {
                let mut out = "Watched Records:\n".to_string();
                out += &storage_manager.debug_watched_records();
                out
            }
            "offline" => {
                let mut out = "Offline Records:\n".to_string();
                out += &storage_manager.debug_offline_records();
                out
            }
            "transactions" => {
                let mut out = "Record Transaction:\n".to_string();
                out += &storage_manager.debug_transactions();
                out
            }
            _ => "Invalid scope\n".to_owned(),
        };
        Ok(out)
    }

    async fn debug_record_purge_bytes(
        &self,
        scope: String,
        bytes: Option<u64>,
    ) -> VeilidAPIResult<String> {
        // <local|remote> [bytes]
        let registry = self.core_context()?.registry();
        let storage_manager = registry.storage_manager();

        self.with_debug_cache(|dc| {
            dc.opened_record_contexts.clear();
        });
        storage_manager.close_all_records().await?;

        let out = match scope.as_str() {
            "local" => storage_manager.purge_local_records_by_bytes(bytes).await,
            "remote" => storage_manager.purge_remote_records_by_bytes(bytes).await,
            _ => "Invalid scope\n".to_owned(),
        };
        Ok(out)
    }

    async fn debug_record_purge_keys(
        &self,
        scope: String,
        keys: Vec<String>,
    ) -> VeilidAPIResult<String> {
        // <local|remote> [bytes]
        let registry = self.core_context()?.registry();
        let storage_manager = registry.storage_manager();

        self.with_debug_cache(|dc| {
            dc.opened_record_contexts.clear();
        });
        storage_manager.close_all_records().await?;

        let keys = keys
            .iter()
            .map(|k| {
                RecordKey::from_str(k)
                    .map(|rk| rk.opaque())
                    .or_else(|_| OpaqueRecordKey::from_str(k))
            })
            .collect::<Result<Vec<OpaqueRecordKey>, VeilidAPIError>>()?;

        let out = match scope.as_str() {
            "local" => storage_manager.purge_local_records_by_keys(keys).await,
            "remote" => storage_manager.purge_remote_records_by_keys(keys).await,
            _ => "Invalid scope\n".to_owned(),
        };
        Ok(out)
    }

    async fn debug_record_create(&self, args: Vec<String>) -> VeilidAPIResult<String> {
        let crypto = self.crypto()?;

        let schema = get_debug_argument_at(
            &args,
            0,
            "debug_record_create",
            "dht_schema",
            get_dht_schema,
        )
        .unwrap_or_else(|_| Ok(DHTSchema::default()))?;

        let csv = get_debug_argument_at(
            &args,
            1,
            "debug_record_create",
            "kind",
            get_crypto_system_version(&crypto),
        )
        .unwrap_or_else(|_| crypto.best());

        let ss = get_debug_argument_at(
            &args,
            2,
            "debug_record_create",
            "safety_selection",
            get_safety_selection(self.core_context()?.registry()),
        )
        .ok();

        // Get routing context with optional safety
        let rc = self.routing_context()?;
        let rc = if let Some(ss) = ss {
            match rc.with_safety(ss) {
                Err(e) => return Ok(format!("Can't use safety selection: {}", e)),
                Ok(v) => v,
            }
        } else {
            rc
        };

        // Do a record create
        let record = match rc.create_dht_record(csv.kind(), schema, None).await {
            Err(e) => return Ok(format!("Can't open DHT record: {}", e)),
            Ok(v) => v,
        };

        // Save routing context for record
        self.with_debug_cache(|dc| {
            dc.opened_record_contexts.insert(record.key(), rc);
        });

        Ok(format!(
            "Created: {} {}\n{:?}",
            record.key(),
            record.owner_keypair().unwrap_or_log(),
            record
        ))
    }

    async fn debug_record_open(&self, args: Vec<String>) -> VeilidAPIResult<String> {
        let registry = self.core_context()?.registry();

        let (key, ss) = get_debug_argument_at(
            &args,
            0,
            "debug_record_open",
            "key",
            get_dht_key(registry.clone()),
        )?;
        let writer =
            get_debug_argument_at(&args, 1, "debug_record_open", "writer", get_keypair).ok();

        // Get routing context with optional safety
        let rc = self.routing_context()?;
        let rc = if let Some(ss) = ss {
            match rc.with_safety(ss) {
                Err(e) => return Ok(format!("Can't use safety selection: {}", e)),
                Ok(v) => v,
            }
        } else {
            rc
        };

        // Do a record open
        let record = match rc.open_dht_record(key.clone(), writer, None).await {
            Err(e) => return Ok(format!("Can't open DHT record: {}", e)),
            Ok(v) => v,
        };

        // Save routing context for record
        self.with_debug_cache(|dc| {
            dc.opened_record_contexts.insert(record.key(), rc);
        });

        Ok(format!("Opened: {}\n{:#?}", key, record))
    }

    async fn debug_record_close(&self, args: Vec<String>) -> VeilidAPIResult<String> {
        let (key, rc) =
            self.clone()
                .get_opened_dht_record_context(&args, "debug_record_close", "key", 0)?;

        // Do a record close
        if let Err(e) = rc.close_dht_record(key.clone()).await {
            return Ok(format!("Can't close DHT record: {}", e));
        };

        Ok(format!("Closed: {:?}", key))
    }

    async fn debug_record_set(&self, args: Vec<String>) -> VeilidAPIResult<String> {
        let mut opt_arg_add = if !args.is_empty() && get_dht_key_no_safety(&args[0]).is_some() {
            1
        } else {
            0
        };
        let (key, rc) =
            self.clone()
                .get_opened_dht_record_context(&args, "debug_record_set", "key", 0)?;
        let subkey = get_debug_argument_at(
            &args,
            opt_arg_add,
            "debug_record_set",
            "subkey",
            get_number::<u32>,
        )?;
        let data =
            get_debug_argument_at(&args, 1 + opt_arg_add, "debug_record_set", "data", get_data)?;
        let writer = match get_debug_argument_at(
            &args,
            2 + opt_arg_add,
            "debug_record_set",
            "writer",
            get_keypair,
        ) {
            Ok(v) => {
                opt_arg_add += 1;
                Some(v)
            }
            Err(_) => None,
        };
        let allow_offline = match get_debug_argument_at(
            &args,
            2 + opt_arg_add,
            "debug_record_set",
            "allow_offline",
            get_allow_offline,
        ) {
            Ok(v) => {
                opt_arg_add += 1;
                Some(v)
            }
            Err(_) => None,
        };
        let min_seqnum = match get_debug_argument_at(
            &args,
            2 + opt_arg_add,
            "debug_record_set",
            "min_seqnum",
            get_string,
        ) {
            Ok(v) => match parse_value_seq_num(&v) {
                Ok(seq) => Some(seq),
                Err(e) => return Ok(e),
            },
            Err(_) => None,
        };

        // Do a record set
        let value = match rc
            .set_dht_value(
                key,
                subkey as ValueSubkey,
                data,
                Some(SetDHTValueOptions {
                    writer,
                    allow_offline,
                    min_seqnum,
                }),
            )
            .await
        {
            Err(e) => {
                return Ok(format!("Can't set DHT value: {}", e));
            }
            Ok(v) => v,
        };
        let out = if let Some(value) = value {
            format!("Newer value found: {:?}", value)
        } else {
            "Success".to_owned()
        };
        Ok(out)
    }

    async fn debug_record_get(&self, args: Vec<String>) -> VeilidAPIResult<String> {
        let opt_arg_add = if !args.is_empty() && get_dht_key_no_safety(&args[0]).is_some() {
            1
        } else {
            0
        };

        let (key, rc) =
            self.clone()
                .get_opened_dht_record_context(&args, "debug_record_get", "key", 0)?;
        let subkey = get_debug_argument_at(
            &args,
            opt_arg_add,
            "debug_record_get",
            "subkey",
            get_number::<u32>,
        )?;
        let force_refresh = if args.len() > 1 + opt_arg_add {
            Some(get_debug_argument_at(
                &args,
                1 + opt_arg_add,
                "debug_record_get",
                "force_refresh",
                get_string,
            )?)
        } else {
            None
        };

        let force_refresh = if let Some(force_refresh) = force_refresh {
            if &force_refresh == "force" {
                true
            } else {
                return Ok(format!("Unknown force: {}", force_refresh));
            }
        } else {
            false
        };

        // Do a record get
        let value = match rc
            .get_dht_value(key, subkey as ValueSubkey, force_refresh)
            .await
        {
            Err(e) => {
                return Ok(format!("Can't get DHT value: {}", e));
            }
            Ok(v) => v,
        };
        let out = if let Some(value) = value {
            format!("{:?}", value)
        } else {
            "No value data returned".to_owned()
        };
        Ok(out)
    }

    async fn debug_record_delete(&self, args: Vec<String>) -> VeilidAPIResult<String> {
        let key = get_debug_argument_at(
            &args,
            0,
            "debug_record_delete",
            "key",
            get_dht_key_no_safety,
        )?;

        // Do a record delete (can use any routing context here)
        let rc = self.routing_context()?;
        match rc.delete_dht_record(key).await {
            Err(e) => return Ok(format!("Can't delete DHT record: {}", e)),
            Ok(v) => v,
        };
        Ok("DHT record deleted".to_string())
    }

    async fn debug_record_info(&self, args: Vec<String>) -> VeilidAPIResult<String> {
        let registry = self.core_context()?.registry();
        let storage_manager = registry.storage_manager();

        let key =
            get_debug_argument_at(&args, 0, "debug_record_info", "key", get_dht_key_no_safety)?;

        let subkey = get_debug_argument_at(
            &args,
            1,
            "debug_record_info",
            "subkey",
            get_number::<ValueSubkey>,
        )
        .ok();

        let out = if let Some(subkey) = subkey {
            let li = storage_manager
                .debug_local_record_subkey_info(key.clone(), subkey)
                .await;
            let ri = storage_manager
                .debug_remote_record_subkey_info(key.clone(), subkey)
                .await;
            format!(
                "Local Subkey Info:\n{}\n\nRemote Subkey Info:\n{}\n",
                li, ri
            )
        } else {
            let li = storage_manager.debug_local_record_info(key.clone());
            let ri = storage_manager.debug_remote_record_info(key.clone());
            format!("Local Info:\n{}\n\nRemote Info:\n{}\n", li, ri)
        };
        Ok(out)
    }

    async fn debug_record_watch(&self, args: Vec<String>) -> VeilidAPIResult<String> {
        let opt_arg_add = if !args.is_empty() && get_dht_key_no_safety(&args[0]).is_some() {
            1
        } else {
            0
        };

        let (key, rc) =
            self.clone()
                .get_opened_dht_record_context(&args, "debug_record_watch", "key", 0)?;

        let mut rest_defaults = false;
        let subkeys = get_debug_argument_at(
            &args,
            opt_arg_add,
            "debug_record_watch",
            "subkeys",
            get_subkeys,
        )
        .ok()
        .map(Some)
        .unwrap_or_else(|| {
            rest_defaults = true;
            None
        });

        let opt_expiration = if rest_defaults {
            None
        } else {
            get_debug_argument_at(
                &args,
                1 + opt_arg_add,
                "debug_record_watch",
                "expiration",
                parse_duration,
            )
            .ok()
            .map(|dur| {
                if dur == 0 {
                    None
                } else {
                    Some(Timestamp::now_non_decreasing().later(TimestampDuration::new(dur)))
                }
            })
            .unwrap_or_else(|| {
                rest_defaults = true;
                None
            })
        };
        let count = if rest_defaults {
            None
        } else {
            get_debug_argument_at(
                &args,
                2 + opt_arg_add,
                "debug_record_watch",
                "count",
                get_number,
            )
            .ok()
            .map(Some)
            .unwrap_or_else(|| {
                rest_defaults = true;
                Some(u32::MAX)
            })
        };

        // Do a record watch
        let active = match rc
            .watch_dht_values(key, subkeys, opt_expiration, count)
            .await
        {
            Err(e) => {
                return Ok(format!("Can't watch DHT value: {}", e));
            }
            Ok(v) => v,
        };
        if !active {
            return Ok("Failed to watch value".to_owned());
        }
        Ok("Success".to_owned())
    }

    async fn debug_record_cancel(&self, args: Vec<String>) -> VeilidAPIResult<String> {
        let opt_arg_add = if !args.is_empty() && get_dht_key_no_safety(&args[0]).is_some() {
            1
        } else {
            0
        };

        let (key, rc) =
            self.clone()
                .get_opened_dht_record_context(&args, "debug_record_watch", "key", 0)?;
        let subkeys = get_debug_argument_at(
            &args,
            opt_arg_add,
            "debug_record_watch",
            "subkeys",
            get_subkeys,
        )
        .ok();

        // Do a record watch cancel
        let still_active = match rc.cancel_dht_watch(key, subkeys).await {
            Err(e) => {
                return Ok(format!("Can't cancel DHT watch: {}", e));
            }
            Ok(v) => v,
        };

        Ok(if still_active {
            "Watch partially cancelled".to_owned()
        } else {
            "Watch cancelled".to_owned()
        })
    }

    async fn debug_record_inspect(&self, args: Vec<String>) -> VeilidAPIResult<String> {
        let opt_arg_add = if !args.is_empty() && get_dht_key_no_safety(&args[0]).is_some() {
            1
        } else {
            0
        };

        let (key, rc) =
            self.clone()
                .get_opened_dht_record_context(&args, "debug_record_inspect", "key", 0)?;

        let mut rest_defaults = false;

        let scope = if rest_defaults {
            Default::default()
        } else {
            get_debug_argument_at(
                &args,
                opt_arg_add,
                "debug_record_inspect",
                "scope",
                get_dht_report_scope,
            )
            .ok()
            .unwrap_or_else(|| {
                rest_defaults = true;
                Default::default()
            })
        };

        let subkeys = if rest_defaults {
            None
        } else {
            get_debug_argument_at(
                &args,
                1 + opt_arg_add,
                "debug_record_inspect",
                "subkeys",
                get_subkeys,
            )
            .ok()
        };

        // Do a record inspect
        let report = match rc.inspect_dht_record(key, subkeys, scope).await {
            Err(e) => {
                return Ok(format!("Can't inspect DHT record: {}", e));
            }
            Ok(v) => v,
        };

        Ok(format!("Success: report={:?}", report))
    }

    fn debug_record_rehydrate(&self, args: Vec<String>) -> VeilidAPIResult<String> {
        let registry = self.core_context()?.registry();
        let storage_manager = registry.storage_manager();

        let key = get_debug_argument_at(
            &args,
            0,
            "debug_record_rehydrate",
            "key",
            get_dht_key_no_safety,
        )?;

        let mut rest_defaults = false;

        let subkeys = if rest_defaults {
            None
        } else {
            get_debug_argument_at(&args, 1, "debug_record_rehydrate", "subkeys", get_subkeys)
                .inspect_err(|_| {
                    rest_defaults = true;
                })
                .ok()
        };

        let consensus_count = if rest_defaults {
            None
        } else {
            get_debug_argument_at(
                &args,
                2,
                "debug_record_rehydrate",
                "consensus_count",
                get_number,
            )
            .inspect_err(|_| {
                rest_defaults = true;
            })
            .ok()
        };

        // Do a record rehydrate
        storage_manager.add_rehydration_request(
            key.opaque(),
            subkeys.unwrap_or_default(),
            consensus_count.unwrap_or_else(|| {
                registry.config().internal().network.dht.set_value_count as usize
            }),
        );

        Ok("Request added".to_owned())
    }

    async fn debug_record(
        &self,
        command: Option<DebugRecordSubcommand>,
    ) -> VeilidAPIResult<String> {
        let Some(command) = command else {
            apibail_missing_argument!("debug_record", "command");
        };

        match command {
            DebugRecordSubcommand::List { scope } => self.debug_record_list(vec![scope]),
            DebugRecordSubcommand::Purge { scope, bytes, keys } => {
                if let Some(keys) = keys {
                    self.debug_record_purge_keys(scope, keys).await
                } else {
                    self.debug_record_purge_bytes(scope, bytes).await
                }
            }
            DebugRecordSubcommand::Create {
                dht_schema,
                crypto_kind,
                safety,
            } => {
                let mut args = Vec::new();
                if let Some(dht_schema) = dht_schema {
                    args.push(dht_schema);
                }
                if let Some(crypto_kind) = crypto_kind {
                    args.push(crypto_kind);
                }
                if let Some(safety) = safety {
                    args.push(safety);
                }
                self.debug_record_create(args).await
            }
            DebugRecordSubcommand::Open { key, writer } => {
                let mut args = vec![key];
                if let Some(writer) = writer {
                    args.push(writer);
                }
                self.debug_record_open(args).await
            }
            DebugRecordSubcommand::Close { key } => {
                let mut args = Vec::new();
                if let Some(key) = key {
                    args.push(key);
                }
                self.debug_record_close(args).await
            }
            DebugRecordSubcommand::Get { params } => self.debug_record_get(params).await,
            DebugRecordSubcommand::Set { params } => self.debug_record_set(params).await,
            DebugRecordSubcommand::Delete { key } => self.debug_record_delete(vec![key]).await,
            DebugRecordSubcommand::Info { key, subkey } => {
                let mut args = vec![key];
                if let Some(subkey) = subkey {
                    args.push(subkey.to_string());
                }
                self.debug_record_info(args).await
            }
            DebugRecordSubcommand::Watch {
                key,
                subkeys,
                expiration,
                count,
            } => {
                let mut args = Vec::new();
                if let Some(key) = key {
                    args.push(key);
                }
                if let Some(subkeys) = subkeys {
                    args.push(subkeys);
                }
                if let Some(expiration) = expiration {
                    args.push(expiration);
                }
                if let Some(count) = count {
                    args.push(count.to_string());
                }
                self.debug_record_watch(args).await
            }
            DebugRecordSubcommand::Cancel { key, subkeys } => {
                let mut args = Vec::new();
                if let Some(key) = key {
                    args.push(key);
                }
                if let Some(subkeys) = subkeys {
                    args.push(subkeys);
                }
                self.debug_record_cancel(args).await
            }
            DebugRecordSubcommand::Inspect {
                key,
                scope,
                subkeys,
            } => {
                let mut args = Vec::new();
                if let Some(key) = key {
                    args.push(key);
                }
                if let Some(scope) = scope {
                    args.push(scope);
                }
                if let Some(subkeys) = subkeys {
                    args.push(subkeys);
                }
                self.debug_record_inspect(args).await
            }
            DebugRecordSubcommand::Rehydrate {
                key,
                subkeys,
                consensus_count,
            } => {
                let mut args = vec![key];
                if let Some(subkeys) = subkeys {
                    args.push(subkeys);
                }
                if let Some(consensus_count) = consensus_count {
                    args.push(consensus_count.to_string());
                }
                self.debug_record_rehydrate(args)
            }
        }
    }

    fn debug_table_list(&self) -> VeilidAPIResult<String> {
        //
        let table_store = self.table_store()?;
        let table_names = table_store.list_all();
        let out = format!(
            "TableStore tables:\n{}",
            table_names
                .iter()
                .map(|(k, v)| format!("{} ({})", k, v))
                .collect::<Vec<String>>()
                .join("\n")
        );
        Ok(out)
    }

    fn _format_columns(columns: &[table_store::ColumnInfo]) -> String {
        let mut out = String::new();
        for (n, col) in columns.iter().enumerate() {
            //
            out += &format!("Column {}:\n", n);
            out += &format!("  Key Count: {}\n", col.key_count);
        }
        out
    }

    async fn debug_table_info(&self, table_name: String) -> VeilidAPIResult<String> {
        //
        let table_store = self.table_store()?;

        let Some(info) = table_store.info(&table_name).await? else {
            return Ok(format!("Table '{}' does not exist", table_name));
        };

        fn debug_print_io_stats(stats: &table_store::IOStatsInfo) -> String {
            let mut info_str = format!(
                "Started:          {}\n\
                Span:             {}\n\
                Transactions:     {:6}\n\
                Reads:            {:6}\n\
                Cache reads:      {:6}\n\
                Writes:           {:6}\n\
                Bytes read:       {:>6}\n\
                Cache bytes read: {:>6}\n\
                Bytes written:    {:>6}\n\
                Deletes:          {:6}\n\
                Prefix deletes:   {:6}\n",
                stats.started,
                stats.span,
                stats.transactions,
                stats.reads,
                stats.cache_reads,
                stats.writes,
                bytesize::ByteSize(stats.bytes_read.as_u64())
                    .display()
                    .iec_short()
                    .to_string(),
                bytesize::ByteSize(stats.cache_read_bytes.as_u64())
                    .display()
                    .iec_short()
                    .to_string(),
                bytesize::ByteSize(stats.bytes_written.as_u64())
                    .display()
                    .iec_short()
                    .to_string(),
                stats.deletes,
                stats.prefix_deletes,
            );

            if !stats.write_size_buckets.is_empty() {
                info_str += "Write size buckets:\n";
                for (size, count) in stats.write_size_buckets.iter() {
                    info_str += &format!("  {size:6}: {count:6}\n");
                }
            }

            if !stats.tx_write_size_buckets.is_empty() {
                info_str += "Transaction write size buckets:\n";
                for (size, (count, avg_duration)) in stats.tx_write_size_buckets.iter() {
                    info_str += &format!("  {size:6}: {count:6}  {avg_duration}\n");
                }
            }

            info_str
        }

        let info_str = format!(
            "Table Name: {}\n\
            Column Count: {}\n\
            IO Stats (since previous query):\n{}\
            IO Stats (overall):\n{}\
            Columns:\n{}\n",
            info.table_name,
            info.column_count,
            indent_all_by(4, debug_print_io_stats(&info.io_stats_since_previous)),
            indent_all_by(4, debug_print_io_stats(&info.io_stats_overall)),
            Self::_format_columns(&info.columns),
        );

        let out = format!("Table info for '{}':\n{}", table_name, info_str);
        Ok(out)
    }

    async fn debug_table(&self, command: Option<DebugTableSubcommand>) -> VeilidAPIResult<String> {
        let Some(command) = command else {
            apibail_missing_argument!("debug_table", "command");
        };

        match command {
            DebugTableSubcommand::List => self.debug_table_list(),
            DebugTableSubcommand::Info { name } => self.debug_table_info(name).await,
        }
    }

    fn debug_punish_list(&self, _args: Vec<String>) -> VeilidAPIResult<String> {
        //
        let registry = self.core_context()?.registry();
        let network_manager = registry.network_manager();
        let address_filter = network_manager.address_filter();

        let out = format!("Address filter punishments:\n{:#?}", address_filter);
        Ok(out)
    }

    fn debug_punish_clear(&self, _args: Vec<String>) -> VeilidAPIResult<String> {
        //
        let registry = self.core_context()?.registry();
        let network_manager = registry.network_manager();
        let address_filter = network_manager.address_filter();

        address_filter.clear_punishments();

        Ok("Address Filter punishments cleared\n".to_owned())
    }

    fn debug_punish_add(&self, target: String) -> VeilidAPIResult<String> {
        //
        let registry = self.core_context()?.registry();
        let network_manager = registry.network_manager();
        let address_filter = network_manager.address_filter();

        if let Some(node_id) = get_node_id(&target) {
            address_filter.punish_node_id(node_id, PunishmentReason::Manual);
            Ok("Address Filter node id punishment added\n".to_owned())
        } else if let Some(ip_addr) = get_ip_addr(&target) {
            address_filter.punish_ip_addr(ip_addr, PunishmentReason::Manual);
            Ok("Address Filter address punishment added\n".to_owned())
        } else {
            apibail_invalid_argument!("debug_punish_add", "target", target);
        }
    }

    fn debug_punish_remove(&self, target: String) -> VeilidAPIResult<String> {
        //
        let registry = self.core_context()?.registry();
        let network_manager = registry.network_manager();
        let address_filter = network_manager.address_filter();

        if let Some(node_id) = get_node_id(&target) {
            address_filter.forgive_node_id(node_id);
            Ok("Address Filter node id punishment forgiven\n".to_owned())
        } else if let Some(ip_addr) = get_ip_addr(&target) {
            address_filter.forgive_ip_addr(ip_addr);
            Ok("Address Filter address punishment forgiven\n".to_owned())
        } else {
            apibail_invalid_argument!("debug_punish_remove", "target", target);
        }
    }

    fn debug_punish(&self, command: Option<DebugPunishSubcommand>) -> VeilidAPIResult<String> {
        let Some(command) = command else {
            apibail_missing_argument!("debug_punish", "command");
        };

        match command {
            DebugPunishSubcommand::List => self.debug_punish_list(vec![]),
            DebugPunishSubcommand::Clear => self.debug_punish_clear(vec![]),
            DebugPunishSubcommand::Add { target } => self.debug_punish_add(target),
            DebugPunishSubcommand::Remove { target } => self.debug_punish_remove(target),
        }
    }

    /// Get the help text for 'internal debug' commands.
    fn debug_help(&self) -> VeilidAPIResult<String> {
        let out = render_parser_help::<DebugCommandParser>();
        let out = out
            .lines()
            .map(|line| line.trim().to_string())
            .collect::<Vec<String>>()
            .join("\n");
        Ok(out)
    }

    #[cfg(feature = "stats")]
    #[allow(clippy::unused_async)]
    async fn debug_stats(&self, command: Option<DebugStatsSubcommand>) -> VeilidAPIResult<String> {
        let registry = self.core_context()?.registry();
        match command.unwrap_or(DebugStatsSubcommand::Print) {
            DebugStatsSubcommand::Print => Ok(registry.stats_manager().print()),
            DebugStatsSubcommand::Reset => {
                registry.stats_manager().reset();
                Ok("Stats reset".to_owned())
            }
            DebugStatsSubcommand::Log { duration, path } => {
                let log_key = registry.log_key();
                if matches!(duration.as_deref(), Some("off" | "stop" | "none")) {
                    crate::StatsManager::init(Some(log_key), crate::StatsSettings::default());
                    return Ok("Stats logging stopped".to_owned());
                }
                let interval_secs = match duration.as_deref() {
                    None => 10u32,
                    Some(d) => match d.parse::<u32>() {
                        Ok(v) if v > 0 => v,
                        _ => apibail_invalid_argument!("debug_stats", "duration", d),
                    },
                };
                let output = if let Some(p) = path {
                    #[cfg(not(all(target_arch = "wasm32", target_os = "unknown")))]
                    {
                        crate::StatsOutput::File(std::path::PathBuf::from(p))
                    }
                    #[cfg(all(target_arch = "wasm32", target_os = "unknown"))]
                    {
                        let _ = p;
                        apibail_invalid_argument!(
                            "debug_stats",
                            "path",
                            "file output unavailable on wasm"
                        );
                    }
                } else {
                    crate::StatsOutput::Facility
                };
                let dest = match &output {
                    crate::StatsOutput::Facility => "the 'stats' log facility".to_owned(),
                    #[cfg(not(all(target_arch = "wasm32", target_os = "unknown")))]
                    crate::StatsOutput::File(p) => p.display().to_string(),
                };
                crate::StatsManager::init(
                    Some(log_key),
                    crate::StatsSettings {
                        enabled: true,
                        interval_secs,
                        output,
                    },
                );
                Ok(format!("Stats logging every {interval_secs}s to {dest}"))
            }
        }
    }

    /// Show where the host application stands in its lifecycle.
    fn debug_lifecycle(&self) -> VeilidAPIResult<String> {
        cfg_if! {
            if #[cfg(feature = "app-lifecycle")] {
                Ok(self.core_context()?.attachment_manager().debug_info_lifecycle())
            } else {
                Err(VeilidAPIError::unimplemented(
                    "lifecycle needs the app-lifecycle feature",
                ))
            }
        }
    }

    /// Get node uptime info.
    async fn debug_uptime(&self) -> VeilidAPIResult<String> {
        let mut result = String::new();

        writeln!(result, "Uptime...").ok();

        let state = self.get_state().await?;

        let uptime = state.attachment.uptime;
        writeln!(result, "  since launch: {uptime:#}").ok();

        if let Some(attached_uptime) = state.attachment.attached_uptime {
            writeln!(result, "  since attachment: {attached_uptime:#}").ok();
        }

        Ok(result)
    }

    /// Cause veilid-core to panic via various means
    #[cfg(debug_assertions)]
    #[expect(clippy::unused_async)]
    async fn debug_die(
        &self,
        mode: Option<String>,
        message: Vec<String>,
    ) -> VeilidAPIResult<String> {
        let arg = mode.as_deref().unwrap_or_default();
        let rest = join_args(message);
        match arg {
            "panic" => {
                if rest.is_empty() {
                    // PANIC-CENSUS: crash-test debug_die panic with no message
                    panic!();
                } else {
                    // PANIC-CENSUS: crash-test debug_die panic with a message
                    panic!("{}", rest);
                }
            }
            "unwrap" => {
                #[expect(clippy::unnecessary_literal_unwrap)]
                Option::<()>::None.unwrap();
                // PANIC-CENSUS: crash-test proves unwrap on None panics
                unreachable!("unwrap must panic");
            }
            "unwrap_or_log" => {
                Option::<()>::None.unwrap_or_log();
                // PANIC-CENSUS: crash-test proves unwrap_or_log panics
                unreachable!("unwrap_or_log must panic");
            }
            "expect" => {
                #[expect(clippy::unnecessary_literal_unwrap)]
                Option::<()>::None.expect(&rest);
                // PANIC-CENSUS: crash-test proves expect panics
                unreachable!("expect must panic");
            }
            "expect_or_log" => {
                Option::<()>::None.expect_or_log(&rest);
                // PANIC-CENSUS: crash-test proves expect_or_log panics
                unreachable!("expect_or_log must panic");
            }
            "div0" => {
                let x = 0u32;
                let y = x / 0u32;
                // PANIC-CENSUS: crash-test proves divide by zero panics
                unreachable!("divide by zero must panic: {}", y);
            }
            "overflow" => {
                let x = u32::MAX;
                let y = x + 1;
                // PANIC-CENSUS: crash-test proves integer overflow panics
                unreachable!("integer overflow must panic: {}", y);
            }
            "oob" => {
                let x = &[3];
                #[expect(clippy::out_of_bounds_indexing)]
                let y = x[1];
                // PANIC-CENSUS: crash-test proves an out of bounds index panics
                unreachable!("array out of bounds must panic: {}", y);
            }
            "nullptr" => {
                // black_box hides the null from the invalid_null_arguments lint; it is still null at runtime
                let x: *const i32 = std::hint::black_box(std::ptr::null());
                // SAFETY: none, this crash test reads null on purpose; volatile keeps the load
                // from folding away. Natively it faults (a signal, not a panic); on wasm address
                // 0 is readable, so it falls through to the panic below
                let y = unsafe { std::ptr::read_volatile(x) };
                // PANIC-CENSUS: crash-test proves a null deref crashes
                unreachable!("null pointer read must crash: {}", y);
            }
            "unreachable" => {
                // PANIC-CENSUS: crash-test calls unreachable directly
                unreachable!("direct call of unreachable macro");
            }
            _ => {
                Ok("One of 'panic [message]', 'unwrap', 'unwrap_or_log', 'expect [message]', 'expect_or_log [message]', 'div0', 'overflow', 'oob', 'nullptr', or 'unreachable' is required".to_string())
            }
        }
    }

    /// Execute an internal debug command.
    /// `appmessage`, `appcall`, and `appreply` preserve raw trailing payload
    /// text and parse payloads in command-specific logic. Buckets/config/purge/
    /// route/record/table/punish/network command routing uses clap while
    /// semantic parsing remains delegated to existing helpers and downstream
    /// components.
    pub async fn debug(&self, args: String) -> VeilidAPIResult<String> {
        let res = {
            let argv_parts = shell_words::split(&args)
                .map_err(|e| VeilidAPIError::parse_error(e.to_string(), args))?;

            if argv_parts.is_empty() || argv_parts.len() == 1 && argv_parts[0] == "help" {
                return self.debug_help();
            }

            let argv: Vec<String> = std::iter::once("debug".to_owned())
                .chain(argv_parts.iter().cloned())
                .collect();
            let nested_family =
                get_nested_debug_family_name(argv_parts.first().map(String::as_str));
            let known_command =
                get_known_debug_command_name(argv_parts.first().map(String::as_str));

            let parsed = match DebugCommandParser::try_parse_from(argv) {
                Ok(parsed) => parsed,
                Err(err) if err.kind() == ErrorKind::DisplayHelp => return Ok(err.to_string()),
                Err(err) if err.kind() == ErrorKind::InvalidSubcommand => {
                    if let Some(family) = nested_family {
                        return Err(VeilidAPIError::generic(format!(
                            "Unknown {} subcommand",
                            family
                        )));
                    }
                    return Err(VeilidAPIError::generic("Unknown debug command"));
                }
                Err(_) => {
                    if let Some(family) = known_command {
                        let arg = argv_parts.get(1).cloned().unwrap_or_default();
                        apibail_invalid_argument!(format!("debug_{}", family), "arg_1", arg);
                    } else {
                        return Err(VeilidAPIError::generic("Unknown debug command"));
                    }
                }
            };

            let command = parsed.command;

            match command {
                DebugCommand::Nodeid => self.debug_nodeid(),
                DebugCommand::Buckets { min_state } => self.debug_buckets(min_state),
                DebugCommand::Dialinfo => self.debug_dialinfo(),
                DebugCommand::Peerinfo { args } => self.debug_peerinfo(args),
                DebugCommand::Contact { node_ref } => self.debug_contact(node_ref),
                DebugCommand::Keypair { cryptokind } => self.debug_keypair(cryptokind),
                DebugCommand::Entries { args } => self.debug_entries(args),
                DebugCommand::Entry { node } => self.debug_entry(node),
                DebugCommand::Punish { command } => self.debug_punish(command),
                DebugCommand::Txtrecord { keypairs } => self.debug_txtrecord(keypairs).await,
                DebugCommand::Relay { args } => self.debug_relay(args).await,
                DebugCommand::Transport { args } => self.debug_transport(args),
                DebugCommand::Chaos { args } => self.debug_chaos(args),
                DebugCommand::Ping { destination } => self.debug_ping(destination).await,
                DebugCommand::Appmessage { destination, data } => {
                    self.debug_app_message(destination, data).await
                }
                DebugCommand::Appcall { destination, data } => {
                    self.debug_app_call(destination, data).await
                }
                DebugCommand::Appreply { id, data } => self.debug_app_reply(id, data).await,
                DebugCommand::Resolve { destination } => self.debug_resolve(destination).await,
                DebugCommand::Nodeinfo => self.debug_nodeinfo().await,
                DebugCommand::Purge { command } => self.debug_purge(command).await,
                DebugCommand::Attach => self.debug_attach().await,
                DebugCommand::Detach => self.debug_detach().await,
                DebugCommand::Config { arg_0, arg_1 } => self.debug_config(arg_0, arg_1).await,
                DebugCommand::Network { command } => self.debug_network(command).await,
                DebugCommand::Route { command } => self.debug_route(command).await,
                DebugCommand::Record { command } => self.debug_record(command).await,
                DebugCommand::Table { command } => self.debug_table(command).await,
                DebugCommand::Uptime => self.debug_uptime().await,
                DebugCommand::Lifecycle => self.debug_lifecycle(),
                #[cfg(feature = "stats")]
                DebugCommand::Stats { command } => self.debug_stats(command).await,
                #[cfg(debug_assertions)]
                DebugCommand::Die { mode, message } => self.debug_die(mode, message).await,
            }
        };
        res
    }

    fn get_destination(
        self,
        registry: VeilidComponentRegistry,
    ) -> impl FnOnce(&str) -> PinBoxFutureStatic<Option<Destination>> {
        move |text| {
            let text = text.to_owned();
            Box::pin(async move {
                // Safety selection
                let (text, ss) = if let Some((first, second)) = text.split_once('+') {
                    let ss = get_safety_selection(registry.clone())(second)?;
                    (first, Some(ss))
                } else {
                    (text.as_str(), None)
                };
                if text.is_empty() {
                    return None;
                }
                if let Some(text) = text.strip_prefix('#') {
                    let routing_table = registry.routing_table();
                    let rss = routing_table.route_spec_store();

                    // Private route

                    let private_route = if let Some(prid) =
                        get_route_id(registry.clone(), false, true)(text)
                    {
                        rss.best_remote_private_route(&RemoteRouteSetId::from_route_id(prid))?
                    } else {
                        let n = get_number(text)?;

                        self.with_debug_cache(|dc| {
                            let prid: &RouteId = dc.imported_routes.get(n)?;
                            let prid = RemoteRouteSetId::from_route_id(prid.clone());
                            let Some(private_route) = rss.best_remote_private_route(&prid) else {
                                // Remove imported route
                                let _ = dc.imported_routes.remove(n);
                                veilid_log!(registry info "removed dead imported route {}", n);
                                return None;
                            };
                            Some(private_route)
                        })?
                    };

                    Some(Destination::private_route(
                        private_route,
                        ss.unwrap_or(SafetySelection::Unsafe(Sequencing::default())),
                    ))
                } else if let Some((first, second)) = text.split_once('@') {
                    if ss.is_some() {
                        return None;
                    }
                    // Relay
                    let relay_di = DialInfo::from_str(second).ok()?;
                    let target_nr = get_node_ref(registry.clone())(first)?;

                    Some(Destination::dial_info(relay_di, target_nr))
                } else {
                    // Direct
                    let target_nr = resolve_filtered_node_ref(
                        registry.clone(),
                        ss.clone()
                            .unwrap_or(SafetySelection::Unsafe(Sequencing::PreferUnordered)),
                    )(text)
                    .await?;

                    Some(Destination::direct(target_nr, ss))
                }
            })
        }
    }

    fn get_opened_dht_record_context(
        self,
        args: &[String],
        context: &str,
        key: &str,
        arg: usize,
    ) -> VeilidAPIResult<(RecordKey, RoutingContext)> {
        let key = match get_debug_argument_at(args, arg, context, key, get_dht_key_no_safety)
            .ok()
            .or_else(|| {
                // If unspecified, use the most recent key opened or created
                self.with_debug_cache(|dc| dc.opened_record_contexts.back().map(|kv| kv.0).cloned())
            }) {
            Some(k) => k,
            None => {
                apibail_missing_argument!("no keys are opened", "key");
            }
        };

        // Get routing context for record

        let Some(rc) = self.with_debug_cache(|dc| dc.opened_record_contexts.get(&key).cloned())
        else {
            apibail_missing_argument!("key is not opened", "key");
        };

        Ok((key, rc))
    }
}

#[cfg(test)]
mod clap_parse_tests {
    use super::*;

    #[test]
    fn parses_top_level_commands() {
        let parsed = DebugCommandParser::try_parse_from(["debug", "nodeinfo"]).ok();
        assert!(matches!(
            parsed,
            Some(DebugCommandParser {
                command: DebugCommand::Nodeinfo
            })
        ));

        let parsed = DebugCommandParser::try_parse_from(["debug", "nodeid"]).ok();
        assert!(matches!(
            parsed,
            Some(DebugCommandParser {
                command: DebugCommand::Nodeid
            })
        ));

        assert!(DebugCommandParser::try_parse_from(["debug", "NoDeId"]).is_err());
    }

    #[test]
    fn rejects_unknown_top_level_command() {
        assert!(DebugCommandParser::try_parse_from(["debug", "definitely-unknown"]).is_err());
    }

    #[test]
    fn single_pass_parser_preserves_quoted_payload_tokens() {
        let parsed =
            DebugCommandParser::try_parse_from(["debug", "appmessage", "destination", "foo bar"])
                .ok();
        assert!(matches!(
            parsed,
            Some(DebugCommandParser {
                command: DebugCommand::Appmessage { .. }
            })
        ));
    }

    #[test]
    fn parses_route_subcommands() {
        let parsed = DebugCommandParser::try_parse_from(["debug", "route", "allocate", "rel"]).ok();
        assert!(matches!(
            parsed,
            Some(DebugCommandParser {
                command: DebugCommand::Route {
                    command: Some(DebugRouteSubcommand::Allocate { .. })
                }
            })
        ));

        let parsed =
            DebugCommandParser::try_parse_from(["debug", "route", "test", "route-id"]).ok();
        assert!(matches!(
            parsed,
            Some(DebugCommandParser {
                command: DebugCommand::Route {
                    command: Some(DebugRouteSubcommand::Test { .. })
                }
            })
        ));
    }

    #[test]
    fn rejects_unknown_route_subcommand() {
        assert!(DebugCommandParser::try_parse_from(["debug", "route", "unknown"]).is_err());
    }

    #[test]
    fn parses_record_subcommands() {
        let parsed = DebugCommandParser::try_parse_from(["debug", "record", "create", "1"]).ok();
        assert!(matches!(
            parsed,
            Some(DebugCommandParser {
                command: DebugCommand::Record {
                    command: Some(DebugRecordSubcommand::Create { .. })
                }
            })
        ));

        let parsed =
            DebugCommandParser::try_parse_from(["debug", "record", "rehydrate", "some-key"]).ok();
        assert!(matches!(
            parsed,
            Some(DebugCommandParser {
                command: DebugCommand::Record {
                    command: Some(DebugRecordSubcommand::Rehydrate { .. })
                }
            })
        ));
    }

    #[test]
    fn parses_table_punish_and_network_subcommands() {
        let parsed = DebugCommandParser::try_parse_from(["debug", "table", "info", "table"]).ok();
        assert!(matches!(
            parsed,
            Some(DebugCommandParser {
                command: DebugCommand::Table {
                    command: Some(DebugTableSubcommand::Info { .. })
                }
            })
        ));

        let parsed = DebugCommandParser::try_parse_from(["debug", "punish", "remove", "node"]).ok();
        assert!(matches!(
            parsed,
            Some(DebugCommandParser {
                command: DebugCommand::Punish {
                    command: Some(DebugPunishSubcommand::Remove { .. })
                }
            })
        ));

        let parsed = DebugCommandParser::try_parse_from(["debug", "network", "stats"]).ok();
        assert!(matches!(
            parsed,
            Some(DebugCommandParser {
                command: DebugCommand::Network {
                    command: Some(DebugNetworkSubcommand::Stats)
                }
            })
        ));

        let parsed = DebugCommandParser::try_parse_from(["debug", "purge", "routes"]).ok();
        assert!(matches!(
            parsed,
            Some(DebugCommandParser {
                command: DebugCommand::Purge {
                    command: Some(DebugPurgeSubcommand::Routes)
                }
            })
        ));
    }

    #[test]
    fn parses_missing_nested_subcommands() {
        let parsed = DebugCommandParser::try_parse_from(["debug", "route"]).ok();
        assert!(matches!(
            parsed,
            Some(DebugCommandParser {
                command: DebugCommand::Route { command: None }
            })
        ));

        let parsed = DebugCommandParser::try_parse_from(["debug", "record"]).ok();
        assert!(matches!(
            parsed,
            Some(DebugCommandParser {
                command: DebugCommand::Record { command: None }
            })
        ));

        let parsed = DebugCommandParser::try_parse_from(["debug", "purge"]).ok();
        assert!(matches!(
            parsed,
            Some(DebugCommandParser {
                command: DebugCommand::Purge { command: None }
            })
        ));
    }

    #[test]
    fn parses_buckets_config_and_die_forms() {
        let parsed = DebugCommandParser::try_parse_from(["debug", "buckets", "dead"]).ok();
        assert!(matches!(
            parsed,
            Some(DebugCommandParser {
                command: DebugCommand::Buckets { min_state: Some(_) }
            })
        ));

        let parsed = DebugCommandParser::try_parse_from(["debug", "config", "insecure"]).ok();
        assert!(matches!(
            parsed,
            Some(DebugCommandParser {
                command: DebugCommand::Config {
                    arg_0: Some(_),
                    arg_1: None
                }
            })
        ));

        #[cfg(debug_assertions)]
        {
            let parsed = DebugCommandParser::try_parse_from(["debug", "die", "panic", "msg"]).ok();
            assert!(matches!(
                parsed,
                Some(DebugCommandParser {
                    command: DebugCommand::Die {
                        mode: Some(_),
                        message
                    }
                }) if message.len() == 1
            ));
        }
    }

    #[test]
    fn parses_record_data_forms() {
        assert_eq!(get_data("#666f6f"), Some(b"foo".to_vec()));
        assert_eq!(get_data("\"foo\\nbar\""), Some(b"foo\nbar".to_vec()));
        assert_eq!(get_data("plain"), Some(b"plain".to_vec()));
        assert!(parse_data("#zz").is_err());
    }

    #[test]
    fn parses_context_free_converters() {
        assert!(matches!(
            parse_bucket_entry_state("dead"),
            Ok(BucketEntryState::Dead)
        ));
        assert!(matches!(
            parse_dht_report_scope("syncset"),
            Ok(DHTReportScope::SyncSet)
        ));
        assert!(PublishedState::from_str("published")
            .map(|x| x.as_bool())
            .unwrap_or(false));
        assert!(parse_subkeys("1..=3,5..=8").is_ok());
    }

    #[test]
    fn generated_help_includes_command_groups() {
        let help = render_parser_help::<DebugCommandParser>();
        assert!(help.contains("nodeid"));
        assert!(help.contains("record"));
        assert!(help.contains("help"));
    }

    #[test]
    fn top_level_help_subcommand_is_available() {
        let err = DebugCommandParser::try_parse_from(["debug", "help", "route"]).unwrap_err();
        assert_eq!(err.kind(), ErrorKind::DisplayHelp);
    }
}
