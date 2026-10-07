"""Common test fixtures."""

from typing import AsyncGenerator, Awaitable, Callable, Optional, TypeVar

import pytest
import pytest_asyncio
import asyncio

import veilid
from veilid.json_api import _JsonVeilidAPI

pytest_plugins = ("pytest_asyncio",)

async def simple_update_callback(update: veilid.VeilidUpdate):
    pass


T = TypeVar("T")


async def wait_for_public_internet_ready(api: _JsonVeilidAPI) -> None:
    """Block until the attached node reports PublicInternet ready."""
    while not (await api.get_state()).attachment.public_internet_ready:
        await asyncio.sleep(0.25)


async def dht_retry(
    api: _JsonVeilidAPI,
    op: Callable[[], Awaitable[T]],
    label: Optional[str] = None,
) -> T:
    """Retry a DHT operation that may raise VeilidAPIErrorTryAgain.

    `op` is a zero-argument coroutine factory invoked once per attempt.
    Before each retry, waits for the node to report PublicInternet ready so
    we don't spin while the network is offline.
    """
    cnt = 0
    while True:
        try:
            return await op()
        except veilid.VeilidAPIErrorTryAgain as e:
            cnt += 1
            tag = label or "dht_retry"
            print(f"  {tag} retry #{cnt}: {e.message}")
            await wait_for_public_internet_ready(api)
            continue


@pytest_asyncio.fixture
async def api_connection() -> AsyncGenerator[_JsonVeilidAPI, None]:
    try:
        api = await veilid.api_connector(simple_update_callback)
    except veilid.VeilidConnectionError:
        pytest.skip("Unable to connect to veilid-server.")

    async with api:
        await wait_for_public_internet_ready(api)

        yield api
