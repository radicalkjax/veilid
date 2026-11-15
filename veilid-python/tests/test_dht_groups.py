# DHT group-roster record tests
#
# Structural, semantic and performance validation for the VeilidChat basic-groups
# design: SMPL records carrying one write-once "ballot" subkey per member (roster)
# and one multi-write "status" subkey per member (activity), written
# non-transactionally with allow_offline=False and watched by every member.
#
# Opt-in gates: PERFORMANCE=1 for timing runs, INTEGRATION=1 for multi-server runs.

import asyncio
import os
import statistics
import time
from typing import Optional

import pytest

import veilid
from veilid import *
from veilid.types import *

from .conftest import dht_retry

##################################################################

# storage_manager/types/mod.rs
MAX_SUBKEY_SIZE = 32768
MAX_RECORD_DATA_SIZE = 1_048_576

# Roster layout: owner subkeys carry the epoch header, then one slot per member
OWNER_SUBKEY_COUNT = 2


def max_subkey_len(subkey_count: int) -> int:
    """Per-subkey value ceiling the schema enforces for a record of this shape.

    See `check_subkey_value_data_smpl` in storage_manager/schema.rs: a record with
    more than 32 subkeys gets a proportionally smaller per-subkey budget, so the
    flat 32 KB cap does not apply to wide member records.
    """
    return min(MAX_SUBKEY_SIZE, MAX_RECORD_DATA_SIZE // subkey_count)


def member_subkey(
    member_index: int,
    slot: int = 0,
    m_cnt: int = 1,
    o_cnt: int = OWNER_SUBKEY_COUNT,
) -> ValueSubkey:
    """Subkey owned by a given member, per the SMPL owner-block-then-member-blocks layout."""
    return ValueSubkey(o_cnt + member_index * m_cnt + slot)


async def make_roster_record(
    api: veilid.VeilidAPI,
    rc: veilid.RoutingContext,
    cs: veilid.CryptoSystem,
    member_count: int,
    m_cnt: int = 1,
    o_cnt: int = OWNER_SUBKEY_COUNT,
) -> tuple[veilid.DHTRecordDescriptor, list[veilid.KeyPair]]:
    """Create an SMPL record shaped like a group roster or activity record.

    Returns the descriptor plus the per-member writer keypairs in schema member
    order, so `member_subkey(i)` addresses `keypairs[i]`'s slot.
    """
    members = []
    keypairs = []
    for _ in range(member_count):
        kp = await cs.generate_key_pair()
        member_id = await api.generate_member_id(kp.key())
        members.append(veilid.DHTSchemaSMPLMember(member_id.value(), m_cnt))
        keypairs.append(kp)

    schema = veilid.DHTSchema.smpl(o_cnt, members)
    desc = await rc.create_dht_record(await cs.kind(), schema)
    return (desc, keypairs)


def report(name: str, **fields):
    """Emit a greppable one-line result record for the spike report."""
    kv = " ".join(f"{k}={v}" for k, v in fields.items())
    print(f"GROUPSPIKE {name} {kv}")


##################################################################
# Structural and semantic tests (single server)


@pytest.mark.asyncio
async def test_group_roster_layout(api_connection: veilid.VeilidAPI):
    """Members write their own slots; the layout formula addresses them correctly."""
    rc = await api_connection.new_routing_context()
    async with rc:
        for kind in await api_connection.valid_crypto_kinds():
            cs = await api_connection.get_crypto_system(kind)
            async with cs:
                MEMBERS = 4
                desc, keypairs = await make_roster_record(api_connection, rc, cs, MEMBERS)

                assert isinstance(desc.schema, veilid.DHTSchemaSMPL)
                assert len(desc.schema.members) == MEMBERS

                await rc.open_dht_record(desc.key, keypairs[0])

                for i, kp in enumerate(keypairs):
                    subkey = member_subkey(i)
                    data = f"member {i} ballot".encode()
                    vd = await dht_retry(
                        api_connection,
                        lambda k=desc.key, s=subkey, d=data, w=kp: rc.set_dht_value(
                            k, s, d, veilid.SetDHTValueOptions(w, False)
                        ),
                        "set member slot",
                    )
                    assert vd is None, "own-slot write should not conflict"

                for i in range(MEMBERS):
                    vd = await rc.get_dht_value(desc.key, member_subkey(i), True)
                    assert vd is not None
                    assert vd.data == f"member {i} ballot".encode()
                    assert vd.seq == 0, "first write of a subkey is seq 0"
                    assert vd.writer == keypairs[i].key()

                await rc.close_dht_record(desc.key)
                await rc.delete_dht_record(desc.key)


@pytest.mark.asyncio
async def test_group_member_write_authorization(api_connection: veilid.VeilidAPI):
    """Slot ownership is enforced: no member may write another's slot, and the
    record owner may not write member slots at all.

    The ballot design rests on this: a transition author owns the record but
    still cannot forge another member's ballot.
    """
    rc = await api_connection.new_routing_context()
    async with rc:
        for kind in await api_connection.valid_crypto_kinds():
            cs = await api_connection.get_crypto_system(kind)
            async with cs:
                desc, keypairs = await make_roster_record(api_connection, rc, cs, 3)
                owner_keypair = desc.owner_key_pair()
                assert owner_keypair is not None

                await rc.open_dht_record(desc.key, keypairs[0])

                # Own slot: allowed
                vd = await dht_retry(
                    api_connection,
                    lambda: rc.set_dht_value(
                        desc.key,
                        member_subkey(0),
                        b"mine",
                        veilid.SetDHTValueOptions(keypairs[0], False),
                    ),
                    "own slot",
                )
                assert vd is None

                # Another member's slot: rejected
                with pytest.raises(Exception):
                    await rc.set_dht_value(
                        desc.key,
                        member_subkey(1),
                        b"forged",
                        veilid.SetDHTValueOptions(keypairs[0], False),
                    )

                # Owner writing a member slot: rejected
                with pytest.raises(Exception):
                    await rc.set_dht_value(
                        desc.key,
                        member_subkey(0),
                        b"owner override",
                        veilid.SetDHTValueOptions(owner_keypair, False),
                    )

                # Member writing an owner slot: rejected
                with pytest.raises(Exception):
                    await rc.set_dht_value(
                        desc.key,
                        ValueSubkey(0),
                        b"member override",
                        veilid.SetDHTValueOptions(keypairs[0], False),
                    )

                # Owner writing its own header slot: allowed
                vd = await dht_retry(
                    api_connection,
                    lambda: rc.set_dht_value(
                        desc.key,
                        ValueSubkey(0),
                        b"epoch header",
                        veilid.SetDHTValueOptions(owner_keypair, False),
                    ),
                    "owner header",
                )
                assert vd is None

                await rc.close_dht_record(desc.key)
                await rc.delete_dht_record(desc.key)


@pytest.mark.asyncio
async def test_group_ballot_write_once_seq(api_connection: veilid.VeilidAPI):
    """Sequence numbers make a rewritten ballot slot detectable.

    Write-once is enforced app-side by treating any slot past its first write as
    poisoned, which requires: first write is seq 0, an identical rewrite does not
    advance (so retries are safe), and a differing rewrite does advance.
    """
    rc = await api_connection.new_routing_context()
    async with rc:
        for kind in await api_connection.valid_crypto_kinds():
            cs = await api_connection.get_crypto_system(kind)
            async with cs:
                desc, keypairs = await make_roster_record(api_connection, rc, cs, 2)
                await rc.open_dht_record(desc.key, keypairs[0])
                subkey = member_subkey(0)
                opts = veilid.SetDHTValueOptions(keypairs[0], False)

                ballot = b"ballot: propose successor A"

                await dht_retry(
                    api_connection,
                    lambda: rc.set_dht_value(desc.key, subkey, ballot, opts),
                    "first write",
                )
                vd = await rc.get_dht_value(desc.key, subkey, True)
                assert vd is not None and vd.seq == 0

                # Idempotent rewrite: same bytes, same writer, no advance
                await rc.set_dht_value(desc.key, subkey, ballot, opts)
                vd = await rc.get_dht_value(desc.key, subkey, True)
                assert vd is not None
                assert vd.seq == 0, "identical rewrite must not advance seq"

                # Retraction attempt: differing bytes advance the seq, marking the slot poisoned
                await dht_retry(
                    api_connection,
                    lambda: rc.set_dht_value(desc.key, subkey, b"ballot: retracted", opts),
                    "rewrite",
                )
                vd = await rc.get_dht_value(desc.key, subkey, True)
                assert vd is not None
                assert vd.seq == 1, "differing rewrite must advance seq"

                # A reader detects it from the report without fetching values
                rr = await rc.inspect_dht_record(
                    desc.key, [(subkey, subkey)], veilid.DHTReportScope.SYNC_GET
                )
                assert rr.network_seqs[0] is not None
                assert int(rr.network_seqs[0]) >= 1, "poisoned slot visible via inspect"

                await rc.close_dht_record(desc.key)
                await rc.delete_dht_record(desc.key)


@pytest.mark.asyncio
async def test_group_subkey_budget(api_connection: veilid.VeilidAPI):
    """The per-subkey ceiling shrinks as the member count grows.

    Pins the budget a VLD1-sized ballot must fit into: at N members the ceiling is
    1 MB / (N + owner subkeys), not a flat 32 KB.
    """
    rc = await api_connection.new_routing_context()
    async with rc:
        for kind in await api_connection.valid_crypto_kinds():
            cs = await api_connection.get_crypto_system(kind)
            async with cs:
                MEMBERS = int(os.getenv("GROUP_MEMBERS", "62"))
                desc, keypairs = await make_roster_record(api_connection, rc, cs, MEMBERS)
                subkey_count = OWNER_SUBKEY_COUNT + MEMBERS
                limit = max_subkey_len(subkey_count)
                # A VLD1 adopt ballot embeds an ML-DSA-signed transition statement
                vld1_ballot = 3686
                report(
                    "subkey_budget",
                    members=MEMBERS,
                    subkey_count=subkey_count,
                    limit_bytes=limit,
                    vld1_ballot_bytes=vld1_ballot,
                    vld1_headroom_bytes=limit - vld1_ballot,
                )

                await rc.open_dht_record(desc.key, keypairs[0])
                subkey = member_subkey(0)
                opts = veilid.SetDHTValueOptions(keypairs[0], False)

                await dht_retry(
                    api_connection,
                    lambda: rc.set_dht_value(desc.key, subkey, b"A" * limit, opts),
                    "at limit",
                )
                vd = await rc.get_dht_value(desc.key, subkey, True)
                assert vd is not None and len(vd.data) == limit

                with pytest.raises(Exception):
                    await rc.set_dht_value(desc.key, subkey, b"A" * (limit + 1), opts)

                await rc.close_dht_record(desc.key)
                await rc.delete_dht_record(desc.key)


@pytest.mark.skipif(os.getenv("PERFORMANCE") != "1", reason="performance test takes a long time")
@pytest.mark.asyncio
async def test_group_roster_scale(api_connection: veilid.VeilidAPI):
    """Create rosters at realistic group sizes and confirm the schema holds.

    Reports creation latency and the resulting per-subkey ceiling per size, which
    is what sets the advertised group-size maximum.
    """
    sizes = [int(s) for s in os.getenv("GROUP_SIZES", "16,64,128,255").split(",")]
    rc = await api_connection.new_routing_context()
    async with rc:
        kind = (await api_connection.valid_crypto_kinds())[0]
        cs = await api_connection.get_crypto_system(kind)
        async with cs:
            for members in sizes:
                start = time.monotonic()
                desc, keypairs = await make_roster_record(api_connection, rc, cs, members)
                create_s = time.monotonic() - start

                subkey_count = OWNER_SUBKEY_COUNT + members
                report(
                    "roster_scale",
                    members=members,
                    subkey_count=subkey_count,
                    limit_bytes=max_subkey_len(subkey_count),
                    create_s=f"{create_s:.3f}",
                )

                await rc.open_dht_record(desc.key, keypairs[0])
                await rc.close_dht_record(desc.key)
                await rc.delete_dht_record(desc.key)


@pytest.mark.skipif(os.getenv("PERFORMANCE") != "1", reason="performance test takes a long time")
@pytest.mark.asyncio
async def test_group_status_write_latency(api_connection: veilid.VeilidAPI):
    """Per-write cost of a status-slot update, which sets the send-path debounce.

    Each member writes only its own subkey and nothing coordinates them, so this
    measures the standalone cost of one non-transactional confirmed write.
    """
    members = int(os.getenv("GROUP_MEMBERS", "16"))
    rounds = int(os.getenv("GROUP_ROUNDS", "3"))

    rc = await api_connection.new_routing_context()
    async with rc:
        kind = (await api_connection.valid_crypto_kinds())[0]
        cs = await api_connection.get_crypto_system(kind)
        async with cs:
            desc, keypairs = await make_roster_record(api_connection, rc, cs, members)
            await rc.open_dht_record(desc.key, keypairs[0])

            async def write(i: int, r: int) -> Optional[float]:
                payload = f"status {i} round {r} {time.monotonic()}".encode()
                opts = veilid.SetDHTValueOptions(keypairs[i], False)
                start = time.monotonic()
                try:
                    await rc.set_dht_value(desc.key, member_subkey(i), payload, opts)
                except Exception as e:
                    print(f"  write member={i} round={r} failed: {e}")
                    return None
                return time.monotonic() - start

            for label, concurrent in (("sequential", False), ("concurrent", True)):
                latencies: list[float] = []
                failures = 0
                for r in range(rounds):
                    if concurrent:
                        results = await asyncio.gather(*[write(i, r) for i in range(members)])
                    else:
                        results = [await write(i, r) for i in range(members)]
                    for x in results:
                        if x is None:
                            failures += 1
                        else:
                            latencies.append(x)

                if latencies:
                    latencies.sort()
                    report(
                        "status_write_latency",
                        mode=label,
                        members=members,
                        rounds=rounds,
                        n=len(latencies),
                        failures=failures,
                        p50=f"{statistics.median(latencies):.3f}",
                        p95=f"{latencies[int(len(latencies) * 0.95) - 1]:.3f}",
                        max=f"{latencies[-1]:.3f}",
                    )

            await rc.close_dht_record(desc.key)
            await rc.delete_dht_record(desc.key)


@pytest.mark.skipif(os.getenv("PERFORMANCE") != "1", reason="performance test takes a long time")
@pytest.mark.asyncio
async def test_group_inspect_cost(api_connection: veilid.VeilidAPI):
    """Cost of scanning every ballot slot, paid once per epoch transition to find
    poisoned slots and tally ballots."""
    members = int(os.getenv("GROUP_MEMBERS", "64"))

    rc = await api_connection.new_routing_context()
    async with rc:
        kind = (await api_connection.valid_crypto_kinds())[0]
        cs = await api_connection.get_crypto_system(kind)
        async with cs:
            desc, keypairs = await make_roster_record(api_connection, rc, cs, members)
            await rc.open_dht_record(desc.key, keypairs[0])

            # Cast a few ballots so the scan has something to find
            for i in range(min(members, 8)):
                await dht_retry(
                    api_connection,
                    lambda i=i: rc.set_dht_value(
                        desc.key,
                        member_subkey(i),
                        f"ballot {i}".encode(),
                        veilid.SetDHTValueOptions(keypairs[i], False),
                    ),
                    "seed ballot",
                )

            full_range = [(member_subkey(0), member_subkey(members - 1))]
            # First network inspect of a scope pays the locating fanout, later ones hit the cache
            repeats = int(os.getenv("GROUP_REPEATS", "5"))
            for scope in (
                veilid.DHTReportScope.LOCAL,
                veilid.DHTReportScope.SYNC_GET,
            ):
                for attempt in range(repeats):
                    start = time.monotonic()
                    rr = await rc.inspect_dht_record(desc.key, full_range, scope)
                    elapsed = time.monotonic() - start
                    seqs = (
                        rr.local_seqs if scope == veilid.DHTReportScope.LOCAL else rr.network_seqs
                    )
                    written = sum(1 for s in seqs if s is not None)
                    report(
                        "inspect_cost",
                        members=members,
                        scope=str(scope),
                        attempt=attempt,
                        elapsed_s=f"{elapsed:.3f}",
                        written_slots=written,
                    )

            await rc.close_dht_record(desc.key)
            await rc.delete_dht_record(desc.key)


@pytest.mark.skipif(os.getenv("PERFORMANCE") != "1", reason="performance test takes a long time")
@pytest.mark.asyncio
async def test_group_inspect_fill_sensitivity(api_connection: veilid.VeilidAPI):
    """Network inspect cost as a function of how many slots are written.

    A subkey nobody has written returns no seq from every node, so it can never
    accumulate consensus and the fanout runs to exhaustion. A roster is mostly
    unwritten slots in the common case, so this is the shape that matters.
    """
    members = int(os.getenv("GROUP_MEMBERS", "8"))

    rc = await api_connection.new_routing_context()
    async with rc:
        kind = (await api_connection.valid_crypto_kinds())[0]
        cs = await api_connection.get_crypto_system(kind)
        async with cs:
            for filled in (members, members // 2, 1):
                desc, keypairs = await make_roster_record(api_connection, rc, cs, members)
                await rc.open_dht_record(desc.key, keypairs[0])
                for i in range(filled):
                    await dht_retry(
                        api_connection,
                        lambda i=i: rc.set_dht_value(
                            desc.key,
                            member_subkey(i),
                            f"slot {i}".encode(),
                            veilid.SetDHTValueOptions(keypairs[i], False),
                        ),
                        "fill",
                    )

                rng = [(member_subkey(0), member_subkey(members - 1))]
                start = time.monotonic()
                await rc.inspect_dht_record(desc.key, rng, veilid.DHTReportScope.SYNC_GET)
                elapsed = time.monotonic() - start
                report(
                    "inspect_fill",
                    members=members,
                    filled=filled,
                    unwritten=members - filled,
                    elapsed_s=f"{elapsed:.3f}",
                )

                await rc.close_dht_record(desc.key)
                await rc.delete_dht_record(desc.key)


@pytest.mark.skipif(os.getenv("PERFORMANCE") != "1", reason="performance test takes a long time")
@pytest.mark.asyncio
async def test_group_get_value_cost(api_connection: veilid.VeilidAPI):
    """Cost of network-verifying a single slot, repeated.

    Separates the per-subkey get path from the range-inspect path: reading one
    ballot back from the network is what arbitration actually needs, and the
    first get pays the fanout that locates the value nodes.
    """
    repeats = int(os.getenv("GROUP_REPEATS", "6"))

    rc = await api_connection.new_routing_context()
    async with rc:
        kind = (await api_connection.valid_crypto_kinds())[0]
        cs = await api_connection.get_crypto_system(kind)
        async with cs:
            desc, keypairs = await make_roster_record(api_connection, rc, cs, 16)
            await rc.open_dht_record(desc.key, keypairs[0])
            subkey = member_subkey(0)
            await dht_retry(
                api_connection,
                lambda: rc.set_dht_value(
                    desc.key,
                    subkey,
                    b"ballot",
                    veilid.SetDHTValueOptions(keypairs[0], False),
                ),
                "seed",
            )

            for force in (False, True):
                for attempt in range(repeats):
                    start = time.monotonic()
                    vd = await rc.get_dht_value(desc.key, subkey, force)
                    elapsed = time.monotonic() - start
                    report(
                        "get_value_cost",
                        force_refresh=force,
                        attempt=attempt,
                        elapsed_s=f"{elapsed:.3f}",
                        got=vd is not None,
                    )

            await rc.close_dht_record(desc.key)
            await rc.delete_dht_record(desc.key)


##################################################################
# Multi-server tests: real cross-node watch behavior
#
# A node is never notified of its own writes, so watch delivery can only be
# measured with a separate writer and watcher node.


@pytest.mark.skipif(
    os.getenv("INTEGRATION") != "1",
    reason="integration test requires two servers running",
)
@pytest.mark.asyncio
async def test_group_watch_notification_latency():
    """Write-to-notification latency and delivery rate for status-slot updates.

    This is the group message-notification path: one watch on the shared record
    stands in for watching every member's log.
    """
    value_change_queue: asyncio.Queue[tuple[float, veilid.VeilidUpdate]] = asyncio.Queue()

    async def watcher_callback(update: veilid.VeilidUpdate):
        if update.kind == veilid.VeilidUpdateKind.VALUE_CHANGE:
            await value_change_queue.put((time.monotonic(), update))

    async def null_callback(update: veilid.VeilidUpdate):
        pass

    try:
        api_watcher = await veilid.api_connector(watcher_callback, 0)
    except veilid.VeilidConnectionError:
        pytest.skip("Unable to connect to veilid-server 0.")
        return
    try:
        api_writer = await veilid.api_connector(null_callback, 1)
    except veilid.VeilidConnectionError:
        pytest.skip("Unable to connect to veilid-server 1.")
        return

    async with api_watcher, api_writer:
        for api in (api_watcher, api_writer):
            await api.debug("record purge local")
            await api.debug("record purge remote")

        while True:
            try:
                await asyncio.wait_for(value_change_queue.get(), timeout=3)
            except asyncio.TimeoutError:
                break

        rc_watcher = await api_watcher.new_routing_context()
        rc_writer = await api_writer.new_routing_context()
        async with rc_watcher, rc_writer:
            kind = (await api_writer.valid_crypto_kinds())[0]
            cs = await api_writer.get_crypto_system(kind)
            async with cs:
                members = int(os.getenv("GROUP_MEMBERS", "8"))
                writes = int(os.getenv("GROUP_WRITES", "10"))

                desc, keypairs = await make_roster_record(api_writer, rc_writer, cs, members)
                await rc_writer.open_dht_record(desc.key, keypairs[0])

                # Publish the record so the watcher node can resolve it
                owner_keypair = desc.owner_key_pair()
                await dht_retry(
                    api_writer,
                    lambda: rc_writer.set_dht_value(
                        desc.key,
                        ValueSubkey(0),
                        b"epoch header",
                        veilid.SetDHTValueOptions(owner_keypair, False),
                    ),
                    "publish record",
                )

                await rc_watcher.open_dht_record(desc.key, None)
                active = await rc_watcher.watch_dht_values(desc.key)
                assert active, "watch must establish before measuring"

                latencies: list[float] = []
                missed = 0
                for n in range(writes):
                    i = n % members
                    sent = time.monotonic()
                    await dht_retry(
                        api_writer,
                        lambda i=i, n=n: rc_writer.set_dht_value(
                            desc.key,
                            member_subkey(i),
                            f"status {i} #{n}".encode(),
                            veilid.SetDHTValueOptions(keypairs[i], False),
                        ),
                        "status write",
                    )
                    try:
                        recv, upd = await asyncio.wait_for(value_change_queue.get(), timeout=30)
                        assert isinstance(upd.detail, veilid.VeilidValueChange)
                        assert upd.detail.key == desc.key
                        latencies.append(recv - sent)
                    except asyncio.TimeoutError:
                        missed += 1
                        print(f"  no notification for write #{n}")

                if latencies:
                    latencies.sort()
                    report(
                        "watch_latency",
                        members=members,
                        writes=writes,
                        delivered=len(latencies),
                        missed=missed,
                        p50=f"{statistics.median(latencies):.3f}",
                        p95=f"{latencies[int(len(latencies) * 0.95) - 1]:.3f}",
                        max=f"{latencies[-1]:.3f}",
                    )
                else:
                    report(
                        "watch_latency", members=members, writes=writes, delivered=0, missed=missed
                    )

                await rc_watcher.cancel_dht_watch(desc.key)
                await rc_watcher.close_dht_record(desc.key)
                await rc_writer.close_dht_record(desc.key)
                await rc_writer.delete_dht_record(desc.key)


@pytest.mark.skipif(
    os.getenv("INTEGRATION") != "1",
    reason="integration test requires multiple servers running",
)
@pytest.mark.skipif(os.getenv("PERFORMANCE") != "1", reason="performance test takes a long time")
@pytest.mark.asyncio
async def test_group_watch_fanout():
    """Notification cost as the watcher count grows.

    Every status write notifies every member, so fan-out rather than record
    capacity is the likely bound on group size. Needs one veilid-server subnode
    per watcher: set GROUP_WATCHERS to the number running beyond subnode 0.
    """
    watcher_count = int(os.getenv("GROUP_WATCHERS", "2"))

    queues: list[asyncio.Queue] = []
    apis = []

    def make_callback(q: asyncio.Queue):
        async def cb(update: veilid.VeilidUpdate):
            if update.kind == veilid.VeilidUpdateKind.VALUE_CHANGE:
                await q.put((time.monotonic(), update))

        return cb

    async def null_callback(update: veilid.VeilidUpdate):
        pass

    try:
        api_writer = await veilid.api_connector(null_callback, 0)
    except veilid.VeilidConnectionError:
        pytest.skip("Unable to connect to veilid-server 0.")
        return

    for idx in range(1, watcher_count + 1):
        try:
            q: asyncio.Queue = asyncio.Queue()
            apis.append(await veilid.api_connector(make_callback(q), idx))
            queues.append(q)
        except veilid.VeilidConnectionError:
            pytest.skip(f"Unable to connect to veilid-server {idx}.")
            return

    try:
        rc_writer = await api_writer.new_routing_context()
        async with rc_writer:
            kind = (await api_writer.valid_crypto_kinds())[0]
            cs = await api_writer.get_crypto_system(kind)
            async with cs:
                desc, keypairs = await make_roster_record(api_writer, rc_writer, cs, 4)
                await rc_writer.open_dht_record(desc.key, keypairs[0])

                # Publish the record so watcher nodes can resolve it
                await dht_retry(
                    api_writer,
                    lambda: rc_writer.set_dht_value(
                        desc.key,
                        ValueSubkey(0),
                        b"epoch header",
                        veilid.SetDHTValueOptions(desc.owner_key_pair(), False),
                    ),
                    "publish record",
                )

                watcher_rcs = []
                for api in apis:
                    rc = await api.new_routing_context()
                    watcher_rcs.append(rc)
                    await rc.open_dht_record(desc.key, None)
                    assert await rc.watch_dht_values(desc.key)

                # Discard watch-establishment notifications: pairing one with a
                # later write reads as negative latency
                for q in queues:
                    while True:
                        try:
                            await asyncio.wait_for(q.get(), timeout=5)
                        except asyncio.TimeoutError:
                            break

                writes = int(os.getenv("GROUP_WRITES", "5"))
                per_watcher: list[list[float]] = [[] for _ in queues]
                missed = 0
                for n in range(writes):
                    sent = time.monotonic()
                    await dht_retry(
                        api_writer,
                        lambda n=n: rc_writer.set_dht_value(
                            desc.key,
                            member_subkey(n % 4),
                            f"fanout #{n}".encode(),
                            veilid.SetDHTValueOptions(keypairs[n % 4], False),
                        ),
                        "fanout write",
                    )
                    for w, q in enumerate(queues):
                        try:
                            while True:
                                recv, _ = await asyncio.wait_for(q.get(), timeout=30)
                                if recv >= sent:
                                    break
                            per_watcher[w].append(recv - sent)
                        except asyncio.TimeoutError:
                            missed += 1

                for w, lat in enumerate(per_watcher):
                    if lat:
                        lat.sort()
                        report(
                            "watch_fanout",
                            watchers=watcher_count,
                            watcher=w,
                            delivered=len(lat),
                            p50=f"{statistics.median(lat):.3f}",
                            max=f"{lat[-1]:.3f}",
                        )
                report("watch_fanout_missed", watchers=watcher_count, missed=missed)

                for rc in watcher_rcs:
                    await rc.cancel_dht_watch(desc.key)
                    await rc.close_dht_record(desc.key)
                await rc_writer.close_dht_record(desc.key)
                await rc_writer.delete_dht_record(desc.key)
    finally:
        for api in apis:
            await api.release()
        await api_writer.release()
