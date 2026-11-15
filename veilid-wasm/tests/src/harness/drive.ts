import { EXTENSION_ID } from '../utils/extension-id.js';
import type { CaseResult, ContextName, RunReply, RunRequest } from './types.js';

// Only the slice of chrome.runtime the driver uses; @types/chrome is not a dependency.
declare const chrome:
  | {
      runtime?: {
        sendMessage: (
          extensionId: string,
          message: unknown,
          respond: (response: unknown) => void,
        ) => void;
        lastError?: { message?: string };
      };
    }
  | undefined;

// What extension/background.ts answers with.
type ExtensionReply = { type?: string; ok?: boolean; error?: string; results?: CaseResult[] };

// Bounds a whole context run (every selected suite), not one case.
const RUN_TIMEOUT_MS = 50 * 60 * 1000;

// Run the selected suites in `context` and bring the results back to the page.
export async function driveContext(
  context: ContextName,
  suites: string[],
  caseFilter?: string,
): Promise<CaseResult[]> {
  switch (context) {
    case 'window': {
      const { runSuites } = await import('./run-suites.js');
      return runSuites(suites, context, (s) => console.log(`[window] ${s}`), caseFilter);
    }
    case 'dedicated':
      return driveWorker(context, suites, caseFilter);
    case 'shared':
      return driveSharedWorker(context, suites, caseFilter);
    case 'service':
      return driveServiceWorker(context, suites, caseFilter);
    case 'extension':
      return driveExtension(context, suites, caseFilter);
  }
}

// Collect progress and the final result off a message port, so a worker that dies mid-run
// reports where it got to instead of just timing out silently.
function collect(
  context: ContextName,
  post: (msg: RunRequest, transfer?: Transferable[]) => void,
  onMessage: (handler: (data: RunReply | undefined) => void) => void,
  onError: (handler: (message: string) => void) => void,
  suites: string[],
  caseFilter?: string,
): Promise<CaseResult[]> {
  return new Promise((resolve, reject) => {
    let lastStep = 'startup';
    const timer = setTimeout(
      () => reject(new Error(`${context} timed out after step: ${lastStep}`)),
      RUN_TIMEOUT_MS,
    );

    onMessage((data) => {
      if (data?.type === 'progress') {
        lastStep = data.step;
        console.log(`[${context}] ${data.step}`);
        return;
      }
      clearTimeout(timer);
      if (data?.type === 'done' && data.ok) {
        resolve(data.results);
        return;
      }
      const error = data?.type === 'done' && !data.ok ? data.error : undefined;
      reject(new Error(error ?? `${context} run failed after ${lastStep}`));
    });

    onError((message) => {
      clearTimeout(timer);
      reject(new Error(`${context} error: ${message} (last step: ${lastStep})`));
    });

    post({ type: 'run', suites, caseFilter });
  });
}

function driveWorker(
  context: ContextName,
  suites: string[],
  caseFilter?: string,
): Promise<CaseResult[]> {
  const worker = new Worker(new URL('../workers/dedicated.worker.ts', import.meta.url), {
    type: 'module',
  });
  return collect(
    context,
    (msg) => worker.postMessage(msg),
    (h) => (worker.onmessage = (e: MessageEvent<RunReply>) => h(e.data)),
    (h) => {
      worker.onerror = (e) => h(e.message);
      worker.onmessageerror = () => h('message deserialization error');
    },
    suites,
    caseFilter,
  ).finally(() => worker.terminate());
}

function driveSharedWorker(
  context: ContextName,
  suites: string[],
  caseFilter?: string,
): Promise<CaseResult[]> {
  const worker = new SharedWorker(new URL('../workers/shared.worker.ts', import.meta.url), {
    type: 'module',
  });
  worker.port.start();
  return collect(
    context,
    (msg) => worker.port.postMessage(msg),
    (h) => (worker.port.onmessage = (e: MessageEvent<RunReply>) => h(e.data)),
    (h) => (worker.onerror = (e) => h(e.message)),
    suites,
    caseFilter,
  );
}

