/**
 * Rill Sandbox Providers
 *
 * All providers support high-performance direct object passing:
 * - Can pass functions, circular references, complex objects
 * - No JSON serialization overhead
 * - True isolation with strong capabilities
 */

export type { HermesProviderOptions } from './HermesProvider';
// Hermes Native (React Native - when RILL_SANDBOX_ENGINE=hermes)
export { HermesProvider, isHermesAvailable } from './HermesProvider';
export type { JSCProviderOptions } from './JSCProvider';
// JSC Native (React Native - Apple platforms only)
export { isApplePlatform, isJSCAvailable, JSCProvider, resolveJSCSandbox } from './JSCProvider';
export type { QuickJSNativeWASMProviderOptions } from './QuickJSNativeWASMProvider';
// QuickJS Native WASM (Web - compiled from native/quickjs)
export { QuickJSNativeWASMProvider } from './QuickJSNativeWASMProvider';
export type { QuickJSProviderOptions } from './QuickJSProvider';
// QuickJS Native (React Native - cross-platform)
export { isQuickJSAvailable, QuickJSProvider, resolveQuickJS } from './QuickJSProvider';
// Node.js VM (Node/Bun only)
export { VMProvider } from './VMProvider';
