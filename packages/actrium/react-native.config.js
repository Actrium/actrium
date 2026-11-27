const appleCodegen = process.env.ACTRIUM_MACOS_CODEGEN === '1' ? {} : { ios: null };

module.exports = {
  dependencies: {
    // macOS uses Apple's codegen even though its pod is linked explicitly.
    '@goaskaway/supramark-markdown-native-rn': {
      platforms: {
        android: null,
        ...appleCodegen,
      },
    },
    'react-native-svg': {
      platforms: {
        android: null,
        ...appleCodegen,
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
