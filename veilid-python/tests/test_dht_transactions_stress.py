# DHT transaction stress tests.
#
# Reproduce commit refusals under volume, concurrency, member versus anonymous
# transactions, and several writers on one record. Opt in with STRESS=1 against
# local veilid-server subnodes (api index i listens at localhost:5959+i). Every
# scenario prints one STRESS SUMMARY line and appends the same record as JSON to
# STRESS_OUT when set; the server logs (stor at debug) explain the refusals.
#
# Knobs: STRESS_TX_COUNT (40), STRESS_SUBKEYS_PER_TX (8), STRESS_CONCURRENCY (4),
# STRESS_WRITERS (3), STRESS_SUBNODES (2).
import asyncio
import contextlib
import json
import os
import random
import time
from typing import Awaitable, Callable, Optional

import pytest
import pytest_asyncio

import veilid
from veilid import *
from veilid.types import *

STRESS = os.getenv("STRESS") == "1"
TX_COUNT = int(os.getenv("STRESS_TX_COUNT", "40"))
SUBKEYS_PER_TX = int(os.getenv("STRESS_SUBKEYS_PER_TX", "8"))
CONCURRENCY = int(os.getenv("STRESS_CONCURRENCY", "4"))
WRITERS = int(os.getenv("STRESS_WRITERS", "3"))
SUBNODES = int(os.getenv("STRESS_SUBNODES", "2"))
STRESS_OUT = os.getenv("STRESS_OUT")
SCENARIO_TIMEOUT = 1800

pytestmark = pytest.mark.skipif(not STRESS, reason="stress test, opt in with STRESS=1")


async def null_update_callback(update: veilid.VeilidUpdate):
    pass


class Outcomes:
    """Per-scenario tally of transaction results and durations."""

    def __init__(self, name: str):
        self.name = name
        self.counts: dict[str, int] = {}
        self.durations: list[float] = []

    def record(self, bucket: str, started: float):
        self.counts[bucket] = self.counts.get(bucket, 0) + 1
        self.durations.append(time.monotonic() - started)

    @staticmethod
    def bucket_for(e: BaseException) -> str:
        msg = str(e)
        if isinstance(e, VeilidAPIErrorTryAgain):
            for needle in (
                "commit held by",
                "end held by",
                "consensus of transaction ids in begin",
                "set did not reach consensus",
                "stage was Failed",
                "fewer than required complete nodes",
                "dht is not online",
            ):
                if needle in msg:
                    return "try_again:" + needle.replace(" ", "_")
            return "try_again:other"
        if isinstance(e, VeilidAPIErrorTransactionNotFound):
            return "transaction_not_found"
        return "error:" + type(e).__name__

    def summary(self) -> dict:
        durations = sorted(self.durations)

        def pct(p: float) -> float:
            if not durations:
                return 0.0
            return durations[min(len(durations) - 1, int(p * len(durations)))]

        return {
            "scenario": self.name,
            "total": len(durations),
            "counts": self.counts,
            "p50_ms": round(pct(0.5) * 1000),
            "p95_ms": round(pct(0.95) * 1000),
            "max_ms": round(durations[-1] * 1000) if durations else 0,
        }

    def report(self):
        s = self.summary()
        print(f"STRESS SUMMARY {json.dumps(s, sort_keys=True)}")
        if STRESS_OUT:
            with open(STRESS_OUT, "a") as f:
                f.write(json.dumps(s, sort_keys=True) + "\n")


async def run_tx(
    api: veilid.VeilidAPI,
    keys: list[RecordKey],
    ops: Callable[[veilid.DHTTransaction], Awaitable[None]],
    outcomes: Outcomes,
    options: Optional[TransactDHTRecordsOptions] = None,
):
    """One begin-ops-commit cycle, tallied; an ops or commit failure rolls back."""
    started = time.monotonic()
    try:
        tx = await api.transact_dht_records(keys, options)
        async with tx:
            await ops(tx)
            await tx.commit()
        outcomes.record("ok", started)
    except VeilidAPIError as e:
        outcomes.record(Outcomes.bucket_for(e), started)


def payload(tag: str, n: int, subkey: int) -> bytes:
    return f"{tag}-{n}-{subkey}".encode()


