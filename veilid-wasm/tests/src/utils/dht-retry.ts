// Retry helper for DHT operations that may transiently fail with
// `{ kind: 'TryAgain' }`. Mirrors veilid-python's `dht_retry` and
// veilid-flutter's `processorRetry`: blocks until the node reports
// PublicInternet ready before each attempt, then runs the closure.
//
// Use this around any network-touching DHT call in tests (transactions,
// set/get, extend, commit, rollback). Local-only ops (create, close,
// delete) don't need it.

import { VeilidAPIError } from 'veilid-wasm';
import { waitForPublicAttachment } from './wait-utils.js';

export interface DhtRetryOptions {
  timeout?: number;
  label?: string;
}

export const dhtRetry = async <T>(
  op: () => Promise<T>,
  options: DhtRetryOptions = {},
): Promise<T> => {
  const { timeout, label } = options;
  const deadline = timeout != null ? Date.now() + timeout : null;
  const tag = label ?? 'dhtRetry';

  let attempt = 0;
  while (true) {
    if (deadline != null && Date.now() >= deadline) {
      throw new Error(`${tag} timeout`);
    }

    await waitForPublicAttachment();

    try {
      return await op();
    } catch (error) {
      const apiErr = error as Partial<VeilidAPIError> | undefined;
      if (apiErr?.kind === 'TryAgain') {
        attempt += 1;
        const message = 'message' in apiErr ? apiErr.message : '';
        console.log(`  ${tag} retry #${attempt}: ${message}`);
        continue;
      }
      throw error;
    }
  }
};
