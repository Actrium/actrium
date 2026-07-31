import { describe, expect, it } from 'bun:test';
import { Engine } from '../../engine';

/**
 * Verifies IIFE/externalized bundle path: Guest reads hooks from globalThis.KeelGuest.
 * Previously these were broken because SDK globals were resolved from the wrong realm.
 */
describe('Engine - externalized keel/guest hooks', () => {
  it('should expose hooks on global KeelGuest (externalized bundles)', async () => {
    const engine = new Engine({ sandbox: 'node-vm', debug: false });

    // This code simulates an externalized guest bundle that reads from global KeelGuest directly.
    await engine.loadBundle(`
      globalThis.__HOOK_TYPES = {
        useHostEvent: typeof KeelGuest.useHostEvent,
        useConfig: typeof KeelGuest.useConfig,
        useSendToHost: typeof KeelGuest.useSendToHost
      };
    `);

    const types = engine.context?.extract('__HOOK_TYPES') as Record<string, unknown>;
    expect(types).toEqual({
      useHostEvent: 'function',
      useConfig: 'function',
      useSendToHost: 'function',
    });

    engine.destroy();
  });
});
