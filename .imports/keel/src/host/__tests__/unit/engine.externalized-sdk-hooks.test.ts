import { describe, expect, it } from 'bun:test';
import { Engine } from '../../engine';
import { createMockJSEngineProvider } from '../test-utils';

/**
 * Verifies IIFE/externalized bundle path: Guest reads hooks from globalThis.RillSDK.
 * Previously these were broken because SDK globals were resolved from the wrong realm.
 */
describe('Engine - externalized rill/sdk hooks', () => {
  it('should expose hooks on global RillSDK (externalized bundles)', async () => {
    const engine = new Engine({ quickjs: createMockJSEngineProvider(), debug: false });

    // This code simulates an externalized guest bundle that reads from global RillSDK directly.
    await engine.loadBundle(`
      globalThis.__HOOK_TYPES = {
        useHostEvent: typeof RillSDK.useHostEvent,
        useConfig: typeof RillSDK.useConfig,
        useSendToHost: typeof RillSDK.useSendToHost
      };
    `);

    const types = engine.context?.getGlobal('__HOOK_TYPES') as Record<string, unknown>;
    expect(types).toEqual({
      useHostEvent: 'function',
      useConfig: 'function',
      useSendToHost: 'function',
    });

    engine.destroy();
  });
});
