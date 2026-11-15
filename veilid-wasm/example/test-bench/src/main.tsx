import { StrictMode } from 'react';
import { createRoot } from 'react-dom/client';
import App from './App.tsx';
import { initVeilid } from './veilid/veilid-core.ts';

async function main() {
  // Only the wasm load happens here; App drives startup so the page can report on it
  await initVeilid();

  createRoot(document.getElementById('root')!).render(
    <StrictMode>
      <App />
    </StrictMode>,
  );
}

main().catch((e) => {
  console.error(e);
});
