/**
 * DefaultJSEngineProvider for React Native
 *
 * Auto-selects the best sandbox provider based on platform:
 * - Apple platforms (iOS, macOS, tvOS, watchOS, visionOS): JSCSandboxProvider (preferred)
 * - Android/other: RNQuickJSProvider
 * - Fallback: NoSandboxProvider (dev-only, insecure)
 */

import { NoSandboxProvider } from './NoSandboxProvider';
import { isApplePlatform, RNJSCSandboxProvider, resolveJSCSandbox } from './RNJSCSandboxProvider';
import { RNQuickJSProvider, resolveRNQuickJS } from './RNQuickJSProvider';

export type DefaultProviderOptions = {
  timeout?: number;
  /**
   * Force a specific sandbox mode. If not specified, auto-detects the best provider.
   * - 'jsc': JavaScriptCore sandbox (Apple platforms only, zero binary overhead)
   * - 'rn': React Native QuickJS JSI binding (cross-platform, ~700KB)
   * - 'none': No sandbox (dev-only, insecure)
   */
  sandbox?: 'jsc' | 'rn' | 'none';
};

/**
 * DefaultJSEngineProvider - Auto-selects the best JS engine provider for React Native
 *
 * Selection priority (when sandbox option not specified):
 * 1. Apple platforms: JSCSandboxProvider (uses system JSC, zero overhead)
 * 2. All platforms: RNQuickJSProvider (if react-native-quickjs available)
 * 3. Fallback: NoSandboxProvider (dev-only, with warning)
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

    // Explicit 'jsc' provider selection (Apple platforms only)
    if (options?.sandbox === 'jsc') {
      const jscModule = resolveJSCSandbox();
      if (jscModule) {
        return new RNJSCSandboxProvider(jscModule, { timeout: options?.timeout });
      }
      console.warn(
        '[rill] JSCSandboxProvider requested but react-native-jsc-sandbox not available or not on Apple platform.'
      );
      // Fall through to QuickJS or NoSandbox
    }

    // Explicit 'rn' provider selection (QuickJS)
    if (options?.sandbox === 'rn') {
      const quickjsModule = resolveRNQuickJS();
      if (quickjsModule) {
        return new RNQuickJSProvider(quickjsModule, { timeout: options?.timeout });
      }
      console.warn('[rill] RNQuickJSProvider requested but react-native-quickjs not available.');
      // Fall through to NoSandbox
    }

    // Auto-detect best provider

    // On Apple platforms, prefer JSCSandboxProvider (zero binary overhead)
    if (isApplePlatform()) {
      const jscModule = resolveJSCSandbox();
      if (jscModule) {
        return new RNJSCSandboxProvider(jscModule, { timeout: options?.timeout });
      }
      // JSC not available, fall through to QuickJS
    }

    // Try RNQuickJSProvider (works on all platforms including Android)
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
