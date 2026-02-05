/**
 * Metro configuration
 * https://reactnative.dev/docs/metro
 *
 * @type {import('@react-native/metro-config').MetroConfig}
 */
const { getDefaultConfig, mergeConfig } = require('@react-native/metro-config');
const path = require('path');

const defaultConfig = getDefaultConfig(__dirname);

defaultConfig.resolver.sourceExts = [
  ...defaultConfig.resolver.sourceExts,
  'ts'
];

// 配置 Subsystem 作为 watchFolder
const subsystemPath = path.resolve(__dirname, 'Subsystem');

const config = {
  watchFolders: [subsystemPath],
  resolver: {
    nodeModulesPaths: [
      path.resolve(__dirname, 'Subsystem/node_modules'),
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
    ]
  }
};

module.exports = mergeConfig(defaultConfig, config);
