"""Common test fixtures."""

from typing import AsyncGenerator, Awaitable, Callable, Optional, TypeVar

import pytest
import pytest_asyncio

import veilid
from veilid.json_api import _JsonVeilidAPI

pytest_plugins = ("pytest_asyncio",)


async def simple_update_callback(update: veilid.VeilidUpdate):
    pass


T = TypeVar("T")


async def dht_retry(
    api: veilid.VeilidAPI,
    op: Callable[..., Awaitable[T]],
    label: Optional[str] = None,
) -> T:
    """Retry a DHT operation that hits a transient network condition.

    `op` is a zero-argument coroutine factory invoked once per attempt. Retries on
    VeilidAPIErrorTryAgain (node not ready) and VeilidAPIErrorTimeout (e.g. a
    freshly-allocated route still establishing its hop connections). Before each
    retry, waits for the node to report PublicInternet ready so we don't spin while
    the network is offline.
    """
    cnt = 0
    while True:
        try:
            return await op()
        except (veilid.VeilidAPIErrorTryAgain, veilid.VeilidAPIErrorTimeout) as e:
            cnt += 1
            tag = label or "dht_retry"
            print(f"  {tag} retry #{cnt}: {e}")
            await api.wait_for_attachment_condition()
            continue


@pytest_asyncio.fixture
async def api_connection() -> AsyncGenerator[_JsonVeilidAPI, None]:
    try:
        api = await veilid.api_connector(simple_update_callback)
    except veilid.VeilidConnectionError:
        pytest.skip("Unable to connect to veilid-server.")

    async with api:
        await api.wait_for_attachment_condition()

        yield api
