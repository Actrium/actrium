const fs = require('fs');
const path = require('path');
const { getDefaultConfig, mergeConfig } = require('@react-native/metro-config');
const { resolve } = require('metro-resolver');

const rootDir = __dirname;
const projectRoot = path.resolve(rootDir, 'ReactNativeApp');

// Resolve keel from symlink in node_modules
const keelPath = fs.realpathSync(path.resolve(rootDir, 'node_modules/keel'));

const defaultConfig = getDefaultConfig(projectRoot);

// Block duplicate React instances from symlinked packages
const escapeRegExp = (str) => str.replace(/[.*+?^${}()|[\]\\]/g, '\\$&');
defaultConfig.resolver.blockList = [
  /.*node_modules\/react\/cjs\/react\.development\.js.*inline-requires-plugin.*/,
  new RegExp(`${escapeRegExp(keelPath)}/node_modules/react/.*`),
  new RegExp(`${escapeRegExp(keelPath)}/node_modules/react-reconciler/.*`),
];

/** @type {import('metro-config').MetroConfig} */
const customConfig = {
  watchman: false,
  watcher: {
    watchman: { deferStates: [] },
    healthCheck: { enabled: false },
  },
  watchFolders: [keelPath],
  resolver: {
    unstable_enableSymlinks: true,
    nodeModulesPaths: [path.resolve(rootDir, 'node_modules')],
    resolveRequest: (context, moduleName, platform) => {
      if (moduleName === 'keel') {
        return {
          type: 'sourceFile',
          filePath: path.resolve(keelPath, 'src', 'host', 'index.ts'),
        };
      }

      if (moduleName === 'keel/host/preset') {
        return {
          type: 'sourceFile',
          filePath: path.resolve(keelPath, 'src', 'host', 'preset', 'index.ts'),
        };
      }

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

      return resolve(context, moduleName, platform);
    },
    extraNodeModules: {
      react: path.resolve(rootDir, 'node_modules/react'),
      'react-reconciler': path.resolve(rootDir, 'node_modules/react-reconciler'),
    },
    unstable_enablePackageExports: true,
    sourceExts: ['ts', 'tsx', 'js', 'jsx', 'json', 'mjs', 'cjs'],
  },
};

module.exports = mergeConfig(defaultConfig, customConfig);
