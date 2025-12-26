import { describe, expect, it } from 'bun:test';
import { Engine } from '../../engine';
import { createMockJSEngineProvider } from '../test-utils';

describe('Engine diagnostics - lifecycle', () => {
  it('tracks loaded/destroyed in health', async () => {
    const engine = new Engine({ provider: createMockJSEngineProvider(), debug: false });

    // before load
    expect(engine.getDiagnostics().health.loaded).toBe(false);
    expect(engine.getDiagnostics().health.destroyed).toBe(false);

    await engine.loadBundle('globalThis.__ok = 1;');
    expect(engine.getDiagnostics().health.loaded).toBe(true);

    engine.destroy();
    expect(engine.getDiagnostics().health.loaded).toBe(false);
    expect(engine.getDiagnostics().health.destroyed).toBe(true);
  });

  it('tracks recordError when bundle execution fails', async () => {
    const engine = new Engine({ provider: createMockJSEngineProvider(), debug: false });

    await expect(engine.loadBundle('throw new Error("boom");')).rejects.toThrow('boom');

    const d = engine.getDiagnostics();
    expect(d.health.errorCount).toBeGreaterThan(0);
    expect(typeof d.health.lastErrorAt).toBe('number');

    engine.destroy();
  });
});
