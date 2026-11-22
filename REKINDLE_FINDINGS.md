# Rekindle findings on veilid-core 0.5.8

This branch is a record and changes no Veilid code. It lists what the Rekindle team found while building on
veilid-core 0.5.8, so the issues can be revisited or taken to the Veilid maintainers. The record sits on top of the
branch `rekindle/v0.5.8-route-pool`, which carries the experimental patches cited below. Rekindle does not pin those
patches: Rekindle's ADR 0017 Policy 6 (2026-11-22) freezes the fork, and Rekindle changes its own usage to fit Veilid's
design. Upstream contact needs the Rekindle owner's go-ahead (ADR 0017 Policy 1).

Paths are relative to `veilid-core/src/` at 0.5.8 plus the route-drain fix `91453e97`.

---

## F1. Route selection can pick an API-allocated receive route as a safety route after a peer-info change

**Status:** genuine Veilid defect. Rekindle cannot avoid it from the app side. Candidate for an upstream report.

- The pool is defined elsewhere as automatic and unpublished routes (`routing_table/routing_domains/public_internet/
  controller/private_route_management.rs`: "manual routes are caller-owned"). But `route_select.rs`
  `first_available_route_inner` checks only `!is_published()`.
- `report_peer_info_changed` clears `published` on every published route (`route_spec_store_cache.rs:433-447`),
  including routes allocated through `new_private_route`/`new_custom_private_route`. The API offers no way to set it
  again. Veilid also sends the app no update when this happens: `PeerInfoChangeEvent` is internal
  (`controller/mod.rs:395-412`).
- After any peer-info change (a relay change, for example), an app's published receive route can be picked as some
  other send's safety route, or as a reply route whose blob goes to callees. That can link the app's receive route to
  its sends.
- **Fix that was tried:** `e1158844` "Keep API-allocated private routes out of the safety route pool". It adds
  `&& arce.is_automatic()` in `first_available_route_inner`, and `preferred_route` still works. Tests are in
  `routing_table/route_spec_store/tests_route_spec_store/test_route_pool.rs`
  (`test_manual_route_is_not_a_pool_route`, `test_published_manual_route_stays_out_after_peer_info_change`,
  `test_preferred_manual_route_is_selected`, `test_automatic_route_is_selected_beside_manual`).
- Related upstream issue: #337.

## F2. API routes are no longer tested by the periodic test after a peer-info change

**Status:** a scheduling inconsistency, low impact. Rekindle does not need a fix.

- `private_route_management.rs:~121` tests a route that is published or that has never been tested. After a
  peer-info change clears `published`, an API route drops out of the periodic 30 s test.
- The 10 s loopback keepalive (`SR_LOOPBACK_KEEPALIVE_INTERVAL`, `private_route_management.rs:7`, `:160-205`)
  still tests every allocated route, and its failure path marks the route for release and reports it in
  `RouteChange.dead_routes`. So a dead API route is still found.
- **Fix that was tried:** `bc741633` "Keep testing API-allocated routes after a peer info change", which tests
  `!is_automatic()` routes too, with test `test_manual_route_is_tested_after_peer_info_change`.

## F3. The untyped `RouteId` is ambiguous when an app imports its own private route blob

**Status:** an API/documentation issue. Rekindle hit it through a usage Veilid doesn't intend, and stopped doing
it. Worth a documentation note upstream.

- An import's id is a hash of the route's keys (`route_remote.rs:21-50`). An app that imports its own blob, for
  example to test its own route's reachability from outside, gets an import whose id equals its allocated route's id.
- A failed remote-route test releases through the generic `release_route(route_id)`
  (`routing_table/tasks/ping_validator.rs:~731-745`). That treats the id as remote only while the import is still
  cached (`route_spec_store/mod.rs:~280-287`, `route_remote.rs:13-19`), and otherwise releases the **allocated**
  route. Every non-automatic release is then reported in `RouteChange.dead_routes`
  (`route_spec_store_cache.rs:235-238`).
- `release_private_route` (`veilid_api/api.rs:~905-924`) resolves ids the same way, so releasing an import after
  Veilid dropped it releases the allocated route instead.
- A deferred retest stores an untyped id (`request_route_test(RouteId)`), so a retest of a gone import can test the
  allocated route with the same id.
