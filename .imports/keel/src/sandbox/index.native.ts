/**
 * @rill/sandbox - JavaScript Sandbox Providers (Native)
 *
 * Provides sandbox implementations for native environments:
 * - QuickJSProvider: QuickJS via native JSI bindings (cross-platform)
 * - JSCProvider: JavaScriptCore via native JSI bindings (Apple platforms)
 * - DefaultProvider: Auto-selects the best provider for the current platform
 */

export type { DefaultProviderOptions } from './default/DefaultProvider';
// Default provider (uses .native.ts variant automatically)
export { DefaultProvider } from './default/DefaultProvider';
export type { JSCProviderOptions } from './providers/JSCProvider';
export {
  isApplePlatform,
  isJSCAvailable,
  JSCProvider,
  resolveJSCSandbox,
} from './providers/JSCProvider';
export type { QuickJSProviderOptions } from './providers/QuickJSProvider';
// Provider exports (native only)
export {
  isQuickJSAvailable,
  QuickJSProvider,
  resolveQuickJS,
} from './providers/QuickJSProvider';
export type {
  JSEngineContext,
  JSEngineProvider,
  JSEngineRuntime,
  JSEngineRuntimeOptions,
} from './types/provider';
// Type and enum exports
export { SandboxType } from './types/provider';
