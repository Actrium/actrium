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
const loomShimsDir = path.resolve(__dirname, 'loom', 'src', 'actr-host', 'shims');

const config = {
  watchFolders: [
    //path.resolve(__dirname, '../loom'),
    supermarkRoot,
  ],
  resolver: {
    nodeModulesPaths: [
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

      // ---- @supramark/core 必须使用 RN 专用入口 ----
      // 默认 main 是 index.ts（含 remark/unified 等 Node.js 依赖，RN 不兼容），
      // RN 端需要指向 index.rn.ts（额外导出 LRUCache/createCacheKey/simpleHash 等工具）
      if (moduleName === '@supramark/core') {
        return {
          type: 'sourceFile',
          filePath: path.resolve(supermarkRoot, 'packages/core/src/index.rn.ts'),
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
