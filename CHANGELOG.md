**Changed in Veilid 0.5.8**

- veilid-core-optimized:
  - New crate holding monomorphic exports, for crates that want full speed optimization in every build profile.
  - BLAKE3 uses its WASM SIMD implementation on wasm32, which adds 14 KB to the `.wasm` and needs WASM SIMD, supported by browsers since Safari 16.4.

- veilid-core:
  - **Breaking** `open_dht_record()` takes an `OpenDHTRecordOptions` argument whose `rehydrate` flag, default true, can be turned off so that opening a record you are only checking for does not count as interest and trigger rehydration.
  - **Breaking** `ProtectedStore` is reached through `VeilidAPI`, and its old Rust interface is gone.
  - **Breaking** `TableStore` is reached through `VeilidAPI`, and its old Rust interface is gone.
  - **Breaking** The `get-size2` crate is gone, taking the `GetSize` impls and `get_heap_size()` off the API types, and StorageManager computes data sizes more simply.
  - **Breaking** The WASM boundary uses tsify 0.5.7 `Ts<T>` types, so a serialization failure throws a catchable error instead of leaking or panicking, and in TypeScript the group `contains_kind` is renamed `containsKind` and typed-value constructors take the branded `CryptoKind` instead of a bare string.
  - **Breaking** `VeilidAPIError` derives from the JavaScript `Error`, closing [#441](https://gitlab.com/veilid/veilid/-/work_items/441).
  - **Breaking** `AttachmentState` variants are reordered from least to most attached, and the type is now `Ord`.
  - **Breaking** `TableDB::store_json`, `load_json` and `delete_json` and `TableDBTransaction::store_json` are gone; the table store serializes its own state as CBOR and apps store raw bytes.
  - **Breaking** `TableDB` and `TableDBTransaction` take keys and values as `Bytes`, `load`, `delete` and `get_keys` return `Bytes`, and `estimate_storage_size_json` is gone.
  - **Breaking** `attach()` and `detach()` succeed when the node is already attaching or detaching, a bind wait reports `Attaching` instead of `Detached`, and `detach()` on a detached node fails with `Not attached`.
  - **Breaking** `ValueData::new` and `ValueData::new_with_seq` are no longer public: apps receive a `ValueData` from the API and never build one.
  - **Breaking** `VeilidAPI::debug_help()`, `debug_uptime()` and `debug_die()` are no longer public; `debug()` runs them as the `help`, `uptime` and `die` commands.
  - **Breaking** A node answers direct bootstrap requests (BOOT and B01T) only when the new `network.routing_table.serve_direct_bootstrap` config is set, which is off by default, or when it finds itself among its own bootstraps, so an app no longer hands its peer set to anyone who asks.
  - **Security** B01T direct-bootstrap requests are answered from the routing domain's last successful bootstrap set, persisted across restarts, without resolving DNS, so an unauthenticated packet can no longer trigger non-veilid traffic or block a worker on a slow resolver (thanks to quikchaos for reporting it).
  - **Security** A dial info, bootstrap record or node id list holding a non-ASCII character fails to parse instead of panicking the node, which any peer could trigger with its own peer info (thanks to quikchaos for reporting it).
  - **Security** A safety route hop with no next hop, an answer of a type this node did not ask for, a DHT question whose record key has an unsupported kind or the wrong length, and a FindNode question with a short node id are refused instead of panicking the node.
  - **Security** Dial info detail, relay info, relay node id and routed signature lists are capped before they are allocated or walked, so a peer info carrying an oversized list of empty elements no longer costs hundreds of megabytes or a second of CPU per message.
  - **Security** A direct bootstrap request counts against its sender's connection rate, `network.max_connection_frequency_per_min`, so a flood of 4-byte requests cannot draw unlimited multi-kilobyte replies.
  - **Security** A bootstrap peer received as JSON is rebuilt from its signed node info bytes, so a spoofed or tampered reply can no longer attach its own dial info, node ids or timestamp to a bootstrap node's signature.
  - **Security** A value staged by a DHT transaction is checked against the record schema and its signature when it arrives, as a plain set value already is, so a writer the schema does not admit can no longer commit a value through a transaction and hold a subkey at a sequence number the record's writers cannot reach (thanks to 4gt.it for reporting it).
  - **Security** A value a storing node hands back inside a DHT transaction is checked against the record schema, as plain get and set answers already are, so a node cannot feed a value from a writer the schema does not admit into a transaction's consensus or to the app.
  - Safety routes are allocated, tested and released per sequence ordering, and a dead route's hops stay out of that ordering's new allocations until the hop or this node publishes new peer info, or the online state changes.
  - A lost loopback no longer schedules another test of its own route, and a multi-hop loopback failure no longer blames the first hop's transport.
  - Per-ordering loopback tallies, a `route test summary` log line for each minute with a loopback failure or release, and the `route diag` and `route reallocate` debug commands.
  - `route allocate` accepts node ids to avoid as hops.
  - A refused connect no longer counts as evidence against a transport.
  - An interface address change marks PublicInternet offline, purges connections and re-detects dial info only when the interface, source address or gateway its traffic leaves from change, so a tunnel, a secondary interface or an alias coming or going no longer drops the relay or re-maps the external port; a change to one family's egress while another still carries traffic retires that family's connections and mappings alone.
  - The Apple route reader no longer keeps a link-local gateway's embedded interface index in its address.
  - A dropped protected connection is re-opened by a keepalive ping instead of a bare connect, which the far end could not attribute to this node and closed at its initial timeout, and a connection this node closed itself no longer counts against the relay's protected-connection lifetime or the re-open breaker, so a move between interfaces keeps its relay.
  - After an egress change the relay optimizer waits for latency measured from the new egress before ranking an incumbent, and a relay is judged on its protected-connection lifetimes only once three have been recorded.
  - An IPv4 egress from this host's own CLAT translator moves with the IPv6 route it rides on, and is not a global family while that route is gone.
  - A send that aborted before any transport was tried no longer counts a node as unreachable while this node's own PublicInternet network is down, so a two-second gap between egresses no longer kills a relay.
  - An outbound-only node asks for dial info detection only when every recent reporter names one external address, once per address, so a symmetric NAT's stable address is detected exactly once instead of once per transport.
  - A concluded detection tells the address check which external addresses it saw, so the address it just examined is not re-detected, the periodic outbound-only recheck is gone, and a transport the detection reached no verdict on is re-detected when enough reporters answer over it.
  - PublicInternet offline detection counts a lost question only when it was sent after the last answer arrived, so a burst of timeouts overlapping a working path no longer flips a node offline and back within a second; the offline decision logs the failing nodes per socket class.
  - PublicInternet leaves the offline state only on an answer to a question sent after it went offline, so an answer to an older question or unasked inbound traffic can no longer flip a node online onto a dead network, where it would fail discovery, unpublish and mark its peers dead.
  - A send this node declined, over a suppressed transport or to a punished node, is no longer recorded as the peer being unreachable, so a relay no longer dies at its first keepalive over a transport the node has stopped using.
  - Last flows are kept per remote address, answers return over the flow their question arrived on, and a lost question over a flow we did not dial blames nobody.
  - Degraded detection counts fresh failures of Reliable peers only, excludes our own relay, and clears its evidence on recovery.
  - App call results are logged truncated.
  - A DHT transaction record no longer fails its stage consensus when as many node transactions went unanswered as answered.
  - Bootstrap records are cached verbatim for every record version received, and a saved routing table from before the cache is discarded on load, so a restarted node bootstraps again instead of holding no records and refusing B01T.
  - New `network.routing_table.bootstrap_records` config, empty by default, takes whole bootstrap records as `veilid-server --dump-txt-record` prints them and serves them to direct bootstrap requests without resolving `bootstrap`.
  - A direct bootstrap request over raw TCP, which is 8 bytes, is answered because each peek reads only what it examines instead of waiting for 64 bytes.
  - Bootstrap hostname resolution runs on the blocking pool instead of the executor.
  - On Android below API 29, which has no system TXT resolver, a bootstrap hostname is contacted directly on port 5150 over UDP or TCP for a B01T exchange, and startup fails if neither protocol is enabled.
  - Stats collection, aggregation and analysis for all subsystems, with a Python script for reviewing the effect of optimizations.
  - DHT transaction quality counters under the `stats` feature cover per-command fanout outcomes, subkey confirmation coverage at End and Commit, node-set attrition, mitigation costs and TryAgain reasons, and `stats reset` zeroes them.
  - A DHT transaction ends only at nodes that answered every set, first re-sending a node the sets it is missing, and set answers that arrive after the early consensus exit are recorded instead of dropped.
  - A DHT transaction commit held by fewer than the required nodes returns TryAgain instead of Ok.
  - A set that reaches a storage node after End is refused without ending that node's transaction.
  - DHT transactions drop the set retry pass, which re-sent 4 of 6391 set RPCs in a stress run and none once late answers were collected.
  - A bootstrap name whose TXT lookup returns nothing within ten seconds is direct-bootstrapped over every enabled protocol at once, so a node behind a phone tether whose DNS relay answers A/AAAA but not TXT still joins.
  - A node never sends a bootstrap request to its own dial info.
  - IPv6 global addresses are kept per interface, and a rotating privacy address no longer counts as an interface change.
  - RPC safety-route round-robin is destination-aware, so a lost question rotates the route for that destination's retry without rotating it for every other destination.
  - DHT transaction keepalives go out only when no command answer has refreshed the node's lease since the last deadline.
  - A DHT transaction command returns at consensus instead of waiting up to half a second for stragglers, which settle in the background.
  - Fix a spurious OnlineDetector flap.
  - TCP connections are dropped when the node goes offline, so they are recreated after the NAT state loss common on cellular.
  - Fix UDP stalls on cellular and CLAT464.
  - Correct timeout for native WS connects.
  - Timeout-bound public internet dial info detection.
  - AIMD experiment for DHT operations, off by default behind the `aimd-dht` feature.
  - Fix a background transaction pre-empting a foreground record open.
  - Looser IPv6 detection where sandboxing hides the default route.
  - Fix excessive `inspect_dht_record` search duration.
  - `set_dht_value` and `DHTTransaction::set` options expose `min_seqnum`, which raises the written sequence number after the automatic increment (`ValueSeqNum::MAX` minus `n` leaves `n` writes), and `DHTReportScope::UpdateSet` no longer fails on a subkey at the maximum sequence number.
  - Holepunching uses a best-effort TTL instead of locks.
  - Fix per-transport statistical suppression so broken protocols stop being selected.
  - Outbound-only address change detection backs off.
  - Rejected and disqualified nodes do not count toward the `inspect_dht_record` consensus width cap.
  - A shorter post-consensus grace period keeps transactions from waiting on slow nodes beyond consensus.
  - OnlineDetector tracks local stalls and no longer blames local degradation on specific nodes.
  - Better watch reporting coalescing.
  - New `mock-api` feature, used with `--no-default-features`, tests apps against a stubbed `VeilidAPI` backed by in-memory maps with no network access, so integration tests run repeatably and fast in CI.
  - Fix watch routes, and route failures weigh less in offline detection.
  - Dial info discovery scales its validation quorum down to the available validator pool (minimum 2), so small networks confirm dial info and settle instead of retrying forever.
  - Node liveness probing sends about half the steady-state pings: never-answered nodes are dead after three lost questions instead of 60 seconds, missing and initial nodes are probed at the unreliable interval instead of every tick, and a failing transport backs off.
  - Degraded detection no longer fires on healthy networks, counting only failures from recently answering transports that span several percent of the live routing table.
  - Stats output reports ping outcomes and queues per purpose and transport, and a new `entries liveness` debug command shows per-node probe ages, transport health and steady-loss spans.
  - Latency statistics are computed when read instead of sorting a rolling window under a lock on every sample, which had made stats accounting the most contended lock in the process and starved the executor under load.
  - Online detector interval data is published as a read-mostly snapshot, so per-node state computation no longer queues behind the detector's state machine.
  - Route loopback failures deferred while offline or degraded are capped, so a dead route cannot stay allocated forever on a node that stays degraded.
  - Table store databases are integrity-checked at open, and a damaged one is rebuilt from its readable rows with the damaged files kept beside the store for inspection.
  - The `stats` feature is off by default.
  - The API tracing logger runs on its own thread with log throttles and a queue, and RPC hot-path logging is trimmed.
  - `aws-lc-sys`, 23 other crates and the cmake build requirement are gone, since the `geolocation` feature's build script uses `ureq` instead of `reqwest`.
  - The Android x86_64 build no longer links `libclang_rt.builtins-x86_64-android.a` from the NDK to work around a missing symbol.
  - Fix the table store per-namespace instance lock on Android.
  - New `wait_for_attachment_condition()` API, in every binding, waits until the attachment state satisfies a condition, by default until the node is attached with PublicInternet ready.
  - New `app-lifecycle` feature, on in veilid-flutter and veilid-wasm, saves the routing table and storage metadata when the app moves to the background, holding iOS background time until the saves land.
  - New `attachment.app_lifecycle.prefer_foreground` config, default true, keeps macOS App Nap and Windows power throttling off while attached.
  - In a browser, `app-lifecycle` follows a page's visibility, back/forward cache and freeze events, and treats a service worker as in the background, keeping it alive until a pending save lands.
  - New `report_app_lifecycle_state()` API lets a host report its lifecycle where the node cannot detect it, and the `attachment.app_lifecycle.auto_detect_state` config, default true, turns detection off.
  - An API call whose feature is compiled out stays callable from the remote API and Python and fails with `Unimplemented`.
  - After a stall of 2.5 s or more the attachment state is sent again even if unchanged, or on the app's return when the stall came while it was in the background.
  - A second browser `online` or `offline` event while a detached node was not listening no longer panics.
  - A detach or shutdown no longer waits for a UPnP request to an unresponsive router or for a bootstrap hostname lookup.
  - Storage metadata that changes while the app is in the background is saved within a second instead of at the next 30s save, holding iOS background time until the save lands.
  - Timeouts of RPCs in flight across a local stall no longer count toward offline or degraded detection, which sometimes took a thawed node offline and back and doubled its degraded hold to 10s.
  - `inspect_dht_record` returns TryAgain when no node answered, instead of a report whose empty network sequence numbers read as nothing newer online.
  - A failed change inspection of a watched record retries in 5s instead of backing off to 10 minutes, and a watch that never established is inspected every 2 minutes instead of 10.
  - A watch reconcile that no node answered retries in 5s instead of waiting 60s, and watch cancels, renewals, reconciles and change inspections wait while the node is offline.
  - `set_dht_value` no longer loops forever when nodes hold different values at its sequence number: it settles on the value most of them hold and returns it, even when that is the value just set, and an identical rewrite resends the stored value rather than encrypting it again.
  - Sets, gets and watches keep asking nodes past ones that answer with an older value or none, instead of waiting out their whole timeout, so a get no longer misses a value held by nodes it never asked.
  - A DHT fanout that can no longer reach consensus stops waiting on a node still in flight after a third of the RPC timeout, or three times its slowest answer if longer, instead of the whole RPC timeout.
  - A question to a node avoids a safety route that lost one to it until the node answers over that route, either side publishes new peer info, or the online state changes, and DHT operations ask a node that lost a routed question after the rest of its window, without waiting out its timeout, until it answers one.
  - A watch held by enough nodes for consensus checks for missed changes once per renewal after its fallback interval reaches the 2-minute ceiling, instead of every 2 minutes.
  - Answering messages takes fewer shared locks: component and routing domain lookups take none, a known sender is registered under the routing table's read lock, and unchanged entry fields are not rewritten.
  - A failed network result's extra log detail is built only when it is logged, instead of formatting every RPC message header.
  - Fix a panic sorting allocated routes when a route's latency changed during the sort.
  - A hidden desktop app keeps saving on the foreground schedule, since saving on every change is only for platforms that freeze a background process.
  - A degraded period caused only by the node's own stalls no longer ends with a check of every watch for missed changes.
  - The `backtrace` feature uses `std::backtrace` instead of the `backtrace` crate, which does not build for iOS on libc 0.2.190.
  - The table store keeps rows as written, encrypted without compression, and a one-time migration at startup converts an existing store; an older version cannot open a store this version has touched.
  - The migration drops the routing table, whose entry format changed, so a node bootstraps once after upgrading.
  - IP addresses in dial info serialize as text in every format.
  - The record store writes a value's data as bytes instead of base64 text, a quarter smaller per subkey, and values written as text still read.
  - The routing table saves each entry on its own and writes only the entries that changed since its last save, instead of rewriting the whole table each time.
  - A saved routing table loads only into a node with the same node ids, bootstrap hosts, bootstrap keys, bootstrap records and network key; generated node ids and the bootstrap keys and records were not checked before.
  - Detaching always saves the routing table, and shutting down saves it again only if something changed after the detach.
  - Attaching on a network that has not changed since the last publication, after a detach or a restart, publishes the same signed peer info again with its dial info, relays and port mappings instead of detecting them.
  - A peer's last report of our address is saved with its routing table entry, so a WAN address that changed while the node was down is still noticed.
  - Safety routes keep their test results across a restart, so a node that was ready comes back ready instead of first testing its routes again.
  - Our published state and a route's first passing test reach disk within ten seconds, even while other routing table changes keep arriving.
  - A restart keeps each routing table entry's per-transport statistics and contact failures as last saved, and drops them when the network changed since the last publication.
  - A UPnP mapping whose gateway is gone is dropped and dial info detected again, instead of failing its renewal on every tick.
  - `keyvaluedb` 0.2.6 updates an overwritten SQLite row in place instead of deleting and reinserting it, halving the WAL writes of an overwrite.
  - A safety route for unordered traffic is tested over a datagram protocol even when another route holds a connection to its first hop, and a node with no datagram protocol, such as a browser, keeps only ordered routes.
  - A hole punch receipt returns over the punched hole rather than through a relay, so the data that follows it goes direct.
  - The hole punch TTL comes from the footgun settings `internal.network.default_hole_punch_ttl` (3) and `internal.network.cgnat_hole_punch_ttl` (8), the second used when this node's IPv4 traffic leaves from a carrier-grade NAT, DS-Lite or CLAT address.
  - The routing table save schedule comes from the footgun settings under `internal.network.routing_table.save`, and the ceiling of a watch's change inspection interval from `internal.network.dht.max_watch_inspect_interval_ms`.
  - A hole punch whose receipt came through a relay, as older responders send it, falls back to the relay rather than treating the relay's flow as the hole.
  - IPv6 link-local addresses are no longer published as LocalNetwork dial info, since a dial info carries no scope id and no peer could dial them, so a tunnel or AirDrop interface rotating its link-local address no longer churns the LocalNetwork peer info.
  - A reply sent while its interface is going away, as in a cellular-to-wifi switch, is a lost send at debug level instead of an RPC error with a backtrace, and an RPC network error's message keeps the socket error behind it.
  - Once PublicInternet outbound readiness is attained, any live ordered safety route keeps it and only an empty pool loses it; two routes per default length remain the bar to attain it and the allocation target, so route churn on a lossy link no longer flaps `public_internet_ready`.
  - A dead safety route still awaiting release no longer counts toward PublicInternet readiness.
  - A watch's fallback inspection is a silence timer: a delivered change restarts it at its current interval instead of switching it off for the rest of the watch, so a record that goes quiet after delivering is inspected again one interval later.
  - A per-node watch that expires unrenewed, after a stall through its renewal window or while its storage node stopped answering, leaves its watch, which reconciles, instead of holding the watch at consensus with nothing renewing it.
  - Watching a record again, or cancelling part of its watch, before its first watch is established updates the watch instead of dropping it.
  - A fallback inspection that finds a missed change reconciles the watch onto fresh routes, so a watch whose pushes stopped arriving is repaired instead of polled.
  - A watch renews only the per-node watches due for renewal or bound to a dead route, and a watch short of consensus reconciles at the paced retry rate, so a storage node that stops answering no longer multiplies renewals and reconciles.
  - At most 16 change inspections run at once, oldest first, so inspecting every watch after a freeze or an outage drains at a bounded rate.
  - A fanout answer that arrives after its node was disqualified is logged at debug level instead of as an error.
  - New `debug chaos` command reproduces a bad network on demand: loss and size caps for every address or by address range, a rate-limited link behind a bounded queue whose capacity drifts and collapses, presets measured on a cellular link (`mobile`, `mobile-bad`, `mtu-blackhole`), and lost watch pushes (`rpc.value_changed.loss`).
  - On iOS, Android and web, a process the platform froze in the background change-inspects every watch once when the app is active again, so a change missed while frozen arrives on return instead of waiting for a storing node's retry.
  - New `VeilidAPI::change_config()` replaces a detached node's configuration for its next attach, refusing the keys that need a restart, and UPnP is set up per attach so a change to `network.upnp` takes effect then.
  - The online detector measures the share of round trips lost on this node's own link, from direct questions to peers that answered recently, and reports Degraded while that loss lasts instead of only after bursts of failures.
  - A peer that answered recently still counts toward offline detection after this node's own loss has cost it its standing, so total loss after a lossy period reads as Offline.
  - Route failures observed under this node's own loss count for less, so more of them are needed before a route is released, and none count while offline.
  - A peer's lost questions and failed sends under this node's own loss count for less instead of not at all, and a lossy period delays a silent peer's death only by the share of it the loss explains, so a peer that stops answering on a lossy link is still found dead; stalls, failure bursts and outages still count against no peer.
  - A relay that can no longer relay is replaced while this node is degraded too, and only an outage holds it.
  - Reconcile retries for a watch short of consensus and retries of failed change inspections back off from 5 seconds, renewals after a watch's push route dies back off from one RPC timeout, and all back off further under measured loss.
  - A dead push route returns a watch's fallback inspection to its base interval on the watch's own schedule, so the inspections it triggers spread out instead of arriving together.
  - Under measured loss a watch's next fallback inspection waits longer, and leaving Degraded no longer change-inspects every watch.
  - A watch at consensus that has had no push since its last fallback inspection keeps inspecting at the fallback ceiling instead of waiting for its renewal window, so changes behind pushes that fail silently arrive within the ceiling; a watch on a record that does not change is inspected at the ceiling too.
  - A storage node left out of a watch because its renewal went unanswered rejoins the watch when it pushes a change on the watch id it holds, instead of having every push dropped, which on a lossy link could leave a record undelivered until the watch's renewal.
  - `debug nodeinfo` shows each routing domain's online state and loss estimate.
  - A node tests only the remote private routes the app imported, at most every 30 seconds while they answer. A reply route serves its one answer and is no longer kept or tested, and a watch keeps its request's reply destination for its ValueChanged pushes. Before, a node retested every reply route that reached it with a SetValue or WatchValue, about every 15 seconds and never letting one expire, which was about half of a busy node's received traffic.
  - A bootstrapping node keeps pinging the peers it already knows, so peers that went missing while its own link was down answer again when it recovers instead of dying.
  - A node sizes its UDP datagrams and TCP segments per address family, starting at 1,200 bytes and growing to what its egress interface carries once a probe shows full-size packets reach a peer and come back, so a path that silently drops larger packets, such as a cellular 464XLAT, no longer stalls; a move to another egress starts the family over at 1,200 bytes.
  - A node advertises the sizes it takes in its peer info, and each relay's sizes in its relay info, and every send uses the smaller of its own size and the next hop's, or 1,200 bytes toward a node that advertises none. Sizes for transports a node does not know are dropped and the rest of the peer info kept, as with dial info, a size outside 576 to 1,500 bytes is rejected, and a probe counts only when its answer arrives whole in one datagram of its size. Older nodes accept and forward the new peer info.
  - PublicInternet peer info is published once every address family's size is settled by a probe, or at 1,200 bytes when no peer can answer one.
  - TCP segments keep to the receiver's size on every platform: Linux and Android hold each write to it and mark where it ends, macOS and iOS lower the connection's segment size, and Windows caps the socket's IP MTU. Linux also announces the node's own size when a connection opens.
  - A watch keeps to the `get_value_count` storage nodes closest to its record and cancels every other watch it learns of, including ones accepted after consensus that it used to leave pushing until they expired, so each change arrives about 3 times instead of 6.
  - A watch whose last push came before a loss period ends inspects again soon after, each at its own point in the fallback interval, so a change pushed during the loss turns up then instead of at the next renewal.
  - A storage node at its record count limit evicts the least recently used record itself when a new one arrives, instead of letting the cache evict it behind its back and logging a consistency error for every record past the limit.
  - A fanout question (FindNode, GetValue, SetValue, InspectValue, WatchValue, TransactBegin) carries a `FanoutConfig` saying how many peers its answer should list, closest to the key first; a node caps the number at its own limit rather than refusing, and a question without one gets what it always got. A watch at consensus change-inspects the nodes holding it with no peer list at all, which is most of an inspection's bytes.
  - A quiet watch's change inspections back off to half the watch lifetime (`max_watch_inspect_interval_ms` now 300,000) instead of two minutes, so each watch still inspects before its renewal.
  - `get_dht_value` settles nodes that hold different values at the newest sequence number the way `set_dht_value` does: the value most of them hold wins, a tie going to the value held closest to the record key. A local value is replaced by a different one at the same sequence number only with `force_refresh`, and never while a write of the app's own to that subkey is waiting to go out.
  - A change inspection a reconcile asked for never forces another reconcile, and a productive inspection that found more than one changed subkey announces the range to the app without reading it, so a backlog is reported once instead of one subkey per inspect-and-reconcile round.
  - A family's MTU probes go to every candidate peer at once from a background processor instead of one peer at a time inside the network tick, and a peer that says nothing within three of its round trips, or a third of the RPC timeout at least, no longer holds the probe pass for the whole timeout, so a cold start whose only capable peer is down publishes about 3 seconds sooner.
  - An app call on a DHT record stops the background work holding a lock it needs (the watch tick's operations, change inspections, rehydration inspects and the offline write flush) at that work's next network wait instead of waiting for it, so `close_dht_record` and `cancel_dht_watch` return in milliseconds rather than after a watch fanout of several seconds; a `get_dht_value` that has to wait for its subkey still waits for the flush of a pending write to it.
  - Operations on the same subkey of a DHT record wait for each other, as the record lock table intends: since 0.5.2 its subkey locks let a second holder in, so an app set could run alongside the offline write flush or a change inspection's fetch of that subkey.
  - An answer that fails validation, or arrives over the wrong route, fails its question at once as an invalid message from that node instead of waiting out the RPC timeout and counting as a lost question.

- veilid-flutter:
  - **Breaking** `Timestamp.fromString()` is now `Timestamp.fromJson()`, and `toString()` on `Timestamp` and `TimestampDuration` writes human-readable output instead of a raw integer.
  - `VeilidConfigRoutingTable` gains `serveDirectBootstrap`, default false.
  - The `debug-load` feature, which logged the library's load and unload, is gone.
  - **Breaking** Full conversion to native assets and Rust build hooks, so apps drop their own steps for building and linking veilid-flutter and let Flutter do it.
  - **Breaking** CocoaPods support is removed.
  - **Breaking** The minimum Flutter version is 3.47.1, for its build hook support.
  - **Breaking** `VeilidProcessorInterface` is a pure interface that declares `waitPublicInternetReady()` and `waitReadyOrDelay()`, and the new `DefaultVeilidProcessor` implements both by asking core through `waitForAttachmentCondition()` instead of reading the cached connection state.
  - `DHT_TRACE` is a `--dart-define` that logs `DHTRecordPool` and DHT collection operations.
  - `rust-android-gradle` is removed, since it no longer keeps up with AGP and Kotlin.
  - Fix multiple-attach idempotency.
  - `AttachmentState.isDetached` no longer returns true for every state.
  - The `time` dependency drops `local-offset`, whose `num_threads` dependency does not build for iOS on libc 0.2.190.
  - **Breaking** The `VeilidProcessorRetry.retry` extension is gone: its wait for network readiness had no bound, so apps bring their own retry strategy.
  - New `Veilid.changeConfig()` replaces a detached node's configuration for its next attach.
  - A Windows release build can bundle a prebuilt library instead of compiling one: `CCACHE_VEILID_FLUTTER_PREBUILT` gives its URL and `CCACHE_VEILID_FLUTTER_PREBUILT_SHA256` its SHA-256 (also spelled `CARGO_`), and a library that does not match fails the build. A feature selection or a debug build always compiles.
  - Every `main` commit and release tag publishes its Windows x64 library to the veilid project's package registry, as `veilid-flutter-x86_64-pc-windows-msvc` version `<crate version>-<first 12 hex digits of the commit>`, with a `.sha256` and a `manifest.json`.

- veilid-cli:
  - The interactive console no longer exits or floods `thingbuf receiver has closed` when a log burst overruns the line editor.

- veilid-python:
  - `VeilidConfigRoutingTable` gains `serve_direct_bootstrap`, default `False`.
  - Python sources are formatted with `black` and checked with `mypy`.
  - More tests.
  - New `change_config()` replaces a detached node's configuration for its next attach.
  - A late answer to a request whose caller gave up no longer stops the client from receiving, and a message may be up to 1 MB.

- veilid-server:
  - veilid-server answers direct bootstrap requests by default, and the packaged `veilid-server.conf` turns that off, so a bootstrap operator enables `serve_direct_bootstrap` in the config file.
  - The process exits at most 500 ms after the node shuts down instead of waiting out system DNS lookups left on the blocking pool.
  - Server errors use eyre's standard report handler instead of color-eyre's colored reports.

- veilid-tools:
  - **Breaking** `hickory-resolver` is gone, 13 crates with it, and every DNS lookup goes through the platform's own resolver so the system's DNS privacy settings apply, with `Resolver::supports_txt()` reporting whether TXT lookups work and `ResolverError::Unsupported` returned when they do not.
  - **Breaking** `MustJoinSingleFuture`'s `single_spawn()`, `single_spawn_local()`, `check()` and `join()` fail with `SingleFutureLockedError` instead of `()` when another caller is polling the future.
  - **Breaking** `socket2_operation()` takes an `AsFd` socket instead of `AsRawFd` and passes the closure a `&Socket` instead of a `&mut Socket`.
  - **Breaking** `scaled_yielding()` drops its `yield_n` threshold, running the closure inline below `cpu_n` and on the CPU pool from there up.
  - **Breaking** The unsafe `aligned_8_u8_vec_uninit()` and `unaligned_u8_vec_uninit()` are replaced by the safe `zeroed_u8_vec()`.
  - **Breaking** `igd::search_gateway()` and the `igd::Gateway` request methods are async, so dropping one ends its router request at once, and `minreq` is gone.
  - **Breaking** `SplitUrlHost::new` is named `SplitUrlHost::try_new`, since it returns a `Result`.
  - **Breaking** `TrimmedBacktrace` and `TrimBacktraceTrait` are gone, and backtrace captures use `std::backtrace`.
  - **Breaking** `bind_async_tcp_listener()` and `connect_async_tcp_stream()` take an optional TCP segment size, and `limit_tcp_stream_mss()` and `tcp_stream_mss()` lower and read it on a connected stream, on Windows through the socket's IP MTU cap.
  - **Breaking** `NetworkInterface` gains `mtu`, the largest IP packet the interface carries, read on Linux, Android, macOS, iOS and Windows.
  - **Breaking** `AssemblyBuffer::split_message()` takes the largest frame to send, header included, `FRAGMENT_LEN` is gone, and `FRAME_HEADER_LEN` is the frame header's size.
  - **Security** `SplitUrl` and `SplitUrlHost` reject a host holding a non-ASCII character instead of panicking on a byte index inside it.
  - AIMD congestion control, behind the `aimd` feature.
  - Best-effort UDP send-with-TTL in `socket_tools` (`UdpTtlSender`).
  - `SegmentLimitedTcpStream` keeps a TCP stream's segments within a limit its `TcpSegmentLimit` lowers while the stream is open, and `TcpListenerMssHandle` changes the segment size a listener announces to the connections it accepts.
  - `AssemblyBuffer::is_whole_message_frame()` says whether a frame carries a whole message by itself.
  - `bind_async_udp_socket()` sends IPv4 datagrams without DF on Linux and Android, as on other platforms, so a router on a path with a smaller MTU fragments them instead of dropping them.
  - IPv4 send-with-TTL takes effect on macOS and iOS, which send from a twin socket bound to the sender's port, and on Windows, which sets it per packet, so a hole punch from them no longer reaches the far NAT first and makes the reply come from another port.
  - Windows UDP sockets no longer fail their next receive after an ICMP port unreachable or TTL expired, as on other platforms.
  - `tokio` 1.53.3 is the minimum: the platform resolver registers its file descriptor through the I/O-safe `AsyncFd` constructor.
  - `background_may_freeze()` reports whether the platform freezes a process in the background rather than only throttling it.
  - Apple route discovery declares the XNU route-message ABI itself on iOS, where libc 0.2.190 no longer exports it.
  - `EgressIdentity` implements `Serialize` and `Deserialize`.
  - `NetworkInterfaceAddressState::interface_addresses` excludes IPv6 link-local addresses, which stay in `own_addresses`.
  - A send whose interface is down or detaching (`ENETDOWN`, `ENXIO`, `EHOSTDOWN`, `ENETRESET` and their Winsock equivalents) is a `NoConnection` network result instead of an error, on every platform.
  - `libc` 0.2.191 is the floor: it restores the iOS dyld bindings the `backtrace` crate imports, so `debug-locks` builds for iOS again.

- veilid-wasm:
  - **Breaking** The wasm file no longer loads at import time through a top-level await, so you must `await` the default export `init()` (`import init from "veilid-wasm"`) before calling any other export.
  - The TypeScript sources are formatted with `prettier` and linted with `eslint` as part of the repository gates.
  - **Breaking** Thrown errors are `Error` instances instead of plain objects, with `VeilidAPIError` extending `Error` and a subclass per variant, so code that switches on `kind` still works but code that compares a caught error to a plain object must change.
  - **Breaking** A constant whose feature is compiled out throws `Unimplemented` instead of reading as undefined.
  - New `changeConfig()` replaces a detached node's configuration for its next attach.

**Changed in Veilid 0.5.7**

- veilid-core:
  - SECURITY: Deadlock on remote connection flood fixed in `ConnectionManager`. Thanks to quikchaos for the report. The PoC will be included in the `doc/security/poc` folder after the patched version is deployed.
  - HPKE (RFC 9180, base mode) added as `hpke_seal`/`hpke_open` on `CryptoSystem` and all bindings. Implemented for `VLD0` and `NONE` cryptosystems
  - `PublicKey`/`SecretKey` are now explicitly intended to be signing keys (VLD0: Ed25519, VLD1: ML-DSA). New types created for HPKE: `EncapsulationKey`/`DecapsulationKey`/`KemKeyPair` (VLD0: DHKEM-X25519, VLD1: ML-KEM), generated with `generate_kem_keypair` or derived from signing keys via the VLD0-only `encapsulation_key_from_signing_key`/`decapsulation_key_from_signing_secret` bridges
  - `NONE` cryptosystem brought in line with the `CryptoSystem` contract for debugging/testing
  - Crypto unit tests split per suite, enhanced with data-size sweeps, documented-error coverage, and RFC 9180 known-answer tests
  - Numerous route and fanout performance improvements

**Changed in Veilid 0.5.6**

- Optimizations:
  - veilid-wasm and veilid-flutter WASM blob size greatly reduced (-47%)
  - veilid-server 50% smaller, same great taste
  - Duplicated crates consolidated
  - Compile times improved
  - Improved CI build support

- veilid-core:
  - Capnp updated 0.25 -> 0.26
  - Macros introduced in veilid-core to visually improve API functions and strip pasted boilerplate
  - `debug-api` feature added to remove the `debug()` command. Enabled by default with velid-remote-api
    and during all debug builds. Disabled by default for release wasm32 builds.
  - `backtrace` support removed for wasm32 builds due to lack of upstream functionality and wasted space.

- veilid-tools:
  - Absorbed unmaintained crates forks: 'igd', 'stop_token', 'tracing_wasm', 'bugsalot'. Stripped out functionality we don't use. 

- veilid-cli:
  - Forks of `cursive` and related crates eliminated in favor of mainline

**Changed in Veilid 0.5.5**

- Bug Fix Release:
  - Direct replies were broken, resulting in bootstrap failures on nodes with new IP addresses
  - Build failures due to confusing cargo feature unification issues affected simple programs linking in the `veilid-core` rust crate
- Smoketest added for all published crates, testing the most-common, simple, non-veilid-workspace build configurations
  - Cargo feature unification bug reproduced with smoketest on 0.5.4, and is gone in this build.
  - Flutter builds for Windows now use compatible linker options and debug CRT for debug builds. `veilid_integration_test` package now uses Veilid-maintained fork of `xterm` package.

**Changed in Veilid 0.5.4**

- Breaking API Changes:
  - `AttachmentState` enum values have changed to eliminate `OverAttached` state
  - `VeilidUpdate::Attachment` now includes much more visibility into actual attachment conditions and network size estimations
  - Internal per-node RPC statistics removed from `PeerStats` due to excessive verbosity and lack of utility at the API level
  - `footgun` feature renamed to `footgun-nodeid-target`
  - Many `VeilidConfig` settings moved behind the new `footgun-config` option to new `VeilidConfigInternal` struct
  - `Sequencing::NoPreference` is now called `Sequencing::PreferUnordered` in preparation for eventually adding `Sequencing::EnsureUnordered`
  - Many internal API surface reductions for functions that people shouldn't have been using anyway, excessive internal-only macros, and other noise

- Global API Additions:
  - `TransactionNotFound` added to `VeilidAPIError`

- All `VeilidConfig` (and veilid-server `Settings`) structs and fields documented
- Minimum Supported Rust Version (MSRV) is now 1.89.0
- `cargo public-api` support for API stability, via script in `scripts/update_public_api.sh` and `scripts/_check_public_api.sh`
  makes changing the public API require a conscious check to accept diffs and rejects API changes in CI otherwise
- Doc comments added for entire public API surface for `veilid-core`, `veilid-tools`, `veilid-wasm`, and `veilid-remote-api` crates, 
  as well as the `veilid-flutter` Dart bindings and `veilid-python` Python bindings
- `panic` surface reduction

- veilid-cli:
  - veilid-cli now can start when no server is running and connect to it later with the 'connect' command
  - 'change_log_level' command is now just 'log' for simplicity, but both are accepted

- veilid-server:
  - `footgun-settings` feature flag added to simplify configuration, moves dangerous-to-use settings into `core.internal`
  - Removed `--ignore-log-targets` command line option in favor of `-l`/`--log-directives` that uses the new logging `RUST_LOG`-style format
  - added `core.network.address_types` configuration field to enable only specific address types, for example: `["ipv4"]` will only enable IPv4 support in veilid-server. Leaving this blank enables all address types supported by the operating system.

- veilid-core:
  - SECURITY: Reject invalid DHT schema and value signatures on inbound set value and value change notifications, not just at get_dht_value time.
    - Credit to Mr. JDH for the security report (https://github.com/Evelynkaz)
  - Added record flush on DHT record close to ensure closed records are fully flushed to the TableStore
  - Rehydration, change inspection, and watch value notifications are now transaction-aware
  - Formatting functions moved to veilid-tools, gets garbage nobody cares about out of the public API
  - Routing domain publication improvements, better logic for dial info confirmation
  - `NetworkManager`'s `Network` abstraction is now a `PlatformNetwork` trait, implemented as `NativeNetwork` and `WasmNetwork`
  - Added relay compiler to unify how relay requirements are calculated
  - Improved address change detection
  - Improved route optimizer, routes no longer are completely reset when switching relays or publishing peer info
  - Fixed UPNP support
  - Fixes for handling SLAAC, APIPA, CLAT46/XLAT464, and earlier detection of CGNAT. Big day for acroynms.
  - Dynamic source Binding for low level protocols to ensure non-temporary addresses are used for outbound flows.
  - Added `OnlineDetector` to `NetworkManager` to enhance detecting switching networks. Things are now reactive to low level address switches rather than tick-based. Next up, OS-level subscriptions to routing table and interface table changes.
  - Improved unit tests
  - (@neequ57) Extract compression from crypto
  - (@kirks) Fix recursion limit for Rust 1.94.0
  - Update keyring-manager to 0.8.2 to fix secret-service connection problems
  - Add flush_dht_record to VeilidAPI interface
  - `NodeCount` aligned u64 type added
  - implemented TransportType and transport-level stats for nodes
  - Simplified route testing, eliminated route exhaustion problems
  - Loopback addresses added to local network routing domain to permit proxy use (such as proxychains4 + tor)
    - If you want to put a proxy in your proxy so you can proxy while you proxy:
      * Set up proxychains4 (`brew install proxychains4` and turn off DNS proxying) 
      * `proxychains4 veilid-server --set-config core.network.privacy.require_inbound_relay=true`
  - Connection table now LRUs across all protocols. Connection count maximum now applies across all connection-oriented protocols.
  - Fixed bug where `api_startup` did not remove its `ApiTracingLayer` callback upon failure.
  - Added `max_concurrent_operations` to VeilidConfigDHT to offer a simple way to cap concurrent DHT operations for automatic throttling.
  - Proper IPv6 support for `wasm32-unknown-unknown` target.
  - Added `record purge <scope> --keys <list>` debug command option to allow purging local or remote comma-delimited lists of record keys from the record store. Bypasses the normal `record delete` mechanism that only deletes local records.

- veilid-tools:
  - Formatting cleanup across the board, primarily for debugging purposes
  - (@hamanasu_ruka) Break up monolithic unit tests
  - Async lock deadlock detection (debug-locks feature) streamlined
  - AsyncKeyedCache added to async_tools
  - Added FlapDetector - hooked in to a bunch of places in veilid-core to check for connection, online, relay, and route flaps

- veilid-flutter:
  - Add `LogFixture` to `veilid_test` package
  - Dart/WASM now lives here next to Dart/FFI
  - Improved unit tests
  - Consolidation of retry logic for VeilidAPI-level operations

- veilid-wasm:
  - Dart WASM implementation moved to veilid-flutter, this now only has the Javascript/Typescript bindings
  - Improved unit tests
  - No more wasm-pack. All tests use wasm-bindgen-test / wdio directly.
  - Fixed route leak in tests
  - Switched to chromedriver for tests

**Changed in Veilid 0.5.3**

- Global Breaking Changes:
  - API Breaking Change: new syntax for `VeilidAPI::new_custom_private_route`, using `PrivateSpec`
  - Feature Breaking Change: OpenTelemetry crates no longer support async-std, so we are removing support for the `opentelemetry` feature if you choose use the `rt-async-std` feature. The `opentelemetry` feature is still available for the Tokio executor as enabled by the `rt-tokio` feature.

- veilid-core:
  - Security: 
    - Validate DH operations are contributory, ensuring DH checks for dangerous identity output
    - Ensure WebSocket connections don't ever resolve hostnames except for bootstrap
  - API Breaking Changes:
    - `Sequencing`, `Stability` and `SafetySelection` Rust API `Default`s have changed to match bindings API defaults
  - Improved DHT transactions speed and reliability
  - Switched to using the `bytes` crate internally for network code, closes [#414](https://gitlab.com/veilid/veilid/-/issues/414)
  - 'PublicInternet Ready' state now requires a complete bootstrap when cold-starting. Fixes connectivity problems for fresh nodes.
  - Bootstrap now uses wide search for nodes as well as closest peers search
  - Crypto improvements
    - Crypto moved to thread pool on native platforms
    - Crypto in-place encrypt/signing
  - Routing table improvements
    - Route compilation cache made more efficient
    - Refactored to use more granular locking
    - Prefer relay-capable last hop for route allocation
    - Add two-pass route allocation to the route allocation system
    - Implemented entry snapshots to eliminate race conditions and sort ordering problems
  - Routing table ping validator improvements
    - Replace polling TickTask with a channel-driven background processor
    - Add priority groups 
    - Add callback chains to enable multi-step route testing
  - RPC processor improvements
    - Add per-destination lost answer tracking
    - Fix a bug where we were counting lost answers multiple times for the same route
  - Storage manager improvements
    - Early consensus exit optimization for transactions
    - Transaction management tuning
  - NetworkManager improvements
    - ConnectionTable fix for connection inactivity timeout

- veilid-tools:
  - Added synchronous TagLock
  - Removed HashAtom
  - Removed MutableFuture

- veilid-flutter:
  - API Breaking Changes: 
      - logging configuration updated to support new directive format

- veilid-wasm:
  - API Breaking Changes: 
      - logging configuration updated to support new directive format
  - Update to veilid-tracing-wasm 0.2.0


**Changed in Veilid 0.5.2**

- _BREAKING API CHANGES_:
  - Added support for `RUST_LOG` environment variable and its syntax and deprecated the `log ignore` syntax
  - `change_log_level`/`changeLogLevel` now takes in a log directive string in `RUST_LOG` / `EnvLogger` format rather than a single log level
    - To upgrade, change the `log_level` parameter from a single level name (`VeilidConfigLogLevel::Debug`) to something like `"#enabled=debug`
  - `VeilidLayerFilter` now has a more flexible powerful and supports setting a default configuration in its constructor
- Minimum Supported Rust Version (MSRV) is now 1.88.0
- Clean up dependencies across the board
- Revamped logging everywhere
- veilid-core:
  - Added `VeilidTracing` simple log setup and extended examples to use it
  - Log facility 'tags' (groups) starting with `#` are now supported
  - `VeilidComponent` now registers log facilities and tags at initialization time in prep for an eventual crate split
  - Change route test failure during allocation to a `VeilidAPIError::TryAgain`
  - Fix connectivity problem resulting in `couldn't look up relay for inbound relay` in logs
  - Update KeyValueDB to 0.1.5 to fix sqlite WAL file explosion
  - Faster DialInfo detection, faster IPV4/IPV6 routability check
  - Fix for LimitedSize underflow in StorageManager
  - Add `instrument` feature to enable tracing instrumentation (on by default)
  - Remove `intf` module, moving remaining platform-specific code to `veilid-tools`
  - Several concurrency and correctness fixes for `StorageManager`'s internal `TableDB` transactions

- veilid-tools:
  - Add `debug-locks` feature implementation for AsyncRwLock, AsyncSemaphore, AsyncMutex
  - Improved `interval()` tick accuracy
  - Fixed subkeys aging out on a record individually [#432](https://gitlab.com/veilid/veilid/-/issues/432)

- veilid-wasm:
  - Reduced size of veilid_wasm_bg.wasm by 25% by disabling tracing instrumentation for this platform

**Changed in Veilid 0.5.1**

- No veilid-server/cli or packages are being built for this release. This is solely a Rust crates update.
- Bugfixes:
  - Fix docs building for veilid-remote-api crate
  - Add missing files
  - Clean up .gitignore and remove files that did not need to be committed
  - Fix docs.rs / build_docs.sh/bat scripts to better check for docs problems
  - Improve veilid-flutter schema validation code (will go away eventually)
  - Bump minimum flutter version to 3.35.0

**Changed in Veilid 0.5.0**

- _0.5.0 BREAKING CHANGES_
  - Many on-the-wire encoding changes: [#453](https://gitlab.com/veilid/veilid/-/issues/453)
  - Rename crypto types: [#463](https://gitlab.com/veilid/veilid/-/issues/463)
    - {CryptoBytes} -> Bare{CryptoBytes}
    - Typed{CryptoBytes} -> {CryptoBytes} [#465](https://gitlab.com/veilid/veilid/-/issues/465)
  - Handle NotInSchema:
    -  switch match alternative failures and list decode failures to `RPCError::Ignore` from `RPCError::Protocol` to keep from punishing newer nodes
    - add `.ignore_ok()` trait that makes it easy to 'soft fail' `RPCError::Ignore`
    - add `rpc_ignore_*` macros
    - canonicalize coders' handling of lengths
  - Envelope and Receipt versions are now FOURCC codes to allow for parallel development of on-the-wire protocols
  - Rename `api_startup_config` -> `api_startup`, and remove the ability to configure the VeilidAPI using a callback.

- _BREAKING API CHANGES_:
  - Eliminated DHTW capability, merged into DHTV capability, now there is only one DHT enabling/disabling capability and all operations are part of it
  - Crypto / CryptoSystem functions now use typed keys everywhere (#483)
  - Eliminated 'best' `CryptoKind` concept, crypto kinds must now be explicitly stated, otherwise upgrades of veilid-core that change the 'best' `CryptoKind` could break functionality silently.
  - Encryption is enabled by default for all DHT operations, closes [#300](https://gitlab.com/veilid/veilid/-/issues/300) (@neequ57)
  - Deprecation of WSS config and removal of Application config
  - Use `VeilidAPIError` type for `ProtectedStore` functions [#480](https://gitlab.com/veilid/veilid/-/issues/480)
  - `get_dht_record_key` moved to `VeilidAPI` from `RoutingContext`, because it does not use the network
  - `TableDBTransaction::store` and `delete` are now async
  - WASM identifier names changed to camelCase, numerous structural changes, many types are now classes, no more string marshaling

- veilid-core:
  - Hop counts removed from private routes [#466](https://gitlab.com/veilid/veilid/-/issues/466)
  - Added `SequenceOrdering` enum to represent ordering mode for protocols rather than a bool
  - `RecordKey`s are now validated on both server side and client side of DHT RPC operations, closes [#299](https://gitlab.com/veilid/veilid/-/issues/299)
  - Revert punishment for FailedToVerifySenderPeerInfo, with a better peer info filter, fixes [#470](https://gitlab.com/veilid/veilid/-/issues/470)
  - Update keyring-manager to eliminate licensing issue
  - Added 'tick lag' detection to check for missed watch updates
  - Added 'DHT Widening', separates consensus width (server side dht operation acceptance) from consensus count (client side), fixes [#435](https://gitlab.com/veilid/veilid/-/issues/435)
  - Deprecation of WSS protocol, closes [#487](https://gitlab.com/veilid/veilid/-/issues/487)
  - Move node id and public key init to routing table
  - VeilidConfig is now read-only and no longer requires a lock [#485](https://gitlab.com/veilid/veilid/-/issues/485)
  - Make `Timestamp::now()` monotonically increasing
  - Add `EnvelopeInfo` tracking, implementing clock drift detection [#416](https://gitlab.com/veilid/veilid/-/issues/416)
  - Improved hairpin NAT handling for contact methods
  - `StorageManager::set_value` now caches if it sent a descriptor [#203](https://gitlab.com/veilid/veilid/-/issues/203)
  - `ValueSeqNum` is now a type-safe newtype struct
  - Safety routes are now twice the length when accessing a node directly [#361](https://gitlab.com/veilid/veilid/-/issues/361)
  - DHT Transactions support now exists. Multiple subkeys over multiple records can not be operated on in a single atomic commit. [#364](https://gitlab.com/veilid/veilid/-/issues/364)
  - RecordStore and StorageManager global locks have been split into per-subkey locks allowing greater parallelism.
  - RecordStore index now has more metadata and will require a one-time 'repair' delay to the TableDB upon starting a node if it contained record data created before 0.5.0. If the node doesn't start immediately, don't panic. :)

- veilid_flutter:
  - Android NDK version requirement is now 28.2.13676358
  - Android Gradle version is now 8.13.2

- veilid-python:
  - Migrated to 'uv' from 'poetry'

- veilid-server:
  - Improved ergonomics for `--dump-txt-record`, `--generate-key-pairs`, and `--set-key-pairs`

- veilid-tools:
  - Rename `get_timestamp()` -> `get_raw_timestamp()` for clarity

- veilid-wasm:
  - Reorganize crate and add `js` and `dart` features to enable different target bindings
  - Revamp bindings for `js` to eliminate excessive strings and improve marshaling

**Changed in Veilid 0.4.8**

- _BREAKING API CHANGES_:
  - set_dht_value now accepts a new flag called `allow_offline`, which defaults to `true`.
    - The previous `writer: Option<KeyPair>` argument position is now `options: Option<SetDHTValueOptions>`
    - This will only be a breaking change for anyone utilizing the previous `writer` argument.
    - `writer` is now a member of `SetDHTValueOptions`, alongside the new `allow_offline` property.

- veilid-core:
  - Add private route example
  - Add `require_inbound_relay` option in VeilidConfig. Default is false, but if enabled, forces OutboundOnly/InboundRelay mode. Can be used as an extra layer of IP address obscurity for some threat models. (@neequ57)
  - Fix crash when peer info has missing or unsupported node ids
  - Add 'auto' mode for detect_address_changes
  - Improved `TypedXXX` conversion traits, including to and from `Vec<u8>`
  - Ensure utf8 replacement characters are never emitted in logs
  - Export `CRYPTO_KIND_VLD0` constant

- veilid-python:
  - Correction of type hints
  - Fixed transaction `__aexit__` to properly rollback transaction if not committed, and not raise an exception

- veilid-server:
  - Use `detect_address_changes: auto` by default

**Changed in Veilid 0.4.7**

- _BREAKING API CHANGES_:
  - Improve type-safety by splitting out type aliases for `FourCC` and `CryptoKey` types into distinct "newtypes". Previously all instances of these types we just type aliases, and could be used interchangeably when in reality they aren't interchangeable. This prevents developers from accidentally passing the wrong type into the API, such as a `SecretKey` when you meant to pass a `PublicKey`. Function signatures have been updated to use the correct types, so code will need to be updated accordingly.
    - `FourCC` has been separated into:
      - `CryptoKind`
      - `VeilidCapability`
    - `CryptoKey` has been separated into:
      - `PublicKey`
      - `SecretKey`
        - This was also renamed from `Secret` -> `SecretKey`, as well as the associated `TypedSecretKey` and `TypedSecretKeyGroup`.
      - `Signature`
      - `Nonce`
      - `HashDigest`
      - `RouteId`
      - `RecordKey` (New) - represents a DHT Record Key
      - `NodeId` (New)
    - Note: Right now veilid-wasm is not benefiting from the type-safety yet We are looking at improving this in the future.
  - Remove a number of unintentional exports from `veilid-core`:
    - `veilid_core::vld0` module - Use the functions exported by `VeilidAPI.crypto()` instead.
      - `veilid_core::vld0::vld0_generate_keypair` 
      - `veilid_core::vld0::CryptoSystemVLD0`
      - `veilid_core::vld0::CRYPTO_KIND_VLD0`
    - Other internals that were not intended for API consumers:
      - `veilid_core::RoutingContextUnlockedInner`
      - `veilid_core::veilid_capnp`
      - `veilid_core::Envelope`
      - `veilid_core::Receipt`
      - `veilid_core::Crypto`
  - The "JSON API" that was exported by `veilid-core` has now been moved into its own crate called [`veilid-remote-api`](https://crates.io/crates/veilid-remote-api).

- veilid-core:
  - Update KeyValueDB to 0.1.3
  - Inspect watched records for value changes made while offline when coming back online
  - Additional table store unit test
  - Eliminate `unwrap()` in `best_node_id()` (fixes crash)

- veilid-tools:
  - Add `HashAtom<>` type for hashing references by identity

- veilid-flutter:
  - Fix exception handling for WASM

**Changed in Veilid 0.4.6**

- Updated pinned Rust version to 1.86

**Changed in Veilid 0.4.5**

- Update capnproto version to 1.1.0
- *BREAKING API CHANGE*:
  - watch_dht_values() now returns a bool rather than an expiration timestamp. Expiration renewal is now managed by veilid-core internally. Apps no longer need to renew watches!
  - inspect_dht_record() and cancel_dht_watch() now take an Option<ValueSubkeyRangeSet> instead of just a ValueSubkeyRangeSet, to make things easier for automatic binding generation someday and to remove ambiguities about the semantics of the default empty set.
  - DHTRecordReport now uses a `Vec<Option<ValueSubkey>>` for seq lists, rather than using the 'ValueSubkey::MAX' sentinel value (0xFFFFFFFF) to represent a missing subkey
  - Renamed config structs to better describe their purpose, and remove "Inner" from a struct that's being exposed via the API. [!402](https://gitlab.com/veilid/veilid/-/merge_requests/402)
    - `VeilidConfig` -> `VeilidStartupOptions`
    - `VeilidConfigInner` -> `VeilidConfig`
  - Gate insecure capabilities behind the new `footgun` feature flag [#394](https://gitlab.com/veilid/veilid/-/issues/394), which is disabled by default. [!400](https://gitlab.com/veilid/veilid/-/merge_requests/400)
    - Calling `app_call` or `app_message` with a `NodeId` target will throw an error. Use a `PrivateRoute` target instead.
    - Creating an `Unsafe` routing context will throw and error. Use `Safe` routing context instead.
    - Any AppCall or AppMessage sent from a direct NodeId will not be received.

- veilid-core:
  - **Security** Signed bootstrap v1 added which closes [#293](https://gitlab.com/veilid/veilid/-/issues/293)
  - Allow shutdown even if tables are closed
  - New, more robust, watchvalue implementation
  - Consensus is now counted from the nodes closest to the key, excluding attempts that have failed, but including new nodes that show up, requiring N out of the M closest nodes to have succeeded and all have been attempted.
  - Watching a node now also triggers an background inspection+valueget to detect if values have changed online
  - Fanout queue disqualifaction for distance-based rejections reimplemented
  - Local rehydration implemented. DHT record subkey data that does not have sufficient consensus online is re-pushed to keep it alive when records are opened.
  - Direct bootstrap v0 now filters out Relayed nodes correctly
  - Closed issue [#400](https://gitlab.com/veilid/veilid/-/issues/400)
  - Closed issue [#377](https://gitlab.com/veilid/veilid/-/issues/377)
  - Add the `veilid_features()` API, which lists the compile-time features that were enabled when `veilid-core` was built (available in language bindings as well). [#401](https://gitlab.com/veilid/veilid/-/issues/400)
  - When `veilid-core` starts up, log the version number, and the compile-time features that were enabled when it was built. [#401](https://gitlab.com/veilid/veilid/-/issues/400)
  - Closed issue [#448](https://gitlab.com/veilid/veilid/-/issues/448)
  - Add background flush for routing table and route spec store, to address issue [#449](https://gitlab.com/veilid/veilid/-/issues/449)

- veilid-flutter:
  - Bindings updated for API changes
  - Corrosion version in cmake build for linux and windows updated to 0.5.1: [#447](https://gitlab.com/veilid/veilid/-/issues/447)
  - Expose the isShutdown API: [!392](https://gitlab.com/veilid/veilid/-/merge_requests/392)

- veilid-python:
  - Fix type assertion bug in watch_dht_values
  - Update WatchValue integration tests
  - Expose the is_shutdown API [!392](https://gitlab.com/veilid/veilid/-/merge_requests/392)

- veilid-server:
  - Put tokio-console behind a feature flag. Closed issue [#274]( https://gitlab.com/veilid/veilid/-/issues/274)
  - Fixed 'daemon' mode `-d` option. Closed issue [#360](https://gitlab.com/veilid/veilid/-/issues/360)

- veilid-wasm:
  - **Breaking** Properly generate TypeScript types for `ValueSubkeyRangeSet`, which would previously resolve to `any`. This is breaking since it can cause type errors to correctly surface in existing applications. [!397](https://gitlab.com/veilid/veilid/-/merge_requests/397)
  - **Breaking** `startupCore()` and `defaultConfig()` now use config objects instead of stringified JSON.
    - `veilidClient.startupCore(callback, JSON.stringify(config))` now becomes `veilidClient.startupCore(callback, config)`. [!402](https://gitlab.com/veilid/veilid/-/merge_requests/402)
    - `JSON.parse(veilidClient.defaultConfig())` is now `veilidClient.defaultConfig()`
    - The `VeilidConfigInner` type is now `VeilidConfig`.
  - Expose the isShutdown API: [#392](https://gitlab.com/veilid/veilid/-/merge_requests/392)

- CI:
  - Ensure Cargo.lock is up-to-date during CI pipelines

**Changed in Veilid 0.4.4**

- veilid-core:
  - Improved termination conditions for DiscoveryContext, faster shutdown acknowledgment
  - Network class discovery from 25s to 5s in worst case
  - Moved some futures to the heap with Box::pin to reduce risk of stack overflows
  - Restore punishment for FailedToVerifySenderPeerInfo
  - Convert inbound relay loop notification from an error to a NetworkResult
  - Relays were being picked before we had good stats on their performance. Relays are now optimized to be within the top 25% of p90 latencies
  - LatencyStats now have median 80% average, p90, p75, and p50 latency numbers
  - RPC Answer stats now are split by 'ordered' and 'unordered' protocols
  - Unordered protocols are given a higher tolerance for lost questions before being marked 'unreliable'
  - Fix race condition in TTL code for udp hole punching
  - Correct use of IPV6_UNICAST_HOPS socket option instead of IP_TTL for ipv6 sockets
  - Move relaying to a relay worker pool
  - Connection table and RPC locking improvements
  - Remove TCP connection retries
  - Deadlock bugfix in relay code
  - 'Finding more nodes' tasks now use PreferOrdered sequencing
  - Fix deadlock in veilid_api duration testing
  - Correct offline_subkey_writes inflight reporting
  - Simplify UPNP code, eliminate useless high-latency retry logic, and ensure it doesn't conflict with NAT detection

- veilid-python:
  - Fix async context implementation so it works correctly when re-entrant (nested `async with` blocks)
  - Validate types for API calls, will raise an assertion error if type is unexpected
  - Add more stress tests

- veilid-wasm:
  - fork tracing-wasm and add output filter for veilid log key

- general:
  - Added some pedantic clippy lints, moved lint config into workspace Cargo.toml
  - Fixed async-std support
  - Added windows and macos clippy lints to CI via earthfile
  - Upgrade all pinned crate dependencies

- CI/CD
  - Began migrating build/package/deploy scripts from shell to Python
  - Switched from using multiple build machines (one per arch/package) to a single machine running the +package-linux Earthly target

- _community contributions_
  - (veilid-wasm) Remove wee_alloc due to memory leak issues -- @bmv437
  - (veilid-core) Fix geolocation feature after recent refactor -- @neequ57
  - (documentation) Update README code snippet to work with current version -- @carvilsi


**Changed in Veilid 0.4.3**

This release exists without changes to the Veilid codebase in order to test fixes to the CI/CD release pipeline.

- CI/CD
  - Fixed broken RPM packaging

**Changed in Veilid 0.4.2**

veilid-core:
- (neequ57) Merged !330 - geolocation feature (off by default) to allow excluding/denylisting route nodes based on geography
- (evelyn) Merged !267 - adding the ability to create dht records with a specified owner key (rust only currently)
- (rivka segan) Merged !335 - fix logic error that used wss when not tls
- New startup/shutdown initialization and component system
- Logs are tagged with the program_name+namespace they are collected in
  - No more per-facility log macros, one unified `veilid_log!()` macro for all events
  - Switch between subnodes in veilid-cli switches which logs you're viewing
  - All global logs and subnode 0 go to console, all other subnodes are accessible via veilid-cli
- Major refactor to add VeilidComponentRegistry as the 'owner' of all components
  - Access to components now uses scoped guards for lifetime management rather than loose Arc clones
  - VeilidComponent trait makes adding common per-component functions easier
  - Unified initialize, post-initialize, pre-terminate, and terminate phase harness
- AsyncCryptoSystemGuard added to make heavy operations happier in async environments
- UDP hole punch needed TTL setting to keep routers from incorrectly making conntracks
- Public address detection was getting stuck in a lock contention, regression from refactor
- PeerInfo caching to eliminate some repeated cloning
- NodeContactMethod cache improvements
- Symmetric NAT and NetworkClass::OutboundOnly were broken. When routing domain address types are known, but there is no dialinfo, that should be OutboundOnly and not Invalid. It's valid to have no dialinfo. Added network class 'confirmation'.

veilid-tools:
- replaced deprecated serde_yaml crate with maintained serde_yaml_ng crate
- Start of VirtualRouter network virtualization
  - standalone virtual router binary in veilid-tools (`cargo run --bin virtual_router`)
  - IAC-style configuration system for repeatable virtualized network generation

veilid-flutter:
- (kimmy.zip) Merged !343 - Fixes for Windows Flutter build
- Android NDK version requirement is now 27.0.12077973
- Android Gradle version is now 8.10.2, with a minimum of 8.8.0
- Android Java version is now 17
- rust-android-gradle upgraded to 0.9.6
- Kotlin version is now 1.9.25
- API added for create_dht_record with 'owner'
  - Breaking change: [!353](https://gitlab.com/veilid/veilid/-/merge_requests/353)

veilid-cli:
- You can now switch between subnodes easily with the 'connect <N>' command where N is the subnode id

veilid-server:
- You can now run multiple subnodes concurrently in the same process with `--subnode_count=N`
- Up to 256 concurrent each of TCP and WebSocket connections now, up from 32
- Turn off detect_address_changes and upnp by default

veilid-wasm:
- (bgrift) Merged [!352](https://gitlab.com/veilid/veilid/-/merge_requests/352) - WASM supports owner on createDhtRecord, also added the getDhtRecordKey function
  - Breaking change: [!352](https://gitlab.com/veilid/veilid/-/merge_requests/352)
- Fixes for heavy sync crypto code, optimizations in debug mode, wasm tests went from 731 seconds to 112 seconds

veilid-python:
- API added for create_dht_record with 'owner'
  - Breaking change:[!353](https://gitlab.com/veilid/veilid/-/merge_requests/353)
- api_connector() now attempts IPC connection to veilid-server before trying port 5959 tcp
- dependencies corrected for pypi package

CICD:
- Updated build machines
  - OS Updates
  - Rust to 1.81
  - Python to 3.12
  - Earthly to 0.8.15
 
general:
- Fix rust-version into workspace cargo.toml
- Earthfile update to 0.8
- Earthfile cache efficiency fixes

**Changed in Veilid 0.4.1**

- Implement top level event bus to do asynchronous lock-free communication between subsystems
- Fix deadlock in socket address change event
- Fix deadlock in peer info change event
- Fix incorrect node info equivalence check
- Ping relays every second instead of every 10 seconds
- MR [!328](https://gitlab.com/veilid/veilid/-/merge_requests/328) 'tiny improvements'

**Changed in Veilid 0.4.0**

- RFC-0001: Constrain DHT Subkey Size, issue #406

- DialInfo detection issues:
  - Add a publish() as well as a commit() for routing domain editor
  - Should only publish our peer info after we're sure we done editing it (end of public address detection task)
  - Publish should happen after relay selection as well
  - Publish should happen if the relay's PeerInfo has changed
  - Publish should not do anything if the PeerInfo hasn't changed
  - `PeerInfo` -> `Arc<PeerInfo>` everywhere to minimize deep clones and ensure read-only PeerInfo
  - Routing domain editing is now more atomic
  - When a node selects a relay it now immediately protects its connections.
  - Made dial info port (for port restricted nat) more resilient to changes, in the case there are multiple mappings
  - Relays that drop protected connections should be deprioritized for relay selection (table saturation detection)
  - clear_network_callback in do_public_dial_info_check is a kludge, removed
  - Raised the bar for DialInfo changes when its just the port
  - Pinging node on the same network works again
  - resolve_node() never returns a dead node even when we want to try to communicate with it again
  - Removed 'bad public address' detection as it wasn't working anyway
  - Added separate parallelism lanes for relay keepalive pings from peer liveness check pings, as they are higher priority
  - Change send_data to always check cache for contact method first instead of going with filtered active flows first, avoids choosing UDP when a preferable TCP connection could be made
  - Nodes that are not relay capable should drop relayed packets

- DHT issues:
  - Make SetValue more likely to succeed by accepting a GetValue consensus if a full SetValue consensus is not reached.
  - Offline subkey writes are cleared too fast and should be thought as 'subkeys not yet synchronized'
  - If set_value is partial / in-flight, it should still be in offline_subkey_writes
  - Make inflight_subkey_writes list and probably some bit for 'written_while_inflight' so we don't clear the offline_subkey_writes until they're really written

- Networking:
  - Fix TIME_WAIT states on Windows
  - Attempt to give priority to relaying flows

- UI:
  - Make veilid-cli display the connection state when reconnecting, and exit more cleanly on ctrl-c
  - Added 'uptime' veilid-cli debug command

- Misc:
  - Fixes for python DHT test

- API Additions:
  - VeilidConfigInner::new parameterization for easier config from rust apps
  - Remove veilid-server specific paths from veilid-core defaults
  - Lots more stats about node performance in PeerStats
  - Uptime stats in VeilidStateAttachment/VeilidUpdateAttachment, issue #317

**Changed in Veilid 0.3.4**
- Crates updates
  - Update crates to newer versions
  - Remove veilid-async-tungstenite and veilid-async-tls crates as they are no longer needed
- Fix startup/shutdown/attach/detach
  - Improved resource accounting
  - Locked startup/shutdown mechanism
  - Perfetto profiler output for ui.perfetto.dev
  - SO_LINGER(0) re-enabled to eliminate TIME_WAIT on restart/detach/attach
  (this may cause noise for WASM in browsers when websockets are RST dropped rather than handshake closed, we will deal with this later)
- _Community Contributions_
  - prototype script to install / run a veilid-server node within a UnifyOS device (tested on a unify dream machine pro SE) @Vyrus-001

**Changed in Veilid 0.3.3**
- Fix set_dht_value and watch_value
  - Watching values incorrectly categorized 'owner' keys as anonymous watchers
  - Setting a dht value with the same sequence number as what is on the network, but with a conflicting value, did not result in the current value being returned to the api caller as it should have been
- DHT cleanup
  - Proper application of DHT capabilities
  - Fanout debugging log target
  - Performance measurement / timing of veilid_api log target
- Fix DHT Rust integration test
- ValueChanged Optional
  - Allow value changed data to be optional in rpc schema
  - Make ValueChanged update no longer happen when value hasn't changed or is older
- Implement closest peers refresh
  - Implement closest peers refresh. Closes issue #372.
  - Find_self/find_target can use capability check
  - Fix offline subkey write reporting to eliminate spurious notifications
  - Add more detail to public address check
- Improved punishment and state
  - Create 'reasons' for dead and unreliable states
  - Make 'punished' its own state
  - Closes issue [#281](https://gitlab.com/veilid/veilid/-/issues/281)
  - Fixes an issue with reliable nodes being marked as 'dead' unjustly
- _Community Contributions_
  - Fixed memory leak in Windows DNS resolver @kyanha

**Changed in Veilid 0.3.2**
- DHT optimization and bugfixes
  - Support for offline write status in DHTRecordReport
  - Fix deprecated functions
  - Improve fanout seeding to ensure records are reached as quickly as possible
- Native IPV4-IPV6 bridging support
  - fix bug where messages sent to a private route without a safety route would not receive replies
  - fix verbose-tracing feature flag
  - improve route allocation to avoid co-located nodes
  - fix contact method for nodes on the same IP block
  - add support for maintaining AddressType-translation relays
- Removed NDK related hotfix, as this has been integrated into cargo-ndk already
- Open sourced the CI/CD build scripts
- Fixes for WatchValue
- Refactor low level network

**Changed in Veilid 0.3.1**
- DHT cleanup
  - Proper application of DHT capabilities
  - Fanout debugging log target
  - Performance measurement / timing of veilid_api log target
- ValueChanged Optional
  - Allow value changed data to be optional in rpc schema
  - Make valuechanged update no longer happen when value hasn't changed or is older
- Clippy fixes and cleanup
- _Community Contributions_
  - Changed VeilidAPI::parse_as_target to a sync function -- @sashanoraa
  - fix dht rust integration test -- @ssurovsev

**Changed in Veilid 0.3.0**
- API BREAKING CHANGES: 
  - WatchValue RPC support
  - InspectRecord RPC support
  - RoutingContext now defaults to Reliable and EnsureOrdered modes
  - generate_shared_secret added that abstracts DH and ensures domain separation
- Closed [#357](https://gitlab.com/veilid/veilid/-/issues/357) - AppCall and AppMessage now have private route information
- Logging: Log facilities now can be enabled and disabled at runtime
- Logging: Log facility added for DHT, network results, and API calls
- CLI: Closed [#358](https://gitlab.com/veilid/veilid/-/issues/357) - veilid-cli now has 'interactive' (-i), 'log viewer' (-l) and 'execute command' (-e) command line options
- Testing: veilid-flutter now has integration tests of its own that work like the veilid-python unit tests
- Network: Failures to hole-punch UDP or reverse-connect TCP or UDP now falls back to inbound relaying
- Bugfix: Signal handling for unix-like platforms was not handling SIGTERM correctly
- Bugfix: Restarting veilid-server quickly might result in failures to bind()
- Bugfix: Closed [#359](https://gitlab.com/veilid/veilid/-/issues/357) - Block node identity from DHT record schema owner/writer
- Bugfix: Closed [#355](https://gitlab.com/veilid/veilid/-/issues/357) - Fixed memory error reading macos/ios interfaces list
- _Community Contributions_
  - Made private route allocation bidirectional by default @kyanha
  - Use $CI_REGISTRY_IMAGE for the registry path @SalvatoreT
  - Add VeilidConfigInner-based VeilidAPI startup @SalvatoreT
  - rebrand trust-dns-resolver to hickory-resolver @kyanha

**Changed in Veilid 0.2.5**
- API BREAKING CHANGES: 
  - on `RoutingContext`: `with_privacy()` renamed to `with_default_safety()`
  - on `RoutingContext`: `with_custom_privacy()` renamed to `with_safety()`
  - on `RoutingContext`: `safety()` method added that returns the current `SafetySelection`
  - Routing contexts are now safety-route-enabled by default. To disable, use `with_safety()` with `SafetySelection::Unsafe`.
- WASM now works better with updated connection manager code
- Async-std flavor of veilid-core now builds correctly again
- Safety route allocation is bidirectional
- Connection table LRU cache now has protection for relays and in-use RPC question/answers
- Dead route notifications are now sent only for manually allocated routes
- Allocated routes that fail tests now have their nodes marked as 'failure to send' so they go 'unreliable' and get re-tested. Also the same route will not immediately be reallocated as a result.
- DHT tests ported from Python to Rust
- Rustls updated to latest release
- Protected connections (such as relays) that drop result in marking the node as 'failure to send' so a different relay gets chosen

**Changed in Veilid 0.2.4**
- Fixed issue with client API failing when ipv6 was disabled
- Android fixed so it can move out of invalid network state
- Numerous WASM binding fixes
- IGD/UPNP fixes for Windows
- Reduce network downtime when local ip addresses change (ipv6 temporary addresses)
- Fix support for Android emulator
- Bootstrap is more robust in environments where some DialInfo won't work, like inbound UDP being firewalled off
- CLI has timestamps in the log output
- Base64 fixes for encoding
- IPv6 capability detection for native platforms

**Changed in Veilid 0.2.3**
- Security fix for WS denial of service
- Support for latest Rust 1.72

**Changed in Veilid 0.2.2**
- Capnproto 1.0.1 + Protobuf 24.3
- DHT set/get correctness fixes
- Connection table fixes
- Node resolution fixes
- More debugging commands (AppMessage, AppCall, resolve, better NodeInfo, etc)
- Reverse connect for WASM nodes
- Better Typescript types for WASM
- Various script and environment cleanups
- Earthly build for aarch64 RPM
- Much improved and faster public address detection

**Changes in Veilid 0.2.1**
- Crates are separated and publishable
- First publication of veilid-core with docs to crates.io and docs.rs
- Avoid large logs of 127.0.0.1:5959 attack payloads
- Use `getrandom` in WASM for RNG
- Increase privacy for WASM builds by rewriting internal paths
- Translations
- Fix python update schema script
- Earthfile cleanup

**Changes in Veilid 0.2.0**
- Rustdoc builds now
- API visibility changes
- Android JNI update
- Fix DHT record data housekeeping
- Public address detection improvement
- Manual port forwarding detection 
- lock_api dependency fix
- DialInfo failover when some dial info does not work

Note: Windows builds may be broken in this release. Please test and let us know by opening an issue.

**Changes in Veilid 0.1.10**
- BREAKING CHANGE: ALL MUST UPDATE
  * VLD0 now adds a BLAKE3 hash round on the DH output to further separate it from the raw key exchange
  * Bootstraps are fixed now due to DH issue
- Windows crate update caused build and nul termination issues for DNS resolver
- Fix for network key on the veilid-server command line
- Strict verification for Ed25519 enabled
- Domain separation for VLD0 signing and crypt
  
**Changes in Veilid 0.1.9**
- SECURITY FIX
  * DESCRIPTION: Decompression was occurring in an unbounded way upon envelope receipt.
  * IMPACT: Node crashes resulting in downtime. There was no risk of RCE or compromise due to Rust's memory protections and no use of unsafe code near the site of the error.
  * INDICATIONS: This resulted in an out-of-memory abort on nodes. Issue first identified on the bootstrap servers. 
  * REMEDIATION: Length check added to decompression on envelopes.
- Earthfile support for generating a debug executable

**Changes in Veilid 0.1.8**
- Fix Python Install Instructions
- Fix to get server version from crate
- Move dev setup into its own folder
- Setup support for Fedora
- Make submodule paths absolute
- veilid-flutter improvements for crypto and timestamp, and endianness bugfix
- Offline subkey writes for DHT
- Fix WASM compilation
- Improve server port allocation
- Add more punishments
- Clap derive refactor for command line args
- gitignore emacs backup files
- Various typos
- Fanout debugging for DHT

**Changes in Veilid 0.1.7**

- Fix for connection table crash
- Fix for incorrect set_dht_value return value
- Python test updates
- Various VeilidChat-prompted veilid-flutter updates

**Changes in Veilid 0.1.6**

- Fix for 'find_node' too many nodes returned issue

**Changes in Veilid 0.1.5**

- Added Changelog 
- Fix detachment issue with suspending network interfaces during operation
- Fix incorrect punishment on relayed un-decryptable messages
- Minor API feature adds
- Relay bugfixes
