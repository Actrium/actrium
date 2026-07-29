// Metro 配置 - 基于 @react-native/metro-config
const fs = require('fs');
const path = require('path');
const { getDefaultConfig, mergeConfig } = require('@react-native/metro-config');

const projectRoot = __dirname;
const workspacePackagesRoot = path.resolve(projectRoot, '..');
const workspaceRoot = path.resolve(workspacePackagesRoot, '..');
const projectNodeModules = path.resolve(projectRoot, 'node_modules');
const workspaceNodeModules = path.resolve(workspaceRoot, 'node_modules');
const supramarkRoot = path.resolve(workspacePackagesRoot, 'supramark');
const yarnLinkRoot = path.resolve(process.env.HOME || '', '.config', 'yarn', 'link');
const existingDirs = (dirs) => Array.from(new Set(dirs)).filter((dir) => fs.existsSync(dir));
const supramarkWebEngineShim = path.resolve(projectRoot, 'src', 'actr-host', 'shims', 'supramark-web-engine.js');

// 将 @supramark/* 包直接解析到 Supramark monorepo 的 TypeScript 源码。
// 这样 Supramark 可以保持 `"main": "src/index.ts"`（workspace 风格），
// 无需为了 Actro app 的消费而把每个 package.json 改成指向 dist/。
const supramarkSourceMap = {
  '@supramark/core': path.resolve(supramarkRoot, 'packages/core/src/index.rn.ts'),
  '@supramark/rn': path.resolve(supramarkRoot, 'packages/renderers/rn/src/index.ts'),
  '@supramark/engines': path.resolve(supramarkRoot, 'packages/engines/src/index.ts'),
  '@supramark/feature-admonition': path.resolve(supramarkRoot, 'packages/features/containers/admonition/src/index.ts'),
  '@supramark/feature-core-markdown': path.resolve(supramarkRoot, 'packages/features/core-markdown/src/index.ts'),
  '@supramark/feature-definition-list': path.resolve(supramarkRoot, 'packages/features/main/definition-list/src/index.ts'),
  '@supramark/feature-diagram-dot': path.resolve(supramarkRoot, 'packages/features/diagrams/dot/src/index.ts'),
  '@supramark/feature-diagram-echarts': path.resolve(supramarkRoot, 'packages/features/diagrams/echarts/src/index.ts'),
  '@supramark/feature-diagram-vega-lite': path.resolve(supramarkRoot, 'packages/features/diagrams/vega-lite/src/index.ts'),
  '@supramark/feature-emoji': path.resolve(supramarkRoot, 'packages/features/main/emoji/src/index.ts'),
  '@supramark/feature-footnote': path.resolve(supramarkRoot, 'packages/features/main/footnote/src/index.ts'),
  '@supramark/feature-gfm': path.resolve(supramarkRoot, 'packages/features/main/gfm/src/index.ts'),
  '@supramark/feature-math': path.resolve(supramarkRoot, 'packages/features/main/math/src/index.ts'),
  '@supramark/feature-weather': path.resolve(supramarkRoot, 'packages/features/containers/weather/src/index.ts'),
  '@supramark/feature-mermaid': path.resolve(supramarkRoot, 'packages/features/diagrams/mermaid/src/index.ts'),
  '@supramark/feature-plantuml': path.resolve(supramarkRoot, 'packages/features/diagrams/plantuml/src/index.ts'),
  '@supramark/markdown-web': path.resolve(supramarkRoot, 'crates/supramark-markdown/packages/web/src/index.ts'),
  '@actrium/graphviz-anywhere-rn': path.resolve(supramarkRoot, 'crates/graphviz-anywhere/packages/react-native/src/index.ts'),
  '@actrium/graphviz-anywhere-web': supramarkWebEngineShim,
  '@actrium/d2-little-web': supramarkWebEngineShim,
  '@actrium/mermaid-little-web': supramarkWebEngineShim,
  '@actrium/plantuml-little-web': supramarkWebEngineShim,
};

// 本地开发包（通过 file: 协议或 yarn link 链接）
const localPackages = {
  rill: path.resolve(workspacePackagesRoot, 'rill'),
  askit: path.resolve(workspacePackagesRoot, 'askit'),
  supramark: supramarkRoot,
  yarnLink: yarnLinkRoot,
  workspaceNodeModules,
};
const asyncRequireModulePath = require.resolve('metro-runtime/src/modules/asyncRequire.js', {
  paths: [projectRoot],
});

const defaultConfig = getDefaultConfig(projectRoot);

if (defaultConfig.watcher) {
  delete defaultConfig.watcher.unstable_workerThreads;
}

// 让 Metro 识别 macos 平台
defaultConfig.resolver.platforms = Array.from(
  new Set([...(defaultConfig.resolver.platforms || []), 'macos']),
);

