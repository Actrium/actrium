/**
 * RNJSCSandboxProvider - JavaScriptCore sandbox for React Native (Apple platforms)
 *
 * Uses react-native-jsc-sandbox with JSI bindings for SYNCHRONOUS operations.
 * Same interface as RNQuickJSProvider.
 */

import { Platform } from 'react-native';
import type { JSEngineContext, JSEngineProvider, JSEngineRuntime } from './engine';

// Interface matching react-native-jsc-sandbox (same as QuickJS)
export interface JSCSandboxContext {
  eval(code: string): unknown;
  setGlobal(name: string, value: unknown): void;
  getGlobal(name: string): unknown;
  dispose(): void;
}

export interface JSCSandboxRuntime {
  createContext(): JSCSandboxContext;
  dispose(): void;
}

export interface JSCSandboxModule {
  createRuntime(options?: { timeout?: number }): JSCSandboxRuntime;
}

export interface RNJSCSandboxProviderOptions {
  timeout?: number;
}

/**
 * RNJSCSandboxProvider - Wraps react-native-jsc-sandbox for rill Engine
 */
export class RNJSCSandboxProvider implements JSEngineProvider {
  private mod: JSCSandboxModule;
  private options: RNJSCSandboxProviderOptions;

  constructor(mod: JSCSandboxModule, options?: RNJSCSandboxProviderOptions) {
    this.mod = mod;
    this.options = options || {};
  }

  createRuntime(): JSEngineRuntime {
    const rt = this.mod.createRuntime({
      timeout: this.options.timeout,
    });

    return {
      createContext: (): JSEngineContext => {
        const ctx = rt.createContext();

        return {
          eval: (code: string): unknown => ctx.eval(code),
          evalAsync: async (code: string): Promise<unknown> => ctx.eval(code),
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
 * Check if running on Apple/Darwin platform
 */
export function isApplePlatform(): boolean {
  return (
    Platform.OS === 'ios' ||
    Platform.OS === 'macos' ||
    Platform.OS === 'tvos' ||
    Platform.OS === 'visionos' ||
    (Platform.OS as string) === 'watchos'
  );
}

/**
 * Check if react-native-jsc-sandbox JSI is available
 */
export function isJSCSandboxAvailable(): boolean {
  if (!isApplePlatform()) {
    return false;
  }

  try {
    // eslint-disable-next-line @typescript-eslint/no-var-requires
    const mod = require('react-native-jsc-sandbox');
    return typeof mod?.isJSCSandboxAvailable === 'function' && mod.isJSCSandboxAvailable();
  } catch {
    return false;
  }
}

/**
 * Resolve the JSC sandbox module
 */
export function resolveJSCSandbox(): JSCSandboxModule | null {
  if (!isApplePlatform()) {
    return null;
  }

  try {
    // eslint-disable-next-line @typescript-eslint/no-var-requires
    const mod = require('react-native-jsc-sandbox');
    if (typeof mod?.getJSCSandboxModule === 'function') {
      return mod.getJSCSandboxModule() as JSCSandboxModule | null;
    }
  } catch {
    // Package not available
  }

  return null;
}
