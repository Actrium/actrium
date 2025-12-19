/**
 * QuickJSProvider stub for non-native environments
 * QuickJSProvider requires native JSI bindings (via @rill/sandbox-native)
 */

import type { JSEngineProvider } from '../types/provider';

export interface QuickJSProviderOptions {
  timeout?: number | undefined;
}

export class QuickJSProvider implements JSEngineProvider {
  constructor(_options?: QuickJSProviderOptions) {
    throw new Error(
      '[QuickJSProvider] Requires native JSI bindings. Use VMProvider or WorkerProvider for non-native environments.'
    );
  }

  createRuntime(): never {
    throw new Error('[QuickJSProvider] Requires native JSI bindings.');
  }
}

/**
 * Check if QuickJS native module is available (always false in non-native)
 */
export function isQuickJSAvailable(): boolean {
  return false;
}

/**
 * Resolve the QuickJS module (always null in non-native)
 */
export function resolveQuickJS(): null {
  return null;
}
