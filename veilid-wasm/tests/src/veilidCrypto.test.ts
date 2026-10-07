import { expect } from '@wdio/globals';

import {
  LOG_LEVEL,
  veilidCoreInitConfig,
  veilidCoreStartupConfig,
} from './utils/veilid-config.js';

import { veilidClient, veilidCrypto } from 'veilid-wasm';
import { textEncoder, unmarshallBytes } from './utils/marshalling-utils.js';
import { getLogTimestamp } from './utils/wait-utils.js';

describe('veilidCrypto', () => {
  before('veilid startup', async () => {
    await veilidClient.initializeCore(veilidCoreInitConfig);
    await veilidClient.startupCore((_update) => {
      // Only print API logs to console if performance logs are disabled
      if (LOG_LEVEL === "Off" && _update.kind === 'Log') {
        const logTimestamp = getLogTimestamp()
        console.log(`${logTimestamp}: ${_update.message}`);
      }
    }, veilidCoreStartupConfig);
  });

  after('veilid shutdown', async () => {
    await veilidClient.shutdownCore();
  });

  it('should list crypto kinds', async () => {
    const kinds = veilidCrypto.VALID_CRYPTO_KINDS;
    await expect(kinds.length).toBeGreaterThan(0)
  });

  for (const cryptoKind of veilidCrypto.VALID_CRYPTO_KINDS) {
    it(`should generate key pair for ${cryptoKind as string}`, async () => {
      const vcrypto = veilidClient.getCrypto(cryptoKind)
      const keypair = vcrypto.generateKeyPair();
      await expect(typeof keypair).toBe('object');

      const keyPairKind = keypair.kind;
      const barePublicKey = keypair.value.key
      const bareSecretKey = keypair.value.secret
      await expect(keyPairKind).toEqual(cryptoKind);
      await expect(unmarshallBytes(barePublicKey.toString()).length).toBe(vcrypto.publicKeyLength());
      await expect(unmarshallBytes(bareSecretKey.toString()).length).toBe(vcrypto.secretKeyLength());

      const isValid = vcrypto.validateKeyPair(
        keypair.key,
        keypair.secret,
      );
      await expect(isValid).toBe(true);

    });
  }

  for (const cryptoKind of veilidCrypto.VALID_CRYPTO_KINDS) {
    it(`should generate random bytes for ${cryptoKind as string}`, async () => {
      const vcrypto = veilidClient.getCrypto(cryptoKind)
      const bytes = vcrypto.randomBytes(64);
      await expect(bytes instanceof Uint8Array).toBe(true);
      await expect(bytes.length).toBe(64);

    });
  }

  for (const cryptoKind of veilidCrypto.VALID_CRYPTO_KINDS) {
    it(`should hash data and validate hash for ${cryptoKind as string}`, async () => {
      const vcrypto = veilidClient.getCrypto(cryptoKind)
      const data = textEncoder.encode('this is my data🚀');
      const hash = vcrypto.generateHash(data);

      await expect(hash).toBeDefined();
      await expect(typeof hash).toBe('object');

      const isValid = vcrypto.validateHash(data, hash);
      await expect(isValid).toBe(true);
    });
  }

  for (const cryptoKind of veilidCrypto.VALID_CRYPTO_KINDS) {
    it(`should hash and validate password for ${cryptoKind as string}`, async () => {
      const vcrypto = veilidClient.getCrypto(cryptoKind)

      const password = textEncoder.encode('this is my data🚀');
      const saltLength = vcrypto.defaultSaltLength();
      await expect(saltLength).toBeGreaterThan(0);

      const salt = vcrypto.randomBytes(saltLength);
      await expect(salt instanceof Uint8Array).toBe(true);
      await expect(salt.length).toBe(saltLength);

      const hash = vcrypto.hashPassword(password, salt);
      await expect(hash).toBeDefined();
      await expect(typeof hash).toBe('string');

      const isValid = vcrypto.verifyPassword(password, hash);
      await expect(isValid).toBe(true);
    });
  }

  for (const cryptoKind of veilidCrypto.VALID_CRYPTO_KINDS) {
    it(`should aead encrypt and decrypt for ${cryptoKind as string}`, async () => {
      const vcrypto = veilidClient.getCrypto(cryptoKind)
      const body = textEncoder.encode(
        'This is an encoded body with my secret data in it🔥'
      );
      const ad = textEncoder.encode(
        'This is data associated with my secret data👋'
      );

      const nonce = vcrypto.randomNonce();
      await expect(typeof nonce).toBe('object');

      const sharedSecred = vcrypto.randomSharedSecret();
      await expect(typeof sharedSecred).toBe('object');

      const encBody = vcrypto.encryptAead(
        body,
        nonce,
        sharedSecred,
        ad
      );
      await expect(encBody instanceof Uint8Array).toBe(true);

      const overhead = vcrypto.aeadOverhead();
      await expect(encBody.length - body.length).toBe(overhead);

      const decBody = vcrypto.decryptAead(
        encBody,
        nonce,
        sharedSecred,
        ad
      );
      await expect(decBody instanceof Uint8Array).toBe(true);
      await expect(body).toEqual(decBody);
    });
  }
  for (const cryptoKind of veilidCrypto.VALID_CRYPTO_KINDS) {
    it(`should hpke seal and open for ${cryptoKind as string}`, async () => {
      const vcrypto = veilidClient.getCrypto(cryptoKind)
      const plaintext = textEncoder.encode('This is a sealed message🔒');
      const aad = textEncoder.encode('This is data bound to the seal👋');

      const keypair = vcrypto.generateKemKeyPair();
      const keypair2 = vcrypto.generateKemKeyPair();

      const sealed = vcrypto.hpkeSeal(keypair.key, aad, plaintext);
      await expect(sealed instanceof Uint8Array).toBe(true);

      const opened = vcrypto.hpkeOpen(keypair.secret, aad, sealed);
      await expect(opened).toEqual(plaintext);

      // wrong recipient
      await expect(() =>
        vcrypto.hpkeOpen(keypair2.secret, aad, sealed)
      ).toThrow();

      // mismatched aad
      await expect(() =>
        vcrypto.hpkeOpen(keypair.secret, plaintext, sealed)
      ).toThrow();

      // tampered blob
      const tampered = new Uint8Array(sealed);
      tampered[tampered.length - 1] ^= 0x80;
      await expect(() =>
        vcrypto.hpkeOpen(keypair.secret, aad, tampered)
      ).toThrow();
    });
  }
  for (const cryptoKind of veilidCrypto.VALID_CRYPTO_KINDS) {
    it(`should derive kem keys from signing keys for ${cryptoKind as string}`, async () => {
      const vcrypto = veilidClient.getCrypto(cryptoKind)
      const plaintext = textEncoder.encode('This is a bridged message🌉');

      const signingKeypair = vcrypto.generateKeyPair();
      const ek = vcrypto.encapsulationKeyFromSigningKey(signingKeypair.key);
      const dk = vcrypto.decapsulationKeyFromSigningSecret(
        signingKeypair.secret
      );

      // bridged halves form a working KEM pair
      const sealed = vcrypto.hpkeSeal(ek, new Uint8Array(0), plaintext);
      const opened = vcrypto.hpkeOpen(dk, new Uint8Array(0), sealed);
      await expect(opened).toEqual(plaintext);
    });
  }
  for (const cryptoKind of veilidCrypto.VALID_CRYPTO_KINDS) {
    it(`should sign and verify for ${cryptoKind as string}`, async () => {
      const vcrypto = veilidClient.getCrypto(cryptoKind)
      const keypair = vcrypto.generateKeyPair();
      const data = textEncoder.encode(
        'This is some data I am signing with my key 🔑'
      );
      await expect(typeof keypair).toBe('object');

      const publicKey = keypair.key;
      const secretKey = keypair.secret;

      const sig = vcrypto.sign(publicKey, secretKey, data);
      await expect(typeof sig).toBe('object');

      await expect(async () => {
        const res = vcrypto.verify(publicKey, data, sig);
        await expect(res).toBe(true);
      }).not.toThrow();
    });
  }

});
