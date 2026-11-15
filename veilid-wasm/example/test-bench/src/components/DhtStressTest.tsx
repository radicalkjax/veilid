import { useState } from 'react';
import { veilidCrypto, DHTSchema, RecordKey } from 'veilid-wasm';

import { describeVeilidError, getRoutingContext } from '../veilid/veilid-core';

const RECORD_COUNT = 30;
const SUBKEY_COUNT = 32;
const INSPECT_COUNT = 1;
const DATA_SIZE = 32 * 1024;

async function dhtStressTest() {
  const routingContext = getRoutingContext();

  const dataArray = new Uint8Array(DATA_SIZE);
  for (let i = 0; i < DATA_SIZE; i++) {
    dataArray[i] = i % 256;
  }

  const operations: Promise<void>[] = [];
  const recordKeys: RecordKey[] = [];

  try {
    for (let r = 0; r < RECORD_COUNT; r++) {
      const schema: DHTSchema = { DFLT: { oCnt: SUBKEY_COUNT } };

      const dhtRecord = await routingContext.createDHTRecord(veilidCrypto.CRYPTO_KIND_VLD0, schema);
      recordKeys.push(dhtRecord.key);

      // Set all subkeys
      for (let n = 0; n < SUBKEY_COUNT; n++) {
        operations.push(
          (async () => {
            const measureName = `${r}-setDhtValue-${n}`;

            performance.mark(measureName + '-start');
            await routingContext.setDHTValue(dhtRecord.key, n, dataArray);
            performance.measure(measureName, measureName + '-start');
          })(),
        );
      }

      // Inspect all records N times while sets are happening
      for (let n = 0; n < INSPECT_COUNT; n++) {
        operations.push(
          (async () => {
            const measureName = `${r}-inspectDhtRecord-${n}`;

            performance.mark(measureName + '-start');
            await routingContext.inspectDHTRecord(dhtRecord.key, null, 'SyncSet');
            performance.measure(measureName, measureName + '-start');
          })(),
        );
      }
    }

    await Promise.all(operations);
  } finally {
    // Records stay open and on local disk otherwise, so every run would cost more
    for (const recordKey of recordKeys) {
      try {
        await routingContext.closeDHTRecord(recordKey);
        await routingContext.deleteDHTRecord(recordKey);
      } catch (error) {
        console.warn('cleanup failed', describeVeilidError(error));
      }
    }
  }

  return operations.length;
}

export function DhtStressTest() {
  const [isRunning, setIsRunning] = useState(false);
  const [result, setResult] = useState<string>();

  return (
    <>
      <button
        onClick={() => {
          if (isRunning) {
            return;
          }
          setIsRunning(true);
          setResult(undefined);
          const startedAt = performance.now();
          dhtStressTest()
            .then(
              (count) => {
                const seconds = (performance.now() - startedAt) / 1000;
                setResult(`${count} operations in ${seconds.toFixed(1)}s`);
              },
              (error: unknown) => setResult(`Failed: ${describeVeilidError(error)}`),
            )
            .finally(() => {
              setIsRunning(false);
            });
        }}
        disabled={isRunning}
      >
        {isRunning ? 'Running...' : 'Run DHT Stress Test'}
      </button>
      {result && <p>{result}</p>}
    </>
  );
}
