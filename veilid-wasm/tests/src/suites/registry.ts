import type { Suite } from '../harness/types.js';
import { clientSuite } from './client.js';
import { cryptoSuite } from './crypto.js';
import { tableSuite } from './table.js';
import { protectedStoreSuite } from './protected-store.js';
import { routingSuite } from './routing.js';
import { dhtSuite } from './dht.js';
import { transactionsSuite } from './transactions.js';
import { lifecycleSuite } from './lifecycle.js';

// Every suite, in the order they run when several are selected.
export const ALL_SUITES: Suite[] = [
  clientSuite,
  cryptoSuite,
  tableSuite,
  protectedStoreSuite,
  routingSuite,
  dhtSuite,
  transactionsSuite,
  lifecycleSuite,
];

export const ALL_SUITE_NAMES = ALL_SUITES.map((s) => s.name);

export function suitesByName(names: string[]): Suite[] {
  return names.map((n) => {
    const suite = ALL_SUITES.find((s) => s.name === n);
    if (!suite) {
      throw new Error(`unknown test suite '${n}', expected one of: ${ALL_SUITE_NAMES.join(', ')}`);
    }
    return suite;
  });
}
