# Protected store tests

import pytest
import veilid

# Keys veilid-core reserves for itself. Applications must never see or touch these.
RESERVED_KEYS = ["device_encryption_key", "_test_key"]

TEST_KEY = "_pytest_secret"
TEST_KEY2 = "_pytest_secret2"


async def _cleanup(api: veilid.VeilidAPI):
    for key in (TEST_KEY, TEST_KEY2):
        await api.remove_user_secret(key)


@pytest.mark.asyncio
async def test_save_load_remove_user_secret(api_connection: veilid.VeilidAPI):
    await _cleanup(api_connection)

    # Saving a new secret reports that nothing was there before.
    assert await api_connection.save_user_secret(TEST_KEY, b"hello") is False
    assert await api_connection.load_user_secret(TEST_KEY) == b"hello"

    # Overwriting reports the previous value.
    assert await api_connection.save_user_secret(TEST_KEY, b"world") is True
    assert await api_connection.load_user_secret(TEST_KEY) == b"world"

    # Removal is idempotent.
    assert await api_connection.remove_user_secret(TEST_KEY) is True
    assert await api_connection.remove_user_secret(TEST_KEY) is False
    assert await api_connection.load_user_secret(TEST_KEY) is None


@pytest.mark.asyncio
async def test_binary_secret_roundtrip(api_connection: veilid.VeilidAPI):
    await _cleanup(api_connection)

    # Non-utf8 bytes, including nulls, must survive the base64 wire encoding.
    value = bytes(range(256))
    await api_connection.save_user_secret(TEST_KEY, value)
    assert await api_connection.load_user_secret(TEST_KEY) == value

    # Empty values are a value, not an absence.
    await api_connection.save_user_secret(TEST_KEY, b"")
    assert await api_connection.load_user_secret(TEST_KEY) == b""

    await _cleanup(api_connection)


@pytest.mark.asyncio
async def test_missing_secret(api_connection: veilid.VeilidAPI):
    assert await api_connection.load_user_secret("_pytest_never_stored") is None
    assert await api_connection.remove_user_secret("_pytest_never_stored") is False


@pytest.mark.asyncio
async def test_list_user_secrets(api_connection: veilid.VeilidAPI):
    await _cleanup(api_connection)

    await api_connection.save_user_secret(TEST_KEY, b"a")
    await api_connection.save_user_secret(TEST_KEY2, b"b")

    keys = await api_connection.list_user_secrets()
    assert TEST_KEY in keys
    assert TEST_KEY2 in keys

    await api_connection.remove_user_secret(TEST_KEY)
    keys = await api_connection.list_user_secrets()
    assert TEST_KEY not in keys
    assert TEST_KEY2 in keys

    await _cleanup(api_connection)


@pytest.mark.asyncio
async def test_list_never_returns_reserved_keys(api_connection: veilid.VeilidAPI):
    """The node has a device_encryption_key of its own; it must stay invisible to apps."""
    await api_connection.save_user_secret(TEST_KEY, b"a")

    keys = await api_connection.list_user_secrets()
    for reserved in RESERVED_KEYS:
        assert reserved not in keys

    await _cleanup(api_connection)


@pytest.mark.asyncio
async def test_reserved_keys_are_rejected(api_connection: veilid.VeilidAPI):
    """An app must not be able to read or clobber veilid's own keys by name."""
    for reserved in RESERVED_KEYS:
        with pytest.raises(veilid.VeilidAPIError):
            await api_connection.load_user_secret(reserved)
        with pytest.raises(veilid.VeilidAPIError):
            await api_connection.save_user_secret(reserved, b"clobbered")
        with pytest.raises(veilid.VeilidAPIError):
            await api_connection.remove_user_secret(reserved)


@pytest.mark.asyncio
async def test_delete_all_user_secrets(api_connection: veilid.VeilidAPI):
    await api_connection.save_user_secret(TEST_KEY, b"a")
    await api_connection.save_user_secret(TEST_KEY2, b"b")

    await api_connection.delete_all_user_secrets()

    keys = await api_connection.list_user_secrets()
    assert TEST_KEY not in keys
    assert TEST_KEY2 not in keys

    # The node still works afterwards, which it would not if its own keys had been wiped.
    state = await api_connection.get_state()
    assert len(state.network.node_ids) >= 1
