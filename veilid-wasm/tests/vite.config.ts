import { defineConfig, searchForWorkspaceRoot, type PluginOption } from 'vite';
import wasm from 'vite-plugin-wasm';
import { fileURLToPath } from 'node:url';

// The web-target wasm-bindgen package loads its `.wasm` by a URL relative to its own module, and
// `veilid-wasm` is a `file:../pkg` dependency, so the asset resolves outside the test root and
// has to be added to `server.fs.allow` below.
const pkgDir = fileURLToPath(new URL('../pkg', import.meta.url));

// Due to "node16" module resolution in tsconfig.json, `vite-plugin-wasm`'s types resolve to the
// module namespace rather than to its callable default export.
const wasmPlugin = wasm as unknown as () => PluginOption;

export default defineConfig({
  plugins: [wasmPlugin()],
  // Module workers need their own plugin pipeline to resolve the bundler-target wasm import.
  worker: {
    format: 'es',
    plugins: () => [wasmPlugin()],
  },
  // Preserve the relative worker/wasm paths of the file: veilid-wasm dep.
  optimizeDeps: {
    exclude: ['veilid-wasm'],
  },
  server: {
    fs: {
      allow: [searchForWorkspaceRoot(process.cwd()), pkgDir],
    },
  },
});
