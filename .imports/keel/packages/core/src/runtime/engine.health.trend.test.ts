import { describe, expect, it } from 'bun:test';
import { Engine } from './engine';
import { createMockJSEngineProvider } from './test-utils';

describe('Engine health trend', () => {
  it('increments errorCount on failure, then can recover and load successfully', async () => {
    const provider = createMockJSEngineProvider();
    const engine = new Engine({ quickjs: provider, debug: false });

    await expect(engine.loadBundle('throw new Error("e1")')).rejects.toThrow();
    const h1 = engine.getHealth();
    expect(h1.errorCount).toBeGreaterThan(0);
    expect(h1.loaded).toBe(false);

    await engine.loadBundle('console.log("ok")');
    const h2 = engine.getHealth();
    expect(h2.loaded).toBe(true);
    expect(h2.errorCount).toBeGreaterThan(0);
    expect(typeof h2.lastErrorAt === 'number' || h2.lastErrorAt === null).toBeTruthy();
  });
});
