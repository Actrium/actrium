import { describe, it, expect } from 'bun:test';
import { Engine, TimeoutError } from './engine';

describe('DefaultJSEngineProvider (auto) - Node/Web', () => {
  it('simple eval without passing quickjs', async () => {
    const engine = new Engine({ debug: false });
    await engine.loadBundle('var a = 1 + 2;');
    expect(engine.isLoaded).toBe(true);
    engine.destroy();
  });

  it('JSX shim available by default', async () => {
    const engine = new Engine({ debug: false });
    const code = `
      const a = ReactJSXRuntime.jsx('View', { id: 'ok', children: 'hi' });
    `;
    await engine.loadBundle(code);
    expect(engine.isLoaded).toBe(true);
    engine.destroy();
  });

  it('dead-loop should be interrupted by timeout', async () => {
    // This test relies on the default provider (VMProvider in this env)
    // to handle the timeout correctly via the engine's timeout option.
    const engine = new Engine({
      debug: false,
      timeout: 100, // Use a short timeout
    });
    let threw = false;
    let error: any;
    try {
      await engine.loadBundle('for(;;){}');
    } catch (e) {
      threw = true;
      error = e;
    }
    expect(threw).toBe(true);
    // The VMProvider should throw a distinctive error
    expect(error.message).toContain('Script execution timed out');
    engine.destroy();
  });
});