- **Observed in Rekindle's real-app runs** with Veilid debug logging: `Remote route test failed (no response): <id>`,
  then `removing remote route from cache <id>`, then `releasing allocated route <id>`, then the app sees its own
  healthy route dead. The allocated route's loopback had been passing every 10 s. This was seen 5 times across two
  runs, and once with a simpler self-test.
- **Fixes that were tried:**
  - `a2c01232` "Release only the tested import when a remote route test fails" (`release_remote_route_id`)
  - `a48b66aa` "Release an expired import before an allocated route with the same id"
  - `18b3cbdf` "Type deferred route tests so a remote retest never tests an allocated route" (`RouteTestTarget`)
  - Tests are in `test_remote_routes.rs`.
- **What the Rekindle team concluded:** there is no public API for an app to test its own route
  (`route_test.rs:54-62`: `test_route` is crate-internal, used only during allocation). Veilid tests an owner's
  route by loopback at the owner, and tests reachability from outside at the importers. Apps should not import their
  own blob.

## F4. No way to pin or guard the hop next to a private route's owner

**Status:** a missing feature, with a security consequence. Candidate for an upstream design discussion.

- Every private-route hop sees the route's public key (`rpc_processor/rpc_route.rs:~660-708`), and the innermost
  hop delivers to the owner's node directly (`route_assemble.rs:124-141`). Anyone holding the blob holds that key.
  So an adversary who holds an app's blob and runs the node next to the owner links the route to the owner's node
  and IP in one step, with no timing analysis needed.
- The allocator ranks candidates rather than drawing them at random (`route_allocate.rs:666-760`). `Reliable` sorts
  by the longest-steady node as seen locally. That acts as a weak, accidental guard, and long-lived Sybil nodes can
  game it.
- A route death that blames hops puts **all** its hops on an avoid list for that ordering (`route_stats.rs:716-787`,
  `route_allocate.rs:110-136`), until node info, peer info or online state changes (`types/loss_epoch.rs`). So an
  adversary who can force deaths (by being on the path, or by DoS-ing the entry hop, which is readable from the blob)
  forces a new next-to-owner hop with each death.
- The API has no "require this node" option: `new_custom_private_route` builds `AllocateRouteParams` with
  `avoid_nodes: Vec::new()` (`veilid_api/api.rs:815-824`) and returns no hop list.
- **Numbers** (draws until an adversary holding share p of nodes sits next to the owner; P = 1 − (1−p)ⁿ): 1% → 69
  draws for 50%; 5% → 14; 10% → 7. With forced deaths every few minutes, a 5% adversary reaches 50% in about an
  hour. That matches Øverlier & Syverson, "Locating Hidden Servers" (IEEE S&P 2006) and the reasoning behind Tor's
  entry guards (guard-spec; proposal 271) and vanguards (vanguards-spec; proposal 333).
- **What real guards would need upstream:**
  - (a) `AllocateRouteParams`/`PrivateSpec` accept a required node for the hop next to the owner;
  - (b) that node is exempt from the dead-hop avoid list, and is not blamed when a loopback death can't localise the
    failing hop;
  - (c) ideally a small guard set with a persisted lifetime, picked from a capped sample (proposal 271 §4.1),
    layered as vanguards;
  - (d) at a deeper level, the next-to-owner hop not seeing a route key linkable to the blob.
- **What Rekindle does meanwhile, with Veilid unchanged:** "bounded exposure". That is a persisted budget on new
  routes whose blobs leave the device, an alert and a paused route when the budget is spent, no time-based rotation
  of distributed routes, a smaller blob audience, and per-class death counts.

## F5. The default route hop count must match the hop count an app requests

**Status:** not a Veilid defect. It is a usage lesson, and a possible documentation note.

- Veilid keeps tested safety routes only for `default_route_hop_count` and its "unsafe" count (`route_spec_store/
  mod.rs:135-142`, `private_route_management.rs:296-385`, four per cell, 10 s keepalive). Rekindle requested 3-hop
  safety routes while leaving `default_route_hop_count` at 1. So every send rode an untested 3-hop route built on the
  spot (`route_select.rs:200-275`), and that route was expired after its next test came due.
- The symptoms looked like route flakiness and invited app-side workarounds (a pinned API "Sending" route, a warm
  standby, self-tests). Setting the default hop count to the requested hop count is the as-built fix.

