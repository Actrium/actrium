/**
 * QuickJS Native Module - JSI binding
 *
 * Provides access to the native QuickJS sandbox via global.__QuickJSSandboxJSI
 */

declare global {
  var __QuickJSSandboxJSI:
    | {
        createRuntime(options?: { timeout?: number }): QuickJSRuntimeNative;
        isAvailable(): boolean;
      }
    | undefined;
}

interface QuickJSContextNative {
  eval(code: string): unknown;
  setGlobal(name: string, value: unknown): void;
  getGlobal(name: string): unknown;
  dispose(): void;
}

interface QuickJSRuntimeNative {
  createContext(): QuickJSContextNative;
  dispose(): void;
}

/**
 * Check if QuickJS native module is available
 */
export function isQuickJSAvailable(): boolean {
  return (
    typeof global !== 'undefined' &&
    global.__QuickJSSandboxJSI !== undefined &&
    typeof global.__QuickJSSandboxJSI.isAvailable === 'function' &&
    global.__QuickJSSandboxJSI.isAvailable()
  );
}

/**
 * Get the native QuickJS module
 */
export function getQuickJSModule() {
  if (!isQuickJSAvailable()) {
    return null;
  }
  return global.__QuickJSSandboxJSI!;
}

// Re-export types
export type { QuickJSContextNative, QuickJSRuntimeNative };
