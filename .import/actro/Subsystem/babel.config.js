module.exports = {
  presets: ['@react-native/babel-preset'],
  plugins: [
    // 仅在非生产环境下注入源码位置
    !isProd && './babel-plugin-function-source-location.js',
  ].filter(Boolean),
};
