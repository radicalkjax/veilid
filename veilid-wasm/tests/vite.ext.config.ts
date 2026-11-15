import { defineConfig, type PluginOption } from 'vite';
import { copyFileSync } from 'node:fs';
import wasm from 'vite-plugin-wasm';
import topLevelAwait from 'vite-plugin-top-level-await';

// Due to "node16" module resolution in tsconfig.json, both plugins' types resolve to the module
// namespace rather than to the callable default export.
const wasmPlugin = wasm as unknown as () => PluginOption;
const topLevelAwaitPlugin = topLevelAwait as unknown as () => PluginOption;

// Chrome loads an unpacked extension straight from disk, so the background service worker must be
// a self-contained static bundle. Same MV3/service-worker constraints as vite.sw.config.ts.
export default defineConfig({
  plugins: [
    wasmPlugin(),
    topLevelAwaitPlugin(),
    {
      name: 'copy-manifest',
      closeBundle() {
        copyFileSync('extension/manifest.json', 'extensionDist/manifest.json');
      },
    },
  ],
  optimizeDeps: { exclude: ['veilid-wasm'] },
  publicDir: false,
  build: {
    outDir: 'extensionDist',
    emptyOutDir: true,
    target: 'esnext',
    rollupOptions: {
      input: 'extension/background.ts',
      output: {
        format: 'es',
        entryFileNames: 'background.js',
        chunkFileNames: 'assets/[name].js',
        assetFileNames: 'assets/[name][extname]',
      },
    },
  },
});
