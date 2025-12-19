/**
 * @rill/sandbox - JavaScript Sandbox Providers
 *
 * Provides multiple sandbox implementations for different environments:
 * - VMProvider: Node.js vm module (Node/Bun)
 * - WorkerProvider: Web Worker sandbox (Browser)
 * - QuickJSProvider: QuickJS via native JSI bindings
 * - JSCProvider: JavaScriptCore via native JSI bindings (Apple platforms)
 * - DefaultProvider: Auto-selects the best provider for the current environment
 */

// Type and enum exports
export { SandboxType } from './types/provider';
export type {
  JSEngineContext,
  JSEngineProvider,
  JSEngineRuntime,
  JSEngineRuntimeOptions,
} from './types/provider';

// Provider exports
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

// Default provider
export { DefaultProvider } from './default/DefaultProvider';
export type { DefaultProviderOptions } from './default/DefaultProvider';
