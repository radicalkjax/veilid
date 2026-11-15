import init, { veilidClient } from 'veilid-wasm';
import {
  LOG_LEVEL,
  veilidCoreInitConfig,
  veilidCoreStartupConfig,
} from '../utils/veilid-config.js';
import { asyncCallWithTimeout, getLogTimestamp, waitForShutdown } from '../utils/wait-utils.js';
import { appCallEventTarget, appMessageEventTarget } from './app-events.js';
import { expect } from './expect.js';
import {
  caseRequiresNetwork,
  caseSelected,
  contextSupportsNetwork,
  NETWORK_SKIP_NOTICE,
  type CaseResult,
  type ContextName,
  type Suite,
} from './types.js';
import { suitesByName } from '../suites/registry.js';

let wasmReady: Promise<unknown> | undefined;

// The slice of `stats print` the analyzer reads
type TxStats = {
  components?: {
    rpc_processor?: { answers_after_waiter_dropped?: { kind: string; count: number }[] };
    storage_manager?: { transaction_quality?: unknown };
  };
};

// Transaction quality counters are cumulative; one compact line per case lets the stress
// analyzer diff consecutive snapshots. Absent when the build has no `stats` feature.
let txQualityAvailable: boolean | undefined;
async function logTxQuality(label: string): Promise<void> {
  if (txQualityAvailable === false) {
    return;
  }
  // Without the `stats` feature the command answers with a plain "unsupported" line
  let stats: TxStats;
  try {
    stats = JSON.parse(await veilidClient.debug('stats print')) as TxStats;
  } catch {
    txQualityAvailable = false;
    return;
  }
  txQualityAvailable = true;
  const late = stats.components?.rpc_processor?.answers_after_waiter_dropped ?? [];
  const line = {
    case: label,
    tx: stats.components?.storage_manager?.transaction_quality,
    late_answers: late.reduce((sum, k) => sum + Number(k.count), 0),
    late_transact_answers: late
      .filter((k) => k.kind === 'TransactCommandA')
      .reduce((sum, k) => sum + Number(k.count), 0),
  };
  console.log(`===TX_QUALITY=== ${JSON.stringify(line)}`);
}

// VeilidAPIError is a plain object, not an Error, so String() on it yields "[object Object]".
function describeError(e: unknown): string {
  if (e instanceof Error) {
    return `${e.message}\n${e.stack ?? ''}`;
  }
  try {
    return JSON.stringify(e) ?? String(e);
  } catch {
    return String(e);
  }
}

// Start a node, run the named suites in whatever context this is running in, then shut down.
//
// A failing case is recorded, never thrown: results have to survive being marshalled back to the
// page from a worker, where an exception would just be lost.
export async function runSuites(
  suiteNames: string[],
  context: ContextName,
  progress: (step: string) => void = () => {},
  caseFilter?: string,
): Promise<CaseResult[]> {
  await (wasmReady ??= init());
  const suites: Suite[] = suitesByName(suiteNames);
  const results: CaseResult[] = [];
  const plan: { suite: Suite; cases: ReturnType<Suite['cases']> }[] = suites.map((s) => ({
    suite: s,
    cases: s.cases().filter((c) => caseSelected(c, caseFilter)),
  }));

  const networkOk = contextSupportsNetwork(context);

  // Report the combinations this context can't run, rather than silently dropping them.
  if (!networkOk) {
    for (const { suite, cases } of plan) {
      for (const c of cases.filter((c) => caseRequiresNetwork(suite, c))) {
        results.push({
          suite: suite.name,
          name: c.name,
          ok: true,
          skipped: NETWORK_SKIP_NOTICE,
        });
      }
    }
  }

  const runnable = plan
    .map(({ suite, cases }) => ({
      suite,
      cases: cases.filter((c) => networkOk || !caseRequiresNetwork(suite, c)),
    }))
    .filter(({ cases }) => cases.length > 0);

  if (runnable.length === 0) {
    return results;
  }

  progress('initializeCore');
  await veilidClient.initializeCore(veilidCoreInitConfig);
  progress('startupCore');
  await veilidClient.startupCore((update) => {
    if (LOG_LEVEL === 'Off' && update.kind === 'Log') {
      console.log(`${getLogTimestamp()}: ${update.message}`);
    } else if (update.kind === 'AppMessage') {
      appMessageEventTarget.dispatchEvent(new CustomEvent('appMessage', { detail: update }));
    } else if (update.kind === 'AppCall') {
      appCallEventTarget.dispatchEvent(new CustomEvent('appCall', { detail: update }));
    }
  }, veilidCoreStartupConfig());

  const needsNetwork = runnable.some(({ suite, cases }) =>
    cases.some((c) => caseRequiresNetwork(suite, c)),
  );

  try {
    if (needsNetwork) {
      progress('attach');
      await veilidClient.attach();
      await veilidClient.waitForAttachmentCondition(undefined, 60_000);
    }

    for (const { suite, cases } of runnable) {
      for (const c of cases) {
        const skipped = c.skipReason?.(context);
        if (skipped !== undefined) {
          results.push({ suite: suite.name, name: c.name, ok: true, skipped });
          continue;
        }
        progress(`${suite.name}:${c.name}`);
        try {
          await c.run({ expect, context });
          results.push({ suite: suite.name, name: c.name, ok: true });
        } catch (e) {
          results.push({
            suite: suite.name,
            name: c.name,
            ok: false,
            error: describeError(e),
          });
        }
        await logTxQuality(`${suite.name}: ${c.name}`);
      }
    }
    await logTxQuality('TOTAL');
  } finally {
    progress('shutdownCore');
    await veilidClient.shutdownCore();
    await asyncCallWithTimeout(waitForShutdown(), 10_000).catch(() => {});
  }

  return results;
}
