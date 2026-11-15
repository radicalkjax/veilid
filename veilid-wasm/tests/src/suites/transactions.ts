import {
  DHTRecordReport,
  KeyPair,
  RecordKey,
  ValueData,
  VeilidDHTTransaction,
  VeilidRoutingContext,
  veilidClient,
  veilidCrypto,
} from 'veilid-wasm';
import { STRESS } from '../utils/veilid-config.js';
import { dhtRetry } from '../utils/dht-retry.js';
import type { CaseContext, Suite, TestCase } from '../harness/types.js';

const DHT_RECORD_COUNT = 2;
const DHT_SUBKEY_SIZE = 16;
const DHT_SUBKEY_COUNT = 2;

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

const fillOrdinals = (totalBytes: number): Uint8Array => {
  const bytes = new Uint8Array(totalBytes);
  for (let i = 0; i < totalBytes; i += SUBKEY_MAX_SIZE_BYTES) {
    bytes.fill(
      (i / SUBKEY_MAX_SIZE_BYTES) & 0xff,
      i,
      Math.min(i + SUBKEY_MAX_SIZE_BYTES, totalBytes),
    );
  }
  return bytes;
};

export const transactionsSuite: Suite = {
  name: 'transactions',
  requiresNetwork: true,
  cases: (): TestCase[] => {
    const cases: TestCase[] = [];

    for (const cryptoKind of veilidCrypto.VALID_CRYPTO_KINDS) {
      const data: Uint8Array[] = [];
      for (let rec = 0; rec < DHT_RECORD_COUNT; rec++) {
        data.push(new Uint8Array(DHT_SUBKEY_SIZE).fill(rec));
      }

      // Stands in for the create/delete records hooks the describe block used.
      const withRecords =
        (
          count: number,
          subkeyCount: number,
          body: (c: CaseContext, rc: VeilidRoutingContext, records: RecordKey[]) => Promise<void>,
        ) =>
        async (c: CaseContext) => {
          const routingContext = VeilidRoutingContext.create();
          const records: RecordKey[] = [];
          for (let rec = 0; rec < count; rec++) {
            const dhtRecord = await routingContext.createDHTRecord(cryptoKind, {
              DFLT: { oCnt: subkeyCount },
            });
            c.expect(dhtRecord.key).toBeInstanceOf(RecordKey);
            c.expect(dhtRecord.owner).toBeInstanceOf(KeyPair);
            c.expect(dhtRecord.schema).toEqual({ DFLT: { oCnt: subkeyCount } });
            records.push(dhtRecord.key);
          }
          try {
            await body(c, routingContext, records);
          } finally {
            for (const key of records) {
              await routingContext.deleteDHTRecord(key);
            }
          }
        };

      const simple = (
        name: string,
        body: (c: CaseContext, rc: VeilidRoutingContext, records: RecordKey[]) => Promise<void>,
      ) =>
        cases.push({
          name: `${name} for ${cryptoKind}`,
          run: withRecords(DHT_RECORD_COUNT, DHT_SUBKEY_COUNT, body),
        });

      simple(
        'should create empty transaction and drop it explicitly',
        async ({ expect }, _rc, records) => {
          const tx = await dhtRetry(() => veilidClient.transactDHTRecords(records), {
            label: 'begin',
          });
          expect(tx).toBeInstanceOf(VeilidDHTTransaction);
          tx.free();
        },
      );

      simple(
        'should create empty transaction and drop it lazily',
        async ({ expect }, _rc, records) => {
          const tx = await dhtRetry(() => veilidClient.transactDHTRecords(records), {
            label: 'begin',
          });
          expect(tx).toBeInstanceOf(VeilidDHTTransaction);
        },
      );

      simple('should create empty transaction and rollback', async ({ expect }, _rc, records) => {
        const tx = await dhtRetry(() => veilidClient.transactDHTRecords(records), {
          label: 'begin',
        });
        expect(tx).toBeInstanceOf(VeilidDHTTransaction);
        await dhtRetry(() => tx.rollback(), { label: 'rollback' });
      });

      simple('should create empty transaction and commit', async ({ expect }, _rc, records) => {
        const tx = await dhtRetry(() => veilidClient.transactDHTRecords(records), {
          label: 'begin',
        });
        expect(tx).toBeInstanceOf(VeilidDHTTransaction);
        await dhtRetry(() => tx.commit(), { label: 'commit' });
      });

      simple(
        'should create transaction, add sets, and rollback',
        async ({ expect }, _rc, records) => {
          const tx = await dhtRetry(() => veilidClient.transactDHTRecords(records), {
            label: 'begin',
          });
          expect(tx).toBeInstanceOf(VeilidDHTTransaction);

          for (let rec = 0; rec < DHT_RECORD_COUNT; rec++) {
            const res = await dhtRetry(() => tx.set(records[rec], 0, data[rec]), {
              label: 'set',
            });
            expect(res).toBeUndefined();
          }
          await dhtRetry(() => tx.rollback(), { label: 'rollback' });
        },
      );

      simple(
        'should create transaction, add sets, and commit',
        async ({ expect }, _rc, records) => {
          const tx = await dhtRetry(() => veilidClient.transactDHTRecords(records), {
            label: 'begin',
          });
          expect(tx).toBeInstanceOf(VeilidDHTTransaction);

          for (let rec = 0; rec < DHT_RECORD_COUNT; rec++) {
            const res = await dhtRetry(() => tx.set(records[rec], 0, data[rec]), {
              label: 'set',
            });
            expect(res).toBeUndefined();
          }
          await dhtRetry(() => tx.commit(), { label: 'commit' });
        },
      );

      simple(
        'should extend transaction with a new record and commit',
        async ({ expect }, rc, records) => {
          const extraDesc = await rc.createDHTRecord(cryptoKind, {
            DFLT: { oCnt: DHT_SUBKEY_COUNT },
          });
          try {
            const tx = await dhtRetry(() => veilidClient.transactDHTRecords(records), {
              label: 'begin',
            });
            expect(tx).toBeInstanceOf(VeilidDHTTransaction);

            for (let rec = 0; rec < DHT_RECORD_COUNT; rec++) {
              const res = await dhtRetry(() => tx.set(records[rec], 0, data[rec]), {
                label: 'set',
              });
              expect(res).toBeUndefined();
            }

            await dhtRetry(() => tx.extend([extraDesc.key]), { label: 'extend' });

            const extraData = new Uint8Array(DHT_SUBKEY_SIZE).fill(DHT_RECORD_COUNT);
            const extraRes = await dhtRetry(() => tx.set(extraDesc.key, 0, extraData), {
              label: 'set extra',
            });
            expect(extraRes).toBeUndefined();

            await dhtRetry(() => tx.commit(), { label: 'commit' });

            const readback = await dhtRetry(() => rc.getDHTValue(extraDesc.key, 0, false), {
              label: 'getDHTValue',
            });
            expect(readback).toBeDefined();
            expect(readback!.data).toEqual(extraData);
          } finally {
            await rc.deleteDHTRecord(extraDesc.key);
          }
        },
      );

      simple(
        'should extend transaction with a new record and rollback',
        async ({ expect }, rc, records) => {
          const extraDesc = await rc.createDHTRecord(cryptoKind, {
            DFLT: { oCnt: DHT_SUBKEY_COUNT },
          });
          try {
            const tx = await dhtRetry(() => veilidClient.transactDHTRecords(records), {
              label: 'begin',
            });
            expect(tx).toBeInstanceOf(VeilidDHTTransaction);

            await dhtRetry(() => tx.set(records[0], 0, data[0]), { label: 'set' });
            await dhtRetry(() => tx.extend([extraDesc.key]), { label: 'extend' });
            const extraData = new Uint8Array(DHT_SUBKEY_SIZE).fill(DHT_RECORD_COUNT);
            await dhtRetry(() => tx.set(extraDesc.key, 0, extraData), { label: 'set extra' });

            await dhtRetry(() => tx.rollback(), { label: 'rollback' });

            const readback = await dhtRetry(() => rc.getDHTValue(extraDesc.key, 0, true), {
              label: 'getDHTValue',
            });
            expect(readback).toBeUndefined();
          } finally {
            await rc.deleteDHTRecord(extraDesc.key);
          }
        },
      );

      simple(
        'should create transaction, add sets, gets, and commit',
        async ({ expect }, _rc, records) => {
          const tx = await dhtRetry(() => veilidClient.transactDHTRecords(records), {
            label: 'begin',
          });
          expect(tx).toBeInstanceOf(VeilidDHTTransaction);

          const sets = [];
          for (let rec = 0; rec < DHT_RECORD_COUNT; rec++) {
            sets.push(
              dhtRetry(() => tx.set(records[rec], 0, data[rec]), { label: `set rec${rec}` }),
            );
          }
          for (const res of await Promise.all(sets)) {
            expect(res).toBeUndefined();
          }

          const gets = [];
          for (let rec = 0; rec < DHT_RECORD_COUNT; rec++) {
            gets.push(dhtRetry(() => tx.get(records[rec], 0), { label: `get rec${rec}` }));
          }
          for (const res of await Promise.all(gets)) {
            expect(res).toBeUndefined();
          }

          await dhtRetry(() => tx.commit(), { label: 'commit' });
        },
      );

      simple(
        'should create empty transaction, fail non-transactional sets and then rollback',
        async ({ expect }, rc, records) => {
          // Non-transactional sets against records held in a transaction must surface
          // TryAgain, don't retry; that's the assertion.
          const tx = await dhtRetry(() => veilidClient.transactDHTRecords(records), {
            label: 'begin',
          });
          expect(tx).toBeInstanceOf(VeilidDHTTransaction);

          const sets: Promise<ValueData | undefined>[] = [];
          for (let subkey = 0; subkey < DHT_SUBKEY_COUNT; subkey++) {
            for (let rec = 0; rec < DHT_RECORD_COUNT; rec++) {
              sets.push(rc.setDHTValue(records[rec], subkey, data[rec]));
            }
          }

          try {
            await Promise.all(sets);
          } catch (error) {
            expect(error).toMatchObject({ kind: 'TryAgain' });
          }
          await dhtRetry(() => tx.rollback(), { label: 'rollback' });
        },
      );

      simple(
        'should create transaction, inspect, add sets to subkey 1, inspect, and commit. Then a new transaction inspect, and gets, and commit',
        async ({ expect }, _rc, records) => {
          const tx = await dhtRetry(() => veilidClient.transactDHTRecords(records), {
            label: 'begin',
          });
          expect(tx).toBeInstanceOf(VeilidDHTTransaction);

          const inspectAll = async (
            t: VeilidDHTTransaction,
            label: string,
          ): Promise<{ rec: number; report: DHTRecordReport }[]> =>
            Promise.all(
              records.map(async (key, rec) => ({
                rec,
                report: await dhtRetry(() => t.inspect(key, null, 'SyncGet'), {
                  label: `${label} rec${rec}`,
                }),
              })),
            );

          for (const res of await inspectAll(tx, 'inspect')) {
            expect(res.report.subkeys).toEqual([[0, 1]]);
            expect(res.report.localSeqs).toEqual([undefined, undefined]);
            expect(res.report.networkSeqs).toEqual([undefined, undefined]);
            expect(res.report.offlineSubkeys).toEqual([]);
          }

          const sets: Promise<ValueData | undefined>[] = records.map((key, rec) =>
            dhtRetry(() => tx.set(key, 1, data[rec]), { label: `set rec${rec}` }),
          );
          for (const res of await Promise.all(sets)) {
            expect(res).toBeUndefined();
          }

          // Unchanged, because we haven't committed yet
          for (const res of await inspectAll(tx, 'inspect2')) {
            expect(res.report.subkeys).toEqual([[0, 1]]);
            expect(res.report.localSeqs).toEqual([undefined, undefined]);
            expect(res.report.networkSeqs).toEqual([undefined, undefined]);
            expect(res.report.offlineSubkeys).toEqual([]);
          }

          await dhtRetry(() => tx.commit(), { label: 'commit' });

          const tx2 = await dhtRetry(() => veilidClient.transactDHTRecords(records), {
            label: 'begin tx2',
          });
          expect(tx2).toBeInstanceOf(VeilidDHTTransaction);

          for (const res of await inspectAll(tx2, 'inspect3')) {
            expect(res.report.subkeys).toEqual([[0, 1]]);
            expect(res.report.localSeqs).toEqual([undefined, 0]);
            expect(res.report.networkSeqs).toEqual([undefined, 0]);
            expect(res.report.offlineSubkeys).toEqual([]);
          }

          // Gets should match inspect
          const expected = [undefined, 0];
          for (let subkey = 0; subkey < DHT_SUBKEY_COUNT; subkey++) {
            const gets = records.map(async (key, rec) => ({
              rec,
              subkey,
              val: await dhtRetry(() => tx2.get(key, subkey), {
                label: `get rec${rec} sk${subkey}`,
              }),
            }));
            for (const res of await Promise.all(gets)) {
              if (expected[res.subkey] == null) {
                expect(res.val).toBeUndefined();
              } else {
                expect(res.val).toBeDefined();
                expect(res.val!.data).toEqual(data[res.rec]);
                expect(res.val!.seq).toEqual(expected[res.subkey]);
              }
            }
          }
          await dhtRetry(() => tx2.commit(), { label: 'commit tx2' });
        },
      );

      if (STRESS) {
        const STRESS_RECORD_COUNT = 8;
        const STRESS_SUBKEY_SIZE = 32768;
        const STRESS_SUBKEY_COUNT = 32;
        const stressData: Uint8Array[] = [];
        for (let rec = 0; rec < STRESS_RECORD_COUNT; rec++) {
          stressData.push(new Uint8Array(STRESS_SUBKEY_SIZE).fill(rec));
        }

        cases.push({
          name: `should create empty transaction, inspect, add gets and commit for ${cryptoKind}`,
          run: withRecords(
            STRESS_RECORD_COUNT,
            STRESS_SUBKEY_COUNT,
            async ({ expect }, _rc, records) => {
              const tx = await dhtRetry(() => veilidClient.transactDHTRecords(records), {
                label: 'begin',
              });
              expect(tx).toBeInstanceOf(VeilidDHTTransaction);

              const inspects = records.map(async (key, rec) => ({
                rec,
                report: await dhtRetry(() => tx.inspect(key, null, 'SyncGet'), {
                  label: `inspect rec${rec}`,
                }),
              }));
              const expectedSeqs = Array(STRESS_SUBKEY_COUNT).fill(undefined);
              for (const res of await Promise.all(inspects)) {
                expect(res.report.subkeys).toEqual([[0, STRESS_SUBKEY_COUNT - 1]]);
                expect(res.report.localSeqs).toEqual(expectedSeqs);
                expect(res.report.networkSeqs).toEqual(expectedSeqs);
                expect(res.report.offlineSubkeys).toEqual([]);
              }

              for (let subkey = 0; subkey < STRESS_SUBKEY_COUNT; subkey++) {
                const gets = records.map((key, rec) =>
                  dhtRetry(() => tx.get(key, subkey), {
                    label: `get rec${rec} sk${subkey}`,
                  }),
                );
                for (const res of await Promise.all(gets)) {
                  expect(res).toBeUndefined();
                }
              }

              await dhtRetry(() => tx.commit(), { label: 'commit' });
            },
          ),
        });

        cases.push({
          name: `should create transaction, fill all records, commit, and then get all records and rollback for ${cryptoKind}`,
          run: withRecords(
            STRESS_RECORD_COUNT,
            STRESS_SUBKEY_COUNT,
            async ({ expect }, _rc, records) => {
              const startBegin = performance.now();
              const tx = await dhtRetry(() => veilidClient.transactDHTRecords(records), {
                label: 'begin',
              });
              expect(tx).toBeInstanceOf(VeilidDHTTransaction);
              console.log(`begin transaction: ${performance.now() - startBegin}ms`);

              for (let subkey = 0; subkey < STRESS_SUBKEY_COUNT; subkey++) {
                const startSet = performance.now();
                const sets = records.map((key, rec) =>
                  dhtRetry(() => tx.set(key, subkey, stressData[rec]), {
                    label: `set rec${rec} sk${subkey}`,
                  }),
                );
                for (const res of await Promise.all(sets)) {
                  expect(res).toBeUndefined();
                }
                console.log(`set subkey ${subkey}: ${performance.now() - startSet}ms`);
              }

              const startCommit = performance.now();
              await dhtRetry(() => tx.commit(), { label: 'commit' });
              console.log(`commit transaction: ${performance.now() - startCommit}ms`);

              const startBegin2 = performance.now();
              const tx2 = await dhtRetry(() => veilidClient.transactDHTRecords(records), {
                label: 'begin tx2',
              });
              expect(tx2).toBeInstanceOf(VeilidDHTTransaction);
              console.log(`begin transaction 2: ${performance.now() - startBegin2}ms`);

              for (let subkey = 0; subkey < STRESS_SUBKEY_COUNT; subkey++) {
                const startGet = performance.now();
                const gets = records.map(async (key, rec) => ({
                  rec,
                  val: await dhtRetry(() => tx2.get(key, subkey), {
                    label: `get rec${rec} sk${subkey}`,
                  }),
                }));
                for (const res of await Promise.all(gets)) {
                  expect(res.val).toBeDefined();
                  expect(res.val!.data).toEqual(stressData[res.rec]);
                  expect(res.val!.seq).toEqual(0);
                }
                console.log(`get subkey ${subkey}: ${performance.now() - startGet}ms`);
              }

              const startRollback = performance.now();
              await dhtRetry(() => tx2.rollback(), { label: 'rollback tx2' });
              console.log(`rollback transaction 2: ${performance.now() - startRollback}ms`);
            },
          ),
        });

        // Store a blob across however many records/subkeys it needs, in one transaction.
        const storeBytesIntoKeys = async (
          c: CaseContext,
          rc: VeilidRoutingContext,
          bytes: Uint8Array,
        ): Promise<RecordKey[]> => {
          const dataChunks = chunkBytes(bytes, DHTKEY_MAX_SIZE_BYTES);
          const subKeysPerChunk = dataChunks.map((d) => chunkBytes(d, SUBKEY_MAX_SIZE_BYTES));
          console.log(`split ${bytes.length} bytes across ${dataChunks.length} DHT records`);

          const keys = await Promise.all(
            subKeysPerChunk.map(async (subKeys) => {
              const record = await rc.createDHTRecord(cryptoKind, {
                DFLT: { oCnt: subKeys.length },
              });
              return record.key;
            }),
          );

          const startBegin = performance.now();
          const tx = await dhtRetry(() => veilidClient.transactDHTRecords(keys), {
            label: 'begin bulk store',
          });
          console.log(`begin transaction: ${performance.now() - startBegin}ms`);

          // Every subkey of every record in parallel; op concurrency bounds what's in flight
          const startSet = performance.now();
          const sets: Promise<ValueData | undefined>[] = [];
          for (let chunkIndex = 0; chunkIndex < subKeysPerChunk.length; chunkIndex++) {
            const subKeys = subKeysPerChunk[chunkIndex];
            for (let subKeyIdx = 0; subKeyIdx < subKeys.length; subKeyIdx++) {
              sets.push(
                dhtRetry(() => tx.set(keys[chunkIndex], subKeyIdx, subKeys[subKeyIdx]), {
                  label: `set chunk${chunkIndex} sk${subKeyIdx}`,
                }),
              );
            }
          }
          const setResults = await Promise.all(sets);
          const setMs = performance.now() - startSet;
          console.log(
            `set ${sets.length} subkeys: ${setMs}ms (${(bytes.length / 1024 / (setMs / 1000)).toFixed(0)} KB/s)`,
          );
          for (const res of setResults) {
            c.expect(res).toBeUndefined();
          }

          const startCommit = performance.now();
          await dhtRetry(() => tx.commit(), { label: 'commit bulk store' });
          console.log(`commit transaction: ${performance.now() - startCommit}ms`);
          console.log(`completed transaction: ${performance.now() - startBegin}ms`);

          return keys;
        };

        for (const sizeMB of [1, 2, 4]) {
          cases.push({
            name: `should bulk store ${sizeMB}MB across records in one transaction for ${cryptoKind}`,
            run: async (c) => {
              const rc = VeilidRoutingContext.create();
              const totalBytes = sizeMB * 1024 * 1024;
              const bytes = fillOrdinals(totalBytes);

              const keys = await storeBytesIntoKeys(c, rc, bytes);
              c.expect(keys.length).toEqual(Math.ceil(totalBytes / DHTKEY_MAX_SIZE_BYTES));

              try {
                // verify the network accepted every subkey, and spot-check content
                const tx = await dhtRetry(() => veilidClient.transactDHTRecords(keys), {
                  label: 'begin bulk verify',
                });
                for (let rec = 0; rec < keys.length; rec++) {
                  const report = await dhtRetry(() => tx.inspect(keys[rec], null, 'SyncGet'), {
                    label: `inspect rec${rec}`,
                  });
                  const subkeyCount = report.networkSeqs.length;
                  c.expect(report.subkeys).toEqual([[0, subkeyCount - 1]]);
                  c.expect(report.networkSeqs).toEqual(Array(subkeyCount).fill(0));

                  const lastSubkey = subkeyCount - 1;
                  const val = await dhtRetry(() => tx.get(keys[rec], lastSubkey), {
                    label: `get rec${rec} sk${lastSubkey}`,
                  });
                  c.expect(val).toBeDefined();
                  const globalOrdinal =
                    (rec * (DHTKEY_MAX_SIZE_BYTES / SUBKEY_MAX_SIZE_BYTES) + lastSubkey) & 0xff;
                  c.expect(val!.data[0]).toEqual(globalOrdinal);
                  c.expect(val!.data.length).toEqual(SUBKEY_MAX_SIZE_BYTES);
                }
                await dhtRetry(() => tx.rollback(), { label: 'rollback bulk verify' });
              } finally {
                for (const key of keys) {
                  await rc.deleteDHTRecord(key);
                }
              }
            },
          });
        }

        // One transaction per record, all run concurrently. Same total work as the
        // all-in-one-transaction tests above, but many transactions on unrelated records.
        // Stresses the global operation gate, which must bound total in-flight bytes
        // regardless of transaction count.
        const PER_TX_RECORD_BYTES = 8 * SUBKEY_MAX_SIZE_BYTES;

        const storeBytesOneRecordPerTx = async (
          c: CaseContext,
          rc: VeilidRoutingContext,
          bytes: Uint8Array,
        ): Promise<RecordKey[]> => {
          const recordChunks = chunkBytes(bytes, PER_TX_RECORD_BYTES);
          const subKeysPerRecord = recordChunks.map((d) => chunkBytes(d, SUBKEY_MAX_SIZE_BYTES));
          console.log(
            `split ${bytes.length} bytes across ${recordChunks.length} records, one transaction each`,
          );

          const keys = await Promise.all(
            subKeysPerRecord.map(async (subKeys) => {
              const record = await rc.createDHTRecord(cryptoKind, {
                DFLT: { oCnt: subKeys.length },
              });
              return record.key;
            }),
          );

          const startAll = performance.now();
          await Promise.all(
            keys.map(async (key, recIdx) => {
              const subKeys = subKeysPerRecord[recIdx];
              const tx = await dhtRetry(() => veilidClient.transactDHTRecords([key]), {
                label: `begin rec${recIdx}`,
              });
              const sets = subKeys.map((sk, subKeyIdx) =>
                dhtRetry(() => tx.set(key, subKeyIdx, sk), {
                  label: `set rec${recIdx} sk${subKeyIdx}`,
                }),
              );
              for (const res of await Promise.all(sets)) {
                c.expect(res).toBeUndefined();
              }
              await dhtRetry(() => tx.commit(), { label: `commit rec${recIdx}` });
            }),
          );
          const allMs = performance.now() - startAll;
          console.log(
            `stored ${keys.length} records in ${keys.length} concurrent transactions: ${allMs}ms (${(bytes.length / 1024 / (allMs / 1000)).toFixed(0)} KB/s)`,
          );

          return keys;
        };

        for (const sizeMB of [1, 2, 4]) {
          cases.push({
            name: `should bulk store ${sizeMB}MB one record per transaction (concurrent) for ${cryptoKind}`,
            run: async (c) => {
              const rc = VeilidRoutingContext.create();
              const totalBytes = sizeMB * 1024 * 1024;
              const bytes = fillOrdinals(totalBytes);

              const keys = await storeBytesOneRecordPerTx(c, rc, bytes);
              c.expect(keys.length).toEqual(Math.ceil(totalBytes / PER_TX_RECORD_BYTES));

              try {
                // verify each record's subkeys landed (one verify transaction per record)
                for (let rec = 0; rec < keys.length; rec++) {
                  const tx = await dhtRetry(() => veilidClient.transactDHTRecords([keys[rec]]), {
                    label: `begin verify rec${rec}`,
                  });
                  const report = await dhtRetry(() => tx.inspect(keys[rec], null, 'SyncGet'), {
                    label: `inspect rec${rec}`,
                  });
                  const subkeyCount = report.networkSeqs.length;
                  c.expect(report.networkSeqs).toEqual(Array(subkeyCount).fill(0));
                  await dhtRetry(() => tx.rollback(), {
                    label: `rollback verify rec${rec}`,
                  });
                }
              } finally {
                for (const key of keys) {
                  await rc.deleteDHTRecord(key);
                }
              }
            },
          });
        }
      }
    }

    return cases;
  },
};
