module.exports = {
  dependencies: {
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
