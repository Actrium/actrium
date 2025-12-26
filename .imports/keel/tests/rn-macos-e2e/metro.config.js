const {getDefaultConfig, mergeConfig} = require('@react-native/metro-config');
const path = require('path');

const rillRoot = path.resolve(__dirname, '../../..');

const config = {
  watchFolders: [rillRoot],
  resolver: {
    extraNodeModules: {
      rill: rillRoot,
    },
  },
};

module.exports = mergeConfig(getDefaultConfig(__dirname), config);
