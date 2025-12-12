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
      external: ['react','react/jsx-runtime','react/jsx-dev-runtime','react-native','@rill/core','@rill/core/sdk'],
      output: {
        format: 'iife',
        name: '__RillGuest',
        globals: {
          react: 'React',
          'react/jsx-runtime': 'ReactJSXRuntime',
          'react/jsx-dev-runtime': 'ReactJSXDevRuntime',
          'react-native': 'ReactNative',
          '@rill/core': 'RillCore',
          '@rill/core/sdk': 'RillCore',
        },
      },
    },
  },
  resolve: {
    alias: {
      '@rill/core/sdk': require.resolve('@rill/core/sdk'),
      '@rill/core': require.resolve('@rill/core'),
    },
  },
  define: {
    'process.env.NODE_ENV': '"production"',
    __DEV__: 'false',
  },
  esbuild: {
    jsx: 'automatic',
    jsxDev: false,
  },
});
