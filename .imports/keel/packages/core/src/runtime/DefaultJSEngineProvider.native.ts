/**
 * DefaultJSEngineProvider for React Native
 * Removes Worker-related code that uses import.meta
 */

import { NoSandboxProvider } from './NoSandboxProvider';
import { RNQuickJSProvider, resolveRNQuickJS } from './RNQuickJSProvider';

export type DefaultProviderOptions = {
  timeout?: number;
  /**
   * Force a specific sandbox mode. If not specified, auto-detects the best provider.
   * - 'rn': React Native QuickJS JSI binding
   * - 'none': No sandbox (dev-only, insecure)
   */
  sandbox?: 'rn' | 'none';
};

/**
 * DefaultJSEngineProvider - Auto-selects the best JS engine provider for React Native
 *
 * Selection priority (when sandbox option not specified):
 * 1. React Native: RNQuickJSProvider
 * 2. Fallback: NoSandboxProvider (dev-only, with warning)
 */
export class DefaultJSEngineProvider {
  static create(options?: DefaultProviderOptions) {
    // 'none' is an explicit opt-out of sandboxing (dev-only)
    if (options?.sandbox === 'none') {
      console.warn(
        '[rill] NoSandboxProvider selected. This is insecure and should only be used in development.'
      );
      return new NoSandboxProvider({ timeout: options?.timeout });
    }

    // Explicit provider selection
    if (options?.sandbox === 'rn') {
      const quickjsModule = resolveRNQuickJS();
      if (quickjsModule) {
        return new RNQuickJSProvider(quickjsModule, { timeout: options?.timeout });
      }
      console.warn('[rill] RNQuickJSProvider requested but react-native-quickjs not available.');
    }

    // Auto-detect best provider for React Native
    // Try RNQuickJSProvider first
    const quickjsModule = resolveRNQuickJS();
    if (quickjsModule) {
      return new RNQuickJSProvider(quickjsModule, { timeout: options?.timeout });
    }

    // Fallback (dev only): This should rarely happen
    console.warn(
      '[rill] No suitable JS engine provider found. Falling back to insecure eval provider (dev-only).'
    );
    return new NoSandboxProvider({ timeout: options?.timeout });
  }
}
