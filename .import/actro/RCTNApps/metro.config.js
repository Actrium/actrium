// Metro configuration based on @react-native/metro-config
const path = require('path');
const { getDefaultConfig, mergeConfig } = require('@react-native/metro-config');
const { resolve } = require('metro-resolver');

const projectRoot = __dirname;
const goAskAwayRoot = path.resolve(projectRoot, '../..');

// Local development packages (symlinked via file: protocol)
const localPackages = {
  rill: path.resolve(goAskAwayRoot, 'rill'),
  askit: path.resolve(goAskAwayRoot, 'askit'),
};

const defaultConfig = getDefaultConfig(projectRoot);

// react-native-macos: ensure Metro recognizes macos platform
defaultConfig.resolver.platforms = Array.from(
  new Set([...(defaultConfig.resolver.platforms || []), 'macos']),
);

// Prevent RN interop-with-react-19 stack from trying to import inline-requires-plugin relative to React
// Also block react from local packages' node_modules to ensure single React instance
defaultConfig.resolver.blockList = [
  /.*node_modules\/react\/cjs\/react\.development\.js.*inline-requires-plugin.*/,
  // Block react from symlinked local packages (rill, askit)
  new RegExp(`${goAskAwayRoot}/(rill|askit)/node_modules/react/.*`),
  new RegExp(`${goAskAwayRoot}/(rill|askit)/node_modules/react-reconciler/.*`),
];

const defaultPolyfills = defaultConfig.serializer?.getPolyfills?.() || [];

/** @type {import('metro-config').MetroConfig} */
const customConfig = {
  // Watch local package directories for changes
  watchFolders: Object.values(localPackages),
  serializer: {
    getPolyfills: () => [
      require.resolve('./src/ensure-console-assert.js'),
      ...defaultPolyfills,
    ],
    // InitializeCore must run before the main module to set up globals (window, etc.)
    // Use react-native-macos path since react-native is redirected for macos platform
    getModulesRunBeforeMainModule: () => [
      require.resolve('react-native-macos/Libraries/Core/InitializeCore'),
    ],
  },
  resolver: {
    unstable_enableSymlinks: true,
    nodeModulesPaths: [
      path.resolve(projectRoot, 'node_modules'),
    ],
    resolveRequest: (context, moduleName, platform) => {
      // macOS platform: redirect react-native (and deep imports) to react-native-macos
      // This ensures platform-specific implementations (Image.macos.js, etc.) are found
      // and prevents bundling both react-native and react-native-macos.
      if (platform === 'macos' && (moduleName === 'react-native' || moduleName.startsWith('react-native/'))) {
        const redirected = moduleName === 'react-native'
          ? 'react-native-macos'
          : 'react-native-macos' + moduleName.slice('react-native'.length);
        return resolve(context, redirected, platform);
      }

      // Resolve rill/host to the host entry point
      if (moduleName === 'rill/host') {
        return {
          type: 'sourceFile',
          filePath: path.resolve(goAskAwayRoot, 'rill', 'src', 'host', 'index.ts'),
        };
      }

      // Resolve askit to host entry point
      if (moduleName === 'askit') {
        return {
          type: 'sourceFile',
          filePath: path.resolve(goAskAwayRoot, 'askit', 'src', 'index.host.ts'),
        };
      }
      if (moduleName === 'askit/host') {
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
      // Fix pnpm dependency isolation issue for react-native-macos peer dependencies
      if (moduleName === '@react-native-macos/virtualized-lists') {
        return { type: 'sourceFile', filePath: path.resolve(projectRoot, 'node_modules', '@react-native-macos', 'virtualized-lists', 'index.js') };
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
