// Metro 配置 - 基于 @react-native/metro-config
const path = require('path');
const { getDefaultConfig, mergeConfig } = require('@react-native/metro-config');
const { resolve } = require('metro-resolver');

const projectRoot = __dirname;
const goAskAwayRoot = path.resolve(projectRoot, '../..');
const supermarkRoot = path.resolve(goAskAwayRoot, 'supermark');
const yarnLinkRoot = path.resolve(process.env.HOME || '', '.config', 'yarn', 'link');

// 将 @supramark/* 包直接解析到 supermark monorepo 的 TypeScript 源码。
// 这样 supermark 可以保持 `"main": "src/index.ts"`（workspace 风格），
// 无需为了 askaway-ipad 的消费而把每个 package.json 改成指向 dist/。
const supramarkSourceMap = {
  '@supramark/core': path.resolve(supermarkRoot, 'packages/core/src/index.rn.ts'),
  '@supramark/rn': path.resolve(supermarkRoot, 'packages/renderers/rn/src/index.ts'),
  '@supramark/diagram-engine': path.resolve(supermarkRoot, 'packages/renderers/diagram-engine/src/index.ts'),
  '@supramark/web-diagram': path.resolve(supermarkRoot, 'packages/renderers/web-diagram/index.js'),
  '@supramark/feature-admonition': path.resolve(supermarkRoot, 'packages/features/feature-admonition/src/index.ts'),
  '@supramark/feature-core-markdown': path.resolve(supermarkRoot, 'packages/features/feature-core-markdown/src/index.ts'),
  '@supramark/feature-definition-list': path.resolve(supermarkRoot, 'packages/features/feature-definition-list/src/index.ts'),
  '@supramark/feature-diagram-dot': path.resolve(supermarkRoot, 'packages/features/feature-diagram-dot/src/index.ts'),
  '@supramark/feature-diagram-echarts': path.resolve(supermarkRoot, 'packages/features/feature-diagram-echarts/src/index.ts'),
  '@supramark/feature-diagram-mermaid': path.resolve(supermarkRoot, 'packages/features/feature-diagram-mermaid/src/index.ts'),
  '@supramark/feature-diagram-plantuml': path.resolve(supermarkRoot, 'packages/features/feature-diagram-plantuml/src/index.ts'),
  '@supramark/feature-diagram-vega-lite': path.resolve(supermarkRoot, 'packages/features/feature-diagram-vega-lite/src/index.ts'),
  '@supramark/feature-emoji': path.resolve(supermarkRoot, 'packages/features/feature-emoji/src/index.ts'),
  '@supramark/feature-footnote': path.resolve(supermarkRoot, 'packages/features/feature-footnote/src/index.ts'),
  '@supramark/feature-gfm': path.resolve(supermarkRoot, 'packages/features/feature-gfm/src/index.ts'),
  '@supramark/feature-math': path.resolve(supermarkRoot, 'packages/features/feature-math/src/index.ts'),
  '@supramark/feature-weather': path.resolve(supermarkRoot, 'packages/features/feature-weather/src/index.ts'),
};

// 本地开发包（通过 file: 协议或 yarn link 链接）
const localPackages = {
  rill: path.resolve(goAskAwayRoot, 'rill'),
  askit: path.resolve(goAskAwayRoot, 'askit'),
  supermark: supermarkRoot,
  yarnLink: yarnLinkRoot,
};

const defaultConfig = getDefaultConfig(projectRoot);

// 让 Metro 识别 macos 平台
defaultConfig.resolver.platforms = Array.from(
  new Set([...(defaultConfig.resolver.platforms || []), 'macos']),
);

