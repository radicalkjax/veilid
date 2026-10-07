import { expect } from '@wdio/globals';

import {
  LOG_LEVEL,
  veilidCoreInitConfig,
  veilidCoreStartupConfig,
} from './utils/veilid-config.js';

import { veilidClient, veilidCrypto, KeyPair, Signature, CryptoKind /*, Sequencing, Stability */ } from 'veilid-wasm';
import { asyncCallWithTimeout, getLogTimestamp, waitForDetached, waitForPublicAttachment, waitForShutdown } from './utils/wait-utils.js';
import { textEncoder } from './utils/marshalling-utils.js';
import { dhtRetry } from './utils/dht-retry.js';

describe('veilidClient', () => {
  before('veilid startup', async () => {
    // console.log("---Init---");
    await veilidClient.initializeCore(veilidCoreInitConfig);
    // console.log("---Startup---");
    // console.log("config: ", veilidCoreStartupConfig);
    await veilidClient.startupCore((_update) => {
      // Only print API logs to console if performance logs are disabled
      if (LOG_LEVEL === "Off" && _update.kind === 'Log') {
        const logTimestamp = getLogTimestamp()
        console.log(`${logTimestamp}: ${_update.message}`);
      }
    }, veilidCoreStartupConfig);
    // console.log("---Started Up---");
  });

  after('veilid shutdown', async () => {
    // console.log("---Shutting Down---");
    await veilidClient.shutdownCore();
    await asyncCallWithTimeout(waitForShutdown(), 10000);
  });

  it('should print version', async () => {
    const version = veilidClient.versionString();
    await expect(typeof version).toBe('string');
    await expect(version.length).toBeGreaterThan(0);
  });

  it('should print features', async () => {
    const features = veilidClient.features();
    await expect(Array.isArray(features)).toBe(true);
    await expect(features.length).toBeGreaterThan(0);
  });

  it('should get config', async () => {
    const defaultConfig = veilidClient.defaultConfig();
    await expect(typeof defaultConfig).toBe('object');

    await expect(defaultConfig).toHaveProperty('programName');
    await expect(defaultConfig).toHaveProperty('namespace');
    await expect(defaultConfig).toHaveProperty('capabilities');
    await expect(defaultConfig).toHaveProperty('protectedStore');
    await expect(defaultConfig).toHaveProperty('tableStore');
    await expect(defaultConfig).toHaveProperty('blockStore');
    await expect(defaultConfig).toHaveProperty('network');
  });

  it('should attach and detach', async () => {
    await veilidClient.attach();
    await asyncCallWithTimeout(waitForPublicAttachment(), 30_000);
    await veilidClient.detach();
    await asyncCallWithTimeout(waitForDetached(), 20_000);
  });

  describe('kitchen sink', () => {
    before('attach', async () => {
      await veilidClient.attach();
      await asyncCallWithTimeout(waitForPublicAttachment(), 30_000);
    });
    after('detach', async () => {
      await veilidClient.detach();
      await asyncCallWithTimeout(waitForDetached(), 20_000);
    });


    it('should get state', async () => {
      const state = await veilidClient.getState();
      await expect(state.attachment).toBeDefined();
      await expect(state.config.config).toBeDefined();
      await expect(state.network).toBeDefined();
    });

    it('should call debug command', async () => {
      const response = await veilidClient.debug('help');
      await expect(response).toBeDefined();
      await expect(response.length).toBeGreaterThan(0);
    });
  });

  describe('global private route functions', () => {
    before('attach', async () => {
      await veilidClient.attach();
      await asyncCallWithTimeout(waitForPublicAttachment(), 30_000);
    });
    after('detach', async () => {
      await veilidClient.detach();
      await asyncCallWithTimeout(waitForDetached(), 20_000);
    });

    it('should create/release a new private route', async () => {
      const routeBlob = await veilidClient.newPrivateRoute();
      await expect(routeBlob).toBeDefined();
      await expect(routeBlob.blob.length).toBeGreaterThan(0);
      await expect(routeBlob.routeId).toBeDefined();

      veilidClient.releasePrivateRoute(routeBlob.routeId);
    });

    it('should create/release a new custom private route with default values', async () => {
      const routeBlob = await dhtRetry(
        () => veilidClient.newCustomPrivateRoute({}),
        { label: 'newCustomPrivateRoute' },
      );
      await expect(routeBlob).toBeDefined();
      await expect(routeBlob.blob.length).toBeGreaterThan(0);
      await expect(routeBlob.routeId).toBeDefined();

      veilidClient.releasePrivateRoute(routeBlob.routeId);
    });

    it('should create/release a new custom private route with specified values', async () => {
      let routeBlob = await dhtRetry(
        () => veilidClient.newCustomPrivateRoute({
          cryptoKinds: [veilidCrypto.CRYPTO_KIND_VLD0],
          hopCount: 1,
          stability: "LowLatency",
          sequencing: "EnsureOrdered",
        }),
        { label: 'newCustomPrivateRoute with specified values' },
      );
      await expect(routeBlob).toBeDefined();
      await expect(routeBlob.blob.length).toBeGreaterThan(0);
      await expect(routeBlob.routeId).toBeDefined();

      veilidClient.releasePrivateRoute(routeBlob.routeId);

      routeBlob = await dhtRetry(
        () => veilidClient.newCustomPrivateRoute({
          cryptoKinds: [],
          hopCount: 0,
          stability: "LowLatency",
          sequencing: "EnsureOrdered",
        }),
        { label: 'newCustomPrivateRoute with specified values (no crypto kinds, default hop count)' },
      );
      await expect(routeBlob).toBeDefined();
      await expect(routeBlob.blob.length).toBeGreaterThan(0);
      await expect(routeBlob.routeId).toBeDefined();

      veilidClient.releasePrivateRoute(routeBlob.routeId);
    });

    it('should fail to create a new custom private route with invalid crypto kind', async () => {
      try {
        await dhtRetry(
          () => veilidClient.newCustomPrivateRoute({
            cryptoKinds: [veilidCrypto.CRYPTO_KIND_VLD0, "BAAD" as CryptoKind],
            hopCount: 1,
            stability: "LowLatency",
            sequencing: "EnsureOrdered",
          }),
          { label: 'newCustomPrivateRoute with invalid crypto kind' },
        );
      } catch (error) {
        await expect(error).toMatchObject({ kind: "Generic" });
      }
    });

    // // Can't catch WASM trap errors
    // it('should fail to create a new custom private route with negative hop count', async () => {
    //   try {
    //     await veilidClient.newCustomPrivateRoute({
    //       cryptoKinds: [veilidCrypto.CRYPTO_KIND_VLD0],
    //       hopCount: -1,
    //       stability: "LowLatency",
    //       sequencing: "EnsureOrdered",
    //     })
    //   } catch (error) {
    //     await expect(error).toBeDefined();
    //   }
    // });

    it('should fail to create a new custom private route with large hop count', async () => {
      try {
        await dhtRetry(
          () => veilidClient.newCustomPrivateRoute({
            cryptoKinds: [veilidCrypto.CRYPTO_KIND_VLD0],
            hopCount: 100,
            stability: "LowLatency",
            sequencing: "EnsureOrdered",
          }),
          { label: 'newCustomPrivateRoute with large hop count' },
        );
      } catch (error) {
        await expect(error).toMatchObject({ kind: "InvalidArgument" });
      }
    });

    // // Can't catch WASM trap errors
    // it('should fail to create a new custom private route with invalid stability', async () => {
    //   try {
    //     await veilidClient.newCustomPrivateRoute({
    //       cryptoKinds: [veilidCrypto.CRYPTO_KIND_VLD0],
    //       hopCount: 1,
    //       stability: "BadLatency" as Stability,
    //       sequencing: "EnsureOrdered",
    //     })
    //   } catch (error) {
    //     await expect(error).toBeDefined();
    //   }
    // });

    // // Can't catch WASM trap errors
    // it('should fail to create a new custom private route with invalid sequencing', async () => {
    //   try {
    //     await veilidClient.newCustomPrivateRoute({
    //       cryptoKinds: [veilidCrypto.CRYPTO_KIND_VLD0],
    //       hopCount: 1,
    //       stability: "LowLatency",
    //       sequencing: "BadOrdered" as Sequencing,
    //     });
    //   } catch (error) {
    //     await expect(error).toBeDefined();
    //   }
    // });
  });

  describe('global crypto functions', () => {

    it(`should sign and verify for all crypto kinds`, async () => {

      const keypairs: KeyPair[] = []
      for (const cryptoKind of veilidCrypto.VALID_CRYPTO_KINDS) {
        const vcrypto = veilidClient.getCrypto(cryptoKind)
        const keypair = vcrypto.generateKeyPair();
        await expect(typeof keypair).toBe('object');

        keypairs.push(keypair);
      }

      const data = textEncoder.encode(
        'This is some data I am signing with my key 🔑'
      );

      let signatures: Signature[];
      await expect(async () => {
        signatures = veilidClient.generateSignatures(data, keypairs);
        await expect(typeof signatures).toBe('object');
      }).not.toThrow();

      const publicKeys = keypairs.map((kp) => kp.key)

      await expect(async () => {
        const res = veilidClient.verifySignatures(publicKeys, data, signatures);
        await expect(res).not.toBeUndefined();
        await expect(res!.length).toEqual(publicKeys.length);
      }).not.toThrow();

      signatures = []
      await expect(async () => {
        const res = veilidClient.verifySignatures(publicKeys, data, signatures);
        await expect(res).not.toBeUndefined();
        await expect(res!.length).toEqual(0);
      }).not.toThrow();

    });
  })
});
