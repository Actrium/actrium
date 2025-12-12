/**
 * VMProvider stub for React Native
 * VMProvider is not available in React Native as it depends on Node.js vm module
 */

import type { JSEngineProvider } from './engine';

export class VMProvider implements JSEngineProvider {
  constructor(_options?: { timeout?: number }) {
    throw new Error(
      '[VMProvider] Not available in React Native environment. Use RNQuickJSProvider instead.'
    );
  }

  createRuntime(): never {
    throw new Error('[VMProvider] Not available in React Native environment.');
  }
}
