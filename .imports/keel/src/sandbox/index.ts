/**
 * @rill/sandbox - JavaScript Sandbox Providers
 *
 * Provides multiple sandbox implementations for different environments:
 * - VMProvider: Node.js vm module (Node/Bun)
 * - QuickJSNativeWASMProvider: QuickJS compiled to WebAssembly (Browser)
 * - QuickJSProvider: QuickJS via native JSI bindings (React Native)
 * - JSCProvider: JavaScriptCore via native JSI bindings (Apple platforms)
 * - DefaultProvider: Auto-selects the best provider for the current environment
 */

export type { DefaultProviderOptions } from './default/DefaultProvider';
// Default provider
export { DefaultProvider } from './default/DefaultProvider';
export type { JSCProviderOptions } from './providers/JSCProvider';
export {
  isApplePlatform,
  isJSCAvailable,
  JSCProvider,
  resolveJSCSandbox,
} from './providers/JSCProvider';
export type { QuickJSNativeWASMProviderOptions } from './providers/QuickJSNativeWASMProvider';
export { QuickJSNativeWASMProvider } from './providers/QuickJSNativeWASMProvider';
export type { QuickJSProviderOptions } from './providers/QuickJSProvider';
export {
  isQuickJSAvailable,
  QuickJSProvider,
  resolveQuickJS,
} from './providers/QuickJSProvider';
// Provider exports
export { VMProvider } from './providers/VMProvider';
export type {
  JSEngineContext,
  JSEngineProvider,
  JSEngineRuntime,
  JSEngineRuntimeOptions,
} from './types/provider';
// Type and enum exports
export { SandboxType } from './types/provider';