// 屏蔽本地链接包中的 react/react-native，防止出现多实例
// React 19 的 interop 层会尝试相对于 React 包加载 inline-requires-plugin，也需要屏蔽
defaultConfig.resolver.blockList = [
  /.*node_modules\/react\/cjs\/react\.development\.js.*inline-requires-plugin.*/,
  // 本地源码包加入 watchFolders，但它们自己的 node_modules 不参与 bundle。
  // 依赖解析统一走 runtime/root node_modules，避免 Metro 递归监听多个依赖树。
  new RegExp(`${workspacePackagesRoot}/(rill|askit|supramark)/node_modules/.*`),
  // 屏蔽 rill、askit 中的 react / react-reconciler
  new RegExp(`${workspacePackagesRoot}/(rill|askit)/node_modules/react/.*`),
  new RegExp(`${workspacePackagesRoot}/(rill|askit)/node_modules/react-reconciler/.*`),
  // 屏蔽 Supramark 中的 react / react-reconciler / react-native
  new RegExp(`${workspacePackagesRoot}/supramark/node_modules/react/.*`),
  new RegExp(`${workspacePackagesRoot}/supramark/node_modules/react-reconciler/.*`),
  new RegExp(`${workspacePackagesRoot}/supramark/node_modules/react-native/.*`),
  // Supramark 自带的 react-native-webview 会把旧版本源码打进 bundle，必须屏蔽。
  new RegExp(`${workspacePackagesRoot}/supramark/node_modules/react-native-webview/.*`),
];

const defaultPolyfills = defaultConfig.serializer?.getPolyfills?.() || [];
const resolveWithDefault = (context, moduleName, platform) => {
  const defaultResolve = context.resolveRequest;
  if (typeof defaultResolve !== 'function') {
    throw new Error(`Metro default resolver is unavailable while resolving ${moduleName}`);
  }
  return defaultResolve(
    Object.freeze({
      ...context,
      resolveRequest: defaultResolve,
    }),
    moduleName,
    platform,
  );
};