// 屏蔽本地链接包中的 react/react-native，防止出现多实例
// React 19 的 interop 层会尝试相对于 React 包加载 inline-requires-plugin，也需要屏蔽
defaultConfig.resolver.blockList = [
  /.*node_modules\/react\/cjs\/react\.development\.js.*inline-requires-plugin.*/,
  // 屏蔽 rill、askit 中的 react / react-reconciler
  new RegExp(`${goAskAwayRoot}/(rill|askit)/node_modules/react/.*`),
  new RegExp(`${goAskAwayRoot}/(rill|askit)/node_modules/react-reconciler/.*`),
  // 屏蔽 supermark 中的 react / react-reconciler / react-native
  new RegExp(`${goAskAwayRoot}/supermark/node_modules/react/.*`),
  new RegExp(`${goAskAwayRoot}/supermark/node_modules/react-reconciler/.*`),
  new RegExp(`${goAskAwayRoot}/supermark/node_modules/react-native/.*`),
  // supermark 自带的 react-native-webview 会把旧版本源码打进 bundle，必须屏蔽。
  new RegExp(`${goAskAwayRoot}/supermark/node_modules/react-native-webview/.*`),
];

const defaultPolyfills = defaultConfig.serializer?.getPolyfills?.() || [];

/** @type {import('metro-config').MetroConfig} */
const customConfig = {
  server: {
    port: 8082,
  },
  // 监听本地包目录的文件变化，实现热更新
  watchFolders: Object.values(localPackages),
  serializer: {
    getPolyfills: () => [
      require.resolve('./src/ensure-console-assert.js'),
      ...defaultPolyfills,
    ],
    getModulesRunBeforeMainModule: () => [
      require.resolve('./src/ensure-console-assert.js'),
    ],
  },
  resolver: {
    unstable_enableSymlinks: true,
    nodeModulesPaths: [
      path.resolve(projectRoot, 'node_modules'),
    ],
    resolveRequest: (context, moduleName, platform) => {
      // ---- macOS 平台适配 ----
      // macOS 下将 react-native 重定向到 react-native-macos
      if (platform === 'macos' && moduleName === 'react-native') {
        return resolve(context, 'react-native-macos', platform);
      }
      // iOS/Android 强制使用本项目的 react-native，避免链接包中的副本
      if ((platform === 'ios' || platform === 'android') && moduleName === 'react-native') {
        return {
          type: 'sourceFile',
          filePath: path.resolve(projectRoot, 'node_modules', 'react-native', 'index.js'),
        };
      }
      // 强制所有入口统一使用 Subsystem 自己的 react-native-webview，
      // 避免 Metro 从 supermark/node_modules 解析到未打补丁的副本。
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
          filePath: path.resolve(goAskAwayRoot, 'rill', 'src', 'host', 'index.ts'),
        };
      }
      // askit: 解析到 host 入口
      if (moduleName === 'askit') {
        return {
          type: 'sourceFile',
          filePath: path.resolve(goAskAwayRoot, 'askit', 'src', 'index.host.ts'),
        };
      }
      if (moduleName === 'askit/core') {
        return {
          type: 'sourceFile',
          filePath: path.resolve(goAskAwayRoot, 'askit', 'src', 'core', 'index.ts'),
        };
      }
      if (moduleName === 'askit/contracts') {
        return {
          type: 'sourceFile',
          filePath: path.resolve(goAskAwayRoot, 'askit', 'src', 'contracts', 'index.ts'),
        };
      }

      // ---- supramark 源码直接解析 ----
      // 将 @supramark/* 包直接解析到 TS 源码，无需 dist/ 构建产物
      if (supramarkSourceMap[moduleName]) {
        return { type: 'sourceFile', filePath: supramarkSourceMap[moduleName] };
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

      // ---- supermark ESM .js → .ts 重写 ----
      // supermark 源码使用 ESM 风格的 `.js` 后缀导入（如 `from './feature.js'`），
      // 但实际文件是 `.ts`。Metro 无法自动解析，需要在这里拦截并重写。
      if (
        context.originModulePath.startsWith(supermarkRoot) &&
        moduleName.endsWith('.js') &&
        (moduleName.startsWith('./') || moduleName.startsWith('../'))
      ) {
        const tsName = moduleName.replace(/\.js$/, '.ts');
        try {
          return resolve(context, tsName, platform);
        } catch {}
        const tsxName = moduleName.replace(/\.js$/, '.tsx');
        try {
          return resolve(context, tsxName, platform);
        } catch {}
      }

      return resolve(context, moduleName, platform);
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
