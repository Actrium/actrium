/**
 * JSC Native Module - JSI binding
 *
 * Provides access to the native JSC sandbox via global.__JSCSandboxJSI
 * Only available on Apple platforms (iOS, macOS, tvOS, visionOS)
 */

declare global {
  var __JSCSandboxJSI:
    | {
        createRuntime(options?: { timeout?: number }): JSCRuntimeNative;
        isAvailable(): boolean;
      }
    | undefined;
}

interface JSCContextNative {
  eval(code: string): unknown;
  setGlobal(name: string, value: unknown): void;
  getGlobal(name: string): unknown;
  dispose(): void;
}

interface JSCRuntimeNative {
  createContext(): JSCContextNative;
  dispose(): void;
}

/**
 * Check if JSC native module is available
 */
export function isJSCAvailable(): boolean {
  return (
    typeof global !== 'undefined' &&
    global.__JSCSandboxJSI !== undefined &&
    typeof global.__JSCSandboxJSI.isAvailable === 'function' &&
    global.__JSCSandboxJSI.isAvailable()
  );
}

/**
 * Get the native JSC module
 */
export function getJSCModule() {
  if (!isJSCAvailable()) {
    return null;
  }
  return global.__JSCSandboxJSI!;
}

// Re-export types
export type { JSCContextNative, JSCRuntimeNative };
