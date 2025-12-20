const path = require('path');

module.exports = {
  dependency: {
    platforms: {
      ios: {
        podspecPath: path.join(__dirname, 'native/platform/RillSandboxNative.podspec'),
      },
      macos: {
        podspecPath: path.join(__dirname, 'native/platform/RillSandboxNative.podspec'),
      },
      android: {
        sourceDir: path.join(__dirname, 'native/platform/android'),
      },
    },
  },
};
