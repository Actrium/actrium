module.exports = {
  presets: ['@react-native/babel-preset'],
  plugins: [
    // Inject source location into function expressions for DevTools navigation
    './babel-plugin-function-source-location.js',
  ],
};
