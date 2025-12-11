import { defineConfig } from 'vite';

export default defineConfig({
  build: {
    lib: {
      entry: 'src/guest.tsx',
      name: '__RillGuest',
      formats: ['iife'],
      fileName: () => 'bundle',
    },
    outDir: 'dist',
    emptyOutDir: false,
    sourcemap: false,
    minify: 'esbuild',
    target: 'es2020',
    rollupOptions: {
      external: ['react','react/jsx-runtime','react-native','rill/reconciler'],
      output: {
        format: 'iife',
        name: '__RillGuest',
        globals: {
          react: 'React',
          'react/jsx-runtime': 'ReactJSXRuntime',
          'react-native': 'ReactNative',
          'rill/reconciler': 'RillReconciler',
        },
      },
    },
  },
  resolve: {
    alias: {
      'rill/sdk': require.resolve('rill/sdk').replace('.js', '.mjs'),
    },
  },
  define: {
    'process.env.NODE_ENV': '"production"',
    __DEV__: 'false',
  },
  esbuild: {
    jsx: 'automatic',
  },
});
