/**
 * DefaultProvider for native environments (with JSI bindings)
 *
 * Auto-selects the best sandbox provider based on platform:
 * - Hermes sandbox: HermesProvider (when RILL_SANDBOX_ENGINE=hermes)
 * - Apple platforms: JSCProvider (uses system JSC, zero binary overhead)
 * - Other platforms: QuickJSProvider (cross-platform)
 * - No fallback - throws error if no provider available
 */

import { isHermesAvailable } from '../native/HermesModule';
import { isJSCAvailable } from '../native/JSCModule';
import { isQuickJSAvailable } from '../native/QuickJSModule';
import { HermesProvider } from '../providers/HermesProvider';
import { JSCProvider } from '../providers/JSCProvider';
import { QuickJSProvider } from '../providers/QuickJSProvider';
import { SandboxType } from '../types/provider';

export type DefaultProviderOptions = {
  timeout?: number;
  /**
   * Force a specific sandbox type. If not specified, auto-detects the best provider.
   * Available types for React Native: SandboxType.Hermes, SandboxType.JSC, SandboxType.QuickJS
   */
  sandbox?: SandboxType.Hermes | SandboxType.JSC | SandboxType.QuickJS;
};

/**
 * DefaultProvider - Auto-selects the best JS engine provider for native platforms
 *
 * Selection priority (when sandbox option not specified):
 * 1. HermesProvider (when RILL_SANDBOX_ENGINE=hermes at build time)
 * 2. Apple platforms: JSCProvider (uses system JSC, zero overhead)
 * 3. All platforms: QuickJSProvider (if available)
 * 4. Error: No fallback (throws if no provider available)
 */
export class DefaultProvider {
  static create(options?: DefaultProviderOptions) {
    // Cache availability checks to avoid repeated native calls and add diagnostics
    const hermesAvailable = isHermesAvailable();
    const jscAvailable = isJSCAvailable();
    const quickjsAvailable = isQuickJSAvailable();
    // One-time availability log to help debug sandbox loading issues
    if (typeof console?.log === 'function') {
      console.log('[rill][DefaultProvider] availability', {
        hermesAvailable,
        jscAvailable,
        quickjsAvailable,
        hermesGlobal: typeof globalThis.__HermesSandboxJSI,
        jscGlobal: typeof globalThis.__JSCSandboxJSI,
        quickjsGlobal: typeof globalThis.__QuickJSSandboxJSI,
      });
    }

    // Build provider options only with defined values
    const providerOptions =
      options?.timeout !== undefined ? { timeout: options.timeout } : undefined;

    // Explicit Hermes provider selection
    if (options?.sandbox === SandboxType.Hermes) {
      if (hermesAvailable) {
        return new HermesProvider(providerOptions);
      }
      throw new Error(
        '[DefaultProvider] HermesProvider requested but Hermes sandbox not available (RILL_SANDBOX_ENGINE!=hermes or native module not linked).'
      );
    }

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

    // Hermes sandbox (when built with RILL_SANDBOX_ENGINE=hermes)
    if (hermesAvailable) {
      return new HermesProvider(providerOptions);
    }

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
      hermesAvailable,
      jscAvailable,
      quickjsAvailable,
      hermesGlobal: typeof globalThis.__HermesSandboxJSI,
      jscGlobal: typeof globalThis.__JSCSandboxJSI,
      quickjsGlobal: typeof globalThis.__QuickJSSandboxJSI,
    };
    throw new Error(
      `[DefaultProvider] No native JSI sandbox module found. ` +
        `Make sure the 'RillSandboxNative' native bindings are linked and installed before creating Engine. ` +
        `Bridgeless (New Architecture): call RillSandboxNativeInstall(&runtime) from RCTHostRuntimeDelegate::didInitializeRuntime. ` +
        `Legacy bridge: ensure legacy auto-install is enabled (default). ` +
        `diag=${JSON.stringify(diag)}`
    );
  }
}
