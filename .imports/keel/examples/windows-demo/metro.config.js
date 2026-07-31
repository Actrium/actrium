const fs = require('fs');
const path = require('path');
const { getDefaultConfig, mergeConfig } = require('@react-native/metro-config');
const { resolve } = require('metro-resolver');

const rootDir = __dirname;

// Resolve keel from symlink in node_modules
const keelPath = fs.realpathSync(path.resolve(rootDir, 'node_modules/keel'));

// react-native-windows path (full RN fork with .windows.js overlays)
const rnwPath = fs.realpathSync(
  path.resolve(require.resolve('react-native-windows/package.json'), '..')
);

const defaultConfig = getDefaultConfig(rootDir);

// Block duplicate React instances from symlinked packages + RNW build artifacts
const escapeRegExp = (str) => str.replace(/[.*+?^${}()|[\]\\]/g, '\\$&');
defaultConfig.resolver.blockList = [
  /.*node_modules\/react\/cjs\/react\.development\.js.*inline-requires-plugin.*/,
  new RegExp(`${escapeRegExp(keelPath)}/node_modules/react/.*`),
  new RegExp(`${escapeRegExp(keelPath)}/node_modules/react-reconciler/.*`),
  // RNW: prevent metro crash from windows/ build artifacts
  new RegExp(`${path.resolve(__dirname, 'windows').replace(/[/\\]/g, '/')}.*`),
  new RegExp(`${rnwPath}/build/.*`),
  new RegExp(`${rnwPath}/target/.*`),
  /.*\.ProjectImports\.zip/,
];

/** @type {import('metro-config').MetroConfig} */
const customConfig = {
  watcher: {
    watchman: { deferStates: [] },
    healthCheck: { enabled: false },
  },
  watchFolders: [keelPath],
  resolver: {
    platforms: ['windows', 'android', 'ios'],
    unstable_enableSymlinks: true,
    nodeModulesPaths: [path.resolve(rootDir, 'node_modules')],
    resolveRequest: (context, moduleName, platform) => {
      // keel monorepo resolution
      if (moduleName === 'keel' || moduleName === 'keel/host') {
        return {
          type: 'sourceFile',
          filePath: path.resolve(keelPath, 'src', 'host', 'index.ts'),
        };
      }
      // Test stubs for isolating crash
      if (moduleName === 'keel-test-a') {
        return { type: 'sourceFile', filePath: path.resolve(rootDir, 'stubs', 'keel-test-a.ts') };
      }
      if (moduleName === 'keel-test-b') {
        return { type: 'sourceFile', filePath: path.resolve(rootDir, 'stubs', 'keel-test-b.ts') };
      }
      if (moduleName === 'keel-test-c') {
        return { type: 'sourceFile', filePath: path.resolve(rootDir, 'stubs', 'keel-test-c.ts') };
      }
      if (moduleName === 'keel-test-d') {
        return { type: 'sourceFile', filePath: path.resolve(rootDir, 'stubs', 'keel-test-d.ts') };
      }
      if (moduleName === 'keel-test-eval-guest') {
        return {
          type: 'sourceFile',
          filePath: path.resolve(rootDir, 'stubs', 'keel-test-eval-guest.ts'),
        };
      }
      if (moduleName === 'keel-test-all') {
        return { type: 'sourceFile', filePath: path.resolve(rootDir, 'stubs', 'keel-test-all.ts') };
      }
      if (moduleName === 'keel/host/preset') {
        return {
          type: 'sourceFile',
          filePath: path.resolve(keelPath, 'src', 'host', 'preset', 'index.ts'),
        };
      }

      // Dedupe react/react-reconciler to local node_modules
      if (moduleName === 'react') {
        return {
          type: 'sourceFile',
          filePath: path.resolve(rootDir, 'node_modules', 'react', 'index.js'),
        };
      }
      if (moduleName === 'react/jsx-runtime') {
        return {
          type: 'sourceFile',
          filePath: path.resolve(rootDir, 'node_modules', 'react', 'jsx-runtime.js'),
        };
      }
      if (moduleName === 'react/jsx-dev-runtime') {
        return {
          type: 'sourceFile',
          filePath: path.resolve(rootDir, 'node_modules', 'react', 'jsx-dev-runtime.js'),
        };
      }
      if (moduleName === 'react-reconciler') {
        return {
          type: 'sourceFile',
          filePath: path.resolve(rootDir, 'node_modules', 'react-reconciler', 'index.js'),
        };
      }

      // Windows platform: redirect react-native → react-native-windows
      // RNW is a full RN fork, so all relative imports cascade within RNW
      // where .windows.js overlay files are found by Metro's platform resolution
      if (platform === 'windows') {
        if (moduleName === 'react-native') {
          return resolve(context, 'react-native-windows', platform);
        }
        if (moduleName.startsWith('react-native/')) {
          const subPath = moduleName.slice('react-native/'.length);
          return resolve(context, 'react-native-windows/' + subPath, platform);
        }
      }

      return resolve(context, moduleName, platform);
    },
    extraNodeModules: {
      react: path.resolve(rootDir, 'node_modules/react'),
      'react-reconciler': path.resolve(rootDir, 'node_modules/react-reconciler'),
    },
    unstable_enablePackageExports: true,
    sourceExts: ['ts', 'tsx', 'js', 'jsx', 'json', 'mjs', 'cjs'],
  },
  transformer: {
    getTransformOptions: async () => ({
      transform: {
        experimentalImportSupport: false,
        inlineRequires: true,
      },
    }),
  },
};

module.exports = mergeConfig(defaultConfig, customConfig);
