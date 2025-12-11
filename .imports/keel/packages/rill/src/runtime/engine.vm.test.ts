import { describe, it, expect } from 'bun:test';
import vm from 'node:vm';
import { Engine } from './engine';
import { VMProvider } from './VMProvider';

// These tests are specific to the VMProvider and should only run in a Node.js/Bun environment.
describe.skipIf(!vm)('VMProvider', () => {
  it('should interrupt a dead-loop with a timeout', async () => {
    const provider = new VMProvider({ timeout: 100 });
    const engine = new Engine({
      quickjs: provider,
      debug: false,
      timeout: 100,
    });

    let threw = false;
    let error: Error | null = null;
    try {
      await engine.loadBundle('for(;;){}');
    } catch (e) {
      threw = true;
      if (e instanceof Error) {
        error = e;
      }
    }

    expect(threw).toBe(true);
    expect(error?.message).toContain('Script execution timed out');
    engine.destroy();
  });
});
