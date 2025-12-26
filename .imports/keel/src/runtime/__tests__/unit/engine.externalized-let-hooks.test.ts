import { describe, expect, it } from 'bun:test';
import { Engine } from '../../engine';
import { createMockJSEngineProvider } from '../test-utils';

/**
 * Verifies IIFE/externalized bundle path: Guest reads hooks from globalThis.RillLet.
 * Previously these were null because createRillSDKModule() set them to null.
 */
describe('Engine - externalized @rill/let hooks', () => {
  it('should expose non-null hooks on global RillLet (externalized bundles)', async () => {
    const engine = new Engine({ quickjs: createMockJSEngineProvider(), debug: false });

    // This code simulates an externalized guest bundle that reads from global RillLet directly.
    await engine.loadBundle(`
      globalThis.__HOOK_TYPES = {
        useHostEvent: typeof RillLet.useHostEvent,
        useConfig: typeof RillLet.useConfig,
        useSendToHost: typeof RillLet.useSendToHost
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
