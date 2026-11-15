// A static top-level `import 'veilid-wasm'` hangs wasm instantiation in vite-bundled module
// workers, so the suite graph is pulled in on demand instead.
import type { RunRequest } from '../harness/types.js';

self.onmessage = async (e: MessageEvent<RunRequest | undefined>) => {
  if (e.data?.type !== 'run') {
    return;
  }
  const step = (s: string) => self.postMessage({ type: 'progress', step: s });
  try {
    const { runSuites } = await import('../harness/run-suites.js');
    const results = await runSuites(e.data.suites ?? [], 'dedicated', step, e.data.caseFilter);
    self.postMessage({ type: 'done', ok: true, results });
  } catch (err) {
    self.postMessage({ type: 'done', ok: false, error: String(err) });
  }
};
