# Crypto veilid tests

import pytest
import veilid
from veilid.api import CryptoSystem


@pytest.mark.asyncio
async def test_valid_crypto_kinds(api_connection: veilid.VeilidAPI):
    kinds = await api_connection.valid_crypto_kinds()
    assert len(kinds) > 0


@pytest.mark.asyncio
async def test_get_crypto_system(api_connection: veilid.VeilidAPI):
    for kind in await api_connection.valid_crypto_kinds():
        cs = await api_connection.get_crypto_system(kind)
        async with cs:
            assert await cs.kind() == kind


@pytest.mark.asyncio
async def test_get_crypto_system_invalid(api_connection: veilid.VeilidAPI):
    with pytest.raises(veilid.VeilidAPIErrorInvalidArgument) as exc:
        await api_connection.get_crypto_system(veilid.CryptoKind.CRYPTO_KIND_NONE)

    assert exc.value.context == "unsupported cryptosystem"
    assert exc.value.argument == "kind"
    assert exc.value.value == "NONE"


@pytest.mark.asyncio
async def test_hash_and_verify_password(api_connection: veilid.VeilidAPI):
    for kind in await api_connection.valid_crypto_kinds():
        cs = await api_connection.get_crypto_system(kind)
        async with cs:
            nonce = await cs.random_nonce()
            salt = nonce.to_bytes()

            # Password match
            phash = await cs.hash_password(b"abc123", salt)
            assert await cs.verify_password(b"abc123", phash)

            # Password mismatch
            await cs.hash_password(b"abc1234", salt)
            assert not await cs.verify_password(b"abc12345", phash)


@pytest.mark.asyncio
async def test_sign_and_verify_signature(api_connection: veilid.VeilidAPI):
    for kind in await api_connection.valid_crypto_kinds():
        cs = await api_connection.get_crypto_system(kind)
        async with cs:
            kp1 = await cs.generate_key_pair()
            kp2 = await cs.generate_key_pair()

            # Signature match
            sig = await cs.sign(kp1.key(), kp1.secret(), b"abc123")
            assert await cs.verify(kp1.key(), b"abc123", sig)

            # Signature mismatch
            sig2 = await cs.sign(kp1.key(), kp1.secret(), b"abc1234")
            assert await cs.verify(kp1.key(), b"abc1234", sig2)
            assert not await cs.verify(kp1.key(), b"abc12345", sig2)
            assert not await cs.verify(kp2.key(), b"abc1234", sig2)


@pytest.mark.asyncio
async def test_sign_and_verify_signatures(api_connection: veilid.VeilidAPI):
    for kind in await api_connection.valid_crypto_kinds():
        cs = await api_connection.get_crypto_system(kind)
        async with cs:
            kind = await cs.kind()
            kp1 = await cs.generate_key_pair()

            # BareSignature match
            sigs = await api_connection.generate_signatures(b"abc123", [kp1])
            keys = [kp1.key()]
            assert (await api_connection.verify_signatures(keys, b"abc123", sigs)) == keys

            # BareSignature mismatch
            assert (await api_connection.verify_signatures([kp1.key()], b"abc1234", sigs)) is None


@pytest.mark.asyncio
async def test_generate_shared_secret(api_connection: veilid.VeilidAPI):
    for kind in await api_connection.valid_crypto_kinds():
        cs = await api_connection.get_crypto_system(kind)
        async with cs:
            kp1 = await cs.generate_key_pair()
            kp2 = await cs.generate_key_pair()
            kp3 = await cs.generate_key_pair()

            ssA = await cs.generate_shared_secret(kp1.key(), kp2.secret(), b"abc123")
            ssB = await cs.generate_shared_secret(kp2.key(), kp1.secret(), b"abc123")

            assert ssA == ssB

            ssC = await cs.generate_shared_secret(kp2.key(), kp1.secret(), b"abc1234")

            assert ssA != ssC

            ssD = await cs.generate_shared_secret(kp3.key(), kp1.secret(), b"abc123")

            assert ssA != ssD


@pytest.mark.asyncio
async def test_hpke_seal_open(api_connection: veilid.VeilidAPI):
    for kind in await api_connection.valid_crypto_kinds():
        cs = await api_connection.get_crypto_system(kind)
        async with cs:
            kkp1 = await cs.generate_kem_key_pair()
            kkp2 = await cs.generate_kem_key_pair()
            assert kkp1 != kkp2

            aad = b"some associated data"
            plaintext = b"hello hpke"

            sealed = await cs.hpke_seal(kkp1.key(), aad, plaintext)
            assert await cs.hpke_open(kkp1.secret(), aad, sealed) == plaintext

            # empty aad and empty plaintext round-trip
            sealed_empty = await cs.hpke_seal(kkp1.key(), b"", b"")
            assert await cs.hpke_open(kkp1.secret(), b"", sealed_empty) == b""

            # wrong recipient
            with pytest.raises(veilid.VeilidAPIError):
                await cs.hpke_open(kkp2.secret(), aad, sealed)

            # mismatched aad
            with pytest.raises(veilid.VeilidAPIError):
                await cs.hpke_open(kkp1.secret(), b"other aad", sealed)

            # tampered blob
            tampered = bytearray(sealed)
            tampered[-1] ^= 0x80
            with pytest.raises(veilid.VeilidAPIError):
                await cs.hpke_open(kkp1.secret(), aad, bytes(tampered))


@pytest.mark.asyncio
async def test_kem_bridge_from_signing_keys(api_connection: veilid.VeilidAPI):
    for kind in await api_connection.valid_crypto_kinds():
        cs = await api_connection.get_crypto_system(kind)
        async with cs:
            kp = await cs.generate_key_pair()

            ek = await cs.encapsulation_key_from_signing_key(kp.key())
            dk = await cs.decapsulation_key_from_signing_secret(kp.secret())
            assert len(ek.value().to_bytes()) == await cs.encapsulation_key_length()
            assert len(dk.value().to_bytes()) == await cs.decapsulation_key_length()

            # bridged halves form a working KEM pair
            sealed = await cs.hpke_seal(ek, b"", b"bridged")
            assert await cs.hpke_open(dk, b"", sealed) == b"bridged"

