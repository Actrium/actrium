import { NativeModules, Platform } from 'react-native';

const { RillTestLogger } = NativeModules;

/**
 * Native logger that writes directly to stderr (captured by terminal)
 * Falls back to console.log on unsupported platforms
 */
export const nativeLog = (message: string): void => {
  if (Platform.OS === 'macos' && RillTestLogger?.log) {
    RillTestLogger.log(message);
  } else {
    console.log(message);
  }
};

/**
 * Writes message to a file (for test result parsing)
 */
export const logToFile = (message: string, path: string): void => {
  if (Platform.OS === 'macos' && RillTestLogger?.logToFile) {
    RillTestLogger.logToFile(message, path);
  }
};
