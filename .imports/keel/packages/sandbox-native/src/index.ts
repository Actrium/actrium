/**
 * @rill/sandbox-native
 *
 * Native JS sandbox for React Native using JSI bindings.
 *
 * Provides two implementations:
 * - QuickJS: Cross-platform (iOS, Android, macOS, Windows)
 * - JSC: Apple platforms only (iOS, macOS, tvOS, visionOS) - zero binary overhead
 */

export {
  isQuickJSAvailable,
  getQuickJSModule,
  type QuickJSContextNative,
  type QuickJSRuntimeNative,
} from './QuickJSModule';

export {
  isJSCAvailable,
  getJSCModule,
  type JSCContextNative,
  type JSCRuntimeNative,
} from './JSCModule';

/**
 * Common context interface
 */
export interface SandboxContext {
  eval(code: string): unknown;
  setGlobal(name: string, value: unknown): void;
  getGlobal(name: string): unknown;
  dispose(): void;
}

/**
 * Common runtime interface
 */
export interface SandboxRuntime {
  createContext(): SandboxContext;
  dispose(): void;
}

/**
 * Common module interface
 */
export interface SandboxModule {
  createRuntime(options?: { timeout?: number }): SandboxRuntime;
  isAvailable(): boolean;
}

/**
 * Get the best available sandbox module for the current platform
 *
 * Priority:
 * 1. JSC (Apple platforms - zero overhead)
 * 2. QuickJS (cross-platform)
 */
export function getSandboxModule(): SandboxModule | null {
  // Prefer JSC on Apple platforms (uses system JSC, no binary overhead)
  const jscModule = getJSCModuleSafe();
  if (jscModule) {
    return jscModule;
  }

  // Fall back to QuickJS
  const quickjsModule = getQuickJSModuleSafe();
  if (quickjsModule) {
    return quickjsModule;
  }

  return null;
}

function getJSCModuleSafe(): SandboxModule | null {
  try {
    if (
      typeof global !== 'undefined' &&
      global.__JSCSandboxJSI !== undefined &&
      typeof global.__JSCSandboxJSI.isAvailable === 'function' &&
      global.__JSCSandboxJSI.isAvailable()
    ) {
      return global.__JSCSandboxJSI as SandboxModule;
    }
  } catch {
    // Not available
  }
  return null;
}

function getQuickJSModuleSafe(): SandboxModule | null {
  try {
    if (
      typeof global !== 'undefined' &&
      global.__QuickJSSandboxJSI !== undefined &&
      typeof global.__QuickJSSandboxJSI.isAvailable === 'function' &&
      global.__QuickJSSandboxJSI.isAvailable()
    ) {
      return global.__QuickJSSandboxJSI as SandboxModule;
    }
  } catch {
    // Not available
  }
  return null;
}
