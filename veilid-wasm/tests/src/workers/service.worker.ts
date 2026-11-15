// Service workers forbid dynamic `import()`, so the suite graph is imported statically (allowed
// in module service workers). vite-plugin-top-level-await rewrites the wasm module's top-level
// await, which service workers also reject.
//
// Consequence: this module body only runs once wasm init resolves, so the message listener
// attaches late. The worker announces itself to its clients rather than leaving the page to
// guess when it is safe to post.
import { runSuites } from '../harness/run-suites.js';
import type { RunRequest } from '../harness/types.js';

// The tsconfig targets DOM, not WebWorker, so the service worker globals aren't declared; this is
// the slice of ServiceWorkerGlobalScope the worker uses.
type ExtendableEventLike = { waitUntil: (p: Promise<unknown>) => void };
type ExtendableMessageEventLike = ExtendableEventLike & {
  data?: RunRequest;
  ports: MessagePort[];
};
type ClientLike = { postMessage: (message: unknown) => void };
type ServiceWorkerScope = {
  addEventListener(type: 'install', listener: () => Promise<void>): void;
  addEventListener(type: 'activate', listener: (e: ExtendableEventLike) => void): void;
  addEventListener(type: 'message', listener: (e: ExtendableMessageEventLike) => void): void;
  skipWaiting: () => Promise<void>;
  clients: {
    claim: () => Promise<void>;
    matchAll: (options?: { includeUncontrolled?: boolean }) => Promise<ClientLike[]>;
  };
};
const sw = self as unknown as ServiceWorkerScope;

sw.addEventListener('install', () => sw.skipWaiting());
sw.addEventListener('activate', (e: ExtendableEventLike) => e.waitUntil(sw.clients.claim()));

sw.addEventListener('message', (event: ExtendableMessageEventLike) => {
  const port = event.ports[0];
  if (event.data?.type === 'ping') {
    port?.postMessage({ type: 'pong' });
    return;
  }
  if (event.data?.type !== 'run') {
    return;
  }
  const step = (s: string) => port?.postMessage({ type: 'progress', step: s });
  event.waitUntil(
    (async () => {
      try {
        const results = await runSuites(
          event.data?.suites ?? [],
          'service',
          step,
          event.data?.caseFilter,
        );
        port?.postMessage({ type: 'done', ok: true, results });
      } catch (err) {
        port?.postMessage({ type: 'done', ok: false, error: String(err) });
      }
    })(),
  );
});

// Listeners are attached; tell any client that is waiting for us.
void (async () => {
  const clients = await sw.clients.matchAll({ includeUncontrolled: true });
  for (const client of clients) {
    client.postMessage({ type: 'sw-ready' });
  }
})();
