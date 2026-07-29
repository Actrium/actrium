/**
 * Metro configuration
 * https://reactnative.dev/docs/metro
 *
 * @type {import('@react-native/metro-config').MetroConfig}
 */
const { getDefaultConfig, mergeConfig } = require('@react-native/metro-config');
const fs = require('fs');
const path = require('path');

const defaultConfig = getDefaultConfig(__dirname);

defaultConfig.resolver.sourceExts = [
  ...defaultConfig.resolver.sourceExts,
  'ts'
];

const workspacePackagesRoot = path.resolve(__dirname, '../../../packages');
const workspaceRoot = path.resolve(__dirname, '../../..');
const workspaceNodeModules = path.resolve(workspaceRoot, 'node_modules');
const supramarkRoot = path.resolve(workspacePackagesRoot, 'supramark');
const runtimeRoot = path.resolve(workspacePackagesRoot, 'actro-runtime');
const askitRoot = path.resolve(workspacePackagesRoot, 'askit');
const rillRoot = path.resolve(workspacePackagesRoot, 'rill');
const runtimeShimsDir = path.resolve(runtimeRoot, 'src', 'actr-host', 'shims');
const supramarkWebEngineShim = path.resolve(runtimeShimsDir, 'supramark-web-engine.js');
const runtimeNodeModules = path.resolve(runtimeRoot, 'node_modules');
const yarnLinkRoot = path.resolve(process.env.HOME || '', '.config', 'yarn', 'link');
const rillNodeModules = path.resolve(rillRoot, 'node_modules');
const askitNodeModules = path.resolve(askitRoot, 'node_modules');
const existingDirs = (dirs) => Array.from(new Set(dirs)).filter((dir) => fs.existsSync(dir));
const resolveFromProject = (moduleName) => require.resolve(moduleName, { paths: [__dirname] });
const packageRoot = (packageName) => path.dirname(resolveFromProject(`${packageName}/package.json`));
const defaultResolve = (context, moduleName, platform) => {
  const resolveRequest = context.resolveRequest;
  if (typeof resolveRequest !== 'function') {
    throw new Error(`Metro default resolver is unavailable while resolving ${moduleName}`);
  }
  return resolveRequest(
    Object.freeze({
      ...context,
      resolveRequest,
    }),
    moduleName,
    platform,
  );
};

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

