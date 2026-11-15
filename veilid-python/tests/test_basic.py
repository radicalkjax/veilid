# Basic veilid tests

import socket
import json
import dataclasses

import pytest
import veilid

from .conftest import simple_update_callback


@pytest.mark.asyncio
async def test_connect(api_connection: veilid.VeilidAPI):
    pass


@pytest.mark.asyncio
async def test_get_node_ids(api_connection: veilid.VeilidAPI):
    state = await api_connection.get_state()
    node_ids = state.network.node_ids

    assert len(node_ids) >= 1

    for node_id in node_ids:
        assert node_id[4] == ":"


@pytest.mark.asyncio
async def test_wait_for_attachment_condition(api_connection: veilid.VeilidAPI):
    # The fixture waited for the default condition, so it holds at once
    attachment = await api_connection.wait_for_attachment_condition()
    assert attachment.public_internet_ready

    with pytest.raises(veilid.VeilidAPIErrorTimeout):
        await api_connection.wait_for_attachment_condition(
            veilid.VeilidStateAttachmentCondition(opt_max_state=veilid.AttachmentState.DETACHED),
            500,
        )

    with pytest.raises(veilid.VeilidAPIErrorInvalidArgument):
        await api_connection.wait_for_attachment_condition(
            veilid.VeilidStateAttachmentCondition(
                opt_min_state=veilid.AttachmentState.ATTACHED_FULL,
                opt_max_state=veilid.AttachmentState.ATTACHED_WEAK,
            )
        )


@pytest.mark.asyncio
async def test_report_app_lifecycle_state(api_connection: veilid.VeilidAPI):
    background = veilid.VeilidAppLifecycleState.BACKGROUND
    if "app-lifecycle" in await api_connection.veilid_features():
        await api_connection.report_app_lifecycle_state(background)
    else:
        # The call is always there, and a node built without the feature says so
        with pytest.raises(veilid.VeilidAPIErrorUnimplemented):
            await api_connection.report_app_lifecycle_state(background)


@pytest.mark.asyncio
async def test_fail_connect():
    with pytest.raises(socket.gaierror) as exc:
        await veilid.json_api_connect("fuahwelifuh32luhwafluehawea", 1, simple_update_callback)

    assert exc.value.errno == socket.EAI_NONAME


@pytest.mark.asyncio
async def test_version(api_connection: veilid.VeilidAPI):
    v = await api_connection.veilid_version()
    print(f"veilid_version: {v.__dict__}")
    assert v.__dict__.keys() >= {"_major", "_minor", "_patch"}

    vstr = await api_connection.veilid_version_string()
    print(f"veilid_version_string: {vstr}")


@pytest.mark.asyncio
async def test_features(api_connection: veilid.VeilidAPI):
    features = await api_connection.veilid_features()
    print(f"veilid_features: {features}")
    assert isinstance(features, list)


@pytest.mark.asyncio
async def test_config(api_connection: veilid.VeilidAPI):
    cfgstr = await api_connection.default_veilid_config()
    print(f"default_veilid_config:\n{cfgstr}")
    cfgjson = json.loads(cfgstr)

    veilidConfigInstance = veilid.VeilidConfig(**cfgjson)
    cfgstr2 = json.dumps(dataclasses.asdict(veilidConfigInstance))

    cfgjson2 = json.loads(cfgstr2)
    veilidConfigInstance2 = veilid.VeilidConfig(**cfgjson2)

    assert veilidConfigInstance == veilidConfigInstance2


@pytest.mark.asyncio
async def test_change_config(api_connection: veilid.VeilidAPI):
    config = (await api_connection.get_state()).config.config

    # Attached, the change is refused
    with pytest.raises(veilid.VeilidAPIErrorGeneric):
        await api_connection.change_config(config)

    await api_connection.detach()
    await api_connection.wait_for_attachment_condition(
        veilid.VeilidStateAttachmentCondition(opt_max_state=veilid.AttachmentState.DETACHED)
    )
    try:
        # Detached, a changeable key takes at once; the config get_state reports
        # carries no secrets and still applies
        flipped = dataclasses.replace(
            config, network=dataclasses.replace(config.network, upnp=not config.network.upnp)
        )
        await api_connection.change_config(flipped)
        after = (await api_connection.get_state()).config.config
        assert after.network.upnp == flipped.network.upnp

        # A key that needs a restart is refused and nothing changes
        with pytest.raises(veilid.VeilidAPIErrorInvalidArgument):
            await api_connection.change_config(
                dataclasses.replace(flipped, namespace=flipped.namespace + "x")
            )
        assert (await api_connection.get_state()).config.config.namespace == flipped.namespace

        await api_connection.change_config(config)
    finally:
        await api_connection.attach()
        await api_connection.wait_for_attachment_condition()