async function driveServiceWorker(
  context: ContextName,
  suites: string[],
  caseFilter?: string,
): Promise<CaseResult[]> {
  // Pre-bundled by `npm run build:sw`; vite's dev-server module output cannot start in a SW.
  const reg = await navigator.serviceWorker.register('/service-worker.js', {
    type: 'module',
  });
  try {
    await navigator.serviceWorker.ready;
    const sw = reg.active ?? reg.waiting ?? reg.installing;
    if (!sw) {
      throw new Error('no service worker instance after registration');
    }
    await waitUntilServiceWorkerResponds(sw);

    const channel = new MessageChannel();
    return await collect(
      context,
      (msg) => sw.postMessage(msg, [channel.port2]),
      (h) => (channel.port1.onmessage = (e: MessageEvent<RunReply>) => h(e.data)),
      () => {},
      suites,
      caseFilter,
    );
  } finally {
    // A busy worker can leave unregister() pending; never block teardown on it.
    await Promise.race([
      reg.unregister().catch(() => undefined),
      new Promise((r) => setTimeout(r, 5000)),
    ]);
  }
}

// The SW attaches its listeners only after its async wasm init, so wait for it to say it is up.
async function waitUntilServiceWorkerResponds(sw: ServiceWorker, timeoutMs = 60000) {
  const deadline = Date.now() + timeoutMs;

  const broadcast = new Promise<void>((resolve) => {
    const onMessage = (e: MessageEvent<{ type?: string } | undefined>) => {
      if (e.data?.type === 'sw-ready') {
        navigator.serviceWorker.removeEventListener('message', onMessage);
        resolve();
      }
    };
    navigator.serviceWorker.addEventListener('message', onMessage);
  });

  const ping = (async () => {
    while (Date.now() < deadline) {
      const answered = await new Promise<boolean>((resolve) => {
        const channel = new MessageChannel();
        const timer = setTimeout(() => resolve(false), 500);
        channel.port1.onmessage = () => {
          clearTimeout(timer);
          resolve(true);
        };
        sw.postMessage({ type: 'ping' }, [channel.port2]);
      });
      if (answered) return;
    }
    throw new Error('service worker never became responsive');
  })();

  await Promise.race([broadcast, ping]);
}

async function driveExtension(
  _context: ContextName,
  suites: string[],
  caseFilter?: string,
): Promise<CaseResult[]> {
  if (typeof chrome === 'undefined' || !chrome.runtime?.sendMessage) {
    throw new Error(
      'chrome.runtime is unavailable to this page; the extension capability in ' +
        'wdio.ext.conf.ts is required for this context',
    );
  }
  const runtime = chrome.runtime;

  const send = (message: unknown, timeoutMs: number): Promise<ExtensionReply> =>
    new Promise((resolve, reject) => {
      const timer = setTimeout(() => reject(new Error('extension message timed out')), timeoutMs);
      runtime.sendMessage(EXTENSION_ID, message, (response) => {
        clearTimeout(timer);
        if (runtime.lastError) {
          reject(new Error(runtime.lastError.message));
          return;
        }
        resolve(response as ExtensionReply);
      });
    });

  // The background worker's listener attaches after its wasm init; until then sendMessage
  // reports no receiving end. Poll until it answers.
  const deadline = Date.now() + 60000;
  let lastError = 'never attempted';
  for (;;) {
    if (Date.now() > deadline) {
      throw new Error(`extension never became responsive: ${lastError}`);
    }
    try {
      const res = await send({ type: 'ping' }, 2000);
      if (res.type === 'pong') break;
      lastError = `unexpected ping reply: ${JSON.stringify(res)}`;
    } catch (e) {
      lastError = String(e);
    }
    await new Promise((r) => setTimeout(r, 500));
  }

  const result = await send({ type: 'run', suites, caseFilter }, RUN_TIMEOUT_MS);
  if (!result.ok) {
    throw new Error(result.error ?? 'extension run failed');
  }
  return result.results ?? [];
}
