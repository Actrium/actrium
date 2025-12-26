/**
 * Bun test setup file for askit
 * Provides mock for react-native module
 */

import { mock, spyOn } from 'bun:test';

// Ensure console.createTask exists (required by React)
const consoleWithTask = console as Console & {
  createTask?: () => { run: (fn: () => void) => void };
};
if (!consoleWithTask.createTask) {
  consoleWithTask.createTask = () => ({ run: (fn: () => void) => fn() });
}

// Silence console output during tests
spyOn(console, 'log').mockImplementation(() => {});
spyOn(console, 'warn').mockImplementation(() => {});
spyOn(console, 'error').mockImplementation(() => {});

// Restore createTask after spyOn
consoleWithTask.createTask = () => ({ run: (fn: () => void) => fn() });

// Mock react-native module
mock.module('react-native', () => ({
  View: 'View',
  Text: 'Text',
  Image: 'Image',
  ScrollView: 'ScrollView',
  TouchableOpacity: 'TouchableOpacity',
  ActivityIndicator: 'ActivityIndicator',
  StyleSheet: {
    create: <T extends Record<string, unknown>>(styles: T): T => styles,
    flatten: (style: unknown) => style,
  },
  Platform: {
    OS: 'ios',
    select: <T>(obj: { ios?: T; android?: T; default?: T }): T | undefined =>
      obj.ios ?? obj.default,
  },
  Dimensions: {
    get: () => ({ width: 375, height: 812 }),
  },
  Alert: {
    alert: () => {},
  },
}));
