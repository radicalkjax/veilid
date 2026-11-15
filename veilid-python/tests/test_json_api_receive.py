"""The receive loop outlives answers nobody waits for. Needs no server."""

import asyncio
import json
from typing import cast

import pytest

from veilid.json_api import _STREAM_LIMIT, _JsonVeilidAPI


class _NullWriter:
    def write(self, data: bytes):
        pass

    async def drain(self):
        pass

    def close(self):
        pass

    async def wait_closed(self):
        pass


async def _null_callback(update):
    pass


def _response(id: int) -> bytes:
    return (
        json.dumps({"type": "Response", "id": id, "op": "GetState", "value": {}}) + "\n"
    ).encode()


@pytest.mark.asyncio
async def test_late_answer_to_a_cancelled_request_keeps_the_connection():
    reader = asyncio.StreamReader()
    writer = cast(asyncio.StreamWriter, _NullWriter())
    api = _JsonVeilidAPI(reader, writer, _null_callback, validate_schema=False)

    # A caller gave up on request 1 (a timeout cancels its future); request 2 is still waited on
    abandoned = await api.allocate_request_future(1)
    abandoned.cancel()
    waited = await api.allocate_request_future(2)

    loop_task = asyncio.create_task(api.handle_recv_messages())
    reader.feed_data(_response(1))
    reader.feed_data(_response(2))

    answer = await asyncio.wait_for(waited, 5)
    assert answer["id"] == 2
    assert not loop_task.done()

    reader.feed_eof()
    await asyncio.wait_for(loop_task, 5)


@pytest.mark.asyncio
async def test_long_line_is_received():
    # A `record list watched` over a few hundred watches runs past half a megabyte
    reader = asyncio.StreamReader(limit=_STREAM_LIMIT)
    writer = cast(asyncio.StreamWriter, _NullWriter())
    api = _JsonVeilidAPI(reader, writer, _null_callback, validate_schema=False)

    waited = await api.allocate_request_future(1)
    loop_task = asyncio.create_task(api.handle_recv_messages())
    long_value = "x" * (512 * 1024)
    line = json.dumps({"type": "Response", "id": 1, "op": "Debug", "value": long_value})
    reader.feed_data((line + "\n").encode())

    answer = await asyncio.wait_for(waited, 5)
    assert answer["value"] == long_value
    assert not loop_task.done()

    reader.feed_eof()
    await asyncio.wait_for(loop_task, 5)
