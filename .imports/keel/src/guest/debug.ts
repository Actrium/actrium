/**
 * Debug Utilities for Sandbox
 *
 * Proxy-based shim wrapper that catches undefined property access.
 */

// ============================================
// Types
// ============================================

/**
 * Debug mode flag
 */
let debugMode = false;

/**
 * Enable or disable debug mode
 */
export function setDebugMode(enabled: boolean): void {
  debugMode = enabled;
}

/**
 * Check if debug mode is enabled
 */
export function isDebugMode(): boolean {
  return debugMode;
}

// ============================================
// Safe Shim Creator
// ============================================

/**
 * Internal symbols and properties to ignore in proxy
 */
const IGNORED_PROPERTIES = new Set<string | symbol>([
  'then', // Promise detection
  'toJSON', // JSON serialization
  'valueOf', // Primitive conversion
  'toString', // String conversion
  'constructor', // Prototype chain
  'prototype', // Prototype chain
  'length', // Array-like
  Symbol.iterator,
  Symbol.toStringTag,
  Symbol.toPrimitive,
]);

/**
 * Create a safe shim wrapper that logs/throws on undefined property access
 *
 * @param shimName - Name of the shim for error messages
 * @param obj - The object to wrap
 * @returns Proxied object that catches undefined access
 */
export function createSafeShim<T extends object>(shimName: string, obj: T): T {
  // If Proxy is not available, return object as-is
  if (typeof Proxy === 'undefined') {
    return obj;
  }

  return new Proxy(obj, {
    get(target: T, prop: string | symbol): unknown {
      // Ignore symbols and internal properties
      if (typeof prop === 'symbol' || IGNORED_PROPERTIES.has(prop)) {
        return Reflect.get(target, prop);
      }

      // Ignore React internal properties
      if (typeof prop === 'string' && prop.startsWith('$$')) {
        return Reflect.get(target, prop);
      }

      // Check if property exists
      if (!(prop in target)) {
        const message = `[rill:shim] ${shimName}.${String(prop)} does not exist! Check if shims need updating.`;

        if (debugMode) {
          throw new Error(message);
        } else {
          console.error(message);
        }
      }

      return Reflect.get(target, prop);
    },
  });
}

// ============================================
// Debug Logging
// ============================================

/**
 * Log only in debug mode
 */
export function debugLog(...args: unknown[]): void {
  if (debugMode) {
    console.log('[rill:debug]', ...args);
  }
}

/**
 * Warn only in debug mode
 */
export function debugWarn(...args: unknown[]): void {
  if (debugMode) {
    console.warn('[rill:debug]', ...args);
  }
}
