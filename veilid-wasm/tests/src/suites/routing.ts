import {
  KeyPair,
  RecordKey,
  RouteId,
  VeilidAppCall,
  VeilidAppMessage,
  VeilidRoutingContext,
  veilidClient,
  veilidCrypto,
} from 'veilid-wasm';
import { textEncoder } from '../utils/marshalling-utils.js';
import {
  asyncCallWithTimeout,
  sendUntilReceived,
  waitForSingleEvent,
} from '../utils/wait-utils.js';
import { appCallEventTarget, appMessageEventTarget } from '../harness/app-events.js';
import type { Suite, TestCase } from '../harness/types.js';

export const routingSuite: Suite = {
  name: 'routing',
  requiresNetwork: true,
  cases: (): TestCase[] => {
    const cases: TestCase[] = [
      {
        name: 'should create using .create()',
        requiresNetwork: false,
        run: ({ expect }) => {
          expect(VeilidRoutingContext.create() instanceof VeilidRoutingContext).toBe(true);
        },
      },
      {
        name: 'should create using new',
        requiresNetwork: false,
        run: ({ expect }) => {
          expect(new VeilidRoutingContext() instanceof VeilidRoutingContext).toBe(true);
        },
      },
      {
        name: 'should create with default safety',
        requiresNetwork: false,
        run: ({ expect }) => {
          const rc = VeilidRoutingContext.create().withDefaultSafety();
          expect(rc instanceof VeilidRoutingContext).toBe(true);
        },
      },
      {
        name: 'should create with safety',
        requiresNetwork: false,
        run: ({ expect }) => {
          const rc = VeilidRoutingContext.create().withSafety({
            Safe: { hopCount: 2, sequencing: 'EnsureOrdered', stability: 'Reliable' },
          });
          expect(rc instanceof VeilidRoutingContext).toBe(true);
        },
      },
      {
        name: 'should fail to create safety with invalid hop count',
        requiresNetwork: false,
        run: ({ expect }) => {
          try {
            const rc = VeilidRoutingContext.create().withSafety({
              Safe: { hopCount: 100, sequencing: 'EnsureOrdered', stability: 'Reliable' },
            });
            throw new Error(`Expected error but got ${JSON.stringify(rc)}`);
          } catch (error) {
            expect(error).toMatchObject({ kind: 'Generic' });
          }
        },
      },
      {
        name: 'should fail to create safety with invalid route',
        requiresNetwork: false,
        run: ({ expect }) => {
          try {
            const rc = VeilidRoutingContext.create().withSafety({
              Safe: {
                preferredRoute: ['ass'] as unknown as RouteId,
                hopCount: 1,
                sequencing: 'EnsureOrdered',
                stability: 'Reliable',
              },
            });
            throw new Error(`Expected error but got ${JSON.stringify(rc)}`);
          } catch (error) {
            expect(error).toMatchObject({ kind: 'Generic' });
          }
        },
      },
      {
        name: 'should create with sequencing',
        requiresNetwork: false,
        run: ({ expect }) => {
          const rc = VeilidRoutingContext.create().withSequencing('EnsureOrdered');
          expect(rc instanceof VeilidRoutingContext).toBe(true);
        },
      },
      {
        name: 'should error if unsafe is used',
        requiresNetwork: false,
        run: ({ expect }) => {
          try {
            const rc = VeilidRoutingContext.create().withSafety({
              Unsafe: 'EnsureOrdered',
            });
            throw new Error(`Expected error but got ${JSON.stringify(rc)}`);
          } catch (error) {
            expect(error).toMatchObject({ kind: 'Generic' });
          }
        },
      },
    ];

    for (const cryptoKind of veilidCrypto.VALID_CRYPTO_KINDS) {
      cases.push({
        name: `should create a new private route, send an app message to it, receive it, and release the route for ${cryptoKind}`,
        run: async ({ expect }) => {
          const routingContext = VeilidRoutingContext.create();
          const routeBlob = await veilidClient.newCustomPrivateRoute({
            cryptoKinds: [cryptoKind],
          });
          expect(routeBlob).toBeDefined();
          expect(routeBlob.blob.length).toBeGreaterThan(0);
          expect(routeBlob.routeId).toBeDefined();

          const routeId = veilidClient.importRemotePrivateRoute(routeBlob.blob);
          expect(routeId).toBeDefined();

          const message = textEncoder.encode('This is a test message');

          // App messages are fire-and-forget; on WASM a slow/flapping route can drop one, so
          // resend until received.
          const receivedMessage = await sendUntilReceived<CustomEvent<VeilidAppMessage>>(
            () => routingContext.appMessage({ RouteId: routeId }, message),
            appMessageEventTarget,
            'appMessage',
          );
          expect(receivedMessage.detail.message).toEqual(message);

          veilidClient.releasePrivateRoute(routeId);
          veilidClient.releasePrivateRoute(routeBlob.routeId);
        },
      });

      cases.push({
        name: `should create a new private route, send an app call to it, receive it, reply to it, and then receive the reply and release the route for ${cryptoKind}`,
        run: async ({ expect }) => {
          const routingContext = VeilidRoutingContext.create();
          const routeBlob = await veilidClient.newCustomPrivateRoute({
            cryptoKinds: [cryptoKind],
          });
          expect(routeBlob).toBeDefined();
          expect(routeBlob.blob.length).toBeGreaterThan(0);
          expect(routeBlob.routeId).toBeDefined();

          const routeId = veilidClient.importRemotePrivateRoute(routeBlob.blob);
          expect(routeId).toBeDefined();

          const message = textEncoder.encode('This is a test message');
          const reply = textEncoder.encode('This is a test reply');

          const appCallOneShot = waitForSingleEvent<CustomEvent<VeilidAppCall>>(
            appCallEventTarget,
            'appCall',
          );
          const appCallPromise = routingContext.appCall({ RouteId: routeId }, message);

          const receivedCall = await asyncCallWithTimeout<CustomEvent<VeilidAppCall>>(
            appCallOneShot,
            30_000,
          );
          expect(receivedCall.detail.message).toEqual(message);

          await veilidClient.appCallReply(receivedCall.detail.callId, reply);

          const returnedReply = await asyncCallWithTimeout<Uint8Array>(appCallPromise, 30_000);
          expect(returnedReply).toEqual(reply);

          veilidClient.releasePrivateRoute(routeId);
          veilidClient.releasePrivateRoute(routeBlob.routeId);
        },
      });

      cases.push({
        name: `should create dht record with default schema for ${cryptoKind}`,
        run: async ({ expect }) => {
          const routingContext = VeilidRoutingContext.create();
          const dhtRecord = await routingContext.createDHTRecord(cryptoKind, {
            DFLT: { oCnt: 1 },
          });
          expect(dhtRecord.key).toBeInstanceOf(RecordKey);
          expect(dhtRecord.owner).toBeInstanceOf(KeyPair);
          expect(dhtRecord.schema).toEqual({ DFLT: { oCnt: 1 } });
        },
      });

      cases.push({
        name: `should create dht record with default schema, no owner for ${cryptoKind}`,
        run: async ({ expect }) => {
          const routingContext = VeilidRoutingContext.create();
          const dhtRecord = await routingContext.createDHTRecord(cryptoKind, {
            DFLT: { oCnt: 1 },
          });
          expect(dhtRecord.key).toBeInstanceOf(RecordKey);
          expect(dhtRecord.owner).toBeInstanceOf(KeyPair);
          expect(dhtRecord.schema).toEqual({ DFLT: { oCnt: 1 } });
        },
      });

      cases.push({
        name: `should create dht record with default schema, with owner, and a deterministic key for ${cryptoKind}`,
        run: async ({ expect }) => {
          const routingContext = VeilidRoutingContext.create();
          const vcrypto = veilidClient.getCrypto(cryptoKind);
          const ownerKeyPair = vcrypto.generateKeyPair();
          const owner = ownerKeyPair.key;

          const dhtRecord = await routingContext.createDHTRecord(
            cryptoKind,
            { DFLT: { oCnt: 1 } },
            ownerKeyPair,
          );
          const dhtRecordKey = await veilidClient.getDHTRecordKey(
            { DFLT: { oCnt: 1 } },
            owner,
            dhtRecord.key.encryptionKey,
          );
          expect(dhtRecord.key).toBeInstanceOf(RecordKey);
          expect(dhtRecord.key.isEqual(dhtRecordKey)).toEqual(true);
          expect(dhtRecord.owner).toBeInstanceOf(KeyPair);
          expect((dhtRecord.owner as KeyPair).isEqual(ownerKeyPair)).toEqual(true);
          expect(dhtRecord.schema).toEqual({ DFLT: { oCnt: 1 } });
        },
      });
    }

    return cases;
  },
};
