// A static top-level `import 'veilid-wasm'` hangs wasm instantiation in vite-bundled module
// workers, so the suite graph is pulled in on demand instead.
import type { RunRequest } from '../harness/types.js';

// The tsconfig targets DOM, not WebWorker, so SharedWorkerGlobalScope isn't declared.
type SharedWorkerScope = { onconnect: (e: MessageEvent) => void };

(self as unknown as SharedWorkerScope).onconnect = (e: MessageEvent) => {
  const port = e.ports[0];
  const step = (s: string) => port.postMessage({ type: 'progress', step: s });

  port.onmessage = async (ev: MessageEvent<RunRequest | undefined>) => {
    if (ev.data?.type !== 'run') {
      return;
    }
    try {
      const { runSuites } = await import('../harness/run-suites.js');
      const results = await runSuites(ev.data.suites ?? [], 'shared', step, ev.data.caseFilter);
      port.postMessage({ type: 'done', ok: true, results });
    } catch (err) {
      port.postMessage({ type: 'done', ok: false, error: String(err) });
    }
  };
  port.start();
};
