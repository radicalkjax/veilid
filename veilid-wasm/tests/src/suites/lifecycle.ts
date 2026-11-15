import { veilidClient } from 'veilid-wasm';
import type { ContextName, Suite, TestCase } from '../harness/types.js';
import { waitForMs } from '../utils/wait-utils.js';

const STATE_TIMEOUT_MS = 5_000;
// Long enough for a change that should not happen to have happened
const SETTLE_MS = 250;

// The state is read with a debug command, which the mock and release builds lack
function skipReason(): string | undefined {
  const features = veilidClient.features();
  if (features.includes('mock-api')) {
    return 'the mock node follows no lifecycle';
  }
  if (!features.includes('debug-api')) {
    return 'reading the lifecycle state needs the debug-api feature';
  }
  return undefined;
}

function skipOutsideWindow(context: ContextName): string | undefined {
  return context === 'window' ? skipReason() : 'only a page has a visibility to follow';
}

async function lifecycleState(): Promise<string> {
  const out = await veilidClient.debug('lifecycle');
  return /^\s*state: (\S+)/m.exec(out)?.[1] ?? '';
}

async function waitForLifecycleState(expected: string): Promise<void> {
  const deadline = Date.now() + STATE_TIMEOUT_MS;
  let state = await lifecycleState();
  while (state !== expected) {
    if (Date.now() >= deadline) {
      throw new Error(`lifecycle state stayed ${state}, expected ${expected}`);
    }
    await waitForMs(50);
    state = await lifecycleState();
  }
}

// Stands in for the browser hiding or showing the page, which automation cannot do everywhere
function setPageHidden(hidden: boolean): void {
  if (hidden) {
    Object.defineProperty(document, 'hidden', { configurable: true, get: () => true });
  } else {
    delete (document as { hidden?: boolean }).hidden;
  }
  document.dispatchEvent(new Event('visibilitychange'));
}

export const lifecycleSuite: Suite = {
  name: 'lifecycle',
  requiresNetwork: false,
  cases: (): TestCase[] => [
    // First, before any host report can stand in for detection
    {
      name: 'should follow the context it runs in',
      skipReason,
      run: async ({ expect, context }) => {
        const state = await lifecycleState();
        if (context === 'window') {
          expect(state).toBe(document.hidden ? 'Background' : 'Active');
        } else if (context === 'service' || context === 'extension') {
          // No warning comes before a worker is stopped, so it is in the background throughout
          expect(state).toBe('Background');
        } else {
          expect(state).toBe('-');
        }
      },
    },
    {
      name: 'should follow page visibility',
      skipReason: skipOutsideWindow,
      run: async () => {
        setPageHidden(true);
        await waitForLifecycleState('Background');
        setPageHidden(false);
        await waitForLifecycleState('Active');
      },
    },
    {
      name: 'should keep a departing page in the background until it is shown',
      skipReason: skipOutsideWindow,
      run: async ({ expect }) => {
        window.dispatchEvent(new PageTransitionEvent('pagehide', { persisted: true }));
        await waitForLifecycleState('Background');
        // Visibility says nothing while the page is going into the back/forward cache
        document.dispatchEvent(new Event('visibilitychange'));
        await waitForMs(SETTLE_MS);
        expect(await lifecycleState()).toBe('Background');
        window.dispatchEvent(new PageTransitionEvent('pageshow', { persisted: true }));
        await waitForLifecycleState('Active');
      },
    },
    {
      name: 'should hear freeze and resume only where the browser has them',
      skipReason: skipOutsideWindow,
      run: async ({ expect }) => {
        document.dispatchEvent(new Event('freeze'));
        if ('onfreeze' in document) {
          await waitForLifecycleState('Background');
          document.dispatchEvent(new Event('resume'));
          await waitForLifecycleState('Active');
        } else {
          await waitForMs(SETTLE_MS);
          expect(await lifecycleState()).toBe('Active');
        }
      },
    },
    {
      name: 'should take host reports only where nothing is detected',
      skipReason,
      run: async ({ expect, context }) => {
        if (context === 'dedicated' || context === 'shared') {
          veilidClient.reportAppLifecycleState('Background');
          await waitForLifecycleState('Background');
          veilidClient.reportAppLifecycleState('Active');
          await waitForLifecycleState('Active');
          return;
        }
        const detected = await lifecycleState();
        veilidClient.reportAppLifecycleState(detected === 'Active' ? 'Background' : 'Active');
        await waitForMs(SETTLE_MS);
        expect(await lifecycleState()).toBe(detected);
      },
    },
  ],
};
