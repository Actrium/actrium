/**
 * WorkerProvider stub for non-browser environments
 * WorkerProvider requires Web Worker API
 */

import type { JSEngineProvider } from '../types/provider';

export class WorkerProvider implements JSEngineProvider {
  constructor(_createWorker?: () => Worker, _options?: { timeout?: number }) {
    throw new Error(
      '[WorkerProvider] Requires Web Worker API. Use QuickJSProvider or JSCProvider.'
    );
  }

  createRuntime(): never {
    throw new Error('[WorkerProvider] Requires Web Worker API.');
  }
}
