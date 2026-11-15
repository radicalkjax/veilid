import { veilidClient, veilidCrypto } from 'veilid-wasm';
import { textEncoder, unmarshallBytes } from '../utils/marshalling-utils.js';
import type { Suite, TestCase } from '../harness/types.js';

export const cryptoSuite: Suite = {
  name: 'crypto',
  requiresNetwork: false,
  cases: (): TestCase[] => {
    const kinds = veilidCrypto.VALID_CRYPTO_KINDS;
    const cases: TestCase[] = [
      {
        name: 'should list crypto kinds',
        run: ({ expect }) => {
          expect(kinds.length).toBeGreaterThan(0);
        },
      },
    ];

    for (const cryptoKind of kinds) {
      cases.push({
        name: `should generate key pair for ${cryptoKind}`,
        run: ({ expect }) => {
          const vcrypto = veilidClient.getCrypto(cryptoKind);
          const keypair = vcrypto.generateKeyPair();
          expect(typeof keypair).toBe('object');

          expect(keypair.kind).toEqual(cryptoKind);
          expect(unmarshallBytes(keypair.value.key.toString()).length).toBe(
            vcrypto.publicKeyLength(),
          );
          expect(unmarshallBytes(keypair.value.secret.toString()).length).toBe(
            vcrypto.secretKeyLength(),
          );

          expect(vcrypto.validateKeyPair(keypair.key, keypair.secret)).toBe(true);
        },
      });

      cases.push({
        name: `should generate random bytes for ${cryptoKind}`,
        run: ({ expect }) => {
          const vcrypto = veilidClient.getCrypto(cryptoKind);
          const bytes = vcrypto.randomBytes(64);
          expect(bytes instanceof Uint8Array).toBe(true);
          expect(bytes.length).toBe(64);
        },
      });

      cases.push({
        name: `should hash data and validate hash for ${cryptoKind}`,
        run: ({ expect }) => {
          const vcrypto = veilidClient.getCrypto(cryptoKind);
          const data = textEncoder.encode('this is my data🚀');
          const hash = vcrypto.generateHash(data);

          expect(hash).toBeDefined();
          expect(typeof hash).toBe('object');
          expect(vcrypto.validateHash(data, hash)).toBe(true);
        },
      });

      cases.push({
        name: `should hash and validate password for ${cryptoKind}`,
        run: ({ expect }) => {
          const vcrypto = veilidClient.getCrypto(cryptoKind);
          const password = textEncoder.encode('this is my data🚀');

          const saltLength = vcrypto.defaultSaltLength();
          expect(saltLength).toBeGreaterThan(0);

          const salt = vcrypto.randomBytes(saltLength);
          expect(salt instanceof Uint8Array).toBe(true);
          expect(salt.length).toBe(saltLength);

          const hash = vcrypto.hashPassword(password, salt);
          expect(hash).toBeDefined();
          expect(typeof hash).toBe('string');
          expect(vcrypto.verifyPassword(password, hash)).toBe(true);
        },
      });

      cases.push({
        name: `should aead encrypt and decrypt for ${cryptoKind}`,
        run: ({ expect }) => {
          const vcrypto = veilidClient.getCrypto(cryptoKind);
          const body = textEncoder.encode('This is an encoded body with my secret data in it🔥');
          const ad = textEncoder.encode('This is data associated with my secret data👋');

          const nonce = vcrypto.randomNonce();
          expect(typeof nonce).toBe('object');

          const sharedSecret = vcrypto.randomSharedSecret();
          expect(typeof sharedSecret).toBe('object');

          const encBody = vcrypto.encryptAead(body, nonce, sharedSecret, ad);
          expect(encBody instanceof Uint8Array).toBe(true);
          expect(encBody.length - body.length).toBe(vcrypto.aeadOverhead());

          const decBody = vcrypto.decryptAead(encBody, nonce, sharedSecret, ad);
          expect(decBody instanceof Uint8Array).toBe(true);
          expect(body).toEqual(decBody);
        },
      });

      cases.push({
        name: `should hpke seal and open for ${cryptoKind}`,
        run: async ({ expect }) => {
          const vcrypto = veilidClient.getCrypto(cryptoKind);
          const plaintext = textEncoder.encode('This is a sealed message🔒');
          const aad = textEncoder.encode('This is data bound to the seal👋');

          const keypair = vcrypto.generateKemKeyPair();
          const keypair2 = vcrypto.generateKemKeyPair();

          const sealed = vcrypto.hpkeSeal(keypair.key, aad, plaintext);
          expect(sealed instanceof Uint8Array).toBe(true);
          expect(vcrypto.hpkeOpen(keypair.secret, aad, sealed)).toEqual(plaintext);

          // wrong recipient
          await expect(() => vcrypto.hpkeOpen(keypair2.secret, aad, sealed)).toThrow();
          // mismatched aad
          await expect(() => vcrypto.hpkeOpen(keypair.secret, plaintext, sealed)).toThrow();
          // tampered blob
          const tampered = new Uint8Array(sealed);
          tampered[tampered.length - 1] ^= 0x80;
          await expect(() => vcrypto.hpkeOpen(keypair.secret, aad, tampered)).toThrow();
        },
      });

      cases.push({
        name: `should derive kem keys from signing keys for ${cryptoKind}`,
        run: ({ expect }) => {
          const vcrypto = veilidClient.getCrypto(cryptoKind);
          const plaintext = textEncoder.encode('This is a bridged message🌉');

          const signingKeypair = vcrypto.generateKeyPair();
          const ek = vcrypto.encapsulationKeyFromSigningKey(signingKeypair.key);
          const dk = vcrypto.decapsulationKeyFromSigningSecret(signingKeypair.secret);

          // bridged halves form a working KEM pair
          const sealed = vcrypto.hpkeSeal(ek, new Uint8Array(0), plaintext);
          expect(vcrypto.hpkeOpen(dk, new Uint8Array(0), sealed)).toEqual(plaintext);
        },
      });

      cases.push({
        name: `should sign and verify for ${cryptoKind}`,
        run: ({ expect }) => {
          const vcrypto = veilidClient.getCrypto(cryptoKind);
          const keypair = vcrypto.generateKeyPair();
          const data = textEncoder.encode('This is some data I am signing with my key 🔑');
          expect(typeof keypair).toBe('object');

          const sig = vcrypto.sign(keypair.key, keypair.secret, data);
          expect(typeof sig).toBe('object');
          expect(vcrypto.verify(keypair.key, data, sig)).toBe(true);
        },
      });
    }

    return cases;
  },
};
