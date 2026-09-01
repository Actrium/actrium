const fs = require('fs');
const path = require('path');
const { getDefaultConfig, mergeConfig } = require('@react-native/metro-config');

const projectRoot = __dirname;
const workspaceRoot = path.resolve(projectRoot, '..');
const workspacePackagesRoot = path.resolve(workspaceRoot, 'packages');
const workspaceNodeModules = path.resolve(workspaceRoot, 'node_modules');
const projectNodeModules = path.resolve(projectRoot, 'node_modules');
const runtimeRoot = path.resolve(workspacePackagesRoot, 'actrium');
const keelRoot = path.resolve(workspacePackagesRoot, 'keel');
const appSdkRoot = path.resolve(workspacePackagesRoot, 'actrium-app-sdk');
const runtimeShimsDir = path.resolve(runtimeRoot, 'src/actr-host/shims');

const existingDirs = (paths) =>
  Array.from(new Set(paths)).filter((candidate) => fs.existsSync(candidate));
const resolveFromProject = (moduleName) =>
  require.resolve(moduleName, { paths: [projectRoot] });
const defaultResolve = (context, moduleName, platform) =>
  context.resolveRequest(context, moduleName, platform);

const defaultConfig = getDefaultConfig(projectRoot);
defaultConfig.resolver.sourceExts = [
  ...defaultConfig.resolver.sourceExts,
  'ts',
];

const config = {
  watchFolders: existingDirs([
    runtimeRoot,
    keelRoot,
    appSdkRoot,
    workspaceNodeModules,
  ]),
  resolver: {
    unstable_enableSymlinks: true,
    unstable_enablePackageExports: true,
    nodeModulesPaths: [workspaceNodeModules, projectNodeModules],
    blockList: [
      new RegExp(`${runtimeRoot}/node_modules/.*`),
      new RegExp(`${keelRoot}/node_modules/.*`),
      new RegExp(`${appSdkRoot}/node_modules/.*`),
    ],
    extraNodeModules: {
      react: path.dirname(resolveFromProject('react/package.json')),
      'react-native': path.dirname(resolveFromProject('react-native/package.json')),
      'react-reconciler': path.dirname(
        resolveFromProject('react-reconciler/package.json'),
      ),
      scheduler: path.dirname(resolveFromProject('scheduler/package.json')),
    },
    resolveRequest: (context, moduleName, platform) => {
      if (
        moduleName === 'react' ||
        moduleName === 'react/jsx-runtime' ||
        moduleName === 'react/jsx-dev-runtime' ||
        moduleName === 'react-native' ||
        moduleName === 'react-reconciler' ||
        moduleName === 'scheduler' ||
        moduleName === '@babel/runtime' ||
        moduleName.startsWith('@babel/runtime/')
      ) {
        return { type: 'sourceFile', filePath: resolveFromProject(moduleName) };
      }

      const localModules = {
        keel: path.resolve(keelRoot, 'src/host/index.ts'),
        'keel/guest': path.resolve(keelRoot, 'src/guest/index.ts'),
        'keel/host/preset': path.resolve(keelRoot, 'src/host/preset/index.ts'),
        askit: path.resolve(appSdkRoot, 'src/index.host.ts'),
        'askit/core': path.resolve(appSdkRoot, 'src/core/index.ts'),
        'askit/contracts': path.resolve(appSdkRoot, 'src/contracts/index.ts'),
      };
      if (localModules[moduleName]) {
        return { type: 'sourceFile', filePath: localModules[moduleName] };
      }

      if (moduleName === 'tslib') {
        return {
          type: 'sourceFile',
          filePath: path.resolve(runtimeShimsDir, 'tslib.js'),
        };
      }

      return defaultResolve(context, moduleName, platform);
    },
  },
};

module.exports = mergeConfig(defaultConfig, config);
