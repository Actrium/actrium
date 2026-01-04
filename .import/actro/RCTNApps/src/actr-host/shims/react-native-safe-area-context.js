// 纯 JS 版本的 react-native-safe-area-context，占位实现
// 避免在 macOS 上访问不存在的原生组件 RNCSafeAreaProvider。

/* eslint-disable no-console */
try {
  // eslint-disable-next-line @typescript-eslint/no-var-requires
  const { logger } = require('../../logger');
  logger.child('shim').debug('Using JS implementation for react-native-safe-area-context');
} catch {
  console.log('[shim] Using JS implementation for react-native-safe-area-context');
}

const React = require('react');
const { View } = require('react-native');

const SafeAreaProvider = ({ children }) => {
  return React.createElement(View, { style: { flex: 1 } }, children);
};

const SafeAreaView = props => {
  return React.createElement(View, props);
};

const useSafeAreaInsets = () => ({
  top: 0,
  bottom: 0,
  left: 0,
  right: 0,
});

const initialWindowMetrics = {
  frame: { x: 0, y: 0, width: 0, height: 0 },
  insets: { top: 0, bottom: 0, left: 0, right: 0 },
};

module.exports = {
  SafeAreaProvider,
  SafeAreaView,
  useSafeAreaInsets,
  initialWindowMetrics,
};

