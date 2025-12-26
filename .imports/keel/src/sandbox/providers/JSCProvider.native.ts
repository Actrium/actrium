/**
 * JSCProvider - JavaScriptCore sandbox via native JSI bindings
 *
 * Uses @rill/sandbox-native for JSI bindings to JSC.
 * Only available on Apple platforms (iOS, macOS, tvOS, visionOS).
 */

import { getJSCModule, isJSCAvailable, type JSCContextNative } from '../native/JSCModule';
import type { JSEngineContext, JSEngineProvider, JSEngineRuntime } from '../types/provider';

export interface JSCProviderOptions {
  timeout?: number | undefined;
}

/**
 * JSCProvider - Wraps native JSC JSI module for rill Engine
 */
export class JSCProvider implements JSEngineProvider {
  private options: JSCProviderOptions;

  constructor(options?: JSCProviderOptions) {
    this.options = options || {};
  }

  createRuntime(): JSEngineRuntime {
    const mod = getJSCModule();
    if (!mod) {
      throw new Error('[JSCProvider] JSC native module not available');
    }

    const runtimeOptions =
      this.options.timeout !== undefined ? { timeout: this.options.timeout } : undefined;
    const rt = mod.createRuntime(runtimeOptions);

    return {
      createContext: (): JSEngineContext => {
        const ctx: JSCContextNative = rt.createContext();

        return {
          eval: (code: string): unknown => ctx.eval(code),
          setGlobal: (name: string, value: unknown): void => ctx.setGlobal(name, value),
          getGlobal: (name: string): unknown => ctx.getGlobal(name),
          dispose: (): void => ctx.dispose(),
        };
      },
      dispose: (): void => rt.dispose(),
    };
  }
}

/**
 * Check if JSC native module is available
 */
export { isJSCAvailable };

/**
 * Check if running on Apple platform (for backward compatibility)
 */
export function isApplePlatform(): boolean {
  // On native, this check is done inside @rill/sandbox-native
  // This function is kept for backward compatibility
  return isJSCAvailable();
}

/**
 * Resolve the JSC sandbox module (for backward compatibility)
 */
export function resolveJSCSandbox() {
  return getJSCModule();
}
