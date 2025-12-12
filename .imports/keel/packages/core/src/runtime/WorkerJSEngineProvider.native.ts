/**
 * WorkerJSEngineProvider stub for React Native
 * Web Workers are not available in React Native
 */

import type { JSEngineProvider } from './engine';

export class WorkerJSEngineProvider implements JSEngineProvider {
  constructor(_createWorker: () => Worker, _options?: { timeout?: number }) {
    throw new Error(
      '[WorkerJSEngineProvider] Not available in React Native environment. Use RNQuickJSProvider instead.'
    );
  }

  createRuntime(): never {
    throw new Error('[WorkerJSEngineProvider] Not available in React Native environment.');
  }
}
