/**
 * RNJSCSandboxProvider - JavaScriptCore sandbox for React Native (Apple platforms)
 *
 * Uses react-native-jsc-sandbox which provides true sandbox isolation via Apple's
 * built-in JavaScriptCore engine. Available on all Apple platforms:
 * - iOS 13.0+
 * - macOS 10.15+
 * - tvOS 13.0+
 * - watchOS 6.0+
 * - visionOS 1.0+
 *
 * Zero binary size overhead since JSC is a system framework.
 */

import { Platform } from 'react-native';
import type { JSEngineContext, JSEngineProvider, JSEngineRuntime } from './engine';

// Interface matching react-native-jsc-sandbox exports
export interface JSCSandboxContext {
  eval(code: string): unknown;
  evalAsync(code: string): Promise<unknown>;
  setGlobal(name: string, value: unknown): Promise<void>;
  getGlobal(name: string): Promise<unknown>;
  dispose(): void;
  setInterruptHandler?(handler: () => boolean): void;
  clearInterruptHandler?(): void;
}

export interface JSCSandboxRuntime {
  createContext(): JSCSandboxContext;
  dispose(): void;
}

export interface JSCSandboxProviderLike {
  createRuntime(): JSCSandboxRuntime;
}

export interface JSCSandboxModule {
  JSCSandboxProvider: new (options?: {
    timeout?: number;
    memoryLimit?: number;
  }) => JSCSandboxProviderLike;
  createJSCSandboxProvider: (options?: {
    timeout?: number;
    memoryLimit?: number;
  }) => JSCSandboxProviderLike;
  isJSCSandboxAvailable: () => boolean;
}

export interface RNJSCSandboxProviderOptions {
  timeout?: number;
  memoryLimit?: number;
}

/**
 * RNJSCSandboxProvider - Wraps react-native-jsc-sandbox for rill Engine
 *
 * This provider creates truly isolated JavaScript contexts using Apple's JavaScriptCore.
 * Each context is completely sandboxed - code in one context cannot access globals or
 * data from another context or the host application.
 */
export class RNJSCSandboxProvider implements JSEngineProvider {
  private provider: JSCSandboxProviderLike;
  private options: RNJSCSandboxProviderOptions;

  constructor(mod: JSCSandboxModule, options?: RNJSCSandboxProviderOptions) {
    this.options = options || {};

    // Create the underlying JSC sandbox provider
    this.provider = new mod.JSCSandboxProvider({
      timeout: this.options.timeout,
      memoryLimit: this.options.memoryLimit,
    });
  }

  createRuntime(): JSEngineRuntime {
    const rt = this.provider.createRuntime();

    return {
      createContext: (): JSEngineContext => {
        const ctx = rt.createContext();

        // Wrap to match rill's JSEngineContext interface
        const wrappedContext: JSEngineContext = {
          // JSC sandbox only supports async eval via native bridge
          eval: (_code: string): unknown => {
            throw new Error(
              '[RNJSCSandboxProvider] Synchronous eval not supported. Use evalAsync instead.'
            );
          },

          evalAsync: async (code: string): Promise<unknown> => {
            return ctx.evalAsync(code);
          },

          setGlobal: (name: string, value: unknown): void => {
            ctx.setGlobal(name, value);
          },

          getGlobal: (name: string): unknown => {
            // Note: JSC sandbox getGlobal is async, but rill interface expects sync
            // Return a promise - callers should use await
            return ctx.getGlobal(name);
          },

          dispose: (): void => {
            ctx.dispose();
          },

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

      dispose: (): void => {
        rt.dispose();
      },
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
    // watchOS typically runs as watchOS app extension
    (Platform.OS as string) === 'watchos'
  );
}

/**
 * Check if react-native-jsc-sandbox is available
 */
export function isJSCSandboxAvailable(): boolean {
  if (!isApplePlatform()) {
    return false;
  }

  try {
    // eslint-disable-next-line @typescript-eslint/no-var-requires
    const mod = require('react-native-jsc-sandbox');
    return typeof mod?.isJSCSandboxAvailable === 'function'
      ? mod.isJSCSandboxAvailable()
      : !!mod?.JSCSandboxProvider;
  } catch {
    return false;
  }
}

/**
 * Resolve the JSC sandbox module
 * Returns null if not available or not on Apple platform
 */
export function resolveJSCSandbox(): JSCSandboxModule | null {
  if (!isApplePlatform()) {
    return null;
  }

  try {
    // eslint-disable-next-line @typescript-eslint/no-var-requires
    const mod = require('react-native-jsc-sandbox') as JSCSandboxModule;
    if (mod?.JSCSandboxProvider) {
      return mod;
    }
  } catch {
    // Package not available
  }

  return null;
}