@pytest_asyncio.fixture
async def apis():
    """Connections to subnodes 0..SUBNODES-1; subnode 0 is required."""
    conns = []
    for i in range(SUBNODES):
        try:
            conns.append(await veilid.api_connector(null_update_callback, i))
        except veilid.VeilidConnectionError:
            if i == 0:
                pytest.skip("Unable to connect to veilid-server 0.")
            break
    async with contextlib.AsyncExitStack() as stack:
        for api in conns:
            await stack.enter_async_context(api)
        for api in conns:
            await api.debug("record purge local")
            await api.debug("record purge remote")
        yield conns


async def first_kind(api: veilid.VeilidAPI) -> CryptoKind:
    return (await api.valid_crypto_kinds())[0]


async def cleanup(rc, keys: list[RecordKey]):
    for key in keys:
        with contextlib.suppress(VeilidAPIError):
            await rc.close_dht_record(key)
        with contextlib.suppress(VeilidAPIError):
            await rc.delete_dht_record(key)


# One owner, one record, sequential transactions each writing every subkey
@pytest.mark.asyncio
async def test_stress_volume_single_writer(apis):
    api0 = apis[0]
    kind = await first_kind(api0)
    outcomes = Outcomes("volume_single_writer")
    rc = await api0.new_routing_context()
    async with rc:
        rec = await rc.create_dht_record(kind, DHTSchema.dflt(SUBKEYS_PER_TX))
        try:

            async def run():
                for n in range(TX_COUNT):

                    async def ops(tx, n=n):
                        for sk in range(SUBKEYS_PER_TX):
                            await tx.set(rec.key, ValueSubkey(sk), payload("vol", n, sk))

                    await run_tx(api0, [rec.key], ops, outcomes)

            await asyncio.wait_for(run(), SCENARIO_TIMEOUT)
        finally:
            outcomes.report()
            await cleanup(rc, [rec.key])


