// Metro 配置，基于 React Native 官方推荐的 @react-native/metro-config
// 这样才能正确处理 Flow 语法（例如 `import typeof`）等 React Native 源码特性
const path = require('path');
const { getDefaultConfig, mergeConfig } = require('@react-native/metro-config');
const { resolve } = require('metro-resolver');

const projectRoot = __dirname;
// Canvas/Artifacts -> Canvas -> GoAskAway
const workspaceRoot = path.resolve(__dirname, '..', '..');
const workspaceAskitRoot = path.resolve(workspaceRoot, 'askit');
const workspaceRillCoreRoot = path.resolve(workspaceRoot, 'rill', 'packages', 'core');

const defaultConfig = getDefaultConfig(projectRoot);

// react-native-macos：确保 Metro 认识 macos 平台
defaultConfig.resolver.platforms = Array.from(
  new Set([...(defaultConfig.resolver.platforms || []), 'macos']),
);


// Prevent RN interop-with-react-19 stack from trying to import inline-requires-plugin relative to React
// and avoid hoisted duplicate copies of React/React-RC
defaultConfig.resolver.blockList = [
  /.*node_modules\/react\/cjs\/react\.development\.js.*inline-requires-plugin.*/,
];

/** @type {import('metro-config').MetroConfig} */
// 获取默认 polyfills 以便在其之前注入我们的兜底
const defaultPolyfills = defaultConfig.serializer?.getPolyfills?.() || [];

const customConfig = {
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
      path.resolve(workspaceRoot, 'node_modules'),
    ],
    /**
     * 强制把 monorepo 内的 askit/rill 解析到 workspace 源码，避免：
     * - actr-host/node_modules 里残留的旧版 askit 导致导出缺失（EngineMonitorOverlay/PanelMarker/extractPanels 为 undefined）
     * - rill 通过 symlink 指向 workspace 时不在 watchFolders 导致解析/转译不稳定
     */
    resolveRequest: (context, moduleName, platform) => {
      const toSourceFile = (filePath) => ({ type: 'sourceFile', filePath });

      // macOS 平台：启用 .native 扩展名支持（与 iOS/Android 行为一致）
      // 默认 macOS 只尝试 .macos.ts → .ts，不包含 .native.ts
      // 设置 preferNativePlatform=true 后解析顺序变为: .macos.ts → .native.ts → .ts
      if (platform === 'macos' && !context._nativeFallbackAttempted) {
        const nativeContext = {
          ...context,
          preferNativePlatform: true,
          _nativeFallbackAttempted: true, // 防止无限递归
        };
        try {
          return resolve(nativeContext, moduleName, platform);
        } catch (e) {
          // 如果带 preferNativePlatform 的解析失败，继续使用原始 context
        }
      }

      // askit（Host 入口）
      if (moduleName === 'askit') {
        return toSourceFile(path.resolve(workspaceAskitRoot, 'src', 'index.host.ts'));
      }
      if (moduleName === 'askit/core') {
        return toSourceFile(path.resolve(workspaceAskitRoot, 'src', 'core', 'index.ts'));
      }
      if (moduleName === 'askit/contracts') {
        return toSourceFile(path.resolve(workspaceAskitRoot, 'src', 'contracts', 'index.ts'));
      }

      // rill（Host 入口）
      if (moduleName === 'rill') {
        return toSourceFile(path.resolve(workspaceRillCoreRoot, 'src', 'runtime', 'index.ts'));
      }
      if (moduleName === 'rill/sdk') {
        return toSourceFile(path.resolve(workspaceRillCoreRoot, 'src', 'sdk', 'index.ts'));
      }
      if (moduleName === 'rill/devtools') {
        return toSourceFile(path.resolve(workspaceRillCoreRoot, 'src', 'devtools', 'index.ts'));
      }
      if (moduleName === 'rill/types') {
        return toSourceFile(path.resolve(workspaceRillCoreRoot, 'src', 'types.ts'));
      }

      // 兼容 @rill/core 名称（避免第三方/旧代码混入）
      if (moduleName === '@rill/core') {
        return toSourceFile(path.resolve(workspaceRillCoreRoot, 'src', 'runtime', 'index.ts'));
      }
      if (moduleName === '@rill/core/sdk') {
        return toSourceFile(path.resolve(workspaceRillCoreRoot, 'src', 'sdk', 'index.ts'));
      }
      if (moduleName === '@rill/core/devtools') {
        return toSourceFile(path.resolve(workspaceRillCoreRoot, 'src', 'devtools', 'index.ts'));
      }
      if (moduleName === '@rill/core/types') {
        return toSourceFile(path.resolve(workspaceRillCoreRoot, 'src', 'types.ts'));
      }

      // 确保只使用宿主的 React/JSX 运行时（避免 askit 自带 node_modules/react 重复加载导致 hooks dispatcher 为空）
      if (moduleName === 'react') {
        return toSourceFile(path.resolve(projectRoot, 'node_modules', 'react', 'index.js'));
      }
      if (moduleName === 'react/jsx-runtime') {
        return toSourceFile(path.resolve(projectRoot, 'node_modules', 'react', 'jsx-runtime.js'));
      }
      if (moduleName === 'react/jsx-dev-runtime') {
        return toSourceFile(path.resolve(projectRoot, 'node_modules', 'react', 'jsx-dev-runtime.js'));
      }
      if (moduleName === 'react-reconciler') {
        return toSourceFile(path.resolve(projectRoot, 'node_modules', 'react-reconciler', 'index.js'));
      }
      if (moduleName === 'react-dom') {
        return toSourceFile(path.resolve(projectRoot, 'node_modules', 'react-dom', 'index.js'));
      }

      return resolve(context, moduleName, platform);
    },
    extraNodeModules: {
      // Force single React instance to avoid resolution oddities
      'react': path.resolve(projectRoot, 'node_modules/react'),
      'react-dom': path.resolve(projectRoot, 'node_modules/react-dom'),
      'react-reconciler': path.resolve(projectRoot, 'node_modules/react-reconciler'),
      // Direct GitHub dependencies
      'rill': path.resolve(projectRoot, 'node_modules/rill'),
      // Prefer workspace source for askit (TypeScript-only, no npm publish)
      'askit': path.resolve(workspaceRoot, 'askit'),
    },
    unstable_enablePackageExports: true,
    sourceExts: ['ts', 'tsx', 'js', 'jsx', 'json', 'mjs', 'cjs'],
  },
  // ⚠️ watchFolders 过大（直接指向 workspaceRoot）会导致 Metro 在某些环境下触发 EMFILE（too many open files）
  // 这里仅监听需要热更新/解析的 workspace 源码目录（askit + rill/core），避免把整个 monorepo 都纳入 watcher。
  watchFolders: [
    workspaceAskitRoot,
    workspaceRillCoreRoot,
  ],
};

module.exports = mergeConfig(defaultConfig, customConfig);
