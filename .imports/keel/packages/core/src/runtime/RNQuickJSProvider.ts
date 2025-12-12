// React Native QuickJS Provider adapter
// Attempts to adapt either 'react-native-quickjs' or 'react-native-quick-js' packages to Engine's provider interface.

import type { JSEngineContext, JSEngineProvider, JSEngineRuntime } from './engine';

export interface RNQuickJSContext {
  eval(code: string): unknown;
  evalAsync?(code: string): Promise<unknown>;
  setGlobal(name: string, value: unknown): void;
  getGlobal(name: string): unknown;
  dispose(): void;
  // Optional interrupt handler support (depends on native package version)
  setInterruptHandler?(handler: () => boolean): void;
  clearInterruptHandler?(): void;
}

export interface RNQuickJSRuntime {
  createContext(): RNQuickJSContext;
  dispose(): void;
  // Optional: some native packages support setting timeout at runtime level
  setTimeout?(ms: number): void;
  setMemoryLimit?(bytes: number): void;
  setMaxStackSize?(size: number): void;
}

export interface RNQuickJSLike {
  createRuntime: (options?: { timeout?: number; memoryLimit?: number }) => RNQuickJSRuntime;
}

export interface RNQuickJSProviderOptions {
  timeout?: number;
  memoryLimit?: number;
}

export class RNQuickJSProvider implements JSEngineProvider {
  private mod: RNQuickJSLike;
  private options: RNQuickJSProviderOptions;

  constructor(mod: RNQuickJSLike, options?: RNQuickJSProviderOptions) {
    this.mod = mod;
    this.options = options || {};
  }

  createRuntime(): JSEngineRuntime {
    // Pass timeout and memory limit to the native runtime if supported
    const rt = this.mod.createRuntime({
      timeout: this.options.timeout,
      memoryLimit: this.options.memoryLimit,
    });

    // Some native packages support setting timeout after creation
    if (this.options.timeout && rt.setTimeout) {
      rt.setTimeout(this.options.timeout);
    }

    return {
      createContext: (): JSEngineContext => {
        const ctx = rt.createContext();

        // Wrap the context to ensure interface compatibility
        const wrappedContext: JSEngineContext = {
          eval: (code: string) => ctx.eval(code),
          evalAsync: ctx.evalAsync ? (code: string) => ctx.evalAsync!(code) : undefined,
          setGlobal: (name: string, value: unknown) => ctx.setGlobal(name, value),
          getGlobal: (name: string) => ctx.getGlobal(name),
          dispose: () => ctx.dispose(),
          // Pass through interrupt handler methods if available
          setInterruptHandler: ctx.setInterruptHandler
            ? (handler: () => boolean) => ctx.setInterruptHandler!(handler)
            : undefined,
          clearInterruptHandler: ctx.clearInterruptHandler
            ? () => ctx.clearInterruptHandler!()
            : undefined,
        };

        return wrappedContext;
      },
      dispose: () => rt.dispose?.(),
    };
  }
}

export function resolveRNQuickJS(): RNQuickJSLike | null {
  try {
    // Try common RN QuickJS packages
    // eslint-disable-next-line @typescript-eslint/no-var-requires
    const m = require('react-native-quickjs');
    if (m?.createRuntime) return m as RNQuickJSLike;
  } catch {
    // Package not available, try next option
  }
  try {
    // eslint-disable-next-line @typescript-eslint/no-var-requires
    const m = require('react-native-quick-js');
    if (m?.createRuntime) return m as RNQuickJSLike;
  } catch {
    // Package not available
  }
  return null;
}
