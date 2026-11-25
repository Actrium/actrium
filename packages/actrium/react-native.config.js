const path = require('node:path');

const supramark = process.env.ACTRIUM_SUPRAMARK_DIR;
const supramarkDependencies = supramark ? {
  '@supramark/markdown-native-rn': {
    root: path.resolve(supramark, 'crates/supramark-markdown/packages/react-native'),
  },
} : {};

module.exports = {
  dependencies: {
    ...supramarkDependencies,
    'react-native-svg': {
      platforms: {
        android: null,
        ...(process.env.ACTRIUM_MACOS_CODEGEN === '1' ? {} : { ios: null }),
      },
    },
    // This runtime is shared by iOS, Android, and macOS. Keep react-native-macos
    // available for Metro's macOS platform resolver, but never autolink or codegen
    // it while CocoaPods/Gradle are configuring the mobile apps.
    'react-native-macos': {
      platforms: {
        android: null,
        ios: null,
      },
    },
    '@react-native-macos/virtualized-lists': {
      platforms: {
        android: null,
        ios: null,
      },
    },
  },
};
