module.exports = function(api) {
  const isProd = api.env('production') || process.env.NODE_ENV === 'production';

  return {
    presets: ['@react-native/babel-preset'],
    plugins: [
      // 仅在非生产环境下注入源码位置
      !isProd && './babel-plugin-function-source-location.js',
      'react-native-reanimated/plugin',
    ].filter(Boolean),
  };
};
