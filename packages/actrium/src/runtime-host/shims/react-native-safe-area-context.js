// Pure JS Version react-native-safe-area-context, in place
// Avoid in macOS Previous access to non-existent primary components RNCSafeAreaProvider.

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

