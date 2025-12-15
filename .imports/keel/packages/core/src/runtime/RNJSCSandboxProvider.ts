/**
 * RNJSCSandboxProvider - JavaScriptCore sandbox for React Native (Apple platforms)
 *
 * This file is the stub for non-React Native environments.
 * The actual implementation is in RNJSCSandboxProvider.native.ts
 *
 * On Apple platforms (iOS, macOS, tvOS, watchOS, visionOS), this provider uses
 * the system JavaScriptCore engine for true sandbox isolation with zero binary overhead.
 */

import type { JSEngineProvider, JSEngineRuntime } from './engine';

export interface RNJSCSandboxProviderOptions {
  timeout?: number;
  memoryLimit?: number;
}

/**
 * Stub implementation for non-React Native environments.
 * Always throws - use VMProvider or WorkerJSEngineProvider instead.
 */
export class RNJSCSandboxProvider implements JSEngineProvider {
  constructor(_options?: RNJSCSandboxProviderOptions) {
    throw new Error(
      '[RNJSCSandboxProvider] Only available on React Native (Apple platforms). ' +
        'Use VMProvider for Node.js or WorkerJSEngineProvider for browsers.'
    );
  }

  /* v8 ignore next 3 -- unreachable: constructor throws */
  createRuntime(): JSEngineRuntime {
    throw new Error('[RNJSCSandboxProvider] Not available in this environment.');
  }
}

/**
 * Check if react-native-jsc-sandbox is available
 * Always returns false in non-RN environments
 */
export function isJSCSandboxAvailable(): boolean {
  return false;
}

/**
 * Resolve the JSC sandbox module
 * Always returns null in non-RN environments
 */
export function resolveJSCSandbox(): null {
  return null;
}

/**
 * Check if running on Apple/Darwin platform
 * Always returns false in non-RN environments
 */
export function isApplePlatform(): boolean {
  return false;
}