# One owner, CONCURRENCY records written at once, each by its own task
@pytest.mark.asyncio
async def test_stress_concurrent_records_single_writer(apis):
    api0 = apis[0]
    kind = await first_kind(api0)
    outcomes = Outcomes("concurrent_records_single_writer")
    rc = await api0.new_routing_context()
    async with rc:
        recs = [
            await rc.create_dht_record(kind, DHTSchema.dflt(SUBKEYS_PER_TX))
            for _ in range(CONCURRENCY)
        ]
        try:

            async def task(rec, i):
                await asyncio.sleep(random.uniform(0, 0.8))
                for n in range(max(1, TX_COUNT // CONCURRENCY)):

                    async def ops(tx, n=n):
                        for sk in range(SUBKEYS_PER_TX):
                            await tx.set(rec.key, ValueSubkey(sk), payload(f"c{i}", n, sk))

                    await run_tx(api0, [rec.key], ops, outcomes)

            await asyncio.wait_for(
                asyncio.gather(*(task(rec, i) for i, rec in enumerate(recs))),
                SCENARIO_TIMEOUT,
            )
        finally:
            outcomes.report()
            await cleanup(rc, [r.key for r in recs])


# WRITERS member keys on one SMPL record, each writing its own subkeys from a
# different subnode, transactions started at random offsets so commits land
# on the same record at different times
@pytest.mark.asyncio
async def test_stress_multi_writer_same_record(apis):
    api0 = apis[0]
    kind = await first_kind(api0)
    outcomes = Outcomes("multi_writer_same_record")
    cs = await api0.get_crypto_system(kind)
    writers = []
    members = []
    async with cs:
        for _ in range(WRITERS):
            kp = await cs.generate_key_pair()
            writers.append(kp)
            members.append(await api0.generate_member_id(kp.key()))
    schema = DHTSchema.smpl(0, [DHTSchemaSMPLMember(m.value(), SUBKEYS_PER_TX) for m in members])
    rcs = [await api.new_routing_context() for api in apis]
    async with contextlib.AsyncExitStack() as stack:
        for rc in rcs:
            await stack.enter_async_context(rc)
        rec = await rcs[0].create_dht_record(kind, schema)
        opened = [(rcs[0], rec.key)]
        try:
            # A record is not on the network until its first write, so writer 0
            # seeds it from subnode 0 before the other subnodes open it
            await rcs[0].close_dht_record(rec.key)
            opened.pop()
            r0 = await rcs[0].open_dht_record(rec.key, writers[0])
            opened.append((rcs[0], r0.key))

            async def seed(tx):
                await tx.set(r0.key, ValueSubkey(0), payload("w0", -1, 0))

            await run_tx(apis[0], [r0.key], seed, outcomes)
            handles = [(apis[0], r0.key, 0)]
            for w in range(1, WRITERS):
                i = w % len(apis)
                r = await rcs[i].open_dht_record(rec.key, writers[w])
                opened.append((rcs[i], r.key))
                handles.append((apis[i], r.key, w))

            async def task(api, key, w):
                await asyncio.sleep(random.uniform(0, 0.8))
                base = w * SUBKEYS_PER_TX
                for n in range(max(1, TX_COUNT // WRITERS)):

                    async def ops(tx, n=n):
                        for sk in range(base, base + SUBKEYS_PER_TX):
                            await tx.set(key, ValueSubkey(sk), payload(f"w{w}", n, sk))

                    await run_tx(api, [key], ops, outcomes)
                    await asyncio.sleep(random.uniform(0, 0.3))

            await asyncio.wait_for(
                asyncio.gather(*(task(api, key, w) for api, key, w in handles)),
                SCENARIO_TIMEOUT,
            )
        finally:
            outcomes.report()
            for rc, key in opened:
                with contextlib.suppress(VeilidAPIError):
                    await rc.close_dht_record(key)
            with contextlib.suppress(VeilidAPIError):
                await rcs[0].delete_dht_record(rec.key)


# A member writer commits in a loop on subnode 0 while subnode 1 (or 0 when
# alone) runs anonymous get-only transactions on the same record
@pytest.mark.asyncio
async def test_stress_anonymous_readers_during_member_writes(apis):
    api0 = apis[0]
    api1 = apis[1] if len(apis) > 1 else apis[0]
    kind = await first_kind(api0)
    writes = Outcomes("readers_during_writes:writer")
    reads = Outcomes("readers_during_writes:anonymous_reader")
    rc0 = await api0.new_routing_context()
    rc1 = await api1.new_routing_context()
    async with rc0, rc1:
        rec = await rc0.create_dht_record(kind, DHTSchema.dflt(SUBKEYS_PER_TX))
        rec1 = None
        try:
            # Seed the record so the reader's open finds it on the network
            async def seed(tx):
                for sk in range(SUBKEYS_PER_TX):
                    await tx.set(rec.key, ValueSubkey(sk), payload("rw", -1, sk))

            await run_tx(api0, [rec.key], seed, writes)
            rec1 = await rc1.open_dht_record(rec.key, None)

            async def writer():
                for n in range(TX_COUNT):

                    async def ops(tx, n=n):
                        for sk in range(SUBKEYS_PER_TX):
                            await tx.set(rec.key, ValueSubkey(sk), payload("rw", n, sk))

                    await run_tx(api0, [rec.key], ops, writes)

            async def reader(key):
                await asyncio.sleep(0.5)
                for _ in range(TX_COUNT):

                    async def ops(tx):
                        for sk in range(SUBKEYS_PER_TX):
                            await tx.get(key, ValueSubkey(sk))

                    await run_tx(api1, [key], ops, reads)
                    await asyncio.sleep(random.uniform(0, 0.2))

            await asyncio.wait_for(asyncio.gather(writer(), reader(rec1.key)), SCENARIO_TIMEOUT)
        finally:
            writes.report()
            reads.report()
            if rec1 is not None:
                with contextlib.suppress(VeilidAPIError):
                    await rc1.close_dht_record(rec1.key)
            await cleanup(rc0, [rec.key])


# Transactions spanning three records, one subkey each
@pytest.mark.asyncio
async def test_stress_multi_record_transactions(apis):
    api0 = apis[0]
    kind = await first_kind(api0)
    outcomes = Outcomes("multi_record_transactions")
    rc = await api0.new_routing_context()
    async with rc:
        recs = [await rc.create_dht_record(kind, DHTSchema.dflt(2)) for _ in range(3)]
        keys = [r.key for r in recs]
        try:

            async def run():
                for n in range(TX_COUNT):

                    async def ops(tx, n=n):
                        for i, key in enumerate(keys):
                            await tx.set(key, ValueSubkey(n % 2), payload(f"m{i}", n, n % 2))

                    await run_tx(api0, keys, ops, outcomes)

            await asyncio.wait_for(run(), SCENARIO_TIMEOUT)
        finally:
            outcomes.report()
            await cleanup(rc, keys)
