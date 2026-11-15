import {
  veilidClient,
  veilidCrypto,
  KeyPair,
  Signature,
  CryptoKind,
  VeilidAPIError,
  VeilidAPIErrorGeneric,
  VeilidAPIErrorInvalidArgument,
  VeilidAPIErrorNotInitialized,
  RecordKey,
} from 'veilid-wasm';
import { textEncoder } from '../utils/marshalling-utils.js';
import { dhtRetry } from '../utils/dht-retry.js';
import type { Suite, TestCase } from '../harness/types.js';

export const clientSuite: Suite = {
  name: 'client',
  requiresNetwork: false,
  cases: (): TestCase[] => [
    {
      name: 'should print version',
      run: ({ expect }) => {
        const version = veilidClient.versionString();
        expect(typeof version).toBe('string');
        expect(version.length).toBeGreaterThan(0);
      },
    },
    {
      name: 'should print features',
      run: ({ expect }) => {
        const features = veilidClient.features();
        expect(Array.isArray(features)).toBe(true);
        expect(features.length).toBeGreaterThan(0);
      },
    },
    {
      name: 'should throw Unimplemented for constants whose feature is compiled out',
      run: ({ expect }) => {
        const features = veilidClient.features();
        const gated: [string, () => unknown][] = [
          ['unstable-tunnels', () => veilidClient.VEILID_CAPABILITY_TUNNEL],
          ['unstable-blockstore', () => veilidClient.VEILID_CAPABILITY_BLOCKSTORE],
          ['enable-crypto-vld0', () => veilidCrypto.CRYPTO_KIND_VLD0],
          ['enable-crypto-none', () => veilidCrypto.CRYPTO_KIND_NONE],
        ];
        for (const [feature, read] of gated) {
          if (features.includes(feature)) {
            expect(typeof read()).toBe('string');
            continue;
          }
          let thrown: unknown;
          try {
            read();
          } catch (e) {
            thrown = e;
          }
          expect(thrown).toMatchObject({ kind: 'Unimplemented' });
        }
      },
    },
    {
      name: 'should get config',
      run: ({ expect }) => {
        const defaultConfig = veilidClient.defaultConfig();
        expect(typeof defaultConfig).toBe('object');

        expect(defaultConfig).toHaveProperty('programName');
        expect(defaultConfig).toHaveProperty('namespace');
        expect(defaultConfig).toHaveProperty('capabilities');
        expect(defaultConfig).toHaveProperty('protectedStore');
        expect(defaultConfig).toHaveProperty('tableStore');
        expect(defaultConfig).toHaveProperty('blockStore');
        expect(defaultConfig).toHaveProperty('network');
      },
    },
    {
      name: 'should sign and verify for all crypto kinds',
      run: ({ expect }) => {
        const keypairs: KeyPair[] = [];
        for (const cryptoKind of veilidCrypto.VALID_CRYPTO_KINDS) {
          const keypair = veilidClient.getCrypto(cryptoKind).generateKeyPair();
          expect(typeof keypair).toBe('object');
          keypairs.push(keypair);
        }

        const data = textEncoder.encode('This is some data I am signing with my key 🔑');

        let signatures: Signature[] = veilidClient.generateSignatures(data, keypairs);
        expect(typeof signatures).toBe('object');

        const publicKeys = keypairs.map((kp) => kp.key);

        let res = veilidClient.verifySignatures(publicKeys, data, signatures);
        expect(res).not.toBeUndefined();
        expect(res!.length).toEqual(publicKeys.length);

        signatures = [];
        res = veilidClient.verifySignatures(publicKeys, data, signatures);
        expect(res).not.toBeUndefined();
        expect(res!.length).toEqual(0);
      },
    },

    // Everything below needs an attached node.
    {
      name: 'should attach and detach',
      requiresNetwork: true,
      // The harness leaves the node attached, so exercise the transitions the other way round
      // and hand it back attached for the cases that follow.
      run: async () => {
        await veilidClient.detach();
        await veilidClient.waitForAttachmentCondition({ optMaxState: 'Detached' }, 20_000);
        await veilidClient.attach();
        await veilidClient.waitForAttachmentCondition(undefined, 30_000);
      },
    },
    {
      name: 'should get state',
      requiresNetwork: true,
      run: async ({ expect }) => {
        const state = await veilidClient.getState();
        expect(state.attachment).toBeDefined();
        expect(state.config.config).toBeDefined();
        expect(state.network).toBeDefined();
      },
    },
    {
      name: 'should call debug command',
      requiresNetwork: true,
      run: async ({ expect }) => {
        const response = await veilidClient.debug('help');
        expect(response).toBeDefined();
        // Release builds leave out debug-api, and then there is no help text to return.
        if (veilidClient.features().includes('debug-api')) {
          expect(response.length).toBeGreaterThan(0);
        }
      },
    },
    {
      name: 'should create/release a new private route',
      requiresNetwork: true,
      run: async ({ expect }) => {
        const routeBlob = await veilidClient.newPrivateRoute();
        expect(routeBlob).toBeDefined();
        expect(routeBlob.blob.length).toBeGreaterThan(0);
        expect(routeBlob.routeId).toBeDefined();

        veilidClient.releasePrivateRoute(routeBlob.routeId);
      },
    },
    {
      name: 'should create/release a new custom private route with default values',
      requiresNetwork: true,
      run: async ({ expect }) => {
        const routeBlob = await dhtRetry(() => veilidClient.newCustomPrivateRoute({}), {
          label: 'newCustomPrivateRoute',
        });
        expect(routeBlob).toBeDefined();
        expect(routeBlob.blob.length).toBeGreaterThan(0);
        expect(routeBlob.routeId).toBeDefined();

        veilidClient.releasePrivateRoute(routeBlob.routeId);
      },
    },
    {
      name: 'should create/release a new custom private route with specified values',
      requiresNetwork: true,
      run: async ({ expect }) => {
        let routeBlob = await dhtRetry(
          () =>
            veilidClient.newCustomPrivateRoute({
              cryptoKinds: [veilidCrypto.CRYPTO_KIND_VLD0],
              hopCount: 1,
              stability: 'LowLatency',
              sequencing: 'EnsureOrdered',
            }),
          { label: 'newCustomPrivateRoute with specified values' },
        );
        expect(routeBlob).toBeDefined();
        expect(routeBlob.blob.length).toBeGreaterThan(0);
        expect(routeBlob.routeId).toBeDefined();

        veilidClient.releasePrivateRoute(routeBlob.routeId);

        routeBlob = await dhtRetry(
          () =>
            veilidClient.newCustomPrivateRoute({
              cryptoKinds: [],
              hopCount: 0,
              stability: 'LowLatency',
              sequencing: 'EnsureOrdered',
            }),
          {
            label:
              'newCustomPrivateRoute with specified values (no crypto kinds, default hop count)',
          },
        );
        expect(routeBlob).toBeDefined();
        expect(routeBlob.blob.length).toBeGreaterThan(0);
        expect(routeBlob.routeId).toBeDefined();

        veilidClient.releasePrivateRoute(routeBlob.routeId);
      },
    },
    {
      name: 'should fail to create a new custom private route with invalid crypto kind',
      requiresNetwork: true,
      run: async ({ expect }) => {
        try {
          await dhtRetry(
            () =>
              veilidClient.newCustomPrivateRoute({
                cryptoKinds: [veilidCrypto.CRYPTO_KIND_VLD0, 'BAAD' as CryptoKind],
                hopCount: 1,
                stability: 'LowLatency',
                sequencing: 'EnsureOrdered',
              }),
            { label: 'newCustomPrivateRoute with invalid crypto kind' },
          );
        } catch (error) {
          expect(error).toBeInstanceOf(VeilidAPIErrorGeneric);
          expect(error).toMatchObject({ kind: 'Generic' });
        }
      },
    },
    {
      name: 'should fail to create a new custom private route with large hop count',
      requiresNetwork: true,
      run: async ({ expect }) => {
        try {
          await dhtRetry(
            () =>
              veilidClient.newCustomPrivateRoute({
                cryptoKinds: [veilidCrypto.CRYPTO_KIND_VLD0],
                hopCount: 100,
                stability: 'LowLatency',
                sequencing: 'EnsureOrdered',
              }),
            { label: 'newCustomPrivateRoute with large hop count' },
          );
        } catch (error) {
          expect(error).toBeInstanceOf(VeilidAPIErrorInvalidArgument);
          expect(error).toMatchObject({ kind: 'InvalidArgument' });
        }
      },
    },
    {
      name: 'should throw Error subclasses carrying a kind',
      run: ({ expect }) => {
        // Thrown from veilid-core's own export, which reaches the classes through the
        // factory rather than veilid-wasm's bindings
        try {
          RecordKey.parse('!!!not-a-key!!!');
          throw new Error('parse should have thrown');
        } catch (error) {
          expect(error).toBeInstanceOf(VeilidAPIError);
          expect(error).toBeInstanceOf(Error);
          expect(error).not.toBeInstanceOf(VeilidAPIErrorNotInitialized);
          const e = error as VeilidAPIError;
          expect(typeof e.kind).toBe('string');
          expect(typeof e.message).toBe('string');
          expect(e.message.length).toBeGreaterThan(0);
          expect(typeof e.stack).toBe('string');
          expect(e.name).toBe(e.constructor.name);
        }
      },
    },
  ],
};
