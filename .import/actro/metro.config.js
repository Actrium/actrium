/**
 * Metro configuration
 * https://reactnative.dev/docs/metro
 *
 * @type {import('@react-native/metro-config').MetroConfig}
 */
const { getDefaultConfig, mergeConfig } = require('@react-native/metro-config');
const { resolve } = require('metro-resolver');
const path = require('path');

const defaultConfig = getDefaultConfig(__dirname);

defaultConfig.resolver.sourceExts = [
  ...defaultConfig.resolver.sourceExts,
  'ts'
];

// 配置 Subsystem 作为 watchFolder
const subsystemPath = path.resolve(__dirname, 'Subsystem');
const supermarkRoot = path.resolve(__dirname, '..', 'supermark');
const loomRoot = path.resolve(__dirname, '..', 'loom');
const loomShimsDir = path.resolve(__dirname, 'loom', 'src', 'actr-host', 'shims');
const loomNodeModules = path.resolve(__dirname, 'loom', 'node_modules');
const yarnLinkRoot = path.resolve(process.env.HOME || '', '.config', 'yarn', 'link');

const config = {
  watchFolders: [
    // 中文注释：当前项目内的 loom 是软链，Metro 需要监听其真实目录。
    loomRoot,
    supermarkRoot,
    yarnLinkRoot,
  ],
  resolver: {
    unstable_enableSymlinks: true,
    nodeModulesPaths: [
      loomNodeModules,
      path.resolve(__dirname, 'node_modules'),
    ],
    // 确保 react 和 react-native 使用主项目的版本，避免冲突
    extraNodeModules: {
      'react': path.resolve(__dirname, 'node_modules/react'),
      'react-native': path.resolve(__dirname, 'node_modules/react-native'),
    },
    // 避免多个 node_modules 导致的模块重复
    blockList: [
      // 忽略 Subsystem 自己的 react 和 react-native
      /Subsystem\/node_modules\/react\/.*/,
      /Subsystem\/node_modules\/react-native\/.*/,
      /..\/loom\/node_modules\/react\/.*/,
      /..\/loom\/node_modules\/react-native\/.*/,
      // 屏蔽 supermark 中的 react / react-native
      new RegExp(`${supermarkRoot}/node_modules/react/.*`),
      new RegExp(`${supermarkRoot}/node_modules/react-native/.*`),
    ],
    resolveRequest: (context, moduleName, platform) => {
      // ---- RN 不兼容模块的 shim 替换 ----
      // @viz-js/viz 使用 import.meta + WASM，Hermes 不支持，用 shim 让其 fallback 到 Kroki 服务端渲染
      if (moduleName === '@viz-js/viz') {
        return { type: 'sourceFile', filePath: path.resolve(loomShimsDir, 'viz-js-viz.js') };
      }
      
      // tslib 的 ESM/CJS 混合导出兼容
      if (moduleName === 'tslib') {
        return { type: 'sourceFile', filePath: path.resolve(loomShimsDir, 'tslib.js') };
      }
      if (moduleName === '@supramark/core') {
        return {
          type: 'sourceFile',
          filePath: path.resolve(supermarkRoot, 'packages/core/src/index.rn.ts'),
        };
      // ---- @supramark/* 包统一从源码目录解析 ----
      // yarn file: 依赖是物理拷贝，源码更新后 node_modules 副本不会同步。
      // 让 Metro 直接从 supermarkRoot 解析，确保始终使用最新源码（含图片等资源）。
      // if (moduleName.startsWith('@supramark/')) {
      //   const fullName = moduleName.slice('@supramark/'.length);
      //   if (!fullName.includes('/')) {
      //     // 顶层包导入（不含子路径如 /web）
      //     let sourceDir;
      //     if (fullName === 'core') {
      //       sourceDir = path.resolve(supermarkRoot, 'packages', 'core');
      //     } else if (fullName.startsWith('feature-')) {
      //       sourceDir = path.resolve(supermarkRoot, 'packages', 'features', fullName);
      //     } else {
      //       sourceDir = path.resolve(supermarkRoot, 'packages', 'renderers', fullName);
      //     }
      //     // core 使用 RN 专用入口 index.rn.ts，其余使用 src/index.ts
      //     const entryFile = fullName === 'core' ? 'src/index.rn.ts' : 'src/index.ts';
      //     return { type: 'sourceFile', filePath: path.resolve(sourceDir, entryFile) };
      //   }
        // 子路径导入（如 @supramark/feature-admonition/web）暂走默认解析
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

      // ---- supermark ESM .js → .ts 重写 ----
      // supermark 源码使用 ESM 风格的 `.js` 后缀导入（如 `from './feature.js'`），
      // 但实际文件是 `.ts`。Metro 无法自动解析，需要在这里拦截并重写。
      if (
        (context.originModulePath.startsWith(supermarkRoot) ||
         context.originModulePath.includes('@supramark/')) &&
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
  }
};

module.exports = mergeConfig(defaultConfig, config);
