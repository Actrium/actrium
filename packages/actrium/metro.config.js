const fs = require('fs');
const path = require('path');
const { getDefaultConfig, mergeConfig } = require('@react-native/metro-config');

const projectRoot = __dirname;
const workspaceRoot = path.resolve(projectRoot, '../..');
const workspacePackagesRoot = path.resolve(projectRoot, '..');
const workspaceNodeModules = path.resolve(workspaceRoot, 'node_modules');
const projectNodeModules = path.resolve(projectRoot, 'node_modules');
const keelRoot = path.resolve(workspacePackagesRoot, 'keel');
const appSdkRoot = path.resolve(workspacePackagesRoot, 'actrium-app-sdk');
const reactNativeMetroConfigRoot = path.dirname(
  require.resolve('@react-native/metro-config/package.json'),
);
const asyncRequireModulePath = require.resolve(
  'metro-runtime/src/modules/asyncRequire',
  { paths: [reactNativeMetroConfigRoot] },
);

const existingDirs = (paths) =>
  Array.from(new Set(paths)).filter((candidate) => fs.existsSync(candidate));

const resolveWithDefault = (context, moduleName, platform) =>
  context.resolveRequest(context, moduleName, platform);

const defaultConfig = getDefaultConfig(projectRoot);
const customConfig = {
  projectRoot,
  watchFolders: existingDirs([
    keelRoot,
    appSdkRoot,
    workspaceNodeModules,
  ]),
  serializer: {
    getModulesRunBeforeMainModule: () => [],
  },
  transformer: {
    asyncRequireModulePath,
  },
  resolver: {
    unstable_enableSymlinks: true,
    unstable_enablePackageExports: true,
    nodeModulesPaths: [projectNodeModules, workspaceNodeModules],
    blockList: [
      new RegExp(`${keelRoot}/node_modules/.*`),
      new RegExp(`${appSdkRoot}/node_modules/.*`),
    ],
    extraNodeModules: {
      react: path.resolve(projectNodeModules, 'react'),
      'react-native': path.resolve(projectNodeModules, 'react-native'),
      'react-reconciler': path.resolve(projectNodeModules, 'react-reconciler'),
    },
    resolveRequest: (context, moduleName, platform) => {
      if (platform === 'macos' && moduleName === './guest-svg-renderer') {
        return {
          type: 'sourceFile',
          filePath: path.join(projectRoot, 'src/runtime-host/guest-svg-renderer.rnsvg.js'),
        };
      }
      if (platform === 'macos' && (
        moduleName === 'react-native-svg' || moduleName === 'react-native-svg/lib/commonjs/xml'
      )) {
        // Use one Fabric component registration for both Guest SVG and Supramark.
        const source = moduleName === 'react-native-svg' ? moduleName : 'react-native-svg/src/xml';
        return resolveWithDefault({ ...context, originModulePath: __filename }, source, platform);
      }
      if (platform === 'macos' && moduleName === './guest-markdown-renderer') {
        return {
          type: 'sourceFile',
          filePath: path.join(projectRoot, 'src/runtime-host/guest-markdown-renderer.supramark.js'),
        };
      }

      if (moduleName === '@babel/runtime' || moduleName.startsWith('@babel/runtime/')) {
        return {
          type: 'sourceFile',
          filePath: require.resolve(moduleName, { paths: [projectRoot] }),
        };
      }

      if (
        platform === 'macos' &&
        (moduleName === 'react-native' || moduleName.startsWith('react-native/'))
      ) {
        const redirected =
          moduleName === 'react-native'
            ? 'react-native-macos'
            : `react-native-macos${moduleName.slice('react-native'.length)}`;
        return {
          type: 'sourceFile',
          filePath: require.resolve(redirected, { paths: [projectRoot] }),
        };
      }

      if (
        (platform === 'ios' || platform === 'android') &&
        moduleName === 'react-native'
      ) {
        return {
          type: 'sourceFile',
          filePath: path.resolve(projectNodeModules, 'react-native/index.js'),
        };
      }

      const localModules = {
        keel: path.resolve(keelRoot, 'src/host/index.ts'),
        'keel/contract': path.resolve(keelRoot, 'src/contract/index.ts'),
        'keel/host': path.resolve(keelRoot, 'src/host/index.ts'),
        'keel/host/preset': path.resolve(keelRoot, 'src/host/preset/index.ts'),
        'keel/v1': path.resolve(keelRoot, 'src/v1.ts'),
        askit: path.resolve(appSdkRoot, 'src/index.host.ts'),
        'askit/core': path.resolve(appSdkRoot, 'src/core/index.ts'),
        'askit/contracts': path.resolve(appSdkRoot, 'src/contracts/index.ts'),
      };
      if (localModules[moduleName]) {
        return { type: 'sourceFile', filePath: localModules[moduleName] };
      }

      const singletonModules = {
        react: 'react/index.js',
        'react/jsx-runtime': 'react/jsx-runtime.js',
        'react/jsx-dev-runtime': 'react/jsx-dev-runtime.js',
        'react-reconciler': 'react-reconciler/index.js',
      };
      if (singletonModules[moduleName]) {
        return {
          type: 'sourceFile',
          filePath: path.resolve(projectNodeModules, singletonModules[moduleName]),
        };
      }

      if (moduleName === 'tslib') {
        return {
          type: 'sourceFile',
          filePath: path.resolve(projectRoot, 'src/runtime-host/shims/tslib.js'),
        };
      }

      return resolveWithDefault(context, moduleName, platform);
    },
    sourceExts: ['ts', 'tsx', 'js', 'jsx', 'json', 'mjs', 'cjs'],
  },
};

module.exports = mergeConfig(defaultConfig, customConfig);
