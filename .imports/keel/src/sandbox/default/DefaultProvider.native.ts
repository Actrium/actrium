/**
 * DefaultProvider for native environments (with JSI bindings)
 *
 * Auto-selects the best sandbox provider based on platform:
 * - Apple platforms: JSCProvider (uses system JSC, zero binary overhead)
 * - Other platforms: QuickJSProvider (cross-platform)
 * - No fallback - throws error if no provider available
 */

// @ts-expect-error - Native module loaded conditionally
import { isJSCAvailable } from '../../sandbox-native/src/jsc';
// @ts-expect-error - Native module loaded conditionally
import { isQuickJSAvailable } from '../../sandbox-native/src/quickjs';
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
    // Build provider options only with defined values
    const providerOptions =
      options?.timeout !== undefined ? { timeout: options.timeout } : undefined;

    // Explicit JSC provider selection (Apple platforms only)
    if (options?.sandbox === SandboxType.JSC) {
      if (isJSCAvailable()) {
        return new JSCProvider(providerOptions);
      }
      throw new Error(
        '[DefaultProvider] JSCProvider requested but JSC sandbox not available (not on Apple platform or native module not linked).'
      );
    }

    // Explicit QuickJS provider selection
    if (options?.sandbox === SandboxType.QuickJS) {
      if (isQuickJSAvailable()) {
        return new QuickJSProvider(providerOptions);
      }
      throw new Error(
        '[DefaultProvider] QuickJSProvider requested but QuickJS native module not available.'
      );
    }

    // Auto-detect best provider

    // On Apple platforms, prefer JSCProvider (zero binary overhead)
    if (isJSCAvailable()) {
      return new JSCProvider(providerOptions);
    }

    // Try QuickJSProvider (works on all platforms including Android)
    if (isQuickJSAvailable()) {
      return new QuickJSProvider(providerOptions);
    }

    // No suitable provider available
    throw new Error(
      '[DefaultProvider] No suitable JS sandbox provider found. ' +
        'Ensure @rill/sandbox-native is properly linked.'
    );
  }
}
