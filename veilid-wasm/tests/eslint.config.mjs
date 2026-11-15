import eslint from '@eslint/js';
import { defineConfig, globalIgnores } from 'eslint/config';
import tseslint from 'typescript-eslint';

export default defineConfig([
  // Build outputs: the prebuilt worker and extension bundles and the coverage report
  globalIgnores(['coverage/', 'extensionDist/', 'public/service-worker.js', 'public/swassets/']),
  eslint.configs.recommended,
  tseslint.configs.recommendedTypeChecked,
  {
    languageOptions: {
      parserOptions: {
        projectService: true,
      },
    },
    rules: {
      '@typescript-eslint/no-floating-promises': 'error',
      // pkg/veilid_wasm.d.ts types each opaque id as `(T & OpaqueNewType<S>) | OpaqueNewType<S>`;
      // the bare brand branch never holds a value, so stringifying one is the inner type
      '@typescript-eslint/no-base-to-string': [
        'error',
        { ignoredTypeNames: ['OpaqueNewType', 'Error', 'RegExp', 'URL', 'URLSearchParams'] },
      ],
      '@typescript-eslint/restrict-template-expressions': [
        'error',
        {
          allow: [
            { name: ['Error', 'URL', 'URLSearchParams'], from: 'lib' },
            { name: 'OpaqueNewType', from: 'package', package: 'veilid-wasm' },
          ],
        },
      ],
    },
  },
]);
