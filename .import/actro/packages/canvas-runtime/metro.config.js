// Metro configuration based on @react-native/metro-config
const path = require('path');
const fs = require('fs');
const { getDefaultConfig, mergeConfig } = require('@react-native/metro-config');
const { resolve } = require('metro-resolver');

const projectRoot = fs.realpathSync.native(__dirname);
const workspacePackagesRoot = path.resolve(projectRoot, '..');
const resolveLocalPackage = (name) => {
  const packagePath = path.resolve(workspacePackagesRoot, name);
  return fs.realpathSync.native(packagePath);
};
const escapeRegExp = (value) => value.replace(/[.*+?^${}()|[\]\\]/g, '\\$&');

// Local development packages in the Actro workspace.
const localPackages = {
  rill: resolveLocalPackage('rill'),
  askit: resolveLocalPackage('askit'),
};
const localPackageRoots = Object.values(localPackages);
const localPackageEntries = {
  'rill': path.resolve(localPackages.rill, 'src', 'index.ts'),
  'rill/host': path.resolve(localPackages.rill, 'src', 'host', 'index.ts'),
  'rill/host/preset': path.resolve(localPackages.rill, 'src', 'host', 'preset', 'index.ts'),
  'rill/guest': path.resolve(localPackages.rill, 'src', 'sdk', 'index.ts'),
  'rill/contract': path.resolve(localPackages.rill, 'src', 'contract', 'index.ts'),
  'askit': path.resolve(localPackages.askit, 'src', 'index.host.ts'),
  'askit/host': path.resolve(localPackages.askit, 'src', 'core', 'index.ts'),
  'askit/contracts': path.resolve(localPackages.askit, 'src', 'contracts', 'index.ts'),
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
  ...localPackageRoots.map(
    (root) => new RegExp(`${escapeRegExp(root)}/node_modules/react/.*`),
  ),
  ...localPackageRoots.map(
    (root) => new RegExp(`${escapeRegExp(root)}/node_modules/react-reconciler/.*`),
  ),
];

const defaultPolyfills = defaultConfig.serializer?.getPolyfills?.() || [];

/** @type {import('metro-config').MetroConfig} */
const customConfig = {
  // Watch local package directories for changes
  watchFolders: localPackageRoots,
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

      if (localPackageEntries[moduleName]) {
        return {
          type: 'sourceFile',
          filePath: localPackageEntries[moduleName],
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