/** @type {import('metro-config').MetroConfig} */
const customConfig = {
  server: {
    port: 8082,
  },
  // 监听本地包目录的文件变化，实现热更新
  watchFolders: existingDirs(Object.values(localPackages)),
  serializer: {
    getPolyfills: () => [
      require.resolve('./src/ensure-console-assert.js'),
      ...defaultPolyfills,
    ],
    getModulesRunBeforeMainModule: () => [
      require.resolve('./src/ensure-console-assert.js'),
    ],
  },
  transformer: {
    asyncRequireModulePath,
  },
  resolver: {
    unstable_enableSymlinks: true,
    nodeModulesPaths: [
      projectNodeModules,
      workspaceNodeModules,
    ],
    resolveRequest: (context, moduleName, platform) => {
      if (moduleName === '@babel/runtime' || moduleName.startsWith('@babel/runtime/')) {
        return {
          type: 'sourceFile',
          filePath: require.resolve(moduleName, { paths: [projectRoot] }),
        };
      }
      // ---- macOS 平台适配 ----
      // macOS 下将 react-native 及其深导入重定向到 react-native-macos
      if (platform === 'macos' && (moduleName === 'react-native' || moduleName.startsWith('react-native/'))) {
        const redirected = moduleName === 'react-native'
          ? 'react-native-macos'
          : 'react-native-macos' + moduleName.slice('react-native'.length);
        return {
          type: 'sourceFile',
          filePath: require.resolve(redirected, { paths: [projectRoot] }),
        };
      }
      // iOS/Android 强制使用本项目的 react-native，避免链接包中的副本
      if ((platform === 'ios' || platform === 'android') && moduleName === 'react-native') {
        return {
          type: 'sourceFile',
          filePath: path.resolve(projectRoot, 'node_modules', 'react-native', 'index.js'),
        };
      }
      // 强制所有入口统一使用 runtime 自己的 react-native-webview，
      // 避免 Metro 从 Supramark/node_modules 解析到未打补丁的副本。
      if (moduleName === 'react-native-webview') {
        return {
          type: 'sourceFile',
          filePath: path.resolve(projectRoot, 'node_modules', 'react-native-webview', 'src', 'index.ts'),
        };
      }
      // macOS 下 safe-area-context 使用 shim
      if (platform === 'macos' && moduleName === 'react-native-safe-area-context') {
        return {
          type: 'sourceFile',
          filePath: path.resolve(projectRoot, 'src', 'actr-host', 'shims', 'react-native-safe-area-context.js'),
        };
      }

      // ---- 本地包入口解析 ----
      // rill: 显式指定 host 入口，避免 package.json exports 歧义
      if (moduleName === 'rill') {
        return {
          type: 'sourceFile',
          filePath: path.resolve(workspacePackagesRoot, 'rill', 'src', 'host', 'index.ts'),
        };
      }
      // askit: 解析到 host 入口
      if (moduleName === 'askit') {
        return {
          type: 'sourceFile',
          filePath: path.resolve(workspacePackagesRoot, 'askit', 'src', 'index.host.ts'),
        };
      }
      if (moduleName === 'askit/core') {
        return {
          type: 'sourceFile',
          filePath: path.resolve(workspacePackagesRoot, 'askit', 'src', 'core', 'index.ts'),
        };
      }
      if (moduleName === 'askit/contracts') {
        return {
          type: 'sourceFile',
          filePath: path.resolve(workspacePackagesRoot, 'askit', 'src', 'contracts', 'index.ts'),
        };
      }

      // ---- supramark 源码直接解析 ----
      // 将 @supramark/* 包直接解析到 TS 源码，无需 dist/ 构建产物
      if (supramarkSourceMap[moduleName]) {
        return { type: 'sourceFile', filePath: supramarkSourceMap[moduleName] };
      }
      if (
        (context.originModulePath.startsWith(path.resolve(supramarkRoot, 'packages', 'core', 'src')) ||
         context.originModulePath.includes('/node_modules/@supramark/core/')) &&
        (moduleName === './plugin-loader.js' || moduleName === './plugin-loader')
      ) {
        return {
          type: 'sourceFile',
          filePath: path.resolve(supramarkRoot, 'packages', 'core', 'src', 'plugin-loader-rn.ts'),
        };
      }

      // ---- 保证全局单一 React 实例 ----
      if (moduleName === 'react') {
        return { type: 'sourceFile', filePath: path.resolve(projectRoot, 'node_modules', 'react', 'index.js') };
      }
      if (moduleName === 'react/jsx-runtime') {
        return { type: 'sourceFile', filePath: path.resolve(projectRoot, 'node_modules', 'react', 'jsx-runtime.js') };
      }
      if (moduleName === 'react/jsx-dev-runtime') {
        return { type: 'sourceFile', filePath: path.resolve(projectRoot, 'node_modules', 'react', 'jsx-dev-runtime.js') };
      }
      if (moduleName === 'react-reconciler') {
        return { type: 'sourceFile', filePath: path.resolve(projectRoot, 'node_modules', 'react-reconciler', 'index.js') };
      }

      // ---- 其他兼容性 shim ----
      // react-native-macos 的 virtualized-lists 依赖修复
      if (moduleName === '@react-native-macos/virtualized-lists') {
        return { type: 'sourceFile', filePath: path.resolve(projectRoot, 'node_modules', '@react-native-macos', 'virtualized-lists', 'index.js') };
      }
      // @viz-js/viz 使用 import.meta，在 RN 运行时会崩溃，用 shim 替代
      if (moduleName === '@viz-js/viz') {
        return {
          type: 'sourceFile',
          filePath: path.resolve(projectRoot, 'src', 'actr-host', 'shims', 'viz-js-viz.js'),
        };
      }
      // tslib 的 ESM/CJS 混合导出兼容
      if (moduleName === 'tslib') {
        return {
          type: 'sourceFile',
          filePath: path.resolve(projectRoot, 'src', 'actr-host', 'shims', 'tslib.js'),
        };
      }

      // ---- Supramark ESM .js → .ts 重写 ----
      // Supramark 源码使用 ESM 风格的 `.js` 后缀导入（如 `from './feature.js'`），
      // 但实际文件是 `.ts`。Metro 无法自动解析，需要在这里拦截并重写。
      if (
        (context.originModulePath.startsWith(supramarkRoot) ||
         context.originModulePath.includes('/node_modules/@supramark/')) &&
        moduleName.endsWith('.js') &&
        (moduleName.startsWith('./') || moduleName.startsWith('../'))
      ) {
        const tsName = moduleName.replace(/\.js$/, '.ts');
        try {
          return resolveWithDefault(context, tsName, platform);
        } catch {}
        const tsxName = moduleName.replace(/\.js$/, '.tsx');
        try {
          return resolveWithDefault(context, tsxName, platform);
        } catch {}
      }

      return resolveWithDefault(context, moduleName, platform);
    },
    // 确保链接包中的 react 相关模块解析到本项目
    extraNodeModules: {
      'react': path.resolve(projectRoot, 'node_modules/react'),
      'react-native': path.resolve(projectRoot, 'node_modules/react-native'),
      // 与 resolveRequest 配合，确保依赖树中的二次解析也回到项目内副本。
      'react-native-webview': path.resolve(projectRoot, 'node_modules/react-native-webview'),
      'react-dom': path.resolve(projectRoot, 'node_modules/react-dom'),
      'react-reconciler': path.resolve(projectRoot, 'node_modules/react-reconciler'),
    },
    unstable_enablePackageExports: true,
    sourceExts: ['ts', 'tsx', 'js', 'jsx', 'json', 'mjs', 'cjs'],
  },
};

module.exports = mergeConfig(defaultConfig, customConfig);
