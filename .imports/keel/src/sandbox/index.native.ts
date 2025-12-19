/**
 * @rill/sandbox - JavaScript Sandbox Providers (Native)
 *
 * Provides sandbox implementations for native environments:
 * - QuickJSProvider: QuickJS via native JSI bindings (cross-platform)
 * - JSCProvider: JavaScriptCore via native JSI bindings (Apple platforms)
 * - DefaultProvider: Auto-selects the best provider for the current platform
 */

// Type and enum exports
export { SandboxType } from './types/provider';
export type {
  JSEngineContext,
  JSEngineProvider,
  JSEngineRuntime,
  JSEngineRuntimeOptions,
} from './types/provider';

// Provider exports (uses .native.ts variants automatically)
export { VMProvider } from './providers/VMProvider';
export { WorkerProvider } from './providers/WorkerProvider';
export {
  QuickJSProvider,
  isQuickJSAvailable,
  resolveQuickJS,
} from './providers/QuickJSProvider';
export type { QuickJSProviderOptions } from './providers/QuickJSProvider';
export {
  JSCProvider,
  isJSCAvailable,
  isApplePlatform,
  resolveJSCSandbox,
} from './providers/JSCProvider';
export type { JSCProviderOptions } from './providers/JSCProvider';

// Default provider (uses .native.ts variant automatically)
export { DefaultProvider } from './default/DefaultProvider';
export type { DefaultProviderOptions } from './default/DefaultProvider';