## F6. The online detector defers loopback blame while Offline

**Status:** designed behaviour. A detector that says Offline while traffic flows would be an upstream bug, but
this is not confirmed.

- While the detector reports Offline, loopback failures are deferred and retested after blame suppression lifts
  (`ping_validator.rs:~806-822`, `private_route_management.rs:~262-276`). In one Rekindle run a node stayed
  "Offline" for more than 15 minutes while traffic was flowing (Rekindle plan RH1r-c, MacA). It was not
  investigated further.

## F7. A destination can stall when every pooled safety route has lost to it

**Status:** observed in Rekindle real-app runs; candidate for an upstream design discussion. Found 2026-11-22 (Rekindle
research VA2, source at `bc741633`).

- Under symmetric routing an `app_call` picks a reply private route and uses it as the safety route
  (`rpc_processor/destination.rs:493-521`, `route_compile.rs:436-446`). Selection is per call, among unpublished
  automatic routes of the exact hop count and ordering (`route_select.rs:346-361`); a route that lost a question to this
  destination in the current loss epoch ranks last (`:388-437`).
- When **every** candidate has lost to the destination, selection keeps round-robining over the marked routes
  (`:428-435`); a new route is allocated only when no candidate matches at all (`:180-200`). The pool target is a
  constant, `DESIRED_BACKGROUND_SAFETY_ROUTE_COUNT = 4` (`route_spec_store/mod.rs`), not configurable.
- A route is killed by lost questions only after `ROUTE_LOST_DESTINATIONS_INVALIDATE_THRESHOLD = 5` distinct
  destinations at full weight, with evidence capped at one failure per destination (`route_stats.rs:137-139`), and any
  answer on that ordering clears it (`:483-484`). So routes that fail only toward one peer are never killed by that
  peer's losses.
- **Observed:** a peer's `app_call`s to a fresh, loopback-healthy private route timed out for ~2.5 min while its calls
  to another peer succeeded; delivery resumed 35 s after the caller's Veilid released one of its own Ordered safety
  routes (`lost_dests=11`).
- **Rekindle's response, Veilid unchanged:** fresh-call retries through its receipted outbox, the peer's route re-read
  when Veilid releases it, and a visible "unreachable, still trying" state; no sequencing or hop-count switch.

## F8. A failed remote route test blames the peer's route for what may be our own route's loss

**Status:** design note. A failed remote route test releases the imported peer route after one failure
(`tasks/ping_validator.rs:734-745`). That test selects its safety route the same way as the app's calls (`:692-697`),
so when our pool is the broken side, the peer's healthy route is released and re-imported repeatedly.

## F9. The death log reports counts, not the evidence that killed the route

**Status:** logging defect, low impact. `death_reason` prints the raw lost-destination count (including zero-weight
entries) and loopback evidence (`route_stats.rs:352-361`), not the lost-destination evidence that crossed the
threshold, so `loopback_streak=1 evidence=1.0 lost_dests=11` reads as a loopback death when it wasn't.

## F10. Redirection on the question path is silent

**Status:** observability gap. "Safety route … lost a question to …, compiling another for it" exists only for
compiles without a reply route (`route_compile.rs:568, 608-618`), i.e. statements; `app_call` redirection happens
silently in reply-route selection, and the chosen route is visible only with the compile-time `instrument` feature
(`route_select.rs:80-94`). The lost question is logged at debug as `scheduling out-of-band route test for <id> after
%SR timeout` (`rpc_processor/mod.rs:1605`) only when a test is enqueued.

## F11. Loss marks are void on any epoch change

**Status:** design note. The loss epoch includes our own peer-info timestamp and the count of online-state changes
(`types/loss_epoch.rs:5-14, 26-37`), matched by equality at selection (`route_select.rs:391-397`), so any such change
silently clears every "lost to this destination" mark and a failing route can win selection again.

---

**Kept for reference:** the patches above stay on `rekindle/v0.5.8-route-pool`, at `e1158844`, `bc741633`,
`a2c01232`, `a48b66aa` and `18b3cbdf` on top of `91453e97`. The Rekindle evidence logs and research reports are in
the Rekindle plan, `.claude/plans/standards-remediation/00-integration-plan.md` §3.0 (lane 1: VA1, RD1, and the
round-22/round-23 counts).
