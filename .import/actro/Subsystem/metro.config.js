// Metro configuration based on @react-native/metro-config
const path = require('path');
const { getDefaultConfig, mergeConfig } = require('@react-native/metro-config');
const { resolve } = require('metro-resolver');

const projectRoot = __dirname;

const defaultConfig = getDefaultConfig(projectRoot);

// react-native-macos: ensure Metro recognizes macos platform
defaultConfig.resolver.platforms = Array.from(
  new Set([...(defaultConfig.resolver.platforms || []), 'macos']),
);

// Prevent RN interop-with-react-19 stack from trying to import inline-requires-plugin relative to React
defaultConfig.resolver.blockList = [
  /.*node_modules\/react\/cjs\/react\.development\.js.*inline-requires-plugin.*/,
];

const defaultPolyfills = defaultConfig.serializer?.getPolyfills?.() || [];

/** @type {import('metro-config').MetroConfig} */
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
    ],
    resolveRequest: (context, moduleName, platform) => {
      // macOS platform: enable .native extension support (consistent with iOS/Android)
      // Default macOS only tries .macos.ts -> .ts, not .native.ts
      // With preferNativePlatform=true, resolution order becomes: .macos.ts -> .native.ts -> .ts
      if (platform === 'macos' && !context._nativeFallbackAttempted) {
        const nativeContext = {
          ...context,
          preferNativePlatform: true,
          _nativeFallbackAttempted: true,
        };
        try {
          return resolve(nativeContext, moduleName, platform);
        } catch (e) {
          // If resolution with preferNativePlatform fails, continue with original context
        }
      }

      // Ensure single React instance
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

      return resolve(context, moduleName, platform);
    },
    extraNodeModules: {
      'react': path.resolve(projectRoot, 'node_modules/react'),
      'react-dom': path.resolve(projectRoot, 'node_modules/react-dom'),
      'react-reconciler': path.resolve(projectRoot, 'node_modules/react-reconciler'),
    },
    unstable_enablePackageExports: true,
    sourceExts: ['ts', 'tsx', 'js', 'jsx', 'json', 'mjs', 'cjs'],
  },
};

module.exports = mergeConfig(defaultConfig, customConfig);
