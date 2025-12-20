/**
 * QuickJSProvider - QuickJS sandbox via native JSI bindings
 *
 * Uses @rill/sandbox-native for JSI bindings to QuickJS.
 * Available on iOS, Android, macOS, Windows.
 */

import {
  isQuickJSAvailable,
  getQuickJSModule,
  type QuickJSContextNative,
} from '../../sandbox-native/QuickJSModule';
import type { JSEngineContext, JSEngineProvider, JSEngineRuntime } from '../types/provider';

export interface QuickJSProviderOptions {
  timeout?: number | undefined;
}

/**
 * QuickJSProvider - Wraps native QuickJS JSI module for rill Engine
 */
export class QuickJSProvider implements JSEngineProvider {
  private options: QuickJSProviderOptions;

  constructor(options?: QuickJSProviderOptions) {
    this.options = options || {};
  }

  createRuntime(): JSEngineRuntime {
    const mod = getQuickJSModule();
    if (!mod) {
      throw new Error('[QuickJSProvider] QuickJS native module not available');
    }

    const runtimeOptions =
      this.options.timeout !== undefined ? { timeout: this.options.timeout } : undefined;
    const rt = mod.createRuntime(runtimeOptions);

    return {
      createContext: (): JSEngineContext => {
        const ctx: QuickJSContextNative = rt.createContext();

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
 * Check if QuickJS native module is available
 */
export { isQuickJSAvailable };

/**
 * Resolve the QuickJS module (for backward compatibility)
 */
export function resolveQuickJS() {
  return getQuickJSModule();
}
