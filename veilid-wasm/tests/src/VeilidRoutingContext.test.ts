import { expect } from '@wdio/globals';

import {
  LOG_LEVEL,
  STRESS,
  veilidCoreInitConfig,
  veilidCoreStartupConfig,
} from './utils/veilid-config.js';

import {
  DHTRecordDescriptor,
  DHTRecordReport,
  KeyPair,
  RecordKey,
  RouteId,
  ValueData,
  VeilidAppCall,
  VeilidAppMessage,
  VeilidDHTTransaction,
  VeilidRoutingContext,
  veilidClient,
  veilidCrypto
} from 'veilid-wasm';
import { textEncoder, textDecoder } from './utils/marshalling-utils.js';
import { asyncCallWithTimeout, sendUntilReceived, waitForPublicAttachment as waitForPublicInternetReady, waitForOfflineSubkeyWrite, getLogTimestamp, waitForSingleEvent, waitForDetached } from './utils/wait-utils.js';
import { dhtRetry } from './utils/dht-retry.js';

const appMessageEventTarget = new EventTarget();
const appCallEventTarget = new EventTarget();

describe('VeilidRoutingContext', () => {
  before('veilid startup', async () => {
    await veilidClient.initializeCore(veilidCoreInitConfig);
    await veilidClient.startupCore((_update) => {
      // Only print API logs to console if performance logs are disabled
      if (LOG_LEVEL === "Off" && _update.kind === 'Log') {
        const logTimestamp = getLogTimestamp()
        console.log(`${logTimestamp}: ${_update.message}`);
      }
      else if (_update.kind === 'AppMessage') {
        appMessageEventTarget.dispatchEvent(new CustomEvent<VeilidAppMessage>('appMessage', { detail: _update }));
      }
      else if (_update.kind === 'AppCall') {
        appCallEventTarget.dispatchEvent(new CustomEvent<VeilidAppCall>('appCall', { detail: _update }));
      }
    }, veilidCoreStartupConfig);
    await veilidClient.attach();
    await asyncCallWithTimeout(waitForPublicInternetReady(), 30_000);
    //console.log("---Started Up---");
  });

  after('veilid shutdown', async () => {
    //console.log("---Shutting Down---");
    await veilidClient.detach();
    await asyncCallWithTimeout(waitForDetached(), 20_000);
    await veilidClient.shutdownCore();
  });

  describe('constructors', () => {
    it('should create using .create()', async () => {
      const routingContext = VeilidRoutingContext.create();
      await expect(routingContext instanceof VeilidRoutingContext).toBe(true);
    });

    it('should create using new', () => {
      const routingContext = new VeilidRoutingContext();
      void expect(routingContext instanceof VeilidRoutingContext).toBe(true);
    });

    it('should create with default safety', async () => {
      const routingContext = VeilidRoutingContext.create().withDefaultSafety();
      await expect(routingContext instanceof VeilidRoutingContext).toBe(true);
    });

    it('should create with safety', async () => {
      const routingContext = VeilidRoutingContext.create().withSafety({
        Safe: {
          hopCount: 2,
          sequencing: 'EnsureOrdered',
          stability: 'Reliable',
        },
      });
      await expect(routingContext instanceof VeilidRoutingContext).toBe(true);
    });

    it('should fail to create safety with invalid hop count', async () => {
      try {
        const routingContext = VeilidRoutingContext.create().withSafety({
          Safe: {
            hopCount: 100,
            sequencing: 'EnsureOrdered',
            stability: 'Reliable',
          },
        });
        throw new Error(`Expected error but got ${JSON.stringify(routingContext)}`);
      } catch (error) {
        await expect(error).toMatchObject({ kind: 'Generic' });
      }
    });

    it('should fail to create safety with invalid route', async () => {

      try {
        const routingContext = VeilidRoutingContext.create().withSafety({
          Safe: {
            preferredRoute: ["ass"] as unknown as RouteId,
            hopCount: 1,
            sequencing: 'EnsureOrdered',
            stability: 'Reliable',
          },
        });
        throw new Error(`Expected error but got ${JSON.stringify(routingContext)}`);
      } catch (error) {
        await expect(error).toMatchObject({ kind: 'Generic' });
      }
    });

    it('should create with sequencing', async () => {
      const routingContext =
        VeilidRoutingContext.create().withSequencing('EnsureOrdered');
      await expect(routingContext instanceof VeilidRoutingContext).toBe(true);
    });

    it('should error if unsafe is used', async () => {
      try {
        const routingContext = VeilidRoutingContext.create().withSafety({
          Unsafe: 'EnsureOrdered',
        });
        throw new Error(`Expected error but got ${JSON.stringify(routingContext)}`);
      } catch (error) {
        await expect(error).toMatchObject({ kind: 'Generic' });
      }
    });
  });

  describe('operations', () => {
    let routingContext: VeilidRoutingContext;

    before('create routing context', () => {
      // routingContext = VeilidRoutingContext.create().withSafety({
      //   Unsafe: 'EnsureOrdered'
      // });

      routingContext = VeilidRoutingContext.create();
    });

    for (const cryptoKind of veilidCrypto.VALID_CRYPTO_KINDS) {

      it('should create a new private route, send an app message to it, receive it, and release the route for ${cryptoKind as string}', async () => {
        const routeBlob = await veilidClient.newCustomPrivateRoute({ cryptoKinds: [cryptoKind] });
        await expect(routeBlob).toBeDefined();
        await expect(routeBlob.blob.length).toBeGreaterThan(0);
        await expect(routeBlob.routeId).toBeDefined();

        const routeId = veilidClient.importRemotePrivateRoute(routeBlob.blob);
        await expect(routeId).toBeDefined();

        const message = textEncoder.encode('This is a test message');

        // App messages are fire-and-forget; on WASM a slow/flapping route can drop one, so resend until received.
        const receivedMessage = await sendUntilReceived<CustomEvent<VeilidAppMessage>>(
          () => routingContext.appMessage({ RouteId: routeId }, message),
          appMessageEventTarget,
          'appMessage',
        );
        await expect(receivedMessage.detail.message).toStrictEqual(message);

        veilidClient.releasePrivateRoute(routeId);
        veilidClient.releasePrivateRoute(routeBlob.routeId);
      });

      it('should create a new private route, send an app call to it, receive it, reply to it, and then receive the reply and release the route for ${cryptoKind as string}', async () => {
        const routeBlob = await veilidClient.newCustomPrivateRoute({ cryptoKinds: [cryptoKind] });
        await expect(routeBlob).toBeDefined();
        await expect(routeBlob.blob.length).toBeGreaterThan(0);
        await expect(routeBlob.routeId).toBeDefined();

        const routeId = veilidClient.importRemotePrivateRoute(routeBlob.blob);
        await expect(routeId).toBeDefined();

        const message = textEncoder.encode('This is a test message');
        const reply = textEncoder.encode('This is a test reply');

        const appCallOneShot = waitForSingleEvent<CustomEvent<VeilidAppCall>>(appCallEventTarget, 'appCall');

        const appCallPromise = routingContext.appCall({ RouteId: routeId }, message);

        const receivedCall = await asyncCallWithTimeout<CustomEvent<VeilidAppCall>>(appCallOneShot, 30_000);
        await expect(receivedCall.detail.message).toStrictEqual(message);

        await veilidClient.appCallReply(receivedCall.detail.callId, reply);

        const returnedReply = await asyncCallWithTimeout<Uint8Array>(appCallPromise, 30_000);
        await expect(returnedReply).toStrictEqual(reply);

        veilidClient.releasePrivateRoute(routeId);
        veilidClient.releasePrivateRoute(routeBlob.routeId);
      });

      describe(`createDhtRecord for ${cryptoKind as string}`, () => {
        it('should create dht record with default schema', async () => {
          const dhtRecord = await routingContext.createDHTRecord(cryptoKind, { 'DFLT': { oCnt: 1 } });
          await expect(dhtRecord.key).toBeInstanceOf(RecordKey);
          await expect(dhtRecord.owner).toBeInstanceOf(KeyPair);
          await expect(dhtRecord.schema).toEqual({ 'DFLT': { oCnt: 1 } });
        });

        it('should create dht record with default schema, no owner for', async () => {
          const dhtRecord = await routingContext.createDHTRecord(cryptoKind, { 'DFLT': { oCnt: 1 } });
          await expect(dhtRecord.key).toBeInstanceOf(RecordKey);
          await expect(dhtRecord.owner).toBeInstanceOf(KeyPair);
          await expect(dhtRecord.schema).toEqual({ 'DFLT': { oCnt: 1 } });
        });

        it('should create dht record with default schema, with owner, and a deterministic key', async () => {
          const vcrypto = veilidClient.getCrypto(cryptoKind)
          const ownerKeyPair = vcrypto.generateKeyPair();
          const owner = ownerKeyPair.key
          const dhtRecord = await routingContext.createDHTRecord(cryptoKind, { 'DFLT': { oCnt: 1 } }, ownerKeyPair);
          const dhtRecordKey = await veilidClient.getDHTRecordKey({ 'DFLT': { oCnt: 1 } }, owner, dhtRecord.key.encryptionKey);
          await expect(dhtRecord.key).toBeInstanceOf(RecordKey);
          await expect(dhtRecord.key.isEqual(dhtRecordKey)).toEqual(true);
          await expect(dhtRecord.owner).toBeInstanceOf(KeyPair);
          await expect((dhtRecord.owner as KeyPair).isEqual(ownerKeyPair)).toEqual(true);
          await expect(dhtRecord.schema).toEqual({ 'DFLT': { oCnt: 1 } });
        });
      });

      describe(`DHT kitchen sink for ${cryptoKind as string}`, () => {
        let dhtRecord: DHTRecordDescriptor;
        const data = '🚀 This example DHT data with unicode a Ā 𐀀 文 🚀';

        beforeEach('create dht record', async () => {
          dhtRecord = await routingContext.createDHTRecord(
            cryptoKind,
            { 'DFLT': { oCnt: 1 } },
          );

          await expect(dhtRecord.key).toBeInstanceOf(RecordKey);
          await expect(dhtRecord.owner).toBeInstanceOf(KeyPair);
          await expect(dhtRecord.schema).toEqual({ 'DFLT': { oCnt: 1 } });
        });

        afterEach('free dht record', async () => {
          await routingContext.deleteDHTRecord(dhtRecord.key);
        });

        it('should set value', async () => {
          const setValueRes = await dhtRetry(
            () => routingContext.setDHTValue(dhtRecord.key, 0, textEncoder.encode(data)),
            { label: 'setDHTValue' },
          );
          await expect(setValueRes).toBeUndefined();
        });

        it('should get value with force refresh', async () => {

          const setValueRes = await dhtRetry(
            () => routingContext.setDHTValue(dhtRecord.key, 0, textEncoder.encode(data)),
            { label: 'setDHTValue' },
          );
          await expect(setValueRes).toBeUndefined();

          // Wait for synchronization
          await waitForOfflineSubkeyWrite(routingContext, dhtRecord.key);

          const getValueRes = await dhtRetry(
            () => routingContext.getDHTValue(dhtRecord.key, 0, true),
            { label: 'getDHTValue forceRefresh' },
          );

          await expect(getValueRes?.data).toBeDefined();
          await expect(textDecoder.decode(getValueRes?.data)).toBe(data);

          await expect(getValueRes?.writer.isEqual((dhtRecord.owner as KeyPair).key)).toEqual(true);
          await expect(getValueRes?.seq).toBe(0);
        });

        it('should open readonly record', async () => {
          await routingContext.closeDHTRecord(dhtRecord.key);

          const readonlyDhtRecord = await dhtRetry(
            () => routingContext.openDHTRecord(dhtRecord.key),
            { label: 'openDHTRecord readonly' },
          );
          await expect(readonlyDhtRecord).toBeDefined();

          // No retry: we're asserting the not-writable rejection.
          const setValueRes = routingContext.setDHTValue(
            dhtRecord.key,
            0,
            textEncoder.encode(data)
          );
          await expect(setValueRes).rejects.toEqual({
            kind: 'Generic',
            message: 'value is not writable',
          });
        });

        it('should open writable record', async () => {
          await routingContext.closeDHTRecord(dhtRecord.key);

          const writeableDhtRecord = await dhtRetry(
            () => routingContext.openDHTRecord(dhtRecord.key, dhtRecord.owner as KeyPair),
            { label: 'openDHTRecord writable' },
          );
          await expect(writeableDhtRecord).toBeDefined();
          const setValueRes = await dhtRetry(
            () => routingContext.setDHTValue(dhtRecord.key, 0, textEncoder.encode(`${data}👋`)),
            { label: 'setDHTValue' },
          );
          await expect(setValueRes).toBeUndefined();
        });

        it('should open readonly record and specify writer during the set', async () => {
          await routingContext.closeDHTRecord(dhtRecord.key);

          const writeableDhtRecord = await dhtRetry(
            () => routingContext.openDHTRecord(dhtRecord.key),
            { label: 'openDHTRecord' },
          );
          await expect(writeableDhtRecord).toBeDefined();
          // No retry: we're asserting the not-writable rejection.
          const setValueResFail = routingContext.setDHTValue(
            dhtRecord.key,
            0,
            textEncoder.encode(`${data}👋`),
          );
          await expect(setValueResFail).rejects.toEqual({
            kind: 'Generic',
            message: 'value is not writable',
          });
          const setValueRes = await dhtRetry(
            () => routingContext.setDHTValue(
              dhtRecord.key,
              0,
              textEncoder.encode(`${data}👋`),
              {
                writer: dhtRecord.owner as KeyPair,
                allowOffline: undefined
              }
            ),
            { label: 'setDHTValue with writer' },
          );
          await expect(setValueRes).toBeUndefined();
        });

        it('should watch value and cancel watch', async () => {
          const setValueRes = await dhtRetry(
            () => routingContext.setDHTValue(dhtRecord.key, 0, textEncoder.encode(data)),
            { label: 'setDHTValue' },
          );
          await expect(setValueRes).toBeUndefined();

          // With typical values
          const watchValueRes = await dhtRetry(
            () => routingContext.watchDhtValues(dhtRecord.key, [[0, 0]], "0", 0xFFFFFFFF),
            { label: 'watchDhtValues' },
          );
          await expect(watchValueRes).toEqual(true);

          const cancelValueRes = await dhtRetry(
            () => routingContext.cancelDHTWatch(dhtRecord.key, []),
            { label: 'cancelDHTWatch' },
          );

          await expect(cancelValueRes).toEqual(false);

        });

        it('should watch value and cancel watch with default values', async () => {
          const setValueRes = await dhtRetry(
            () => routingContext.setDHTValue(dhtRecord.key, 0, textEncoder.encode(data)),
            { label: 'setDHTValue' },
          );
          await expect(setValueRes).toBeUndefined();

          // Again with default values
          const watchValueRes = await dhtRetry(
            () => routingContext.watchDhtValues(dhtRecord.key),
            { label: 'watchDhtValues default' },
          );
          await expect(watchValueRes).toEqual(true);

          const cancelValueRes = await dhtRetry(
            () => routingContext.cancelDHTWatch(dhtRecord.key),
            { label: 'cancelDHTWatch default' },
          );
          await expect(cancelValueRes).toEqual(false);
        });

        it('should set a value and inspect it', async () => {
          const setValueRes = await dhtRetry(
            () => routingContext.setDHTValue(dhtRecord.key, 0, textEncoder.encode(data)),
            { label: 'setDHTValue' },
          );
          await expect(setValueRes).toBeUndefined();

          // Inspect locally
          const inspectRes = await dhtRetry(
            () => routingContext.inspectDHTRecord(dhtRecord.key, [[0, 0]], "Local"),
            { label: 'inspectDHTRecord Local' },
          );
          await expect(inspectRes).toBeDefined();
          await expect(inspectRes.subkeys).toEqual([[0, 0]]);
          await expect(inspectRes.localSeqs).toEqual([0]);
          await expect(inspectRes.networkSeqs).toEqual([undefined]);

          // Wait for synchronization
          await waitForOfflineSubkeyWrite(routingContext, dhtRecord.key);

          // Inspect network
          const inspectRes2 = await dhtRetry(
            () => routingContext.inspectDHTRecord(dhtRecord.key, [[0, 0]], "SyncGet"),
            { label: 'inspectDHTRecord SyncGet' },
          );
          await expect(inspectRes2).toBeDefined();
          await expect(inspectRes2.subkeys).toEqual([[0, 0]]);
          await expect(inspectRes2.offlineSubkeys).toEqual([]);
          await expect(inspectRes2.localSeqs).toEqual([0]);
          await expect(inspectRes2.networkSeqs).toEqual([0]);
        });

        it('should set a value and inspect it with defaults', async () => {
          const setValueRes = await dhtRetry(
            () => routingContext.setDHTValue(dhtRecord.key, 0, textEncoder.encode(data)),
            { label: 'setDHTValue' },
          );
          await expect(setValueRes).toBeUndefined();

          // Wait for synchronization
          await waitForOfflineSubkeyWrite(routingContext, dhtRecord.key);

          // Inspect locally
          const inspectRes = await dhtRetry(
            () => routingContext.inspectDHTRecord(dhtRecord.key),
            { label: 'inspectDHTRecord default' },
          );
          await expect(inspectRes).toBeDefined();
          await expect(inspectRes.offlineSubkeys).toEqual([]);
          await expect(inspectRes.localSeqs).toEqual([0]);
          await expect(inspectRes.networkSeqs).toEqual([undefined]);
        });

        it('should flush a record and confirm no offline subkeys remain', async () => {
          const setValueRes = await routingContext.setDHTValue(
            dhtRecord.key,
            0,
            textEncoder.encode(data)
          );
          await expect(setValueRes).toBeUndefined();

          // flush with no timeout should wait until writes drain and return true
          const flushRes = await routingContext.flushDHTRecord(dhtRecord.key);
          await expect(flushRes).toBe(true);

          // no pending writes — tight timeout should still return true immediately
          const flushResWithTimeout = await routingContext.flushDHTRecord(dhtRecord.key, 10);
          await expect(flushResWithTimeout).toBe(true);
        });
      });

      describe(`DHT transactions simple tests for ${cryptoKind as string}`, () => {

        const DHT_RECORD_COUNT = 2
        const DHT_SUBKEY_SIZE = 16
        const DHT_SUBKEY_COUNT = 2
        const data: Uint8Array[] = [];
        for (let rec = 0; rec < DHT_RECORD_COUNT; rec++) {
          data.push(new Uint8Array(DHT_SUBKEY_SIZE).fill(rec));
        }

        let dhtRecords: RecordKey[];

        beforeEach('create dht records', async () => {
          dhtRecords = [];
          for (let rec = 0; rec < DHT_RECORD_COUNT; rec++) {
            const dhtRecord = await routingContext.createDHTRecord(
              cryptoKind,
              { 'DFLT': { oCnt: DHT_SUBKEY_COUNT } },
            );
            await expect(dhtRecord.key).toBeInstanceOf(RecordKey);
            await expect(dhtRecord.owner).toBeInstanceOf(KeyPair);
            await expect(dhtRecord.schema).toEqual({ 'DFLT': { oCnt: DHT_SUBKEY_COUNT } });
            dhtRecords.push(dhtRecord.key);
          }
        });

        afterEach('free dht records', async () => {
          for (let rec = 0; rec < DHT_RECORD_COUNT; rec++) {
            await routingContext.deleteDHTRecord(dhtRecords[rec]);
          }
          dhtRecords = [];
        });

        it('should create empty transaction and drop it explicitly', async () => {
          const tx = await dhtRetry(() => veilidClient.transactDHTRecords(dhtRecords), { label: 'begin' });
          await expect(tx).toBeInstanceOf(VeilidDHTTransaction);
          tx.free();
        });

        it('should create empty transaction and drop it lazily', async () => {
          const tx = await dhtRetry(() => veilidClient.transactDHTRecords(dhtRecords), { label: 'begin' });
          await expect(tx).toBeInstanceOf(VeilidDHTTransaction);
        });

        it('should create empty transaction and rollback', async () => {
          const tx = await dhtRetry(() => veilidClient.transactDHTRecords(dhtRecords), { label: 'begin' });
          await expect(tx).toBeInstanceOf(VeilidDHTTransaction);
          await dhtRetry(() => tx.rollback(), { label: 'rollback' });
        });

        it('should create empty transaction and commit', async () => {
          const tx = await dhtRetry(() => veilidClient.transactDHTRecords(dhtRecords), { label: 'begin' });
          await expect(tx).toBeInstanceOf(VeilidDHTTransaction);
          await dhtRetry(() => tx.commit(), { label: 'commit' });
        });

        it('should create transaction, add sets, and rollback', async () => {
          const tx = await dhtRetry(() => veilidClient.transactDHTRecords(dhtRecords), { label: 'begin' });
          await expect(tx).toBeInstanceOf(VeilidDHTTransaction);

          for (let rec = 0; rec < DHT_RECORD_COUNT; rec++) {
            const res = await dhtRetry(() => tx.set(dhtRecords[rec], 0, data[rec]), { label: 'set' });
            await expect(res).toBeUndefined();
          }

          await dhtRetry(() => tx.rollback(), { label: 'rollback' });
        });

        it('should create transaction, add sets, and commit', async () => {
          const tx = await dhtRetry(() => veilidClient.transactDHTRecords(dhtRecords), { label: 'begin' });
          await expect(tx).toBeInstanceOf(VeilidDHTTransaction);

          for (let rec = 0; rec < DHT_RECORD_COUNT; rec++) {
            const res = await dhtRetry(() => tx.set(dhtRecords[rec], 0, data[rec]), { label: 'set' });
            await expect(res).toBeUndefined();
          }

          await dhtRetry(() => tx.commit(), { label: 'commit' });
        });

        it('should extend transaction with a new record and commit', async () => {
          const extraDesc = await routingContext.createDHTRecord(
            cryptoKind,
            { 'DFLT': { oCnt: DHT_SUBKEY_COUNT } },
          );
          try {
            const tx = await dhtRetry(() => veilidClient.transactDHTRecords(dhtRecords), { label: 'begin' });
            await expect(tx).toBeInstanceOf(VeilidDHTTransaction);

            for (let rec = 0; rec < DHT_RECORD_COUNT; rec++) {
              const res = await dhtRetry(() => tx.set(dhtRecords[rec], 0, data[rec]), { label: 'set' });
              await expect(res).toBeUndefined();
            }

            await dhtRetry(() => tx.extend([extraDesc.key]), { label: 'extend' });

            const extraData = new Uint8Array(DHT_SUBKEY_SIZE).fill(DHT_RECORD_COUNT);
            const extraRes = await dhtRetry(() => tx.set(extraDesc.key, 0, extraData), { label: 'set extra' });
            await expect(extraRes).toBeUndefined();

            await dhtRetry(() => tx.commit(), { label: 'commit' });

            const readback = await dhtRetry(() => routingContext.getDHTValue(extraDesc.key, 0, false), { label: 'getDHTValue' });
            await expect(readback).toBeDefined();
            await expect(readback!.data).toEqual(extraData);
          } finally {
            await routingContext.deleteDHTRecord(extraDesc.key);
          }
        });

        it('should extend transaction with a new record and rollback', async () => {
          const extraDesc = await routingContext.createDHTRecord(
            cryptoKind,
            { 'DFLT': { oCnt: DHT_SUBKEY_COUNT } },
          );
          try {
            const tx = await dhtRetry(() => veilidClient.transactDHTRecords(dhtRecords), { label: 'begin' });
            await expect(tx).toBeInstanceOf(VeilidDHTTransaction);

            await dhtRetry(() => tx.set(dhtRecords[0], 0, data[0]), { label: 'set' });
            await dhtRetry(() => tx.extend([extraDesc.key]), { label: 'extend' });
            const extraData = new Uint8Array(DHT_SUBKEY_SIZE).fill(DHT_RECORD_COUNT);
            await dhtRetry(() => tx.set(extraDesc.key, 0, extraData), { label: 'set extra' });

            await dhtRetry(() => tx.rollback(), { label: 'rollback' });

            const readback = await dhtRetry(() => routingContext.getDHTValue(extraDesc.key, 0, true), { label: 'getDHTValue' });
            await expect(readback).toBeUndefined();
          } finally {
            await routingContext.deleteDHTRecord(extraDesc.key);
          }
        });

        it('should create transaction, add sets, gets, and commit', async () => {

          const tx = await dhtRetry(() => veilidClient.transactDHTRecords(dhtRecords), { label: 'begin' });
          await expect(tx).toBeInstanceOf(VeilidDHTTransaction);

          const sets = [];
          for (let rec = 0; rec < DHT_RECORD_COUNT; rec++) {
            sets.push(dhtRetry(() => tx.set(dhtRecords[rec], 0, data[rec]), { label: `set rec${rec}` }));
          }

          const allSetRes = await Promise.all(sets)
          for (const res of allSetRes) {
            await expect(res).toBeUndefined();
          }

          const gets = [];
          for (let rec = 0; rec < DHT_RECORD_COUNT; rec++) {
            gets.push(dhtRetry(() => tx.get(dhtRecords[rec], 0), { label: `get rec${rec}` }));
          }

          const allGetRes = await Promise.all(gets)
          for (const res of allGetRes) {
            await expect(res).toBeUndefined();
          }

          await dhtRetry(() => tx.commit(), { label: 'commit' });
        });

        it('should create empty transaction, fail non-transactional sets and then rollback', async () => {
          // Non-transactional sets against records held in a transaction must
          // surface TryAgain — don't retry; that's the assertion.
          const tx = await dhtRetry(() => veilidClient.transactDHTRecords(dhtRecords), { label: 'begin' });
          await expect(tx).toBeInstanceOf(VeilidDHTTransaction);

          const sets: Promise<ValueData | undefined>[] = [];
          for (let subkey = 0; subkey < DHT_SUBKEY_COUNT; subkey++) {
            for (let rec = 0; rec < DHT_RECORD_COUNT; rec++) {
              sets.push(routingContext.setDHTValue(dhtRecords[rec], subkey, data[rec]));
            }
          }

          try {
            await Promise.all(sets);
          } catch (error) {
            await expect(error).toMatchObject({ kind: "TryAgain" });
          }
          await dhtRetry(() => tx.rollback(), { label: 'rollback' });

        });

        it('should create transaction, inspect, add sets to subkey 1, inspect, and commit. Then a new transaction inspect, and gets, and commit', async () => {
          // Begin
          const tx = await dhtRetry(() => veilidClient.transactDHTRecords(dhtRecords), { label: 'begin' });
          await expect(tx).toBeInstanceOf(VeilidDHTTransaction);

          // Inspect 1
          const inspects1: Promise<{ rec: number, report: DHTRecordReport }>[] = [];
          for (let rec = 0; rec < DHT_RECORD_COUNT; rec++) {
            inspects1.push((async () => {
              const report = await dhtRetry(() => tx.inspect(dhtRecords[rec], null, "SyncGet"), { label: `inspect rec${rec}` });
              return { rec: rec, report: report };
            })());
          }
          const allInspects1Res = await Promise.all(inspects1)
          for (const res of allInspects1Res) {
            //console.log("res1", res);
            await expect(res.report.subkeys).toEqual([[0, 1]]);
            await expect(res.report.localSeqs).toEqual([undefined, undefined]);
            await expect(res.report.networkSeqs).toEqual([undefined, undefined]);
            await expect(res.report.offlineSubkeys).toEqual([]);
          }

          // Sets
          const sets: Promise<ValueData | undefined>[] = [];
          for (let rec = 0; rec < DHT_RECORD_COUNT; rec++) {
            sets.push(dhtRetry(() => tx.set(dhtRecords[rec], 1, data[rec]), { label: `set rec${rec}` }));
          }

          const allSetRes = await Promise.all(sets)
          for (const res of allSetRes) {
            await expect(res).toBeUndefined();
          }

          // Inspect 2 (should be the same because we haven't committed yet)
          const inspects2: Promise<{ rec: number, report: DHTRecordReport }>[] = [];
          for (let rec = 0; rec < DHT_RECORD_COUNT; rec++) {
            inspects2.push((async () => {
              const report = await dhtRetry(() => tx.inspect(dhtRecords[rec], null, "SyncGet"), { label: `inspect2 rec${rec}` });
              return { rec: rec, report: report };
            })());
          }
          const allInspects2Res = await Promise.all(inspects2)
          for (const res of allInspects2Res) {
            //console.log("res2", res);
            await expect(res.report.subkeys).toEqual([[0, 1]]);
            await expect(res.report.localSeqs).toEqual([undefined, undefined]);
            await expect(res.report.networkSeqs).toEqual([undefined, undefined]);
            await expect(res.report.offlineSubkeys).toEqual([]);
          }

          // Commit
          await dhtRetry(() => tx.commit(), { label: 'commit' });

          // Transaction 2
          const tx2 = await dhtRetry(() => veilidClient.transactDHTRecords(dhtRecords), { label: 'begin tx2' });
          await expect(tx2).toBeInstanceOf(VeilidDHTTransaction);

          // Inspect 3 (should be updated post-commit)
          const inspects3: Promise<{ rec: number, report: DHTRecordReport }>[] = [];
          for (let rec = 0; rec < DHT_RECORD_COUNT; rec++) {
            inspects3.push((async () => {
              const report = await dhtRetry(() => tx2.inspect(dhtRecords[rec], null, "SyncGet"), { label: `inspect3 rec${rec}` });
              return { rec: rec, report: report };
            })());
          }
          const allInspects3Res = await Promise.all(inspects3)
          for (const res of allInspects3Res) {
            //console.log("res3", res);
            await expect(res.report.subkeys).toEqual([[0, 1]]);
            await expect(res.report.localSeqs).toEqual([undefined, 0]);
            await expect(res.report.networkSeqs).toEqual([undefined, 0]);
            await expect(res.report.offlineSubkeys).toEqual([]);
          }

          // Gets should match inspect
          const expected = [undefined, 0];
          for (let subkey = 0; subkey < DHT_SUBKEY_COUNT; subkey++) {
            const gets: Promise<{ rec: number, subkey: number, val: ValueData | undefined }>[] = [];
            for (let rec = 0; rec < DHT_RECORD_COUNT; rec++) {
              gets.push((async () => {
                const val = await dhtRetry(() => tx2.get(dhtRecords[rec], subkey), { label: `get rec${rec} sk${subkey}` });
                return { rec: rec, subkey: subkey, val: val };
              })());
            }

            const allGetRes = await Promise.all(gets)
            for (const res of allGetRes) {
              if (expected[res.subkey] == null) {
                await expect(res.val).toBeUndefined();
              } else {
                await expect(res.val).toBeDefined();
                await expect(res.val!.data).toEqual(data[res.rec]);
                await expect(res.val!.seq).toEqual(expected[res.subkey]);
              }
            }
          }
          await dhtRetry(() => tx2.commit(), { label: 'commit tx2' });
        });
      });


      if (STRESS) {
        describe(`DHT transactions full records tests for ${cryptoKind as string}`, () => {
          const DHT_RECORD_COUNT = 8
          const DHT_SUBKEY_SIZE = 32768
          const DHT_SUBKEY_COUNT = 32
          const data: Uint8Array[] = [];
          for (let rec = 0; rec < DHT_RECORD_COUNT; rec++) {
            data.push(new Uint8Array(DHT_SUBKEY_SIZE).fill(rec));
          }

          let dhtRecords: RecordKey[];

          beforeEach('create dht records', async () => {
            dhtRecords = [];
            for (let rec = 0; rec < DHT_RECORD_COUNT; rec++) {
              const dhtRecord = await routingContext.createDHTRecord(
                cryptoKind,
                { 'DFLT': { oCnt: DHT_SUBKEY_COUNT } },
              );
              await expect(dhtRecord.key).toBeInstanceOf(RecordKey);
              await expect(dhtRecord.owner).toBeInstanceOf(KeyPair);
              await expect(dhtRecord.schema).toEqual({ 'DFLT': { oCnt: DHT_SUBKEY_COUNT } });
              dhtRecords.push(dhtRecord.key);
            }
          });

          afterEach('free dht records', async () => {
            for (let rec = 0; rec < DHT_RECORD_COUNT; rec++) {
              await routingContext.deleteDHTRecord(dhtRecords[rec]);
            }
            dhtRecords = [];
          });


          it('should create empty transaction, inspect, add gets and commit', async () => {
            const tx = await dhtRetry(() => veilidClient.transactDHTRecords(dhtRecords), { label: 'begin' });
            await expect(tx).toBeInstanceOf(VeilidDHTTransaction);

            // Inspect 1
            const inspects1: Promise<{ rec: number, report: DHTRecordReport }>[] = [];
            for (let rec = 0; rec < DHT_RECORD_COUNT; rec++) {
              inspects1.push((async () => {
                const report = await dhtRetry(() => tx.inspect(dhtRecords[rec], null, "SyncGet"), { label: `inspect rec${rec}` });
                return { rec: rec, report: report };
              })());
            }
            const allInspects1Res = await Promise.all(inspects1)
            const expectedSeqs = Array(DHT_SUBKEY_COUNT).fill(undefined);
            for (const res of allInspects1Res) {
              await expect(res.report.subkeys).toEqual([[0, DHT_SUBKEY_COUNT - 1]]);
              await expect(res.report.localSeqs).toEqual(expectedSeqs);
              await expect(res.report.networkSeqs).toEqual(expectedSeqs);
              await expect(res.report.offlineSubkeys).toEqual([]);
            }

            for (let subkey = 0; subkey < DHT_SUBKEY_COUNT; subkey++) {
              const gets: Promise<ValueData | undefined>[] = [];
              for (let rec = 0; rec < DHT_RECORD_COUNT; rec++) {
                gets.push(dhtRetry(() => tx.get(dhtRecords[rec], subkey), { label: `get rec${rec} sk${subkey}` }));
              }
              const allGetRes = await Promise.all(gets)
              for (const res of allGetRes) {
                await expect(res).toBeUndefined();
              }
            }

            await dhtRetry(() => tx.commit(), { label: 'commit' });
          });


          it('should create transaction, fill all records, commit, and then get all records and rollback', async () => {
            const startBegin = performance.now();
            const tx = await dhtRetry(() => veilidClient.transactDHTRecords(dhtRecords), { label: 'begin' });
            await expect(tx).toBeInstanceOf(VeilidDHTTransaction);
            console.log(`begin transaction: ${performance.now() - startBegin}ms`)

            // Sets
            for (let subkey = 0; subkey < DHT_SUBKEY_COUNT; subkey++) {
              const startSet = performance.now();

              const sets: Promise<ValueData | undefined>[] = [];
              for (let rec = 0; rec < DHT_RECORD_COUNT; rec++) {
                sets.push(dhtRetry(() => tx.set(dhtRecords[rec], subkey, data[rec]), { label: `set rec${rec} sk${subkey}` }));
              }

              const allSetRes = await Promise.all(sets)
              for (const res of allSetRes) {
                await expect(res).toBeUndefined();
              }

              console.log(`set subkey ${subkey}: ${performance.now() - startSet}ms`)
            }

            const startCommit = performance.now();
            await dhtRetry(() => tx.commit(), { label: 'commit' });
            console.log(`commit transaction: ${performance.now() - startCommit}ms`)

            const startBegin2 = performance.now();
            const tx2 = await dhtRetry(() => veilidClient.transactDHTRecords(dhtRecords), { label: 'begin tx2' });
            await expect(tx2).toBeInstanceOf(VeilidDHTTransaction);
            console.log(`begin transaction 2: ${performance.now() - startBegin2}ms`)

            // Gets
            for (let subkey = 0; subkey < DHT_SUBKEY_COUNT; subkey++) {
              const startGet = performance.now();

              const gets: Promise<{ rec: number, subkey: number, val: ValueData | undefined }>[] = [];
              for (let rec = 0; rec < DHT_RECORD_COUNT; rec++) {
                gets.push((async () => {
                  const val = await dhtRetry(() => tx2.get(dhtRecords[rec], subkey), { label: `get rec${rec} sk${subkey}` });
                  return { rec: rec, subkey: subkey, val: val };
                })());
              }

              const allGetRes = await Promise.all(gets)
              for (const res of allGetRes) {
                await expect(res.val).toBeDefined();
                await expect(res.val!.data).toEqual(data[res.rec]);
                await expect(res.val!.seq).toEqual(0);
              }

              console.log(`get subkey ${subkey}: ${performance.now() - startGet}ms`)
            }

            const startRollback = performance.now();
            await dhtRetry(() => tx2.rollback(), { label: 'rollback tx2' });
            console.log(`rollback transaction 2: ${performance.now() - startRollback}ms`)
          });
        });

        describe(`DHT transactions bulk store tests for ${cryptoKind as string}`, () => {
          // veilid-core limits: MAX_SUBKEY_SIZE and MAX_RECORD_DATA_SIZE
          const SUBKEY_MAX_SIZE_BYTES = 32768;
          const DHTKEY_MAX_SIZE_BYTES = 1048576;

          const chunkBytes = (bytes: Uint8Array, chunkSize: number): Uint8Array[] => {
            const chunks: Uint8Array[] = [];
            for (let offset = 0; offset < bytes.length; offset += chunkSize) {
              chunks.push(bytes.subarray(offset, Math.min(offset + chunkSize, bytes.length)));
            }
            return chunks;
          };

          // Store a blob across however many records/subkeys it needs, in one transaction.
          const storeBytesIntoKeys = async (bytes: Uint8Array): Promise<RecordKey[]> => {
            const dataChunks = chunkBytes(bytes, DHTKEY_MAX_SIZE_BYTES);
            const subKeysPerChunk = dataChunks.map((data) => chunkBytes(data, SUBKEY_MAX_SIZE_BYTES));
            console.log(`split ${bytes.length} bytes across ${dataChunks.length} DHT records`);

            const keys = await Promise.all(
              subKeysPerChunk.map(async (subKeys) => {
                const record = await routingContext.createDHTRecord(
                  cryptoKind,
                  { 'DFLT': { oCnt: subKeys.length } },
                );
                return record.key;
              }),
            );

            const startBegin = performance.now();
            const tx = await dhtRetry(() => veilidClient.transactDHTRecords(keys), { label: 'begin bulk store' });
            console.log(`begin transaction: ${performance.now() - startBegin}ms`);

            // Every subkey of every record in parallel; op concurrency bounds what's in flight
            const startSet = performance.now();
            const sets: Promise<ValueData | undefined>[] = [];
            for (let chunkIndex = 0; chunkIndex < subKeysPerChunk.length; chunkIndex++) {
              const subKeys = subKeysPerChunk[chunkIndex];
              for (let subKeyIdx = 0; subKeyIdx < subKeys.length; subKeyIdx++) {
                sets.push(dhtRetry(
                  () => tx.set(keys[chunkIndex], subKeyIdx, subKeys[subKeyIdx]),
                  { label: `set chunk${chunkIndex} sk${subKeyIdx}` },
                ));
              }
            }
            const setResults = await Promise.all(sets);
            const setMs = performance.now() - startSet;
            console.log(`set ${sets.length} subkeys: ${setMs}ms (${(bytes.length / 1024 / (setMs / 1000)).toFixed(0)} KB/s)`);
            for (const res of setResults) {
              await expect(res).toBeUndefined();
            }

            const startCommit = performance.now();
            await dhtRetry(() => tx.commit(), { label: 'commit bulk store' });
            console.log(`commit transaction: ${performance.now() - startCommit}ms`);
            console.log(`completed transaction: ${performance.now() - startBegin}ms`);

            return keys;
          };

          for (const sizeMB of [1, 2, 4]) {
            it(`should bulk store ${sizeMB}MB across records in one transaction`, async () => {
              const totalBytes = sizeMB * 1024 * 1024;
              // each subkey filled with its global ordinal for content verification
              const bytes = new Uint8Array(totalBytes);
              for (let i = 0; i < totalBytes; i += SUBKEY_MAX_SIZE_BYTES) {
                bytes.fill((i / SUBKEY_MAX_SIZE_BYTES) & 0xff, i, Math.min(i + SUBKEY_MAX_SIZE_BYTES, totalBytes));
              }

              const keys = await storeBytesIntoKeys(bytes);
              await expect(keys.length).toEqual(Math.ceil(totalBytes / DHTKEY_MAX_SIZE_BYTES));

              try {
                // verify the network accepted every subkey, and spot-check content
                const tx = await dhtRetry(() => veilidClient.transactDHTRecords(keys), { label: 'begin bulk verify' });
                for (let rec = 0; rec < keys.length; rec++) {
                  const report = await dhtRetry(() => tx.inspect(keys[rec], null, 'SyncGet'), { label: `inspect rec${rec}` });
                  const subkeyCount = report.networkSeqs.length;
                  await expect(report.subkeys).toEqual([[0, subkeyCount - 1]]);
                  await expect(report.networkSeqs).toEqual(Array(subkeyCount).fill(0));

                  const lastSubkey = subkeyCount - 1;
                  const val = await dhtRetry(() => tx.get(keys[rec], lastSubkey), { label: `get rec${rec} sk${lastSubkey}` });
                  await expect(val).toBeDefined();
                  const globalOrdinal = (rec * (DHTKEY_MAX_SIZE_BYTES / SUBKEY_MAX_SIZE_BYTES) + lastSubkey) & 0xff;
                  await expect(val!.data[0]).toEqual(globalOrdinal);
                  await expect(val!.data.length).toEqual(SUBKEY_MAX_SIZE_BYTES);
                }
                await dhtRetry(() => tx.rollback(), { label: 'rollback bulk verify' });
              } finally {
                for (const key of keys) {
                  await routingContext.deleteDHTRecord(key);
                }
              }
            });
          }

          // One transaction per record, all run concurrently. Same total work as the
          // all-in-one-transaction tests above, but many transactions on unrelated
          // records — stresses the global operation gate, which must bound total
          // in-flight bytes regardless of transaction count.
          const PER_TX_RECORD_BYTES = 8 * SUBKEY_MAX_SIZE_BYTES;

          const storeBytesOneRecordPerTx = async (bytes: Uint8Array): Promise<RecordKey[]> => {
            const recordChunks = chunkBytes(bytes, PER_TX_RECORD_BYTES);
            const subKeysPerRecord = recordChunks.map((data) => chunkBytes(data, SUBKEY_MAX_SIZE_BYTES));
            console.log(`split ${bytes.length} bytes across ${recordChunks.length} records, one transaction each`);

            const keys = await Promise.all(
              subKeysPerRecord.map(async (subKeys) => {
                const record = await routingContext.createDHTRecord(
                  cryptoKind,
                  { 'DFLT': { oCnt: subKeys.length } },
                );
                return record.key;
              }),
            );

            const startAll = performance.now();
            await Promise.all(
              keys.map(async (key, recIdx) => {
                const subKeys = subKeysPerRecord[recIdx];
                const tx = await dhtRetry(() => veilidClient.transactDHTRecords([key]), { label: `begin rec${recIdx}` });
                const sets = subKeys.map((sk, subKeyIdx) =>
                  dhtRetry(() => tx.set(key, subKeyIdx, sk), { label: `set rec${recIdx} sk${subKeyIdx}` }));
                for (const res of await Promise.all(sets)) {
                  await expect(res).toBeUndefined();
                }
                await dhtRetry(() => tx.commit(), { label: `commit rec${recIdx}` });
              }),
            );
            const allMs = performance.now() - startAll;
            console.log(`stored ${keys.length} records in ${keys.length} concurrent transactions: ${allMs}ms (${(bytes.length / 1024 / (allMs / 1000)).toFixed(0)} KB/s)`);

            return keys;
          };

          for (const sizeMB of [1, 2, 4]) {
            it(`should bulk store ${sizeMB}MB one record per transaction (concurrent)`, async () => {
              const totalBytes = sizeMB * 1024 * 1024;
              const bytes = new Uint8Array(totalBytes);
              for (let i = 0; i < totalBytes; i += SUBKEY_MAX_SIZE_BYTES) {
                bytes.fill((i / SUBKEY_MAX_SIZE_BYTES) & 0xff, i, Math.min(i + SUBKEY_MAX_SIZE_BYTES, totalBytes));
              }

              const keys = await storeBytesOneRecordPerTx(bytes);
              await expect(keys.length).toEqual(Math.ceil(totalBytes / PER_TX_RECORD_BYTES));

              try {
                // verify each record's subkeys landed (one verify transaction per record)
                for (let rec = 0; rec < keys.length; rec++) {
                  const tx = await dhtRetry(() => veilidClient.transactDHTRecords([keys[rec]]), { label: `begin verify rec${rec}` });
                  const report = await dhtRetry(() => tx.inspect(keys[rec], null, 'SyncGet'), { label: `inspect rec${rec}` });
                  const subkeyCount = report.networkSeqs.length;
                  await expect(report.networkSeqs).toEqual(Array(subkeyCount).fill(0));
                  await dhtRetry(() => tx.rollback(), { label: `rollback verify rec${rec}` });
                }
              } finally {
                for (const key of keys) {
                  await routingContext.deleteDHTRecord(key);
                }
              }
            });
          }
        });
      }
    }
  });
});
