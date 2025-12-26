/**
 * DefaultProvider for native environments (with JSI bindings)
 *
 * Auto-selects the best sandbox provider based on platform:
 * - Apple platforms: JSCProvider (uses system JSC, zero binary overhead)
 * - Other platforms: QuickJSProvider (cross-platform)
 * - No fallback - throws error if no provider available
 */

import { isJSCAvailable } from '../native/JSCModule';
import { isQuickJSAvailable } from '../native/QuickJSModule';
import { JSCProvider } from '../providers/JSCProvider';
import { QuickJSProvider } from '../providers/QuickJSProvider';
import { SandboxType } from '../types/provider';

export type DefaultProviderOptions = {
  timeout?: number;
  /**
   * Force a specific sandbox type. If not specified, auto-detects the best provider.
   * Available types for React Native: SandboxType.JSC, SandboxType.QuickJS
   */
  sandbox?: SandboxType.JSC | SandboxType.QuickJS;
};

/**
 * DefaultProvider - Auto-selects the best JS engine provider for native platforms
 *
 * Selection priority (when sandbox option not specified):
 * 1. Apple platforms: JSCProvider (uses system JSC, zero overhead)
 * 2. All platforms: QuickJSProvider (if available)
 * 3. Error: No fallback (throws if no provider available)
 */
export class DefaultProvider {
  static create(options?: DefaultProviderOptions) {
    // Cache availability checks to avoid repeated native calls and add diagnostics
    const jscAvailable = isJSCAvailable();
    const quickjsAvailable = isQuickJSAvailable();
    // One-time availability log to帮助定位沙箱加载问题
    if (typeof console?.log === 'function') {
      console.log('[rill][DefaultProvider] availability', {
        jscAvailable,
        quickjsAvailable,
        jscGlobal: typeof globalThis.__JSCSandboxJSI,
        quickjsGlobal: typeof globalThis.__QuickJSSandboxJSI,
      });
    }

    // Build provider options only with defined values
    const providerOptions =
      options?.timeout !== undefined ? { timeout: options.timeout } : undefined;

    // Explicit JSC provider selection (Apple platforms only)
    if (options?.sandbox === SandboxType.JSC) {
      if (jscAvailable) {
        return new JSCProvider(providerOptions);
      }
      throw new Error(
        '[DefaultProvider] JSCProvider requested but JSC sandbox not available (not on Apple platform or native module not linked).'
      );
    }

    // Explicit QuickJS provider selection
    if (options?.sandbox === SandboxType.QuickJS) {
      if (quickjsAvailable) {
        return new QuickJSProvider(providerOptions);
      }
      throw new Error(
        '[DefaultProvider] QuickJSProvider requested but QuickJS native module not available.'
      );
    }

    // Auto-detect best provider

    // On Apple platforms, prefer JSCProvider (zero binary overhead)
    if (jscAvailable) {
      return new JSCProvider(providerOptions);
    }

    // Try QuickJSProvider (works on all platforms including Android)
    if (quickjsAvailable) {
      return new QuickJSProvider(providerOptions);
    }

    // No suitable provider available
    const diag = {
      jscAvailable,
      quickjsAvailable,
      jscGlobal: typeof globalThis.__JSCSandboxJSI,
      quickjsGlobal: typeof globalThis.__QuickJSSandboxJSI,
    };
    throw new Error(
      `[DefaultProvider] No suitable JS sandbox provider found. Ensure @rill/sandbox-native is properly linked. diag=${JSON.stringify(
        diag
      )}`
    );
  }
}
