/**
 * HermesProvider - Hermes sandbox via native JSI bindings
 *
 * Uses @rill/sandbox-native for JSI bindings to Hermes.
 * Available when RILL_SANDBOX_ENGINE=hermes is set during native build.
 */

import {
  getHermesModule,
  type HermesContextNative,
  isHermesAvailable,
} from '../native/HermesModule';
import type { JSEngineContext, JSEngineProvider, JSEngineRuntime } from '../types/provider';

export interface HermesProviderOptions {
  timeout?: number | undefined;
}

/**
 * HermesProvider - Wraps native Hermes JSI module for rill Engine
 */
export class HermesProvider implements JSEngineProvider {
  private options: HermesProviderOptions;

  constructor(options?: HermesProviderOptions) {
    this.options = options || {};
  }

  createRuntime(): JSEngineRuntime {
    const mod = getHermesModule();
    if (!mod) {
      throw new Error('[HermesProvider] Hermes native module not available');
    }

    const runtimeOptions =
      this.options.timeout !== undefined ? { timeout: this.options.timeout } : undefined;
    const rt = mod.createRuntime(runtimeOptions);

    return {
      createContext: (): JSEngineContext => {
        const ctx: HermesContextNative = rt.createContext();

        return {
          eval: (code: string): unknown => ctx.eval(code),
          evalBytecode: (bytecode: ArrayBuffer): unknown => ctx.evalBytecode(bytecode),
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
 * Check if Hermes native module is available
 */
export { isHermesAvailable };