const config = {
  watchFolders: existingDirs([
    // 中文注释：当前项目内的 actro-runtime 是软链，Metro 需要监听其真实目录。
    runtimeRoot,
    supramarkRoot,
    askitRoot,
    rillRoot,
    yarnLinkRoot,
    workspaceNodeModules,
  ]),
  resolver: {
    unstable_enableSymlinks: true,
    nodeModulesPaths: [
      workspaceNodeModules,
      runtimeNodeModules,
      rillNodeModules,
      askitNodeModules,
      path.resolve(__dirname, 'node_modules'),
    ],
    // 确保 react 和 react-native 使用主项目的版本，避免冲突
    extraNodeModules: {
      'react': packageRoot('react'),
      'react-native': packageRoot('react-native'),
      'react-reconciler': packageRoot('react-reconciler'),
      'scheduler': packageRoot('scheduler'),
    },
    // 避免多个 node_modules 导致的模块重复
    blockList: [
      // 本地源码包加入 watchFolders，但依赖解析统一走 app/runtime/root
      // node_modules，避免 Metro 递归监听多个包自己的依赖树。
      new RegExp(`${workspacePackagesRoot}/(actro-runtime|rill|askit|supramark)/node_modules/.*`),
      // 忽略 actro-runtime 自己的 react 和 react-native
      /..\/actro-runtime\/node_modules\/react\/.*/,
      /..\/actro-runtime\/node_modules\/react-native\/.*/,
      // 屏蔽 supramark 中的 react / react-native（含 bun .bun 目录下的副本）
      new RegExp(`${supramarkRoot}/node_modules/react/.*`),
      new RegExp(`${supramarkRoot}/node_modules/react-native/.*`),
      new RegExp(`${supramarkRoot}/node_modules/\\.bun/.*\\/react/.*`),
      new RegExp(`${supramarkRoot}/node_modules/\\.bun/.*\\/react-native/.*`),
      // 屏蔽 supramark 子包内嵌 node_modules 中的 react / react-native
      new RegExp(`${supramarkRoot}/packages/.*/node_modules/react/.*`),
      new RegExp(`${supramarkRoot}/packages/.*/node_modules/react-native/.*`),
      // 屏蔽 rill 中的 react
      new RegExp(`${rillRoot}/node_modules/react/.*`),
      // 屏蔽 askit 中的 react / react-native
      new RegExp(`${askitRoot}/node_modules/react/.*`),
      new RegExp(`${askitRoot}/node_modules/react-native/.*`),
    ],
    resolveRequest: (context, moduleName, platform) => {
      if (
        moduleName === 'react' ||
        moduleName === 'react/jsx-runtime' ||
        moduleName === 'react/jsx-dev-runtime' ||
        moduleName === 'react-native' ||
        moduleName === 'react-reconciler' ||
        moduleName === 'scheduler'
      ) {
        return { type: 'sourceFile', filePath: resolveFromProject(moduleName) };
      }
      if (moduleName === '@babel/runtime' || moduleName.startsWith('@babel/runtime/')) {
        return { type: 'sourceFile', filePath: resolveFromProject(moduleName) };
      }

      // ---- rill/askit 从源码目录解析 ----
      // yarn file: 依赖是物理拷贝，源码更新后 node_modules 副本不会同步。
      // 让 Metro 直接从 rillRoot/askitRoot 解析，确保始终使用最新源码。
      if (moduleName === 'rill') {
        return { type: 'sourceFile', filePath: path.resolve(rillRoot, 'src', 'host', 'index.ts') };
      }
      if (moduleName === 'rill/let') {
        return { type: 'sourceFile', filePath: path.resolve(rillRoot, 'src', 'guest', 'let', 'index.ts') };
      }
      if (moduleName === 'rill/host/preset') {
        return { type: 'sourceFile', filePath: path.resolve(rillRoot, 'src', 'host', 'preset', 'index.ts') };
      }
      if (moduleName === 'rill/sandbox') {
        return { type: 'sourceFile', filePath: path.resolve(rillRoot, 'src', 'sandbox', 'index.ts') };
      }
      if (moduleName === 'rill/sandbox/native') {
        return { type: 'sourceFile', filePath: path.resolve(rillRoot, 'src', 'sandbox', 'native', 'index.ts') };
      }
      if (moduleName === 'rill/sandbox/web') {
        return { type: 'sourceFile', filePath: path.resolve(rillRoot, 'src', 'sandbox', 'web', 'index.ts') };
      }
      if (moduleName === 'rill/devtools') {
        return { type: 'sourceFile', filePath: path.resolve(rillRoot, 'src', 'devtools', 'index.ts') };
      }
      if (moduleName === 'askit') {
        return { type: 'sourceFile', filePath: path.resolve(askitRoot, 'src', 'index.host.ts') };
      }
      if (moduleName === 'askit/core') {
        return { type: 'sourceFile', filePath: path.resolve(askitRoot, 'src', 'core', 'index.ts') };
      }
      if (moduleName === 'askit/contracts') {
        return { type: 'sourceFile', filePath: path.resolve(askitRoot, 'src', 'contracts', 'index.ts') };
      }

      // ---- 保证 rill/askit 中的 React 相关模块解析到主项目副本 ----
      // rill/askit 源码目录自带了 node_modules/react 等依赖，
      // 如果不重定向，askit 内部的 import { useRef } from 'react' 会解析到自己的 react 副本，
      // 导致 React 实例不一致，出现 "Invalid hook call" 错误。
      const originPath = context.originModulePath || '';
      const isFromLinkedPackage =
        originPath.includes('/node_modules/rill/') ||
        originPath.includes('/node_modules/askit/') ||
        originPath.startsWith(rillRoot + path.sep) ||
        originPath.startsWith(askitRoot + path.sep);

      if (isFromLinkedPackage) {
        if (moduleName === 'react') {
          return { type: 'sourceFile', filePath: resolveFromProject('react') };
        }
        if (moduleName === 'react/jsx-runtime') {
          return { type: 'sourceFile', filePath: resolveFromProject('react/jsx-runtime') };
        }
        if (moduleName === 'react/jsx-dev-runtime') {
          return { type: 'sourceFile', filePath: resolveFromProject('react/jsx-dev-runtime') };
        }
        if (moduleName === 'react-reconciler') {
          return { type: 'sourceFile', filePath: resolveFromProject('react-reconciler') };
        }
        if (moduleName === 'scheduler') {
          return { type: 'sourceFile', filePath: resolveFromProject('scheduler') };
        }
      }

      // ---- RN 不兼容模块的 shim 替换 ----
      // @viz-js/viz 使用 import.meta + WASM，Hermes 不支持，用 shim 让其 fallback 到 Kroki 服务端渲染
      if (moduleName === '@viz-js/viz') {
        return { type: 'sourceFile', filePath: path.resolve(runtimeShimsDir, 'viz-js-viz.js') };
      }

      // tslib 的 ESM/CJS 混合导出兼容
      if (moduleName === 'tslib') {
        return { type: 'sourceFile', filePath: path.resolve(runtimeShimsDir, 'tslib.js') };
      }
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

      // ---- supramark peer dependency: pako ----
      // pako 没有 main 字段（只有 exports），Metro 可能无法正确解析入口。
      // 用 require.resolve 直接定位 CJS 入口。
      if (
        context.originModulePath.includes('@supramark/') &&
        moduleName === 'pako'
      ) {
        try {
          const resolved = require.resolve('pako', {
            paths: [path.resolve(__dirname, 'node_modules')],
          });
          return { type: 'sourceFile', filePath: resolved };
        } catch {}
      }

      // ---- supramark ESM .js → .ts 重写 ----
      // supramark 源码使用 ESM 风格的 `.js` 后缀导入（如 `from './feature.js'`），
      // 但实际文件是 `.ts`。Metro 无法自动解析，需要在这里拦截并重写。
      if (
        (context.originModulePath.startsWith(supramarkRoot) ||
         context.originModulePath.includes('@supramark/')) &&
        moduleName.endsWith('.js') &&
        (moduleName.startsWith('./') || moduleName.startsWith('../'))
      ) {
        const tsName = moduleName.replace(/\.js$/, '.ts');
        try {
          return defaultResolve(context, tsName, platform);
        } catch {}
        const tsxName = moduleName.replace(/\.js$/, '.tsx');
        try {
          return defaultResolve(context, tsxName, platform);
        } catch {}
      }

      return defaultResolve(context, moduleName, platform);
    },
  }
};

module.exports = mergeConfig(defaultConfig, config);
