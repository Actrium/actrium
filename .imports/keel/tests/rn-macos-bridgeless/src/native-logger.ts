import { NativeModules } from 'react-native';

const { KeelTestLogger } = NativeModules;

/**
 * Native logger that writes directly to stderr (captured by terminal).
 *
 * Uses the KeelTestLogger native module (fprintf to stderr), falls back to
 * console.log when the native module is not available.
 */
export const nativeLog = (message: string): void => {
  if (KeelTestLogger?.log) {
    KeelTestLogger.log(message);
  } else {
    console.log(message);
  }
};
