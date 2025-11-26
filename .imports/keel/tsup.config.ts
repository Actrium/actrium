import { defineConfig } from 'tsup';

export default defineConfig([
  // SDK - 零依赖，供插件开发使用
  {
    entry: ['src/sdk/index.ts'],
    outDir: 'dist/sdk',
    format: ['cjs', 'esm'],
    dts: true,
    sourcemap: true,
    clean: true,
    external: ['react'],
    treeshake: true,
  },
  // Runtime - 供宿主 App 使用
  {
    entry: ['src/runtime/index.ts'],
    outDir: 'dist/runtime',
    format: ['cjs', 'esm'],
    dts: true,
    sourcemap: true,
    clean: true,
    external: ['react', 'react-native', 'react-native-quickjs'],
    treeshake: true,
  },
  // CLI - 构建工具
  {
    entry: ['src/cli/index.ts'],
    outDir: 'dist/cli',
    format: ['cjs'],
    dts: true,
    sourcemap: true,
    clean: true,
    banner: {
      js: '#!/usr/bin/env node',
    },
  },
  // Reconciler - 核心渲染器 (内部使用)
  {
    entry: ['src/reconciler/index.ts'],
    outDir: 'dist/reconciler',
    format: ['cjs', 'esm'],
    dts: true,
    sourcemap: true,
    clean: true,
    external: ['react', 'react-reconciler'],
  },
]);
