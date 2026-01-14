/**
 * DevTools Console Interceptor
 *
 * Hooks into global console methods to forward logs to NativeLogger,
 * which then displays them in the DevTools Console panel.
 */
import { NativeModules } from 'react-native';

type NativeLoggerModule = {
  log?: (message: string) => void;
  warn?: (message: string) => void;
  error?: (message: string) => void;
};

const nativeLogger: NativeLoggerModule | undefined = (NativeModules as any).NativeLogger;

// Store original console methods
const originalConsole = {
  log: console.log.bind(console),
  warn: console.warn.bind(console),
  error: console.error.bind(console),
  debug: console.debug.bind(console),
  info: console.info.bind(console),
};

// Format arguments to string
function formatArgs(args: unknown[]): string {
  return args
    .map((arg) => {
      if (arg === undefined) return 'undefined';
      if (arg === null) return 'null';
      if (typeof arg === 'string') return arg;
      if (typeof arg === 'number' || typeof arg === 'boolean') return String(arg);
      if (arg instanceof Error) return `${arg.name}: ${arg.message}\n${arg.stack || ''}`;
      try {
        return JSON.stringify(arg, null, 2);
      } catch {
        return String(arg);
      }
    })
    .join(' ');
}

// Check if already installed
let isInstalled = false;

/**
 * Install console interceptor to forward logs to DevTools
 */
export function installConsoleInterceptor(): void {
  if (isInstalled) return;
  isInstalled = true;

  // Hook console.log
  console.log = (...args: unknown[]) => {
    originalConsole.log(...args);
    if (nativeLogger?.log) {
      nativeLogger.log(formatArgs(args));
    }
  };

  // Hook console.warn
  console.warn = (...args: unknown[]) => {
    originalConsole.warn(...args);
    if (nativeLogger?.warn) {
      nativeLogger.warn(formatArgs(args));
    }
  };

  // Hook console.error
  console.error = (...args: unknown[]) => {
    originalConsole.error(...args);
    if (nativeLogger?.error) {
      nativeLogger.error(formatArgs(args));
    }
  };

  // Hook console.debug (forward to log)
  console.debug = (...args: unknown[]) => {
    originalConsole.debug(...args);
    if (nativeLogger?.log) {
      nativeLogger.log(`[debug] ${formatArgs(args)}`);
    }
  };

  // Hook console.info (forward to log)
  console.info = (...args: unknown[]) => {
    originalConsole.info(...args);
    if (nativeLogger?.log) {
      nativeLogger.log(formatArgs(args));
    }
  };

  // Log that interceptor is installed
  originalConsole.log('[DevTools] Console interceptor installed');
}

/**
 * Restore original console methods
 */
export function uninstallConsoleInterceptor(): void {
  if (!isInstalled) return;
  isInstalled = false;

  console.log = originalConsole.log;
  console.warn = originalConsole.warn;
  console.error = originalConsole.error;
  console.debug = originalConsole.debug;
  console.info = originalConsole.info;
}
