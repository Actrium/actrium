/**
 * Vite Configuration for Rill Library Build
 *
 * Replaces tsup for library bundling
 */

import { defineConfig } from 'vite';
import { resolve } from 'path';
import dts from 'vite-plugin-dts';

export default defineConfig({
  plugins: [
    dts({
      include: ['src/**/*.ts', 'src/**/*.tsx'],
      exclude: ['src/**/*.test.ts', 'src/**/*.test.tsx', 'src/__tests__/**', 'src/__mocks__/**'],
      rollupTypes: true,
      outDir: 'dist',
    }),
  ],
  build: {
    lib: {
      entry: {
        'runtime/index': resolve(__dirname, 'src/runtime/index.ts'),
        'sdk/index': resolve(__dirname, 'src/sdk/index.ts'),
        'reconciler/index': resolve(__dirname, 'src/reconciler/index.ts'),
      },
      formats: ['es', 'cjs'],
      fileName: (format, entryName) => {
        const ext = format === 'es' ? 'mjs' : 'js';
        return `${entryName}.${ext}`;
      },
    },
    rollupOptions: {
      external: [
        'react',
        'react-native',
        'react-native-quickjs',
        'react-reconciler',
        'react-dom',
        'react/jsx-runtime',
        'vite',
        'commander',
        'path',
        'fs',
        'url',
        'oxc-parser',
      ],
      output: {
        preserveModules: false,
      },
    },
    sourcemap: true,
    outDir: 'dist',
    emptyOutDir: true,
    target: 'es2020',
    minify: false,
  },
  resolve: {
    alias: {
      'rill/sdk': resolve(__dirname, 'src/sdk'),
      rill: resolve(__dirname, 'src/runtime'),
    },
  },
});
