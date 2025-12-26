/**
 * DefaultProvider - Auto-selects the best JS engine provider based on environment
 *
 * Strategy:
 * - Node/Bun: VMProvider (zero overhead, full capabilities)
 * - Web: QuickJSNativeWASMProvider (strong isolation + full capabilities)
 *
 * All providers support high-performance direct object passing (no JSON serialization).
 */

import { QuickJSNativeWASMProvider } from '../providers/QuickJSNativeWASMProvider';
import { VMProvider } from '../providers/VMProvider';
import { SandboxType } from '../types/provider';

function isNodeEnv(): boolean {
  return (
    typeof process !== 'undefined' && process.versions != null && process.versions.node != null
  );
}

// Lazily resolve vm module
import type * as vm from 'node:vm';

type NodeVM = typeof vm;

function getVm(): NodeVM | null {
  if (typeof require === 'undefined') {
    return null;
  }
  try {
    // eslint-disable-next-line @typescript-eslint/no-var-requires
    return require('node:vm');
  } catch {
    return null;
  }
}

function isWASMCapable(): boolean {
  return typeof WebAssembly !== 'undefined';
}

export type DefaultProviderOptions = {
  timeout?: number;
  /**
   * Force a specific sandbox type. If not specified, auto-detects the best provider.
   * - VM: Node.js/Bun only (native vm module)
   * - JSC: Apple platforms only (requires native JSI bindings)
   * - QuickJS: Cross-platform native (requires native JSI bindings)
   * - WasmQuickJS: Web/cross-platform (WASM, no native bindings required)
   */
  sandbox?: SandboxType;
  /**
   * Path to QuickJS WASM files (for Web environments)
   * @default '/quickjs_sandbox.wasm'
   */
  wasmPath?: string;
};

/**
 * DefaultProvider - Auto-selects the best JS engine provider based on environment
 *
 * Selection priority (when sandbox option not specified):
 * 1. Node/Bun: VMProvider (native, zero overhead)
 * 2. Web: QuickJSNativeWASMProvider (WASM, strong isolation)
 * 3. Error: No provider available
 */
export class DefaultProvider {
  static create(options?: DefaultProviderOptions) {
    const envInfo = {
      isNode: isNodeEnv(),
      hasVm: !!getVm(),
      isWASMCapable: isWASMCapable(),
    };

    // Build provider options
    const providerOptions =
      options?.timeout !== undefined ? { timeout: options.timeout } : undefined;

    // Explicit provider selection
    if (options?.sandbox === SandboxType.VM) {
      if (isNodeEnv() && getVm()) {
        return new VMProvider(providerOptions);
      }
      throw new Error(
        '[DefaultProvider] VMProvider requested but not available in this environment.'
      );
    }

    if (options?.sandbox === SandboxType.JSC) {
      // JSC requires native JSI bindings, only available in React Native on Apple platforms
      throw new Error(
        '[DefaultProvider] JSC sandbox requires native JSI bindings (Apple platforms only). ' +
          'Use in React Native with JavaScriptCore runtime.'
      );
    }

    if (options?.sandbox === SandboxType.QuickJS) {
      // Native QuickJS requires native JSI bindings
      throw new Error(
        '[DefaultProvider] Native QuickJS sandbox requires native JSI bindings. ' +
          'Use in React Native with react-native-quickjs native module.'
      );
    }

    if (options?.sandbox === SandboxType.WasmQuickJS) {
      if (isWASMCapable()) {
        return new QuickJSNativeWASMProvider({
          timeout: options?.timeout,
          wasmPath: options?.wasmPath,
        });
      }
      throw new Error('[DefaultProvider] WasmQuickJS requested but WebAssembly is not available.');
    }

    // Auto-detect best provider

    // 1. Node/Bun environment - use VMProvider (native, fast, supports timeout)
    if (isNodeEnv() && getVm()) {
      return new VMProvider(providerOptions);
    }

    // 2. Web environment with WASM support - use QuickJS Native WASM
    if (isWASMCapable()) {
      return new QuickJSNativeWASMProvider({
        timeout: options?.timeout,
        wasmPath: options?.wasmPath,
      });
    }

    // No suitable provider available
    throw new Error(
      `[DefaultProvider] No suitable JS sandbox provider found. ` +
        `Environment: ${JSON.stringify(envInfo)}. ` +
        `Ensure WebAssembly is supported (for browsers) or run in Node.js/Bun.`
    );
  }
}
