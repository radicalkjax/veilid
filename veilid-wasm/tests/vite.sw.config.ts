import { defineConfig, type PluginOption } from 'vite';
import wasm from 'vite-plugin-wasm';
import topLevelAwait from 'vite-plugin-top-level-await';

// Due to "node16" module resolution in tsconfig.json, both plugins' types resolve to the module
// namespace rather than to the callable default export.
const wasmPlugin = wasm as unknown as () => PluginOption;
const topLevelAwaitPlugin = topLevelAwait as unknown as () => PluginOption;

// Service workers reject scripts with top-level await, which is what vite-plugin-wasm emits to
// instantiate the module, and Vite's dev-server module output cannot start in a SW scope at all.
// Bundle the SW entry ahead of time with the TLA transform applied and serve it as a static
// script from the site root, so its scope covers the test page.
export default defineConfig({
  plugins: [wasmPlugin(), topLevelAwaitPlugin()],
  optimizeDeps: { exclude: ['veilid-wasm'] },
  // outDir is public/ itself; don't recursively copy it into itself
  publicDir: false,
  build: {
    outDir: 'public',
    emptyOutDir: false,
    // vite-plugin-wasm emits modern syntax
    target: 'esnext',
    rollupOptions: {
      input: 'src/workers/service.worker.ts',
      output: {
        format: 'es',
        // Deterministic names: outDir is public/ which isn't emptied between builds, so
        // hashed names would pile up.
        entryFileNames: 'service-worker.js',
        chunkFileNames: 'swassets/[name].js',
        assetFileNames: 'swassets/[name][extname]',
      },
    },
  },
});
